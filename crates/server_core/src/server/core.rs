//! Core game server implementation with callback-based event system
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

/// Main game server that manages plugins, players, and events using callback dispatch
/// 
/// The GameServer orchestrates all server components including:
/// - Player and connection management
/// - Plugin system integration with callback-based events
/// - Event processing and direct dispatch to plugins
/// - Regional boundaries enforcement
pub struct GameServer {
    /// Unique identifier for this server region
    pub region_id: RegionId,
    /// Spatial boundaries for this region
    pub region_bounds: RegionBounds,
    /// Active players in this region
    pub players: Arc<DashMap<PlayerId, Player>>,
    /// Plugin system manager with callback registration
    pub plugin_loader: PluginLoader,
    /// Connection management system
    pub connection_manager: Arc<ConnectionManager>,
    /// Callback-based event processing system
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
        
        // Create callback-based event processing system
        let event_processor = Arc::new(EventProcessor::new());
        
        // Create connection manager
        let connection_manager = Arc::new(ConnectionManager::new());
        
        // Create plugin loader with callback support
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
    /// Plugins are automatically registered with the callback-based event system.
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
    /// 1. Starts the callback-based event processing system
    /// 2. Binds to the specified address
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
        
        // Start the callback-based event processing system FIRST
        info!("Starting callback-based event processor...");
        self.event_processor.start().await;
        
        let listener = TcpListener::bind(addr).await
            .map_err(|e| ServerError::Network(format!("Failed to bind to {}: {}", addr, e)))?;
        
        info!("Game server starting on {}", addr);
        info!("Region ID: {:?}", self.region_id);
        info!("Region bounds: {:?}", self.region_bounds);
        info!("Event system: Callback-based dispatch");
        
        // Log event system statistics
        let event_stats = self.plugin_loader.get_event_stats().await;
        info!("Event system stats: {} registered events, {} total callbacks", 
              event_stats.registered_events, event_stats.total_callbacks);
        
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
    /// 2. Shuts down all loaded plugins (unregisters callbacks)
    /// 3. Stops the callback-based event processing system
    /// 4. Closes all active connections
    /// 
    /// # Returns
    /// Result indicating shutdown success or any errors encountered
    pub async fn shutdown(&self) -> Result<(), ServerError> {
        info!("Shutting down game server with callback-based events...");
        
        // Signal shutdown to all components
        let _ = self.shutdown_signal.send(true);
        
        // Shutdown plugins first (this unregisters their callbacks)
        self.plugin_loader.shutdown_all().await?;
        
        // Shutdown callback-based event processor
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

    /// Get the callback-based event processor
    pub fn get_event_processor(&self) -> Arc<EventProcessor> {
        self.event_processor.clone()
    }
    
    /// Get comprehensive server statistics
    /// 
    /// # Returns
    /// Statistics about server state including players, connections, and events
    pub async fn get_server_stats(&self) -> ServerStats {
        let event_stats = self.plugin_loader.get_event_stats().await;
        
        ServerStats {
            region_id: self.region_id,
            player_count: self.players.len(),
            connection_count: self.connection_manager.connection_count(),
            plugin_count: self.plugin_loader.plugin_count().await,
            registered_events: event_stats.registered_events,
            total_callbacks: event_stats.total_callbacks,
            event_details: event_stats.callback_details,
        }
    }
    
    /// Check if a specific plugin is loaded
    /// 
    /// # Arguments
    /// * `plugin_name` - Name of the plugin to check
    /// 
    /// # Returns
    /// True if the plugin is loaded and registered for callbacks
    pub async fn is_plugin_loaded(&self, plugin_name: &str) -> bool {
        self.plugin_loader.is_plugin_loaded(plugin_name).await
    }
    
    /// Emit a core event through the server's event system
    /// 
    /// This is a convenience method for emitting core events directly from the server.
    /// 
    /// # Arguments
    /// * `event` - Core event to emit
    /// 
    /// # Returns
    /// Result indicating success or failure
    pub async fn emit_core_event(&self, event: CoreEvent) -> Result<(), ServerError> {
        let event_id = EventId::new(EventNamespace::new("core"), event.event_type());
        let event_arc: Arc<dyn GameEvent + Send + Sync> = Arc::new(event);
        self.event_processor.emit_event(event_id, event_arc).await
    }
}

/// Comprehensive server statistics
#[derive(Debug, Clone)]
pub struct ServerStats {
    pub region_id: RegionId,
    pub player_count: usize,
    pub connection_count: usize,
    pub plugin_count: usize,
    pub registered_events: usize,
    pub total_callbacks: usize,
    pub event_details: std::collections::HashMap<String, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_types::{Position, RegionBounds};
    use std::time::Duration;
    use tokio::time::timeout;
    
    #[tokio::test]
    async fn test_server_creation_with_callbacks() {
        let region_bounds = RegionBounds {
            min_x: -100.0,
            max_x: 100.0,
            min_y: -100.0,
            max_y: 100.0,
            min_z: -10.0,
            max_z: 10.0,
        };
        
        let server = GameServer::new(region_bounds);
        
        // Verify initial state
        assert_eq!(server.players.len(), 0);
        
        // Test event processor is callback-based
        let stats = server.get_server_stats().await;
        assert_eq!(stats.player_count, 0);
        assert_eq!(stats.plugin_count, 0);
        assert_eq!(stats.registered_events, 0);
        assert_eq!(stats.total_callbacks, 0);
    }
    
    #[tokio::test]
    async fn test_server_startup_shutdown_callbacks() {
        let region_bounds = RegionBounds {
            min_x: -100.0,
            max_x: 100.0,
            min_y: -100.0,
            max_y: 100.0,
            min_z: -10.0,
            max_z: 10.0,
        };
        
        let server = GameServer::new(region_bounds);
        
        // Test that server can be shut down cleanly
        let shutdown_result = timeout(Duration::from_millis(100), server.shutdown()).await;
        assert!(shutdown_result.is_ok());
    }
    
    #[tokio::test]
    async fn test_core_event_emission() {
        let region_bounds = RegionBounds {
            min_x: -100.0,
            max_x: 100.0,
            min_y: -100.0,
            max_y: 100.0,
            min_z: -10.0,
            max_z: 10.0,
        };
        
        let server = GameServer::new(region_bounds);
        
        // Start event processor
        server.event_processor.start().await;
        
        // Test emitting a core event
        let test_event = CoreEvent::CustomMessage {
            data: serde_json::json!({
                "test": "callback_event_system"
            })
        };
        
        let result = server.emit_core_event(test_event).await;
        assert!(result.is_ok());
    }
}