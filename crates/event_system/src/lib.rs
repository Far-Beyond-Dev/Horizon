//! Core event system focused only on essential server events
//!
//! This system handles only core server lifecycle and infrastructure events.
//! Game-specific events (movement, combat, chat, etc.) are handled by plugins.

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

// ============================================================================
// Core Types (Minimal set)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub Uuid);

impl PlayerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RegionId(pub Uuid);

impl RegionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

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
}

// ============================================================================
// Event Traits and Core Infrastructure
// ============================================================================

pub trait Event: Send + Sync + Any + std::fmt::Debug {
    fn type_name() -> &'static str
    where
        Self: Sized;
    fn serialize(&self) -> Result<Vec<u8>, EventError>;
    fn deserialize(data: &[u8]) -> Result<Self, EventError>
    where
        Self: Sized;
    fn as_any(&self) -> &dyn Any;
}

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

#[async_trait]
pub trait EventHandler: Send + Sync {
    async fn handle(&self, data: &[u8]) -> Result<(), EventError>;
    fn expected_type_id(&self) -> TypeId;
    fn handler_name(&self) -> &str;
}

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
// Refined Event System with Cleaner API
// ============================================================================

pub struct EventSystem {
    handlers: RwLock<HashMap<String, Vec<Arc<dyn EventHandler>>>>,
    stats: RwLock<EventSystemStats>,
}

impl EventSystem {
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
            stats: RwLock::new(EventSystemStats::default()),
        }
    }

    /// Improved API: Register a core server event handler
    pub async fn on_core<T, F>(&self, event_name: &str, handler: F) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static,
    {
        let event_key = format!("core:{}", event_name);
        self.register_typed_handler(event_key, event_name, handler)
            .await
    }

    /// Improved API: Register a client event handler with namespace
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

    /// Improved API: Register a plugin event handler
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

    /// Internal helper for registering typed handlers
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

    /// Emit a core event
    pub async fn emit_core<T>(&self, event_name: &str, event: &T) -> Result<(), EventError>
    where
        T: Event,
    {
        let event_key = format!("core:{}", event_name);
        self.emit_event(&event_key, event).await
    }

    /// Emit a client event
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

    /// Emit a plugin event
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

    /// Internal emit implementation
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

    pub async fn get_stats(&self) -> EventSystemStats {
        let stats = self.stats.read().await;
        stats.clone()
    }
}

// ============================================================================
// Core Server Events ONLY
// ============================================================================

/// Player connected to the server (core infrastructure event)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerConnectedEvent {
    pub player_id: PlayerId,
    pub connection_id: String,
    pub remote_addr: String,
    pub timestamp: u64,
}

/// Player disconnected from the server (core infrastructure event)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerDisconnectedEvent {
    pub player_id: PlayerId,
    pub connection_id: String,
    pub reason: DisconnectReason,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DisconnectReason {
    ClientDisconnect,
    Timeout,
    ServerShutdown,
    Error(String),
}

/// Plugin loaded (core infrastructure event)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginLoadedEvent {
    pub plugin_name: String,
    pub version: String,
    pub capabilities: Vec<String>,
    pub timestamp: u64,
}

/// Plugin unloaded (core infrastructure event)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginUnloadedEvent {
    pub plugin_name: String,
    pub timestamp: u64,
}

/// Server region started (core infrastructure event)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionStartedEvent {
    pub region_id: RegionId,
    pub bounds: RegionBounds,
    pub timestamp: u64,
}

/// Server region stopped (core infrastructure event)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionStoppedEvent {
    pub region_id: RegionId,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionBounds {
    pub min_x: f64,
    pub max_x: f64,
    pub min_y: f64,
    pub max_y: f64,
    pub min_z: f64,
    pub max_z: f64,
}

/// Raw client message received (for routing to plugins)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawClientMessageEvent {
    pub player_id: PlayerId,
    pub message_type: String,
    pub data: Vec<u8>,
    pub timestamp: u64,
}

// ============================================================================
// Statistics and Error Types
// ============================================================================

#[derive(Debug, Default, Clone)]
pub struct EventSystemStats {
    pub total_handlers: usize,
    pub events_emitted: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum EventError {
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Deserialization error: {0}")]
    Deserialization(serde_json::Error),
    #[error("Handler not found: {0}")]
    HandlerNotFound(String),
    #[error("Handler execution error: {0}")]
    HandlerExecution(String),
}

// ============================================================================
// Plugin Development Macros and Utilities
// ============================================================================

/// Simplified plugin trait that doesn't require unsafe code
#[async_trait]
pub trait SimplePlugin: Send + Sync + 'static {
    /// Plugin name
    fn name(&self) -> &str;

    /// Plugin version
    fn version(&self) -> &str;

