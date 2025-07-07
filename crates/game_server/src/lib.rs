//! # Minimal Game Server - Clean Core Infrastructure
//!
//! A refactored game server focused on core infrastructure only
//!
//! This server handles only essential infrastructure: connections, plugin management,
//! and message routing. All game logic is delegated to plugins.

#[allow(unused_imports, dead_code)]
use horizon_event_system::{
    create_horizon_event_system, current_timestamp, DisconnectReason, EventSystem, PlayerConnectedEvent,
    PlayerDisconnectedEvent, PlayerId, RawClientMessageEvent, RegionBounds, RegionId,
    RegionStartedEvent,
    // GORC imports
    GorcManager, SubscriptionManager, MulticastManager, SpatialPartition, Position,
};
use futures::{SinkExt, StreamExt};
use plugin_system::{create_plugin_manager_with_events, PluginManager};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn, trace};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::TcpListener as StdTcpListener;

use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::accept_async;

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
    pub use_reuse_port: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:8080".parse().expect("Attempted to use ServerConfig.default(), but field `bind_address` is not parsable in the source code"),
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
            use_reuse_port: false
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
        let connection_id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let connection = ClientConnection::new(remote_addr);

        let mut connections = self.connections.write().await;
        connections.insert(connection_id, connection);

        info!("🔗 Connection {} from {}", connection_id, remote_addr);
        connection_id
    }

    async fn remove_connection(&self, connection_id: ConnectionId) {
        let mut connections = self.connections.write().await;
        if let Some(connection) = connections.remove(&connection_id) {
            info!(
                "❌ Connection {} from {} disconnected",
                connection_id, connection.remote_addr
            );
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
    horizon_event_system: Arc<EventSystem>,
    connection_manager: Arc<ConnectionManager>,
    plugin_manager: Arc<PluginManager>,
    shutdown_sender: broadcast::Sender<()>,
    region_id: RegionId,
    // GORC components
    gorc_manager: Arc<GorcManager>,
    subscription_manager: Arc<SubscriptionManager>,
    multicast_manager: Arc<MulticastManager>,
    spatial_partition: Arc<SpatialPartition>,
}

impl GameServer {
    pub fn new(config: ServerConfig) -> Self {
        let region_id = RegionId::new();
        let horizon_event_system = create_horizon_event_system();
        let connection_manager = Arc::new(ConnectionManager::new());
        let (shutdown_sender, _) = broadcast::channel(1);

        // Create plugin manager with the event system
        let plugin_manager = Arc::new(create_plugin_manager_with_events(
            horizon_event_system.clone(),
            &config.plugin_directory,
            region_id,
        ));

        // Initialize GORC components
        let gorc_manager = Arc::new(GorcManager::new());
        let subscription_manager = Arc::new(SubscriptionManager::new());
        let multicast_manager = Arc::new(MulticastManager::new());
        let spatial_partition = Arc::new(SpatialPartition::new());

        Self {
            config,
            horizon_event_system,
            connection_manager,
            plugin_manager,
            shutdown_sender,
            region_id,
            gorc_manager,
            subscription_manager,
            multicast_manager,
            spatial_partition,
        }
    }

    pub async fn start(&self) -> Result<(), ServerError> {
        info!("🚀 Starting game server on {}", self.config.bind_address);
        info!("🌍 Region ID: {}", self.region_id.0);

        // Register minimal core event handlers
        self.register_core_handlers().await?;

        info!(
            "🔌 Loading plugins from: {}",
            self.config.plugin_directory.display()
        );
        match self.plugin_manager.load_all_plugins().await {
            Ok(loaded_plugins) => {
                info!(
                    "✅ Successfully loaded {} plugins: {:?}",
                    loaded_plugins.len(),
                    loaded_plugins
                );
            }
            Err(e) => {
                warn!(
                    "⚠️ Plugin loading failed: {}. Server will continue without plugins.",
                    e
                );
            }
        }

        // Display plugin statistics
        let plugin_stats = self.plugin_manager.get_plugin_stats().await;
        info!("📊 Plugin System Status:");
        info!("  - Plugins loaded: {}", plugin_stats.total_plugins);
        info!("  - Total handlers: {}", plugin_stats.total_handlers);
        info!(
            "  - Client events routed: {}",
            plugin_stats.client_events_routed
        );

        self.horizon_event_system
            .emit_core(
                "region_started",
                &RegionStartedEvent {
                    region_id: self.region_id,
                    bounds: self.config.region_bounds.clone(),
                    timestamp: current_timestamp(),
                },
            )
            .await
            .map_err(|e| ServerError::Internal(e.to_string()))?;

        // Emit region started event (for plugins)
        self.horizon_event_system
            .emit_core(
                "region_started",
                &RegionStartedEvent {
                    region_id: self.region_id,
                    bounds: self.config.region_bounds.clone(),
                    timestamp: current_timestamp(),
                },
            )
            .await
            .map_err(|e| ServerError::Internal(e.to_string()))?;

        // Start TCP listener
        //let listener = TcpListener::bind(self.config.bind_address)
          //  .await
          //  .map_err(|e| ServerError::Network(format!("Failed to bind: {}", e)))?;

        //info!("🌐 Server listening on {}", self.config.bind_address);

        // Start TCP Listener Beta
        
        let core_count = num_cpus::get();
        let num_acceptors = if self.config.use_reuse_port {
            core_count
        } else {
            1
        };

        info!(
             "🧠 Detected {} CPU cores, using {} acceptor(s)",
                core_count, num_acceptors
        );

        let mut listeners = Vec::new();

        for i in 0..num_acceptors {
            let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))
                .map_err(|e| ServerError::Network(format!("Socket creation failed: {e}")))?;
            socket.set_reuse_address(true).ok();
            if self.config.use_reuse_port {
                #[cfg(any(
                    target_os = "linux",
                    target_os = "android",
                    target_os = "macos",
                    target_os = "freebsd",
                    target_os = "netbsd",
                    target_os = "openbsd"
                ))]
                {
                    socket.set_reuse_port(true).map_err(|e| {
                        ServerError::Network(format!("Failed to set SO_REUSEPORT: {e}"))
                    })?;
                }

                #[cfg(not(any(
                    target_os = "linux",
                    target_os = "android",
                    target_os = "macos",
                    target_os = "freebsd",
                    target_os = "netbsd",
                    target_os = "openbsd"
                )))]
                {
                    warn!("SO_REUSEPORT requested but not supported on this platform. Ignoring.");
                }
            }
            socket.bind(&self.config.bind_address.into())
                .map_err(|e| ServerError::Network(format!("Bind failed: {e}")))?;

            socket.listen(65535)
                .map_err(|e| ServerError::Network(format!("Listen failed: {e}")))?;

            let std_listener: StdTcpListener = socket.into();
            std_listener.set_nonblocking(true).ok();
            
            let listener = TcpListener::from_std(std_listener)
            .map_err(|e| ServerError::Network(format!("Tokio listener creation failed: {e}")))?;

        listeners.push(listener);
        info!("✅ Listener {} bound on {}", i, self.config.bind_address);

        }

        // Main server loop (infrastructure only)
        let mut shutdown_receiver = self.shutdown_sender.subscribe();


        // TODO: @haywood
        use futures::stream::{FuturesUnordered, StreamExt as FuturesStreamExt};
        let mut accept_futures = listeners
            .into_iter()
            .map(|listener| {
                async move {
                    loop {
                        match listener.accept().await {
                            Ok((stream, addr)) => {
                                let connection_manager = self.connection_manager.clone();
                                let horizon_event_system = self.horizon_event_system.clone();

                                tokio::spawn(async move {
                                    if let Err(e) = handle_connection(
                                        stream,
                                        addr,
                                        connection_manager,
                                        horizon_event_system,
                                    ).await {
                                        error!("Connection error: {:?}", e);
                                    }
                                });
                            }
                            Err(e) => {
                                error!("Failed to accept connection: {}", e);
                                break;
                            }
                        }
                    }
                }
            })
            .collect::<FuturesUnordered<_>>();

        tokio::select! {
            _ = accept_futures.next() => {} // Accept loop(s) will run until error or shutdown
            _ = shutdown_receiver.recv() => {
                info!("Shutdown signal received");
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

        self.horizon_event_system
            .on_core("player_connected", |event: PlayerConnectedEvent| {
                info!(
                    "👋 Player {} connected from {}",
                    event.player_id, event.remote_addr
                );
                Ok(())
            })
            .await
            .map_err(|e| ServerError::Internal(e.to_string()))?;

        self.horizon_event_system
            .on_core("player_disconnected", |event: PlayerDisconnectedEvent| {
                info!(
                    "👋 Player {} disconnected: {:?}",
                    event.player_id, event.reason
                );
                Ok(())
            })
            .await
            .map_err(|e| ServerError::Internal(e.to_string()))?;

        self.horizon_event_system
            .on_core("region_started", |event: RegionStartedEvent| {
                info!(
                    "🌍 Region {:?} started with bounds: {:?}",
                    event.region_id, event.bounds
                );
                Ok(())
            })
            .await
            .map_err(|e| ServerError::Internal(e.to_string()))?;

        Ok(())
    }

    pub async fn shutdown(&self) -> Result<(), ServerError> {
        info!("🛑 Shutting down server...");
        let _ = self.shutdown_sender.send(());
        Ok(())
    }

    pub fn get_horizon_event_system(&self) -> Arc<EventSystem> {
        self.horizon_event_system.clone()
    }

    /// Gets the GORC manager for replication channel management
    pub fn get_gorc_manager(&self) -> Arc<GorcManager> {
        self.gorc_manager.clone()
    }

    /// Gets the subscription manager for dynamic subscription handling
    pub fn get_subscription_manager(&self) -> Arc<SubscriptionManager> {
        self.subscription_manager.clone()
    }

    /// Gets the multicast manager for efficient group communication
    pub fn get_multicast_manager(&self) -> Arc<MulticastManager> {
        self.multicast_manager.clone()
    }

    /// Gets the spatial partition for spatial queries and region management
    pub fn get_spatial_partition(&self) -> Arc<SpatialPartition> {
        self.spatial_partition.clone()
    }
}

