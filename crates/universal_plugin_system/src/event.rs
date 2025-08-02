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

/// Generic structured event key that can represent arbitrary domain hierarchies
/// 
/// This is a truly generic event key that doesn't hardcode any specific domains.
/// Host applications can use it to represent their own domain structures like:
/// - `["core", "user_login"]` 
/// - `["client", "chat", "message"]`
/// - `["plugin", "inventory", "item_added"]`
/// - `["gorc", "Player", "0", "position_update"]`
/// 
/// The key is just a hierarchy of string segments that get joined with colons.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct StructuredEventKey {
    /// The domain hierarchy as a sequence of segments
    /// e.g., ["core", "user_login"] or ["client", "chat", "message"]
    pub segments: Vec<CompactString>,
}

impl StructuredEventKey {
    /// Create a new structured key from segments
    pub fn new(segments: Vec<&str>) -> Self {
        Self {
            segments: segments.into_iter().map(|s| s.into()).collect(),
        }
    }
    
    /// Create a key with a single segment
    pub fn single(segment: &str) -> Self {
        Self {
            segments: vec![segment.into()],
        }
    }
    
    /// Create a key with two segments (domain + event)
    pub fn domain_event(domain: &str, event: &str) -> Self {
        Self {
            segments: vec![domain.into(), event.into()],
        }
    }
    
    /// Create a key with three segments (domain + category + event)
    pub fn domain_category_event(domain: &str, category: &str, event: &str) -> Self {
        Self {
            segments: vec![domain.into(), category.into(), event.into()],
        }
    }
    
    /// Get the first segment (usually the domain)
    pub fn domain(&self) -> Option<&str> {
        self.segments.first().map(|s| s.as_str())
    }
    
    /// Get the last segment (usually the event name)
    pub fn event_name(&self) -> Option<&str> {
        self.segments.last().map(|s| s.as_str())
    }
    
    /// Check if this key matches a pattern
    pub fn matches_pattern(&self, pattern: &[&str]) -> bool {
        if pattern.len() != self.segments.len() {
            return false;
        }
        
        for (segment, pattern_segment) in self.segments.iter().zip(pattern.iter()) {
            if *pattern_segment != "*" && segment.as_str() != *pattern_segment {
                return false;
            }
        }
        
        true
    }
    
    /// Check if this key starts with the given prefix
    pub fn starts_with(&self, prefix: &[&str]) -> bool {
        if prefix.len() > self.segments.len() {
            return false;
        }
        
        for (segment, prefix_segment) in self.segments.iter().zip(prefix.iter()) {
            if segment.as_str() != *prefix_segment {
                return false;
            }
        }
        
        true
    }
}

impl EventKeyType for StructuredEventKey {
    fn to_string(&self) -> String {
        self.segments.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(":")
    }
}

/// Typed event key wrapper for better performance with known types
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TypedEventKey<T> {
    /// The underlying structured key
    pub key: StructuredEventKey,
    /// Phantom data for the event type
    _phantom: std::marker::PhantomData<T>,
}

impl<T> TypedEventKey<T> {
    /// Create a new typed event key
    pub fn new(key: StructuredEventKey) -> Self {
        Self {
            key,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T> EventKeyType for TypedEventKey<T> 
where
    T: std::fmt::Debug + Clone + std::cmp::Eq + std::hash::Hash + Send + Sync + 'static,
{
    fn to_string(&self) -> String {
        self.key.to_string()
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
        &self,
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
        &self,
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
            if key_string != "core:server_tick" {
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

// Convenience methods for StructuredEventKey to provide domain-based API
impl<P: EventPropagator<StructuredEventKey>> EventBus<StructuredEventKey, P> {
    /// Register an event handler using domain and event name
    /// 
    /// # Arguments
    /// * `domain` - The domain (first segment) like "core", "client", "plugin"
    /// * `event_name` - The event name (last segment)
    /// * `handler` - The event handler function
    pub async fn on<T, F>(
        &self,
        domain: &str,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let key = StructuredEventKey::domain_event(domain, event_name);
        self.on_key(key, handler).await
    }

    /// Register an event handler using domain, category, and event name
    /// 
    /// # Arguments
    /// * `domain` - The domain (first segment) like "core", "client", "plugin"
    /// * `category` - The category (middle segment) like "chat", "inventory"
    /// * `event_name` - The event name (last segment)
    /// * `handler` - The event handler function
    pub async fn on_category<T, F>(
        &self,
        domain: &str,
        category: &str,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let key = StructuredEventKey::domain_category_event(domain, category, event_name);
        self.on_key(key, handler).await
    }

    /// Emit an event using domain and event name
    /// 
    /// # Arguments
    /// * `domain` - The domain (first segment) like "core", "client", "plugin"
    /// * `event_name` - The event name (last segment)
    /// * `event` - The event data to emit
    pub async fn emit<T>(
        &self,
        domain: &str,
        event_name: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event + Serialize,
    {
        let key = StructuredEventKey::domain_event(domain, event_name);
        self.emit_key(key, event).await
    }

    /// Emit an event using domain, category, and event name
    /// 
    /// # Arguments
    /// * `domain` - The domain (first segment) like "core", "client", "plugin"
    /// * `category` - The category (middle segment) like "chat", "inventory"
    /// * `event_name` - The event name (last segment)
    /// * `event` - The event data to emit
    pub async fn emit_category<T>(
        &self,
        domain: &str,
        category: &str,
        event_name: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event + Serialize,
    {
        let key = StructuredEventKey::domain_category_event(domain, category, event_name);
        self.emit_key(key, event).await
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