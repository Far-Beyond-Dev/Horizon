//! Complete game server implementation with WebSocket handling and type-safe events
//! 
//! High-performance game server with plugin system, event-driven architecture,
//! and WebSocket-based client communication with enhanced type-safe routing.

use event_system::{create_event_system, current_timestamp, ClientEventRouter, create_event_system_with_router};
use plugin_system::{PluginManager, ServerContextImpl};
use types::*;
use futures::{SinkExt, StreamExt};
use serde_json;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};
use tracing::{debug, error, info, warn};

// ============================================================================
// Enhanced Server Configuration
// ============================================================================

/// Enhanced server configuration with client event routing settings
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Address to bind the server to
    pub bind_address: SocketAddr,
    /// Region bounds
    pub region_bounds: RegionBounds,
    /// Plugin directory
    pub plugin_directory: PathBuf,
    /// Maximum number of concurrent connections
    pub max_connections: usize,
    /// WebSocket ping interval in seconds
    pub ping_interval: u64,
    /// Connection timeout in seconds
    pub connection_timeout: u64,
    /// Client event routing configuration
    pub client_event_config: ClientEventConfig,
}

#[derive(Debug, Clone)]
pub struct ClientEventConfig {
    /// Enable type-safe client event routing
    pub enable_typed_routing: bool,
    /// Maximum message size for client events
    pub max_message_size: usize,
    /// Rate limiting for client events (per second)
    pub rate_limit_per_second: u32,
    /// Enable event validation
    pub validate_events: bool,
    /// Log all client events for debugging
    pub debug_log_events: bool,
}

impl Default for ClientEventConfig {
    fn default() -> Self {
        Self {
            enable_typed_routing: true,
            max_message_size: 1024 * 16, // 16KB
            rate_limit_per_second: 100,
            validate_events: true,
            debug_log_events: false,
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:8080".parse().unwrap(),
            region_bounds: RegionBounds {
                min_x: -1000.0,
                max_x: 1000.0,
                min_y: -1000.0,
                max_y: 1000.0,
                min_z: -100.0,
                max_z: 100.0,
            },
            plugin_directory: PathBuf::from("plugins"),
            max_connections: 1000,
            ping_interval: 30,
            connection_timeout: 60,
            client_event_config: ClientEventConfig::default(),
        }
    }
}

// ============================================================================
// Enhanced Connection Management with Event Tracking
// ============================================================================

/// Enhanced client connection with event tracking
#[derive(Debug)]
struct ClientConnection {
    player_id: Option<PlayerId>,
    remote_addr: SocketAddr,
    connected_at: std::time::SystemTime,
    events_sent: u64,
    last_event_time: std::time::SystemTime,
    rate_limit_tokens: u32,
}

impl ClientConnection {
    fn new(remote_addr: SocketAddr) -> Self {
        let now = std::time::SystemTime::now();
        Self {
            player_id: None,
            remote_addr,
            connected_at: now,
            events_sent: 0,
            last_event_time: now,
            rate_limit_tokens: 100, // Start with full tokens
        }
    }

    fn can_send_event(&mut self, config: &ClientEventConfig) -> bool {
        let now = std::time::SystemTime::now();
        
        // Refill tokens based on time passed
        if let Ok(duration) = now.duration_since(self.last_event_time) {
            let seconds_passed = duration.as_secs() as u32;
            self.rate_limit_tokens = (self.rate_limit_tokens + seconds_passed * config.rate_limit_per_second)
                .min(config.rate_limit_per_second);
            self.last_event_time = now;
        }

        if self.rate_limit_tokens > 0 {
            self.rate_limit_tokens -= 1;
            self.events_sent += 1;
            true
        } else {
            false
        }
    }
}

type ConnectionId = usize;

/// Enhanced connection manager with event routing capabilities
struct ConnectionManager {
    connections: Arc<RwLock<HashMap<ConnectionId, ClientConnection>>>,
    next_id: Arc<std::sync::atomic::AtomicUsize>,
    sender: broadcast::Sender<(ConnectionId, Vec<u8>)>,
    event_stats: Arc<RwLock<ConnectionEventStats>>,
}

