//! High-performance event system implementation with type-safe client events
//! 
//! Provides type-safe event handling with automatic JSON serialization and
//! efficient routing for game server use with namespace-based organization.

use async_trait::async_trait;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, trace, warn, info};

pub mod types;
pub use types::*;

// ============================================================================
// Event Registry Implementation
// ============================================================================

/// Thread-safe registry for event handlers with namespace optimization
pub struct EventRegistry {
    /// Map from EventId to list of handlers
    handlers: RwLock<HashMap<EventId, Vec<Arc<dyn EventHandler>>>>,
    /// Namespace-based handler lookup optimization
    namespace_index: RwLock<HashMap<EventNamespace, Vec<EventId>>>,
    /// Statistics tracking
    stats: EventSystemStatsImpl,
    /// Handler performance metrics
    handler_metrics: Arc<RwLock<HashMap<String, HandlerMetrics>>>,
}

#[derive(Debug, Default, Clone)]
struct HandlerMetrics {
    total_executions: u64,
    total_execution_time_ms: u64,
    average_execution_time_ms: f64,
    last_execution: Option<std::time::SystemTime>,
    error_count: u64,
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
            namespace_index: RwLock::new(HashMap::new()),
            stats: EventSystemStatsImpl::new(),
            handler_metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Register a handler for an event with enhanced metrics
    pub async fn register_handler(
        &self,
        event_id: EventId,
        handler: Arc<dyn EventHandler>,
    ) -> Result<(), EventError> {
        let mut handlers = self.handlers.write().await;
        let mut namespace_index = self.namespace_index.write().await;
        
        // Add to main handler registry
        handlers
            .entry(event_id.clone())
            .or_insert_with(Vec::new)
            .push(handler.clone());
        
        // Update namespace index for efficient lookups
        namespace_index
            .entry(event_id.namespace.clone())
            .or_insert_with(Vec::new)
            .push(event_id.clone());
        
        // Initialize handler metrics
        let mut metrics = self.handler_metrics.write().await;
        metrics.insert(
            handler.handler_name().to_string(),
            HandlerMetrics::default()
        );
        
        // Update stats
        self.stats.increment_handlers_registered();
        
        debug!("✅ Registered handler '{}' for event: {}", 
               handler.handler_name(), event_id);
        Ok(())
    }

    /// Emit an event to all registered handlers with performance tracking
    pub async fn emit_event(
        &self,
        event_id: &EventId,
        data: &[u8],
    ) -> Result<(), EventError> {
        let start_time = std::time::Instant::now();
        let handlers = self.handlers.read().await;
        
        if let Some(event_handlers) = handlers.get(event_id) {
            self.stats.increment_emitted(&event_id.namespace);
            
            let handler_count = event_handlers.len();
            debug!("📤 Emitting event {} to {} handlers", event_id, handler_count);
            
            // Execute all handlers concurrently with individual performance tracking
            let mut handles = Vec::new();
            
            for handler in event_handlers {
                let handler = handler.clone();
                let data = data.to_vec();
                let event_id = event_id.clone();
                let handler_metrics = self.handler_metrics.clone();
                
                let handle = tokio::spawn(async move {
                    let handler_start = std::time::Instant::now();
                    let handler_name = handler.handler_name().to_string();
                    
                    match handler.handle(&data).await {
                        Ok(()) => {
                            let execution_time = handler_start.elapsed();
                            
                            // Update handler metrics
                            let mut metrics = handler_metrics.write().await;
                            if let Some(metric) = metrics.get_mut(&handler_name) {
                                metric.total_executions += 1;
                                let execution_ms = execution_time.as_millis() as u64;
                                metric.total_execution_time_ms += execution_ms;
                                metric.average_execution_time_ms = 
                                    metric.total_execution_time_ms as f64 / metric.total_executions as f64;
                                metric.last_execution = Some(std::time::SystemTime::now());
                            }
                            
                            trace!("✅ Handler '{}' completed for event {} in {:?}", 
                                  handler_name, event_id, execution_time);
                        }
                        Err(e) => {
                            // Update error metrics
                            let mut metrics = handler_metrics.write().await;
                            if let Some(metric) = metrics.get_mut(&handler_name) {
                                metric.error_count += 1;
                            }
                            
                            error!("❌ Handler '{}' failed for event {}: {}", 
                                  handler_name, event_id, e);
                        }
                    }
                });
                
                handles.push(handle);
            }
            
            // Wait for all handlers to complete
            let mut successful_handlers = 0;
            let mut failed_handlers = 0;
            
            for handle in handles {
                match handle.await {
                    Ok(()) => successful_handlers += 1,
                    Err(e) => {
                        failed_handlers += 1;
                        error!("Handler task panicked: {}", e);
                    }
                }
            }
            
            let total_time = start_time.elapsed();
            self.stats.increment_handled(&event_id.namespace);
            
            if failed_handlers > 0 {
                warn!("⚠️ Event {} completed with {}/{} handlers successful (took {:?})", 
                      event_id, successful_handlers, handler_count, total_time);
            } else {
                debug!("✅ Event {} completed successfully on all {} handlers (took {:?})", 
                       event_id, handler_count, total_time);
            }
        } else {
            warn!("⚠️ No handlers registered for event: {}", event_id);
            self.stats.increment_unhandled_events();
        }
        
        Ok(())
    }
    
