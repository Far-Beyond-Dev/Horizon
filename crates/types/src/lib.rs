//! Core types and traits for the Horizon game server event system
//! 
//! This module provides the foundation for a high-performance, type-safe
//! event system with automatic JSON serialization and namespace-based routing.

use async_trait::async_trait;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

// ============================================================================
// Core Identifiers
// ============================================================================

/// Unique identifier for players
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub Uuid);

impl PlayerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for PlayerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for regions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RegionId(pub Uuid);

impl RegionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for RegionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ============================================================================
// Event System Core
// ============================================================================

/// Event namespace for organizing events
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventNamespace {
    /// Core server events (connections, game state, etc.)
    Core,
    /// Plugin-to-plugin communication
    Plugin(String),
    /// Client events forwarded from WebSocket connections
    Client,
    /// Custom namespace
    Custom(String),
}

impl std::fmt::Display for EventNamespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventNamespace::Core => write!(f, "core"),
            EventNamespace::Plugin(name) => write!(f, "plugin:{}", name),
            EventNamespace::Client => write!(f, "client"),
            EventNamespace::Custom(name) => write!(f, "custom:{}", name),
        }
    }
}

/// Complete event identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventId {
    pub namespace: EventNamespace,
    pub name: String,
}

impl EventId {
    pub fn new(namespace: EventNamespace, name: impl Into<String>) -> Self {
        Self {
            namespace,
            name: name.into(),
        }
    }
    
    pub fn core(name: impl Into<String>) -> Self {
        Self::new(EventNamespace::Core, name)
    }
    
    pub fn plugin(plugin_name: impl Into<String>, event_name: impl Into<String>) -> Self {
        Self::new(EventNamespace::Plugin(plugin_name.into()), event_name)
    }
    
    pub fn client(name: impl Into<String>) -> Self {
        Self::new(EventNamespace::Client, name)
    }
}

impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.namespace, self.name)
    }
}

// ============================================================================
// Event Traits
// ============================================================================

/// Trait for serializable events
pub trait Event: Send + Sync + Any + std::fmt::Debug {
    /// Get the type name for this event
    fn type_name() -> &'static str where Self: Sized;
    
    /// Serialize this event to JSON
    fn serialize(&self) -> Result<Vec<u8>, EventError>;
    
    /// Deserialize from JSON bytes
    fn deserialize(data: &[u8]) -> Result<Self, EventError> where Self: Sized;
    
    /// Get TypeId for runtime type checking
    fn type_id(&self) -> TypeId {
        TypeId::of::<Self>()
    }
    
    /// Cast to Any for downcasting
    fn as_any(&self) -> &dyn Any;
}

/// Auto-implement Event for types that implement the required traits
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

/// Type-erased event handler
#[async_trait]
pub trait EventHandler: Send + Sync {
    /// Handle an event with automatic deserialization
    async fn handle(&self, data: &[u8]) -> Result<(), EventError>;
    
    /// Get the expected type ID for this handler
    fn expected_type_id(&self) -> TypeId;
    
    /// Get handler name for debugging
    fn handler_name(&self) -> &str;
}

/// Concrete implementation of EventHandler for specific types
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
// Event System Interface (Object-Safe)
// ============================================================================

/// Core event system interface (object-safe - can be used with dyn)
#[async_trait]
pub trait EventSystem: Send + Sync {
    /// Type-erased handler registration (internal use)
    async fn register_handler(&self, event_id: EventId, handler: Arc<dyn EventHandler>) -> Result<(), EventError>;
    
    /// Emit raw JSON data to an event
    async fn emit_raw(&self, event_id: EventId, data: &[u8]) -> Result<(), EventError>;
    
    /// Get statistics about registered handlers
    async fn get_stats(&self) -> EventSystemStats;
}

/// Extension trait for generic event system methods (not object-safe, but provides nice API)
#[async_trait]
pub trait EventSystemExt {
    /// Register an event handler with automatic type deserialization
    async fn on<T, F>(&self, event_name: &str, handler: F) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static;
    
    /// Register an event handler with namespace
    async fn on_namespaced<T, F>(&self, event_id: EventId, handler: F) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static;
    
    /// Emit an event with automatic serialization
    async fn emit<T>(&self, event_name: &str, event: &T) -> Result<(), EventError>
    where
        T: Event;
    