#[derive(Debug, Default, Clone)]
struct ConnectionEventStats {
    total_messages_received: u64,
    total_events_routed: u64,
    invalid_messages: u64,
    rate_limited_messages: u64,
    events_by_type: HashMap<String, u64>,
}

impl ConnectionManager {
    fn new() -> Self {
        let (sender, _) = broadcast::channel(1000);
        
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(std::sync::atomic::AtomicUsize::new(1)),
            sender,
            event_stats: Arc::new(RwLock::new(ConnectionEventStats::default())),
        }
    }

    async fn add_connection(&self, remote_addr: SocketAddr) -> ConnectionId {
        let connection_id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let connection = ClientConnection::new(remote_addr);
        
        let mut connections = self.connections.write().await;
        connections.insert(connection_id, connection);
        
        info!("New connection {} from {}", connection_id, remote_addr);
        connection_id
    }

    async fn remove_connection(&self, connection_id: ConnectionId) {
        let mut connections = self.connections.write().await;
        if let Some(connection) = connections.remove(&connection_id) {
            info!("Connection {} from {} disconnected (sent {} events)", 
                  connection_id, connection.remote_addr, connection.events_sent);
        }
    }

    async fn check_rate_limit(&self, connection_id: ConnectionId, config: &ClientEventConfig) -> bool {
        let mut connections = self.connections.write().await;
        if let Some(connection) = connections.get_mut(&connection_id) {
            let can_send = connection.can_send_event(config);
            if !can_send {
                let mut stats = self.event_stats.write().await;
                stats.rate_limited_messages += 1;
            }
            can_send
        } else {
            false
        }
    }

    async fn record_event(&self, event_type: &str) {
        let mut stats = self.event_stats.write().await;
        stats.total_events_routed += 1;
        *stats.events_by_type.entry(event_type.to_string()).or_insert(0) += 1;
    }

    async fn record_invalid_message(&self) {
        let mut stats = self.event_stats.write().await;
        stats.invalid_messages += 1;
    }

    async fn get_stats(&self) -> ConnectionEventStats {
        let stats = self.event_stats.read().await;
        stats.clone()
    }

    async fn set_player_id(&self, connection_id: ConnectionId, player_id: PlayerId) {
        let mut connections = self.connections.write().await;
        if let Some(connection) = connections.get_mut(&connection_id) {
            connection.player_id = Some(player_id);
        }
    }
    
    async fn get_player_id(&self, connection_id: ConnectionId) -> Option<PlayerId> {
        let connections = self.connections.read().await;
        connections.get(&connection_id).and_then(|c| c.player_id)
    }
    
    async fn send_to_connection(&self, connection_id: ConnectionId, message: Vec<u8>) {
        let _ = self.sender.send((connection_id, message));
    }
    
    async fn broadcast(&self, message: Vec<u8>) {
        let connections = self.connections.read().await;
        for &connection_id in connections.keys() {
            let _ = self.sender.send((connection_id, message.clone()));
        }
    }
    
    async fn connection_count(&self) -> usize {
        let connections = self.connections.read().await;
        connections.len()
    }
    
    fn subscribe(&self) -> broadcast::Receiver<(ConnectionId, Vec<u8>)> {
        self.sender.subscribe()
    }
}

// ============================================================================
// Enhanced Game Server with Type-Safe Event Routing
// ============================================================================

/// Enhanced game server with integrated plugin system and client event routing
pub struct GameServer {
    config: ServerConfig,
    plugin_manager: Arc<PluginManager>,
    server_context: Arc<ServerContextImpl>,
    connection_manager: Arc<ConnectionManager>,
    shutdown_sender: broadcast::Sender<()>,
    region_id: RegionId,
    client_event_router: Arc<ClientEventRouter>,
    client_event_validator: Arc<ClientEventValidator>,
}

