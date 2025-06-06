//! Server context implementation for plugin interaction
//! 
//! Provides a safe interface for plugins to interact with the game server.

use crate::server::{ConnectionManager, EventProcessor};
use async_trait::async_trait;
use dashmap::DashMap;
use shared_types::*;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Implementation of ServerContext for plugins
/// 
/// Provides plugins with controlled access to server functionality including:
/// - Event emission and handling
/// - Player data access
/// - Message sending capabilities
/// - Logging services
/// - Region information
pub struct ServerContextImpl {
    /// Server region identifier
    region_id: RegionId,
    /// Active players registry
    players: Arc<DashMap<PlayerId, Player>>,
    /// Connection management system
    connection_manager: Arc<ConnectionManager>,
    /// Event processing system
    event_processor: Arc<EventProcessor>,
}

impl ServerContextImpl {
    /// Create a new server context implementation
    /// 
    /// # Arguments
    /// * `region_id` - Server region identifier
    /// * `players` - Shared player registry
    /// * `connection_manager` - Connection management system
    /// * `event_processor` - Event processing system
    pub fn new(
        region_id: RegionId,
        players: Arc<DashMap<PlayerId, Player>>,
        connection_manager: Arc<ConnectionManager>,
        event_processor: Arc<EventProcessor>,
    ) -> Self {
        Self {
            region_id,
            players,
            connection_manager,
            event_processor,
        }
    }
}

#[async_trait]
impl ServerContext for ServerContextImpl {
    /// Emit a game event to the event system
    /// 
    /// # Arguments
    /// * `namespace` - Event namespace for organization
    /// * `event` - The event data to emit
    /// 
    /// # Returns
    /// Result indicating success or failure of event emission
    async fn emit_event(&self, namespace: EventNamespace, event: Box<dyn GameEvent>) -> Result<(), ServerError> {
        let event_id = EventId::new(namespace, event.event_type());
        self.event_processor.emit_event(event_id, event.into()).await?;
        Ok(())
    }
    
    /// Get the region identifier for this server
    /// 
    /// # Returns
    /// The unique region identifier
    fn region_id(&self) -> RegionId {
        self.region_id
    }
    
    /// Get all active players in the region
    /// 
    /// # Returns
    /// Result containing a vector of all active players
    /// 
    /// # Errors
    /// Returns `ServerError` if player data cannot be accessed
    async fn get_players(&self) -> Result<Vec<Player>, ServerError> {
        Ok(self.players.iter().map(|entry| entry.value().clone()).collect())
    }
    
    /// Get a specific player by ID
    /// 
    /// # Arguments
    /// * `id` - Player identifier to look up
    /// 
    /// # Returns
    /// Result containing the player if found, or None if not found
    /// 
    /// # Errors
    /// Returns `ServerError` if player data cannot be accessed
    async fn get_player(&self, id: PlayerId) -> Result<Option<Player>, ServerError> {
        Ok(self.players.get(&id).map(|entry| entry.value().clone()))
    }
    
    /// Send a message to a specific player
    /// 
    /// # Arguments
    /// * `player_id` - Target player identifier
    /// * `message` - Message bytes to send
    /// 
    /// # Returns
    /// Result indicating success or failure of message delivery
    /// 
    /// # Errors
    /// Returns `ServerError::Network` if the message cannot be sent
    async fn send_to_player(&self, player_id: PlayerId, message: &[u8]) -> Result<(), ServerError> {
        self.connection_manager.send_to_connection(player_id, message).await
    }
    
    /// Broadcast a message to all players in the region
    /// 
    /// # Arguments
    /// * `message` - Message bytes to broadcast
    /// 
    /// # Returns
    /// Result indicating success or failure of broadcast
    /// 
    /// # Errors
    /// Returns `ServerError::Network` if the broadcast fails
    async fn broadcast_to_region(&self, message: &[u8]) -> Result<(), ServerError> {
        self.connection_manager.broadcast_to_all(message).await
    }
    
    /// Log a message with the specified level
    /// 
    /// Integrates with the server's logging system to provide consistent
    /// log formatting and routing.
    /// 
    /// # Arguments
    /// * `level` - Log level for the message
    /// * `message` - Message content to log
    fn log(&self, level: LogLevel, message: &str) {
        match level {
            LogLevel::Error => error!("{}", message),
            LogLevel::Warn => warn!("{}", message),
            LogLevel::Info => info!("{}", message),
            LogLevel::Debug => debug!("{}", message),
            LogLevel::Trace => tracing::trace!("{}", message),
        }
    }
}

/// Additional utility methods for the server context
impl ServerContextImpl {
    /// Get the current number of active players
    /// 
    /// # Returns
    /// Number of active players in the region
    pub fn player_count(&self) -> usize {
        self.players.len()
    }
    
    /// Check if a player is currently active
    /// 
    /// # Arguments
    /// * `player_id` - Player identifier to check
    /// 
    /// # Returns
    /// True if the player is active, false otherwise
    pub fn is_player_active(&self, player_id: PlayerId) -> bool {
        self.players.contains_key(&player_id)
    }
    
    /// Get connection statistics
    /// 
    /// # Returns
    /// Number of active connections
    pub fn connection_count(&self) -> usize {
        self.connection_manager.connection_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::EventProcessor;
    
    #[tokio::test]
    async fn test_server_context_creation() {
        let region_id = RegionId::new();
        let players = Arc::new(DashMap::new());
        let connection_manager = Arc::new(ConnectionManager::new());
        let event_processor = Arc::new(EventProcessor::new());
        
        let context = ServerContextImpl::new(
            region_id,
            players,
            connection_manager,
            event_processor,
        );
        
        assert_eq!(context.region_id(), region_id);
        assert_eq!(context.player_count(), 0);
        assert_eq!(context.connection_count(), 0);
    }
    
    #[tokio::test]
    async fn test_get_players_empty() {
        let region_id = RegionId::new();
        let players = Arc::new(DashMap::new());
        let connection_manager = Arc::new(ConnectionManager::new());
        let event_processor = Arc::new(EventProcessor::new());
        
        let context = ServerContextImpl::new(
            region_id,
            players,
            connection_manager,
            event_processor,
        );
        
        let result = context.get_players().await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}