    /// Get comprehensive statistics
    pub async fn get_stats(&self) -> EventSystemStats {
        let handlers = self.handlers.read().await;
        let namespace_index = self.namespace_index.read().await;
        
        let mut events_by_namespace = HashMap::new();
        let mut total_handlers = 0;
        
        // Calculate handlers per namespace
        for (namespace, event_ids) in namespace_index.iter() {
            let mut namespace_handler_count = 0;
            for event_id in event_ids {
                if let Some(handlers_list) = handlers.get(event_id) {
                    namespace_handler_count += handlers_list.len();
                }
            }
            events_by_namespace.insert(namespace.to_string(), namespace_handler_count);
            total_handlers += namespace_handler_count;
        }
        
        EventSystemStats {
            total_handlers,
            events_by_namespace,
            total_events_emitted: self.stats.events_emitted.load(Ordering::Relaxed),
            total_events_handled: self.stats.events_handled.load(Ordering::Relaxed),
        }
    }
    
    /// Remove all handlers for an event
    pub async fn remove_handlers(&self, event_id: &EventId) -> Result<usize, EventError> {
        let mut handlers = self.handlers.write().await;
        let mut namespace_index = self.namespace_index.write().await;
        
        let removed_count = handlers
            .remove(event_id)
            .map(|handlers| handlers.len())
            .unwrap_or(0);
        
        // Clean up namespace index
        if let Some(event_ids) = namespace_index.get_mut(&event_id.namespace) {
            event_ids.retain(|id| id != event_id);
            if event_ids.is_empty() {
                namespace_index.remove(&event_id.namespace);
            }
        }
        
        debug!("🗑️ Removed {} handlers for event: {}", removed_count, event_id);
        Ok(removed_count)
    }
    
    /// Get handlers for a specific namespace (useful for debugging)
    pub async fn get_namespace_handlers(&self, namespace: &EventNamespace) -> Vec<EventId> {
        let namespace_index = self.namespace_index.read().await;
        namespace_index
            .get(namespace)
            .cloned()
            .unwrap_or_default()
    }
}

/// Internal statistics tracking with namespace breakdown
#[derive(Debug)]
struct EventSystemStatsImpl {
    events_emitted: AtomicU64,
    events_handled: AtomicU64,
    unhandled_events: AtomicU64,
    handlers_registered: AtomicU64,
    system_start_time: std::time::Instant,
}