impl GameServer {
    /// Create a new enhanced game server
    pub fn new(config: ServerConfig) -> Self {
        let region_id = RegionId::new();
        let (event_system, client_router) = create_event_system_with_router();
        
        // Create plugin manager with enhanced event system
        let mut plugin_manager = PluginManager::new(
            event_system.clone(),
            &config.plugin_directory,
            region_id,
        );
        
        // Set the client router in the plugin manager
        plugin_manager.set_client_router(client_router.clone());
        let plugin_manager = Arc::new(plugin_manager);
        
        // Get enhanced server context
        let server_context = plugin_manager.get_server_context();
        
        // Create enhanced connection manager
        let connection_manager = Arc::new(ConnectionManager::new());
        
        // Create client event validator
        let client_event_validator = Arc::new(ClientEventValidator::new(config.client_event_config.clone()));
        
        // Create shutdown channel
        let (shutdown_sender, _) = broadcast::channel(1);
        
        Self {
            config,
            plugin_manager,
            server_context,
            connection_manager,
            shutdown_sender,
            region_id,
            client_event_router: client_router,
            client_event_validator,
        }
    }
    
    /// Start the enhanced server
    pub async fn start(&self) -> Result<(), ServerError> {
        info!("🚀 Starting enhanced game server on {}", self.config.bind_address);
        info!("Region ID: {}", self.region_id.0);
        info!("Client event routing: {}", if self.config.client_event_config.enable_typed_routing { "ENABLED" } else { "DISABLED" });
        
        // Setup enhanced plugin system
        if let Err(e) = self.plugin_manager.setup_event_routing().await {
            warn!("Failed to setup event routing: {}", e);
        }
        
        // Load all plugins with enhanced capabilities
        info!("🔌 Loading plugins from: {}", self.config.plugin_directory.display());
        match self.plugin_manager.load_all_plugins().await {
            Ok(plugins) => {
                info!("✅ Loaded {} plugins with type-safe event support: {:?}", plugins.len(), plugins);
                
                // Display plugin statistics
                let stats = self.plugin_manager.get_plugin_stats().await;
                info!("📊 Plugin System Stats:");
                info!("  - Total event handlers: {}", stats.total_handlers);
                info!("  - Plugins loaded: {}", stats.total_plugins);
                for plugin in &stats.plugins {
                    info!("    - {} v{}: {} handlers, capabilities: {:?}", 
                          plugin.name, plugin.version, plugin.handler_count, plugin.capabilities);
                }
            }
            Err(e) => {
                error!("❌ Failed to load plugins: {}", e);
                return Err(ServerError::Plugin(e));
            }
        }
        
        // Register enhanced core event handlers
        self.register_enhanced_core_handlers().await?;
        
        // Start TCP listener
        let listener = TcpListener::bind(self.config.bind_address).await
            .map_err(|e| ServerError::Network(format!("Failed to bind to {}: {}", self.config.bind_address, e)))?;
        
        info!("🌐 Server listening on {} with enhanced client event routing", self.config.bind_address);
        
        // Main server loop
        let mut shutdown_receiver = self.shutdown_sender.subscribe();
        
        loop {
            tokio::select! {
                // Accept new connections
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            let connection_manager = self.connection_manager.clone();
                            let server_context = self.server_context.clone();
                            let config = self.config.clone();
                            let validator = self.client_event_validator.clone();
                            let router = self.client_event_router.clone();
                            
                            tokio::spawn(async move {
                                if let Err(e) = handle_enhanced_connection(
                                    stream, 
                                    addr, 
                                    connection_manager, 
                                    server_context, 
                                    config,
                                    validator,
                                    router,
                                ).await {
                                    error!("Connection error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {}", e);
                        }
                    }
                }
                
                // Handle shutdown signal
                _ = shutdown_receiver.recv() => {
                    info!("Shutdown signal received");
                    break;
                }
            }
        }
        
        info!("Server stopped");
        Ok(())
    }
    
