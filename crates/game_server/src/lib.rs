//! Complete game server implementation with WebSocket handling
//! 
//! High-performance game server with plugin system, event-driven architecture,
//! and WebSocket-based client communication.

use event_system::{create_event_system, current_timestamp};
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

// Import the extension trait for generic methods
use types::EventSystemExt;

// ============================================================================
// Server Configuration
// ============================================================================

/// Server configuration
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
        }
    }
}

// ============================================================================
// Connection Management
// ============================================================================

/// Represents a connected client
#[derive(Debug)]
struct ClientConnection {
    player_id: Option<PlayerId>,
    remote_addr: SocketAddr,
    connected_at: std::time::SystemTime,
}

impl ClientConnection {
    fn new(remote_addr: SocketAddr) -> Self {
        Self {
            player_id: None,
            remote_addr,
            connected_at: std::time::SystemTime::now(),
        }
    }
}

/// Connection identifier
type ConnectionId = usize;

/// Manages active WebSocket connections
struct ConnectionManager {
    connections: Arc<RwLock<HashMap<ConnectionId, ClientConnection>>>,
    next_id: Arc<std::sync::atomic::AtomicUsize>,
    sender: broadcast::Sender<(ConnectionId, Vec<u8>)>,
}

impl ConnectionManager {
    fn new() -> Self {
        let (sender, _) = broadcast::channel(1000);
        
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(std::sync::atomic::AtomicUsize::new(1)),
            sender,
        }
    }
    
    /// Add a new connection
    async fn add_connection(&self, remote_addr: SocketAddr) -> ConnectionId {
        let connection_id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let connection = ClientConnection::new(remote_addr);
        
        let mut connections = self.connections.write().await;
        connections.insert(connection_id, connection);
        
        info!("New connection {} from {}", connection_id, remote_addr);
        connection_id
    }
    
    /// Remove a connection
    async fn remove_connection(&self, connection_id: ConnectionId) {
        let mut connections = self.connections.write().await;
        if let Some(connection) = connections.remove(&connection_id) {
            info!("Connection {} from {} disconnected", connection_id, connection.remote_addr);
        }
    }
    
    /// Set player ID for a connection
    async fn set_player_id(&self, connection_id: ConnectionId, player_id: PlayerId) {
        let mut connections = self.connections.write().await;
        if let Some(connection) = connections.get_mut(&connection_id) {
            connection.player_id = Some(player_id);
        }
    }
    
    /// Get player ID for a connection
    async fn get_player_id(&self, connection_id: ConnectionId) -> Option<PlayerId> {
        let connections = self.connections.read().await;
        connections.get(&connection_id).and_then(|c| c.player_id)
    }
    
    /// Send message to specific connection
    async fn send_to_connection(&self, connection_id: ConnectionId, message: Vec<u8>) {
        let _ = self.sender.send((connection_id, message));
    }
    
    /// Broadcast message to all connections
    async fn broadcast(&self, message: Vec<u8>) {
        let connections = self.connections.read().await;
        for &connection_id in connections.keys() {
            let _ = self.sender.send((connection_id, message.clone()));
        }
    }
    
    /// Get connection count
    async fn connection_count(&self) -> usize {
        let connections = self.connections.read().await;
        connections.len()
    }
    
    /// Subscribe to messages for a specific connection
    fn subscribe(&self) -> broadcast::Receiver<(ConnectionId, Vec<u8>)> {
        self.sender.subscribe()
    }
}

// ============================================================================
// Game Server
// ============================================================================

/// Main game server
pub struct GameServer {
    config: ServerConfig,
    plugin_manager: Arc<PluginManager>,
    server_context: Arc<ServerContextImpl>,
    connection_manager: Arc<ConnectionManager>,
    shutdown_sender: broadcast::Sender<()>,
    region_id: RegionId,
}

impl GameServer {
    /// Create a new game server
    pub fn new(config: ServerConfig) -> Self {
        let region_id = RegionId::new();
        let event_system = create_event_system();
        
        // Create plugin manager
        let plugin_manager = Arc::new(PluginManager::new(
            event_system.clone(),
            &config.plugin_directory,
            region_id,
        ));
        
        // Get server context from plugin manager
        let server_context = plugin_manager.get_server_context();
        
        // Create connection manager
        let connection_manager = Arc::new(ConnectionManager::new());
        
        // Create shutdown channel
        let (shutdown_sender, _) = broadcast::channel(1);
        
        Self {
            config,
            plugin_manager,
            server_context,
            connection_manager,
            shutdown_sender,
            region_id,
        }
    }
    
