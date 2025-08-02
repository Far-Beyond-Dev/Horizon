//! Core game server implementation.
//!
//! This module contains the main `GameServer` struct and its implementation,
//! providing the central orchestration of all server components including
//! the universal plugin system, event handling, and client connections.

use crate::{
    config::ServerConfig,
    connection::ConnectionManager,
    error::ServerError,
    server::handlers::handle_connection,
};
use futures::stream::{FuturesUnordered, StreamExt as FuturesStreamExt};
use universal_plugin_system::{
    EventBus, StructuredEventKey, PluginManager as UniversalPluginManager, PluginContext, PluginConfig,
    propagation::{AllEqPropagator, DefaultPropagator, CompositePropagator},
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

/// Event utility functions
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Simple shutdown state for server coordination
#[derive(Debug, Clone)]
pub struct ShutdownState {
    shutdown_initiated: Arc<std::sync::atomic::AtomicBool>,
}

impl ShutdownState {
    pub fn new() -> Self {
        Self {
            shutdown_initiated: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub fn initiate_shutdown(&self) {
        self.shutdown_initiated.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn is_shutdown_initiated(&self) -> bool {
        self.shutdown_initiated.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// The core game server structure.
/// 
/// `GameServer` orchestrates all server components including networking,
/// event processing, and plugin management using the universal plugin system.
/// It provides a clean infrastructure-only foundation that delegates all
/// game logic to plugins.
/// 
/// # Architecture
/// 
/// * **Universal Event System**: Central hub for all plugin communication
/// * **Connection Management**: WebSocket connection lifecycle and player mapping  
/// * **Universal Plugin System**: Dynamic loading and management with generic event handling
/// * **Multi-threaded Networking**: Configurable accept loop scaling
/// 
/// # Design Philosophy
/// 
/// The server core contains NO game logic - it only provides infrastructure.
/// All game mechanics, rules, and behaviors are implemented as plugins that
/// communicate through the universal event system.
pub struct GameServer {
    /// Server configuration settings
    config: ServerConfig,
    
    /// The universal event bus for plugin communication
    event_bus: Arc<EventBus<StructuredEventKey, CompositePropagator<StructuredEventKey>>>,
    
    /// Manager for client connections and messaging
    connection_manager: Arc<ConnectionManager>,
    
    /// Universal plugin manager
    plugin_manager: Arc<UniversalPluginManager<StructuredEventKey, CompositePropagator<StructuredEventKey>>>,
    
    /// Channel for coordinating server shutdown
    shutdown_sender: broadcast::Sender<()>,
    
    /// Unique identifier for this server region
    region_id: String,
}

impl GameServer {
    /// Creates a new game server with the specified configuration.
    /// 
    /// Initializes all core components including the universal event system,
    /// connection management, and plugin system. The server is ready to
    /// start after construction.
    /// 
    /// # Arguments
    /// 
    /// * `config` - Configuration parameters for server behavior
    /// 
    /// # Returns
    /// 
    /// A new `GameServer` instance ready to be started.
    pub fn new(config: ServerConfig) -> Self {
        let region_id = uuid::Uuid::new_v4().to_string();
        let connection_manager = Arc::new(ConnectionManager::new());
        let (shutdown_sender, _) = broadcast::channel(1);

        // Create composite propagator with universal capabilities
        let propagator = CompositePropagator::new_or()
            .add_propagator(Box::new(AllEqPropagator::new()))
            .add_propagator(Box::new(DefaultPropagator::new()));
        
        // Create universal event bus
        let event_bus = Arc::new(EventBus::with_propagator(propagator));
        
        // Create plugin context
        let plugin_context = PluginContext::new(event_bus.clone());        
        let plugin_context = Arc::new(plugin_context);
        
        // Create plugin config
        let plugin_config = PluginConfig::default();
        
        // Create universal plugin manager
        let plugin_manager = Arc::new(UniversalPluginManager::new(
            event_bus.clone(),
            plugin_context,
            plugin_config
        ));

        Self {
            config,
            event_bus,
            connection_manager,
            plugin_manager,
            shutdown_sender,
            region_id,
        }
    }

    /// Starts the game server and begins accepting connections with graceful shutdown support.
    pub async fn start_with_shutdown_state(&self, shutdown_state: ShutdownState) -> Result<(), ServerError> {
        self.start_internal(Some(shutdown_state)).await
    }

    /// Starts the game server and begins accepting connections.
    pub async fn start(&self) -> Result<(), ServerError> {
        self.start_internal(None).await
    }

    /// Internal method for starting the server with optional shutdown state.
    async fn start_internal(&self, shutdown_state: Option<ShutdownState>) -> Result<(), ServerError> {
        info!("🚀 Starting game server on {}", self.config.bind_address);
        info!("🌍 Region ID: {}", self.region_id);

        // Register minimal core event handlers
        self.register_core_handlers().await?;

        // Load and initialize plugins using universal system
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
        self.event_bus
            .emit("core", "region_started", &serde_json::json!({
                "region_id": self.region_id.clone(),
                "bounds": self.config.region_bounds.clone(),
                "timestamp": current_timestamp(),
            }))
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

        // Create futures for all accept loops with shutdown monitoring
        let mut accept_futures = listeners
            .into_iter()
            .map(|listener| {
                let connection_manager = self.connection_manager.clone();
                let event_bus = self.event_bus.clone();
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
                                let event_bus = event_bus.clone();

                                // Spawn individual connection handler
                                tokio::spawn(async move {
                                    if let Err(e) = handle_connection(
                                        stream,
                                        addr,
                                        connection_manager,
                                        event_bus,
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
        
        // Shutdown plugins
        if let Err(e) = self.plugin_manager.shutdown().await {
            error!("Plugin shutdown error: {}", e);
        }
        
        info!("✅ Server cleanup completed");

        info!("Server stopped");
        Ok(())
    }

    /// Registers core infrastructure event handlers.
    async fn register_core_handlers(&self) -> Result<(), ServerError> {
        // Core infrastructure events only - no game logic!

        self.event_bus
            .on("core", "player_connected", |event: serde_json::Value| {
                if let (Some(player_id), Some(remote_addr)) = (event.get("player_id"), event.get("remote_addr")) {
                    info!(
                        "👋 Player {} connected from {}",
                        player_id, remote_addr
                    );
                }
                Ok(())
            })
            .await
            .map_err(|e| ServerError::Internal(e.to_string()))?;

        self.event_bus
            .on("core", "player_disconnected", |event: serde_json::Value| {
                if let (Some(player_id), Some(reason)) = (event.get("player_id"), event.get("reason")) {
                    info!(
                        "👋 Player {} disconnected: {}",
                        player_id, reason
                    );
                }
                Ok(())
            })
            .await
            .map_err(|e| ServerError::Internal(e.to_string()))?;

        self.event_bus
            .on("core", "region_started", |event: serde_json::Value| {
                if let Some(region_id) = event.get("region_id") {
                    info!(
                        "🌍 Region {} started",
                        region_id
                    );
                }
                Ok(())
            })
            .await
            .map_err(|e| ServerError::Internal(e.to_string()))?;

        // Register a simple ping handler for testing validity of the client connection
        self.event_bus
            .on("system", "ping", |data: serde_json::Value| {
                info!("🔧 GameServer: Received 'ping' event, data: {:?}", data);

                let response = serde_json::json!({
                    "timestamp": current_timestamp(),
                    "message": "pong",
                });

                info!("🔧 GameServer: Responding to 'ping' event with response: {:?}", response);
                Ok(())
            })
            .await
            .map_err(|e| ServerError::Internal(e.to_string()))?;

        Ok(())
    }

    /// Starts the server tick loop that emits periodic tick events with shutdown support.
    async fn start_server_tick_with_shutdown(&self, shutdown_state: Option<ShutdownState>) {
        if self.config.tick_interval_ms == 0 {
            return; // Tick disabled
        }

        let event_bus = self.event_bus.clone();
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
                
                if let Err(e) = event_bus.emit("core", "server_tick", &tick_event).await {
                    error!("Failed to emit server_tick event: {}", e);
                    // Continue ticking even if emission fails
                }
            }
            
            info!("✅ Server tick loop completed gracefully");
        });
    }

    /// Initiates server shutdown.
    pub async fn shutdown(&self) -> Result<(), ServerError> {
        info!("🛑 Shutting down server...");
        let _ = self.shutdown_sender.send(());
        Ok(())
    }

    /// Gets a reference to the universal event bus.
    pub fn get_event_bus(&self) -> Arc<EventBus<StructuredEventKey, CompositePropagator<StructuredEventKey>>> {
        self.event_bus.clone()
    }

    /// Gets the universal plugin manager.
    pub fn get_plugin_manager(&self) -> Arc<UniversalPluginManager<StructuredEventKey, CompositePropagator<StructuredEventKey>>> {
        self.plugin_manager.clone()
    }
}