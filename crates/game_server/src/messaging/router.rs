//! Message routing logic for dispatching client messages to plugins.
//!
//! This module handles the parsing and routing of incoming client messages
//! to the appropriate plugin handlers through the event system.

use crate::{connection::ConnectionId, error::ServerError, messaging::ClientMessage};
use horizon_event_system::{current_timestamp, EventSystem, RawClientMessageEvent, GorcObjectId};
use tracing::{debug, trace, warn};

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
/// 6. Check if the message is GORC-compatible and route to GORC handlers if applicable
/// 
/// # Example Message Format
/// 
/// Standard client message (routed only to client handlers):
/// ```json
/// {
///   "namespace": "movement",
///   "event": "move_request", 
///   "data": { "target_x": 100.0, "target_y": 200.0 }
/// }
/// ```
/// 
/// GORC message (routed to both client and GORC handlers due to instance_uuid field):
/// ```json
/// {
///   "namespace": "auth",
///   "event": "login",
///   "data": {
///     "instance_uuid": "12345678-1234-1234-1234-123456789abc",
///     "object_id": "auth_session_001", 
///     "credentials": {
///       "username": "admin",
///       "password": "password123"
///     }
///   }
/// }
/// ```
/// 
/// The presence of `instance_uuid` in the data field determines GORC routing.
pub async fn route_client_message(
    text: &str,
    connection_id: ConnectionId,
    connection_manager: &crate::connection::ConnectionManager,
    horizon_event_system: &EventSystem,
) -> Result<(), ServerError> {
    // Check if this is a native GORC event format first
    if let Ok(parsed_json) = serde_json::from_str::<serde_json::Value>(text) {
        if let Some(msg_type) = parsed_json.get("type").and_then(|v| v.as_str()) {
            if msg_type == "gorc_event" {
                return route_native_gorc_event(text, connection_id, connection_manager, horizon_event_system).await;
            }
        }
    }
    
    // Parse as generic ClientMessage structure (legacy format)
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

    // Check if this message should also be routed to GORC handlers
    // For messages that match the GORC format, also emit as GORC events
    if is_gorc_compatible_message(&message) {
        if let Err(e) = route_to_gorc_handlers(&message, player_id, horizon_event_system).await {
            // Log warning but don't fail the overall message routing
            warn!("Failed to route message to GORC handlers: {}", e);
        }
    }

    trace!(
        "✅ Routed '{}:{}' message from player {} to plugins",
        message.namespace, message.event, player_id
    );
    Ok(())
}