    /// Register event handlers during pre-initialization
    async fn register_handlers(&mut self, events: Arc<EventSystem>) -> Result<(), PluginError>;

    /// Initialize the plugin
    async fn on_init(&mut self, _context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        Ok(()) // Default implementation
    }

    /// Shutdown the plugin
    async fn on_shutdown(&mut self, _context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        Ok(()) // Default implementation
    }
}

/// Macro to create a plugin with minimal boilerplate
#[macro_export]
macro_rules! create_simple_plugin {
    ($plugin_type:ty) => {
        use $crate::Plugin;

        /// Wrapper to bridge SimplePlugin and Plugin traits
        struct PluginWrapper {
            inner: $plugin_type,
        }

        #[async_trait]
        impl Plugin for PluginWrapper {
            fn name(&self) -> &str {
                self.inner.name()
            }

            fn version(&self) -> &str {
                self.inner.version()
            }

            async fn pre_init(
                &mut self,
                context: Arc<dyn ServerContext>,
            ) -> Result<(), PluginError> {
                self.inner.register_handlers(context.events()).await
            }

            async fn init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
                self.inner.on_init(context).await
            }

            async fn shutdown(
                &mut self,
                context: Arc<dyn ServerContext>,
            ) -> Result<(), PluginError> {
                self.inner.on_shutdown(context).await
            }
        }

        /// Plugin creation function - required export
        #[no_mangle]
        pub unsafe extern "C" fn create_plugin() -> *mut dyn Plugin {
            let plugin = Box::new(PluginWrapper {
                inner: <$plugin_type>::new(),
            });
            Box::into_raw(plugin)
        }

        /// Plugin destruction function - required export
        #[no_mangle]
        pub unsafe extern "C" fn destroy_plugin(plugin: *mut dyn Plugin) {
            if !plugin.is_null() {
                let _ = Box::from_raw(plugin);
            }
        }
    };
}

/// Convenience macro for registering multiple handlers
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

/// Simple macro for single handler registration (alternative to the bulk macro)
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

#[async_trait]
pub trait ServerContext: Send + Sync {
    fn events(&self) -> Arc<EventSystem>;
    fn region_id(&self) -> RegionId;
    fn log(&self, level: LogLevel, message: &str);

    /// Send raw data to a specific player
    async fn send_to_player(&self, player_id: PlayerId, data: &[u8]) -> Result<(), ServerError>;

    /// Broadcast raw data to all players
    async fn broadcast(&self, data: &[u8]) -> Result<(), ServerError>;
}

#[async_trait]
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;

    async fn pre_init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError>;
    async fn init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError>;
    async fn shutdown(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError>;
}

#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("Plugin initialization failed: {0}")]
    InitializationFailed(String),
    #[error("Plugin execution error: {0}")]
    ExecutionError(String),
    #[error("Plugin not found: {0}")]
    NotFound(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

// ============================================================================
// Utility Functions
// ============================================================================

pub fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub fn create_event_system() -> Arc<EventSystem> {
    Arc::new(EventSystem::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    struct TestEvent {
        message: String,
    }

    #[tokio::test]
    async fn test_refined_event_api() {
        let events = create_event_system();

        // Test the new cleaner API - individual registration
        events
            .on_core("server_started", |event: TestEvent| {
                println!("Core event: {}", event.message);
                Ok(())
            })
            .await
            .unwrap();

        events
            .on_client("movement", "player_moved", |event: TestEvent| {
                println!("Client movement event: {}", event.message);
                Ok(())
            })
            .await
            .unwrap();

        events
            .on_plugin("combat", "attack", |event: TestEvent| {
                println!("Combat plugin event: {}", event.message);
                Ok(())
            })
            .await
            .unwrap();

        // Test emission
        events
            .emit_core(
                "server_started",
                &TestEvent {
                    message: "Server is running".to_string(),
                },
            )
            .await
            .unwrap();

        events
            .emit_client(
                "movement",
                "player_moved",
                &TestEvent {
                    message: "Player moved to new position".to_string(),
                },
            )
            .await
            .unwrap();

        events
            .emit_plugin(
                "combat",
                "attack",
                &TestEvent {
                    message: "Player attacked".to_string(),
                },
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_bulk_registration_macro() -> Result<(), Box<dyn std::error::Error>> {
        let events = create_event_system();

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
            .unwrap();

        events
            .emit_plugin(
                "combat",
                "damage",
                &TestEvent {
                    message: "Damage dealt!".to_string(),
                },
            )
            .await
            .unwrap();

        Ok(()) as Result<(), Box<dyn std::error::Error>>
    }

    #[tokio::test]
    async fn test_simple_on_event_macro() -> Result<(), Box<dyn std::error::Error>> {
        let events = create_event_system();

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
            .unwrap();

        Ok(())
    }
}
