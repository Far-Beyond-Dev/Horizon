//! Refactored game server focused on core infrastructure only
//! 
//! This server handles only essential infrastructure: connections, plugin management,
//! and message routing. All game logic is delegated to plugins.

use event_system::{
    EventSystem, create_event_system, 
    PlayerConnectedEvent, PlayerDisconnectedEvent, RawClientMessageEvent,
    RegionStartedEvent, DisconnectReason, RegionBounds, 
    PlayerId, RegionId, current_timestamp
};
use plugin_system::{PluginManager, create_plugin_manager_with_events};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};
use tracing::{debug, error, info, warn};

// ============================================================================
// Minimal Server Configuration
// ============================================================================

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind_address: SocketAddr,
    pub region_bounds: RegionBounds,
    pub plugin_directory: PathBuf,
    pub max_connections: usize,
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
            connection_timeout: 60,
        }
    }
}

// ============================================================================
// Minimal Connection Management (Infrastructure Only)
// ============================================================================

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

type ConnectionId = usize;

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

    async fn add_connection(&self, remote_addr: SocketAddr) -> ConnectionId {
        let connection_id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let connection = ClientConnection::new(remote_addr);
        
        let mut connections = self.connections.write().await;
        connections.insert(connection_id, connection);
        
        info!("🔗 Connection {} from {}", connection_id, remote_addr);
        connection_id
    }

    async fn remove_connection(&self, connection_id: ConnectionId) {
        let mut connections = self.connections.write().await;
        if let Some(connection) = connections.remove(&connection_id) {
            info!("❌ Connection {} from {} disconnected", 
                  connection_id, connection.remote_addr);
        }
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
    
    fn subscribe(&self) -> broadcast::Receiver<(ConnectionId, Vec<u8>)> {
        self.sender.subscribe()
    }
}

// ============================================================================
// Core Infrastructure Game Server
// ============================================================================

pub struct GameServer {
    config: ServerConfig,
    event_system: Arc<EventSystem>,
    connection_manager: Arc<ConnectionManager>,
    plugin_manager: Arc<PluginManager>,
    shutdown_sender: broadcast::Sender<()>,
    region_id: RegionId,
}

impl GameServer {
    pub fn new(config: ServerConfig) -> Self {
        let region_id = RegionId::new();
        let event_system = create_event_system();
        let connection_manager = Arc::new(ConnectionManager::new());
        let (shutdown_sender, _) = broadcast::channel(1);
        
                // Create plugin manager with the event system
        let plugin_manager = Arc::new(create_plugin_manager_with_events(
            event_system.clone(),
            &config.plugin_directory,
            region_id,
        ));

        Self {
            config,
            event_system,
            connection_manager,
            plugin_manager,
            shutdown_sender,
            region_id,
        }
    }
    
    pub async fn start(&self) -> Result<(), ServerError> {
        info!("🚀 Starting game server on {}", self.config.bind_address);
        info!("🌍 Region ID: {}", self.region_id.0);
        
        // Register minimal core event handlers
        self.register_core_handlers().await?;
        
                info!("🔌 Loading plugins from: {}", self.config.plugin_directory.display());
        match self.plugin_manager.load_all_plugins().await {
            Ok(loaded_plugins) => {
                info!("✅ Successfully loaded {} plugins: {:?}", loaded_plugins.len(), loaded_plugins);
            }
            Err(e) => {
                warn!("⚠️ Plugin loading failed: {}. Server will continue without plugins.", e);
            }
        }

        // Display plugin statistics
        let plugin_stats = self.plugin_manager.get_plugin_stats().await;
        info!("📊 Plugin System Status:");
        info!("  - Plugins loaded: {}", plugin_stats.total_plugins);
        info!("  - Total handlers: {}", plugin_stats.total_handlers);
        info!("  - Client events routed: {}", plugin_stats.client_events_routed);

        self.event_system.emit_core("region_started", &RegionStartedEvent {
            region_id: self.region_id,
            bounds: self.config.region_bounds.clone(),
            timestamp: current_timestamp(),
        }).await.map_err(|e| ServerError::Internal(e.to_string()))?;

        // Emit region started event (for plugins)
        self.event_system.emit_core("region_started", &RegionStartedEvent {
            region_id: self.region_id,
            bounds: self.config.region_bounds.clone(),
            timestamp: current_timestamp(),
        }).await.map_err(|e| ServerError::Internal(e.to_string()))?;
        
        // Start TCP listener
        let listener = TcpListener::bind(self.config.bind_address).await
            .map_err(|e| ServerError::Network(format!("Failed to bind: {}", e)))?;
        
        info!("🌐 Server listening on {}", self.config.bind_address);
        
        // Main server loop (infrastructure only)
        let mut shutdown_receiver = self.shutdown_sender.subscribe();
        
        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            let connection_manager = self.connection_manager.clone();
                            let event_system = self.event_system.clone();
                            
                            tokio::spawn(async move {
                                if let Err(e) = handle_connection(
                                    stream, 
                                    addr, 
                                    connection_manager, 
                                    event_system,
                                ).await {
                                    error!("Connection error: {:?}", e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {}", e);
                        }
                    }
                }
                
                _ = shutdown_receiver.recv() => {
                    info!("Shutdown signal received");
                    break;
                }
            }
        }