    /// Start the server
    pub async fn start(&self) -> Result<(), ServerError> {
        info!("Starting game server on {}", self.config.bind_address);
        info!("Region ID: {}", self.region_id.0);
        info!("Region bounds: {:?}", self.config.region_bounds);
        
        // Load all plugins
        info!("Loading plugins from: {}", self.config.plugin_directory.display());
        match self.plugin_manager.load_all_plugins().await {
            Ok(plugins) => {
                info!("Loaded {} plugins: {:?}", plugins.len(), plugins);
            }
            Err(e) => {
                error!("Failed to load plugins: {}", e);
                return Err(ServerError::Plugin(e));
            }
        }
        
        // Register core event handlers
        self.register_core_handlers().await?;
        
        // Start TCP listener
        let listener = TcpListener::bind(self.config.bind_address).await
            .map_err(|e| ServerError::Network(format!("Failed to bind to {}: {}", self.config.bind_address, e)))?;
        
        info!("Server listening on {}", self.config.bind_address);
        
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
                            
                            tokio::spawn(async move {
                                if let Err(e) = handle_connection(stream, addr, connection_manager, server_context, config).await {
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
    
    /// Register core event handlers
    async fn register_core_handlers(&self) -> Result<(), ServerError> {
        let events = self.server_context.events();
        
        // Register handler for player events to emit to plugins
        let server_context = self.server_context.clone();
        events.on("player_joined_internal", move |event: PlayerJoinedEvent| {
            let server_context = server_context.clone();
            tokio::spawn(async move {
                server_context.add_player(event.player.clone()).await;
            });
            Ok(())
        }).await?;
        
        let server_context = self.server_context.clone();
        events.on("player_left_internal", move |event: PlayerLeftEvent| {
            let server_context = server_context.clone();
            tokio::spawn(async move {
                server_context.remove_player(event.player_id).await;
            });
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
    
    /// Get server statistics
    pub async fn get_stats(&self) -> ServerStats {
        let event_stats = self.server_context.events().get_stats().await;
        let connection_count = self.connection_manager.connection_count().await;
        let loaded_plugins = self.plugin_manager.get_loaded_plugins().await;
        
        ServerStats {
            region_id: self.region_id,
            connection_count,
            plugin_count: loaded_plugins.len(),
            event_stats,
        }
    }
}

// ============================================================================
// Connection Handling
// ============================================================================

/// Handle a single WebSocket connection
async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    connection_manager: Arc<ConnectionManager>,
    server_context: Arc<ServerContextImpl>,
    config: ServerConfig,
) -> Result<(), ServerError> {
    // Perform WebSocket handshake
    let ws_stream = accept_async(stream).await
        .map_err(|e| ServerError::Network(format!("WebSocket handshake failed: {}", e)))?;
    
    let (ws_sender, mut ws_receiver) = ws_stream.split();
    let ws_sender = Arc::new(tokio::sync::Mutex::new(ws_sender));
    let connection_id = connection_manager.add_connection(addr).await;

    // Subscribe to outgoing messages
    let mut message_receiver = connection_manager.subscribe();

    // Clone handles for tasks
    let connection_manager_clone = connection_manager.clone();
    let server_context_clone = server_context.clone();
    let config_clone = config.clone();
    let ws_sender_incoming = ws_sender.clone();
    let ws_sender_outgoing = ws_sender.clone();

    // Incoming messages task
    let incoming_task = {
        let connection_manager = connection_manager.clone();
        let server_context = server_context.clone();
        let config = config.clone();
        let ws_sender = ws_sender_incoming;
        async move {
            while let Some(msg) = ws_receiver.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Err(e) = handle_client_message(
                            &text,
                            connection_id,
                            &connection_manager,
                            &server_context,
                            &config,
                        ).await {
                            error!("Error handling client message: {}", e);

                            // Send error response
                            let error_response = NetworkResponse::Error {
                                message: e.to_string(),
                            };
                            if let Ok(response_text) = serde_json::to_string(&error_response) {
                                let mut ws_sender = ws_sender.lock().await;
                                let _ = ws_sender.send(Message::Text(response_text.into())).await;
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        debug!("Client {} requested close", connection_id);
                        break;
                    }
                    Ok(Message::Ping(data)) => {
                        let mut ws_sender = ws_sender.lock().await;
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

    connection_manager_clone.remove_connection(connection_id).await;

    Ok(())
}

/// Handle a message from a client
async fn handle_client_message(
    text: &str,
    connection_id: ConnectionId,
    connection_manager: &ConnectionManager,
    server_context: &ServerContextImpl,
    config: &ServerConfig,
) -> Result<(), ServerError> {
    let message: NetworkMessage = serde_json::from_str(text)
        .map_err(|e| ServerError::Network(format!("Invalid JSON: {}", e)))?;
    
    debug!("Received message from connection {}: {:?}", connection_id, message);
    
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
            handle_client_data(connection_id, event_type, data, connection_manager, server_context).await
        }
    }
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

/// Handle custom client data
async fn handle_client_data(
    connection_id: ConnectionId,
    event_type: String,
    data: serde_json::Value,
    connection_manager: &ConnectionManager,
    server_context: &ServerContextImpl,
) -> Result<(), ServerError> {
    let player_id = connection_manager.get_player_id(connection_id).await
        .ok_or_else(|| ServerError::Player("Player not found for connection".to_string()))?;
    
    // Create client message event
    let event = ClientMessageEvent {
        player_id,
        message_type: event_type.clone(),
        data: data.clone(),
    };
    
    // Emit to both core and client namespaces
    server_context.events().emit_namespaced(EventId::core("client_message"), &event).await?;
    server_context.events().emit_namespaced(EventId::client(&event_type), &event).await?;
    
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
    
    Ok(())
}

// ============================================================================
// Server Statistics
// ============================================================================

/// Server statistics
#[derive(Debug, Clone)]
pub struct ServerStats {
    pub region_id: RegionId,
    pub connection_count: usize,
    pub plugin_count: usize,
    pub event_stats: EventSystemStats,
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Create a new game server with default configuration
pub fn create_server() -> GameServer {
    GameServer::new(ServerConfig::default())
}

/// Create a game server with custom configuration
pub fn create_server_with_config(config: ServerConfig) -> GameServer {
    GameServer::new(config)
}