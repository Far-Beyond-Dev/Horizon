//! Enhanced communication system with backward compatibility.
//!
//! This module integrates the new generalized communication stack with the existing
//! WebSocket-based connection manager while maintaining full backward compatibility.

use crate::connection::{ConnectionManager, ConnectionId, GameServerResponseSender};
use horizon_event_system::{
    CommunicationFactory, TcpJsonEndpoint, 
    transport::{Connection, TcpConnection},
    EventSystem, PlayerId,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use serde::{Serialize, Deserialize};

/// Enhanced connection manager that supports both WebSocket and raw protocol connections.
pub struct EnhancedConnectionManager {
    /// Original WebSocket-based connection manager for backward compatibility.
    websocket_manager: Arc<ConnectionManager>,
    
    /// New protocol-based connections indexed by connection ID.
    protocol_connections: Arc<tokio::sync::RwLock<std::collections::HashMap<ConnectionId, ProtocolConnectionInfo>>>,
    
    /// Next connection ID counter.
    next_id: Arc<std::sync::atomic::AtomicUsize>,
}

/// Information about a protocol-based connection.
#[derive(Debug)]
struct ProtocolConnectionInfo {
    player_id: Option<PlayerId>,
    remote_addr: SocketAddr,
    connection_type: ConnectionType,
}

/// Types of connections supported.
#[derive(Debug)]
enum ConnectionType {
    /// Legacy WebSocket connection (handled by existing ConnectionManager).
    WebSocket,
    /// New TCP+JSON connection using the generalized stack.
    TcpJson,
    /// Future: TCP+Binary connection.
    TcpBinary,
    /// Future: UDP+JSON connection.
    UdpJson,
    /// Future: UDP+Binary connection.
    UdpBinary,
}

impl EnhancedConnectionManager {
    /// Create a new enhanced connection manager that wraps the existing one.
    pub fn new(websocket_manager: Arc<ConnectionManager>) -> Self {
        Self {
            websocket_manager,
            protocol_connections: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            next_id: Arc::new(std::sync::atomic::AtomicUsize::new(10_000)), // Start higher to avoid conflicts
        }
    }
    
    /// Add a WebSocket connection (delegates to existing manager).
    pub async fn add_websocket_connection(&self, remote_addr: SocketAddr) -> ConnectionId {
        self.websocket_manager.add_connection(remote_addr).await
    }
    
    /// Add a TCP+JSON connection using the new generalized stack.
    pub async fn add_tcp_json_connection(&self, remote_addr: SocketAddr) -> ConnectionId {
        let connection_id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        let info = ProtocolConnectionInfo {
            player_id: None,
            remote_addr,
            connection_type: ConnectionType::TcpJson,
        };
        
        let mut connections = self.protocol_connections.write().await;
        connections.insert(connection_id, info);
        
        tracing::info!("🔗 TCP+JSON Connection {} from {}", connection_id, remote_addr);
        connection_id
    }
    
    /// Remove any type of connection.
    pub async fn remove_connection(&self, connection_id: ConnectionId) {
        // Try protocol connections first
        {
            let mut connections = self.protocol_connections.write().await;
            if let Some(info) = connections.remove(&connection_id) {
                tracing::info!("❌ {:?} Connection {} from {} disconnected", 
                              info.connection_type, connection_id, info.remote_addr);
                return;
            }
        }
        
        // Fall back to WebSocket manager
        self.websocket_manager.remove_connection(connection_id).await;
    }
    
    /// Set player ID for any type of connection.
    pub async fn set_player_id(&self, connection_id: ConnectionId, player_id: PlayerId) {
        // Try protocol connections first
        {
            let mut connections = self.protocol_connections.write().await;
            if let Some(info) = connections.get_mut(&connection_id) {
                info.player_id = Some(player_id);
                return;
            }
        }
        
        // Fall back to WebSocket manager
        self.websocket_manager.set_player_id(connection_id, player_id).await;
    }
    
    /// Get player ID for any type of connection.
    pub async fn get_player_id(&self, connection_id: ConnectionId) -> Option<PlayerId> {
        // Try protocol connections first
        {
            let connections = self.protocol_connections.read().await;
            if let Some(info) = connections.get(&connection_id) {
                return info.player_id;
            }
        }
        
        // Fall back to WebSocket manager
        self.websocket_manager.get_player_id(connection_id).await
    }
    
    /// Send message to a connection (automatically detects connection type).
    pub async fn send_to_connection(&self, connection_id: ConnectionId, message: Vec<u8>) {
        // Check if it's a protocol connection
        let is_protocol_connection = {
            let connections = self.protocol_connections.read().await;
            connections.contains_key(&connection_id)
        };
        
        if is_protocol_connection {
            // For now, protocol connections don't have a direct send mechanism
            // In a full implementation, we'd store the actual connection endpoints
            tracing::warn!("Protocol connection {} send not yet implemented", connection_id);
        } else {
            // Use WebSocket manager
            self.websocket_manager.send_to_connection(connection_id, message).await;
        }
    }
    
    /// Create a response sender for the event system that works with both connection types.
    pub fn create_response_sender(&self) -> EnhancedResponseSender {
        EnhancedResponseSender {
            websocket_sender: GameServerResponseSender::new(self.websocket_manager.clone()),
            enhanced_manager: Arc::new(self.clone()),
        }
    }
    
    /// Get the underlying WebSocket manager for backward compatibility.
    pub fn websocket_manager(&self) -> &Arc<ConnectionManager> {
        &self.websocket_manager
    }
}

impl Clone for EnhancedConnectionManager {
    fn clone(&self) -> Self {
        Self {
            websocket_manager: self.websocket_manager.clone(),
            protocol_connections: self.protocol_connections.clone(),
            next_id: self.next_id.clone(),
        }
    }
}

/// Enhanced response sender that works with both WebSocket and protocol connections.
#[derive(Clone)]
pub struct EnhancedResponseSender {
    /// WebSocket response sender for backward compatibility.
    websocket_sender: GameServerResponseSender,
    /// Reference to enhanced manager for protocol connections.
    enhanced_manager: Arc<EnhancedConnectionManager>,
}

impl horizon_event_system::ClientResponseSender for EnhancedResponseSender {
    fn send_to_client(&self, player_id: PlayerId, data: Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        // For now, delegate to WebSocket sender
        // TODO: Check if player is on a protocol connection first
        self.websocket_sender.send_to_client(player_id, data)
    }
    
    fn is_connection_active(&self, player_id: PlayerId) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + '_>> {
        self.websocket_sender.is_connection_active(player_id)
    }
    
    fn get_auth_status(&self, player_id: PlayerId) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<horizon_event_system::AuthenticationStatus>> + Send + '_>> {
        self.websocket_sender.get_auth_status(player_id)
    }
    
    fn get_connection_info(&self, player_id: PlayerId) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<horizon_event_system::ClientConnectionInfo>> + Send + '_>> {
        self.websocket_sender.get_connection_info(player_id)
    }
}

