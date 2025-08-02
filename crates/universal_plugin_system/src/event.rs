//! Core event system with flexible event propagation support

use crate::error::EventError;
use crate::propagation::EventPropagator;
use crate::cache::{SerializationBufferPool, CachedEventData};
use crate::monitoring::PerformanceMonitor;
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
    #[inline]
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
    #[inline]
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
    #[inline]
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
    #[inline]
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
    /// Event handlers organized by event key (lock-free with SmallVec optimization)
    handlers: DashMap<K, SmallVec<[Arc<dyn EventHandler>; 4]>>,
    /// Event propagation logic
    propagator: P,
    /// High-performance serialization buffer pool to reduce allocations
    serialization_pool: SerializationBufferPool,
    /// Statistics for monitoring performance
    stats: Arc<tokio::sync::RwLock<EventStats>>,
    /// Detailed performance monitor for advanced metrics
    performance_monitor: PerformanceMonitor,
    /// Phantom data for the key type
    _phantom: std::marker::PhantomData<K>,
}

impl<K: EventKeyType, P: EventPropagator<K>> EventBus<K, P> {
    /// Create a new event bus with custom propagator
    pub fn with_propagator(propagator: P) -> Self {
        Self {
            handlers: DashMap::new(),
            propagator,
            serialization_pool: SerializationBufferPool::new(),
            stats: Arc::new(tokio::sync::RwLock::new(EventStats::default())),
            performance_monitor: PerformanceMonitor::new(),
            _phantom: std::marker::PhantomData,
        }
    }

    /// Register a typed event handler with a custom event key
    #[inline]
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
    #[inline]
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

    /// Internal emit implementation with performance optimizations
    async fn emit_with_key<T>(
        &self,
        key: K,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event + Serialize,
    {
        let emit_start = std::time::Instant::now();
        
        // Serialize the event once using the buffer pool for zero-copy sharing
        let cached_event_data = CachedEventData::new(event, T::event_type().to_string(), &self.serialization_pool)?;
        let event_data = Arc::new(EventData {
            data: cached_event_data.data.clone(),
            type_name: cached_event_data.type_name.clone(),
            metadata: HashMap::new(),
        });

        // Get handlers for this event (lock-free access)
        let handlers = self.handlers.get(&key).map(|entry| entry.value().clone());

        if let Some(handlers) = handlers {
            if !handlers.is_empty() {
                debug!("📤 Emitting {} to {} handlers", key.to_string(), handlers.len());

                // Create propagation context
                let context = crate::propagation::PropagationContext {
                    event_key: key.clone(),
                    metadata: event_data.metadata.clone(),
                };

                // Use FuturesUnordered for concurrent handler execution
                let mut futures = FuturesUnordered::new();

                for handler in handlers.iter() {
                    // Check if this handler should receive the event
                    if self.propagator.should_propagate(&key, &context).await {
                        // Optionally transform the event (with zero-copy sharing)
                        let final_event = self.propagator
                            .transform_event(event_data.clone(), &context)
                            .await
                            .unwrap_or_else(|| event_data.clone());

                        let handler_clone = handler.clone();
                        let handler_name = handler.handler_name().to_string();
                        let performance_monitor = &self.performance_monitor;

                        futures.push(async move {
                            let handler_start = std::time::Instant::now();
                            let result = handler_clone.handle(&final_event).await;
                            let handler_duration = handler_start.elapsed();
                            
                            // Record handler execution metrics
                            performance_monitor.record_handler_execution(
                                &handler_name, 
                                handler_duration, 
                                result.is_ok()
                            ).await;
                            
                            if let Err(e) = &result {
                                error!("❌ Handler {} failed: {}", handler_name, e);
                            }
                            result
                        });
                    }
                }

                // Execute all handlers concurrently and collect results
                let mut success_count = 0;
                let mut failure_count = 0;

                while let Some(result) = futures.next().await {
                    match result {
                        Ok(_) => success_count += 1,
                        Err(_) => failure_count += 1,
                    }
                }

                // Update statistics atomically
                let mut stats = self.stats.write().await;
                stats.events_emitted += 1;
                stats.events_handled += success_count;
                stats.handler_failures += failure_count;
                
                // Record emission performance metrics
                let emit_duration = emit_start.elapsed();
                self.performance_monitor.record_emit(&cached_event_data.type_name, emit_duration).await;
            }
        } else {
            // No handlers found - simplified logging for typed keys
            let key_string = key.to_string();
            if key_string != "core:server_tick" {
                warn!("⚠️ No handlers for event: {}", key_string);
            }
            
            // Still record the emission for metrics
            let emit_duration = emit_start.elapsed();
            self.performance_monitor.record_emit(T::event_type(), emit_duration).await;
        }

        Ok(())
    }

    /// Get current statistics
    #[inline]
    pub async fn stats(&self) -> EventStats {
        self.stats.read().await.clone()
    }

    /// Get handler count
    #[inline]
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }

    /// Get all registered event keys
    #[inline]
    pub fn registered_keys(&self) -> Vec<K> {
        self.handlers.iter().map(|entry| entry.key().clone()).collect()
    }
    