/// Routes a native GORC event directly to the EventSystem.
/// 
/// This function handles the native GORC event format that clients use to communicate
/// directly with GORC instances without requiring conversion to ClientMessage format.
/// 
/// # Native GORC Event Format
/// 
/// ```json
/// {
///   "type": "gorc_event",
///   "object_id": "GorcObjectId(...)",
///   "channel": 0,
///   "event": "move",
///   "data": { ... },
///   "player_id": "..."
/// }
/// ```
/// 
/// # Arguments
/// 
/// * `text` - The raw JSON text of the native GORC event
/// * `connection_id` - The connection ID of the client
/// * `connection_manager` - Manager for connection tracking
/// * `horizon_event_system` - Event system for routing
/// 
/// # Returns
/// 
/// `Ok(())` if the event was successfully routed, or a `ServerError` if parsing failed
async fn route_native_gorc_event(
    text: &str,
    connection_id: ConnectionId,
    connection_manager: &crate::connection::ConnectionManager,
    horizon_event_system: &EventSystem,
) -> Result<(), ServerError> {
    // Parse the native GORC event
    #[derive(serde::Deserialize)]
    struct NativeGorcEvent {
        #[serde(rename = "type")]
        msg_type: String,
        object_id: String,
        channel: u8,
        event: String,
        data: serde_json::Value,
        player_id: String,
    }
    
    let gorc_msg: NativeGorcEvent = serde_json::from_str(text)
        .map_err(|e| ServerError::Network(format!("Invalid native GORC event JSON: {e}")))?;
        
    let player_id = connection_manager
        .get_player_id(connection_id)
        .await
        .ok_or_else(|| ServerError::Internal("Player not found".to_string()))?;
        
    debug!(
        "🎯 Routing native GORC event: object_id='{}', channel={}, event='{}' from player {}",
        gorc_msg.object_id, gorc_msg.channel, gorc_msg.event, player_id
    );
    
    // Create raw message event for core handlers
    let raw_event = RawClientMessageEvent {
        player_id,
        message_type: format!("gorc:{}:{}", gorc_msg.channel, gorc_msg.event),
        data: gorc_msg.data.to_string().into_bytes(),
        timestamp: current_timestamp(),
    };
    
    // Emit to core for processing
    horizon_event_system
        .emit_core("raw_client_message", &raw_event)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;
    
    // Parse the object_id to extract GORC ID if it's in the expected format
    let gorc_id = if gorc_msg.object_id.starts_with("GorcObjectId(") && gorc_msg.object_id.ends_with(")") {
        // Extract the UUID from "GorcObjectId(uuid)"
        let uuid_str = &gorc_msg.object_id[13..gorc_msg.object_id.len()-1]; // Remove "GorcObjectId(" and ")"
        GorcObjectId::from_str(uuid_str)
            .map_err(|e| ServerError::Internal(format!("Invalid GORC object ID format: {}", e)))?
    } else {
        return Err(ServerError::Network("Invalid GORC object_id format".to_string()));
    };
    
    // Route to GORC handlers using the EventSystem's native GORC routing
    match horizon_event_system.emit_gorc_instance(
        gorc_id,
        gorc_msg.channel, 
        &gorc_msg.event,
        &gorc_msg.data,
        horizon_event_system::Dest::Both
    ).await {
        Ok(()) => {
            debug!("✅ Successfully routed native GORC event to handlers: {}:{}:{}", 
                gorc_id, gorc_msg.channel, gorc_msg.event);
        }
        Err(e) => {
            // Log as warning but don't fail - might be no handlers registered yet
            warn!("📝 No GORC handlers found for {}:{}:{}: {}", 
                gorc_id, gorc_msg.channel, gorc_msg.event, e);
        }
    }
    
    trace!(
        "✅ Processed native GORC event '{}:{}' from player {} via connection {}",
        gorc_msg.channel, gorc_msg.event, player_id, connection_id
    );
    
    Ok(())
}

/// Checks if a client message is a GORC event.
/// 
/// A message is considered a GORC event if it contains an `instance_uuid` field
/// in its data. This provides a unified message format where the presence of
/// this field determines routing behavior.
/// 
/// # Arguments
/// 
/// * `message` - The parsed client message to check
/// 
/// # Returns
/// 
/// `true` if the message contains an instance_uuid and should be routed to GORC handlers
fn is_gorc_compatible_message(message: &ClientMessage) -> bool {
    // Simple check: if the message has an instance_uuid field, it's a GORC event
    if let Ok(data_obj) = serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(message.data.clone()) {
        return data_obj.contains_key("instance_uuid");
    }
    
    false
}

/// Routes a client message to available GORC handlers.
/// 
/// This function attempts to map client messages to GORC handler patterns
/// by interpreting the namespace and event as GORC object type and event names.
/// 
/// # Arguments
/// 
/// * `message` - The client message to route
/// * `player_id` - The player ID of the originating client
/// * `horizon_event_system` - The event system for emitting GORC events
/// 
/// # Returns
/// 
/// `Ok(())` if routing succeeded, or a `ServerError` if routing failed
async fn route_to_gorc_handlers(
    message: &ClientMessage,
    player_id: horizon_event_system::PlayerId,
    horizon_event_system: &EventSystem,
) -> Result<(), ServerError> {
    // Extract GORC parameters from the message
    let object_type = extract_object_type_from_message(message);
    let channel = extract_channel_from_message(message);
    let event_name = &message.event;
    
    debug!(
        "🔄 Attempting GORC routing: object_type='{}', channel={}, event='{}'",
        object_type, channel, event_name
    );
    
    // Extract instance_uuid and object_id from message
    let instance_uuid = extract_instance_uuid_from_message(message);
    let object_id = extract_object_id_from_message(message);
    
    // Create a proper GorcEvent structure
    let gorc_event = horizon_event_system::GorcEvent {
        object_id,
        instance_uuid,
        object_type: object_type.clone(),
        channel,
        data: serde_json::to_vec(&serde_json::json!({
            "player_id": player_id,
            "event_name": event_name,
            "original_namespace": message.namespace,
            "data": message.data,
            "timestamp": current_timestamp()
        })).unwrap_or_default(),
        priority: "Normal".to_string(),
        timestamp: current_timestamp(),
    };
    
    // Try to route the client message to GORC instance handlers
    match horizon_event_system.route_client_message_to_gorc(player_id, &object_type, channel, event_name, &gorc_event).await {
        Ok(()) => {
            debug!("✅ Successfully routed client message to GORC handlers: {}:{}:{}", object_type, channel, event_name);
        }
        Err(e) => {
            // This is expected if no GORC handlers exist for this pattern
            debug!("📝 No GORC handlers found for {}:{}:{}: {}", object_type, channel, event_name, e);
        }
    }
    
    Ok(())
}