    /// Register enhanced core event handlers
    async fn register_enhanced_core_handlers(&self) -> Result<(), ServerError> {
        let events = self.server_context.events();
        
        // Enhanced player event handlers
        let server_context = self.server_context.clone();
        events.on_core("player_joined_internal", move |event: PlayerJoinedEvent| {
            let server_context = server_context.clone();
            tokio::spawn(async move {
                server_context.add_player(event.player.clone()).await;
                info!("✅ Player {} added to server context", event.player.name);
            });
            Ok(())
        }).await?;
        
        let server_context = self.server_context.clone();
        events.on_core("player_left_internal", move |event: PlayerLeftEvent| {
            let server_context = server_context.clone();
            tokio::spawn(async move {
                if let Some(player) = server_context.remove_player(event.player_id).await {
                    info!("✅ Player {} removed from server context", player.name);
                }
            });
            Ok(())
        }).await?;
        
        // Register client event monitoring
        events.on_core("client_event_processed", move |event: serde_json::Value| {
            debug!("📊 Client event processed: {:?}", event);
            Ok(())
        }).await?;
        
        Ok(())
    }
    
    /// Shutdown the server gracefully
    pub async fn shutdown(&self) -> Result<(), ServerError> {
        info!("Shutting down server...");
        
        // Shutdown plugins
        self.plugin_manager.shutdown_all().await?;
        
        // Send shutdown signal
        let _ = self.shutdown_sender.send(());
        
        info!("Server shutdown complete");
        Ok(())
    }
    
    /// Get enhanced server statistics
    pub async fn get_enhanced_stats(&self) -> EnhancedServerStats {
        let event_stats = self.server_context.events().get_stats().await;
        let connection_count = self.connection_manager.connection_count().await;
        let plugin_stats = self.plugin_manager.get_plugin_stats().await;
        let connection_stats = self.connection_manager.get_stats().await;
        let router_stats = self.client_event_router.get_stats().await;
        
        EnhancedServerStats {
            region_id: self.region_id,
            connection_count,
            plugin_stats,
            event_stats,
            connection_event_stats: connection_stats,
            router_stats,
            config: self.config.clone(),
        }
    }
}

// ============================================================================
// Enhanced Connection Handling with Type-Safe Event Routing
// ============================================================================

/// Handle a single WebSocket connection with enhanced event routing
async fn handle_enhanced_connection(
    stream: TcpStream,
    addr: SocketAddr,
    connection_manager: Arc<ConnectionManager>,
    server_context: Arc<ServerContextImpl>,
    config: ServerConfig,
    validator: Arc<ClientEventValidator>,
    router: Arc<ClientEventRouter>,
) -> Result<(), ServerError> {
    // Perform WebSocket handshake
    let ws_stream = accept_async(stream).await
        .map_err(|e| ServerError::Network(format!("WebSocket handshake failed: {}", e)))?;
    
    let (ws_sender, mut ws_receiver) = ws_stream.split();
    let ws_sender = Arc::new(tokio::sync::Mutex::new(ws_sender));
    let connection_id = connection_manager.add_connection(addr).await;

    info!("🔗 Enhanced connection {} established from {}", connection_id, addr);

    // Subscribe to outgoing messages
    let mut message_receiver = connection_manager.subscribe();

    // Clone handles for tasks
    let connection_manager_outgoing = connection_manager.clone();
    let ws_sender_incoming = ws_sender.clone();
    let ws_sender_outgoing = ws_sender.clone();

    // Incoming messages task with type-safe routing
    let incoming_task = {
        let connection_manager = connection_manager.clone();
        let server_context = server_context.clone();
        let config = config.clone();
        let validator = validator.clone();
        let router = router.clone();
        
        async move {
            while let Some(msg) = ws_receiver.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        // Enhanced message handling with type-safe routing
                        if let Err(e) = handle_enhanced_client_message(
                            &text,
                            connection_id,
                            &connection_manager,
                            &server_context,
                            &config,
                            &validator,
                            &router,
                        ).await {
                            error!("❌ Error handling client message: {}", e);
                            connection_manager.record_invalid_message().await;
                            
                            // Send error response
                            let error_response = NetworkResponse::Error {
                                message: e.to_string(),
                            };
                            if let Ok(response_text) = serde_json::to_string(&error_response) {
                                let mut ws_sender = ws_sender_incoming.lock().await;
                                let _ = ws_sender.send(Message::Text(response_text.into())).await;
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        debug!("🔌 Client {} requested close", connection_id);
                        break;
                    }
                    Ok(Message::Ping(data)) => {
                        let mut ws_sender = ws_sender_incoming.lock().await;
                        let _ = ws_sender.send(Message::Pong(data)).await;
                    }
                    Err(e) => {
                        error!("WebSocket error for connection {}: {}", connection_id, e);
                        break;
                    }
                    _ => {}
                }
            }
        }
    };

    // Outgoing messages task
    let outgoing_task = {
        let ws_sender = ws_sender_outgoing;
        async move {
            while let Ok((target_connection_id, message)) = message_receiver.recv().await {
                if target_connection_id == connection_id {
                    let message_text = String::from_utf8_lossy(&message);
                    let mut ws_sender = ws_sender.lock().await;
                    if let Err(e) = ws_sender.send(Message::Text(message_text.to_string().into())).await {
                        error!("Failed to send message to client: {}", e);
                        break;
                    }
                }
            }
        }
    };

    // Run both tasks concurrently, exit when either finishes
    tokio::select! {
        _ = incoming_task => {},
        _ = outgoing_task => {},
    }

    // Clean up on disconnect
    if let Some(player_id) = connection_manager.get_player_id(connection_id).await {
        // Emit player left event
        let event = PlayerLeftEvent {
            player_id,
            timestamp: current_timestamp(),
        };

        let _ = server_context.events().emit_namespaced(
            EventId::core("player_left"),
            &event,
        ).await;

        let _ = server_context.events().emit("player_left_internal", &event).await;
    }

    connection_manager_outgoing.remove_connection(connection_id).await;

    Ok(())
}

