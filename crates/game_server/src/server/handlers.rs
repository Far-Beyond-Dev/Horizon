//! Connection handling logic for WebSocket clients.
//!
//! This module contains the core connection handling logic that manages
//! the lifecycle of individual client connections, including WebSocket
//! handshaking, message processing, and cleanup.

use crate::{
    connection::ConnectionManager,
    error::ServerError,
    messaging::route_client_message,
};
use futures::{SinkExt, StreamExt};
use horizon_event_system::{
    current_timestamp, DisconnectReason, EventSystem, PlayerConnectedEvent,
    PlayerDisconnectedEvent, PlayerId,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{debug, error, trace};

/// Handles a single client connection from establishment to cleanup.
/// 
/// This function manages the complete lifecycle of a client connection,
/// including WebSocket handshaking, player ID assignment, message routing,
/// and proper cleanup when the connection ends.
/// 
/// # Connection Flow
/// 
/// 1. Perform WebSocket handshake
/// 2. Register connection with the connection manager
/// 3. Generate and assign a player ID
/// 4. Emit player connected event
/// 5. Start message handling tasks (incoming and outgoing)
/// 6. Handle connection termination and cleanup
/// 7. Emit player disconnected event
/// 
/// # Arguments
/// 
/// * `stream` - The TCP stream for the client connection
/// * `addr` - The remote address of the client
/// * `connection_manager` - Manager for tracking connections
/// * `horizon_event_system` - Event system for plugin communication
/// 
/// # Returns
/// 
/// `Ok(())` if the connection was handled successfully, or a `ServerError`
/// if there was a failure during connection handling.
/// 
/// # Message Handling
/// 
/// The function spawns two concurrent tasks:
/// 
/// * **Incoming Task**: Receives messages from the client and routes them to plugins
/// * **Outgoing Task**: Receives messages from plugins and sends them to the client
/// 
/// These tasks run until the connection is closed or an error occurs.
pub async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    connection_manager: Arc<ConnectionManager>,
    horizon_event_system: Arc<EventSystem>,
) -> Result<(), ServerError> {
    // Perform WebSocket handshake
    let ws_stream = accept_async(stream)
        .await
        .map_err(|e| ServerError::Network(format!("WebSocket handshake failed: {e}")))?;

    let (ws_sender, mut ws_receiver) = ws_stream.split();
    let ws_sender = Arc::new(tokio::sync::Mutex::new(ws_sender));
    let connection_id = connection_manager.add_connection(addr).await;
    connection_manager.register_ws_sender(connection_id, ws_sender.clone()).await;

    // Generate player ID and emit connection event
    let player_id = PlayerId::new();
    connection_manager
        .set_player_id(connection_id, player_id)
        .await;

    // Emit core infrastructure event
    horizon_event_system
        .emit_core(
            "player_connected",
            &PlayerConnectedEvent {
                player_id,
                connection_id: connection_id.to_string(),
                remote_addr: addr.to_string(),
                timestamp: current_timestamp(),
            },
        )
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    let mut message_receiver = connection_manager.subscribe();
    let ws_sender_incoming = ws_sender.clone();
    let ws_sender_outgoing = ws_sender.clone();

    // Incoming message task - routes raw messages to plugins
    let incoming_task = {
        let connection_manager = connection_manager.clone();
        let horizon_event_system = horizon_event_system.clone();

        async move {
            while let Some(msg) = ws_receiver.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        // Route raw message to plugins via events
                        if let Err(e) = route_client_message(
                            &text,
                            connection_id,
                            &connection_manager,
                            &horizon_event_system,
                        )
                        .await
                        {
                            trace!("❌ Message routing error: {}", e);
                        }
                    }
                    Ok(Message::Close(_)) => {
                        debug!("🔌 Client {} requested close", connection_id);
                        break;
                    }
                    Ok(Message::Ping(data)) => {
                        let mut ws_sender = ws_sender_incoming.lock().await;
                        let _ = ws_sender.send(Message::Pong(data)).await;
                    }
                    Err(e) => {
                        error!("WebSocket error for connection {}: {}", connection_id, e);
                        break;
                    }
                    _ => {}
                }
            }
        }
    };

    // Outgoing message task
    let outgoing_task = {
        let ws_sender = ws_sender_outgoing;
        async move {
            while let Ok((target_connection_id, message)) = message_receiver.recv().await {
                if target_connection_id == connection_id {
                    let message_text = String::from_utf8_lossy(&message);
                    let mut ws_sender = ws_sender.lock().await;
                    if let Err(e) = ws_sender
                        .send(Message::Text(message_text.to_string().into()))
                        .await
                    {
                        error!("Failed to send message: {}", e);
                        break;
                    }
                }
            }
        }
    };

    // Run both tasks concurrently until one completes
    tokio::select! {
        _ = incoming_task => {},
        _ = outgoing_task => {},
    }

    // Emit disconnection event
    if let Some(player_id) = connection_manager.get_player_id(connection_id).await {
        horizon_event_system
            .emit_core(
                "player_disconnected",
                &PlayerDisconnectedEvent {
                    player_id,
                    connection_id: connection_id.to_string(),
                    reason: DisconnectReason::ClientDisconnect,
                    timestamp: current_timestamp(),
                },
            )
            .await
            .map_err(|e| ServerError::Internal(e.to_string()))?;
    }

    connection_manager.remove_connection(connection_id).await;
    connection_manager.remove_ws_sender(connection_id).await;
    Ok(())
}