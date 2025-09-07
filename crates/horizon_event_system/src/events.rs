//! # Event Traits and Core Events
//!
//! This module defines the core event infrastructure and all built-in event types
//! for the Horizon Event System. It includes the fundamental [`Event`] trait,
//! handler abstractions, and all server infrastructure events.
//!
//! ## Event Categories
//!
//! ### Core Events
//! Infrastructure events that are essential for server operation:
//! - Player connection/disconnection events
//! - Plugin lifecycle events  
//! - Region management events
//!
//! ### Client Message Events
//! Raw messages from game clients that need to be routed to plugins for processing.
//!
//! ## Design Principles
//!
//! - **Type Safety**: All events are strongly typed with compile-time guarantees
//! - **Serialization**: Built-in JSON serialization for network transmission
//! - **Performance**: Efficient serialization and handler dispatch
//! - **Extensibility**: Easy to add new event types by implementing [`Event`]

use crate::types::{PlayerId, RegionId, RegionBounds, DisconnectReason, AuthenticationStatus};
use async_trait::async_trait;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{any::{Any, TypeId}, fmt::Debug};

// ============================================================================
// Event Traits and Core Infrastructure
// ============================================================================

/// Core trait that all events must implement.
/// 
/// This trait provides the fundamental capabilities needed for type-safe event handling:
/// - Serialization for network transmission
/// - Type identification for routing
/// - Dynamic typing support for generic handlers
/// 
/// Most types will automatically implement this trait through the blanket implementation
/// if they implement the required marker traits.
/// 
/// # Safety
/// 
/// Events must be Send + Sync as they may be processed across multiple threads.
/// The Debug requirement ensures events can be logged for debugging purposes.
pub trait Event: Send + Sync + Any + std::fmt::Debug {
    /// Returns the type name of this event for debugging and routing.
    /// 
    /// This should return a stable, unique identifier for the event type.
    fn type_name() -> &'static str
    where
        Self: Sized;
    
    /// Serializes the event to bytes for network transmission or storage.
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(Vec<u8>)` containing the serialized event data, or
    /// `Err(EventError)` if serialization fails.
    fn serialize(&self) -> Result<Vec<u8>, EventError>;
    
    /// Deserializes an event from bytes.
    /// 
    /// # Arguments
    /// 
    /// * `data` - Byte slice containing serialized event data
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(Self)` with the deserialized event, or `Err(EventError)`
    /// if deserialization fails.
    fn deserialize(data: &[u8]) -> Result<Self, EventError>
    where
        Self: Sized;
    
    /// Returns a reference to this event as `&dyn Any` for dynamic typing.
    /// 
    /// This enables runtime type checking and downcasting when needed.
    fn as_any(&self) -> &dyn Any;
}