impl EventSystemStatsImpl {
    fn new() -> Self {
        Self {
            events_emitted: AtomicU64::new(0),
            events_handled: AtomicU64::new(0),
            unhandled_events: AtomicU64::new(0),
            handlers_registered: AtomicU64::new(0),
            system_start_time: std::time::Instant::now(),
        }
    }
    
    fn increment_emitted(&self, _namespace: &EventNamespace) {
        self.events_emitted.fetch_add(1, Ordering::Relaxed);
        // Note: In practice, you'd want to implement proper async increment for namespace stats
    }
    
    fn increment_handled(&self, _namespace: &EventNamespace) {
        self.events_handled.fetch_add(1, Ordering::Relaxed);
        // Note: In practice, you'd want to implement proper async increment for namespace stats
    }
    
    fn increment_unhandled_events(&self) {
        self.unhandled_events.fetch_add(1, Ordering::Relaxed);
    }
    
    fn increment_handlers_registered(&self) {
        self.handlers_registered.fetch_add(1, Ordering::Relaxed);
    }
}

// ============================================================================
// Enhanced Event System Implementation
// ============================================================================

/// Main event system implementation with client event integration
pub struct EventSystemImpl {
    registry: Arc<EventRegistry>,
    default_namespace: EventNamespace,
    client_event_router: Option<Arc<ClientEventRouter>>,
}

impl EventSystemImpl {
    /// Create a new event system
    pub fn new() -> Self {
        Self {
            registry: Arc::new(EventRegistry::new()),
            default_namespace: EventNamespace::Core,
            client_event_router: None,
        }
    }
    
    /// Create with specific default namespace
    pub fn with_namespace(namespace: EventNamespace) -> Self {
        Self {
            registry: Arc::new(EventRegistry::new()),
            default_namespace: namespace,
            client_event_router: None,
        }
    }
    
    /// Create with client event routing capabilities
    pub fn with_client_routing() -> Self {
        let mut system = Self::new();
        system.client_event_router = Some(Arc::new(ClientEventRouter::new()));
        system
    }
    
    /// Helper to create EventId with default namespace
    fn make_event_id(&self, event_name: &str) -> EventId {
        EventId::new(self.default_namespace.clone(), event_name)
    }
    
    /// Get the client event router
    pub fn get_client_router(&self) -> Option<Arc<ClientEventRouter>> {
        self.client_event_router.clone()
    }
    
    /// Remove all handlers for an event (convenience method)
    pub async fn remove_handlers(&self, event_name: &str) -> Result<usize, EventError> {
        let event_id = self.make_event_id(event_name);
        self.registry.remove_handlers(&event_id).await
    }
    