impl std::fmt::Debug for EnhancedResponseSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnhancedResponseSender")
            .field("websocket_sender", &"[GameServerResponseSender]")
            .field("enhanced_manager", &"[EnhancedConnectionManager]")
            .finish()
    }
}

/// Message format for the new protocol connections (backward compatible with existing JSON format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolMessage {
    /// The plugin namespace that should handle this message.
    pub namespace: String,
    
    /// The specific event type within the namespace.
    pub event: String,
    
    /// The message payload as a JSON value.
    pub data: serde_json::Value,
    
    /// Optional protocol metadata (for future extensions).
    #[serde(default)]
    pub meta: Option<serde_json::Value>,
}

impl From<crate::messaging::ClientMessage> for ProtocolMessage {
    /// Convert from existing ClientMessage for backward compatibility.
    fn from(msg: crate::messaging::ClientMessage) -> Self {
        Self {
            namespace: msg.namespace,
            event: msg.event,
            data: msg.data,
            meta: None,
        }
    }
}

impl From<ProtocolMessage> for crate::messaging::ClientMessage {
    /// Convert to existing ClientMessage for backward compatibility.
    fn from(msg: ProtocolMessage) -> Self {
        Self {
            namespace: msg.namespace,
            event: msg.event,
            data: msg.data,
        }
    }
}