/// Blanket implementation of Event trait for types that meet the requirements.
/// 
/// Any type that implements Serialize + DeserializeOwned + Send + Sync + Any + Debug
/// automatically gets Event implementation with JSON serialization.
/// 
/// This makes it very easy to create new event types - just derive the required traits:
/// 
/// ```rust
/// #[derive(Debug, Serialize, Deserialize)]
/// struct MyEvent {
///     data: String,
/// }
/// // MyEvent now implements Event automatically!
/// ```
impl<T> Event for T
where
    T: Serialize + DeserializeOwned + Send + Sync + Any + std::fmt::Debug + 'static,
{
    fn type_name() -> &'static str {
        std::any::type_name::<T>()
    }

    fn serialize(&self) -> Result<Vec<u8>, EventError> {
        serde_json::to_vec(self).map_err(|e| {
            let type_name = Self::type_name();
            tracing::error!(
                "🔴 Event serialization failed for type '{}': {} (event debug: {:?})",
                type_name,
                e,
                self
            );
            EventError::Serialization(e)
        })
    }

    fn deserialize(data: &[u8]) -> Result<Self, EventError> {
        serde_json::from_slice(data).map_err(|e| {
            let type_name = Self::type_name();
            let data_preview = if data.len() > 200 {
                format!("{}... (truncated {} bytes)", 
                    String::from_utf8_lossy(&data[..200]), 
                    data.len() - 200)
            } else {
                String::from_utf8_lossy(data).to_string()
            };
            
            tracing::error!(
                "🔴 Event deserialization failed for type '{}': {} (data length: {} bytes, content preview: '{}')",
                type_name,
                e,
                data.len(),
                data_preview
            );
            EventError::Deserialization(e)
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Handler trait for processing events asynchronously.
/// 
/// This trait abstracts over the type-specific event handling logic and provides
/// a uniform interface for the event system to call handlers.
/// 
/// Most users will not implement this trait directly, but instead use the
/// `TypedEventHandler` wrapper or the registration macros.
#[async_trait]
pub trait EventHandler: Send + Sync + 'static + Debug {
    /// Handles an event from serialized data.
    /// 
    /// # Arguments
    /// 
    /// * `data` - Serialized event data
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if the event was handled successfully, or `Err(EventError)`
    /// if handling failed.
    async fn handle(&self, data: &[u8]) -> Result<(), EventError>;
    
    /// Returns the TypeId of the event type this handler expects.
    /// 
    /// This is used for type checking and routing to ensure events are only
    /// sent to compatible handlers.
    fn expected_type_id(&self) -> TypeId;
    
    /// Returns a human-readable name for this handler for debugging.
    fn handler_name(&self) -> &str;
}

/// Type-safe wrapper for event handlers.
/// 
/// This struct bridges between the generic `EventHandler` trait and specific
/// event types, providing compile-time type safety while allowing runtime
/// polymorphism.
/// 
/// # Type Parameters
/// 
/// * `T` - The event type this handler processes
/// * `F` - The function type that handles the event
/// 
/// # Examples
/// 
/// ```rust
/// let handler = TypedEventHandler::new(
///     "my_handler".to_string(),
///     |event: MyEvent| {
///         println!("Received: {:?}", event);
///         Ok(())
///     }
/// );
/// ```
pub struct TypedEventHandler<T, F>
where
    T: Event,
    F: Fn(T) -> Result<(), EventError> + Send + Sync,
{
    handler: F,
    name: String,
    _phantom: std::marker::PhantomData<T>,
}

// Implement Clone for TypedEventHandler if F is Clone
impl<T, F> Clone for TypedEventHandler<T, F>
where
    T: Event,
    F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone,
{
    fn clone(&self) -> Self {
        Self {
            handler: self.handler.clone(),
            name: self.name.clone(),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T, F> std::fmt::Debug for TypedEventHandler<T, F>
where
    T: Event,
    F: Fn(T) -> Result<(), EventError> + Send + Sync,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypedEventHandler")
            .field("name", &self.name)
            .finish()
    }
}

impl<T, F> TypedEventHandler<T, F>
where
    T: Event,
    F: Fn(T) -> Result<(), EventError> + Send + Sync,
{
    /// Creates a new typed event handler.
    /// 
    /// # Arguments
    /// 
    /// * `name` - Human-readable name for debugging
    /// * `handler` - Function to handle events of type T
    pub fn new(name: String, handler: F) -> Self {
        Self {
            handler,
            name,
            _phantom: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<T, F> EventHandler for TypedEventHandler<T, F>
where
    T: Event,
    F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
{
    async fn handle(&self, data: &[u8]) -> Result<(), EventError> {
        match T::deserialize(data) {
            Ok(event) => (self.handler)(event),
            Err(e) => {
                // Enhanced logging for deserialization failures (type mismatches)
                let expected_type = std::any::type_name::<T>();
                let data_preview = if data.len() > 100 {
                    format!("{}... ({} more bytes)", 
                        String::from_utf8_lossy(&data[..100]), 
                        data.len() - 100)
                } else {
                    String::from_utf8_lossy(data).to_string()
                };
                
                tracing::warn!(
                    "🟡 EventHandler '{}' (expects type '{}'): Deserialization failed - {}. Data preview: '{}'. This is likely a type mismatch and the handler will be skipped.",
                    self.name,
                    expected_type,
                    e,
                    data_preview
                );
                Ok(())
            }
        }
    }

    fn expected_type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    fn handler_name(&self) -> &str {
        &self.name
    }
}

// ============================================================================
// Core Server Events ONLY
// ============================================================================

/// Event emitted when a player connects to the server.
/// 
/// This is a core infrastructure event that provides essential information
/// about new player connections. It's typically used for:
/// - Initializing player data structures
/// - Setting up player-specific resources
/// - Logging connection activity
/// - Updating player count statistics
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::{PlayerConnectedEvent, PlayerId, current_timestamp};
/// 
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// #     let events = horizon_event_system::create_horizon_event_system();
/// events.emit_core("player_connected", &PlayerConnectedEvent {
///     player_id: PlayerId::new(),
///     connection_id: "conn_abc123".to_string(),
///     remote_addr: "192.168.1.100:45678".to_string(),
///     timestamp: current_timestamp(),
/// }).await?;
/// #     Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerConnectedEvent {
    /// Unique identifier for the player
    pub player_id: PlayerId,
    /// Connection-specific identifier for this session
    pub connection_id: String,
    /// Remote address of the client connection
    pub remote_addr: String,
    /// Unix timestamp when the connection was established
    pub timestamp: u64,
}

/// Event emitted when a player disconnects from the server.
/// 
/// This event provides information about player disconnections including
/// the reason for disconnection. It's used for:
/// - Cleaning up player resources
/// - Saving player state
/// - Logging disconnection activity
/// - Updating player count statistics
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::{PlayerDisconnectedEvent, PlayerId, DisconnectReason, current_timestamp};
/// 
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// #     let events = horizon_event_system::create_horizon_event_system();
/// #     let player_id = PlayerId::new();
/// #     let connection_id = "conn_123".to_string();
/// events.emit_core("player_disconnected", &PlayerDisconnectedEvent {
///     player_id: player_id,
///     connection_id: connection_id,
///     reason: DisconnectReason::ClientDisconnect,
///     timestamp: current_timestamp(),
/// }).await?;
/// #     Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerDisconnectedEvent {
    /// Unique identifier for the player
    pub player_id: PlayerId,
    /// Connection-specific identifier for the session
    pub connection_id: String,
    /// Reason for the disconnection
    pub reason: DisconnectReason,
    /// Unix timestamp when the disconnection occurred
    pub timestamp: u64,
}

/// Event emitted to set the authentication status of a player.
/// 
/// This event allows backend plugins to set the authentication status
/// of a connected player. It provides a standardized way to manage
/// authentication state across the plugin ecosystem.
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::{AuthenticationStatusSetEvent, PlayerId, AuthenticationStatus, current_timestamp};
/// 
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// #     let events = horizon_event_system::create_horizon_event_system();
/// events.emit_core("auth_status_set", &AuthenticationStatusSetEvent {
///     player_id: PlayerId::new(),
///     status: AuthenticationStatus::Authenticated,
///     timestamp: current_timestamp(),
/// }).await?;
/// #     Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticationStatusSetEvent {
    /// Unique identifier for the player
    pub player_id: PlayerId,
    /// The authentication status to set
    pub status: AuthenticationStatus,
    /// Unix timestamp when the status was set
    pub timestamp: u64,
}

/// Event emitted to query the authentication status of a player.
/// 
/// This event allows plugins to request the current authentication status
/// of a player. The response is typically handled through a separate
/// mechanism or callback.
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::{AuthenticationStatusGetEvent, PlayerId, current_timestamp};
/// 
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// #     let events = horizon_event_system::create_horizon_event_system();
/// events.emit_core("auth_status_get", &AuthenticationStatusGetEvent {
///     player_id: PlayerId::new(),
///     request_id: Some("req_123".to_string()),
///     timestamp: current_timestamp(),
/// }).await?;
/// #     Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticationStatusGetEvent {
    /// Unique identifier for the player
    pub player_id: PlayerId,
    /// Request ID for correlating responses
    pub request_id: String,
    /// Unix timestamp when the query was made
    pub timestamp: u64,
}

/// Event emitted in response to authentication status queries.
/// 
/// This event provides the response to an `AuthenticationStatusGetEvent`,
/// containing the current authentication status of the requested player.
/// Plugins can register handlers for this event to receive query responses.
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::{AuthenticationStatusGetResponseEvent, PlayerId, AuthenticationStatus, current_timestamp};
/// 
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// #     let events = horizon_event_system::create_horizon_event_system();
/// events.emit_core("auth_status_get_response", &AuthenticationStatusGetResponseEvent {
///     player_id: PlayerId::new(),
///     request_id: "req_123".to_string(),
///     status: Some(AuthenticationStatus::Authenticated),
///     timestamp: current_timestamp(),
/// }).await?;
/// #     Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticationStatusGetResponseEvent {
    /// Unique identifier for the player
    pub player_id: PlayerId,
    /// Request ID for correlating with the original query
    pub request_id: String,
    /// Current authentication status (None if player not found)
    pub status: Option<AuthenticationStatus>,
    /// Unix timestamp when the response was generated
    pub timestamp: u64,
}

/// Event emitted when a player's authentication status changes.
/// 
/// This event notifies all interested plugins when a player's authentication
/// status has changed, allowing them to react appropriately to authentication
/// state transitions.
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::{AuthenticationStatusChangedEvent, PlayerId, AuthenticationStatus, current_timestamp};
/// 
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// #     let events = horizon_event_system::create_horizon_event_system();
/// events.emit_core("auth_status_changed", &AuthenticationStatusChangedEvent {
///     player_id: PlayerId::new(),
///     old_status: AuthenticationStatus::Authenticating,
///     new_status: AuthenticationStatus::Authenticated,
///     timestamp: current_timestamp(),
/// }).await?;
/// #     Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticationStatusChangedEvent {
    /// Unique identifier for the player
    pub player_id: PlayerId,
    /// Previous authentication status
    pub old_status: AuthenticationStatus,
    /// New authentication status
    pub new_status: AuthenticationStatus,
    /// Unix timestamp when the status changed
    pub timestamp: u64,
}

