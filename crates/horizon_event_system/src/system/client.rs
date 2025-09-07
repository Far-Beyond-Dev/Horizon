/// Client connection and response handling
use crate::events::EventError;
use crate::types::{PlayerId, AuthenticationStatus};
// use serde::{Deserialize, Serialize}; // Unused
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
    /// Current authentication status of the connection
    pub auth_status: AuthenticationStatus,
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
            .field("auth_status", &self.auth_status)
            .field("response_sender", &"[response_sender]")
            .finish()
    }
}

impl ClientConnectionRef {
    /// Kick (disconnect) this client from the server, with an optional reason
    pub async fn kick(&self, reason: Option<String>) -> Result<(), EventError> {
        self.response_sender
            .kick(self.player_id, reason)
            .await
            .map_err(|e| EventError::HandlerExecution(format!("Failed to kick client: {}", e)))
    }
    /// Creates a new client connection reference
    pub fn new(
        player_id: PlayerId,
        remote_addr: SocketAddr,
        connection_id: String,
        connected_at: u64,
        auth_status: AuthenticationStatus,
        response_sender: Arc<dyn ClientResponseSender + Send + Sync>,
    ) -> Self {
        Self {
            player_id,
            remote_addr,
            connection_id,
            connected_at,
            auth_status,
            response_sender,
        }
    }

    /// Gets the current authentication status of this connection
    pub fn auth_status(&self) -> AuthenticationStatus {
        self.auth_status
    }

    /// Checks if the connection is authenticated
    pub fn is_authenticated(&self) -> bool {
        self.auth_status == AuthenticationStatus::Authenticated
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

    /// Get the authentication status of a client
    fn get_auth_status(&self, player_id: PlayerId) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<AuthenticationStatus>> + Send + '_>>;

    /// Kick (disconnect) a client by player ID, sending a close frame and removing the connection.
    fn kick(&self, player_id: PlayerId, reason: Option<String>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>>;

    /// Broadcast data to all connected clients
    fn broadcast_to_all(&self, _data: Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<usize, String>> + Send + '_>> {
        // Default implementation that provides a working fallback - returns 0 clients reached
        // Individual implementations should override this with actual broadcast functionality
        Box::pin(async move { Ok(0) })
    }

    /// Get connection information for a client (optional implementation)
    fn get_connection_info(&self, _player_id: PlayerId) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<ClientConnectionInfo>> + Send + '_>> {
        // Default implementation returns None to maintain backwards compatibility
        Box::pin(async move { None::<ClientConnectionInfo> })
    }
}

/// Information about a client connection
#[derive(Debug, Clone)]
pub struct ClientConnectionInfo {
    pub player_id: PlayerId,
    pub remote_addr: std::net::SocketAddr,
    pub connection_id: String,
    pub connected_at: u64,
    pub auth_status: AuthenticationStatus,
}