    /// Emit an event with namespace
    async fn emit_namespaced<T>(&self, event_id: EventId, event: &T) -> Result<(), EventError>
    where
        T: Event;
}

/// Implement the extension trait for any type that implements EventSystem
#[async_trait]
impl<S> EventSystemExt for S
where
    S: EventSystem + ?Sized,
{
    async fn on<T, F>(&self, event_name: &str, handler: F) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static,
    {
        let event_id = EventId::core(event_name);
        self.on_namespaced(event_id, handler).await
    }
    
    async fn on_namespaced<T, F>(&self, event_id: EventId, handler: F) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static,
    {
        let handler_name = format!("{}::{}", event_id, T::type_name());
        let typed_handler = TypedEventHandler::new(handler_name, handler);
        let handler_arc = Arc::new(typed_handler);
        self.register_handler(event_id, handler_arc).await
    }
    
    async fn emit<T>(&self, event_name: &str, event: &T) -> Result<(), EventError>
    where
        T: Event,
    {
        let event_id = EventId::core(event_name);
        self.emit_namespaced(event_id, event).await
    }
    
    async fn emit_namespaced<T>(&self, event_id: EventId, event: &T) -> Result<(), EventError>
    where
        T: Event,
    {
        let data = event.serialize()?;
        self.emit_raw(event_id, &data).await
    }
}

/// Client event system extensions
#[async_trait]
pub trait ClientEventSystemExt {
    async fn on_client<T, F>(&self, event_name: &str, handler: F) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static;
    
    async fn emit_client<T>(&self, event_name: &str, event: &T) -> Result<(), EventError>
    where
        T: Event;
    
    async fn on_core<T, F>(&self, event_name: &str, handler: F) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static;
    
    async fn on_plugin<T, F>(&self, plugin_name: &str, event_name: &str, handler: F) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static;
}

/// Statistics about the event system
#[derive(Debug, Clone)]
pub struct EventSystemStats {
    pub total_handlers: usize,
    pub events_by_namespace: HashMap<String, usize>,
    pub total_events_emitted: u64,
    pub total_events_handled: u64,
}

// ============================================================================
// Server Context
// ============================================================================

/// Server context provided to plugins
#[async_trait]
pub trait ServerContext: Send + Sync {
    /// Get the event system
    fn events(&self) -> Arc<dyn EventSystem>;
    
    /// Get current region ID
    fn region_id(&self) -> RegionId;
    
    /// Get all active players
    async fn get_players(&self) -> Result<Vec<Player>, ServerError>;
    
    /// Get specific player by ID
    async fn get_player(&self, id: PlayerId) -> Result<Option<Player>, ServerError>;
    
    /// Send message to specific player
    async fn send_to_player(&self, player_id: PlayerId, message: &[u8]) -> Result<(), ServerError>;
    
    /// Broadcast message to all players
    async fn broadcast(&self, message: &[u8]) -> Result<(), ServerError>;
    
    /// Log a message
    fn log(&self, level: LogLevel, message: &str);
}

// ============================================================================
// Plugin Interface
// ============================================================================

/// Plugin trait with lifecycle methods
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Plugin name
    fn name(&self) -> &str;
    
    /// Plugin version
    fn version(&self) -> &str;
    
    /// Pre-initialization: register event handlers
    /// This is where plugins should call `context.events().on()` to register handlers
    async fn pre_init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError>;
    
    /// Initialization: emit events, do startup tasks
    /// This is where plugins can emit events to other plugins and do startup work
    async fn init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError>;
    
    /// Shutdown the plugin
    async fn shutdown(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError>;
}

// ============================================================================
// Game Types
// ============================================================================

/// 3D position
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Position {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
    
    pub fn distance_to(&self, other: &Position) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

/// Player information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub position: Position,
    pub metadata: HashMap<String, String>,
}

impl Player {
    pub fn new(name: String, position: Position) -> Self {
        Self {
            id: PlayerId::new(),
            name,
            position,
            metadata: HashMap::new(),
        }
    }
}

/// Region bounds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionBounds {
    pub min_x: f64,
    pub max_x: f64,
    pub min_y: f64,
    pub max_y: f64,
    pub min_z: f64,
    pub max_z: f64,
}

