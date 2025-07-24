/// Event emission methods
use crate::events::{Event, EventError};
use crate::gorc::instance::GorcObjectId;
use super::core::EventSystem;
use super::stats::{DetailedEventSystemStats, HandlerCategoryStats};
use futures;
use tracing::{debug, error, warn};

impl EventSystem {
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
    /// Optimized for high throughput (500k messages/sec target).
    async fn emit_event<T>(&self, event_key: &str, event: &T) -> Result<(), EventError>
    where
        T: Event,
    {
        // Pre-serialize once for all handlers to avoid duplicate serialization
        let data = event.serialize()?;
        
        // Use read lock with minimal hold time
        let event_handlers = {
            let handlers = self.handlers.read().await;
            handlers.get(event_key).cloned()
        };

        if let Some(event_handlers) = event_handlers {
            // Only log debug info if handlers exist to reduce overhead
            if event_handlers.len() > 0 {
                if cfg!(debug_assertions) {
                    debug!("📤 Emitting {} to {} handlers", event_key, event_handlers.len());
                }

                // Parallel handler execution for better throughput
                let futures: Vec<_> = event_handlers
                    .iter()
                    .map(|handler| {
                        let data_ref = &data;
                        let handler_name = handler.handler_name();
                        async move {
                            if let Err(e) = handler.handle(data_ref).await {
                                error!("❌ Handler {} failed: {}", handler_name, e);
                            }
                        }
                    })
                    .collect();

                // Execute all handlers concurrently for maximum throughput
                futures::future::join_all(futures).await;
            }

            // Batch stats updates to reduce lock contention
            let mut stats = self.stats.write().await;
            stats.events_emitted += 1;
            
            // Update GORC-specific stats with branch prediction optimization
            if event_key.as_bytes().get(0) == Some(&b'g') && event_key.starts_with("gorc") {
                stats.gorc_events_emitted += 1;
            }
        } else if cfg!(debug_assertions) {
            // Only warn in debug builds to reduce production overhead
            // Exempt core:server_tick from warning spam
            if event_key != "core:server_tick" {
                warn!("⚠️ No handlers for event: {}", event_key);
            }
        }

        Ok(())
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
}