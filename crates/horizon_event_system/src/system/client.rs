/// Client connection and response handling
use crate::events::EventError;
use crate::types::PlayerId;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;

/// Connection-aware client reference that provides handlers with access to the client connection
/// and methods to respond directly to that specific client.
#[derive(Clone)]
pub struct ClientConnectionRef {
    /// The player ID associated with this connection
    pub player_id: PlayerId,
    /// The remote address of the client
    pub remote_addr: SocketAddr,
    /// Connection ID for internal tracking
    pub connection_id: String,
    /// Timestamp when the connection was established
    pub connected_at: u64,
    /// Sender for direct response to this specific client
    response_sender: Arc<dyn ClientResponseSender + Send + Sync>,
}

impl std::fmt::Debug for ClientConnectionRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientConnectionRef")
            .field("player_id", &self.player_id)
            .field("remote_addr", &self.remote_addr)
            .field("connection_id", &self.connection_id)
            .field("connected_at", &self.connected_at)
            .field("response_sender", &"[response_sender]")
            .finish()
    }
}

impl ClientConnectionRef {
    /// Creates a new client connection reference
    pub fn new(
        player_id: PlayerId,
        remote_addr: SocketAddr,
        connection_id: String,
        connected_at: u64,
        response_sender: Arc<dyn ClientResponseSender + Send + Sync>,
    ) -> Self {
        Self {
            player_id,
            remote_addr,
            connection_id,
            connected_at,
            response_sender,
        }
    }

    /// Send a direct response to this specific client
    pub async fn respond(&self, data: &[u8]) -> Result<(), EventError> {
        self.response_sender
            .send_to_client(self.player_id, data.to_vec())
            .await
            .map_err(|e| EventError::HandlerExecution(format!("Failed to send response: {}", e)))
    }

    /// Send a JSON response to this specific client
    pub async fn respond_json<T: serde::Serialize>(&self, data: &T) -> Result<(), EventError> {
        let json = serde_json::to_vec(data)
            .map_err(|e| EventError::HandlerExecution(format!("JSON serialization failed: {}", e)))?;
        self.respond(&json).await
    }

    /// Check if this connection is still active
    pub async fn is_active(&self) -> bool {
        self.response_sender.is_connection_active(self.player_id).await
    }
}

/// Trait for sending responses to clients - implemented by the server/connection manager
pub trait ClientResponseSender: std::fmt::Debug {
    /// Send data to a specific client
    fn send_to_client(&self, player_id: PlayerId, data: Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>>;
    
    /// Check if a client connection is still active
    fn is_connection_active(&self, player_id: PlayerId) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + '_>>;
}