impl RegionBounds {
    pub fn contains(&self, position: &Position) -> bool {
        position.x >= self.min_x && position.x <= self.max_x &&
        position.y >= self.min_y && position.y <= self.max_y &&
        position.z >= self.min_z && position.z <= self.max_z
    }
}

// ============================================================================
// Core Events
// ============================================================================

/// Player joined the server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerJoinedEvent {
    pub player: Player,
    pub timestamp: u64,
}

/// Player left the server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerLeftEvent {
    pub player_id: PlayerId,
    pub timestamp: u64,
}

/// Player moved
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerMovedEvent {
    pub player_id: PlayerId,
    pub old_position: Position,
    pub new_position: Position,
    pub timestamp: u64,
}

/// Custom message from client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientMessageEvent {
    pub player_id: PlayerId,
    pub message_type: String,
    pub data: serde_json::Value,
}

// ============================================================================
// Client Event Types
// ============================================================================

/// Player chat message from client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageEvent {
    pub player_id: PlayerId,
    pub message: String,
    pub channel: Option<String>,
    pub timestamp: u64,
}

/// Player movement command from client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveCommandEvent {
    pub player_id: PlayerId,
    pub target_x: f64,
    pub target_y: f64,
    pub target_z: f64,
    pub movement_type: MovementType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MovementType {
    Walk,
    Run,
    Teleport,
}

/// Combat action from client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatActionEvent {
    pub player_id: PlayerId,
    pub action_type: CombatActionType,
    pub target_id: Option<PlayerId>,
    pub target_position: Option<Position>,
    pub ability_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CombatActionType {
    Attack,
    Defend,
    Cast,
    Dodge,
}

/// Crafting request from client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftingRequestEvent {
    pub player_id: PlayerId,
    pub recipe_id: u32,
    pub quantity: u32,
    pub ingredient_sources: Vec<InventorySlot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventorySlot {
    pub slot_id: u32,
    pub item_id: u32,
    pub quantity: u32,
}

/// Player interaction event from client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInteractionEvent {
    pub player_id: PlayerId,
    pub target_player_id: PlayerId,
    pub interaction_type: InteractionType,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InteractionType {
    Trade,
    Challenge,
    Friend,
    Inspect,
    Follow,
}

/// Item usage event (for inter-plugin communication)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UseItemEvent {
    pub player_id: PlayerId,
    pub item_id: u32,
    pub target_player_id: Option<PlayerId>,
    pub target_position: Option<(f64, f64, f64)>,
}

// ============================================================================
// Errors
// ============================================================================

/// Log levels
#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

/// Event system errors
#[derive(Error, Debug)]
pub enum EventError {
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Deserialization error: {0}")]
    Deserialization(serde_json::Error),
    #[error("Handler not found for event: {0}")]
    HandlerNotFound(String),
    #[error("Type mismatch for event: {0}")]
    TypeMismatch(String),
    #[error("Handler execution error: {0}")]
    HandlerExecution(String),
}

/// Plugin errors
#[derive(Error, Debug)]
pub enum PluginError {
    #[error("Plugin initialization failed: {0}")]
    InitializationFailed(String),
    #[error("Plugin execution error: {0}")]
    ExecutionError(String),
    #[error("Plugin not found: {0}")]
    NotFound(String),
    #[error("Plugin already loaded: {0}")]
    AlreadyLoaded(String),
}

/// Server errors
#[derive(Error, Debug)]
pub enum ServerError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Player error: {0}")]
    Player(String),
    #[error("Plugin error: {0}")]
    Plugin(#[from] PluginError),
    #[error("Event error: {0}")]
    Event(#[from] EventError),
    #[error("Internal error: {0}")]
    Internal(String),
}

// ============================================================================
// Network Messages
// ============================================================================

/// WebSocket message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum NetworkMessage {
    /// Player wants to join
    Join { name: String },
    /// Player wants to move
    Move { position: Position },
    /// Player is leaving
    Leave,
    /// Custom game data
    Data { event_type: String, data: serde_json::Value },
}

/// Network message response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum NetworkResponse {
    /// Join successful
    JoinSuccess { player_id: PlayerId },
    /// Join failed
    JoinFailed { reason: String },
    /// Move successful
    MoveSuccess,
    /// Move failed
    MoveFailed { reason: String },
    /// Custom response
    Data { event_type: String, data: serde_json::Value },
    /// Error occurred
    Error { message: String },
}