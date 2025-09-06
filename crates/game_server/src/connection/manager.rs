//! Connection manager for tracking and managing client connections.
//!
//! This module provides the central management system for all client connections,
//! handling connection lifecycle, player ID assignment, and message broadcasting.

use super::{client::ClientConnection, ConnectionId};
use horizon_event_system::{PlayerId, AuthenticationStatus};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::info;
use futures_util::sink::SinkExt;
use futures_util::stream::SplitSink;
use tokio_tungstenite::{WebSocketStream, tungstenite::Message};

/// Central manager for all client connections.
/// 
/// The `ConnectionManager` tracks active connections, assigns unique IDs,
/// manages player associations, and provides message broadcasting capabilities.
/// It uses async-safe data structures to handle concurrent access from multiple
/// connection handlers.
/// 
/// # Architecture
/// 
/// * Uses `RwLock<HashMap>` for thread-safe connection storage
/// * Implements atomic connection ID generation
/// * Provides broadcast channel for outgoing messages
/// * Maintains bidirectional player-connection mapping
#[derive(Debug)]
pub struct ConnectionManager {
    /// Map of connection ID to client connection information
    connections: Arc<RwLock<HashMap<ConnectionId, ClientConnection>>>,
    ws_senders: Arc<RwLock<HashMap<ConnectionId, Arc<tokio::sync::Mutex<SplitSink<WebSocketStream<tokio::net::TcpStream>, Message>>>>>>,
    
    /// Atomic counter for generating unique connection IDs
    next_id: Arc<std::sync::atomic::AtomicUsize>,
    
    /// Broadcast sender for outgoing messages to specific connections
    sender: broadcast::Sender<(ConnectionId, Vec<u8>)>,
}

