//! Client connection representation and management.
//!
//! This module defines the structure and behavior of individual client
//! connections, tracking their state and metadata.

use horizon_event_system::{PlayerId, AuthenticationStatus};
use std::net::SocketAddr;
use std::time::SystemTime;

/// Represents an individual client connection to the server.
/// 
/// This structure tracks the essential information about a connected client,
/// including their player ID (once assigned), network address, connection timing,
/// and authentication status.
/// 
/// # Fields
/// 
/// * `player_id` - Optional player ID assigned after successful authentication/identification
/// * `remote_addr` - The network address of the connected client
/// * `connected_at` - Timestamp when the connection was established
/// * `auth_status` - Current authentication status of the connection
#[derive(Debug)]
pub struct ClientConnection {
    /// The player ID assigned to this connection (None until assigned)
    pub player_id: Option<PlayerId>,
    
    /// The remote network address of the client
    pub remote_addr: SocketAddr,
    
    /// When this connection was established
    pub connected_at: SystemTime,
    
    /// Current authentication status of this connection
    pub auth_status: AuthenticationStatus,
}

impl ClientConnection {
    /// Creates a new client connection with the specified remote address.
    /// 
    /// The connection starts without a player ID assigned, in an unauthenticated state,
    /// and records the current time as the connection timestamp.
    /// 
    /// # Arguments
    /// 
    /// * `remote_addr` - The network address of the connecting client
    /// 
    /// # Returns
    /// 
    /// A new `ClientConnection` instance ready for use.
    pub fn new(remote_addr: SocketAddr) -> Self {
        Self {
            player_id: None,
            remote_addr,
            connected_at: SystemTime::now(),
            auth_status: AuthenticationStatus::default(),
        }
    }

    /// Gets the current authentication status of the connection.
    pub fn auth_status(&self) -> AuthenticationStatus {
        self.auth_status
    }

    /// Sets the authentication status of the connection.
    pub fn set_auth_status(&mut self, status: AuthenticationStatus) {
        self.auth_status = status;
    }
}