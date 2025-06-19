//! High-performance event system implementation with socket.io-like API
//! 
//! Provides type-safe event handling with automatic JSON serialization and
//! efficient routing for game server use.

use types::*;
use async_trait::async_trait;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, trace, warn};

// ============================================================================
// Event Registry
// ============================================================================

/// Thread-safe registry for event handlers
pub struct EventRegistry {
    /// Map from EventId to list of handlers
    handlers: RwLock<HashMap<EventId, Vec<Arc<dyn EventHandler>>>>,
    /// Statistics
    stats: EventSystemStatsImpl,
}

impl std::fmt::Debug for EventRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventRegistry")
            .field("stats", &self.stats)
            .finish()
    }
}

impl EventRegistry {
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
            stats: EventSystemStatsImpl::new(),
        }
    }
    
    /// Register a handler for an event
    pub async fn register_handler(
        &self,
        event_id: EventId,
        handler: Arc<dyn EventHandler>,
    ) -> Result<(), EventError> {
        let mut handlers = self.handlers.write().await;
        
        handlers
            .entry(event_id.clone())
            .or_insert_with(Vec::new)
            .push(handler);
        
        debug!("Registered handler for event: {}", event_id);
        Ok(())
    }
    
    /// Emit an event to all registered handlers
    pub async fn emit_event(
        &self,
        event_id: &EventId,
        data: &[u8],
    ) -> Result<(), EventError> {
        let handlers = self.handlers.read().await;
        
        if let Some(event_handlers) = handlers.get(event_id) {
            self.stats.increment_emitted();
            
            // Execute all handlers concurrently
            let mut handles = Vec::new();
            
            for handler in event_handlers {
                let handler = handler.clone();
                let data = data.to_vec();
                let event_id = event_id.clone();
                
                let handle = tokio::spawn(async move {
                    match handler.handle(&data).await {
                        Ok(()) => {
                            trace!("Handler {} completed for event {}", 
                                  handler.handler_name(), event_id);
                        }
                        Err(e) => {
                            error!("Handler {} failed for event {}: {}", 
                                  handler.handler_name(), event_id, e);
                        }
                    }
                });
                
                handles.push(handle);
            }
            
            // Wait for all handlers to complete
            for handle in handles {
                if let Err(e) = handle.await {
                    error!("Handler task failed: {}", e);
                }
            }
            
            self.stats.increment_handled();
            debug!("Emitted event {} to {} handlers", event_id, event_handlers.len());
        } else {
            warn!("No handlers registered for event: {}", event_id);
        }
        
        Ok(())
    }
    
    /// Get statistics
    pub async fn get_stats(&self) -> EventSystemStats {
        let handlers = self.handlers.read().await;
        
        let mut events_by_namespace = HashMap::new();
        let mut total_handlers = 0;
        
        for (event_id, handlers_list) in handlers.iter() {
            let namespace = event_id.namespace.to_string();
            *events_by_namespace.entry(namespace).or_insert(0) += handlers_list.len();
            total_handlers += handlers_list.len();
        }
        
        EventSystemStats {
            total_handlers,
            events_by_namespace,
            total_events_emitted: self.stats.events_emitted.load(Ordering::Relaxed),
            total_events_handled: self.stats.events_handled.load(Ordering::Relaxed),
        }
    }
    
    /// Remove all handlers for an event (utility method)
    pub async fn remove_handlers(&self, event_id: &EventId) -> Result<usize, EventError> {
        let mut handlers = self.handlers.write().await;
        
        let removed_count = handlers
            .remove(event_id)
            .map(|handlers| handlers.len())
            .unwrap_or(0);
        
        debug!("Removed {} handlers for event: {}", removed_count, event_id);
        Ok(removed_count)
    }
}

/// Internal statistics tracking
#[derive(Debug)]
struct EventSystemStatsImpl {
    events_emitted: AtomicU64,
    events_handled: AtomicU64,
}

impl EventSystemStatsImpl {
    fn new() -> Self {
        Self {
            events_emitted: AtomicU64::new(0),
            events_handled: AtomicU64::new(0),
        }
    }
    
    fn increment_emitted(&self) {
        self.events_emitted.fetch_add(1, Ordering::Relaxed);
    }
    
    fn increment_handled(&self) {
        self.events_handled.fetch_add(1, Ordering::Relaxed);
    }
}

// ============================================================================
// Event System Implementation
// ============================================================================

/// Main event system implementation with socket.io-like API
pub struct EventSystemImpl {
    registry: Arc<EventRegistry>,
    default_namespace: EventNamespace,
}

