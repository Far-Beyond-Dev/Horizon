/// UDP socket event system implementation
/// Provides complete UDP socket compatibility for binary event transmission
use crate::events::{Event, EventHandler, TypedEventHandler, EventError, GorcEvent};
use crate::types::PlayerId;
use crate::gorc::instance::GorcObjectId;
use super::core::EventSystem;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::{RwLock, Mutex};
use tokio::net::UdpSocket;
use std::net::SocketAddr;
use tokio::time::{Duration, Instant};
use serde::{Serialize, Deserialize};
use tracing::{info, warn, error, debug};
use std::io;

/// UDP event header for protocol identification and routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdpEventHeader {
    /// Protocol version for compatibility
    pub version: u8,
    /// Event type identifier
    pub event_type: String,
    /// Source player or object identifier
    pub source_id: String,
    /// Target player or object identifier (empty for broadcast)
    pub target_id: String,
    /// Sequence number for ordering and duplicate detection
    pub sequence: u32,
    /// Timestamp for latency measurement
    pub timestamp: u64,
    /// Compression type applied to payload
    pub compression: UdpCompressionType,
    /// Total packet size including header
    pub total_size: u32,
}

/// UDP event packet structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdpEventPacket {
    /// Packet header
    pub header: UdpEventHeader,
    /// Binary payload data
    pub payload: Vec<u8>,
    /// Checksum for integrity verification
    pub checksum: u32,
}

/// Compression types for UDP packets
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum UdpCompressionType {
    None,
    Deflate,
    Lz4,
}

/// UDP connection state for each client
#[derive(Debug, Clone)]
pub struct UdpConnection {
    /// Remote socket address
    pub addr: SocketAddr,
    /// Player ID associated with this connection
    pub player_id: PlayerId,
    /// Connection ID for tracking
    pub connection_id: String,
    /// Last seen timestamp
    pub last_seen: Instant,
    /// Connection establishment time
    pub connected_at: Instant,
    /// Outbound sequence number
    pub out_sequence: u32,
    /// Inbound sequence number for duplicate detection
    pub in_sequence: u32,
    /// Bandwidth tracking
    pub bytes_sent: u64,
    /// Bytes received tracking
    pub bytes_received: u64,
    /// Connection state
    pub state: UdpConnectionState,
}

/// UDP connection states
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UdpConnectionState {
    Connecting,
    Connected,
    Disconnecting,
    Disconnected,
}

impl UdpConnection {
    pub fn new(addr: SocketAddr, player_id: PlayerId, connection_id: String) -> Self {
        let now = Instant::now();
        Self {
            addr,
            player_id,
            connection_id,
            last_seen: now,
            connected_at: now,
            out_sequence: 0,
            in_sequence: 0,
            bytes_sent: 0,
            bytes_received: 0,
            state: UdpConnectionState::Connecting,
        }
    }

    pub fn next_sequence(&mut self) -> u32 {
        self.out_sequence = self.out_sequence.wrapping_add(1);
        self.out_sequence
    }

    pub fn update_last_seen(&mut self) {
        self.last_seen = Instant::now();
    }

    pub fn is_sequence_valid(&self, sequence: u32) -> bool {
        sequence == self.in_sequence.wrapping_add(1) || sequence > self.in_sequence
    }

    pub fn update_in_sequence(&mut self, sequence: u32) {
        if sequence > self.in_sequence {
            self.in_sequence = sequence;
        }
    }
}

/// UDP event system manager
pub struct UdpEventSystem {
    /// UDP socket for communication
    socket: Arc<UdpSocket>,
    /// Connection mapping by address
    connections: Arc<RwLock<HashMap<SocketAddr, UdpConnection>>>,
    /// Player ID to address mapping
    player_addresses: Arc<RwLock<HashMap<PlayerId, SocketAddr>>>,
    /// Event handlers for UDP events
    udp_handlers: Arc<RwLock<HashMap<String, Vec<Arc<dyn UdpEventHandler>>>>>,
    /// Binary event serializers
    serializers: Arc<RwLock<HashMap<String, Arc<dyn BinaryEventSerializer>>>>,
    /// Binary event deserializers
    deserializers: Arc<RwLock<HashMap<String, Arc<dyn BinaryEventDeserializer>>>>,
    /// Running state
    running: Arc<Mutex<bool>>,
    /// Compression settings
    compression_enabled: bool,
    compression_threshold: usize,
    /// Socket buffer sizes
    send_buffer_size: usize,
    recv_buffer_size: usize,
}