// ============================================================================
// Minimal Connection Handling (No Game Logic)
// ============================================================================

async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    connection_manager: Arc<ConnectionManager>,
    horizon_event_system: Arc<EventSystem>,
) -> Result<(), ServerError> {
    let ws_stream = accept_async(stream)
        .await
        .map_err(|e| ServerError::Network(format!("WebSocket handshake failed: {e}")))?;

    let (ws_sender, mut ws_receiver) = ws_stream.split();
    let ws_sender = Arc::new(tokio::sync::Mutex::new(ws_sender));
    let connection_id = connection_manager.add_connection(addr).await;

    // Generate player ID and emit connection event
    let player_id = PlayerId::new();
    connection_manager
        .set_player_id(connection_id, player_id)
        .await;

    // Emit core infrastructure event
    horizon_event_system
        .emit_core(
            "player_connected",
            &PlayerConnectedEvent {
                player_id,
                connection_id: connection_id.to_string(),
                remote_addr: addr.to_string(),
                timestamp: current_timestamp(),
            },
        )
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    let mut message_receiver = connection_manager.subscribe();
    let ws_sender_incoming = ws_sender.clone();
    let ws_sender_outgoing = ws_sender.clone();

    // Incoming message task - routes raw messages to plugins
    let incoming_task = {
        let connection_manager = connection_manager.clone();
        let horizon_event_system = horizon_event_system.clone();

        async move {
            while let Some(msg) = ws_receiver.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        // Route raw message to plugins via events
                        if let Err(e) = route_client_message(
                            &text,
                            connection_id,
                            &connection_manager,
                            &horizon_event_system,
                        )
                        .await
                        {
                            trace!("❌ Message routing error: {}", e);
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
                    if let Err(e) = ws_sender
                        .send(Message::Text(message_text.to_string().into()))
                        .await
                    {
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
        horizon_event_system
            .emit_core(
                "player_disconnected",
                &PlayerDisconnectedEvent {
                    player_id,
                    connection_id: connection_id.to_string(),
                    reason: DisconnectReason::ClientDisconnect,
                    timestamp: current_timestamp(),
                },
            )
            .await
            .map_err(|e| ServerError::Internal(e.to_string()))?;
    }

    connection_manager.remove_connection(connection_id).await;
    Ok(())
}

/// Route raw client message to plugins (no game logic in core!)
async fn route_client_message(
    text: &str,
    connection_id: ConnectionId,
    connection_manager: &ConnectionManager,
    horizon_event_system: &EventSystem,
) -> Result<(), ServerError> {
    // Parse as generic message structure
    let message: ClientMessage = serde_json::from_str(text)
        .map_err(|e| ServerError::Network(format!("Invalid JSON: {e}")))?;

    let player_id = connection_manager
        .get_player_id(connection_id)
        .await
        .ok_or_else(|| ServerError::Internal("Player not found".to_string()))?;

    debug!(
        "📨 Routing message to namespace '{}' event '{}' from player {}",
        message.namespace, message.event, player_id
    );

    // Create raw message event for plugins to handle
    let raw_event = RawClientMessageEvent {
        player_id,
        message_type: format!("{}:{}", &message.namespace, &message.event),
        data: message.data.to_string().into_bytes(),
        timestamp: current_timestamp(),
    };

    // Emit to core for routing (plugins will listen to this)
    horizon_event_system
        .emit_core("raw_client_message", &raw_event)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    // Generic routing using client-specified namespace and event
    horizon_event_system
        .emit_client(&message.namespace, &message.event, &message.data)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    info!(
        "✅ Routed '{}:{}' message from player {} to plugins",
        message.namespace, message.event, player_id
    );
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
        let events = server.get_horizon_event_system();

        // Verify we can register core handlers
        events
            .on_core("test_event", |event: serde_json::Value| {
                println!("Test core event: {:?}", event);
                Ok(())
            })
            .await
            .expect("Failed to register core event handler");

        // Emit a test event
        events
            .emit_core(
                "test_event",
                &serde_json::json!({
                    "test": "data"
                }),
            )
            .await
            .expect("Failed to register core event handler");
    }

    #[tokio::test]
    async fn test_plugin_message_routing() {
        let server = create_server();
        let events = server.get_horizon_event_system();

        // Register handlers that plugins would register
        events
            .on_client("movement", "move_request", |event: serde_json::Value| {
                println!("Movement plugin would handle: {:?}", event);
                Ok(())
            })
            .await
            .expect("Failed to register movement handler");

        events
            .on_client("chat", "send_message", |event: serde_json::Value| {
                println!("Chat plugin would handle: {:?}", event);
                Ok(())
            })
            .await
            .expect("Failed to register chat handler");

        // Test routing
        events
            .emit_client(
                "movement",
                "move_request",
                &serde_json::json!({
                    "target_x": 100.0,
                    "target_y": 200.0,
                    "target_z": 0.0
                }),
            )
            .await
            .expect("Failed to emit client event for movement");

        events
            .emit_client(
                "chat",
                "send_message",
                &serde_json::json!({
                    "message": "Hello world!",
                    "channel": "general"
                }),
            )
            .await
            .expect("Failed to emit client event for chat");
        }

    #[tokio::test]
    async fn test_generic_client_message_routing() {
        let server = create_server();
        let events = server.get_horizon_event_system();

        // Register handlers for different namespaces/events
        events
            .on_client("movement", "jump", |event: serde_json::Value| {
                println!("Movement plugin handles jump: {:?}", event);
                Ok(())
            })
            .await
            .expect("Failed to register movement handler");

        events
            .on_client("inventory", "use_item", |event: serde_json::Value| {
                println!("Inventory plugin handles use_item: {:?}", event);
                Ok(())
            })
            .await
            .expect("Failed to register inventory handler");

        events
            .on_client(
                "custom_plugin",
                "custom_event",
                |event: serde_json::Value| {
                    println!("Custom plugin handles custom_event: {:?}", event);
                    Ok(())
                },
            )
            .await
            .expect("Failed to register custom event handler");

        // Test the new generic routing
        events
            .emit_client(
                "movement",
                "jump",
                &serde_json::json!({
                    "height": 5.0
                }),
            )
            .await
            .expect("Failed to emit client event for movement");

        events
            .emit_client(
                "inventory",
                "use_item",
                &serde_json::json!({
                    "item_id": "potion_health",
                    "quantity": 1
                }),
            )
            .await
            .expect("Failed to emit client event for inventory");

        events
            .emit_client(
                "custom_plugin",
                "custom_event",
                &serde_json::json!({
                    "custom_data": "anything"
                }),
            )
            .await
            .expect("Failed to emit client event for custom plugin");

        println!("✅ All messages routed generically without hardcoded logic!");
    }

    /// Example of how clean the new server is - just infrastructure!
    #[tokio::test]
    async fn demonstrate_clean_separation() {
        let server = create_server();
        let events = server.get_horizon_event_system();

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
        events
            .on_core("player_connected", |event: PlayerConnectedEvent| {
                println!("✅ Core: Player {} connected", event.player_id);
                Ok(())
            })
            .await
            .expect("Failed to register core player connected handler");

        // This would be handled by movement plugin, not core
        events
            .on_client("movement", "jump", |event: serde_json::Value| {
                println!("🦘 Movement Plugin: Jump event {:?}", event);
                Ok(())
            })
            .await
            .expect("Failed to register movement handler");

        println!("✨ Clean separation achieved with generic routing!");
    }

    #[tokio::test]
    async fn test_gorc_integration() {
        let server = create_server();
        
        // Test GORC component accessibility
        let gorc_manager = server.get_gorc_manager();
        let subscription_manager = server.get_subscription_manager();
        let multicast_manager = server.get_multicast_manager();
        let spatial_partition = server.get_spatial_partition();
        
        // Test basic GORC functionality
        let stats = gorc_manager.get_stats().await;
        assert_eq!(stats.total_subscriptions, 0);
        
        // Test spatial partition
        spatial_partition.add_region(
            "test_region".to_string(),
            RegionBounds {
                min_x: 0.0, max_x: 1000.0,
                min_y: 0.0, max_y: 1000.0,
                min_z: 0.0, max_z: 1000.0,
            },
        ).await;
        
        // Test subscription management
        let player_id = PlayerId::new();
        let position = Position::new(100.0, 100.0, 100.0);
        subscription_manager.add_player(player_id, position).await;
        
        // Test multicast group creation
        use std::collections::HashSet;
        let channels: HashSet<u8> = vec![0, 1].into_iter().collect();
        let group_id = multicast_manager.create_group(
            "test_group".to_string(),
            channels,
            horizon_event_system::ReplicationPriority::Normal,
        ).await;
        
        // Add player to multicast group
        let added = multicast_manager.add_player_to_group(player_id, group_id).await;
        assert!(added);
        
        println!("✅ GORC integration test passed!");
        println!("  - GORC Manager: Initialized with default channels");
        println!("  - Subscription Manager: Player subscription system ready");
        println!("  - Multicast Manager: Group creation and player management working");
        println!("  - Spatial Partition: Region management and spatial queries available");
    }
}