    /// Get access to the performance monitor for detailed metrics
    #[inline]
    pub fn performance_monitor(&self) -> &PerformanceMonitor {
        &self.performance_monitor
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
    #[inline]
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

    /// Alias for on_category - register an event handler using domain, category, and event name
    /// 
    /// This provides compatibility with existing code that expects `on_categorized`.
    #[inline]
    pub async fn on_categorized<T, F>(
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
        self.on_category(domain, category, event_name, handler).await
    }

    /// Emit an event using domain and event name
    /// 
    /// # Arguments
    /// * `domain` - The domain (first segment) like "core", "client", "plugin"
    /// * `event_name` - The event name (last segment)
    /// * `event` - The event data to emit
    #[inline]
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

    /// Alias for emit_category - emit an event using domain, category, and event name
    /// 
    /// This provides compatibility with existing code that expects `emit_categorized`.
    #[inline]
    pub async fn emit_categorized<T>(
        &self,
        domain: &str,
        category: &str,
        event_name: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event + Serialize,
    {
        self.emit_category(domain, category, event_name, event).await
    }

    // ============================================================================
    // Event Class System - Different event classes with custom metadata
    // ============================================================================

    /// Register a handler for GORC-style events with spatial awareness
    /// 
    /// This event class is designed for game object replication with spatial metadata.
    /// The event key structure is: [domain, object_type, channel, event_name, spatial_aware]
    /// 
    /// # Arguments
    /// * `domain` - The domain (e.g., "gorc", "game")
    /// * `object_type` - The type of object (e.g., "Player", "Asteroid", "Ship")
    /// * `channel` - The replication channel number (0-255)
    /// * `event_name` - The event name (e.g., "position_update", "health_changed")
    /// * `spatial_aware` - Whether this event should use spatial propagation
    /// * `handler` - The event handler function
    /// 
    /// # Examples
    /// 
    /// ```rust,no_run
    /// use universal_plugin_system::*;
    /// use serde::{Serialize, Deserialize}; 
    /// use std::sync::Arc;
    /// 
    /// #[derive(Debug, Serialize, Deserialize)]
    /// struct PositionUpdateEvent { x: f32, y: f32, z: f32 }
    /// impl event::Event for PositionUpdateEvent {
    ///     fn event_type() -> &'static str { "position_update" }
    /// }
    /// 
    /// #[derive(Debug, Serialize, Deserialize)]
    /// struct InventoryEvent { item_count: u32 }
    /// impl event::Event for InventoryEvent {
    ///     fn event_type() -> &'static str { "inventory" }
    /// }
    /// 
    /// # #[tokio::main]
    /// # async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    /// let event_system: Arc<event::EventBus<event::StructuredEventKey, _>> = 
    ///     Arc::new(event::EventBus::with_propagator(propagation::AllEqPropagator::new()));
    /// 
    /// // Handler for spatially-aware player position updates
    /// event_system.on_gorc_class("gorc", "Player", 0, "position_update", true, 
    ///     |event: PositionUpdateEvent| {
    ///         println!("Player moved to ({}, {}, {})", event.x, event.y, event.z);
    ///         Ok(())
    ///     }
    /// ).await?;
    /// 
    /// // Handler for non-spatial inventory events  
    /// event_system.on_gorc_class("gorc", "Player", 1, "inventory_update", false,
    ///     |event: InventoryEvent| {
    ///         println!("Player inventory changed: {} items", event.item_count);
    ///         Ok(())
    ///     }
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn on_gorc_class<T, F>(
        &self,
        domain: &str,
        object_type: &str,
        channel: u8,
        event_name: &str,
        spatial_aware: bool,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let key = StructuredEventKey {
            segments: vec![
                domain.into(),
                object_type.into(),
                channel.to_string().into(),
                event_name.into(),
                spatial_aware.to_string().into(),
            ],
        };
        self.on_key(key, handler).await
    }

    /// Emit a GORC-style event with spatial awareness
    /// 
    /// # Arguments
    /// * `domain` - The domain (e.g., "gorc", "game")
    /// * `object_type` - The type of object (e.g., "Player", "Asteroid")
    /// * `channel` - The replication channel number (0-255)
    /// * `event_name` - The event name (e.g., "position_update")
    /// * `spatial_aware` - Whether this event should use spatial propagation
    /// * `event` - The event data to emit
    pub async fn emit_gorc_class<T>(
        &self,
        domain: &str,
        object_type: &str,
        channel: u8,
        event_name: &str,
        spatial_aware: bool,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event + Serialize,
    {
        let key = StructuredEventKey {
            segments: vec![
                domain.into(),
                object_type.into(),
                channel.to_string().into(),
                event_name.into(),
                spatial_aware.to_string().into(),
            ],
        };
        self.emit_key(key, event).await
    }

    /// Register a handler for custom event class with metadata and flags
    /// 
    /// This demonstrates how host applications can define their own event classes
    /// with custom metadata parameters. The event key structure is:
    /// [domain, event_name, metadata, flag]
    /// 
    /// # Arguments
    /// * `domain` - The domain (e.g., "core", "client", "custom")
    /// * `event_name` - The event name
    /// * `metadata` - Custom metadata string
    /// * `flag` - Custom boolean flag
    /// * `handler` - The event handler function
    /// 
    /// # Examples
    /// 
    /// ```rust,no_run
    /// use universal_plugin_system::*;
    /// use serde::{Serialize, Deserialize}; 
    /// use std::sync::Arc;
    /// 
    /// #[derive(Debug, Serialize, Deserialize)]
    /// struct UserLoginEvent { user_id: u32, username: String }
    /// impl event::Event for UserLoginEvent {
    ///     fn event_type() -> &'static str { "user_login" }
    /// }
    /// 
    /// # #[tokio::main]
    /// # async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    /// let event_system: Arc<event::EventBus<event::StructuredEventKey, _>> = 
    ///     Arc::new(event::EventBus::with_propagator(propagation::AllEqPropagator::new()));
    /// 
    /// // Handler for custom events with priority flag
    /// event_system.on_custom_evt_class("core", "user_login", "session_123", true,
    ///     |event: UserLoginEvent| {
    ///         println!("High priority login: {}", event.username);
    ///         Ok(())
    ///     }
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn on_custom_evt_class<T, F>(
        &self,
        domain: &str,
        event_name: &str,
        metadata: &str,
        flag: bool,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let key = StructuredEventKey {
            segments: vec![
                domain.into(),
                event_name.into(),
                metadata.into(),
                flag.to_string().into(),
            ],
        };
        self.on_key(key, handler).await
    }

    /// Emit a custom event class with metadata and flags
    /// 
    /// # Arguments
    /// * `domain` - The domain
    /// * `event_name` - The event name
    /// * `metadata` - Custom metadata string
    /// * `flag` - Custom boolean flag
    /// * `event` - The event data to emit
    pub async fn emit_custom_evt_class<T>(
        &self,
        domain: &str,
        event_name: &str,
        metadata: &str,
        flag: bool,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event + Serialize,
    {
        let key = StructuredEventKey {
            segments: vec![
                domain.into(),
                event_name.into(),
                metadata.into(),
                flag.to_string().into(),
            ],
        };
        self.emit_key(key, event).await
    }

    /// Register a handler for extended event class with multiple metadata fields
    /// 
    /// This event class demonstrates more complex metadata patterns that host
    /// applications might need. The event key structure is:
    /// [domain, category, event_name, priority, region, persistent]
    /// 
    /// # Arguments
    /// * `domain` - The domain
    /// * `category` - The category
    /// * `event_name` - The event name  
    /// * `priority` - Priority level (0-255)
    /// * `region` - Region identifier
    /// * `persistent` - Whether the event should be persisted
    /// * `handler` - The event handler function
    /// 
    /// # Examples
    /// 
    /// ```rust,no_run
    /// use universal_plugin_system::*;
    /// use serde::{Serialize, Deserialize}; 
    /// use std::sync::Arc;
    /// 
    /// #[derive(Debug, Serialize, Deserialize)]
    /// struct DamageEvent { attacker_id: u32, target_id: u32, amount: u32 }
    /// impl event::Event for DamageEvent {
    ///     fn event_type() -> &'static str { "damage_event" }
    /// }
    /// 
    /// # #[tokio::main]
    /// # async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    /// let event_system: Arc<event::EventBus<event::StructuredEventKey, _>> = 
    ///     Arc::new(event::EventBus::with_propagator(propagation::AllEqPropagator::new()));
    /// 
    /// // Handler for high-priority persistent events in a specific region
    /// event_system.on_extended_class("game", "combat", "damage_dealt", 255, "region_1", true,
    ///     |event: DamageEvent| {
    ///         println!("Critical damage event: {}", event.amount);
    ///         Ok(())
    ///     }
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn on_extended_class<T, F>(
        &self,
        domain: &str,
        category: &str,
        event_name: &str,
        priority: u8,
        region: &str,
        persistent: bool,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let key = StructuredEventKey {
            segments: vec![
                domain.into(),
                category.into(),
                event_name.into(),
                priority.to_string().into(),
                region.into(),
                persistent.to_string().into(),
            ],
        };
        self.on_key(key, handler).await
    }

    /// Emit an extended event class with multiple metadata fields
    /// 
    /// # Arguments
    /// * `domain` - The domain
    /// * `category` - The category
    /// * `event_name` - The event name
    /// * `priority` - Priority level (0-255)
    /// * `region` - Region identifier
    /// * `persistent` - Whether the event should be persisted
    /// * `event` - The event data to emit
    pub async fn emit_extended_class<T>(
        &self,
        domain: &str,
        category: &str,
        event_name: &str,
        priority: u8,
        region: &str,
        persistent: bool,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event + Serialize,
    {
        let key = StructuredEventKey {
            segments: vec![
                domain.into(),
                category.into(),
                event_name.into(),
                priority.to_string().into(),
                region.into(),
                persistent.to_string().into(),
            ],
        };
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