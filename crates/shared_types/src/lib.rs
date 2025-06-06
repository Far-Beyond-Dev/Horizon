use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use uuid::Uuid;

/// Unique identifier for players
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub Uuid);

impl PlayerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for PlayerId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for PlayerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

impl Default for RegionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Namespace for events to prevent conflicts between plugins
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventNamespace(pub String);

impl EventNamespace {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
    
    pub fn plugin_default(plugin_name: &str) -> Self {
        Self(format!("plugin.{}", plugin_name))
    }
}

impl fmt::Display for EventNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Event identifier combining namespace and event type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId {
    pub namespace: EventNamespace,
    pub event_type: String,
}

impl EventId {
    pub fn new(namespace: EventNamespace, event_type: impl Into<String>) -> Self {
        Self {
            namespace,
            event_type: event_type.into(),
        }
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{}", self.namespace, self.event_type)
    }
}

/// Position in 3D space
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
    pub fn new(name: impl Into<String>, position: Position) -> Self {
        Self {
            id: PlayerId::new(),
            name: name.into(),
            position,
            metadata: HashMap::new(),
        }
    }
}

/// Core game events that all plugins can listen to
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum CoreEvent {
    PlayerJoined { player: Player },
    PlayerLeft { player_id: PlayerId },
    PlayerMoved { player_id: PlayerId, old_position: Position, new_position: Position },
    RegionChanged { region_id: RegionId },
    CustomMessage { data: serde_json::Value },
}

/// Trait for serializable events
pub trait GameEvent: fmt::Debug + Send + Sync + 'static {
    fn event_type(&self) -> &'static str;
    fn serialize(&self) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>>;
    fn as_any(&self) -> &dyn Any;
}

impl GameEvent for CoreEvent {
    fn event_type(&self) -> &'static str {
        match self {
            CoreEvent::PlayerJoined { .. } => "player_joined",
            CoreEvent::PlayerLeft { .. } => "player_left", 
            CoreEvent::PlayerMoved { .. } => "player_moved",
            CoreEvent::RegionChanged { .. } => "region_changed",
            CoreEvent::CustomMessage { .. } => "custom_message",
        }
    }
    
    fn serialize(&self) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(serde_json::to_vec(self)?)
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl From<Box<dyn GameEvent>> for Arc<dyn GameEvent + Send + Sync> {
    fn from(b: Box<dyn GameEvent>) -> Self {
        // Explicitly construct an Arc with the correct trait object type
        let boxed: Box<dyn GameEvent + Send + Sync> = b as Box<dyn GameEvent + Send + Sync>;
        Arc::from(boxed)
    }
}

/// Server context provided to plugins
#[async_trait]
pub trait ServerContext: Send + Sync {
    /// Emit an event to the event system
    async fn emit_event(&self, namespace: EventNamespace, event: Box<dyn GameEvent>) -> Result<(), ServerError>;
    
    /// Get current region ID
    fn region_id(&self) -> RegionId;
    
    /// Get all players in the region
    async fn get_players(&self) -> Result<Vec<Player>, ServerError>;
    
    /// Get specific player by ID
    async fn get_player(&self, id: PlayerId) -> Result<Option<Player>, ServerError>;
    
    /// Send message to specific player
    async fn send_to_player(&self, player_id: PlayerId, message: &[u8]) -> Result<(), ServerError>;
    
    /// Broadcast message to all players in region
    async fn broadcast_to_region(&self, message: &[u8]) -> Result<(), ServerError>;
    
    /// Log message (for debugging/monitoring)
    fn log(&self, level: LogLevel, message: &str);
}

/// Plugin trait that all plugins must implement
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Plugin name (used for default namespace)
    fn name(&self) -> &'static str;
    
    /// Plugin version
    fn version(&self) -> &'static str;
    
    /// Initialize the plugin
    async fn initialize(&mut self, context: &dyn ServerContext) -> Result<(), PluginError>;
    
    /// Handle an event
    async fn handle_event(&mut self, event_id: &EventId, event: &dyn GameEvent, context: &dyn ServerContext) -> Result<(), PluginError>;
    
    /// Get event IDs this plugin wants to listen to
    fn subscribed_events(&self) -> Vec<EventId>;
    
    /// Shutdown the plugin
    async fn shutdown(&mut self, context: &dyn ServerContext) -> Result<(), PluginError>;
}

/// Opaque type for FFI-safe plugin pointers
#[repr(C)]
pub struct PluginOpaque;

/// Function signature for plugin creation (FFI-safe)
pub type PluginCreateFn = unsafe extern "C" fn() -> *mut PluginOpaque;

/// Errors that can occur in the server
#[derive(thiserror::Error, Debug)]
pub enum ServerError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Player not found: {0}")]
    PlayerNotFound(PlayerId),
    #[error("Region error: {0}")]
    Region(String),
    #[error("Plugin error: {0}")]
    Plugin(#[from] PluginError),
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Plugin-specific errors
#[derive(thiserror::Error, Debug)]
pub enum PluginError {
    #[error("Plugin initialization failed: {0}")]
    InitializationFailed(String),
    #[error("Plugin execution error: {0}")]
    ExecutionError(String),
    #[error("Plugin configuration error: {0}")]
    ConfigurationError(String),
    #[error("Plugin dependency error: {0}")]
    DependencyError(String),
}

/// Log levels for plugin logging
#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

/// Network message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum NetworkMessage {
    PlayerJoin { name: String },
    PlayerMove { position: Position },
    PlayerLeave,
    GameData { data: serde_json::Value },
    PluginMessage { plugin: String, data: serde_json::Value },
}

/// Connection information for a client
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub player_id: PlayerId,
    pub remote_addr: std::net::SocketAddr,
    pub connected_at: std::time::SystemTime,
}

/// Region bounds for the server
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