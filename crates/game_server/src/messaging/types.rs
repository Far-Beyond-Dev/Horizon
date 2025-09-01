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
/// # Examples
/// 
/// Standard movement message:
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
/// 
/// GORC message (routed to both client and GORC handlers due to instance_uuid):
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
/// The presence of `instance_uuid` in the data determines GORC routing behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientMessage {
    /// The plugin namespace that should handle this message
    pub namespace: String,
    
    /// The specific event type within the namespace
    pub event: String,
    
    /// The message payload as a JSON value
    pub data: serde_json::Value,
}