//! Message routing logic for dispatching client messages to plugins.
//!
//! This module handles the parsing and routing of incoming client messages
//! to the appropriate plugin handlers through the event system.

use crate::{connection::ConnectionId, error::ServerError, messaging::ClientMessage};
use universal_plugin_system::{
    EventBus, StructuredEventKey, propagation::CompositePropagator,
};
use tracing::{debug, info};
use serde_json;

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

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
/// * `event_bus` - Universal event bus for dispatching to plugins
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
/// 3. Route the parsed message to the appropriate plugin namespace/event
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
    event_bus: &EventBus<StructuredEventKey, CompositePropagator<StructuredEventKey>>,
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

    // Create client message for routing with player context
    let client_message_data = serde_json::json!({
        "player_id": player_id,
        "namespace": message.namespace,
        "event": message.event,
        "data": message.data,
        "timestamp": current_timestamp()
    });

    // Route to plugins via the universal event system
    event_bus
        .emit(&message.namespace, &message.event, &client_message_data)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    info!(
        "✅ Routed '{}:{}' message from player {} to plugins",
        message.namespace, message.event, player_id
    );
    Ok(())
}