impl EventSystemImpl {
    /// Create a new event system
    pub fn new() -> Self {
        Self {
            registry: Arc::new(EventRegistry::new()),
            default_namespace: EventNamespace::Core,
        }
    }
    
    /// Create with specific default namespace
    pub fn with_namespace(namespace: EventNamespace) -> Self {
        Self {
            registry: Arc::new(EventRegistry::new()),
            default_namespace: namespace,
        }
    }
    
    /// Helper to create EventId with default namespace
    fn make_event_id(&self, event_name: &str) -> EventId {
        EventId::new(self.default_namespace.clone(), event_name)
    }
    
    /// Remove all handlers for an event (convenience method)
    pub async fn remove_handlers(&self, event_name: &str) -> Result<usize, EventError> {
        let event_id = self.make_event_id(event_name);
        self.registry.remove_handlers(&event_id).await
    }
}

#[async_trait]
impl EventSystem for EventSystemImpl {
    /// Register a handler for an event
    async fn register_handler(&self, event_id: EventId, handler: Arc<dyn EventHandler>) -> Result<(), EventError> {
        self.registry.register_handler(event_id, handler).await
    }
    
    /// Emit raw JSON data to an event
    async fn emit_raw(&self, event_id: EventId, data: &[u8]) -> Result<(), EventError> {
        self.registry.emit_event(&event_id, data).await
    }
    
    /// Get statistics about the event system
    async fn get_stats(&self) -> EventSystemStats {
        self.registry.get_stats().await
    }
}

// ============================================================================
// Async Event Handler (for async closures)
// ============================================================================

/// Async version of event handler for more complex operations
#[async_trait]
pub trait AsyncEventHandler: Send + Sync {
    async fn handle(&self, data: &[u8]) -> Result<(), EventError>;
    fn expected_type_id(&self) -> TypeId;
    fn handler_name(&self) -> &str;
}

/// Concrete async handler implementation
pub struct TypedAsyncEventHandler<T, F, Fut>
where
    T: Event,
    F: Fn(T) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<(), EventError>> + Send + Sync + 'static,
{
    handler: F,
    name: String,
    _phantom: std::marker::PhantomData<(T, Fut)>,
}

impl<T, F, Fut> TypedAsyncEventHandler<T, F, Fut>
where
    T: Event,
    F: Fn(T) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<(), EventError>> + Send + Sync + 'static,
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
impl<T, F, Fut> AsyncEventHandler for TypedAsyncEventHandler<T, F, Fut>
where
    T: Event,
    F: Fn(T) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<(), EventError>> + Send + Sync + 'static,
{
    async fn handle(&self, data: &[u8]) -> Result<(), EventError> {
        let event = T::deserialize(data)?;
        (self.handler)(event).await
    }
    
    fn expected_type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }
    
    fn handler_name(&self) -> &str {
        &self.name
    }
}

// Bridge async handlers to sync interface
#[async_trait]
impl<T, F, Fut> EventHandler for TypedAsyncEventHandler<T, F, Fut>
where
    T: Event,
    F: Fn(T) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<(), EventError>> + Send + Sync + 'static,
{
    async fn handle(&self, data: &[u8]) -> Result<(), EventError> {
        AsyncEventHandler::handle(self, data).await
    }
    
    fn expected_type_id(&self) -> TypeId {
        AsyncEventHandler::expected_type_id(self)
    }
    
    fn handler_name(&self) -> &str {
        AsyncEventHandler::handler_name(self)
    }
}

// ============================================================================
// Additional Event System Extensions
// ============================================================================

/// Additional extension trait for event system utilities
#[async_trait]
pub trait EventSystemUtils {
    /// Register an async event handler
    async fn on_async<T, F, Fut>(&self, event_name: &str, handler: F) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<(), EventError>> + Send + Sync + 'static;
    
    /// Emit an event and wait for all handlers to complete
    async fn emit_and_wait<T>(&self, event_name: &str, event: &T) -> Result<(), EventError>
    where
        T: Event;
    
    /// Remove all handlers for an event by name
    async fn remove_handlers_by_name(&self, event_name: &str) -> Result<usize, EventError>;
}

#[async_trait]
impl EventSystemUtils for EventSystemImpl {
    async fn on_async<T, F, Fut>(&self, event_name: &str, handler: F) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<(), EventError>> + Send + Sync + 'static,
    {
        let event_id = self.make_event_id(event_name);
        let handler_name = format!("{}::{}", event_id, T::type_name());
        let async_handler = TypedAsyncEventHandler::new(handler_name, handler);
        let handler_arc: Arc<dyn EventHandler> = Arc::new(async_handler);
        
        self.registry.register_handler(event_id, handler_arc).await?;
        Ok(())
    }
    
