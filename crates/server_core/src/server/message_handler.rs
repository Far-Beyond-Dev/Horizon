//! Message handling and processing system
//! 
//! Processes incoming client messages and coordinates responses.

use crate::server::{ConnectionManager, EventProcessor};
use dashmap::DashMap;
use shared_types::*;
use std::sync::Arc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, error};

/// Handles incoming messages from WebSocket connections
/// 
/// The MessageHandler processes different types of client messages including:
/// - Player join/leave requests
/// - Movement commands
/// - Custom game data
/// - Plugin-specific messages
pub struct MessageHandler {
    /// Active players in the game
    players: Arc<DashMap<PlayerId, Player>>,
    /// Connection management system
    connection_manager: Arc<ConnectionManager>,
    /// Event processing system
    event_processor: Arc<EventProcessor>,
    /// Regional boundary constraints
    region_bounds: RegionBounds,
}

impl MessageHandler {
    /// Create a new message handler
    /// 
    /// # Arguments
    /// * `players` - Shared player registry
    /// * `connection_manager` - Connection management system
    /// * `event_processor` - Event processing system
    /// * `region_bounds` - Regional boundary constraints
    pub fn new(
        players: Arc<DashMap<PlayerId, Player>>,
        connection_manager: Arc<ConnectionManager>,
        event_processor: Arc<EventProcessor>,
        region_bounds: RegionBounds,
    ) -> Self {
        Self {
            players,
            connection_manager,
            event_processor,
            region_bounds,
        }
    }
    
    /// Process an incoming message from a client
    /// 
    /// # Arguments
    /// * `text` - JSON message text from client
    /// * `connection_id` - Connection identifier
    /// 
    /// # Returns
    /// Result indicating success or processing error
    /// 
    /// # Errors
    /// Returns `ServerError` for malformed messages, invalid operations, or system errors
    pub async fn handle_message(
        &self,
        text: &str,
        connection_id: PlayerId,
    ) -> Result<(), ServerError> {
        let message: NetworkMessage = serde_json::from_str(text)
            .map_err(|e| ServerError::Serialization(format!("Invalid JSON: {}", e)))?;

        debug!("Received message from connection {}: {:?}", connection_id, message);

        match message {
            NetworkMessage::PlayerJoin { name } => {
                self.handle_player_join(connection_id, name).await
            }
            NetworkMessage::PlayerMove { position } => {
                self.handle_player_move(connection_id, position).await
            }
            NetworkMessage::PlayerLeave => {
                self.handle_player_leave(connection_id).await
            }
            NetworkMessage::GameData { data } => {
                self.handle_game_data(connection_id, data).await
            }
            NetworkMessage::PluginMessage { plugin, data } => {
                self.handle_plugin_message(connection_id, plugin, data).await
            }
        }
    }
    
    /// Handle player join request
    /// 
    /// # Arguments
    /// * `connection_id` - Connection identifier
    /// * `name` - Player's chosen name
    async fn handle_player_join(&self, connection_id: PlayerId, name: String) -> Result<(), ServerError> {
        let player = Player::new(name.clone(), Position::new(0.0, 0.0, 0.0));
        let player_copy = player.clone();
        self.players.insert(connection_id, player);

        // Send success response to the joining player
        let response = NetworkMessage::GameData { 
            data: serde_json::json!({
                "type": "join_success",
                "player_id": connection_id,
                "name": name,
                "position": {"x": 0.0, "y": 0.0, "z": 0.0}
            })
        };

        self.send_response(connection_id, &response).await?;

        // Emit event for plugins
        self.event_processor.emit_event(
            EventId::new(EventNamespace::new("core"), "player_joined"),
            Arc::new(CoreEvent::PlayerJoined { player: player_copy })
        ).await?;

        info!("Player {} ({}) joined successfully", name, connection_id);
        Ok(())
    }
    