/// Enhanced client message handling with type-safe event routing
async fn handle_enhanced_client_message(
    text: &str,
    connection_id: ConnectionId,
    connection_manager: &ConnectionManager,
    server_context: &ServerContextImpl,
    config: &ServerConfig,
    validator: &ClientEventValidator,
    router: &ClientEventRouter,
) -> Result<(), ServerError> {
    
    // Rate limiting check
    if !connection_manager.check_rate_limit(connection_id, &config.client_event_config).await {
        warn!("⚠️ Rate limit exceeded for connection {}", connection_id);
        return Err(ServerError::Network("Rate limit exceeded".to_string()));
    }

    // Parse the network message
    let message: NetworkMessage = serde_json::from_str(text)
        .map_err(|e| ServerError::Network(format!("Invalid JSON: {}", e)))?;
    
    if config.client_event_config.debug_log_events {
        debug!("📨 Received message from connection {}: {:?}", connection_id, message);
    }
    
    match message {
        NetworkMessage::Join { name } => {
            handle_player_join(connection_id, name, connection_manager, server_context).await
        }
        
        NetworkMessage::Move { position } => {
            handle_player_move(connection_id, position, connection_manager, server_context, config).await
        }
        
        NetworkMessage::Leave => {
            handle_player_leave(connection_id, connection_manager, server_context).await
        }
        
        NetworkMessage::Data { event_type, data } => {
            // THIS IS THE KEY ENHANCEMENT - Type-safe event routing!
            handle_typed_client_data(
                connection_id, 
                event_type, 
                data, 
                connection_manager, 
                server_context,
                validator,
                router,
            ).await
        }
    }
}

