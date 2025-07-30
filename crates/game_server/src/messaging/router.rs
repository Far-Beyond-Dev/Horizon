//! Message routing logic for dispatching client messages to plugins.
//!
//! This module handles the parsing and routing of incoming client messages
//! to the appropriate plugin handlers through the event system.

use crate::{connection::ConnectionId, error::ServerError, messaging::ClientMessage};
use horizon_event_system::{current_timestamp, EventSystem, PlayerId, RawClientMessageEvent};
use tracing::{debug, info};

/// Routes a raw client message to the appropriate plugin handlers.
/// 
/// This function parses incoming text messages from clients, extracts the
/// namespace and event information, and routes them through the event system
/// to registered plugin handlers.
/// 
/// # Arguments
/// 
/// * `text` - The raw message text from the client (expected to be JSON)
/// * `connection_id` - The unique identifier for the client connection
/// * `connection_manager` - Manager for looking up player information
/// * `horizon_event_system` - Event system for dispatching to plugins
/// 
/// # Returns
/// 
/// `Ok(())` if the message was successfully routed, or a `ServerError` if
/// parsing failed or the player was not found.
/// 
/// # Message Flow
/// 
/// 1. Parse the raw text as a `ClientMessage` JSON structure
/// 2. Look up the player ID for the connection
/// 3. Create a `RawClientMessageEvent` for core processing
/// 4. Emit the raw event to core handlers
/// 5. Route the parsed message to the appropriate plugin namespace/event
/// 
/// # Example Message Format
/// 
/// ```json
/// {
///   "namespace": "movement",
///   "event": "move_request", 
///   "data": { "target_x": 100.0, "target_y": 200.0 }
/// }
/// ```
pub async fn route_client_message(
    text: &str,
    connection_id: ConnectionId,
    connection_manager: &crate::connection::ConnectionManager,
    horizon_event_system: &EventSystem,
) -> Result<(), ServerError> {
    // Parse as generic message structure
    let message: ClientMessage = serde_json::from_str(text)
        .map_err(|e| ServerError::Network(format!("Invalid JSON: {e}")))?;

    let player_id = connection_manager
        .get_player_id(connection_id)
        .await
        .ok_or_else(|| ServerError::Internal("Player not found".to_string()))?;

    debug!(
        "📨 Routing message to namespace '{}' event '{}' from player {}",
        message.namespace, message.event, player_id
    );

    // Create raw message event for plugins to handle
    let raw_event = RawClientMessageEvent {
        player_id,
        message_type: format!("{}:{}", &message.namespace, &message.event),
        data: message.data.to_string().into_bytes(),
        timestamp: current_timestamp(),
    };

    // Emit to core for routing (plugins will listen to this)
    horizon_event_system
        .emit_core("raw_client_message", &raw_event)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    // Generic routing using client-specified namespace and event with connection context
    horizon_event_system
        .emit_client_with_context(&message.namespace, &message.event, player_id, &message.data)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    info!(
        "✅ Routed '{}:{}' message from player {} to plugins",
        message.namespace, message.event, player_id
    );
    Ok(())
}