/// Trait for UDP-specific event handlers
#[async_trait::async_trait]
pub trait UdpEventHandler: Send + Sync {
    /// Handle a UDP event with connection context
    async fn handle_udp_event(
        &self,
        packet: &UdpEventPacket,
        connection: &mut UdpConnection,
        data: &[u8],
    ) -> Result<Option<Vec<u8>>, EventError>;

    /// Get handler name for debugging
    fn handler_name(&self) -> &str;
}

/// Trait for binary event serialization
pub trait BinaryEventSerializer: Send + Sync {
    /// Serialize an event to binary format
    fn serialize(&self, event: &dyn Event) -> Result<Vec<u8>, EventError>;
    
    /// Get the event type this serializer handles
    fn event_type(&self) -> &str;
}

/// Trait for binary event deserialization
pub trait BinaryEventDeserializer: Send + Sync {
    /// Deserialize binary data to an event
    fn deserialize(&self, data: &[u8]) -> Result<Box<dyn Event>, EventError>;
    
    /// Get the event type this deserializer handles
    fn event_type(&self) -> &str;
}

/// Default JSON-based binary serializer
pub struct JsonBinarySerializer {
    event_type: String,
}

impl JsonBinarySerializer {
    pub fn new(event_type: String) -> Self {
        Self { event_type }
    }
}

impl BinaryEventSerializer for JsonBinarySerializer {
    fn serialize(&self, event: &dyn Event) -> Result<Vec<u8>, EventError> {
        event.serialize()
    }

    fn event_type(&self) -> &str {
        &self.event_type
    }
}

