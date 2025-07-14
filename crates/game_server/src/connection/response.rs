//! Response sender implementation for the event system.
//!
//! This module provides the implementation of `ClientResponseSender` that
//! integrates the connection manager with the event system for sending
//! responses back to clients.

use super::manager::ConnectionManager;
use horizon_event_system::{ClientResponseSender, PlayerId};
use std::sync::Arc;

/// Implementation of `ClientResponseSender` for the game server.
/// 
/// This struct bridges the gap between the event system and the connection
/// manager, allowing plugins to send responses back to clients through
/// the standardized event system interface.
/// 
/// # Architecture
/// 
/// The `GameServerResponseSender` wraps the connection manager and provides
/// the async interface required by the event system. It handles the mapping
/// from player IDs to active connections and manages message delivery.
#[derive(Clone, Debug)]
pub struct GameServerResponseSender {
    /// Reference to the connection manager for looking up and messaging connections
    connection_manager: Arc<ConnectionManager>,
}

impl GameServerResponseSender {
    /// Creates a new response sender with the given connection manager.
    /// 
    /// # Arguments
    /// 
    /// * `connection_manager` - The connection manager to use for sending responses
    /// 
    /// # Returns
    /// 
    /// A new `GameServerResponseSender` instance ready to handle responses.
    pub fn new(connection_manager: Arc<ConnectionManager>) -> Self {
        Self { connection_manager }
    }
}

impl ClientResponseSender for GameServerResponseSender {
    /// Sends data to a specific client identified by player ID.
    /// 
    /// This method looks up the active connection for the given player
    /// and queues the data for delivery through the connection manager.
    /// 
    /// # Arguments
    /// 
    /// * `player_id` - The ID of the player to send data to
    /// * `data` - The raw bytes to send to the client
    /// 
    /// # Returns
    /// 
    /// A future that resolves to `Ok(())` if the message was queued successfully,
    /// or an error string if the player is not found or not connected.
    fn send_to_client(&self, player_id: PlayerId, data: Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        let connection_manager = self.connection_manager.clone();
        Box::pin(async move {
            if let Some(connection_id) = connection_manager.get_connection_id_by_player(player_id).await {
                connection_manager.send_to_connection(connection_id, data).await;
                Ok(())
            } else {
                Err(format!("Player {} not found or not connected", player_id))
            }
        })
    }

    /// Checks if a player connection is currently active.
    /// 
    /// This method verifies whether a player is currently connected
    /// and available to receive messages.
    /// 
    /// # Arguments
    /// 
    /// * `player_id` - The ID of the player to check
    /// 
    /// # Returns
    /// 
    /// A future that resolves to `true` if the player is connected,
    /// or `false` if they are not currently active.
    fn is_connection_active(&self, player_id: PlayerId) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + '_>> {
        let connection_manager = self.connection_manager.clone();
        Box::pin(async move {
            connection_manager.get_connection_id_by_player(player_id).await.is_some()
        })
    }
}