    /// Get namespace-specific handlers (debugging)
    pub async fn get_namespace_handlers(&self, namespace: &EventNamespace) -> Vec<EventId> {
        self.registry.get_namespace_handlers(namespace).await
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
// Client Event System Extensions Implementation
// ============================================================================

/// Implementation of the client event system extensions
#[async_trait]
impl ClientEventSystemExt for EventSystemImpl {
    async fn on_client<T, F>(&self, event_name: &str, handler: F) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static,
    {
        let event_id = EventId::client(event_name);
        let handler_name = format!("client:{}::{}", event_name, T::type_name());
        let typed_handler = TypedEventHandler::new(handler_name, handler);
        let handler_arc: Arc<dyn EventHandler> = Arc::new(typed_handler);
        
        self.registry.register_handler(event_id, handler_arc).await?;
        info!("📱 Registered client event handler: {} -> {}", event_name, T::type_name());
        Ok(())
    }
    
    async fn emit_client<T>(&self, event_name: &str, event: &T) -> Result<(), EventError>
    where
        T: Event,
    {
        let event_id = EventId::client(event_name);
        let data = event.serialize()?;
        self.registry.emit_event(&event_id, &data).await?;
        debug!("📱 Emitted client event: {} ({})", event_name, T::type_name());
        Ok(())
    }
    
    async fn on_core<T, F>(&self, event_name: &str, handler: F) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static,
    {
        let event_id = EventId::core(event_name);
        let handler_name = format!("core:{}::{}", event_name, T::type_name());
        let typed_handler = TypedEventHandler::new(handler_name, handler);
        let handler_arc: Arc<dyn EventHandler> = Arc::new(typed_handler);
        
        self.registry.register_handler(event_id, handler_arc).await?;
        info!("🔧 Registered core event handler: {} -> {}", event_name, T::type_name());
        Ok(())
    }
    
    async fn on_plugin<T, F>(&self, plugin_name: &str, event_name: &str, handler: F) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static,
    {
        let event_id = EventId::plugin(plugin_name, event_name);
        let handler_name = format!("plugin:{}:{}::{}", plugin_name, event_name, T::type_name());
        let typed_handler = TypedEventHandler::new(handler_name, handler);
        let handler_arc: Arc<dyn EventHandler> = Arc::new(typed_handler);
        
        self.registry.register_handler(event_id, handler_arc).await?;
        info!("🔌 Registered plugin event handler: {}::{} -> {}", plugin_name, event_name, T::type_name());
        Ok(())
    }
}

// ============================================================================
// Client Event Router
// ============================================================================

/// Routes raw client messages to typed events
pub struct ClientEventRouter {
    event_mappings: RwLock<HashMap<String, EventTypeInfo>>,
    routing_stats: RwLock<ClientEventRoutingStats>,
}

#[derive(Debug, Clone)]
struct EventTypeInfo {
    type_name: String,
    expected_fields: Vec<String>,
    last_routed: std::time::SystemTime,
    route_count: u64,
}

#[derive(Debug, Default, Clone)]
pub struct ClientEventRoutingStats {
    pub total_messages_routed: u64,
    pub successful_routes: u64,
    pub failed_routes: u64,
    pub routes_by_type: HashMap<String, u64>,
}

impl ClientEventRouter {
    pub fn new() -> Self {
        let mut event_mappings = HashMap::new();
        
        // Register known client event types
        event_mappings.insert("chat_message".to_string(), EventTypeInfo {
            type_name: "ChatMessageEvent".to_string(),
            expected_fields: vec!["message".to_string(), "channel".to_string()],
            last_routed: std::time::SystemTime::now(),
            route_count: 0,
        });
        
        event_mappings.insert("move_command".to_string(), EventTypeInfo {
            type_name: "MoveCommandEvent".to_string(),
            expected_fields: vec!["target_x".to_string(), "target_y".to_string(), "target_z".to_string()],
            last_routed: std::time::SystemTime::now(),
            route_count: 0,
        });
        
        event_mappings.insert("combat_action".to_string(), EventTypeInfo {
            type_name: "CombatActionEvent".to_string(),
            expected_fields: vec!["action_type".to_string(), "ability_id".to_string()],
            last_routed: std::time::SystemTime::now(),
            route_count: 0,
        });
        
        Self {
            event_mappings: RwLock::new(event_mappings),
            routing_stats: RwLock::new(ClientEventRoutingStats::default()),
        }
    }
    
    /// Route a client message to the appropriate typed event
    pub async fn route_message(
        &self,
        message_type: &str,
        raw_data: &[u8],
        player_id: PlayerId,
        event_system: Arc<EventSystemImpl>,
    ) -> Result<(), EventError> {
        println!("🔄 Routing client message: {} for player {}", message_type, player_id);

        let mut stats = self.routing_stats.write().await;
        stats.total_messages_routed += 1;
        
        let routing_result = match message_type {
            "chat_message" => {
                self.route_chat_message(raw_data, player_id, event_system.clone()).await
            }
            
            "move_command" => {
                self.route_move_command(raw_data, player_id, event_system.clone()).await
            }
            
            "combat_action" => {
                self.route_combat_action(raw_data, player_id, event_system.clone()).await
            }
            
            "crafting_request" => {
                self.route_crafting_request(raw_data, player_id, event_system.clone()).await
            }
            
            "player_interaction" => {
                self.route_player_interaction(raw_data, player_id, event_system.clone()).await
            }
            
            _ => {
                warn!("🤷 Unknown client message type: {}", message_type);
                Err(EventError::HandlerNotFound(format!("Unknown message type: {}", message_type)))
            }
        };
        
        match routing_result {
            Ok(()) => {
                stats.successful_routes += 1;
                *stats.routes_by_type.entry(message_type.to_string()).or_insert(0) += 1;
                
                // Update event mapping stats
                let mut mappings = self.event_mappings.write().await;
                if let Some(mapping) = mappings.get_mut(message_type) {
                    mapping.route_count += 1;
                    mapping.last_routed = std::time::SystemTime::now();
                }
                
                debug!("✅ Successfully routed {} event for player {}", message_type, player_id);
            }
            Err(ref e) => {
                stats.failed_routes += 1;
                error!("❌ Failed to route {} event for player {}: {:?}", message_type, player_id, e);
            }
        }
        
        routing_result
    }
    
    /// Route chat message events
    async fn route_chat_message(
        &self,
        raw_data: &[u8],
        player_id: PlayerId,
        event_system: Arc<EventSystemImpl>,
    ) -> Result<(), EventError> {
        let chat_data: serde_json::Value = serde_json::from_slice(raw_data)
            .map_err(EventError::Deserialization)?;
        
        let chat_event = ChatMessageEvent {
            player_id,
            message: chat_data["message"].as_str().unwrap_or("").to_string(),
            channel: chat_data["channel"].as_str().map(|s| s.to_string()),
            timestamp: current_timestamp(),
        };
        
        let data = chat_event.serialize()?;
        event_system.emit_raw(EventId::client("chat_message"), &data).await?;
        Ok(())
    }
    
    /// Route movement command events
    async fn route_move_command(
        &self,
        raw_data: &[u8],
        player_id: PlayerId,
        event_system: Arc<EventSystemImpl>,
    ) -> Result<(), EventError> {
        let move_data: serde_json::Value = serde_json::from_slice(raw_data)
            .map_err(EventError::Deserialization)?;
        
        let move_event = MoveCommandEvent {
            player_id,
            target_x: move_data["target_x"].as_f64().unwrap_or(0.0),
            target_y: move_data["target_y"].as_f64().unwrap_or(0.0),
            target_z: move_data["target_z"].as_f64().unwrap_or(0.0),
            movement_type: match move_data["type"].as_str() {
                Some("run") => MovementType::Run,
                Some("teleport") => MovementType::Teleport,
                _ => MovementType::Walk,
            },
        };
        
        let data = move_event.serialize()?;
        event_system.emit_raw(EventId::client("move_command"), &data).await?;
        Ok(())
    }
    
    /// Route combat action events
    async fn route_combat_action(
        &self,
        raw_data: &[u8],
        player_id: PlayerId,
        event_system: Arc<EventSystemImpl>,
    ) -> Result<(), EventError> {
        let combat_event: CombatActionEvent = serde_json::from_slice(raw_data)
            .map_err(EventError::Deserialization)?;
        
        let data = combat_event.serialize()?;
        event_system.emit_raw(EventId::client("combat_action"), &data).await?;
        Ok(())
    }
    
    /// Route crafting request events
    async fn route_crafting_request(
        &self,
        raw_data: &[u8],
        player_id: PlayerId,
        event_system: Arc<EventSystemImpl>,
    ) -> Result<(), EventError> {
        let crafting_event: CraftingRequestEvent = serde_json::from_slice(raw_data)
            .map_err(EventError::Deserialization)?;
        
        let data = crafting_event.serialize()?;
        event_system.emit_raw(EventId::client("crafting_request"), &data).await?;
        Ok(())
    }
    
    /// Route player interaction events
    async fn route_player_interaction(
        &self,
        raw_data: &[u8],
        player_id: PlayerId,
        event_system: Arc<EventSystemImpl>,
    ) -> Result<(), EventError> {
        let interaction_event: PlayerInteractionEvent = serde_json::from_slice(raw_data)
            .map_err(EventError::Deserialization)?;
        
        let data = interaction_event.serialize()?;
        event_system.emit_raw(EventId::client("player_interaction"), &data).await?;
        Ok(())
    }
    
    /// Get routing statistics
    pub async fn get_stats(&self) -> ClientEventRoutingStats {
        let stats = self.routing_stats.read().await;
        stats.clone()
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Create a new event system instance
pub fn create_event_system() -> Arc<EventSystemImpl> {
    Arc::new(EventSystemImpl::new())
}

/// Create event system with client routing
pub fn create_event_system_with_routing() -> Arc<EventSystemImpl> {
    Arc::new(EventSystemImpl::with_client_routing())
}

/// Create event system with specific namespace
pub fn create_event_system_with_namespace(namespace: EventNamespace) -> Arc<EventSystemImpl> {
    Arc::new(EventSystemImpl::with_namespace(namespace))
}

/// Create event system with client event router
pub fn create_event_system_with_router() -> (Arc<EventSystemImpl>, Arc<ClientEventRouter>) {
    let event_system = create_event_system();
    let router = Arc::new(ClientEventRouter::new());
    (event_system, router)
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
    
    #[tokio::test]
    async fn test_basic_event_emission() {
        let events = create_event_system();
        
        let received = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let received_clone = received.clone();
        
        // Register handler
        events.on("test_event", move |event: ChatMessageEvent| {
            let received = received_clone.clone();
            tokio::spawn(async move {
                received.lock().await.push(event);
            });
            Ok(())
        }).await.unwrap();
        
        // Emit event
        events.emit("test_event", &ChatMessageEvent {
            player_id: PlayerId::new(),
            message: "hello".to_string(),
            channel: Some("test".to_string()),
            timestamp: current_timestamp(),
        }).await.unwrap();
        
        // Wait a bit for async handling
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        // Check received
        let received_events = received.lock().await;
        assert_eq!(received_events.len(), 1);
        assert_eq!(received_events[0].message, "hello");
    }
    
    #[tokio::test]
    async fn test_client_event_routing() {
        let (event_system, router) = create_event_system_with_router();
        let player_id = PlayerId::new();
        
        // Test chat message routing
        let chat_data = serde_json::json!({
            "message": "Hello world!",
            "channel": "general"
        });
        
        let result = router.route_message(
            "chat_message",
            &serde_json::to_vec(&chat_data).unwrap(),
            player_id,
            event_system,
        ).await;
        
        assert!(result.is_ok());
        
        // Check routing stats
        let stats = router.get_stats().await;
        assert_eq!(stats.total_messages_routed, 1);
        assert_eq!(stats.successful_routes, 1);
    }
    
    #[tokio::test]
    async fn test_namespace_handlers() {
        let events = create_event_system_with_routing();
        
        // Register handlers in different namespaces
        events.on_client("test_client", |_: ChatMessageEvent| Ok(())).await.unwrap();
        events.on_core("test_core", |_: PlayerJoinedEvent| Ok(())).await.unwrap();
        events.on_plugin("test_plugin", "test_event", |_: UseItemEvent| Ok(())).await.unwrap();
        
        // Check namespace handlers
        let client_handlers = events.get_namespace_handlers(&EventNamespace::Client).await;
        let core_handlers = events.get_namespace_handlers(&EventNamespace::Core).await;
        let plugin_handlers = events.get_namespace_handlers(&EventNamespace::Plugin("test_plugin".to_string())).await;
        
        assert_eq!(client_handlers.len(), 1);
        assert_eq!(core_handlers.len(), 1);
        assert_eq!(plugin_handlers.len(), 1);
    }
}