        // Shutdown plugins when server stops
        info!("🔌 Shutting down plugins...");
        if let Err(e) = self.plugin_manager.shutdown_all().await {
            error!("Error shutting down plugins: {}", e);
        } else {
            info!("✅ All plugins shut down successfully");
        }
        
        info!("Server stopped");
        Ok(())
    }
    
    /// Register only essential core infrastructure event handlers
    async fn register_core_handlers(&self) -> Result<(), ServerError> {
        // Core infrastructure events only - no game logic!
        
        self.event_system.on_core("player_connected", |event: PlayerConnectedEvent| {
            info!("👋 Player {} connected from {}", 
                  event.player_id, event.remote_addr);
            Ok(())
        }).await.map_err(|e| ServerError::Internal(e.to_string()))?;
        
        self.event_system.on_core("player_disconnected", |event: PlayerDisconnectedEvent| {
            info!("👋 Player {} disconnected: {:?}", 
                  event.player_id, event.reason);
            Ok(())
        }).await.map_err(|e| ServerError::Internal(e.to_string()))?;
        
        self.event_system.on_core("region_started", |event: RegionStartedEvent| {
            info!("🌍 Region {:?} started with bounds: {:?}", 
                  event.region_id, event.bounds);
            Ok(())
        }).await.map_err(|e| ServerError::Internal(e.to_string()))?;
        
        Ok(())
    }
    
    pub async fn shutdown(&self) -> Result<(), ServerError> {
        info!("🛑 Shutting down server...");
        let _ = self.shutdown_sender.send(());
        Ok(())
    }
    
    pub fn get_event_system(&self) -> Arc<EventSystem> {
        self.event_system.clone()
    }
}

// ============================================================================
// Minimal Connection Handling (No Game Logic)
// ============================================================================

