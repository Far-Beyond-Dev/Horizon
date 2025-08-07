//! Core event system with flexible event propagation support

use crate::error::EventError;
use crate::propagation::EventPropagator;
use async_trait::async_trait;
use compact_str::CompactString;
use dashmap::DashMap;
use futures::stream::{FuturesUnordered, StreamExt};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::any::TypeId;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use tracing::{debug, error, warn};

/// Trait that all events must implement
pub trait Event: Send + Sync + Debug + 'static {
    /// Returns the event type name for routing
    fn event_type() -> &'static str
    where
        Self: Sized;

    /// Returns the TypeId for type-safe handling
    fn type_id(&self) -> TypeId {
        TypeId::of::<Self>()
    }
}

/// Trait for event keys that can be used for routing
/// 
/// This allows users to define their own event key types for better performance
/// and type safety instead of being locked into strings.
pub trait EventKeyType: Clone + PartialEq + Eq + std::hash::Hash + Send + Sync + std::fmt::Debug + 'static {
    /// Convert to a string representation for storage/debugging
    fn to_string(&self) -> String;
}

/// Default string-based event key for simple use cases
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct EventKey {
    /// Primary namespace (e.g., "core", "client", "plugin")
    pub namespace: CompactString,
    /// Secondary identifier (e.g., plugin name, object type)
    pub category: Option<CompactString>,
    /// Event name
    pub event_name: CompactString,
}

impl EventKeyType for EventKey {
    fn to_string(&self) -> String {
        if let Some(ref category) = self.category {
            format!("{}:{}:{}", self.namespace, category, self.event_name)
        } else {
            format!("{}:{}", self.namespace, self.event_name)
        }
    }
}

/// Enum-based event key for better performance and type safety
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum StructuredEventKey {
    /// Core server events
    Core { event_name: CompactString },
    /// Client events with namespace
    Client { namespace: CompactString, event_name: CompactString },
    /// Plugin-to-plugin events
    Plugin { plugin_name: CompactString, event_name: CompactString },
    /// GORC object events
    Gorc { object_type: CompactString, channel: u8, event_name: CompactString },
    /// GORC instance events
    GorcInstance { object_type: CompactString, channel: u8, event_name: CompactString },
    /// Custom event types
    Custom { fields: Vec<CompactString> },
}

impl EventKeyType for StructuredEventKey {
    fn to_string(&self) -> String {
        match self {
            StructuredEventKey::Core { event_name } => format!("core:{}", event_name),
            StructuredEventKey::Client { namespace, event_name } => format!("client:{}:{}", namespace, event_name),
            StructuredEventKey::Plugin { plugin_name, event_name } => format!("plugin:{}:{}", plugin_name, event_name),
            StructuredEventKey::Gorc { object_type, channel, event_name } => format!("gorc:{}:{}:{}", object_type, channel, event_name),
            StructuredEventKey::GorcInstance { object_type, channel, event_name } => format!("gorc_instance:{}:{}:{}", object_type, channel, event_name),
            StructuredEventKey::Custom { fields } => fields.iter().map(|f| f.as_str()).collect::<Vec<_>>().join(":"),
        }
    }
}

/// Strongly-typed namespaces for even better performance
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum EventNamespace {
    Core,
    Client,
    Plugin,
    Gorc,
    GorcInstance,
    Custom(u32), // Custom namespaces identified by hash
}

/// Fast event key using enums and owned strings (removed lifetime issues)
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TypedEventKey {
    pub namespace: EventNamespace,
    pub components: Vec<CompactString>,
}

impl EventKeyType for TypedEventKey {
    fn to_string(&self) -> String {
        let namespace_str = match self.namespace {
            EventNamespace::Core => "core",
            EventNamespace::Client => "client", 
            EventNamespace::Plugin => "plugin",
            EventNamespace::Gorc => "gorc",
            EventNamespace::GorcInstance => "gorc_instance",
            EventNamespace::Custom(id) => return format!("custom_{}:{}", id, self.components.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(":")),
        };
        
        if self.components.is_empty() {
            namespace_str.to_string()
        } else {
            format!("{}:{}", namespace_str, self.components.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(":"))
        }
    }
}