/// Enhanced typed client data handling - routes to type-safe events
async fn handle_typed_client_data(
    connection_id: ConnectionId,
    event_type: String,
    data: serde_json::Value,
    connection_manager: &ConnectionManager,
    server_context: &ServerContextImpl,
    validator: &ClientEventValidator,
    router: &ClientEventRouter,
) -> Result<(), ServerError> {
    
    let player_id = connection_manager.get_player_id(connection_id).await
        .ok_or_else(|| ServerError::Player("Player not found for connection".to_string()))?;
    
    // Validate the event if configured
    if let Err(e) = validator.validate_event(&event_type, &data).await {
        warn!("❌ Event validation failed for {}: {}", event_type, e);
        return Err(ServerError::Internal(format!("Event validation failed: {}", e)));
    }

    // Convert data to bytes for routing
    let data_bytes = serde_json::to_vec(&data)
        .map_err(|e| ServerError::Internal(format!("Failed to serialize event data: {}", e)))?;
    
    // Route to the type-safe client event system
    router.route_message(&event_type, &data_bytes, player_id, server_context.events()).await
        .map_err(|e| ServerError::Internal(format!("Failed to route event: {}", e)))?;
    
    // Record the event for statistics
    connection_manager.record_event(&event_type).await;
    
    // Send acknowledgment
    let response = NetworkResponse::Data {
        event_type: "ack".to_string(),
        data: serde_json::json!({
            "original_event": event_type,
            "timestamp": current_timestamp()
        }),
    };
    
    let response_text = serde_json::to_string(&response)
        .map_err(|e| ServerError::Network(format!("Failed to serialize response: {}", e)))?;
    connection_manager.send_to_connection(connection_id, response_text.into_bytes()).await;
    
    info!("✅ Routed {} event from player {} to plugins", event_type, player_id);
    Ok(())
}

/// Handle player join request
async fn handle_player_join(
    connection_id: ConnectionId,
    name: String,
    connection_manager: &ConnectionManager,
    server_context: &ServerContextImpl,
) -> Result<(), ServerError> {
    // Create new player
    let player = Player::new(name.clone(), Position::new(0.0, 0.0, 0.0));
    let player_id = player.id;
    
    // Associate connection with player
    connection_manager.set_player_id(connection_id, player_id).await;
    
    // Send success response
    let response = NetworkResponse::JoinSuccess { player_id };
    let response_text = serde_json::to_string(&response)
        .map_err(|e| ServerError::Network(format!("Failed to serialize response: {}", e)))?;
    
    connection_manager.send_to_connection(connection_id, response_text.into_bytes()).await;
    
    // Emit player joined event
    let event = PlayerJoinedEvent {
        player: player.clone(),
        timestamp: current_timestamp(),
    };
    
    server_context.events().emit_namespaced(EventId::core("player_joined"), &event).await?;
    server_context.events().emit("player_joined_internal", &event).await?;
    
    info!("Player {} ({}) joined", name, player_id);
    Ok(())
}

/// Handle player move request
async fn handle_player_move(
    connection_id: ConnectionId,
    position: Position,
    connection_manager: &ConnectionManager,
    server_context: &ServerContextImpl,
    config: &ServerConfig,
) -> Result<(), ServerError> {
    // Validate position is within bounds
    if !config.region_bounds.contains(&position) {
        let response = NetworkResponse::MoveFailed {
            reason: "Position outside region bounds".to_string(),
        };
        let response_text = serde_json::to_string(&response)
            .map_err(|e| ServerError::Network(format!("Failed to serialize response: {}", e)))?;
        connection_manager.send_to_connection(connection_id, response_text.into_bytes()).await;
        return Ok(());
    }
    
    // Get player ID
    let player_id = connection_manager.get_player_id(connection_id).await
        .ok_or_else(|| ServerError::Player("Player not found for connection".to_string()))?;
    
    // Get old position
    let old_position = server_context.get_player(player_id).await?
        .map(|p| p.position)
        .unwrap_or_else(|| Position::new(0.0, 0.0, 0.0));
    
    // Update player position
    server_context.update_player_position(player_id, position).await?;
    
    // Send success response
    let response = NetworkResponse::MoveSuccess;
    let response_text = serde_json::to_string(&response)
        .map_err(|e| ServerError::Network(format!("Failed to serialize response: {}", e)))?;
    connection_manager.send_to_connection(connection_id, response_text.into_bytes()).await;
    
    // Emit player moved event
    let event = PlayerMovedEvent {
        player_id,
        old_position,
        new_position: position,
        timestamp: current_timestamp(),
    };
    
    server_context.events().emit_namespaced(EventId::core("player_moved"), &event).await?;
    
    debug!("Player {} moved to {:?}", player_id, position);
    Ok(())
}

