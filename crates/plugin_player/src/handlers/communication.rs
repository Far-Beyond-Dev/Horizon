//! # Communication Handler
//! 
//! Manages player chat and communication events on GORC channel 2, providing
//! spatial voice communication, text messaging, and social interaction systems
//! within the 300-meter communication range.
//! 
//! ## Channel 2 Characteristics
//! 
//! - **Purpose**: Social communication - text chat, voice, emotes
//! - **Range**: 300m replication radius for local area communication
//! - **Frequency**: Event-driven with moderate priority
//! - **Features**: Multi-channel support, direct messaging, broadcast communication
//! 
//! ## Communication System Design
//! 
//! The communication system provides realistic space communication:
//! 1. **Local Communication**: Ships within 300m can communicate directly
//! 2. **Channel System**: Multiple communication channels (general, emergency, private)
//! 3. **Direct Messaging**: Player-to-player private communication
//! 4. **Broadcast Mode**: Ship-to-all-nearby communication
//! 
//! ## Communication Channels
//! 
//! - **"general"**: General purpose communication (default)
//! - **"emergency"**: Emergency distress signals (high priority)
//! - **"trade"**: Commercial and trading communication  
//! - **"fleet"**: Fleet coordination and tactical communication
//! - **"private"**: Direct player-to-player messaging
//! 
//! ## Security and Moderation
//! 
//! - **Player Ownership**: Players can only send messages as themselves
//! - **Rate Limiting**: Prevents spam and message flooding (future enhancement)
//! - **Content Filtering**: Basic profanity and abuse prevention (future enhancement)
//! - **Message Length**: Enforced maximum message length for network efficiency

use std::sync::Arc;
use horizon_event_system::{
    EventSystem, PlayerId, GorcEvent, GorcObjectId, ClientConnectionRef, ObjectInstance,
    EventError,
};
use tracing::{debug, error};
use serde_json;
use crate::events::PlayerChatRequest;

/// Handles communication requests from players on GORC channel 2.
/// 
/// This handler processes chat messages, validates sender authorization,
/// and broadcasts communication events to nearby ships within the 300m
/// communication range for realistic local area networking.
/// 
/// # Parameters
/// 
/// - `gorc_event`: The GORC event containing chat data
/// - `client_player`: ID of the player sending the message
/// - `_connection`: Client connection (available for future rate limiting)
/// - `_object_instance`: Player's object instance (available for position-based features)
/// - `events`: Event system for broadcasting communication events
/// - `luminal_handle`: Async runtime handle for background processing
/// 
/// # Returns
/// 
/// `Result<(), EventError>` - Success or detailed error information
/// 
/// # Communication Flow
/// 
/// 1. Parse chat request from GORC event data
/// 2. Validate player owns the transmitting ship
/// 3. Apply content filtering and validation
/// 4. Create communication broadcast message
/// 5. Emit to all ships within 300m range on channel 2
/// 6. Log communication event for monitoring
/// 
/// # Example Chat Request
/// 
/// ```json
/// {
///     "player_id": 42,
///     "message": "Requesting docking clearance at Station Alpha",
///     "channel": "general",
///     "target_player": null
/// }
/// ```
/// 
/// # Broadcast Message
/// 
/// ```json
/// {
///     "sender_player": 42,
///     "message": "Requesting docking clearance at Station Alpha",
///     "channel": "general", 
///     "timestamp": "2024-01-15T10:30:45.123Z"
/// }
/// ```
pub async fn handle_communication_request(
    gorc_event: GorcEvent,
    client_player: PlayerId,
    _connection: ClientConnectionRef,
    _object_instance: &mut ObjectInstance,
    events: Arc<EventSystem>,
    luminal_handle: luminal::Handle,
) -> Result<(), EventError> {
    debug!("📡 GORC: Received client communication request from ship {}: {:?}", 
        client_player, gorc_event);
    
    // Parse chat data from GORC event payload
    let event_data = serde_json::from_slice::<serde_json::Value>(&gorc_event.data)
        .map_err(|e| {
            error!("📡 GORC: ❌ Failed to parse JSON from GORC event data: {}", e);
            EventError::HandlerExecution("Invalid JSON in communication request".to_string())
        })?;
    
    let chat_data = serde_json::from_value::<PlayerChatRequest>(event_data)
        .map_err(|e| {
            error!("📡 GORC: ❌ Failed to parse PlayerChatRequest: {}", e);
            EventError::HandlerExecution("Invalid communication request format".to_string())
        })?;
    
    debug!("📡 GORC: Ship {} requests to transmit: '{}'", 
        chat_data.player_id, chat_data.message);
    
    // SECURITY: Validate player ownership - players can only send messages as themselves
    if chat_data.player_id != client_player {
        error!("📡 GORC: ❌ Security violation: Player {} tried to send message as {}", 
            client_player, chat_data.player_id);
        return Err(EventError::HandlerExecution(
            "Unauthorized communication".to_string()
        ));
    }
    
    // Validate and filter the message content
    if let Err(reason) = validate_message_content(&chat_data.message, &chat_data.channel) {
        error!("📡 GORC: ❌ Message validation failed: {}", reason);
        return Err(EventError::HandlerExecution(reason));
    }
    
    // Broadcast communication to nearby ships
    let chat_data_owned = chat_data.clone();
    broadcast_communication(
        &gorc_event.object_id,
        chat_data_owned,
        events,
        luminal_handle,
    ).await;
    
    Ok(())
}