async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    connection_manager: Arc<ConnectionManager>,
    event_system: Arc<EventSystem>,
) -> Result<(), ServerError> {
    let ws_stream = accept_async(stream).await
        .map_err(|e| ServerError::Network(format!("WebSocket handshake failed: {}", e)))?;
    
    let (ws_sender, mut ws_receiver) = ws_stream.split();
    let ws_sender = Arc::new(tokio::sync::Mutex::new(ws_sender));
    let connection_id = connection_manager.add_connection(addr).await;

    // Generate player ID and emit connection event
    let player_id = PlayerId::new();
    connection_manager.set_player_id(connection_id, player_id).await;
    
    // Emit core infrastructure event
    event_system.emit_core("player_connected", &PlayerConnectedEvent {
        player_id,
        connection_id: connection_id.to_string(),
        remote_addr: addr.to_string(),
        timestamp: current_timestamp(),
    }).await.map_err(|e| ServerError::Internal(e.to_string()))?;

    let mut message_receiver = connection_manager.subscribe();
    let ws_sender_incoming = ws_sender.clone();
    let ws_sender_outgoing = ws_sender.clone();

    // Incoming message task - routes raw messages to plugins
    let incoming_task = {
        let connection_manager = connection_manager.clone();
        let event_system = event_system.clone();
        
        async move {
            while let Some(msg) = ws_receiver.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        // Route raw message to plugins via events
                        if let Err(e) = route_client_message(
                            &text,
                            connection_id,
                            &connection_manager,
                            &event_system,
                        ).await {
                            error!("❌ Message routing error: {}", e);
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

    // Outgoing message task
    let outgoing_task = {
        let ws_sender = ws_sender_outgoing;
        async move {
            while let Ok((target_connection_id, message)) = message_receiver.recv().await {
                if target_connection_id == connection_id {
                    let message_text = String::from_utf8_lossy(&message);
                    let mut ws_sender = ws_sender.lock().await;
                    if let Err(e) = ws_sender.send(Message::Text(message_text.to_string().into())).await {
                        error!("Failed to send message: {}", e);
                        break;
                    }
                }
            }
        }
    };

    tokio::select! {
        _ = incoming_task => {},
        _ = outgoing_task => {},
    }

    // Emit disconnection event
    if let Some(player_id) = connection_manager.get_player_id(connection_id).await {
        event_system.emit_core("player_disconnected", &PlayerDisconnectedEvent {
            player_id,
            connection_id: connection_id.to_string(),
            reason: DisconnectReason::ClientDisconnect,
            timestamp: current_timestamp(),
        }).await.map_err(|e| ServerError::Internal(e.to_string()))?;
    }

    connection_manager.remove_connection(connection_id).await;
    Ok(())
}

/// Route raw client message to plugins (no game logic in core!)
async fn route_client_message(
    text: &str,
    connection_id: ConnectionId,
    connection_manager: &ConnectionManager,
    event_system: &EventSystem,
) -> Result<(), ServerError> {
    
    // Parse as generic message structure
    let message: ClientMessage = serde_json::from_str(text)
        .map_err(|e| ServerError::Network(format!("Invalid JSON: {}", e)))?;
    
    let player_id = connection_manager.get_player_id(connection_id).await
        .ok_or_else(|| ServerError::Internal("Player not found".to_string()))?;
    
    debug!("📨 Routing message to namespace '{}' event '{}' from player {}", 
           message.namespace, message.event, player_id);
    
    // Create raw message event for plugins to handle
    let raw_event = RawClientMessageEvent {
        player_id,
        message_type: format!("{}:{}", message.namespace, message.event),
        data: message.data.to_string().into_bytes(),
        timestamp: current_timestamp(),
    };
    
    // Emit to core for routing (plugins will listen to this)
    event_system.emit_core("raw_client_message", &raw_event).await
        .map_err(|e| ServerError::Internal(e.to_string()))?;
    
    // Generic routing using client-specified namespace and event
    event_system.emit_client(&message.namespace, &message.event, &message.data).await
        .map_err(|e| ServerError::Internal(e.to_string()))?;
    
    info!("✅ Routed '{}:{}' message from player {} to plugins", 
          message.namespace, message.event, player_id);
    Ok(())
}

// ============================================================================
// Simple Message Types (Infrastructure Only)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClientMessage {
    namespace: String,
    event: String,
    data: serde_json::Value,
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

// ============================================================================
// Utility Functions
// ============================================================================

pub fn create_server() -> GameServer {
    GameServer::new(ServerConfig::default())
}

pub fn create_server_with_config(config: ServerConfig) -> GameServer {
    GameServer::new(config)
}

// ============================================================================
// Example Usage and Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_core_server_creation() {
        let server = create_server();
        let events = server.get_event_system();
        
        // Verify we can register core handlers
        events.on_core("test_event", |event: serde_json::Value| {
            println!("Test core event: {:?}", event);
            Ok(())
        }).await.unwrap();
        
        // Emit a test event
        events.emit_core("test_event", &serde_json::json!({
            "test": "data"
        })).await.unwrap();
    }
    
    #[tokio::test]
    async fn test_plugin_message_routing() {
        let server = create_server();
        let events = server.get_event_system();
        
        // Register handlers that plugins would register
        events.on_client("movement", "move_request", |event: serde_json::Value| {
            println!("Movement plugin would handle: {:?}", event);
            Ok(())
        }).await.unwrap();
        
        events.on_client("chat", "send_message", |event: serde_json::Value| {
            println!("Chat plugin would handle: {:?}", event);
            Ok(())
        }).await.unwrap();
        
        // Test routing
        events.emit_client("movement", "move_request", &serde_json::json!({
            "target_x": 100.0,
            "target_y": 200.0,
            "target_z": 0.0
        })).await.unwrap();
        
        events.emit_client("chat", "send_message", &serde_json::json!({
            "message": "Hello world!",
            "channel": "general"
        })).await.unwrap();
    }
    
    #[tokio::test]
    async fn test_generic_client_message_routing() {
        let server = create_server();
        let events = server.get_event_system();
        
        // Register handlers for different namespaces/events
        events.on_client("movement", "jump", |event: serde_json::Value| {
            println!("Movement plugin handles jump: {:?}", event);
            Ok(())
        }).await.unwrap();
        
        events.on_client("inventory", "use_item", |event: serde_json::Value| {
            println!("Inventory plugin handles use_item: {:?}", event);
            Ok(())
        }).await.unwrap();
        
        events.on_client("custom_plugin", "custom_event", |event: serde_json::Value| {
            println!("Custom plugin handles custom_event: {:?}", event);
            Ok(())
        }).await.unwrap();
        
        // Test the new generic routing
        events.emit_client("movement", "jump", &serde_json::json!({
            "height": 5.0
        })).await.unwrap();
        
        events.emit_client("inventory", "use_item", &serde_json::json!({
            "item_id": "potion_health",
            "quantity": 1
        })).await.unwrap();
        
        events.emit_client("custom_plugin", "custom_event", &serde_json::json!({
            "custom_data": "anything"
        })).await.unwrap();
        
        println!("✅ All messages routed generically without hardcoded logic!");
    }
    
    /// Example of how clean the new server is - just infrastructure!
    #[tokio::test]
    async fn demonstrate_clean_separation() {
        let server = create_server();
        let events = server.get_event_system();
        
        println!("🧹 This server only handles:");
        println!("  - WebSocket connections");
        println!("  - Generic message routing"); 
        println!("  - Plugin communication");
        println!("  - Core infrastructure events");
        println!("");
        println!("🎮 Game logic is handled by plugins:");
        println!("  - Movement, combat, chat, inventory");
        println!("  - All game-specific events");
        println!("  - Business logic and rules");
        
        // Show the clean API in action
        events.on_core("player_connected", |event: PlayerConnectedEvent| {
            println!("✅ Core: Player {} connected", event.player_id);
            Ok(())
        }).await.unwrap();
        
        // This would be handled by movement plugin, not core
        events.on_client("movement", "jump", |event: serde_json::Value| {
            println!("🦘 Movement Plugin: Jump event {:?}", event);
            Ok(())
        }).await.unwrap();
        
        println!("✨ Clean separation achieved with generic routing!");
    }
}