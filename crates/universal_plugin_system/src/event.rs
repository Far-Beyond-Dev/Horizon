//! Core event system with flexible event propagation support

use crate::error::EventError;
use crate::propagation::{EventPropagator, PropagationContext};
use async_trait::async_trait;
use compact_str::CompactString;
use dashmap::DashMap;
use futures::stream::{FuturesUnordered, StreamExt};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::any::{Any, TypeId};
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

/// Event key for routing events to handlers
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct EventKey {
    /// Primary namespace (e.g., "core", "client", "plugin")
    pub namespace: CompactString,
    /// Secondary identifier (e.g., plugin name, object type)
    pub category: Option<CompactString>,
    /// Event name
    pub event_name: CompactString,
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

/// Core event bus with pluggable propagation logic
pub struct EventBus<P: EventPropagator> {
    /// Event handlers organized by event key
    handlers: DashMap<String, SmallVec<[Arc<dyn EventHandler>; 4]>>,
    /// Event propagation logic
    propagator: P,
    /// Statistics
    stats: Arc<tokio::sync::RwLock<EventStats>>,
}

impl<P: EventPropagator> EventBus<P> {
    /// Create a new event bus with custom propagator
    pub fn with_propagator(propagator: P) -> Self {
        Self {
            handlers: DashMap::new(),
            propagator,
            stats: Arc::new(tokio::sync::RwLock::new(EventStats::default())),
        }
    }

    /// Register a typed event handler
    pub async fn on<T, F>(
        &mut self,
        namespace: &str,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let key = EventKey::simple(namespace, event_name);
        self.register_handler(key, handler).await
    }

    /// Register a categorized event handler
    pub async fn on_categorized<T, F>(
        &mut self,
        namespace: &str,
        category: &str,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let key = EventKey::categorized(namespace, category, event_name);
        self.register_handler(key, handler).await
    }

    /// Internal handler registration
    async fn register_handler<T, F>(
        &mut self,
        key: EventKey,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let handler_name = format!("{}::{}", key.to_string(), T::event_type());
        let typed_handler = TypedEventHandler::new(handler_name, handler);
        let handler_arc: Arc<dyn EventHandler> = Arc::new(typed_handler);

        let key_string = key.to_string();
        self.handlers
            .entry(key_string.clone())
            .or_insert_with(SmallVec::new)
            .push(handler_arc);

        // Update stats
        let mut stats = self.stats.write().await;
        stats.total_handlers += 1;

        debug!("📝 Registered handler for {}", key_string);
        Ok(())
    }

    /// Emit an event
    pub async fn emit<T>(
        &self,
        namespace: &str,
        event_name: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event + Serialize,
    {
        let key = EventKey::simple(namespace, event_name);
        self.emit_with_key(key, event).await
    }

    /// Emit a categorized event
    pub async fn emit_categorized<T>(
        &self,
        namespace: &str,
        category: &str,
        event_name: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event + Serialize,
    {
        let key = EventKey::categorized(namespace, category, event_name);
        self.emit_with_key(key, event).await
    }

    /// Internal emit implementation
    async fn emit_with_key<T>(
        &self,
        key: EventKey,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event + Serialize,
    {
        // Serialize the event
        let event_data = Arc::new(EventData::new(event)?);
        let key_string = key.to_string();

        // Get handlers for this event
        let handlers = self.handlers.get(&key_string).map(|entry| entry.value().clone());

        if let Some(handlers) = handlers {
            if !handlers.is_empty() {
                debug!("📤 Emitting {} to {} handlers", key_string, handlers.len());

                // Create propagation context
                let context = PropagationContext {
                    event_key: key_string.clone(),
                    metadata: event_data.metadata.clone(),
                };

                // Use propagator to determine which handlers should receive the event
                let mut futures = FuturesUnordered::new();

                for handler in handlers.iter() {
                    // Check if this handler should receive the event
                    if self.propagator.should_propagate(&key_string, &context).await {
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
            // No handlers found
            if key_string != "core:server_tick" {
                // Don't spam logs with server_tick events
                let available_keys: Vec<String> = self.handlers
                    .iter()
                    .filter_map(|entry| {
                        let stored_key = entry.key();
                        if stored_key.contains(&key.namespace.as_str()) {
                            Some(stored_key.clone())
                        } else {
                            None
                        }
                    })
                    .collect();

                if !available_keys.is_empty() {
                    warn!("⚠️ No handlers for event: {} (similar keys available: {:?})", key_string, available_keys);
                } else {
                    warn!("⚠️ No handlers for event: {} (no similar handlers found)", key_string);
                }
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
    pub fn registered_keys(&self) -> Vec<String> {
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