    /// Handle player movement request
    /// 
    /// # Arguments
    /// * `connection_id` - Connection identifier
    /// * `position` - New position coordinates
    async fn handle_player_move(&self, connection_id: PlayerId, position: Position) -> Result<(), ServerError> {
        // Validate position is within region bounds
        if !self.region_bounds.contains(&position) {
            return Err(ServerError::Region("Position outside region bounds".to_string()));
        }

        if let Some(mut player) = self.players.get_mut(&connection_id) {
            let old_position = player.position;
            player.position = position;

            // Send movement confirmation
            let response = NetworkMessage::GameData { 
                data: serde_json::json!({
                    "type": "move_success",
                    "position": position
                })
            };

            self.send_response(connection_id, &response).await?;

            // Emit movement event
            self.event_processor.emit_event(
                EventId::new(EventNamespace::new("core"), "player_moved"),
                Arc::new(CoreEvent::PlayerMoved { 
                    player_id: connection_id, 
                    old_position, 
                    new_position: position 
                })
            ).await?;

            debug!("Player {} moved to {:?}", connection_id, position);
        }
        
        Ok(())
    }
    
    /// Handle player leave request
    /// 
    /// # Arguments
    /// * `connection_id` - Connection identifier
    async fn handle_player_leave(&self, connection_id: PlayerId) -> Result<(), ServerError> {
        if let Some(player) = self.players.remove(&connection_id) {
            // Send goodbye message
            let response = NetworkMessage::GameData { 
                data: serde_json::json!({
                    "type": "leave_success"
                })
            };

            self.send_response(connection_id, &response).await?;

            // Emit leave event
            self.event_processor.emit_event(
                EventId::new(EventNamespace::new("core"), "player_left"),
                Arc::new(CoreEvent::PlayerLeft { player_id: player.0 })
            ).await?;

            info!("Player {} left", connection_id);
        }
        
        Ok(())
    }
    
    /// Handle custom game data message
    /// 
    /// # Arguments
    /// * `connection_id` - Connection identifier
    /// * `data` - Custom JSON data
    async fn handle_game_data(&self, connection_id: PlayerId, data: serde_json::Value) -> Result<(), ServerError> {
        // Echo the data back for testing
        let response = NetworkMessage::GameData { 
            data: serde_json::json!({
                "type": "echo",
                "original": data
            })
        };

        self.send_response(connection_id, &response).await?;

        // Emit custom message event
        self.event_processor.emit_event(
            EventId::new(EventNamespace::new("core"), "custom_message"),
            Arc::new(CoreEvent::CustomMessage { data })
        ).await?;
        
        Ok(())
    }
    
    /// Handle plugin-specific message
    /// 
    /// # Arguments
    /// * `connection_id` - Connection identifier
    /// * `plugin` - Target plugin name
    /// * `data` - Plugin-specific data
    async fn handle_plugin_message(
        &self,
        connection_id: PlayerId,
        plugin: String,
        data: serde_json::Value
    ) -> Result<(), ServerError> {
        // Send acknowledgment response
        let response = NetworkMessage::GameData { 
            data: serde_json::json!({
                "type": "plugin_message_received",
                "plugin": plugin,
                "data": data
            })
        };

        self.send_response(connection_id, &response).await?;

        // Emit plugin message event
        self.event_processor.emit_event(
            EventId::new(EventNamespace::plugin_default(&plugin), "message"),
            Arc::new(CoreEvent::CustomMessage { data })
        ).await?;
        
        Ok(())
    }
    
    /// Send a response message to a connection
    /// 
    /// # Arguments
    /// * `connection_id` - Target connection
    /// * `response` - Message to send
    async fn send_response(&self, connection_id: PlayerId, response: &NetworkMessage) -> Result<(), ServerError> {
        if let Ok(response_text) = serde_json::to_string(response) {
            let message_bytes = response_text.as_bytes();
            self.connection_manager.send_to_connection(connection_id, message_bytes).await?;
        }
        Ok(())
    }
}