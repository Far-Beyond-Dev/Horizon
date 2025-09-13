//! Core game server implementation.
//!
//! This module contains the main `GameServer` struct and its implementation,
//! providing the central orchestration of all server components including
//! event systems, plugin management, and GORC infrastructure.

use crate::{
    config::ServerConfig,
    connection::{ConnectionManager, GameServerResponseSender},
    error::ServerError,
    server::handlers::handle_connection,
};
use plugin_system::PluginManager;
use futures::stream::{FuturesUnordered, StreamExt as FuturesStreamExt};
use horizon_event_system::{
    current_timestamp, EventSystem, GorcManager, MulticastManager,
    PlayerConnectedEvent, PlayerDisconnectedEvent, RegionId, RegionStartedEvent, SpatialPartition,
    SubscriptionManager, AuthenticationStatusSetEvent, AuthenticationStatusGetEvent, 
    AuthenticationStatusGetResponseEvent, AuthenticationStatusChangedEvent, ShutdownState,
};
use horizon_sockets::SocketBuilder;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::time::{interval, Duration};
use tracing::{error, info, trace, warn};
use bug::bug_with_handle;

#[cfg(any(target_os = "linux", target_os = "android", target_os = "freebsd", target_os = "openbsd", target_os = "netbsd", target_os = "dragonfly", target_os = "macos"))]
use std::os::fd::AsRawFd;

/// The core game server structure.
/// 
/// `GameServer` orchestrates all server components including networking,
/// event processing, plugin management, and GORC (Game Object Replication Channel)
/// infrastructure. It provides a clean infrastructure-only foundation that
/// delegates all game logic to plugins.
/// 
/// # Architecture
/// 
/// * **Event System**: Central hub for all plugin communication
/// * **Connection Management**: WebSocket connection lifecycle and player mapping
/// * **Plugin System**: Dynamic loading and management of game logic plugins
/// * **GORC Components**: Advanced replication and spatial management
/// * **Multi-threaded Networking**: Configurable accept loop scaling
/// 
/// # Design Philosophy
/// 
/// The server core contains NO game logic - it only provides infrastructure.
/// All game mechanics, rules, and behaviors are implemented as plugins that
/// communicate through the event system.
pub struct GameServer {
    /// Server configuration settings
    config: ServerConfig,
    
    /// The event system for plugin communication
    horizon_event_system: Arc<EventSystem>,
    
    /// Manager for client connections and messaging
    connection_manager: Arc<ConnectionManager>,
    
    /// Manager for loading and managing plugins
    plugin_manager: Arc<PluginManager>,
    
    /// Channel for coordinating server shutdown
    shutdown_sender: broadcast::Sender<()>,
    
    /// Unique identifier for this server region
    region_id: RegionId,
    
    // GORC (Game Object Replication Channel) components
    /// Main GORC manager for replication channels
    gorc_manager: Arc<GorcManager>,
    
    /// Manager for dynamic subscription handling
    subscription_manager: Arc<SubscriptionManager>,
    
    /// Manager for efficient group communication
    multicast_manager: Arc<MulticastManager>,
    
    /// Spatial partitioning for region and proximity queries
    spatial_partition: Arc<SpatialPartition>,
}