/// Synchronous wrapper for communication request handling that works with GORC client handlers.
///
/// This function provides the same functionality as `handle_communication_request` but in
/// a synchronous context suitable for use with the GORC client event system.
pub fn handle_communication_request_sync(
    gorc_event: GorcEvent,
    client_player: PlayerId,
    _connection: ClientConnectionRef,
    _object_instance: &mut ObjectInstance,
    events: Arc<EventSystem>,
    luminal_handle: luminal::Handle,
) -> Result<(), EventError> {
    debug!("📡 GORC: Received client communication request from ship {}: {:?}", 
        client_player, gorc_event);
    
    // Parse chat data from GORC event payload
    let event_data = serde_json::from_slice::<serde_json::Value>(&gorc_event.data)
        .map_err(|e| {
            error!("📡 GORC: ❌ Failed to parse JSON from GORC event data: {}", e);
            EventError::HandlerExecution("Invalid JSON in communication request".to_string())
        })?;
    
    let chat_data = serde_json::from_value::<PlayerChatRequest>(event_data)
        .map_err(|e| {
            error!("📡 GORC: ❌ Failed to parse PlayerChatRequest: {}", e);
            EventError::HandlerExecution("Invalid communication request format".to_string())
        })?;
    
    debug!("📡 GORC: Ship {} requests to transmit: '{}'", 
        chat_data.player_id, chat_data.message);
    
    // SECURITY: Validate player ownership - players can only send messages as themselves
    if chat_data.player_id != client_player {
        error!("📡 GORC: ❌ Security violation: Player {} tried to send message as {}", 
            client_player, chat_data.player_id);
        return Err(EventError::HandlerExecution(
            "Unauthorized communication".to_string()
        ));
    }
    
    // Validate and filter the message content
    if let Err(reason) = validate_message_content(&chat_data.message, &chat_data.channel) {
        error!("📡 GORC: ❌ Message validation failed: {}", reason);
        return Err(EventError::HandlerExecution(reason));
    }
    
    // Broadcast communication to nearby ships
    let object_id_str = gorc_event.object_id.clone();
    let chat_broadcast = serde_json::json!({
        "sender_player": chat_data.player_id,
        "message": chat_data.message,
        "channel": chat_data.channel,
        "target_player": chat_data.target_player,
        "timestamp": chrono::Utc::now()
    });
    
    if let Ok(gorc_id) = GorcObjectId::from_str(&object_id_str) {
        luminal_handle.spawn(async move {
            if let Err(e) = events.emit_gorc_instance(
                gorc_id, 
                2, // Channel 2: Communication events
                "space_communication", 
                &chat_broadcast, 
                horizon_event_system::Dest::Client
            ).await {
                error!("📡 GORC: ❌ Failed to broadcast communication: {}", e);
            } else {
                debug!("📡 GORC: ✅ Broadcasting communication from ship {} on channel '{}' to ships within 300m", 
                    chat_data.player_id, chat_data.channel);
            }
        });
    } else {
        error!("📡 GORC: ❌ Invalid GORC object ID format: {}", object_id_str);
    }
    
    Ok(())
}

