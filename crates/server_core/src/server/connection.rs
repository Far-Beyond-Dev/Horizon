//! Connection management system
//! 
//! Handles WebSocket connections, player sessions, and message routing.

use crate::server::MessageHandler;
use dashmap::DashMap;
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use shared_types::*;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};
use tracing::{error, info, warn};

/// Type alias for WebSocket stream
type WsStream = WebSocketStream<TcpStream>;
/// Type alias for WebSocket sink (outgoing messages)
pub type WsSink = SplitSink<WsStream, Message>;
/// Type alias for WebSocket receiver (incoming messages)  
type WsReceiver = SplitStream<WsStream>;

/// Unique identifier for a connection
pub type ConnectionId = PlayerId;

/// Manages WebSocket connections and player sessions
/// 
/// The ConnectionManager handles:
/// - WebSocket handshake and connection establishment
/// - Message routing between clients and the message handler
/// - Connection cleanup on disconnect
/// - Broadcasting capabilities
pub struct ConnectionManager {
    /// Active WebSocket connections mapped by player ID
    connections: Arc<DashMap<ConnectionId, WsSink>>,
}

impl ConnectionManager {
    /// Create a new connection manager
    pub fn new() -> Self {
        Self {
            connections: Arc::new(DashMap::new()),
        }
    }
    
    /// Handle a new incoming TCP connection
    /// 
    /// This method:
    /// 1. Performs WebSocket handshake
    /// 2. Creates a unique connection ID
    /// 3. Splits the connection for bidirectional communication
    /// 4. Spawns a task to handle incoming messages
    /// 
    /// # Arguments
    /// * `stream` - TCP stream from the client
    /// * `addr` - Client's socket address
    /// * `message_handler` - Handler for processing incoming messages
    pub async fn handle_new_connection(
        &self,
        stream: TcpStream,
        addr: SocketAddr,
        message_handler: Arc<MessageHandler>,
    ) {
        // Perform WebSocket handshake
        let ws_stream = match accept_async(stream).await {
            Ok(ws) => ws,
            Err(e) => {
                error!("WebSocket handshake failed for {}: {}", addr, e);
                return;
            }
        };

        let (ws_sink, ws_receiver) = ws_stream.split();
        let connection_id = ConnectionId::new();

        // Store the connection
        self.connections.insert(connection_id, ws_sink);
        info!("Connection {} established from {}", connection_id, addr);

        // Spawn task to handle messages from this connection
        let connections = self.connections.clone();
        tokio::spawn(async move {
            Self::handle_connection_messages(
                connection_id,
                ws_receiver,
                message_handler,
                connections.clone(),
            ).await;
            
            // Clean up on disconnect
            connections.remove(&connection_id);
            info!("Connection {} from {} closed", connection_id, addr);
        });
    }
    
    /// Handle incoming messages from a specific connection
    /// 
    /// # Arguments
    /// * `connection_id` - Unique identifier for this connection
    /// * `ws_receiver` - WebSocket receiver stream
    /// * `message_handler` - Handler for processing messages
    /// * `connections` - Shared connection map for sending responses
    async fn handle_connection_messages(
        connection_id: ConnectionId,
        mut ws_receiver: WsReceiver,
        message_handler: Arc<MessageHandler>,
        connections: Arc<DashMap<ConnectionId, WsSink>>,
    ) {
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Err(e) = message_handler.handle_message(
                        &text,
                        connection_id,
                    ).await {
                        error!("Error handling message from {}: {}", connection_id, e);
                        
                        // Send error response to client
                        Self::send_error_response(&connections, connection_id, &e.to_string()).await;
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("Connection {} requested close", connection_id);
                    break;
                }
                Ok(Message::Ping(data)) => {
                    // Respond to ping with pong
                    if let Some(mut sink) = connections.get_mut(&connection_id) {
                        let _ = sink.send(Message::Pong(data)).await;
                    }
                }
                Ok(Message::Pong(_)) => {
                    // Pong received, connection is alive
                }
                Err(e) => {
                    error!("WebSocket error for connection {}: {}", connection_id, e);
                    break;
                }
                _ => {
                    warn!("Received unsupported message type from {}", connection_id);
                }
            }
        }
    }
    
    /// Send an error response to a specific connection
    /// 
    /// # Arguments
    /// * `connections` - Shared connection map
    /// * `connection_id` - Target connection
    /// * `error_message` - Error message to send
    async fn send_error_response(
        connections: &DashMap<ConnectionId, WsSink>,
        connection_id: ConnectionId,
        error_message: &str,
    ) {
        let error_response = NetworkMessage::GameData { 
            data: serde_json::json!({
                "type": "error",
                "message": error_message
            })
        };
        
        if let Ok(response_text) = serde_json::to_string(&error_response) {
            if let Some(mut sink) = connections.get_mut(&connection_id) {
                let _ = sink.send(Message::Text(response_text)).await;
            }
        }
    }
    
    /// Send a message to a specific connection
    /// 
    /// # Arguments
    /// * `connection_id` - Target connection ID
    /// * `message` - Message bytes to send
    /// 
    /// # Returns
    /// Result indicating success or failure
    pub async fn send_to_connection(&self, connection_id: ConnectionId, message: &[u8]) -> Result<(), ServerError> {
        if let Some(mut sink) = self.connections.get_mut(&connection_id) {
            let message_text = String::from_utf8_lossy(message);
            sink.send(Message::Text(message_text.to_string())).await
                .map_err(|e| ServerError::Network(format!("Failed to send message: {}", e)))?;
        }
        Ok(())
    }
    
    /// Broadcast a message to all connected clients
    /// 
    /// # Arguments
    /// * `message` - Message bytes to broadcast
    /// 
    /// # Returns
    /// Result indicating success or failure
    pub async fn broadcast_to_all(&self, message: &[u8]) -> Result<(), ServerError> {
        let message_text = String::from_utf8_lossy(message);
        let msg = Message::Text(message_text.to_string());
        
        for mut entry in self.connections.iter_mut() {
            let _ = entry.value_mut().send(msg.clone()).await;
        }
        
        Ok(())
    }
    
    /// Get the number of active connections
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }
    
    /// Close all connections gracefully
    pub async fn shutdown_all(&self) {
        for mut entry in self.connections.iter_mut() {
            let _ = entry.value_mut().send(Message::Close(None)).await;
        }
        self.connections.clear();
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}