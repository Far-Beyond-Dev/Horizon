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
    create_horizon_event_system, current_timestamp, EventSystem, GorcManager, MulticastManager,
    PlayerConnectedEvent, PlayerDisconnectedEvent, RegionId, RegionStartedEvent, SpatialPartition,
    SubscriptionManager, AuthenticationStatusSetEvent, AuthenticationStatusGetEvent, 
    AuthenticationStatusGetResponseEvent, AuthenticationStatusChangedEvent,
};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::TcpListener as StdTcpListener;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

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
        let mut horizon_event_system = create_horizon_event_system();
        let connection_manager = Arc::new(ConnectionManager::new());
        let (shutdown_sender, _) = broadcast::channel(1);

        // Set up connection-aware response sender
        let response_sender = Arc::new(GameServerResponseSender::new(connection_manager.clone()));
        Arc::get_mut(&mut horizon_event_system).unwrap()
            .set_client_response_sender(response_sender);

        // Initialize plugin manager
        let plugin_manager = Arc::new(PluginManager::new(horizon_event_system.clone()));

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
        info!("🚀 Starting game server on {}", self.config.bind_address);
        info!("🌍 Region ID: {}", self.region_id.0);

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
            self.start_server_tick().await;
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

        // Determine number of accept loops based on configuration
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

        // Create TCP listeners
        let mut listeners = Vec::new();

        for i in 0..num_acceptors {
            let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))
                .map_err(|e| ServerError::Network(format!("Socket creation failed: {e}")))?;
            socket.set_reuse_address(true).ok();
            
            // Enable SO_REUSEPORT if supported and configured
            if self.config.use_reuse_port {
                #[cfg(any(target_os = "linux", target_os = "android"))]
                {
                    let sockfd = socket.as_raw_fd();
                    let optval: libc::c_int = 1;
                    let ret = unsafe {
                        libc::setsockopt(
                            sockfd,
                            libc::SOL_SOCKET,
                            libc::SO_REUSEPORT,
                            &optval as *const _ as *const libc::c_void,
                            std::mem::size_of_val(&optval) as libc::socklen_t,
                        )
                    };
                    if ret != 0 {
                        warn!("Failed to set SO_REUSEPORT: {}", std::io::Error::last_os_error());
                    } else {
                        info!("SO_REUSEPORT enabled for load balancing across acceptor threads");
                    }
                }
                #[cfg(any(target_os = "freebsd", target_os = "openbsd", target_os = "netbsd", target_os = "dragonfly", target_os = "macos"))]
                {
                    use std::os::fd::AsRawFd;
                    let sockfd = socket.as_raw_fd();
                    let optval: libc::c_int = 1;
                    let ret = unsafe {
                        libc::setsockopt(
                            sockfd,
                            libc::SOL_SOCKET,
                            libc::SO_REUSEPORT,
                            &optval as *const _ as *const libc::c_void,
                            std::mem::size_of_val(&optval) as libc::socklen_t,
                        )
                    };
                    if ret != 0 {
                        warn!("Failed to set SO_REUSEPORT: {}", std::io::Error::last_os_error());
                    } else {
                        info!("SO_REUSEPORT enabled for load balancing across acceptor threads");
                    }
                }
                #[cfg(target_os = "windows")]
                {
                    warn!("SO_REUSEPORT is not supported on Windows. Using SO_REUSEADDR only.");
                }
                #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "freebsd", target_os = "openbsd", target_os = "netbsd", target_os = "dragonfly", target_os = "macos", target_os = "windows")))]
                {
                    warn!("SO_REUSEPORT support unknown for this platform");
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

        // Main server accept loops
        let mut shutdown_receiver = self.shutdown_sender.subscribe();

        // Create futures for all accept loops
        let mut accept_futures = listeners
            .into_iter()
            .map(|listener| {
                let connection_manager = self.connection_manager.clone();
                let horizon_event_system = self.horizon_event_system.clone();
                
                async move {
                    loop {
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

        // Run until shutdown
        tokio::select! {
            _ = accept_futures.next() => {} // Accept loop(s) will run until error or shutdown
            _ = shutdown_receiver.recv() => {
                info!("Shutdown signal received");
            }
        }

        // Server shutdown cleanup
        info!("🧹 Performing server cleanup...");
        
        // Shutdown plugins first
        if let Err(e) = self.plugin_manager.shutdown().await {
            error!("Plugin shutdown failed: {}", e);
        }
        
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
                async move {
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
                    Ok(())
                }
            })
            .await
            .map_err(|e| ServerError::Internal(e.to_string()))?;

        let connection_manager_for_get = self.connection_manager.clone();
        let horizon_event_system_for_get = self.horizon_event_system.clone();
        self.horizon_event_system
            .on_core_async("auth_status_get", move |event: AuthenticationStatusGetEvent| {
                let conn_mgr = connection_manager_for_get.clone();
                let event_system = horizon_event_system_for_get.clone();
                async move {
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
                    Ok(())
                }
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

        self.horizon_event_system
            .on_client_with_connection("system", "ping", |data: serde_json::Value, conn| async move {
                info!("🔧 StarsBeyondPlugin: Received 'ping' event with connection: {:?}, data: {:?}", conn, data);

                let response = serde_json::json!({
                    "timestamp": current_timestamp(),
                    "message": "pong",
                });
            
                println!("🔧 StarsBeyondPlugin: Responding to 'ping' event with response: {:?}", response);
            
                let response_bytes = serde_json::to_vec(&response)
                    .map_err(|e| horizon_event_system::EventError::HandlerExecution(format!("Failed to serialize response: {}", e)))?;
            
                conn.respond(&response_bytes).await?;
            
                Ok(())
        }).await.unwrap();

        Ok(())
    }

    /// Starts the server tick loop that emits periodic tick events.
    /// 
    /// Creates a background task that emits `server_tick` events at the configured
    /// interval. This allows plugins and other components to perform periodic
    /// operations like game state updates, cleanup, or maintenance tasks.
    /// 
    /// The tick system is non-blocking and runs independently of the main
    /// server accept loops.
    async fn start_server_tick(&self) {
        if self.config.tick_interval_ms == 0 {
            return; // Tick disabled
        }

        let event_system = self.horizon_event_system.clone();
        let tick_interval = Duration::from_millis(self.config.tick_interval_ms);
        
        tokio::spawn(async move {
            let mut ticker = interval(tick_interval);
            let mut tick_count: u64 = 0;
            
            loop {
                ticker.tick().await;
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
        });
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