    async fn emit_and_wait<T>(&self, event_name: &str, event: &T) -> Result<(), EventError>
    where
        T: Event,
    {
        // This implementation is the same as emit() since we already wait
        // for all handlers in the registry implementation
        use types::EventSystemExt;
        self.emit(event_name, event).await
    }
    
    async fn remove_handlers_by_name(&self, event_name: &str) -> Result<usize, EventError> {
        self.remove_handlers(event_name).await
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Create a new event system instance
pub fn create_event_system() -> Arc<dyn EventSystem> {
    Arc::new(EventSystemImpl::new())
}

/// Create event system with specific namespace
pub fn create_event_system_with_namespace(namespace: EventNamespace) -> Arc<dyn EventSystem> {
    Arc::new(EventSystemImpl::with_namespace(namespace))
}

/// Helper to get current timestamp
pub fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use types::EventSystemExt; // Import the extension trait
    
    // Mock Event implementation for testing
    #[derive(Debug, Clone)]
    struct TestEvent {
        message: String,
        value: i32,
    }
    
    impl Event for TestEvent {
        fn serialize(&self) -> Result<Vec<u8>, EventError> {
            Ok(format!("{}:{}", self.message, self.value).into_bytes())
        }
        
        fn deserialize(data: &[u8]) -> Result<Self, EventError> {
            let s = String::from_utf8_lossy(data);
            let parts: Vec<&str> = s.split(':').collect();
            if parts.len() == 2 {
                Ok(TestEvent {
                    message: parts[0].to_string(),
                    value: parts[1].parse().map_err(|e| EventError::Serialization(Box::new(e)))?,
                })
            } else {
                Err(EventError::Serialization(Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid format"
                ))))
            }
        }
        
        fn type_name() -> &'static str {
            "TestEvent"
        }
    }
    
    #[tokio::test]
    async fn test_basic_event_emission() {
        let events = create_event_system();
        
        let received = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let received_clone = received.clone();
        
        // Register handler - note we need EventSystemExt in scope
        events.on("test_event", move |event: TestEvent| {
            let received = received_clone.clone();
            tokio::spawn(async move {
                received.lock().await.push(event);
            });
            Ok(())
        }).await.unwrap();
        
        // Emit event
        events.emit("test_event", &TestEvent {
            message: "hello".to_string(),
            value: 42,
        }).await.unwrap();
        
        // Wait a bit for async handling
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        // Check received
        let received_events = received.lock().await;
        assert_eq!(received_events.len(), 1);
        assert_eq!(received_events[0].message, "hello");
        assert_eq!(received_events[0].value, 42);
    }
    
    #[tokio::test]
    async fn test_namespaced_events() {
        let events = create_event_system();
        
        let received = Arc::new(tokio::sync::Mutex::new(0));
        let received_clone = received.clone();
        
        // Register handler with namespace
        events.on_namespaced(
            EventId::plugin("test_plugin", "custom_event"),
            move |_event: TestEvent| {
                let received = received_clone.clone();
                tokio::spawn(async move {
                    *received.lock().await += 1;
                });
                Ok(())
            }
        ).await.unwrap();
        
        // Emit to different namespace - should not trigger
        events.emit("custom_event", &TestEvent {
            message: "test".to_string(),
            value: 1,
        }).await.unwrap();
        
        // Emit to correct namespace - should trigger
        events.emit_namespaced(
            EventId::plugin("test_plugin", "custom_event"),
            &TestEvent {
                message: "test".to_string(),
                value: 1,
            }
        ).await.unwrap();
        
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        let count = *received.lock().await;
        assert_eq!(count, 1);
    }
    
    #[tokio::test]
    async fn test_multiple_handlers() {
        let events = create_event_system();
        
        let counter = Arc::new(tokio::sync::Mutex::new(0));
        
        // Register multiple handlers for same event
        for _i in 0..3 {
            let counter_clone = counter.clone();
            events.on("multi_test", move |_event: TestEvent| {
                let counter = counter_clone.clone();
                tokio::spawn(async move {
                    *counter.lock().await += 1;
                });
                Ok(())
            }).await.unwrap();
        }
        
        // Emit event
        events.emit("multi_test", &TestEvent {
            message: "multi".to_string(),
            value: 123,
        }).await.unwrap();
        
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        
        let count = *counter.lock().await;
        assert_eq!(count, 3);
    }
}