/// Handle a TCP+JSON connection using the new generalized stack.
pub async fn handle_tcp_json_connection(
    stream: TcpStream,
    addr: SocketAddr,
    enhanced_manager: Arc<EnhancedConnectionManager>,
    event_system: Arc<EventSystem>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create communication endpoint
    let tcp_connection = TcpConnection::new(stream, addr);
    let mut endpoint = CommunicationFactory::tcp_json()
        .build(Box::new(tcp_connection));
    
    // Register with enhanced manager
    let connection_id = enhanced_manager.add_tcp_json_connection(addr).await;
    
    // Generate player ID
    let player_id = PlayerId::new();
    enhanced_manager.set_player_id(connection_id, player_id).await;
    
    tracing::info!("🎮 TCP+JSON Player {} connected (Connection: {})", player_id, connection_id);
    
    // Message handling loop
    while let Ok(Some(message)) = endpoint.receive_message::<ProtocolMessage>().await {
        tracing::debug!("📨 TCP+JSON message from player {}: {}:{}", 
                       player_id, message.namespace, message.event);
        
        // Convert to existing message format for compatibility
        let client_message: crate::messaging::ClientMessage = message.into();
        
        // Route through existing message router
        if let Err(e) = crate::messaging::route_client_message(
            &serde_json::to_string(&client_message)?,
            connection_id,
            enhanced_manager.websocket_manager(),
            &event_system,
        ).await {
            tracing::error!("Failed to route TCP+JSON message: {}", e);
        }
    }
    
    // Cleanup
    enhanced_manager.remove_connection(connection_id).await;
    tracing::info!("🔌 TCP+JSON Player {} disconnected", player_id);
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::ConnectionManager;
    
    #[tokio::test]
    async fn test_enhanced_connection_manager() {
        let websocket_manager = Arc::new(ConnectionManager::new());
        let enhanced = EnhancedConnectionManager::new(websocket_manager.clone());
        
        // Test WebSocket connection (should work as before)
        let ws_conn_id = enhanced.add_websocket_connection("127.0.0.1:8080".parse().unwrap()).await;
        assert!(ws_conn_id > 0);
        
        // Test player ID management
        let player_id = PlayerId::new();
        enhanced.set_player_id(ws_conn_id, player_id).await;
        assert_eq!(enhanced.get_player_id(ws_conn_id).await, Some(player_id));
        
        enhanced.remove_connection(ws_conn_id).await;
    }
    
    #[test]
    fn test_message_conversion() {
        let protocol_msg = ProtocolMessage {
            namespace: "test".to_string(),
            event: "event".to_string(),
            data: serde_json::json!({"key": "value"}),
            meta: Some(serde_json::json!({"version": 1})),
        };
        
        let client_msg: crate::messaging::ClientMessage = protocol_msg.clone().into();
        assert_eq!(client_msg.namespace, protocol_msg.namespace);
        assert_eq!(client_msg.event, protocol_msg.event);
        assert_eq!(client_msg.data, protocol_msg.data);
        
        let back_to_protocol: ProtocolMessage = client_msg.into();
        assert_eq!(back_to_protocol.namespace, protocol_msg.namespace);
        assert_eq!(back_to_protocol.event, protocol_msg.event);
        assert_eq!(back_to_protocol.data, protocol_msg.data);
        // Meta is lost in conversion (expected)
        assert_eq!(back_to_protocol.meta, None);
    }
}