/// Handle player leave request
async fn handle_player_leave(
    connection_id: ConnectionId,
    connection_manager: &ConnectionManager,
    server_context: &ServerContextImpl,
) -> Result<(), ServerError> {
    if let Some(player_id) = connection_manager.get_player_id(connection_id).await {
        // Emit player left event
        let event = PlayerLeftEvent {
            player_id,
            timestamp: current_timestamp(),
        };
        
        server_context.events().emit_namespaced(EventId::core("player_left"), &event).await?;
        server_context.events().emit("player_left_internal", &event).await?;
        
        info!("Player {} left", player_id);
    }
    
    Ok(())
}

// ============================================================================
// Client Event Validator
// ============================================================================

/// Validates client events before routing to prevent malicious data
pub struct ClientEventValidator {
    config: ClientEventConfig,
    known_events: Arc<RwLock<HashMap<String, EventSchema>>>,
}

#[derive(Debug, Clone)]
struct EventSchema {
    required_fields: Vec<String>,
    max_size: usize,
    allow_extra_fields: bool,
}

impl ClientEventValidator {
    pub fn new(config: ClientEventConfig) -> Self {
        let mut known_events = HashMap::new();
        
        // Register known event schemas
        known_events.insert("chat_message".to_string(), EventSchema {
            required_fields: vec!["message".to_string()],
            max_size: 1000,
            allow_extra_fields: true,
        });
        
        known_events.insert("move_command".to_string(), EventSchema {
            required_fields: vec!["target_x".to_string(), "target_y".to_string(), "target_z".to_string()],
            max_size: 200,
            allow_extra_fields: false,
        });
        
        known_events.insert("combat_action".to_string(), EventSchema {
            required_fields: vec!["action_type".to_string(), "ability_id".to_string()],
            max_size: 500,
            allow_extra_fields: true,
        });
        
        Self {
            config,
            known_events: Arc::new(RwLock::new(known_events)),
        }
    }
    
    pub async fn validate_event(&self, event_type: &str, data: &serde_json::Value) -> Result<(), String> {
        if !self.config.validate_events {
            return Ok(());
        }
        
        let known_events = self.known_events.read().await;
        
        if let Some(schema) = known_events.get(event_type) {
            // Check required fields
            if let Some(obj) = data.as_object() {
                for required_field in &schema.required_fields {
                    if !obj.contains_key(required_field) {
                        return Err(format!("Missing required field: {}", required_field));
                    }
                }
            }
            
            // Check size
            let serialized = serde_json::to_string(data).unwrap_or_default();
            if serialized.len() > schema.max_size {
                return Err(format!("Event too large: {} > {}", serialized.len(), schema.max_size));
            }
        }
        
        Ok(())
    }
}

// ============================================================================
// Enhanced Statistics
// ============================================================================

/// Enhanced server statistics with detailed event tracking
#[derive(Debug, Clone)]
pub struct EnhancedServerStats {
    pub region_id: RegionId,
    pub connection_count: usize,
    pub plugin_stats: plugin_system::PluginSystemStats,
    pub event_stats: EventSystemStats,
    pub connection_event_stats: ConnectionEventStats,
    pub router_stats: event_system::ClientEventRoutingStats,
    pub config: ServerConfig,
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Create a new enhanced game server with default configuration
pub fn create_enhanced_server() -> GameServer {
    GameServer::new(ServerConfig::default())
}

/// Create an enhanced game server with custom configuration
pub fn create_enhanced_server_with_config(config: ServerConfig) -> GameServer {
    GameServer::new(config)
}

/// Create a game server with default configuration (legacy compatibility)
pub fn create_server() -> GameServer {
    GameServer::new(ServerConfig::default())
}

/// Create a game server with custom configuration (legacy compatibility)
pub fn create_server_with_config(config: ServerConfig) -> GameServer {
    GameServer::new(config)
}