/// Event emitted when a player's position is updated.
/// 
/// This is a core server event that standardizes player movement data across all systems.
/// Plugins should emit this event after parsing client movement data, and core systems 
/// like GORC can subscribe to receive standardized position updates.
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::{PlayerMovementEvent, Vec3, PlayerId, current_timestamp};
/// 
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// #     let events = horizon_event_system::create_horizon_event_system();
/// #     let player_id = PlayerId::new();
/// events.emit_core("player_movement", &PlayerMovementEvent {
///     player_id,
///     old_position: Some(Vec3::new(100.0, 0.0, 200.0)),
///     new_position: Vec3::new(110.0, 0.0, 205.0),
///     timestamp: current_timestamp(),
/// }).await?;
/// #     Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerMovementEvent {
    /// Unique identifier for the player
    pub player_id: PlayerId,
    /// Previous position (if known)
    pub old_position: Option<crate::types::Vec3>,
    /// New position 
    pub new_position: crate::types::Vec3,
    /// Unix timestamp when the movement occurred
    pub timestamp: u64,
}

/// Event emitted when a plugin is successfully loaded.
/// 
/// This event signals that a plugin has been loaded into the server and
/// is ready to process events. It includes metadata about the plugin's
/// capabilities and version information.
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::{PluginLoadedEvent, current_timestamp};
/// 
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// #     let events = horizon_event_system::create_horizon_event_system();
/// events.emit_core("plugin_loaded", &PluginLoadedEvent {
///     plugin_name: "combat_system".to_string(),
///     version: "2.1.0".to_string(),
///     capabilities: vec!["damage_calculation".to_string(), "status_effects".to_string()],
///     timestamp: current_timestamp(),
/// }).await?;
/// #     Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginLoadedEvent {
    /// Name of the loaded plugin
    pub plugin_name: String,
    /// Version string of the plugin
    pub version: String,
    /// List of capabilities or features provided by this plugin
    pub capabilities: Vec<String>,
    /// Unix timestamp when the plugin was loaded
    pub timestamp: u64,
}

