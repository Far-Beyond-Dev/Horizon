//! Core game server implementation
//! 
//! Contains the main GameServer struct and its primary functionality including
//! initialization, startup, shutdown, and coordination of other components.

use crate::context::ServerContextImpl;
use crate::plugin::PluginLoader;
use crate::server::{ConnectionManager, EventProcessor, MessageHandler, ServerConfig};
use dashmap::DashMap;
use shared_types::*;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info};

/// Main game server that manages plugins, players, and events
/// 
/// The GameServer orchestrates all server components including:
/// - Player and connection management
/// - Plugin system integration
/// - Event processing and distribution
/// - Regional boundaries enforcement
pub struct GameServer {
    /// Unique identifier for this server region
    pub region_id: RegionId,
    /// Spatial boundaries for this region
    pub region_bounds: RegionBounds,
    /// Active players in this region
    pub players: Arc<DashMap<PlayerId, Player>>,
    /// Plugin system manager
    pub plugin_loader: PluginLoader,
    /// Connection management system
    pub connection_manager: Arc<ConnectionManager>,
    /// Event processing system
    pub event_processor: Arc<EventProcessor>,
    /// Message handling system
    pub message_handler: Arc<MessageHandler>,
    /// Shutdown coordination signal
    pub shutdown_signal: tokio::sync::watch::Sender<bool>,
}

impl GameServer {
    /// Create a new game server for the specified region
    /// 
    /// # Arguments
    /// * `region_bounds` - Spatial boundaries for this server region
    /// 
    /// # Returns
    /// A new GameServer instance ready for plugin loading and startup
    pub fn new(region_bounds: RegionBounds) -> Self {
        let region_id = RegionId::new();
        let players = Arc::new(DashMap::new());
        let (shutdown_signal, _) = tokio::sync::watch::channel(false);
        
        // Create event processing system
        let event_processor = Arc::new(EventProcessor::new());
        
        // Create connection manager
        let connection_manager = Arc::new(ConnectionManager::new());
        
        // Create plugin loader
        let plugin_loader = PluginLoader::new(
            region_id,
            players.clone(),
            connection_manager.clone(),
            event_processor.clone(),
        );
        
        // Create message handler
        let message_handler = Arc::new(MessageHandler::new(
            players.clone(),
            connection_manager.clone(),
            event_processor.clone(),
            region_bounds.clone(),
        ));
        
        Self {
            region_id,
            region_bounds,
            players,
            plugin_loader,
            connection_manager,
            event_processor,
            message_handler,
            shutdown_signal,
        }
    }
    
    /// Load a plugin from a dynamic library
    /// 
    /// # Arguments
    /// * `library_path` - Path to the plugin dynamic library
    /// 
    /// # Returns
    /// Result indicating success or failure of plugin loading
    /// 
    /// # Errors
    /// Returns `ServerError::Plugin` if the plugin fails to load or initialize
    pub async fn load_plugin(&mut self, library_path: impl AsRef<std::path::Path>) -> Result<(), ServerError> {
        self.plugin_loader.load_plugin(library_path).await
    }
    
    /// Start the server on the specified address
    /// 
    /// This method:
    /// 1. Binds to the specified address
    /// 2. Starts the event processing system
    /// 3. Begins accepting WebSocket connections
    /// 4. Handles graceful shutdown on signal
    /// 
    /// # Arguments
    /// * `addr` - Socket address to bind to
    /// 
    /// # Returns
    /// Result indicating server startup success or failure
    /// 
    /// # Errors
    /// Returns `ServerError::Network` if binding fails
    pub async fn start(&self, addr: impl Into<SocketAddr>) -> Result<(), ServerError> {
        let addr = addr.into();
        let listener = TcpListener::bind(addr).await
            .map_err(|e| ServerError::Network(format!("Failed to bind to {}: {}", addr, e)))?;
        
        info!("Game server starting on {}", addr);
        info!("Region ID: {:?}", self.region_id);
        info!("Region bounds: {:?}", self.region_bounds);
        
        // Start event processing
        self.event_processor.start().await;
        
        // Accept connections loop
        loop {
            let mut shutdown_rx = self.shutdown_signal.subscribe();
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            info!("New connection from: {}", addr);
                            self.handle_new_connection(stream, addr).await;
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.changed() => {
                    info!("Shutdown signal received, stopping server");
                    break;
                }
            }
        }
        
        Ok(())
    }
    
    /// Handle a new incoming connection
    /// 
    /// # Arguments
    /// * `stream` - TCP stream from the new connection
    /// * `addr` - Socket address of the connecting client
    async fn handle_new_connection(&self, stream: tokio::net::TcpStream, addr: SocketAddr) {
        self.connection_manager.handle_new_connection(
            stream,
            addr,
            self.message_handler.clone(),
        ).await;
    }
    
    /// Shutdown the server gracefully
    /// 
    /// This method:
    /// 1. Signals all components to shutdown
    /// 2. Closes all active connections
    /// 3. Shuts down all loaded plugins
    /// 4. Stops the event processing system
    /// 
    /// # Returns
    /// Result indicating shutdown success or any errors encountered
    pub async fn shutdown(&self) -> Result<(), ServerError> {
        info!("Shutting down game server...");
        
        // Signal shutdown to all components
        let _ = self.shutdown_signal.send(true);
        
        // Shutdown plugins
        self.plugin_loader.shutdown_all().await?;
        
        // Shutdown event processor
        self.event_processor.shutdown().await;
        
        // Close all connections
        self.connection_manager.shutdown_all().await;
        
        info!("Game server shutdown complete");
        Ok(())
    }
    
    /// Get the region ID for this server
    pub fn region_id(&self) -> RegionId {
        self.region_id
    }
    
    /// Get the region bounds for this server
    pub fn region_bounds(&self) -> &RegionBounds {
        &self.region_bounds
    }

    pub fn get_event_processor(&self) -> Arc<EventProcessor> {
        self.event_processor.clone()
    }
}