//! # Event System Core Implementation
//!
//! This module contains the core [`EventSystem`] implementation that manages
//! event routing, handler execution, and system statistics. It provides the
//! central hub for all event processing in the Horizon Event System with
//! full GORC integration for object instance events.

use crate::events::{Event, EventHandler, TypedEventHandler, EventError, GorcEvent};
use crate::gorc::instance::{GorcObjectId, GorcInstanceManager, ObjectInstance};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use serde::{Deserialize, Serialize};

/// The core event system that manages event routing and handler execution.
/// 
/// This is the central hub for all event processing in the system. It provides
/// type-safe event registration and emission with support for different event
/// categories (core, client, plugin, and GORC instance events).
#[derive(Debug)]
pub struct EventSystem {
    /// Map of event keys to their registered handlers
    handlers: RwLock<HashMap<String, Vec<Arc<dyn EventHandler>>>>,
    /// System statistics for monitoring
    stats: RwLock<EventSystemStats>,
    /// GORC instance manager for object-specific events
    gorc_instances: Option<Arc<GorcInstanceManager>>,
}

impl EventSystem {
    /// Creates a new event system with no registered handlers.
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
            stats: RwLock::new(EventSystemStats::default()),
            gorc_instances: None,
        }
    }

    /// Creates a new event system with GORC instance manager integration
    pub fn with_gorc(gorc_instances: Arc<GorcInstanceManager>) -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
            stats: RwLock::new(EventSystemStats::default()),
            gorc_instances: Some(gorc_instances),
        }
    }

    /// Sets the GORC instance manager for this event system
    pub fn set_gorc_instances(&mut self, gorc_instances: Arc<GorcInstanceManager>) {
        self.gorc_instances = Some(gorc_instances);
    }

    /// Registers a handler for core server events.
    pub async fn on_core<T, F>(&self, event_name: &str, handler: F) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let event_key = format!("core:{event_name}");
        self.register_typed_handler(event_key, event_name, handler)
            .await
    }

    /// Registers a handler for client events with namespace.
    pub async fn on_client<T, F>(
        &self,
        namespace: &str,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let event_key = format!("client:{namespace}:{event_name}");
        self.register_typed_handler(event_key, event_name, handler)
            .await
    }

    /// Registers a handler for plugin-to-plugin events.
    pub async fn on_plugin<T, F>(
        &self,
        plugin_name: &str,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let event_key = format!("plugin:{plugin_name}:{event_name}");
        self.register_typed_handler(event_key, event_name, handler)
            .await
    }

    /// Registers a handler for GORC instance events with direct object access.
    /// 
    /// This provides access to both the GORC event and the actual object instance,
    /// allowing handlers to directly modify object state or access object-specific data.
    /// 
    /// # Arguments
    /// 
    /// * `object_type` - Type of the game object (e.g., "Asteroid", "Player")
    /// * `channel` - Replication channel (0=Critical, 1=Detailed, 2=Cosmetic, 3=Metadata)
    /// * `event_name` - Specific event name (e.g., "position_update", "health_change")
    /// * `handler` - Function that handles the event and has access to the object instance
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// // Handle asteroid position updates with access to the actual asteroid instance
    /// events.on_gorc_instance("Asteroid", 0, "position_update", 
    ///     |event: GorcEvent, asteroid_instance: &mut ObjectInstance| {
    ///         if let Some(asteroid) = asteroid_instance.get_object_mut::<Asteroid>() {
    ///             // Direct access to the asteroid object
    ///             println!("Asteroid at {:?} updated position", asteroid.position());
    ///             Ok(())
    ///         } else {
    ///             Err(EventError::HandlerExecution("Failed to get asteroid".to_string()))
    ///         }
    ///     }
    /// ).await?;
    /// ```
    pub async fn on_gorc_instance<F>(
        &self,
        object_type: &str,
        channel: u8,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        F: Fn(GorcEvent, &mut ObjectInstance) -> Result<(), EventError> + Send + Sync + 'static,
    {
        let event_key = format!("gorc_instance:{}:{}:{}", object_type, channel, event_name);
        
        // Store the handler with the GORC instance manager reference
        let gorc_instances = self.gorc_instances.clone();
        let handler = Arc::new(handler);

        // Wrapper: auto-resolve UUID to object ref, handle errors, and keep API clean
        let wrapper_handler = move |event: GorcEvent| -> Result<(), EventError> {
            let instances = gorc_instances.as_ref().ok_or_else(|| {
                EventError::HandlerExecution("GORC instance manager not available".to_string())
            })?;

            let object_id = GorcObjectId::from_str(&event.object_id).map_err(|e| {
                EventError::HandlerExecution(format!("Invalid object ID in GORC event: {}", e))
            })?;

            // Use a channel to bridge async instance lookup to sync handler
            let (tx, rx) = std::sync::mpsc::channel();
            let instances_clone = instances.clone();
            let handler_clone = handler.clone();
            let event_clone = event.clone();

            // Spawn async task to resolve object and call handler
            let runtime = tokio::runtime::Handle::current();
            runtime.spawn(async move {
                let result = if let Some(mut instance) = instances_clone.get_object(object_id).await {
                    handler_clone(event_clone, &mut instance)
                } else {
                    Err(EventError::HandlerNotFound(format!("Object instance {} not found", object_id)))
                };
                let _ = tx.send(result);
            });

            // Wait for result (could add timeout in future)
            rx.recv().unwrap_or_else(|_| Err(EventError::HandlerExecution("Handler execution failed to complete".to_string())))
        };

        let handler_name = format!("{}::{}", event_key, "GorcInstanceEvent");
        let typed_handler = TypedEventHandler::new(handler_name, wrapper_handler);
        let handler_arc: Arc<dyn EventHandler> = Arc::new(typed_handler);

        let mut handlers = self.handlers.write().await;
        handlers.entry(event_key.clone()).or_insert_with(Vec::new).push(handler_arc);

        let mut stats = self.stats.write().await;
        stats.total_handlers += 1;

        info!("📝 Registered GORC instance handler for {}", event_key);
        Ok(())
    }

    /// Registers a handler for basic GORC events (legacy API).
    /// 
    /// This is the simpler API that just receives the GORC event without object instance access.
    /// Use `on_gorc_instance` for more advanced use cases where you need object access.
    pub async fn on_gorc<T, F>(
        &self,
        object_type: &str,
        channel: u8,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let event_key = format!("gorc:{}:{}:{}", object_type, channel, event_name);
        self.register_typed_handler(event_key, event_name, handler)
            .await
    }

    /// Internal helper for registering typed handlers.
    async fn register_typed_handler<T, F>(
        &self,
        event_key: String,
        _event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
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
    pub async fn emit_core<T>(&self, event_name: &str, event: &T) -> Result<(), EventError>
    where
        T: Event,
    {
        let event_key = format!("core:{event_name}");
        self.emit_event(&event_key, event).await
    }

    /// Emits a client event to all registered handlers.
    pub async fn emit_client<T>(
        &self,
        namespace: &str,
        event_name: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event,
    {
        let event_key = format!("client:{namespace}:{event_name}");
        self.emit_event(&event_key, event).await
    }

    /// Emits a plugin event to all registered handlers.
    pub async fn emit_plugin<T>(
        &self,
        plugin_name: &str,
        event_name: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event,
    {
        let event_key = format!("plugin:{plugin_name}:{event_name}");
        self.emit_event(&event_key, event).await
    }

    /// Emits a GORC instance event for a specific object instance.
    /// 
    /// This is the new API for emitting events that target specific object instances.
    /// The event will only be delivered to handlers that are registered for this
    /// specific object type, channel, and event name.
    /// 
    /// # Arguments
    /// 
    /// * `object_id` - The specific object instance to emit the event for
    /// * `channel` - Replication channel for the event
    /// * `event_name` - Name of the specific event
    /// * `event` - The event data to emit
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// // Emit a position update for a specific asteroid instance
    /// events.emit_gorc_instance(asteroid_id, 0, "position_update", &GorcEvent {
    ///     object_id: asteroid_id.to_string(),
    ///     object_type: "Asteroid".to_string(),
    ///     channel: 0,
    ///     data: position_data,
    ///     priority: "Critical".to_string(),
    ///     timestamp: current_timestamp(),
    /// }).await?;
    /// ```
    pub async fn emit_gorc_instance<T>(
        &self,
        object_id: GorcObjectId,
        channel: u8,
        event_name: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event,
    {
        // First, get the object instance to determine its type
        if let Some(ref gorc_instances) = self.gorc_instances {
            if let Some(instance) = gorc_instances.get_object(object_id).await {
                let object_type = &instance.type_name;
                
                // Emit to both the instance-specific and general GORC handlers
                let instance_key = format!("gorc_instance:{}:{}:{}", object_type, channel, event_name);
                let general_key = format!("gorc:{}:{}:{}", object_type, channel, event_name);
                
                // Emit to instance-specific handlers first
                if let Err(e) = self.emit_event(&instance_key, event).await {
                    warn!("Failed to emit instance event: {}", e);
                }
                
                // Then emit to general handlers for backward compatibility
                self.emit_event(&general_key, event).await
            } else {
                Err(EventError::HandlerNotFound(format!("Object instance {} not found", object_id)))
            }
        } else {
            Err(EventError::HandlerExecution("GORC instance manager not available".to_string()))
        }
    }

    /// Emits a GORC event using the legacy API (object type string).
    /// 
    /// This method is kept for backward compatibility but it's recommended to use
    /// `emit_gorc_instance` for better type safety and instance targeting.
    pub async fn emit_gorc<T>(
        &self,
        object_type: &str,
        channel: u8,
        event_name: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event,
    {
        let event_key = format!("gorc:{}:{}:{}", object_type, channel, event_name);
        self.emit_event(&event_key, event).await
    }

    /// Internal emit implementation that handles the actual event dispatch.
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
            
            // Update GORC-specific stats
            if event_key.starts_with("gorc") {
                stats.gorc_events_emitted += 1;
            }
        } else {
            warn!("⚠️ No handlers for event: {}", event_key);
        }

        Ok(())
    }

    /// Returns current system statistics.
    pub async fn get_stats(&self) -> EventSystemStats {
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// Gets detailed statistics including GORC instance information
    pub async fn get_detailed_stats(&self) -> DetailedEventSystemStats {
        let base_stats = self.get_stats().await;
        let handler_count_by_category = self.get_handler_count_by_category().await;
        
        let gorc_instance_stats = if let Some(ref gorc_instances) = self.gorc_instances {
            Some(gorc_instances.get_stats().await)
        } else {
            None
        };

        DetailedEventSystemStats {
            base: base_stats,
            handler_count_by_category,
            gorc_instance_stats,
        }
    }

    /// Gets handler count breakdown by event category
    async fn get_handler_count_by_category(&self) -> HandlerCategoryStats {
        let handlers = self.handlers.read().await;
        let mut core_handlers = 0;
        let mut client_handlers = 0;
        let mut plugin_handlers = 0;
        let mut gorc_handlers = 0;
        let mut gorc_instance_handlers = 0;

        for (key, handler_list) in handlers.iter() {
            let count = handler_list.len();
            if key.starts_with("core:") {
                core_handlers += count;
            } else if key.starts_with("client:") {
                client_handlers += count;
            } else if key.starts_with("plugin:") {
                plugin_handlers += count;
            } else if key.starts_with("gorc_instance:") {
                gorc_instance_handlers += count;
            } else if key.starts_with("gorc:") {
                gorc_handlers += count;
            }
        }

        HandlerCategoryStats {
            core_handlers,
            client_handlers,
            plugin_handlers,
            gorc_handlers,
            gorc_instance_handlers,
        }
    }

    /// Broadcasts a GORC event to all subscribers of an object instance
    /// 
    /// This method combines event emission with the GORC subscription system
    /// to ensure events are only sent to players who are subscribed to the
    /// object's replication zones.
    pub async fn broadcast_gorc_to_subscribers<T>(
        &self,
        object_id: GorcObjectId,
        channel: u8,
        event_name: &str,
        event: &T,
    ) -> Result<usize, EventError>
    where
        T: Event,
    {
        let Some(ref gorc_instances) = self.gorc_instances else {
            return Err(EventError::HandlerExecution(
                "GORC instance manager not available".to_string()
            ));
        };

        // Get the object instance and its subscribers
        if let Some(instance) = gorc_instances.get_object(object_id).await {
            let subscribers = instance.get_subscribers(channel);
            
            if !subscribers.is_empty() {
                // Emit the event to handlers
                self.emit_gorc_instance(object_id, channel, event_name, event).await?;
                
                // In a real implementation, you would also send the serialized event
                // directly to the network layer for the specific subscribers
                debug!(
                    "📡 Broadcasted GORC event {} to {} subscribers for object {}",
                    event_name, subscribers.len(), object_id
                );
                
                Ok(subscribers.len())
            } else {
                debug!("No subscribers for object {} channel {}", object_id, channel);
                Ok(0)
            }
        } else {
            Err(EventError::HandlerNotFound(format!("Object instance {} not found", object_id)))
        }
    }

    /// Removes all handlers for a specific event pattern
    pub async fn remove_handlers(&self, pattern: &str) -> usize {
        let mut handlers = self.handlers.write().await;
        let mut removed_count = 0;

        handlers.retain(|key, handler_list| {
            if key.contains(pattern) {
                removed_count += handler_list.len();
                false
            } else {
                true
            }
        });

        if removed_count > 0 {
            let mut stats = self.stats.write().await;
            stats.total_handlers = stats.total_handlers.saturating_sub(removed_count);
            info!("🗑️ Removed {} handlers matching pattern '{}'", removed_count, pattern);
        }

        removed_count
    }

    /// Gets all registered event keys
    pub async fn get_registered_events(&self) -> Vec<String> {
        let handlers = self.handlers.read().await;
        handlers.keys().cloned().collect()
    }

    /// Checks if handlers are registered for a specific event
    pub async fn has_handlers(&self, event_key: &str) -> bool {
        let handlers = self.handlers.read().await;
        handlers.contains_key(event_key)
    }

    /// Gets the number of handlers for a specific event
    pub async fn get_handler_count(&self, event_key: &str) -> usize {
        let handlers = self.handlers.read().await;
        handlers.get(event_key).map(|h| h.len()).unwrap_or(0)
    }

    /// Validates the event system configuration
    pub async fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();
        let handlers = self.handlers.read().await;

        // Check for potential issues
        for (key, handler_list) in handlers.iter() {
            if handler_list.is_empty() {
                issues.push(format!("Event key '{}' has no handlers", key));
            }
            
            if handler_list.len() > 100 {
                issues.push(format!("Event key '{}' has excessive handlers: {}", key, handler_list.len()));
            }
        }

        if let Some(ref gorc_instances) = self.gorc_instances {
            let instance_stats = gorc_instances.get_stats().await;
            if instance_stats.total_objects > 10000 {
                issues.push(format!("High number of GORC objects: {}", instance_stats.total_objects));
            }
        }

        issues
    }
}

/// Statistics about the event system's performance and usage.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct EventSystemStats {
    /// Total number of registered event handlers
    pub total_handlers: usize,
    /// Total number of events emitted since system start
    pub events_emitted: u64,
    /// Total number of GORC events emitted
    pub gorc_events_emitted: u64,
    /// Average events per second (calculated over recent history)
    pub avg_events_per_second: f64,
    /// Peak events per second recorded
    pub peak_events_per_second: f64,
}

/// Detailed statistics including category breakdowns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedEventSystemStats {
    /// Base event system statistics
    pub base: EventSystemStats,
    /// Handler count by category
    pub handler_count_by_category: HandlerCategoryStats,
    /// GORC instance manager statistics
    pub gorc_instance_stats: Option<crate::gorc::instance::InstanceManagerStats>,
}

/// Handler count breakdown by event category
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandlerCategoryStats {
    /// Number of core event handlers
    pub core_handlers: usize,
    /// Number of client event handlers
    pub client_handlers: usize,
    /// Number of plugin event handlers
    pub plugin_handlers: usize,
    /// Number of basic GORC event handlers
    pub gorc_handlers: usize,
    /// Number of GORC instance event handlers
    pub gorc_instance_handlers: usize,
}

impl Default for EventSystem {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to create an event system with GORC integration
pub fn create_event_system_with_gorc(
    gorc_instances: Arc<GorcInstanceManager>
) -> Arc<EventSystem> {
    Arc::new(EventSystem::with_gorc(gorc_instances))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{PlayerConnectedEvent, GorcEvent};
    use crate::types::PlayerId;
    use crate::gorc::instance::{GorcInstanceManager, GorcObjectId};

    #[tokio::test]
    async fn test_event_system_creation() {
        let events = EventSystem::new();
        let stats = events.get_stats().await;
        
        assert_eq!(stats.total_handlers, 0);
        assert_eq!(stats.events_emitted, 0);
    }

    #[tokio::test]
    async fn test_core_event_registration_and_emission() {
        let events = EventSystem::new();
        
        // Register handler
        events.on_core("player_connected", |event: PlayerConnectedEvent| {
            assert!(!event.player_id.to_string().is_empty());
            Ok(())
        }).await.unwrap();
        
        // Check handler was registered
        let stats = events.get_stats().await;
        assert_eq!(stats.total_handlers, 1);
        
        // Emit event
        let player_event = PlayerConnectedEvent {
            player_id: PlayerId::new(),
            connection_id: "test_conn".to_string(),
            remote_addr: "127.0.0.1:8080".to_string(),
            timestamp: crate::utils::current_timestamp(),
        };
        
        events.emit_core("player_connected", &player_event).await.unwrap();
        
        // Check event was emitted
        let stats = events.get_stats().await;
        assert_eq!(stats.events_emitted, 1);
    }

    #[tokio::test]
    async fn test_gorc_event_system() {
        let gorc_instances = Arc::new(GorcInstanceManager::new());
        let events = EventSystem::with_gorc(gorc_instances);
        
        // Register GORC handler
        events.on_gorc("Asteroid", 0, "position_update", |event: GorcEvent| {
            assert_eq!(event.object_type, "Asteroid");
            assert_eq!(event.channel, 0);
            Ok(())
        }).await.unwrap();
        
        // Emit GORC event
        let gorc_event = GorcEvent {
            object_id: GorcObjectId::new().to_string(),
            object_type: "Asteroid".to_string(),
            channel: 0,
            data: vec![1, 2, 3, 4],
            priority: "Critical".to_string(),
            timestamp: crate::utils::current_timestamp(),
        };
        
        events.emit_gorc("Asteroid", 0, "position_update", &gorc_event).await.unwrap();
        
        let stats = events.get_stats().await;
        assert_eq!(stats.gorc_events_emitted, 1);
    }

    #[tokio::test]
    async fn test_handler_category_stats() {
        let events = EventSystem::new();
        
        // Register different types of handlers
        events.on_core("test_core", |_: PlayerConnectedEvent| Ok(())).await.unwrap();
        events.on_client("test", "test_client", |_: PlayerConnectedEvent| Ok(())).await.unwrap();
        events.on_plugin("test_plugin", "test_event", |_: PlayerConnectedEvent| Ok(())).await.unwrap();
        events.on_gorc("TestObject", 0, "test_gorc", |_: GorcEvent| Ok(())).await.unwrap();
        
        let detailed_stats = events.get_detailed_stats().await;
        let category_stats = detailed_stats.handler_count_by_category;
        
        assert_eq!(category_stats.core_handlers, 1);
        assert_eq!(category_stats.client_handlers, 1);
        assert_eq!(category_stats.plugin_handlers, 1);
        assert_eq!(category_stats.gorc_handlers, 1);
        assert_eq!(category_stats.gorc_instance_handlers, 0);
    }

    #[tokio::test]
    async fn test_event_validation() {
        let events = EventSystem::new();
        
        // Register a handler then remove it to create an empty key
        events.on_core("test_event", |_: PlayerConnectedEvent| Ok(())).await.unwrap();
        
        let issues = events.validate().await;
        // Should not have issues with a properly registered handler
        assert!(issues.is_empty());
    }

    #[tokio::test]
    async fn test_handler_removal() {
        let events = EventSystem::new();
        
        events.on_core("test1", |_: PlayerConnectedEvent| Ok(())).await.unwrap();
        events.on_core("test2", |_: PlayerConnectedEvent| Ok(())).await.unwrap();
        events.on_client("namespace", "test3", |_: PlayerConnectedEvent| Ok(())).await.unwrap();
        
        let initial_stats = events.get_stats().await;
        assert_eq!(initial_stats.total_handlers, 3);
        
        // Remove core handlers
        let removed = events.remove_handlers("core:").await;
        assert_eq!(removed, 2);
        
        let final_stats = events.get_stats().await;
        assert_eq!(final_stats.total_handlers, 1);
    }
}