/// Extracts the object type from a client message for GORC routing.
/// 
/// # Arguments
/// 
/// * `message` - The client message to analyze
/// 
/// # Returns
/// 
/// A string representing the object type for GORC handler routing
fn extract_object_type_from_message(message: &ClientMessage) -> String {
    // First try to extract from data if it has an explicit object_type field
    if let Ok(data_obj) = serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(message.data.clone()) {
        if let Some(object_type) = data_obj.get("object_type") {
            if let Some(type_str) = object_type.as_str() {
                return type_str.to_string();
            }
        }
    }
    
    // Fallback to using the namespace as object type
    // Convert to PascalCase for consistency with GORC conventions
    capitalize_first_letter(&message.namespace)
}

/// Extracts the channel from a client message for GORC routing.
/// 
/// # Arguments
/// 
/// * `message` - The client message to analyze
/// 
/// # Returns
/// 
/// The channel number (defaults to 0 if not specified)
fn extract_channel_from_message(message: &ClientMessage) -> u8 {
    if let Ok(data_obj) = serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(message.data.clone()) {
        if let Some(channel_value) = data_obj.get("channel") {
            if let Some(channel_num) = channel_value.as_u64() {
                return (channel_num as u8).min(3); // GORC channels are 0-3
            }
        }
    }
    
    // Default to channel 0
    0
}

/// Extracts the instance_uuid from a client message for GORC routing.
/// 
/// # Arguments
/// 
/// * `message` - The client message to analyze
/// 
/// # Returns
/// 
/// A string representing the instance UUID, or generates a new one if not provided
fn extract_instance_uuid_from_message(message: &ClientMessage) -> String {
    if let Ok(data_obj) = serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(message.data.clone()) {
        if let Some(instance_uuid) = data_obj.get("instance_uuid") {
            if let Some(uuid_str) = instance_uuid.as_str() {
                return uuid_str.to_string();
            }
        }
    }
    
    // Generate a new UUID if not provided (for cases where client doesn't specify)
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut hasher = DefaultHasher::new();
    message.namespace.hash(&mut hasher);
    message.event.hash(&mut hasher);
    message.data.to_string().hash(&mut hasher);
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default().as_nanos().hash(&mut hasher);
    
    format!("gen_{:x}", hasher.finish())
}

/// Extracts the object_id from a client message for GORC routing.
/// 
/// # Arguments
/// 
/// * `message` - The client message to analyze
/// 
/// # Returns
/// 
/// A string representing the object ID, or generates one based on message content
fn extract_object_id_from_message(message: &ClientMessage) -> String {
    if let Ok(data_obj) = serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(message.data.clone()) {
        if let Some(object_id) = data_obj.get("object_id") {
            if let Some(id_str) = object_id.as_str() {
                return id_str.to_string();
            }
        }
        // Also check for entity_id as an alternative
        if let Some(entity_id) = data_obj.get("entity_id") {
            if let Some(id_str) = entity_id.as_str() {
                return id_str.to_string();
            }
        }
    }
    
    // Generate a default object_id based on namespace and event
    format!("{}_{}", message.namespace, message.event)
}

/// Capitalizes the first letter of a string for PascalCase conversion.
/// 
/// # Arguments
/// 
/// * `s` - The string to capitalize
/// 
/// # Returns
/// 
/// A new string with the first letter capitalized
fn capitalize_first_letter(s: &str) -> String {
    if s.is_empty() {
        return s.to_string();
    }
    
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}