impl EventKey {
    /// Create a new event key
    pub fn new(namespace: &str, category: Option<&str>, event_name: &str) -> Self {
        Self {
            namespace: CompactString::new(namespace),
            category: category.map(CompactString::new),
            event_name: CompactString::new(event_name),
        }
    }

    /// Create a simple two-part key (namespace:event_name)
    pub fn simple(namespace: &str, event_name: &str) -> Self {
        Self::new(namespace, None, event_name)
    }

    /// Create a three-part key (namespace:category:event_name)
    pub fn categorized(namespace: &str, category: &str, event_name: &str) -> Self {
        Self::new(namespace, Some(category), event_name)
    }

    /// Convert to string representation for storage
    pub fn to_string(&self) -> String {
        if let Some(ref category) = self.category {
            format!("{}:{}:{}", self.namespace, category, self.event_name)
        } else {
            format!("{}:{}", self.namespace, self.event_name)
        }
    }

    /// Parse from string representation
    pub fn from_string(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        match parts.len() {
            2 => Some(Self::simple(parts[0], parts[1])),
            3 => Some(Self::categorized(parts[0], parts[1], parts[2])),
            _ => None,
        }
    }
}

/// Serialized event data that can cross boundaries safely
#[derive(Debug, Clone)]
pub struct EventData {
    /// The raw serialized data
    pub data: Arc<Vec<u8>>,
    /// Type information for deserialization
    pub type_name: String,
    /// Event metadata
    pub metadata: HashMap<String, String>,
}

impl EventData {
    /// Create new event data from a serializable event
    pub fn new<T: Event + Serialize>(event: &T) -> Result<Self, EventError> {
        let data = serde_json::to_vec(event)
            .map_err(|e| EventError::SerializationFailed(e.to_string()))?;
        
        Ok(Self {
            data: Arc::new(data),
            type_name: T::event_type().to_string(),
            metadata: HashMap::new(),
        })
    }

    /// Deserialize to a specific event type
    pub fn deserialize<T: Event + for<'de> Deserialize<'de>>(&self) -> Result<T, EventError> {
        serde_json::from_slice(&self.data)
            .map_err(|e| EventError::DeserializationFailed(e.to_string()))
    }

    /// Add metadata to the event
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }
}

/// Trait for event handlers
#[async_trait]
pub trait EventHandler: Send + Sync {
    /// Handle an event
    async fn handle(&self, event: &EventData) -> Result<(), EventError>;
    
    /// Get handler name for debugging
    fn handler_name(&self) -> &str;
    
    /// Get the type this handler expects
    fn expected_type(&self) -> TypeId;
}

/// Typed event handler for type-safe event handling
pub struct TypedEventHandler<T, F>
where
    T: Event + for<'de> Deserialize<'de>,
    F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static,
{
    handler: F,
    name: String,
    _phantom: std::marker::PhantomData<T>,
}

impl<T, F> TypedEventHandler<T, F>
where
    T: Event + for<'de> Deserialize<'de>,
    F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static,
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
    T: Event + for<'de> Deserialize<'de>,
    F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static,
{
    async fn handle(&self, event_data: &EventData) -> Result<(), EventError> {
        // Type check
        if event_data.type_name != T::event_type() {
            return Err(EventError::InvalidEventFormat(format!(
                "Expected {}, got {}",
                T::event_type(),
                event_data.type_name
            )));
        }

        // Deserialize and handle
        let event = event_data.deserialize::<T>()?;
        (self.handler)(event)
    }

    fn handler_name(&self) -> &str {
        &self.name
    }

    fn expected_type(&self) -> TypeId {
        TypeId::of::<T>()
    }
}

/// Statistics for event system monitoring
#[derive(Debug, Clone, Default)]
pub struct EventStats {
    pub events_emitted: u64,
    pub events_handled: u64,
    pub handler_failures: u64,
    pub total_handlers: usize,
}

/// Core event bus with pluggable propagation logic and typed event keys
pub struct EventBus<K: EventKeyType, P: EventPropagator<K>> {
    /// Event handlers organized by event key
    handlers: DashMap<K, SmallVec<[Arc<dyn EventHandler>; 4]>>,
    /// Event propagation logic
    propagator: P,
    /// Statistics
    stats: Arc<tokio::sync::RwLock<EventStats>>,
    /// Phantom data for the key type
    _phantom: std::marker::PhantomData<K>,
}