/// Broadcasts communication messages to nearby ships within 300m range.
/// 
/// This function creates a standardized communication message and emits it
/// via the GORC instance event system for automatic spatial replication.
/// 
/// # Parameters
/// 
/// - `object_id_str`: String representation of the transmitting ship's GORC object ID
/// - `chat_data`: The validated communication request data
/// - `events`: Event system for broadcasting
/// - `luminal_handle`: Async runtime handle
/// 
/// # Communication Range
/// 
/// The 300m range provides:
/// - Realistic local area communication for ship-to-ship coordination
/// - Reasonable range for docking requests and local traffic control
/// - Prevents global chat spam while allowing area-based social interaction
/// - Network efficiency by limiting message scope to relevant recipients
/// 
/// # Channel-Specific Behavior
/// 
/// Different channels may have different broadcasting characteristics:
/// - **emergency**: Higher priority, potentially longer range
/// - **private**: Direct player-to-player only (bypasses spatial range)
/// - **general**: Standard 300m spatial range
/// - **trade**: Standard range with potential persistence for trading posts
async fn broadcast_communication(
    object_id_str: &str,
    chat_data: PlayerChatRequest,
    events: Arc<EventSystem>,
    luminal_handle: luminal::Handle,
) {
    // Create communication broadcast payload
    let chat_broadcast = serde_json::json!({
        "sender_player": chat_data.player_id,
        "message": chat_data.message,
        "channel": chat_data.channel,
        "target_player": chat_data.target_player,
        "timestamp": chrono::Utc::now()
    });
    
    // Parse GORC object ID and emit the communication event
    if let Ok(gorc_id) = GorcObjectId::from_str(object_id_str) {
        luminal_handle.spawn(async move {
            // Emit on channel 2 (communication) with 300m replication range
            if let Err(e) = events.emit_gorc_instance(
                gorc_id, 
                2, // Channel 2: Communication events
                "space_communication", 
                &chat_broadcast, 
                horizon_event_system::Dest::Client
            ).await {
                error!("📡 GORC: ❌ Failed to broadcast communication: {}", e);
            } else {
                debug!("📡 GORC: ✅ Broadcasting communication from ship {} on channel '{}' to ships within 300m", 
                    chat_data.player_id, chat_data.channel);
            }
        });
    } else {
        error!("📡 GORC: ❌ Invalid GORC object ID format: {}", object_id_str);
    }
}

/// Validates message content for appropriate communication.
/// 
/// This function performs content validation and filtering:
/// - Message length limits
/// - Channel-appropriate content validation
/// - Basic profanity filtering (future enhancement)
/// - Spam detection (future enhancement)
/// 
/// # Parameters
/// 
/// - `message`: The message content to validate
/// - `channel`: The communication channel being used
/// 
/// # Returns
/// 
/// `Result<(), String>` - Ok if valid, Err with reason if invalid
/// 
/// # Validation Rules
/// 
/// - **Maximum Length**: 500 characters for network efficiency
/// - **Minimum Length**: 1 character (no empty messages)
/// - **Valid Channels**: Must be a supported communication channel
/// - **Content Policy**: No malicious or inappropriate content (future)
pub fn validate_message_content(message: &str, channel: &str) -> Result<(), String> {
    // Check message length constraints
    if message.is_empty() {
        return Err("Message cannot be empty".to_string());
    }
    
    if message.len() > 500 {
        return Err(format!("Message too long: {} characters (max 500)", message.len()));
    }
    
    // Validate channel is supported
    let valid_channels = ["general", "emergency", "trade", "fleet", "private"];
    if !valid_channels.contains(&channel) {
        return Err(format!("Invalid communication channel: {}", channel));
    }
    
    // Future enhancements:
    // - Profanity filtering
    // - Spam detection and rate limiting
    // - Content moderation and reporting
    // - Language detection and translation
    
    Ok(())
}

/// Handles special communication channel behaviors and routing.
/// 
/// Different channels may require special handling:
/// - Emergency channels might have extended range
/// - Private messages bypass spatial range restrictions
/// - Fleet channels might integrate with group systems
/// 
/// # Parameters
/// 
/// - `channel`: The communication channel
/// - `sender`: The player sending the message
/// - `target_player`: Optional target for direct messaging
/// 
/// # Returns
/// 
/// `CommunicationBehavior` - Configuration for how to handle this message
pub enum CommunicationBehavior {
    /// Standard spatial broadcast within configured range
    Spatial { range: f32 },
    /// Direct message between two specific players
    Direct { target: PlayerId },
    /// Emergency broadcast with extended range and priority
    Emergency { range: f32, priority: bool },
    /// Fleet communication to group members only
    Fleet { fleet_id: Option<u32> },
}

pub fn determine_communication_behavior(
    channel: &str,
    _sender: PlayerId,
    target_player: Option<PlayerId>,
) -> CommunicationBehavior {
    match channel {
        "emergency" => CommunicationBehavior::Emergency { 
            range: 1000.0, // Extended emergency range
            priority: true 
        },
        "private" => {
            if let Some(target) = target_player {
                CommunicationBehavior::Direct { target }
            } else {
                CommunicationBehavior::Spatial { range: 300.0 }
            }
        },
        "fleet" => CommunicationBehavior::Fleet { 
            fleet_id: None // Future: integrate with fleet system
        },
        _ => CommunicationBehavior::Spatial { range: 300.0 }, // Default behavior
    }
}