impl ConnectionManager {
    /// Creates a new connection manager.
    /// 
    /// Initializes the internal data structures and broadcast channel
    /// with a reasonable buffer size for message queuing.
    /// 
    /// # Returns
    /// 
    /// A new `ConnectionManager` instance ready to handle connections.
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1000);
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            ws_senders: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(std::sync::atomic::AtomicUsize::new(1)),
            sender,
        }
    }

    /// Adds a new connection and returns its unique ID.
    /// 
    /// Creates a new connection entry with the provided remote address
    /// and assigns it a unique connection ID for tracking.
    /// 
    /// # Arguments
    /// 
    /// * `remote_addr` - The network address of the connecting client
    /// 
    /// # Returns
    /// 
    /// A unique `ConnectionId` assigned to this connection.
    pub async fn add_connection(&self, remote_addr: SocketAddr) -> ConnectionId {
        let connection_id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let connection = ClientConnection::new(remote_addr);
        let mut connections = self.connections.write().await;
        connections.insert(connection_id, connection);
        info!("🔗 Connection {} from {}", connection_id, remote_addr);
        connection_id
    }

    /// Register the WebSocket sender for a connection
    pub async fn register_ws_sender(&self, connection_id: ConnectionId, ws_sender: Arc<tokio::sync::Mutex<SplitSink<WebSocketStream<tokio::net::TcpStream>, Message>>>) {
        let mut senders = self.ws_senders.write().await;
        senders.insert(connection_id, ws_sender);
    }

    /// Remove the WebSocket sender for a connection
    pub async fn remove_ws_sender(&self, connection_id: ConnectionId) {
        let mut senders = self.ws_senders.write().await;
        senders.remove(&connection_id);
    }

    /// Kick (disconnect) a connection by ID, sending a close frame
    pub async fn kick_connection(&self, connection_id: ConnectionId, reason: Option<String>) -> Result<(), String> {
        let senders = self.ws_senders.read().await;
        if let Some(ws_sender) = senders.get(&connection_id) {
            let mut ws_sender = ws_sender.lock().await;
            use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
            let close_msg = Message::Close(Some(tokio_tungstenite::tungstenite::protocol::CloseFrame {
                code: CloseCode::Normal,
                reason: reason.unwrap_or_else(|| "Kicked by server".into()).into(),
            }));
            let _ = ws_sender.send(close_msg).await;
        }
        drop(senders);
        self.remove_connection(connection_id).await;
        self.remove_ws_sender(connection_id).await;
        Ok(())
    }

    /// Kick (disconnect) a player by PlayerId
    pub async fn kick_player(&self, player_id: PlayerId, reason: Option<String>) -> Result<(), String> {
        if let Some(conn_id) = self.get_connection_id_by_player(player_id).await {
            self.kick_connection(conn_id, reason).await
        } else {
            Err("Player not connected".to_string())
        }
    }

    /// Removes a connection from the manager.
    /// 
    /// Cleans up the connection entry and logs the disconnection.
    /// This should be called when a client disconnects or times out.
    /// 
    /// # Arguments
    /// 
    /// * `connection_id` - The ID of the connection to remove
    pub async fn remove_connection(&self, connection_id: ConnectionId) {
        let mut connections = self.connections.write().await;
        if let Some(connection) = connections.remove(&connection_id) {
            info!(
                "❌ Connection {} from {} disconnected",
                connection_id, connection.remote_addr
            );
        }
    }

    /// Associates a player ID with a connection.
    /// 
    /// This is typically called after successful authentication or
    /// when a player is assigned to a connection.
    /// 
    /// # Arguments
    /// 
    /// * `connection_id` - The connection to update
    /// * `player_id` - The player ID to assign
    pub async fn set_player_id(&self, connection_id: ConnectionId, player_id: PlayerId) {
        let mut connections = self.connections.write().await;
        if let Some(connection) = connections.get_mut(&connection_id) {
            connection.player_id = Some(player_id);
        }
    }

    /// Retrieves the player ID associated with a connection.
    /// 
    /// # Arguments
    /// 
    /// * `connection_id` - The connection to look up
    /// 
    /// # Returns
    /// 
    /// The associated `PlayerId` if found, or `None` if the connection
    /// doesn't exist or doesn't have a player assigned.
    pub async fn get_player_id(&self, connection_id: ConnectionId) -> Option<PlayerId> {
        let connections = self.connections.read().await;
        connections.get(&connection_id).and_then(|c| c.player_id)
    }

    /// Sends a message to a specific connection.
    /// 
    /// Queues a message for delivery to the specified connection through
    /// the internal broadcast channel.
    /// 
    /// # Arguments
    /// 
    /// * `connection_id` - The target connection
    /// * `message` - The message data to send
    pub async fn send_to_connection(&self, connection_id: ConnectionId, message: Vec<u8>) {
        if let Err(e) = self.sender.send((connection_id, message)) {
            tracing::error!("Failed to send message to connection {}: {:?}", connection_id, e);
        }
    }

    /// Creates a new receiver for outgoing messages.
    /// 
    /// Each connection handler should call this to get a receiver
    /// for messages targeted to their specific connection.
    /// 
    /// # Returns
    /// 
    /// A broadcast receiver for connection-targeted messages.
    pub fn subscribe(&self) -> broadcast::Receiver<(ConnectionId, Vec<u8>)> {
        self.sender.subscribe()
    }

    /// Finds the connection ID associated with a player.
    /// 
    /// Searches through active connections to find the one associated
    /// with the specified player ID.
    /// 
    /// # Arguments
    /// 
    /// * `player_id` - The player to search for
    /// 
    /// # Returns
    /// 
    /// The `ConnectionId` if the player is found and connected,
    /// or `None` if the player is not currently connected.
    pub async fn get_connection_id_by_player(&self, player_id: PlayerId) -> Option<ConnectionId> {
        let connections = self.connections.read().await;
        for (conn_id, connection) in connections.iter() {
            if connection.player_id == Some(player_id) {
                return Some(*conn_id);
            }
        }
        None
    }

    /// Sets the authentication status for a connection.
    /// 
    /// Updates the authentication status of the specified connection.
    /// This is typically called when authentication state changes.
    /// 
    /// # Arguments
    /// 
    /// * `connection_id` - The connection to update
    /// * `status` - The new authentication status
    pub async fn set_auth_status(&self, connection_id: ConnectionId, status: AuthenticationStatus) {
        let mut connections = self.connections.write().await;
        if let Some(connection) = connections.get_mut(&connection_id) {
            connection.set_auth_status(status);
        }
    }

    /// Gets the authentication status for a connection.
    /// 
    /// # Arguments
    /// 
    /// * `connection_id` - The connection to query
    /// 
    /// # Returns
    /// 
    /// The current authentication status, or `None` if the connection doesn't exist.
    pub async fn get_auth_status(&self, connection_id: ConnectionId) -> Option<AuthenticationStatus> {
        let connections = self.connections.read().await;
        connections.get(&connection_id).map(|c| c.auth_status())
    }

    /// Gets the authentication status for a player.
    /// 
    /// # Arguments
    /// 
    /// * `player_id` - The player to query
    /// 
    /// # Returns
    /// 
    /// The current authentication status, or `None` if the player is not connected.
    pub async fn get_auth_status_by_player(&self, player_id: PlayerId) -> Option<AuthenticationStatus> {
        let connections = self.connections.read().await;
        for connection in connections.values() {
            if connection.player_id == Some(player_id) {
                return Some(connection.auth_status());
            }
        }
        None
    }

    /// Sets the authentication status for a player.
    /// 
    /// Updates the authentication status of the connection associated with the specified player.
    /// 
    /// # Arguments
    /// 
    /// * `player_id` - The player to update
    /// * `status` - The new authentication status
    /// 
    /// # Returns
    /// 
    /// `true` if the player was found and updated, `false` otherwise.
    pub async fn set_auth_status_by_player(&self, player_id: PlayerId, status: AuthenticationStatus) -> bool {
        let mut connections = self.connections.write().await;
        for connection in connections.values_mut() {
            if connection.player_id == Some(player_id) {
                connection.set_auth_status(status);
                return true;
            }
        }
        false
    }

    /// Gets detailed connection information for a player.
    /// 
    /// # Arguments
    /// 
    /// * `player_id` - The player to query
    /// 
    /// # Returns
    /// 
    /// Connection information if the player is connected, `None` otherwise.
    pub async fn get_connection_info_by_player(&self, player_id: PlayerId) -> Option<(ConnectionId, SocketAddr, std::time::SystemTime, AuthenticationStatus)> {
        let connections = self.connections.read().await;
        for (conn_id, connection) in connections.iter() {
            if connection.player_id == Some(player_id) {
                return Some((*conn_id, connection.remote_addr, connection.connected_at, connection.auth_status()));
            }
        }
        None
    }
}