/// Event emitted when a plugin is unloaded from the server.
/// 
/// This event indicates that a plugin has been cleanly unloaded and
/// should no longer receive events or process requests.
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::{PluginUnloadedEvent, current_timestamp};
/// 
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// #     let events = horizon_event_system::create_horizon_event_system();
/// events.emit_core("plugin_unloaded", &PluginUnloadedEvent {
///     plugin_name: "old_combat_system".to_string(),
///     timestamp: current_timestamp(),
/// }).await?;
/// #     Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginUnloadedEvent {
    /// Name of the unloaded plugin
    pub plugin_name: String,
    /// Unix timestamp when the plugin was unloaded
    pub timestamp: u64,
}

/// Event emitted when a game region is started.
/// 
/// Regions are logical areas of the game world that can be managed
/// independently. This event indicates that a region is now active
/// and ready to accept players and process game logic.
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::{RegionStartedEvent, RegionId, RegionBounds, current_timestamp};
/// 
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// #     let events = horizon_event_system::create_horizon_event_system();
/// events.emit_core("region_started", &RegionStartedEvent {
///     region_id: RegionId::new(),
///     bounds: RegionBounds {
///         min_x: -1000.0, max_x: 1000.0,
///         min_y: 0.0, max_y: 256.0,
///         min_z: -1000.0, max_z: 1000.0,
///     },
///     timestamp: current_timestamp(),
/// }).await?;
/// #     Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionStartedEvent {
    /// Unique identifier for the region
    pub region_id: RegionId,
    /// Spatial boundaries of the region
    pub bounds: RegionBounds,
    /// Unix timestamp when the region was started
    pub timestamp: u64,
}

