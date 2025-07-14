//! Message type definitions for client-server communication.
//!
//! This module defines the structure of messages exchanged between
//! clients and the server, providing a standardized format for
//! plugin communication.

use serde::{Deserialize, Serialize};

/// A message sent from a client to the server.
/// 
/// This structure defines the standard format for all client messages,
/// using a namespace/event pattern to route messages to appropriate plugins.
/// 
/// # Fields
/// 
/// * `namespace` - The plugin namespace (e.g., "movement", "chat", "inventory")
/// * `event` - The specific event within the namespace (e.g., "move_request", "send_message")
/// * `data` - The payload data for the event as a JSON value
/// 
/// # Example
/// 
/// ```json
/// {
///   "namespace": "movement",
///   "event": "move_request",
///   "data": {
///     "target_x": 100.0,
///     "target_y": 200.0,
///     "target_z": 0.0
///   }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientMessage {
    /// The plugin namespace that should handle this message
    pub namespace: String,
    
    /// The specific event type within the namespace
    pub event: String,
    
    /// The message payload as a JSON value
    pub data: serde_json::Value,
}