impl GameServer {
    /// Creates a new game server with the specified configuration.
    /// 
    /// Initializes all core components including the event system, connection
    /// management, plugin system, and GORC infrastructure. The server is
    /// ready to start after construction.
    /// 
    /// # Arguments
    /// 
    /// * `config` - Configuration parameters for server behavior
    /// 
    /// # Returns
    /// 
    /// A new `GameServer` instance ready to be started.
    /// 
    /// # Component Initialization
    /// 
    /// 1. Creates event system and connection manager
    /// 2. Sets up client response sender for event system integration
    /// 3. Initializes plugin manager with event system binding
    /// 4. Creates all GORC components for advanced networking
    /// 5. Generates unique region ID for this server instance
    pub fn new(config: ServerConfig) -> Self {
    let region_id = RegionId::new();
    use horizon_event_system::gorc::instance::GorcInstanceManager;
    let gorc_instance_manager = Arc::new(GorcInstanceManager::new());
    let mut horizon_event_system = Arc::new(EventSystem::with_gorc(gorc_instance_manager.clone()));
        let connection_manager = Arc::new(ConnectionManager::new());
        let (shutdown_sender, _) = broadcast::channel(1);

        // Set up connection-aware response sender
        let response_sender = Arc::new(GameServerResponseSender::new(connection_manager.clone()));
        if let Some(event_system_mut) = Arc::get_mut(&mut horizon_event_system) {
            event_system_mut.set_client_response_sender(response_sender);
        } else {
            bug_with_handle!(horizon_bugs::get_bugs(), "crash", {
                error_type = "⚠️ Failed to get mutable reference to event system during initialization",
                function = "GameServer::new",
                line = "117",
                os = std::env::consts::OS
            });
        }

        // Initialize plugin manager with safety configuration and GORC support
        let plugin_manager = Arc::new(PluginManager::with_gorc(horizon_event_system.clone(), config.plugin_safety.clone(), gorc_instance_manager.clone()));

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

    /// Starts the game server and begins accepting connections with graceful shutdown support.
    /// 
    /// This method performs the complete server startup sequence including
    /// plugin loading, event handler registration, network binding, and
    /// the main accept loop. The server runs until shutdown is requested through
    /// the provided shutdown state.
    /// 
    /// # Arguments
    /// 
    /// * `shutdown_state` - Shared shutdown state for coordinating graceful shutdown
    /// 
    /// # Startup Sequence
    /// 
    /// 1. Register core infrastructure event handlers
    /// 2. Load and initialize all plugins from the plugin directory
    /// 3. Emit region started event to notify plugins
    /// 4. Create TCP listeners (potentially multiple for multi-threading)
    /// 5. Start accept loops to handle incoming connections
    /// 6. Monitor shutdown state and gracefully halt when shutdown is initiated
    /// 7. Clean shutdown of all plugins
    /// 
    /// # Multi-threading
    /// 
    /// If `use_reuse_port` is enabled in configuration, the server will
    /// create multiple accept loops equal to the number of CPU cores for
    /// improved performance under high load.
    /// 
    /// # Returns
    /// 
    /// `Ok(())` if the server started and stopped cleanly, or a `ServerError`
    /// if there was a failure during startup or operation.
    pub async fn start_with_shutdown_state(&self, shutdown_state: ShutdownState) -> Result<(), ServerError> {
        self.start_internal(Some(shutdown_state)).await
    }

    /// Starts the game server and begins accepting connections.
    /// 
    /// This method performs the complete server startup sequence including
    /// plugin loading, event handler registration, network binding, and
    /// the main accept loop. The server runs until shutdown is requested.
    /// 
    /// # Startup Sequence
    /// 
    /// 1. Register core infrastructure event handlers
    /// 2. Load and initialize all plugins from the plugin directory
    /// 3. Emit region started event to notify plugins
    /// 4. Create TCP listeners (potentially multiple for multi-threading)
    /// 5. Start accept loops to handle incoming connections
    /// 6. Run until shutdown signal received
    /// 7. Clean shutdown of all plugins
    /// 
    /// # Multi-threading
    /// 
    /// If `use_reuse_port` is enabled in configuration, the server will
    /// create multiple accept loops equal to the number of CPU cores for
    /// improved performance under high load.
    /// 
    /// # Returns
    /// 
    /// `Ok(())` if the server started and stopped cleanly, or a `ServerError`
    /// if there was a failure during startup or operation.
    pub async fn start(&self) -> Result<(), ServerError> {
        self.start_internal(None).await
    }

    /// Internal method for starting the server with optional shutdown state.
    async fn start_internal(&self, shutdown_state: Option<ShutdownState>) -> Result<(), ServerError> {
        info!("🚀 Starting game server on {}", self.config.bind_address);
        info!("🌍 Region ID: {}", self.region_id.0);

        info!("🔧 Runtime handle configured for async handlers");

        // Register minimal core event handlers
        self.register_core_handlers().await?;

        // Load and initialize plugins
        info!("🔌 Loading plugins from: {}", self.config.plugin_directory.display());
        if let Err(e) = self.plugin_manager.load_plugins_from_directory(&self.config.plugin_directory).await {
            error!("Failed to load plugins: {}", e);
            return Err(ServerError::Internal(format!("Plugin loading failed: {}", e)));
        }

        let plugin_count = self.plugin_manager.plugin_count();
        if plugin_count > 0 {
            info!("🎉 Successfully loaded {} plugin(s): {:?}", 
                  plugin_count, self.plugin_manager.plugin_names());
        } else {
            info!("📭 No plugins loaded");
        }

        // Start server tick if configured
        if self.config.tick_interval_ms > 0 {
            self.start_server_tick_with_shutdown(shutdown_state.clone()).await;
            info!("🕒 Server tick started with interval: {}ms", self.config.tick_interval_ms);
        } else {
            info!("⏸️ Server tick disabled (interval: 0ms)");
        }

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


        // Unified listener creation logic for all platforms
        let core_count = num_cpus::get();
        let use_reuse_port = self.config.use_reuse_port;
        let num_acceptors = if use_reuse_port { core_count } else { 1 };
        info!("🧠 Detected {} CPU cores, using {} acceptor(s)", core_count, num_acceptors);

        // Try to create multiple listeners, but if any fail, fall back to one listener
        let mut listeners = Vec::new();
        let mut multi_listener_error = None;
        for i in 0..num_acceptors {
            let mut builder = match SocketBuilder::new().bind(self.config.bind_address.to_string()) {
                Ok(b) => b,
                Err(e) => {
                    multi_listener_error = Some(format!("SocketBuilder bind failed: {e}"));
                    break;
                }
            };
            if use_reuse_port {
                match builder.reuse_port(true) {
                    Ok(b) => { builder = b; },
                    Err(e) => {
                        multi_listener_error = Some(format!("SO_REUSEPORT failed: {e}"));
                        break;
                    }
                }
            }
            builder = match builder.backlog(65535) {
                Ok(b) => b,
                Err(e) => {
                    multi_listener_error = Some(format!("SocketBuilder backlog failed: {e}"));
                    break;
                }
            };
            let listener = match builder.tcp_listener() {
                Ok(l) => l,
                Err(e) => {
                    multi_listener_error = Some(format!("TcpListener creation failed: {e}"));
                    break;
                }
            };
            let std_listener = match listener.as_std().try_clone() {
                Ok(sl) => sl,
                Err(e) => {
                    multi_listener_error = Some(format!("Failed to clone std TcpListener: {e}"));
                    break;
                }
            };
            std_listener.set_nonblocking(true).ok();
            let tokio_listener = match tokio::net::TcpListener::from_std(std_listener) {
                Ok(tl) => tl,
                Err(e) => {
                    multi_listener_error = Some(format!("Tokio listener creation failed: {e}"));
                    break;
                }
            };
            listeners.push(tokio_listener);
            trace!("✅ Listener {} bound on {}", i, self.config.bind_address);
        }

        // If any error occurred, fall back to single listener
        if multi_listener_error.is_some() {
            warn!("Multi-listener creation failed: {}. Falling back to single listener with many acceptors.", multi_listener_error.unwrap());
            listeners.clear();
            let mut builder = SocketBuilder::new()
                .bind(self.config.bind_address.to_string())
                .map_err(|e| ServerError::Network(format!("SocketBuilder bind failed: {e}")))?;
            builder = builder.backlog(65535)
                .map_err(|e| ServerError::Network(format!("SocketBuilder backlog failed: {e}")))?;
            let listener = builder.tcp_listener()
                .map_err(|e| ServerError::Network(format!("TcpListener creation failed: {e}")))?;
            let std_listener = listener.as_std().try_clone()
                .map_err(|e| ServerError::Network(format!("Failed to clone std TcpListener: {e}")))?;
            std_listener.set_nonblocking(true).ok();
            let tokio_listener = tokio::net::TcpListener::from_std(std_listener)
                .map_err(|e| ServerError::Network(format!("Tokio listener creation failed: {e}")))?;
            listeners.push(tokio_listener);
            info!("Fallback: Single listener bound on {}", self.config.bind_address);
        }

        // Main server accept loops
        let mut shutdown_receiver = self.shutdown_sender.subscribe();

        // Create futures for all accept loops with shutdown monitoring
        let mut accept_futures = listeners
            .into_iter()
            .map(|listener| {
                let connection_manager = self.connection_manager.clone();
                let horizon_event_system = self.horizon_event_system.clone();
                let shutdown_state_clone = shutdown_state.clone();
                
                async move {
                    loop {
                        // Check if shutdown has been initiated
                        if let Some(ref shutdown_state) = shutdown_state_clone {
                            if shutdown_state.is_shutdown_initiated() {
                                info!("🛑 Accept loop stopping - shutdown initiated");
                                break;
                            }
                        }

                        match listener.accept().await {
                            Ok((stream, addr)) => {
                                let connection_manager = connection_manager.clone();
                                let horizon_event_system = horizon_event_system.clone();

                                // Spawn individual connection handler
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

        // Run until shutdown is initiated or internal shutdown signal
        tokio::select! {
            _ = accept_futures.next() => {} // Accept loop(s) will run until error or shutdown
            _ = shutdown_receiver.recv() => {
                info!("Internal shutdown signal received");
            }
        }

        // Server shutdown cleanup
        info!("🧹 Performing server cleanup...");
        
        // Note: Plugin shutdown is now handled by the application layer
        // to ensure it happens even if the server task times out
        
        info!("✅ Server cleanup completed");

        info!("Server stopped");
        Ok(())
    }

    /// Registers core infrastructure event handlers.
    /// 
    /// Sets up handlers for essential server events like player connections,
    /// disconnections, and region management. These handlers provide logging
    /// and basic infrastructure functionality only - no game logic.
    /// 
    /// # Returns
    /// 
    /// `Ok(())` if all handlers were registered successfully, or a `ServerError`
    /// if registration failed.
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

        // Register authentication status management handlers
        let connection_manager_for_set = self.connection_manager.clone();
        let horizon_event_system_for_set = self.horizon_event_system.clone();
        self.horizon_event_system
            .on_core_async("auth_status_set", move |event: AuthenticationStatusSetEvent| {
                let conn_mgr = connection_manager_for_set.clone();
                let event_system = horizon_event_system_for_set.clone();
                
                // Use block_on to execute async code in sync handler
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.block_on(async move {
                        // Get old status before setting new one
                        let old_status = conn_mgr.get_auth_status_by_player(event.player_id).await;
                        
                        let success = conn_mgr.set_auth_status_by_player(event.player_id, event.status).await;
                        if success {
                            info!("🔐 Updated auth status for player {} to {:?}", event.player_id, event.status);
                            
                            // Emit status changed event if status actually changed
                            if let Some(old_status) = old_status {
                                if old_status != event.status {
                                    let auth_status_changed_event = AuthenticationStatusChangedEvent {
                                        player_id: event.player_id,
                                        old_status,
                                        new_status: event.status,
                                        timestamp: current_timestamp(),
                                    };
                                    if let Err(e) = event_system.emit_core("auth_status_changed", &auth_status_changed_event).await {
                                        warn!("⚠️ Failed to emit auth status changed event for player {}: {:?}", event.player_id, e);
                                    }
                                }
                            }
                        } else {
                            warn!("⚠️ Failed to update auth status for player {} - player not found", event.player_id);
                        }
                    });
                }
                Ok(())
            })
            .await
            .map_err(|e| ServerError::Internal(e.to_string()))?;

        let connection_manager_for_get = self.connection_manager.clone();
        let horizon_event_system_for_get = self.horizon_event_system.clone();
        self.horizon_event_system
            .on_core_async("auth_status_get", move |event: AuthenticationStatusGetEvent| {
                let conn_mgr = connection_manager_for_get.clone();
                let event_system = horizon_event_system_for_get.clone();
                
                // Use block_on to execute async code in sync handler
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.block_on(async move {
                        let status = conn_mgr.get_auth_status_by_player(event.player_id).await;
                        
                        // Emit response event with the queried status
                        let response_event = AuthenticationStatusGetResponseEvent {
                            player_id: event.player_id,
                            request_id: event.request_id.clone(),
                            status,
                            timestamp: current_timestamp(),
                        };
                        
                        if let Err(e) = event_system.emit_core("auth_status_get_response", &response_event).await {
                            warn!("⚠️ Failed to emit auth status response for player {} request {}: {:?}", 
                                  event.player_id, event.request_id, e);
                        } else {
                            info!("🔍 Auth status query response for player {}: {:?} (request: {})", 
                                  event.player_id, status, event.request_id);
                        }
                    });
                }
                Ok(())
            })
            .await
            .map_err(|e| ServerError::Internal(e.to_string()))?;

        self.horizon_event_system
            .on_core("auth_status_changed", |event: AuthenticationStatusChangedEvent| {
                info!(
                    "🔄 Player {} auth status changed: {:?} -> {:?}",
                    event.player_id, event.old_status, event.new_status
                );
                Ok(())
            })
            .await
            .map_err(|e| ServerError::Internal(e.to_string()))?;


        // Register a simple ping handler for testing validity of the client connection
        self.horizon_event_system
            .on_client("system", "ping", |data: serde_json::Value, player_id: horizon_event_system::PlayerId, conn| {
                info!("🔧 GameServer: Received 'ping' event with connection: {:?}, data: {:?}", conn, data);

                let response = serde_json::json!({
                    "timestamp": current_timestamp(),
                    "message": "pong",
                });

                debug!("🔧 GameServer: Responding to 'ping' event with response: {:?}", response);

                // Use block_on to execute async response in sync handler
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.block_on(async {
                        let response_bytes = match serde_json::to_vec(&response) {
                            Ok(bytes) => bytes,
                            Err(e) => {
                                error!("Failed to serialize ping response: {}", e);
                                return;
                            }
                        };

                        if let Err(e) = conn.respond(&response_bytes).await {
                            error!("Failed to send ping response: {}", e);
                        }
                    });
                }

                Ok(())
        }).await.map_err(|e| ServerError::Internal(e.to_string()))?;

        Ok(())
    }

    /// Starts the server tick loop that emits periodic tick events with shutdown support.
    /// 
    /// Creates a background task that emits `server_tick` events at the configured
    /// interval. This allows plugins and other components to perform periodic
    /// operations like game state updates, cleanup, or maintenance tasks.
    /// 
    /// The tick system monitors the shutdown state and gracefully stops when
    /// shutdown is initiated, ensuring no new tick events are processed.
    /// 
    /// # Arguments
    /// 
    /// * `shutdown_state` - Optional shutdown state for coordinated shutdown
    async fn start_server_tick_with_shutdown(&self, shutdown_state: Option<ShutdownState>) {
        if self.config.tick_interval_ms == 0 {
            return; // Tick disabled
        }

        let event_system = self.horizon_event_system.clone();
        let tick_interval = Duration::from_millis(self.config.tick_interval_ms);
        
        tokio::spawn(async move {
            let mut ticker = interval(tick_interval);
            let mut tick_count: u64 = 0;
            
            loop {
                // Check for shutdown before each tick
                if let Some(ref shutdown_state) = shutdown_state {
                    if shutdown_state.is_shutdown_initiated() {
                        info!("🕒 Server tick stopping - shutdown initiated");
                        break;
                    }
                }

                ticker.tick().await;
                
                // Double-check shutdown state after tick wait (in case shutdown happened during wait)
                if let Some(ref shutdown_state) = shutdown_state {
                    if shutdown_state.is_shutdown_initiated() {
                        info!("🕒 Server tick stopping - shutdown initiated during tick wait");
                        break;
                    }
                }
                
                tick_count += 1;
                
                let tick_event = serde_json::json!({
                    "tick_count": tick_count,
                    "timestamp": current_timestamp()
                });
                
                if let Err(e) = event_system.emit_core("server_tick", &tick_event).await {
                    error!("Failed to emit server_tick event: {}", e);
                    // Continue ticking even if emission fails
                }
            }
            
            info!("✅ Server tick loop completed gracefully");
        });
    }

    /// Starts the server tick loop that emits periodic tick events.
    /// 
    /// Creates a background task that emits `server_tick` events at the configured
    /// interval. This allows plugins and other components to perform periodic
    /// operations like game state updates, cleanup, or maintenance tasks.
    /// 
    /// The tick system is non-blocking and runs independently of the main
    /// server accept loops.
    #[allow(dead_code)]
    async fn start_server_tick(&self) {
        self.start_server_tick_with_shutdown(None).await;
    }

    /// Initiates server shutdown.
    /// 
    /// Signals all server components to begin graceful shutdown, including
    /// stopping accept loops and cleaning up active connections.
    /// 
    /// # Returns
    /// 
    /// `Ok(())` if the shutdown signal was sent successfully.
    pub async fn shutdown(&self) -> Result<(), ServerError> {
        info!("🛑 Shutting down server...");
        let _ = self.shutdown_sender.send(());
        Ok(())
    }

    /// Gets a reference to the event system.
    /// 
    /// Provides access to the core event system for plugins and external
    /// components that need to interact with the server's event infrastructure.
    /// 
    /// # Returns
    /// 
    /// An `Arc<EventSystem>` that can be used to register handlers and emit events.
    pub fn get_horizon_event_system(&self) -> Arc<EventSystem> {
        self.horizon_event_system.clone()
    }

    /// Gets the GORC manager for replication channel management.
    /// 
    /// # Returns
    /// 
    /// An `Arc<GorcManager>` for managing game object replication channels.
    pub fn get_gorc_manager(&self) -> Arc<GorcManager> {
        self.gorc_manager.clone()
    }

    /// Gets the subscription manager for dynamic subscription handling.
    /// 
    /// # Returns
    /// 
    /// An `Arc<SubscriptionManager>` for managing player subscriptions to game events.
    pub fn get_subscription_manager(&self) -> Arc<SubscriptionManager> {
        self.subscription_manager.clone()
    }

    /// Gets the multicast manager for efficient group communication.
    /// 
    /// # Returns
    /// 
    /// An `Arc<MulticastManager>` for managing multicast groups and broadcasting.
    pub fn get_multicast_manager(&self) -> Arc<MulticastManager> {
        self.multicast_manager.clone()
    }

    /// Gets the spatial partition for spatial queries and region management.
    /// 
    /// # Returns
    /// 
    /// An `Arc<SpatialPartition>` for spatial indexing and proximity queries.
    pub fn get_spatial_partition(&self) -> Arc<SpatialPartition> {
        self.spatial_partition.clone()
    }

    /// Gets the plugin manager for plugin lifecycle management.
    /// 
    /// # Returns
    /// 
    /// An `Arc<PluginManager>` for managing dynamic plugins.
    pub fn get_plugin_manager(&self) -> Arc<PluginManager> {
        self.plugin_manager.clone()
    }

}