/// Event emitted when a game region is stopped.
/// 
/// This event indicates that a region is no longer active and players
/// should be evacuated or transferred to other regions.
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::{RegionStoppedEvent, RegionId, current_timestamp};
/// 
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// #     let events = horizon_event_system::create_horizon_event_system();
/// #     let region_id = RegionId::new();
/// events.emit_core("region_stopped", &RegionStoppedEvent {
///     region_id: region_id,
///     timestamp: current_timestamp(),
/// }).await?;
/// #     Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionStoppedEvent {
    /// Unique identifier for the region
    pub region_id: RegionId,
    /// Unix timestamp when the region was stopped
    pub timestamp: u64,
}

/// Raw client message event for routing to plugins.
/// 
/// This event represents unprocessed messages received from game clients.
/// It serves as a bridge between the core networking layer and game plugins,
/// allowing plugins to handle different types of client messages without
/// the core system needing to understand the message formats.
/// 
/// The event contains the raw binary data along with metadata about the
/// message type and sender. Plugins can register for specific message types
/// and deserialize the data according to their own protocols.
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::{RawClientMessageEvent, PlayerId, current_timestamp};
/// 
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// #     let events = horizon_event_system::create_horizon_event_system();
/// #     let player_id = PlayerId::new();
/// // Emit a raw client message for plugin processing
/// events.emit_client("movement", "position_update", &RawClientMessageEvent {
///     player_id: player_id,
///     message_type: "move".to_string(),
///     data: serde_json::json!({"x": 100, "y": 200}),
///     timestamp: current_timestamp(),
/// }).await?;
/// #     Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawClientMessageEvent {
    /// ID of the player who sent the message
    pub player_id: PlayerId,
    /// Type identifier for the message (e.g., "move", "chat", "action")
    pub message_type: String,
    /// Raw binary message data
    pub data: Vec<u8>,
    /// Unix timestamp when the message was received
    pub timestamp: u64,
}

