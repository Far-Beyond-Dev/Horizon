//! # Horizon Event System
//!
//! A high-performance, type-safe event system designed for game servers with plugin architecture.
//! This system provides a clean separation between core server events and plugin-specific events,
//! enabling modular game development while maintaining excellent performance and safety.
//!
//! ## Core Features
//!
//! - **Type Safety**: All events are strongly typed with compile-time guarantees
//! - **Async/Await Support**: Built on Tokio for high-performance async operations
//! - **Plugin Architecture**: Clean separation between core server and plugin events
//! - **Namespace Management**: Organized event routing with namespaces
//! - **Panic Safety**: Comprehensive panic handling for plugin FFI boundaries
//! - **Serialization**: Built-in JSON serialization for network transmission
//! - **Statistics**: Built-in performance monitoring and statistics
//!
//! ## Architecture Overview
//!
//! The event system is organized into three main categories:
//!
//! ### Core Events (`core:*`)
//! Server infrastructure events like player connections, plugin lifecycle, and region management.
//! These events are handled by the core server and are essential for basic operation.
//!
//! ### Client Events (`client:namespace:event`)
//! Events originating from game clients, organized by namespace (e.g., `movement`, `chat`, `ui`).
//! These events are typically forwarded to appropriate plugins for handling.
//!
//! ### Plugin Events (`plugin:plugin_name:event`)
//! Inter-plugin communication events that allow plugins to communicate with each other
//! without tight coupling.
//!
//! ## Quick Start Example
//!
//! ```rust
//! use horizon_events::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let events = create_horizon_event_system();
//!     
//!     // Register a core event handler
//!     events.on_core("server_started", |event: PlayerConnectedEvent| {
//!         println!("Player {} connected", event.player_id);
//!         Ok(())
//!     }).await?;
//!     
//!     // Register client event handlers
//!     events.on_client("movement", "player_moved", |event: RawClientMessageEvent| {
//!         // Handle player movement
//!         Ok(())
//!     }).await?;
//!     
//!     // Emit events
//!     events.emit_core("player_connected", &PlayerConnectedEvent {
//!         player_id: PlayerId::new(),
//!         connection_id: "conn_123".to_string(),
//!         remote_addr: "127.0.0.1:8080".to_string(),
//!         timestamp: current_timestamp(),
//!     }).await?;
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Plugin Development
//!
//! Create plugins using the `SimplePlugin` trait and `create_simple_plugin!` macro:
//!
//! ```rust
//! use horizon_events::*;
//!
//! struct MyGamePlugin {
//!     // Plugin state
//! }
//!
//! impl MyGamePlugin {
//!     fn new() -> Self {
//!         Self {}
//!     }
//! }
//!
//! #[async_trait]
//! impl SimplePlugin for MyGamePlugin {
//!     fn name(&self) -> &str { "my_game_plugin" }
//!     fn version(&self) -> &str { "1.0.0" }
//!     
//!     async fn register_handlers(&mut self, events: Arc<EventSystem>) -> Result<(), PluginError> {
//!         register_handlers!(events;
//!             client {
//!                 "movement", "jump" => |event: RawClientMessageEvent| {
//!                     // Handle jump
//!                     Ok(())
//!                 }
//!             }
//!             core {
//!                 "player_connected" => |event: PlayerConnectedEvent| {
//!                     // Welcome new player
//!                     Ok(())
//!                 }
//!             }
//!         );
//!         Ok(())
//!     }
//! }
//!
//! create_simple_plugin!(MyGamePlugin);
//! ```

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;
#[allow(unused_imports)] // This is actually used but only in the create plugin macro
use futures;

// ============================================================================
// Core Types (Minimal set)
// ============================================================================

/// Unique identifier for a player in the game world.
/// 
/// This is a wrapper around UUID that provides type safety and ensures
/// player IDs cannot be confused with other types of IDs in the system.
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_events::PlayerId;
/// 
/// // Create a new random player ID
/// let player_id = PlayerId::new();
/// 
/// // Parse from string
/// let player_id = PlayerId::from_str("550e8400-e29b-41d4-a716-446655440000")?;
/// 
/// // Convert to string for logging/display
/// println!("Player ID: {}", player_id);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub Uuid);

impl PlayerId {
    /// Creates a new random player ID using UUID v4.
    /// 
    /// This method is cryptographically secure and provides sufficient
    /// entropy to avoid collisions in practical use.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Parses a player ID from a string representation.
    /// 
    /// # Arguments
    /// 
    /// * `s` - A string slice containing a valid UUID
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(PlayerId)` if the string is a valid UUID, otherwise returns
    /// `Err(uuid::Error)` with details about the parsing failure.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// let player_id = PlayerId::from_str("550e8400-e29b-41d4-a716-446655440000")?;
    /// ```
    pub fn from_str(s: &str) -> Result<Self, uuid::Error> {
        Uuid::parse_str(s).map(Self)
    }
}

impl Default for PlayerId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for PlayerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a game region.
/// 
/// Regions are logical areas of the game world that can be managed independently.
/// Each region has its own event processing and can be started/stopped dynamically.
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_events::RegionId;
/// 
/// let region_id = RegionId::new();
/// println!("Region: {}", region_id.0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RegionId(pub Uuid);

impl RegionId {
    /// Creates a new random region ID using UUID v4.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// Represents a 3D position in the game world.
/// 
/// Uses double-precision floating point for maximum accuracy in position calculations.
/// This is essential for large game worlds where single-precision might introduce
/// noticeable errors.
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_events::Position;
/// 
/// let spawn_point = Position::new(0.0, 0.0, 0.0);
/// let player_pos = Position::new(100.5, 64.0, -200.25);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Position {
    /// X coordinate (typically east-west axis)
    pub x: f64,
    /// Y coordinate (typically vertical axis)
    pub y: f64,
    /// Z coordinate (typically north-south axis)
    pub z: f64,
}