impl<K: EventKeyType, P: EventPropagator<K>> EventBus<K, P> {
    /// Create a new event bus with custom propagator
    pub fn with_propagator(propagator: P) -> Self {
        Self {
            handlers: DashMap::new(),
            propagator,
            stats: Arc::new(tokio::sync::RwLock::new(EventStats::default())),
            _phantom: std::marker::PhantomData,
        }
    }

    /// Register a typed event handler with a custom event key
    pub async fn on_key<T, F>(
        &mut self,
        key: K,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        self.register_handler(key, handler).await
    }

    /// Internal handler registration
    async fn register_handler<T, F>(
        &mut self,
        key: K,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let handler_name = format!("{}::{}", key.to_string(), T::event_type());
        let typed_handler = TypedEventHandler::new(handler_name, handler);
        let handler_arc: Arc<dyn EventHandler> = Arc::new(typed_handler);

        self.handlers
            .entry(key.clone())
            .or_insert_with(SmallVec::new)
            .push(handler_arc);

        // Update stats
        let mut stats = self.stats.write().await;
        stats.total_handlers += 1;

        debug!("📝 Registered handler for {}", key.to_string());
        Ok(())
    }

    /// Emit an event with a custom event key
    pub async fn emit_key<T>(
        &self,
        key: K,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event + Serialize,
    {
        self.emit_with_key(key, event).await
    }

    /// Internal emit implementation
    async fn emit_with_key<T>(
        &self,
        key: K,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event + Serialize,
    {
        // Serialize the event
        let event_data = Arc::new(EventData::new(event)?);

        // Get handlers for this event
        let handlers = self.handlers.get(&key).map(|entry| entry.value().clone());

        if let Some(handlers) = handlers {
            if !handlers.is_empty() {
                debug!("📤 Emitting {} to {} handlers", key.to_string(), handlers.len());

                // Create propagation context
                let context = crate::propagation::PropagationContext {
                    event_key: key.clone(),
                    metadata: event_data.metadata.clone(),
                };

                // Use propagator to determine which handlers should receive the event
                let mut futures = FuturesUnordered::new();

                for handler in handlers.iter() {
                    // Check if this handler should receive the event
                    if self.propagator.should_propagate(&key, &context).await {
                        // Optionally transform the event
                        let final_event = self.propagator
                            .transform_event(event_data.clone(), &context)
                            .await
                            .unwrap_or_else(|| event_data.clone());

                        let handler_clone = handler.clone();
                        let handler_name = handler.handler_name().to_string();

                        futures.push(async move {
                            if let Err(e) = handler_clone.handle(&final_event).await {
                                error!("❌ Handler {} failed: {}", handler_name, e);
                                return Err(e);
                            }
                            Ok(())
                        });
                    }
                }

                // Execute all handlers concurrently
                let mut success_count = 0;
                let mut failure_count = 0;

                while let Some(result) = futures.next().await {
                    match result {
                        Ok(_) => success_count += 1,
                        Err(_) => failure_count += 1,
                    }
                }

                // Update stats
                let mut stats = self.stats.write().await;
                stats.events_emitted += 1;
                stats.events_handled += success_count;
                stats.handler_failures += failure_count;
            }
        } else {
            // No handlers found - simplified logging for typed keys
            let key_string = key.to_string();
            if key_string != "core:server_tick" && key_string != "core:raw_client_message" {
                warn!("⚠️ No handlers for event: {}", key_string);
            }
        }

        Ok(())
    }

    /// Get current statistics
    pub async fn stats(&self) -> EventStats {
        self.stats.read().await.clone()
    }

    /// Get handler count
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }

    /// Get all registered event keys
    pub fn registered_keys(&self) -> Vec<K> {
        self.handlers.iter().map(|entry| entry.key().clone()).collect()
    }
}

// Implement Event for common types that might be used
impl Event for serde_json::Value {
    fn event_type() -> &'static str {
        "json_value"
    }
}

impl Event for String {
    fn event_type() -> &'static str {
        "string"
    }
}

impl Event for Vec<u8> {
    fn event_type() -> &'static str {
        "bytes"
    }
}