/// Default JSON-based binary deserializer
pub struct JsonBinaryDeserializer<T> {
    event_type: String,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> JsonBinaryDeserializer<T>
where
    T: Event + for<'de> Deserialize<'de> + 'static,
{
    pub fn new(event_type: String) -> Self {
        Self {
            event_type,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T> BinaryEventDeserializer for JsonBinaryDeserializer<T>
where
    T: Event + for<'de> Deserialize<'de> + 'static,
{
    fn deserialize(&self, data: &[u8]) -> Result<Box<dyn Event>, EventError> {
        let json_str = String::from_utf8(data.to_vec())
            .map_err(|e| EventError::HandlerExecution(format!("UTF-8 decode error: {}", e)))?;
        
        let event: T = serde_json::from_str(&json_str)
            .map_err(|e| EventError::HandlerExecution(format!("JSON deserialize error: {}", e)))?;
        
        Ok(Box::new(event))
    }

    fn event_type(&self) -> &str {
        &self.event_type
    }
}

/// UDP-specific event handler implementation
pub struct TypedUdpEventHandler<F> {
    handler_name: String,
    handler_fn: F,
}

impl<F> TypedUdpEventHandler<F>
where
    F: Fn(&UdpEventPacket, &mut UdpConnection, &[u8]) -> Result<Option<Vec<u8>>, EventError>
        + Send
        + Sync,
{
    pub fn new(handler_name: String, handler_fn: F) -> Self {
        Self {
            handler_name,
            handler_fn,
        }
    }
}

#[async_trait::async_trait]
impl<F> UdpEventHandler for TypedUdpEventHandler<F>
where
    F: Fn(&UdpEventPacket, &mut UdpConnection, &[u8]) -> Result<Option<Vec<u8>>, EventError>
        + Send
        + Sync,
{
    async fn handle_udp_event(
        &self,
        packet: &UdpEventPacket,
        connection: &mut UdpConnection,
        data: &[u8],
    ) -> Result<Option<Vec<u8>>, EventError> {
        (self.handler_fn)(packet, connection, data)
    }

    fn handler_name(&self) -> &str {
        &self.handler_name
    }
}

impl UdpEventSystem {
    /// Create a new UDP event system
    pub async fn new(
        bind_addr: SocketAddr,
        compression_enabled: bool,
        compression_threshold: usize,
    ) -> Result<Self, io::Error> {
        let socket = UdpSocket::bind(bind_addr).await?;
        
        // Set socket buffer sizes for high performance
        let send_buffer_size = 1024 * 1024; // 1MB
        let recv_buffer_size = 1024 * 1024; // 1MB
        
        // Note: Tokio UdpSocket doesn't expose buffer size methods directly
        // These would need to be set on the std socket before converting to tokio

        info!("UDP event system bound to {}", bind_addr);

        Ok(Self {
            socket: Arc::new(socket),
            connections: Arc::new(RwLock::new(HashMap::new())),
            player_addresses: Arc::new(RwLock::new(HashMap::new())),
            udp_handlers: Arc::new(RwLock::new(HashMap::new())),
            serializers: Arc::new(RwLock::new(HashMap::new())),
            deserializers: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(Mutex::new(false)),
            compression_enabled,
            compression_threshold,
            send_buffer_size,
            recv_buffer_size,
        })
    }

    /// Start the UDP event system
    pub async fn start(&self) -> Result<(), io::Error> {
        let mut running = self.running.lock().await;
        if *running {
            return Ok(());
        }
        *running = true;
        drop(running);

        info!("Starting UDP event system");

        // Start the receive loop
        self.start_receive_loop().await;

        Ok(())
    }

    /// Stop the UDP event system
    pub async fn stop(&self) {
        let mut running = self.running.lock().await;
        *running = false;
        info!("UDP event system stopped");
    }

    /// Register a UDP event handler
    pub async fn register_udp_handler<F>(
        &self,
        event_type: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        F: Fn(&UdpEventPacket, &mut UdpConnection, &[u8]) -> Result<Option<Vec<u8>>, EventError>
            + Send
            + Sync
            + 'static,
    {
        let handler_name = format!("udp:{}::{}", event_type, std::any::type_name::<F>());
        let typed_handler = TypedUdpEventHandler::new(handler_name.clone(), handler);
        let handler_arc: Arc<dyn UdpEventHandler> = Arc::new(typed_handler);

        let mut handlers = self.udp_handlers.write().await;
        handlers
            .entry(event_type.to_string())
            .or_insert_with(Vec::new)
            .push(handler_arc);

        info!("Registered UDP handler for {}", event_type);
        Ok(())
    }

    /// Register a binary event serializer
    pub async fn register_serializer(
        &self,
        event_type: &str,
        serializer: Arc<dyn BinaryEventSerializer>,
    ) {
        let mut serializers = self.serializers.write().await;
        serializers.insert(event_type.to_string(), serializer);
        info!("Registered binary serializer for {}", event_type);
    }

    /// Register a binary event deserializer
    pub async fn register_deserializer(
        &self,
        event_type: &str,
        deserializer: Arc<dyn BinaryEventDeserializer>,
    ) {
        let mut deserializers = self.deserializers.write().await;
        deserializers.insert(event_type.to_string(), deserializer);
        info!("Registered binary deserializer for {}", event_type);
    }

    /// Register default JSON serializer/deserializer for an event type
    pub async fn register_json_codec<T>(&self, event_type: &str)
    where
        T: Event + for<'de> Deserialize<'de> + 'static,
    {
        let serializer = Arc::new(JsonBinarySerializer::new(event_type.to_string()));
        let deserializer = Arc::new(JsonBinaryDeserializer::<T>::new(event_type.to_string()));

        self.register_serializer(event_type, serializer).await;
        self.register_deserializer(event_type, deserializer).await;
    }

    /// Send a UDP event to a specific player
    pub async fn send_udp_event<T>(
        &self,
        player_id: PlayerId,
        event_type: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event,
    {
        let player_addresses = self.player_addresses.read().await;
        let addr = player_addresses.get(&player_id).copied();
        drop(player_addresses);

        let addr = addr.ok_or_else(|| {
            EventError::HandlerNotFound(format!("Player {} not connected via UDP", player_id))
        })?;

        self.send_udp_event_to_addr(addr, event_type, event).await
    }

    /// Send a UDP event to a specific address
    pub async fn send_udp_event_to_addr<T>(
        &self,
        addr: SocketAddr,
        event_type: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event,
    {
        // Serialize the event
        let serializers = self.serializers.read().await;
        let serializer = serializers.get(event_type).ok_or_else(|| {
            EventError::HandlerNotFound(format!("No serializer for event type {}", event_type))
        })?;
        let payload = serializer.serialize(event)?;
        drop(serializers);

        // Get connection for sequence number
        let mut connections = self.connections.write().await;
        let connection = connections.get_mut(&addr).ok_or_else(|| {
            EventError::HandlerNotFound(format!("No UDP connection to {}", addr))
        })?;

        let sequence = connection.next_sequence();
        let source_id = connection.player_id.to_string();
        drop(connections);

        // Apply compression if enabled and beneficial
        let (compressed_payload, compression) = if self.compression_enabled 
            && payload.len() > self.compression_threshold {
            match self.compress_data(&payload) {
                Ok(compressed) if compressed.len() < payload.len() => {
                    (compressed, UdpCompressionType::Deflate)
                }
                _ => (payload, UdpCompressionType::None),
            }
        } else {
            (payload, UdpCompressionType::None)
        };

        // Create packet header
        let header = UdpEventHeader {
            version: 1,
            event_type: event_type.to_string(),
            source_id,
            target_id: String::new(),
            sequence,
            timestamp: crate::utils::current_timestamp(),
            compression,
            total_size: (compressed_payload.len() + 64) as u32, // Approximate header size
        };

        // Create packet
        let checksum = self.calculate_checksum(&compressed_payload);
        let packet = UdpEventPacket {
            header,
            payload: compressed_payload,
            checksum,
        };

        // Serialize packet
        let packet_data = serde_json::to_vec(&packet)
            .map_err(|e| EventError::HandlerExecution(format!("Packet serialization failed: {}", e)))?;

        // Send packet
        match self.socket.send_to(&packet_data, addr).await {
            Ok(bytes_sent) => {
                debug!("Sent {} bytes to {}", bytes_sent, addr);
                
                // Update connection stats
                let mut connections = self.connections.write().await;
                if let Some(conn) = connections.get_mut(&addr) {
                    conn.bytes_sent += bytes_sent as u64;
                    conn.update_last_seen();
                }
                
                Ok(())
            }
            Err(e) => Err(EventError::HandlerExecution(format!("UDP send failed: {}", e))),
        }
    }

    /// Broadcast a UDP event to all connected players
    pub async fn broadcast_udp_event<T>(
        &self,
        event_type: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event,
    {
        let player_addresses = self.player_addresses.read().await;
        let addresses: Vec<SocketAddr> = player_addresses.values().copied().collect();
        drop(player_addresses);

        for addr in addresses {
            if let Err(e) = self.send_udp_event_to_addr(addr, event_type, event).await {
                warn!("Failed to send UDP event to {}: {}", addr, e);
            }
        }

        Ok(())
    }

    /// Add a UDP connection for a player
    pub async fn add_udp_connection(
        &self,
        addr: SocketAddr,
        player_id: PlayerId,
        connection_id: String,
    ) {
        let connection = UdpConnection::new(addr, player_id, connection_id);
        
        let mut connections = self.connections.write().await;
        connections.insert(addr, connection);
        drop(connections);

        let mut player_addresses = self.player_addresses.write().await;
        player_addresses.insert(player_id, addr);
        drop(player_addresses);

        info!("Added UDP connection for player {} at {}", player_id, addr);
    }

    /// Remove a UDP connection
    pub async fn remove_udp_connection(&self, addr: SocketAddr) {
        let mut connections = self.connections.write().await;
        if let Some(connection) = connections.remove(&addr) {
            drop(connections);
            
            let mut player_addresses = self.player_addresses.write().await;
            player_addresses.remove(&connection.player_id);
            
            info!("Removed UDP connection for player {} at {}", connection.player_id, addr);
        }
    }

    /// Get connection statistics
    pub async fn get_udp_stats(&self) -> UdpStats {
        let connections = self.connections.read().await;
        let connection_count = connections.len();
        let total_bytes_sent = connections.values().map(|c| c.bytes_sent).sum();
        let total_bytes_received = connections.values().map(|c| c.bytes_received).sum();
        
        UdpStats {
            connection_count,
            total_bytes_sent,
            total_bytes_received,
            send_buffer_size: self.send_buffer_size,
            recv_buffer_size: self.recv_buffer_size,
        }
    }

    /// Start the receive loop
    async fn start_receive_loop(&self) {
        let socket = self.socket.clone();
        let connections = self.connections.clone();
        let player_addresses = self.player_addresses.clone();
        let udp_handlers = self.udp_handlers.clone();
        let deserializers = self.deserializers.clone();
        let running = self.running.clone();

        tokio::spawn(async move {
            let mut buffer = vec![0u8; 65536]; // 64KB buffer

            loop {
                {
                    let running_guard = running.lock().await;
                    if !*running_guard {
                        break;
                    }
                }

                match socket.recv_from(&mut buffer).await {
                    Ok((size, addr)) => {
                        if let Err(e) = Self::handle_received_packet(
                            &buffer[..size],
                            addr,
                            &connections,
                            &player_addresses,
                            &udp_handlers,
                            &deserializers,
                        ).await {
                            warn!("Failed to handle UDP packet from {}: {}", addr, e);
                        }
                    }
                    Err(e) => {
                        error!("UDP receive error: {}", e);
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }

            info!("UDP receive loop stopped");
        });
    }

    /// Handle a received UDP packet
    async fn handle_received_packet(
        data: &[u8],
        addr: SocketAddr,
        connections: &Arc<RwLock<HashMap<SocketAddr, UdpConnection>>>,
        player_addresses: &Arc<RwLock<HashMap<PlayerId, SocketAddr>>>,
        udp_handlers: &Arc<RwLock<HashMap<String, Vec<Arc<dyn UdpEventHandler>>>>>,
        deserializers: &Arc<RwLock<HashMap<String, Arc<dyn BinaryEventDeserializer>>>>,
    ) -> Result<(), EventError> {
        // Deserialize packet
        let packet: UdpEventPacket = serde_json::from_slice(data)
            .map_err(|e| EventError::HandlerExecution(format!("Packet deserialization failed: {}", e)))?;

        // Verify checksum
        let calculated_checksum = Self::calculate_checksum_static(&packet.payload);
        if calculated_checksum != packet.checksum {
            return Err(EventError::HandlerExecution("Checksum mismatch".to_string()));
        }

        // Update connection
        {
            let mut connections_guard = connections.write().await;
            if let Some(connection) = connections_guard.get_mut(&addr) {
                if !connection.is_sequence_valid(packet.header.sequence) {
                    warn!("Invalid sequence number {} from {}", packet.header.sequence, addr);
                    return Ok(()); // Skip duplicate or out-of-order packet
                }
                
                connection.update_in_sequence(packet.header.sequence);
                connection.bytes_received += data.len() as u64;
                connection.update_last_seen();
            } else {
                warn!("Received packet from unknown connection: {}", addr);
                return Ok(());
            }
        }

        // Decompress payload if needed
        let payload = match packet.header.compression {
            UdpCompressionType::None => packet.payload.clone(),
            UdpCompressionType::Deflate => {
                Self::decompress_data_static(&packet.payload)
                    .map_err(|e| EventError::HandlerExecution(format!("Decompression failed: {}", e)))?
            }
            UdpCompressionType::Lz4 => {
                return Err(EventError::HandlerExecution("LZ4 compression not implemented".to_string()));
            }
        };

        // Find and execute handlers
        let handlers_guard = udp_handlers.read().await;
        if let Some(event_handlers) = handlers_guard.get(&packet.header.event_type) {
            let event_handlers = event_handlers.clone();
            drop(handlers_guard);

            for handler in event_handlers {
                let mut connections_guard = connections.write().await;
                if let Some(connection) = connections_guard.get_mut(&addr) {
                    if let Err(e) = handler.handle_udp_event(&packet, connection, &payload).await {
                        error!("UDP handler {} failed: {}", handler.handler_name(), e);
                    }
                }
                drop(connections_guard);
            }
        }

        Ok(())
    }

    /// Calculate checksum for data integrity
    fn calculate_checksum(&self, data: &[u8]) -> u32 {
        Self::calculate_checksum_static(data)
    }

    fn calculate_checksum_static(data: &[u8]) -> u32 {
        // Simple CRC32-like checksum
        let mut checksum = 0u32;
        for &byte in data {
            checksum = checksum.wrapping_mul(31).wrapping_add(byte as u32);
        }
        checksum
    }

    /// Compress data using deflate algorithm
    fn compress_data(&self, data: &[u8]) -> Result<Vec<u8>, EventError> {
        use flate2::{Compression, write::DeflateEncoder};
        use std::io::Write;

        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(data)
            .map_err(|e| EventError::HandlerExecution(format!("Compression write failed: {}", e)))?;
        
        encoder.finish()
            .map_err(|e| EventError::HandlerExecution(format!("Compression finish failed: {}", e)))
    }

    /// Decompress data using deflate algorithm
    fn decompress_data_static(data: &[u8]) -> Result<Vec<u8>, EventError> {
        use flate2::read::DeflateDecoder;
        use std::io::Read;

        let mut decoder = DeflateDecoder::new(data);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)
            .map_err(|e| EventError::HandlerExecution(format!("Decompression failed: {}", e)))?;
        
        Ok(decompressed)
    }
}

/// UDP statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdpStats {
    pub connection_count: usize,
    pub total_bytes_sent: u64,
    pub total_bytes_received: u64,
    pub send_buffer_size: usize,
    pub recv_buffer_size: usize,
}

/// Extension trait for EventSystem to support UDP events
#[async_trait::async_trait]
pub trait UdpEventSystemExt {
    /// Register a UDP event handler
    async fn on_udp<F>(
        &self,
        event_type: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        F: Fn(&UdpEventPacket, &mut UdpConnection, &[u8]) -> Result<Option<Vec<u8>>, EventError>
            + Send
            + Sync
            + 'static;

    /// Emit a UDP event to a player
    async fn emit_udp<T>(
        &self,
        player_id: PlayerId,
        event_type: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event;

    /// Broadcast a UDP event to all players
    async fn broadcast_udp<T>(
        &self,
        event_type: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event;

    /// Get UDP system statistics
    async fn get_udp_stats(&self) -> Option<UdpStats>;
}

#[async_trait::async_trait]
impl UdpEventSystemExt for EventSystem {
    async fn on_udp<F>(
        &self,
        event_type: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        F: Fn(&UdpEventPacket, &mut UdpConnection, &[u8]) -> Result<Option<Vec<u8>>, EventError>
            + Send
            + Sync
            + 'static,
    {
        if let Some(ref udp_system) = self.udp_system {
            udp_system.register_udp_handler(event_type, handler).await
        } else {
            Err(EventError::HandlerExecution("UDP system not initialized".to_string()))
        }
    }

    async fn emit_udp<T>(
        &self,
        player_id: PlayerId,
        event_type: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event,
    {
        if let Some(ref udp_system) = self.udp_system {
            udp_system.send_udp_event(player_id, event_type, event).await
        } else {
            Err(EventError::HandlerExecution("UDP system not initialized".to_string()))
        }
    }

    async fn broadcast_udp<T>(
        &self,
        event_type: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event,
    {
        if let Some(ref udp_system) = self.udp_system {
            udp_system.broadcast_udp_event(event_type, event).await
        } else {
            Err(EventError::HandlerExecution("UDP system not initialized".to_string()))
        }
    }

    async fn get_udp_stats(&self) -> Option<UdpStats> {
        if let Some(ref udp_system) = self.udp_system {
            Some(udp_system.get_udp_stats().await)
        } else {
            None
        }
    }
}