/// GORC (Game Object Replication Channels) event for object state replication.
/// 
/// This event represents a change in game object state that needs to be
/// replicated to interested observers through the GORC system. It contains
/// the object identifier, serialized state data, and metadata about the
/// replication context.
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::{GorcEvent, ReplicationPriority, current_timestamp};
/// 
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// #     let events = horizon_event_system::create_horizon_event_system();
/// #     let asteroid_id = "asteroid_123";
/// // Emit a GORC event for asteroid position update
/// events.emit_gorc("Asteroid", 0, "position_update", &GorcEvent {
///     object_id: asteroid_id.to_string(),
///     object_type: "Asteroid".to_string(),
///     channel: 0,
///     data: serde_json::json!({"x": 100.0, "y": 200.0, "z": 300.0}),
///     priority: ReplicationPriority::Critical,
///     timestamp: current_timestamp(),
/// }).await?;
/// #     Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GorcEvent {
    /// Unique identifier for the object being replicated
    pub object_id: String,
    /// UUID for the specific instance of the registered GORC object
    pub instance_uuid: String,
    /// Type of the object (e.g., "Asteroid", "Player", "Ship")
    pub object_type: String,
    /// Replication channel (0=Critical, 1=Detailed, 2=Cosmetic, 3=Metadata)
    pub channel: u8,
    /// Serialized object state data for this update
    pub data: Vec<u8>,
    /// Priority level for this replication update
    pub priority: String, // We'll use String to avoid circular dependency
    /// Unix timestamp when the event was created
    pub timestamp: u64,
}

/// Destination enum for GORC event emission
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dest {
    /// Only trigger client-side replication (send to subscribed clients)
    Client,
    /// Only trigger server-side handlers (process on server)
    Server,
    /// Trigger both client replication and server handlers
    Both,
    /// Do nothing (useful for conditional logic)
    None,
}

impl Default for Dest {
    fn default() -> Self {
        Dest::Both
    }
}

// ============================================================================
// Client Event Wrapper Types
// ============================================================================

/// Wrapper type for client events that include connection context.
/// 
/// When client events are emitted with context (via `emit_client_with_context`),
/// they are wrapped with player information. This wrapper type allows handlers
/// to work with strongly typed events while still accessing the connection context.
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::{ClientEventWrapper, PlayerId};
/// use serde::{Deserialize, Serialize};
/// 
/// #[derive(Debug, Clone, Serialize, Deserialize)]
/// struct ChatMessage {
///     channel: String,
///     message: String,
/// }
/// 
/// // Handler that receives the wrapped event
/// events.on_client_typed("chat", "message", 
///     |wrapper: ClientEventWrapper<ChatMessage>| {
///         println!("Player {} said: {}", wrapper.player_id, wrapper.data.message);
///         Ok(())
///     }
/// ).await?;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientEventWrapper<T> {
    /// The player ID of the client that sent this event
    pub player_id: crate::types::PlayerId,
    /// The actual event data
    pub data: T,
}

impl<T> ClientEventWrapper<T> {
    /// Creates a new client event wrapper.
    pub fn new(player_id: crate::types::PlayerId, data: T) -> Self {
        Self { player_id, data }
    }

    /// Extracts the inner event data, consuming the wrapper.
    pub fn into_data(self) -> T {
        self.data
    }

    /// Gets a reference to the inner event data.
    pub fn data(&self) -> &T {
        &self.data
    }

    /// Gets a mutable reference to the inner event data.
    pub fn data_mut(&mut self) -> &mut T {
        &mut self.data
    }
}

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during event system operations.
/// 
/// This enum covers all possible error conditions in the event system,
/// from serialization failures to handler execution errors.
#[derive(Debug, thiserror::Error)]
pub enum EventError {
    /// Serialization failed when converting event to bytes
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    /// Deserialization failed when converting bytes to event
    #[error("Deserialization error: {0}")]
    Deserialization(serde_json::Error),
    /// No handler found for the specified event type
    #[error("Handler not found: {0}")]
    HandlerNotFound(String),
    /// Handler execution failed during event processing
    #[error("Handler execution error: {0}")]
    HandlerExecution(String),
    /// Runtime error when dealing with async operations
    #[error("Runtime error: {0}")]
    RuntimeError(String),
    #[error("An unexpected error occurred: {0}")]
    Other(String),
}

// Tests module
mod tests;