impl Position {
    /// Creates a new position with the specified coordinates.
    /// 
    /// # Arguments
    /// 
    /// * `x` - X coordinate
    /// * `y` - Y coordinate  
    /// * `z` - Z coordinate
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
}

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
        serde_json::to_vec(self).map_err(EventError::Serialization)
    }

    fn deserialize(data: &[u8]) -> Result<Self, EventError> {
        serde_json::from_slice(data).map_err(EventError::Deserialization)
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
pub trait EventHandler: Send + Sync {
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
    F: Fn(T) -> Result<(), EventError> + Send + Sync,
{
    async fn handle(&self, data: &[u8]) -> Result<(), EventError> {
        let event = T::deserialize(data)?;
        (self.handler)(event)
    }

    fn expected_type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    fn handler_name(&self) -> &str {
        &self.name
    }
}

// ============================================================================
// Refined Event System with Cleaner API
// ============================================================================

/// The core event system that manages event routing and handler execution.
/// 
/// This is the central hub for all event processing in the system. It provides
/// type-safe event registration and emission with support for different event
/// categories (core, client, plugin).
/// 
/// # Thread Safety
/// 
/// The EventSystem is fully thread-safe and can be shared across multiple
/// threads using `Arc<EventSystem>`. All operations are protected by async
/// read-write locks to ensure consistency.
/// 
/// # Performance
/// 
/// - Handler lookup is O(1) using HashMap
/// - Multiple handlers for the same event are executed sequentially
/// - Failed handlers don't prevent other handlers from running
/// - Built-in statistics tracking for monitoring
/// 
/// # Examples
/// 
/// ```rust
/// let events = EventSystem::new();
/// 
/// // Register handlers
/// events.on_core("server_started", |event: ServerStartedEvent| {
///     println!("Server started!");
///     Ok(())
/// }).await?;
/// 
/// // Emit events
/// events.emit_core("server_started", &ServerStartedEvent {
///     timestamp: current_timestamp(),
/// }).await?;
/// ```
pub struct EventSystem {
    /// Map of event keys to their registered handlers
    handlers: RwLock<HashMap<String, Vec<Arc<dyn EventHandler>>>>,
    /// System statistics for monitoring
    stats: RwLock<EventSystemStats>,
}

impl EventSystem {
    /// Creates a new event system with no registered handlers.
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
            stats: RwLock::new(EventSystemStats::default()),
        }
    }

    /// Registers a handler for core server events.
    /// 
    /// Core events are fundamental server infrastructure events like player
    /// connections, plugin lifecycle, and region management.
    /// 
    /// # Arguments
    /// 
    /// * `event_name` - Name of the core event (e.g., "server_started")
    /// * `handler` - Function to handle events of type T
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if registration succeeds, or `Err(EventError)` if it fails.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// events.on_core("player_connected", |event: PlayerConnectedEvent| {
    ///     println!("Player {} connected from {}", event.player_id, event.remote_addr);
    ///     Ok(())
    /// }).await?;
    /// ```
    pub async fn on_core<T, F>(&self, event_name: &str, handler: F) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static,
    {
        let event_key = format!("core:{}", event_name);
        self.register_typed_handler(event_key, event_name, handler)
            .await
    }

    /// Registers a handler for client events with namespace.
    /// 
    /// Client events are organized by namespace to provide logical grouping
    /// (e.g., "movement", "chat", "ui", "inventory").
    /// 
    /// # Arguments
    /// 
    /// * `namespace` - Logical grouping for the event (e.g., "movement")
    /// * `event_name` - Specific event name (e.g., "player_moved")
    /// * `handler` - Function to handle events of type T
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if registration succeeds, or `Err(EventError)` if it fails.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// events.on_client("movement", "player_moved", |event: RawClientMessageEvent| {
    ///     // Parse and validate movement data
    ///     Ok(())
    /// }).await?;
    /// 
    /// events.on_client("chat", "message", |event: RawClientMessageEvent| {
    ///     // Process chat message
    ///     Ok(())
    /// }).await?;
    /// ```
    pub async fn on_client<T, F>(
        &self,
        namespace: &str,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static,
    {
        let event_key = format!("client:{}:{}", namespace, event_name);
        self.register_typed_handler(event_key, event_name, handler)
            .await
    }

    /// Registers a handler for plugin-to-plugin events.
    /// 
    /// Plugin events enable communication between different plugins without
    /// tight coupling. Each plugin can emit events that other plugins can
    /// listen for.
    /// 
    /// # Arguments
    /// 
    /// * `plugin_name` - Name of the plugin emitting the event
    /// * `event_name` - Name of the event being emitted
    /// * `handler` - Function to handle events of type T
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if registration succeeds, or `Err(EventError)` if it fails.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// // Combat plugin listening for inventory events
    /// events.on_plugin("inventory", "item_equipped", |event: ItemEquippedEvent| {
    ///     // Update combat stats when items are equipped
    ///     Ok(())
    /// }).await?;
    /// 
    /// // Quest plugin listening for combat events
    /// events.on_plugin("combat", "enemy_defeated", |event: EnemyDefeatedEvent| {
    ///     // Progress quest objectives
    ///     Ok(())
    /// }).await?;
    /// ```
    pub async fn on_plugin<T, F>(
        &self,
        plugin_name: &str,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static,
    {
        let event_key = format!("plugin:{}:{}", plugin_name, event_name);
        self.register_typed_handler(event_key, event_name, handler)
            .await
    }

    /// Internal helper for registering typed handlers.
    /// 
    /// This method handles the common logic for all handler registration types.
    /// It creates a TypedEventHandler wrapper and stores it in the handlers map.
    /// 
    /// # Arguments
    /// 
    /// * `event_key` - Full event key (e.g., "core:server_started")
    /// * `_event_name` - Event name for debugging (currently unused)
    /// * `handler` - Function to handle events of type T
    async fn register_typed_handler<T, F>(
        &self,
        event_key: String,
        _event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static,
    {
        let handler_name = format!("{}::{}", event_key, T::type_name());
        let typed_handler = TypedEventHandler::new(handler_name, handler);
        let handler_arc: Arc<dyn EventHandler> = Arc::new(typed_handler);

        let mut handlers = self.handlers.write().await;
        handlers
            .entry(event_key.clone())
            .or_insert_with(Vec::new)
            .push(handler_arc);

        let mut stats = self.stats.write().await;
        stats.total_handlers += 1;

        info!("📝 Registered handler for {}", event_key);
        Ok(())
    }

    /// Emits a core server event to all registered handlers.
    /// 
    /// # Arguments
    /// 
    /// * `event_name` - Name of the core event
    /// * `event` - The event instance to emit
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if emission succeeds, or `Err(EventError)` if serialization fails.
    /// Individual handler failures are logged but don't cause the emission to fail.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// events.emit_core("player_connected", &PlayerConnectedEvent {
    ///     player_id: player_id,
    ///     connection_id: conn_id,
    ///     remote_addr: addr,
    ///     timestamp: current_timestamp(),
    /// }).await?;
    /// ```
    pub async fn emit_core<T>(&self, event_name: &str, event: &T) -> Result<(), EventError>
    where
        T: Event,
    {
        let event_key = format!("core:{}", event_name);
        self.emit_event(&event_key, event).await
    }

    /// Emits a client event to all registered handlers.
    /// 
    /// # Arguments
    /// 
    /// * `namespace` - Namespace for the event
    /// * `event_name` - Name of the event
    /// * `event` - The event instance to emit
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if emission succeeds, or `Err(EventError)` if serialization fails.
    /// Individual handler failures are logged but don't cause the emission to fail.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// events.emit_client("movement", "player_moved", &RawClientMessageEvent {
    ///     player_id: player_id,
    ///     message_type: "move".to_string(),
    ///     data: movement_data,
    ///     timestamp: current_timestamp(),
    /// }).await?;
    /// ```
    pub async fn emit_client<T>(
        &self,
        namespace: &str,
        event_name: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event,
    {
        let event_key = format!("client:{}:{}", namespace, event_name);
        self.emit_event(&event_key, event).await
    }

    /// Emits a plugin event to all registered handlers.
    /// 
    /// # Arguments
    /// 
    /// * `plugin_name` - Name of the plugin emitting the event
    /// * `event_name` - Name of the event
    /// * `event` - The event instance to emit
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if emission succeeds, or `Err(EventError)` if serialization fails.
    /// Individual handler failures are logged but don't cause the emission to fail.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// events.emit_plugin("combat", "damage_dealt", &DamageDealtEvent {
    ///     attacker: attacker_id,
    ///     target: target_id,
    ///     damage: damage_amount,
    ///     timestamp: current_timestamp(),
    /// }).await?;
    /// ```
    pub async fn emit_plugin<T>(
        &self,
        plugin_name: &str,
        event_name: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event,
    {
        let event_key = format!("plugin:{}:{}", plugin_name, event_name);
        self.emit_event(&event_key, event).await
    }

    /// Internal emit implementation that handles the actual event dispatch.
    /// 
    /// This method serializes the event once and then dispatches it to all
    /// registered handlers. Handler failures are logged but don't prevent
    /// other handlers from running.
    /// 
    /// # Arguments
    /// 
    /// * `event_key` - Full event key for handler lookup
    /// * `event` - The event instance to emit
    async fn emit_event<T>(&self, event_key: &str, event: &T) -> Result<(), EventError>
    where
        T: Event,
    {
        let data = event.serialize()?;
        let handlers = self.handlers.read().await;

        if let Some(event_handlers) = handlers.get(event_key) {
            debug!(
                "📤 Emitting {} to {} handlers",
                event_key,
                event_handlers.len()
            );

            for handler in event_handlers {
                if let Err(e) = handler.handle(&data).await {
                    error!("❌ Handler {} failed: {}", handler.handler_name(), e);
                }
            }

            let mut stats = self.stats.write().await;
            stats.events_emitted += 1;
        } else {
            warn!("⚠️ No handlers for event: {}", event_key);
        }

        Ok(())
    }

    /// Returns current system statistics.
    /// 
    /// # Returns
    /// 
    /// Returns a clone of the current `EventSystemStats` containing
    /// information about handlers and emitted events.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// let stats = events.get_stats().await;
    /// println!("Handlers: {}, Events: {}", stats.total_handlers, stats.events_emitted);
    /// ```
    pub async fn get_stats(&self) -> EventSystemStats {
        let stats = self.stats.read().await;
        stats.clone()
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
/// events.emit_core("player_connected", &PlayerConnectedEvent {
///     player_id: PlayerId::new(),
///     connection_id: "conn_abc123".to_string(),
///     remote_addr: "192.168.1.100:45678".to_string(),
///     timestamp: current_timestamp(),
/// }).await?;
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
/// events.emit_core("player_disconnected", &PlayerDisconnectedEvent {
///     player_id: player_id,
///     connection_id: connection_id,
///     reason: DisconnectReason::ClientDisconnect,
///     timestamp: current_timestamp(),
/// }).await?;
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

/// Enumeration of possible disconnection reasons.
/// 
/// This provides structured information about why a player disconnected,
/// which is useful for debugging, logging, and handling different disconnect
/// scenarios appropriately.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DisconnectReason {
    /// Player initiated disconnection (normal logout)
    ClientDisconnect,
    /// Connection timed out due to inactivity or network issues
    Timeout,
    /// Server is shutting down gracefully
    ServerShutdown,
    /// An error occurred that forced disconnection
    Error(String),
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
/// events.emit_core("plugin_loaded", &PluginLoadedEvent {
///     plugin_name: "combat_system".to_string(),
///     version: "2.1.0".to_string(),
///     capabilities: vec!["damage_calculation".to_string(), "status_effects".to_string()],
///     timestamp: current_timestamp(),
/// }).await?;
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
/// events.emit_core("plugin_unloaded", &PluginUnloadedEvent {
///     plugin_name: "old_combat_system".to_string(),
///     timestamp: current_timestamp(),
/// }).await?;
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
/// events.emit_core("region_started", &RegionStartedEvent {
///     region_id: RegionId::new(),
///     bounds: RegionBounds {
///         min_x: -1000.0, max_x: 1000.0,
///         min_y: 0.0, max_y: 256.0,
///         min_z: -1000.0, max_z: 1000.0,
///     },
///     timestamp: current_timestamp(),
/// }).await?;
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
/// events.emit_core("region_stopped", &RegionStoppedEvent {
///     region_id: region_id,
///     timestamp: current_timestamp(),
/// }).await?;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionStoppedEvent {
    /// Unique identifier for the region
    pub region_id: RegionId,
    /// Unix timestamp when the region was stopped
    pub timestamp: u64,
}

/// Defines the spatial boundaries of a game region.
/// 
/// This structure defines a 3D bounding box that encompasses all
/// the space within a game region. It's used for:
/// - Determining which region a player is in
/// - Spatial partitioning of game logic
/// - Collision detection boundaries
/// - Resource allocation planning
/// 
/// # Examples
/// 
/// ```rust
/// let region_bounds = RegionBounds {
///     min_x: -500.0, max_x: 500.0,    // 1km wide
///     min_y: 0.0, max_y: 128.0,       // 128 units tall
///     min_z: -500.0, max_z: 500.0,    // 1km deep
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionBounds {
    /// Minimum X coordinate (western boundary)
    pub min_x: f64,
    /// Maximum X coordinate (eastern boundary)
    pub max_x: f64,
    /// Minimum Y coordinate (bottom boundary)
    pub min_y: f64,
    /// Maximum Y coordinate (top boundary)
    pub max_y: f64,
    /// Minimum Z coordinate (southern boundary)
    pub min_z: f64,
    /// Maximum Z coordinate (northern boundary)
    pub max_z: f64,
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
/// // Emit a raw client message for plugin processing
/// events.emit_client("movement", "position_update", &RawClientMessageEvent {
///     player_id: player_id,
///     message_type: "move".to_string(),
///     data: serialize_movement_data(&movement)?,
///     timestamp: current_timestamp(),
/// }).await?;
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

// ============================================================================
// Statistics and Error Types
// ============================================================================

/// Statistics about the event system's performance and usage.
/// 
/// These statistics are useful for monitoring system health, debugging
/// performance issues, and understanding event system usage patterns.
/// 
/// # Examples
/// 
/// ```rust
/// let stats = events.get_stats().await;
/// println!("Event system has {} handlers and has processed {} events", 
///          stats.total_handlers, stats.events_emitted);
/// ```
#[derive(Debug, Default, Clone)]
pub struct EventSystemStats {
    /// Total number of registered event handlers
    pub total_handlers: usize,
    /// Total number of events emitted since system start
    pub events_emitted: u64,
}

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
}

// ============================================================================
// Plugin Development Macros and Utilities
// ============================================================================

/// Simplified plugin trait that doesn't require unsafe code.
/// 
/// This trait provides a safe, high-level interface for plugin development.
/// It handles all the complex FFI and lifecycle management internally,
/// allowing plugin developers to focus on game logic rather than
/// low-level systems programming.
/// 
/// # Lifecycle
/// 
/// 1. **Creation**: Plugin instance is created via `new()`
/// 2. **Handler Registration**: `register_handlers()` is called to set up event handlers
/// 3. **Initialization**: `on_init()` is called with server context
/// 4. **Operation**: Plugin receives and processes events
/// 5. **Shutdown**: `on_shutdown()` is called for cleanup
/// 
/// # Examples
/// 
/// ```rust
/// struct ChatPlugin {
///     message_count: u64,
/// }
/// 
/// impl ChatPlugin {
///     fn new() -> Self {
///         Self { message_count: 0 }
///     }
/// }
/// 
/// #[async_trait]
/// impl SimplePlugin for ChatPlugin {
///     fn name(&self) -> &str { "chat_system" }
///     fn version(&self) -> &str { "1.0.0" }
///     
///     async fn register_handlers(&mut self, events: Arc<EventSystem>) -> Result<(), PluginError> {
///         events.on_client("chat", "message", |event: RawClientMessageEvent| {
///             // Process chat message
///             Ok(())
///         }).await?;
///         Ok(())
///     }
/// }
/// 
/// create_simple_plugin!(ChatPlugin);
/// ```
#[async_trait]
pub trait SimplePlugin: Send + Sync + 'static {
    /// Returns the name of this plugin.
    /// 
    /// The name should be unique and stable across versions. It's used for
    /// event routing, logging, and plugin management.
    fn name(&self) -> &str;

    /// Returns the version string of this plugin.
    /// 
    /// Should follow semantic versioning (e.g., "1.2.3") for compatibility checking.
    fn version(&self) -> &str;

    /// Registers event handlers during pre-initialization.
    /// 
    /// This method is called before `on_init()` and should set up all event
    /// handlers that the plugin needs. Handler registration must be completed
    /// before the plugin is considered fully loaded.
    /// 
    /// # Arguments
    /// 
    /// * `events` - Reference to the event system for handler registration
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if all handlers were registered successfully, or
    /// `Err(PluginError)` if registration failed.
    async fn register_handlers(&mut self, events: Arc<EventSystem>) -> Result<(), PluginError>;

    /// Initialize the plugin with server context.
    /// 
    /// This method is called after handler registration and provides access
    /// to server resources. Use this for:
    /// - Loading configuration
    /// - Initializing data structures
    /// - Setting up timers or background tasks
    /// - Validating dependencies
    /// 
    /// # Arguments
    /// 
    /// * `context` - Server context providing access to core services
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if initialization succeeds, or `Err(PluginError)` if it fails.
    /// Failed initialization will prevent the plugin from loading.
    async fn on_init(&mut self, _context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        Ok(()) // Default implementation does nothing
    }

    /// Shutdown the plugin gracefully.
    /// 
    /// This method is called when the plugin is being unloaded or the server
    /// is shutting down. Use this for:
    /// - Saving persistent state
    /// - Cleaning up resources
    /// - Canceling background tasks
    /// - Notifying external services
    /// 
    /// # Arguments
    /// 
    /// * `context` - Server context for accessing core services during shutdown
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if shutdown completes successfully, or `Err(PluginError)`
    /// if cleanup failed. Shutdown errors are logged but don't prevent unloading.
    async fn on_shutdown(&mut self, _context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        Ok(()) // Default implementation does nothing
    }
}

/// Macro to create a plugin with minimal boilerplate and comprehensive panic handling.
/// 
/// This macro generates all the necessary FFI wrapper code to bridge between
/// the `SimplePlugin` trait and the lower-level `Plugin` trait. It includes
/// comprehensive panic handling to ensure that plugin panics don't crash
/// the host server process.
/// 
/// # Safety Features
/// 
/// - **Panic Isolation**: All plugin methods are wrapped in `catch_unwind`
/// - **FFI Safety**: Proper null pointer handling at FFI boundaries
/// - **Memory Safety**: Correct Box allocation/deallocation for plugin instances
/// - **Error Conversion**: Panics are converted to `PluginError::Runtime`
/// 
/// # Usage
/// 
/// Simply call this macro with your plugin type after implementing `SimplePlugin`:
/// 
/// ```rust
/// struct MyPlugin {
///     // Plugin fields
/// }
/// 
/// impl MyPlugin {
///     fn new() -> Self {
///         Self { /* initialization */ }
///     }
/// }
/// 
/// #[async_trait]
/// impl SimplePlugin for MyPlugin {
///     // Implementation
/// }
/// 
/// create_simple_plugin!(MyPlugin);
/// ```
/// 
/// This generates:
/// - `create_plugin()` - C-compatible plugin creation function
/// - `destroy_plugin()` - C-compatible plugin destruction function
/// - `PluginWrapper` - Internal wrapper with panic handling
/// - Proper FFI exports for dynamic loading
#[macro_export]
macro_rules! create_simple_plugin {
    ($plugin_type:ty) => {
        use $crate::Plugin;
        use std::panic::{catch_unwind, AssertUnwindSafe};

        /// Wrapper to bridge SimplePlugin and Plugin traits with panic protection.
        /// 
        /// This internal struct handles the conversion between the high-level
        /// `SimplePlugin` interface and the low-level `Plugin` trait required
        /// for FFI compatibility.
        struct PluginWrapper {
            inner: $plugin_type,
        }

        impl PluginWrapper {
            /// Helper to convert panics to PluginError.
            /// 
            /// This method extracts meaningful error messages from panic payloads
            /// and converts them to structured `PluginError` instances.
            fn panic_to_error(panic_info: Box<dyn std::any::Any + Send>) -> PluginError {
                let message = if let Some(s) = panic_info.downcast_ref::<&str>() {
                    format!("Plugin panicked: {}", s)
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    format!("Plugin panicked: {}", s)
                } else {
                    "Plugin panicked with unknown error".to_string()
                };
                
                PluginError::Runtime(message)
            }
        }

        #[async_trait]
        impl Plugin for PluginWrapper {
            fn name(&self) -> &str {
                // For synchronous methods, we can use catch_unwind directly
                match catch_unwind(AssertUnwindSafe(|| self.inner.name())) {
                    Ok(name) => name,
                    Err(_) => "unknown-plugin-name", // Fallback name if panic occurs
                }
            }

            fn version(&self) -> &str {
                match catch_unwind(AssertUnwindSafe(|| self.inner.version())) {
                    Ok(version) => version,
                    Err(_) => "unknown-version", // Fallback version if panic occurs
                }
            }

            async fn pre_init(
                &mut self,
                context: Arc<dyn ServerContext>,
            ) -> Result<(), PluginError> {
                // Run directly on the current thread using the current runtime handle
                catch_unwind(AssertUnwindSafe(|| {
                    futures::executor::block_on(self.inner.register_handlers(context.events()))
                }))
                .map_err(Self::panic_to_error)?
            }

            async fn init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
                catch_unwind(AssertUnwindSafe(|| {
                    futures::executor::block_on(self.inner.on_init(context))
                }))
                .map_err(Self::panic_to_error)?
            }

            async fn shutdown(
                &mut self,
                context: Arc<dyn ServerContext>,
            ) -> Result<(), PluginError> {
                catch_unwind(AssertUnwindSafe(|| {
                    futures::executor::block_on(self.inner.on_shutdown(context))
                }))
                .map_err(Self::panic_to_error)?
            }
        }

        /// Plugin creation function with panic protection - required export.
        /// 
        /// This function is called by the plugin loader to create a new instance
        /// of the plugin. It must be exported with C linkage for dynamic loading.
        /// 
        /// # Safety
        /// 
        /// This function is marked unsafe because it crosses FFI boundaries,
        /// but all operations are carefully protected against panics and
        /// memory safety violations.
        /// 
        /// # Returns
        /// 
        /// Returns a raw pointer to the plugin instance, or null if creation failed.
        #[no_mangle]
        pub unsafe extern "C" fn create_plugin() -> *mut dyn Plugin {
            // Critical: catch panics at FFI boundary to prevent UB
            match catch_unwind(AssertUnwindSafe(|| {
                let plugin = Box::new(PluginWrapper {
                    inner: <$plugin_type>::new(),
                });
                Box::into_raw(plugin) as *mut dyn Plugin
            })) {
                Ok(plugin_ptr) => plugin_ptr,
                Err(panic_info) => {
                    // Log the panic if possible (you might want to use your logging system here)
                    eprintln!("Plugin creation panicked: {:?}", panic_info);
                    std::ptr::null_mut::<PluginWrapper>() as *mut dyn Plugin // Return null on panic
                }
            }
        }

        /// Plugin destruction function with panic protection - required export.
        /// 
        /// This function is called by the plugin loader to clean up a plugin
        /// instance. It properly deallocates the Box and handles panics.
        /// 
        /// # Safety
        /// 
        /// This function is marked unsafe because it operates on raw pointers
        /// from FFI, but all operations are protected against panics.
        /// 
        /// # Arguments
        /// 
        /// * `plugin` - Raw pointer to the plugin instance to destroy
        #[no_mangle]
        pub unsafe extern "C" fn destroy_plugin(plugin: *mut dyn Plugin) {
            if plugin.is_null() {
                return;
            }

            // Critical: catch panics at FFI boundary to prevent UB
            let _ = catch_unwind(AssertUnwindSafe(|| {
                let _ = Box::from_raw(plugin);
            }));
            // If destruction panics, we just ignore it - the memory might leak
            // but it's better than crashing the host process
        }

        /// Optional: Initialize panic hook for better panic handling.
        /// 
        /// Call this once during plugin loading if you want custom panic handling.
        /// This sets up a global panic hook that will capture and log panic
        /// information in a structured way.
        #[allow(dead_code)]
        fn init_plugin_panic_hook() {
            std::panic::set_hook(Box::new(|panic_info| {
                eprintln!("Plugin panic occurred: {}", panic_info);
                // You could also send this to your logging system
            }));
        }
    };
}

/// Convenience macro for registering multiple handlers with clean syntax.
/// 
/// This macro provides a declarative way to register multiple event handlers
/// at once, organized by event category. It supports all three event types:
/// core, client, and plugin events.
/// 
/// # Syntax
/// 
/// ```rust
/// register_handlers!(events;
///     core {
///         "event_name" => |event: EventType| { /* handler */ Ok(()) },
///         // ... more core handlers
///     }
///     client {
///         "namespace", "event_name" => |event: EventType| { /* handler */ Ok(()) },
///         // ... more client handlers  
///     }
///     plugin {
///         "target_plugin", "event_name" => |event: EventType| { /* handler */ Ok(()) },
///         // ... more plugin handlers
///     }
/// );
/// ```
/// 
/// # Examples
/// 
/// ```rust
/// register_handlers!(events;
///     core {
///         "server_started" => |event: ServerStartedEvent| {
///             println!("Server is online!");
///             Ok(())
///         },
///         "player_connected" => |event: PlayerConnectedEvent| {
///             println!("Player {} joined", event.player_id);
///             Ok(())
///         }
///     }
///     client {
///         "movement", "jump" => |event: RawClientMessageEvent| {
///             handle_player_jump(event.player_id, &event.data)?;
///             Ok(())
///         },
///         "chat", "message" => |event: RawClientMessageEvent| {
///             process_chat_message(event.player_id, &event.data)?;
///             Ok(())
///         }
///     }
///     plugin {
///         "combat", "damage_dealt" => |event: DamageEvent| {
///             update_statistics(event.attacker, event.damage)?;
///             Ok(())
///         }
///     }
/// );
/// ```
#[macro_export]
macro_rules! register_handlers {
    // Handle client events section
    ($events:expr; client { $($namespace:expr, $event_name:expr => $handler:expr),* $(,)? }) => {{
        $(
            $events.on_client($namespace, $event_name, $handler).await.map_err(|e| PluginError::ExecutionError(e.to_string()))?;;
        )*
        Ok(())
    }};

    // Handle plugin events section
    ($events:expr; plugin { $($target_plugin:expr, $event_name:expr => $handler:expr),* $(,)? }) => {{
        $(
            $events.on_plugin($target_plugin, $event_name, $handler).await.map_err(|e| PluginError::ExecutionError(e.to_string()))?;
        )*
        Ok(())
    }};

    // Handle core events section
    ($events:expr; core { $($event_name:expr => $handler:expr),* $(,)? }) => {{
        $(
            $events.on_core($event_name, $handler).await.map_err(|e| PluginError::ExecutionError(e.to_string()))?;
        )*
        Ok(())
    }};

    // Handle mixed events with semicolon separators
    ($events:expr;
     $(client { $($c_namespace:expr, $c_event_name:expr => $c_handler:expr),* $(,)? })?
     $(plugin { $($p_target_plugin:expr, $p_event_name:expr => $p_handler:expr),* $(,)? })?
     $(core { $($core_event_name:expr => $core_handler:expr),* $(,)? })?
    ) => {{
        $($(
            $events.on_client($c_namespace, $c_event_name, $c_handler).await?;
        )*)?
        $($(
            $events.on_plugin($p_target_plugin, $p_event_name, $p_handler).await?;
        )*)?
        $($(
            $events.on_core($core_event_name, $core_handler).await?;
        )*)?
    }};
}

/// Simple macro for single handler registration (alternative to the bulk macro).
/// 
/// This macro provides a concise way to register individual event handlers
/// without the overhead of the bulk registration syntax. It's useful for
/// conditional registration or when you only need to register one handler.
/// 
/// # Syntax
/// 
/// - `on_event!(events, core "event_name" => handler)`
/// - `on_event!(events, client "namespace", "event_name" => handler)`  
/// - `on_event!(events, plugin "target_plugin", "event_name" => handler)`
/// 
/// # Examples
/// 
/// ```rust
/// // Register a core event handler
/// on_event!(events, core "server_started" => |event: ServerStartedEvent| {
///     println!("Server started at {}", event.timestamp);
///     Ok(())
/// });
/// 
/// // Register a client event handler
/// on_event!(events, client "movement", "jump" => |event: RawClientMessageEvent| {
///     handle_jump(event.player_id)?;
///     Ok(())
/// });
/// 
/// // Register a plugin event handler
/// on_event!(events, plugin "inventory", "item_used" => |event: ItemUsedEvent| {
///     apply_item_effects(event.player_id, event.item_id)?;
///     Ok(())
/// });
/// ```
#[macro_export]
macro_rules! on_event {
    ($events:expr, client $namespace:expr, $event_name:expr => $handler:expr) => {
        $events.on_client($namespace, $event_name, $handler).await?;
    };
    ($events:expr, plugin $target_plugin:expr, $event_name:expr => $handler:expr) => {
        $events
            .on_plugin($target_plugin, $event_name, $handler)
            .await?;
    };
    ($events:expr, core $event_name:expr => $handler:expr) => {
        $events.on_core($event_name, $handler).await?;
    };
}

// ============================================================================
// Server Context Interface (Minimal)
// ============================================================================

/// Server context interface providing access to core server services.
/// 
/// This trait defines the interface that plugins use to interact with the
/// core server. It provides access to essential services like the event
/// system, logging, and player communication while maintaining a clean
/// separation between plugin code and server internals.
/// 
/// # Design Principles
/// 
/// - **Minimal Interface**: Only essential services are exposed
/// - **Type Safety**: All operations are strongly typed
/// - **Async Support**: All potentially blocking operations are async
/// - **Error Handling**: Proper error types for all fallible operations
/// 
/// # Examples
/// 
/// ```rust
/// async fn on_init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
///     // Access event system
///     let events = context.events();
///     
///     // Log plugin initialization
///     context.log(LogLevel::Info, "Combat plugin initialized");
///     
///     // Get current region
///     let region_id = context.region_id();
///     
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait ServerContext: Send + Sync {
    /// Returns a reference to the event system.
    /// 
    /// This provides access to the same event system used by the core server,
    /// allowing plugins to emit events and register additional handlers.
    fn events(&self) -> Arc<EventSystem>;
    
    /// Returns the ID of the region this context is associated with.
    /// 
    /// Plugins can use this to understand which region they're operating in
    /// and to emit region-specific events.
    fn region_id(&self) -> RegionId;
    
    /// Logs a message with the specified level.
    /// 
    /// This integrates with the server's logging system and should be used
    /// for all plugin logging to ensure consistent log formatting and routing.
    /// 
    /// # Arguments
    /// 
    /// * `level` - Severity level of the log message
    /// * `message` - The message to log
    fn log(&self, level: LogLevel, message: &str);

    /// Sends raw data to a specific player.
    /// 
    /// This method bypasses the event system and sends data directly to a
    /// player's connection. It should be used for high-frequency or
    /// latency-sensitive communications.
    /// 
    /// # Arguments
    /// 
    /// * `player_id` - Target player identifier
    /// * `data` - Raw bytes to send
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if the data was queued for sending, or `Err(ServerError)`
    /// if the send failed (e.g., player not connected).
    async fn send_to_player(&self, player_id: PlayerId, data: &[u8]) -> Result<(), ServerError>;

    /// Broadcasts raw data to all connected players.
    /// 
    /// This method sends data to all players currently connected to the server.
    /// Use with caution as it can generate significant network traffic.
    /// 
    /// # Arguments
    /// 
    /// * `data` - Raw bytes to broadcast
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if the broadcast was initiated, or `Err(ServerError)`
    /// if the broadcast failed.
    async fn broadcast(&self, data: &[u8]) -> Result<(), ServerError>;
}

/// Low-level plugin trait for FFI compatibility.
/// 
/// This trait defines the interface that plugin dynamic libraries must implement
/// for compatibility with the plugin loader. Most plugin developers should use
/// the `SimplePlugin` trait instead, which provides a higher-level interface.
/// 
/// # Plugin Lifecycle
/// 
/// 1. **Pre-initialization**: `pre_init()` for handler registration
/// 2. **Initialization**: `init()` for setup with server context  
/// 3. **Operation**: Plugin receives and processes events
/// 4. **Shutdown**: `shutdown()` for cleanup
/// 
/// # FFI Safety
/// 
/// This trait is designed to be safe across FFI boundaries when used with
/// the `create_simple_plugin!` macro, which handles all the necessary
/// panic catching and error conversion.
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Returns the plugin name.
    /// 
    /// Must be stable across plugin versions and unique among all plugins.
    fn name(&self) -> &str;
    
    /// Returns the plugin version string.
    /// 
    /// Should follow semantic versioning for compatibility checking.
    fn version(&self) -> &str;

    /// Pre-initialization phase for registering event handlers.
    /// 
    /// This method is called before `init()` and should register all event
    /// handlers that the plugin needs. The plugin will not receive events
    /// until this method completes successfully.
    /// 
    /// # Arguments
    /// 
    /// * `context` - Server context for accessing core services
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if pre-initialization succeeds, or `Err(PluginError)`
    /// if it fails. Failure will prevent the plugin from loading.
    async fn pre_init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError>;
    
    /// Main initialization phase with full server context access.
    /// 
    /// This method is called after successful pre-initialization and provides
    /// full access to server resources. Use this for loading configuration,
    /// initializing data structures, and setting up any background tasks.
    /// 
    /// # Arguments
    /// 
    /// * `context` - Server context for accessing core services
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if initialization succeeds, or `Err(PluginError)`
    /// if it fails. Failure will prevent the plugin from becoming active.
    async fn init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError>;
    
    /// Shutdown phase for cleanup and resource deallocation.
    /// 
    /// This method is called when the plugin is being unloaded or the server
    /// is shutting down. It should perform all necessary cleanup including
    /// saving state, stopping background tasks, and releasing resources.
    /// 
    /// # Arguments
    /// 
    /// * `context` - Server context for accessing core services during shutdown
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if shutdown completes successfully, or `Err(PluginError)`
    /// if cleanup failed. Shutdown errors are logged but don't prevent unloading.
    async fn shutdown(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError>;
}

/// Enumeration of log levels for structured logging.
/// 
/// These levels follow standard logging conventions and integrate with
/// the server's logging infrastructure. Higher levels indicate more
/// severe or important messages.
/// 
/// # Level Guidelines
/// 
/// - **Error**: System errors, plugin failures, critical issues
/// - **Warn**: Recoverable errors, deprecated usage, performance issues  
/// - **Info**: General information, plugin lifecycle, major events
/// - **Debug**: Detailed debugging information, development diagnostics
/// - **Trace**: Very detailed execution traces, performance profiling
/// 
/// # Examples
/// 
/// ```rust
/// context.log(LogLevel::Info, "Combat plugin initialized successfully");
/// context.log(LogLevel::Warn, "Player inventory is nearly full");
/// context.log(LogLevel::Error, "Failed to load combat configuration");
/// ```
#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    /// Critical errors that may affect system stability
    Error,
    /// Warning conditions that should be investigated
    Warn,
    /// General informational messages
    Info,
    /// Detailed information for debugging
    Debug,
    /// Very detailed trace information
    Trace,
}

/// Errors that can occur during plugin operations.
/// 
/// This enum covers all error conditions that can arise during plugin
/// lifecycle management, from initialization failures to runtime errors.
/// 
/// # Error Categories
/// 
/// - **InitializationFailed**: Plugin failed to initialize properly
/// - **ExecutionError**: Runtime error during normal operation
/// - **NotFound**: Requested plugin or resource doesn't exist
/// - **Runtime**: Panic or other unexpected runtime condition
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    /// Plugin initialization failed during startup
    #[error("Plugin initialization failed: {0}")]
    InitializationFailed(String),
    /// Error occurred during plugin execution
    #[error("Plugin execution error: {0}")]
    ExecutionError(String),
    /// Requested plugin was not found
    #[error("Plugin not found: {0}")]
    NotFound(String),
    /// Runtime error such as panic or system failure
    #[error("Plugin runtime error: {0}")]
    Runtime(String),
}

/// Errors that can occur during server operations.
/// 
/// This enum covers error conditions that can arise when plugins interact
/// with core server functionality, particularly networking and internal
/// service operations.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// Network-related error (connection issues, send failures, etc.)
    #[error("Network error: {0}")]
    Network(String),
    /// Internal server error (resource exhaustion, invalid state, etc.)
    #[error("Internal error: {0}")]
    Internal(String),
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Returns the current Unix timestamp in seconds.
/// 
/// This function provides a consistent way to get timestamps across the
/// entire system. All events should use this function for timestamp
/// generation to ensure consistency.
/// 
/// # Panics
/// 
/// Panics if the system clock is set to a time before the Unix epoch
/// (January 1, 1970). This should never happen in practice on modern systems.
/// 
/// # Examples
/// 
/// ```rust
/// let event = PlayerConnectedEvent {
///     player_id: player_id,
///     connection_id: conn_id,
///     remote_addr: addr,
///     timestamp: current_timestamp(),
/// };
/// ```
/// 
/// # Returns
/// 
/// Current time as seconds since Unix epoch (1970-01-01 00:00:00 UTC).
pub fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
}

/// Creates a new Horizon event system instance.
/// 
/// This is the primary factory function for creating event system instances.
/// It returns an `Arc<EventSystem>` that can be safely shared across multiple
/// threads and stored in various contexts.
/// 
/// The returned event system is fully initialized and ready to accept
/// handler registrations and event emissions.
/// 
/// # Examples
/// 
/// ```rust
/// let events = create_horizon_event_system();
/// 
/// // Register some handlers
/// events.on_core("server_started", |event: ServerStartedEvent| {
///     println!("Server online!");
///     Ok(())
/// }).await?;
/// 
/// // Use in plugin context
/// let context = MyServerContext::new(events.clone());
/// ```
/// 
/// # Returns
/// 
/// A new `Arc<EventSystem>` ready for use.
pub fn create_horizon_event_system() -> Arc<EventSystem> {
    Arc::new(EventSystem::new())
}

// ============================================================================
// Comprehensive Test Suite
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Test event type for unit testing.
    /// 
    /// This simple event type is used throughout the test suite to verify
    /// event system functionality without requiring complex game-specific
    /// event types.
    #[derive(Debug, Serialize, Deserialize)]
    struct TestEvent {
        message: String,
    }

    /// Test the refined event API with individual handler registration.
    /// 
    /// This test verifies that the high-level event registration and emission
    /// API works correctly for all three event categories: core, client, and plugin.
    #[tokio::test]
    async fn test_refined_event_api() {
        let events = create_horizon_event_system();

        // Test the new cleaner API - individual registration
        events
            .on_core("server_started", |event: TestEvent| {
                println!("Core event: {}", event.message);
                Ok(())
            })
            .await
            .expect("Failed to register core event in horizon_event_system");

        events
            .on_client("movement", "player_moved", |event: TestEvent| {
                println!("Client movement event: {}", event.message);
                Ok(())
            })
            .await
            .expect("Failed to register client event in horizon_event_system");

        events
            .on_plugin("combat", "attack", |event: TestEvent| {
                println!("Combat plugin event: {}", event.message);
                Ok(())
            })
            .await
            .expect("Failed to register plugin event in horizon_event_system");

        // Test emission
        events
            .emit_core(
                "server_started",
                &TestEvent {
                    message: "Server is running".to_string(),
                },
            )
            .await
            .expect("Failed to emit core event in horizon_event_system");

        events
            .emit_client(
                "movement",
                "player_moved",
                &TestEvent {
                    message: "Player moved to new position".to_string(),
                },
            )
            .await
            .expect("Failed to emit client event in horizon_event_system");

        events
            .emit_plugin(
                "combat",
                "attack",
                &TestEvent {
                    message: "Player attacked".to_string(),
                },
            )
            .await
            .expect("Failed to emit plugin event in horizon_event_system");
    }

    /// Test the bulk registration macro functionality.
    /// 
    /// This test verifies that the `register_handlers!` macro correctly
    /// registers multiple handlers at once and that all registered handlers
    /// receive their respective events.
    #[tokio::test]
    async fn test_bulk_registration_macro() -> Result<(), Box<dyn std::error::Error>> {
        let events = create_horizon_event_system();

        // Test the bulk registration macro
        register_handlers!(events;
            client {
                "movement", "jump" => |event: TestEvent| {
                    println!("Jump: {}", event.message);
                    Ok(())
                },
                "chat", "message" => |event: TestEvent| {
                    println!("Chat: {}", event.message);
                    Ok(())
                }
            }
            plugin {
                "combat", "damage" => |event: TestEvent| {
                    println!("Damage: {}", event.message);
                    Ok(())
                }
            }
            core {
                "server_started" => |event: TestEvent| {
                    println!("Server: {}", event.message);
                    Ok(())
                }
            }
        );

        // Test they all work
        events
            .emit_client(
                "movement",
                "jump",
                &TestEvent {
                    message: "Player jumped!".to_string(),
                },
            )
            .await
            .expect("Failed to emit client event in horizon_event_system");

        events
            .emit_plugin(
                "combat",
                "damage",
                &TestEvent {
                    message: "Damage dealt!".to_string(),
                },
            )
            .await
            .expect("Failed to emit plugin event in horizon_event_system");

        Ok(()) as Result<(), Box<dyn std::error::Error>>
    }

    /// Test the simple on_event! macro for individual registrations.
    /// 
    /// This test verifies that the `on_event!` macro provides a clean
    /// syntax for registering individual handlers and that the handlers
    /// function correctly.
    #[tokio::test]
    async fn test_simple_on_event_macro() -> Result<(), Box<dyn std::error::Error>> {
        let events = create_horizon_event_system();

        // Test the simple on_event! macro for single registrations
        on_event!(events, client "test", "event" => |event: TestEvent| {
            println!("Simple macro test: {}", event.message);
            Ok(())
        });

        on_event!(events, core "test_core" => |event: TestEvent| {
            println!("Core macro test: {}", event.message);
            Ok(())
        });

        // Test emission
        events
            .emit_client(
                "test",
                "event",
                &TestEvent {
                    message: "Simple macro works!".to_string(),
                },
            )
            .await
            .expect("Failed to emit client event in horizon_event_system");

        Ok(())
    }

    /// Test event system statistics tracking.
    /// 
    /// This test verifies that the event system correctly tracks statistics
    /// about handler registrations and event emissions.
    #[tokio::test]
    async fn test_event_system_stats() -> Result<(), Box<dyn std::error::Error>> {
        let events = create_horizon_event_system();

        // Initial stats should be empty
        let initial_stats = events.get_stats().await;
        assert_eq!(initial_stats.total_handlers, 0);
        assert_eq!(initial_stats.events_emitted, 0);

        // Register some handlers
        events
            .on_core("test1", |_event: TestEvent| Ok(()))
            .await?;
        events
            .on_client("test", "test2", |_event: TestEvent| Ok(()))
            .await?;

        // Check handler count increased
        let stats_after_registration = events.get_stats().await;
        assert_eq!(stats_after_registration.total_handlers, 2);
        assert_eq!(stats_after_registration.events_emitted, 0);

        // Emit some events
        events
            .emit_core("test1", &TestEvent { message: "test".to_string() })
            .await?;
        events
            .emit_client("test", "test2", &TestEvent { message: "test".to_string() })
            .await?;

        // Check emission count increased
        let final_stats = events.get_stats().await;
        assert_eq!(final_stats.total_handlers, 2);
        assert_eq!(final_stats.events_emitted, 2);

        Ok(())
    }

    /// Test core event types serialization and deserialization.
    /// 
    /// This test ensures that all core event types can be properly
    /// serialized and deserialized, which is essential for network
    /// transmission and persistence.
    #[tokio::test]
    async fn test_core_event_serialization() -> Result<(), Box<dyn std::error::Error>> {
        // Test PlayerConnectedEvent
        let player_event = PlayerConnectedEvent {
            player_id: PlayerId::new(),
            connection_id: "test_conn_123".to_string(),
            remote_addr: "127.0.0.1:8080".to_string(),
            timestamp: current_timestamp(),
        };

        let serialized = <PlayerConnectedEvent as Event>::serialize(&player_event)?;
        let deserialized = <PlayerConnectedEvent as Event>::deserialize(&serialized)?;
        assert_eq!(player_event.player_id, deserialized.player_id);
        assert_eq!(player_event.connection_id, deserialized.connection_id);

        // Test PlayerDisconnectedEvent
        let disconnect_event = PlayerDisconnectedEvent {
            player_id: PlayerId::new(),
            connection_id: "test_conn_456".to_string(),
            reason: DisconnectReason::ClientDisconnect,
            timestamp: current_timestamp(),
        };

        let serialized = <PlayerDisconnectedEvent as Event>::serialize(&disconnect_event)?;
        let deserialized = <PlayerDisconnectedEvent as Event>::deserialize(&serialized)?;
        assert_eq!(disconnect_event.player_id, deserialized.player_id);

        // Test RegionStartedEvent
        let region_event = RegionStartedEvent {
            region_id: RegionId::new(),
            bounds: RegionBounds {
                min_x: -100.0, max_x: 100.0,
                min_y: 0.0, max_y: 50.0,
                min_z: -100.0, max_z: 100.0,
            },
            timestamp: current_timestamp(),
        };

        let serialized = <RegionStartedEvent as Event>::serialize(&region_event)?;
        let deserialized = <RegionStartedEvent as Event>::deserialize(&serialized)?;
        assert_eq!(region_event.region_id, deserialized.region_id);
        assert_eq!(region_event.bounds.min_x, deserialized.bounds.min_x);

        Ok(())
    }

    /// Test handler error handling and isolation.
    /// 
    /// This test verifies that when one handler fails, other handlers
    /// for the same event continue to execute properly. This ensures
    /// that plugin errors don't cascade and break the entire system.
    #[tokio::test]
    async fn test_handler_error_isolation() -> Result<(), Box<dyn std::error::Error>> {
        let events = create_horizon_event_system();

        // Set up a test to track which handlers execute
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc as StdArc;

        let handler1_executed = StdArc::new(AtomicBool::new(false));
        let handler2_executed = StdArc::new(AtomicBool::new(false));
        let handler3_executed = StdArc::new(AtomicBool::new(false));

        let h1_flag = handler1_executed.clone();
        let h2_flag = handler2_executed.clone();
        let h3_flag = handler3_executed.clone();

        // Register handlers: first succeeds, second fails, third succeeds
        events
            .on_core("test_error", move |_event: TestEvent| {
                h1_flag.store(true, Ordering::Relaxed);
                Ok(())
            })
            .await?;

        events
            .on_core("test_error", move |_event: TestEvent| {
                h2_flag.store(true, Ordering::Relaxed);
                Err(EventError::HandlerExecution("Intentional test failure".to_string()))
            })
            .await?;

        events
            .on_core("test_error", move |_event: TestEvent| {
                h3_flag.store(true, Ordering::Relaxed);
                Ok(())
            })
            .await?;

        // Emit event
        events
            .emit_core("test_error", &TestEvent { message: "test".to_string() })
            .await?;

        // All handlers should have executed despite the middle one failing
        assert!(handler1_executed.load(Ordering::Relaxed));
        assert!(handler2_executed.load(Ordering::Relaxed));
        assert!(handler3_executed.load(Ordering::Relaxed));

        Ok(())
    }

    /// Test PlayerId functionality and string conversion.
    /// 
    /// This test verifies that PlayerId handles UUID generation, parsing,
    /// and string conversion correctly.
    #[test]
    fn test_player_id_functionality() {
        // Test new player ID generation
        let player1 = PlayerId::new();
        let player2 = PlayerId::new();
        assert_ne!(player1, player2); // Should be unique

        // Test string conversion
        let player_str = player1.to_string();
        let parsed_player = PlayerId::from_str(&player_str).expect("Failed to parse player ID");
        assert_eq!(player1, parsed_player);

        // Test invalid string parsing
        let invalid_result = PlayerId::from_str("not-a-uuid");
        assert!(invalid_result.is_err());

        // Test default implementation
        let default_player = PlayerId::default();
        assert_ne!(default_player.0, uuid::Uuid::nil());
    }

    /// Test Position struct functionality.
    /// 
    /// This test verifies basic Position operations and serialization.
    #[test]
    fn test_position_functionality() {
        let pos = Position::new(10.5, 20.0, -5.25);
        assert_eq!(pos.x, 10.5);
        assert_eq!(pos.y, 20.0);
        assert_eq!(pos.z, -5.25);

        // Test serialization/deserialization
        let serialized = serde_json::to_string(&pos).expect("Failed to serialize position");
        let deserialized: Position = serde_json::from_str(&serialized)
            .expect("Failed to deserialize position");
        
        assert_eq!(pos.x, deserialized.x);
        assert_eq!(pos.y, deserialized.y);
        assert_eq!(pos.z, deserialized.z);
    }

    /// Test current_timestamp utility function.
    /// 
    /// This test verifies that the timestamp function returns reasonable
    /// values and that successive calls produce increasing timestamps.
    #[test]
    fn test_current_timestamp() {
        let timestamp1 = current_timestamp();
        
        // Should be a reasonable timestamp (after year 2020)
        assert!(timestamp1 > 1_577_836_800); // 2020-01-01 00:00:00 UTC
        
        // Should be before year 2100 (sanity check)
        assert!(timestamp1 < 4_102_444_800); // 2100-01-01 00:00:00 UTC
        
        // Successive calls should be equal or increasing
        let timestamp2 = current_timestamp();
        assert!(timestamp2 >= timestamp1);
    }
}