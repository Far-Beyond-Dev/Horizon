/// Event system management and utility methods
use crate::events::{Event, EventError};
use crate::gorc::instance::GorcObjectId;
use super::core::EventSystem;
use tracing::{debug, info, warn};
use base64::{Engine as _, engine::general_purpose};

impl EventSystem {
    /// Broadcasts a GORC instance event to all subscribers of that instance.
    /// 
    /// This is a higher-level API that not only emits the event to handlers
    /// but also handles the network broadcasting to all subscribers of the
    /// specific object instance on the given channel.
    /// 
    /// # Arguments
    /// 
    /// * `object_id` - The specific object instance to broadcast for
    /// * `channel` - Replication channel for the broadcast
    /// * `event_name` - Name of the event being broadcasted
    /// * `event` - The event data to broadcast
    /// 
    /// # Returns
    /// 
    /// Returns the number of subscribers that received the broadcast
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// // Broadcast a critical health update to all subscribers
    /// let subscriber_count = events.broadcast_gorc_instance(
    ///     player_id, 
    ///     0, // Critical channel
    ///     "health_critical", 
    ///     &HealthCriticalEvent {
    ///         current_health: 15.0,
    ///         max_health: 100.0,
    ///         is_bleeding: true,
    ///     }
    /// ).await?;
    /// 
    /// println!("Broadcasted health update to {} subscribers", subscriber_count);
    /// ```
    pub async fn broadcast_gorc_instance<T>(
        &self,
        object_id: GorcObjectId,
        channel: u8,
        event_name: &str,
        event: &T,
    ) -> Result<usize, EventError>
    where
        T: Event + serde::Serialize,
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
                
                // Send the serialized event directly to the network layer for subscribers
                if let Some(ref client_sender) = self.client_response_sender {
                    // Serialize the event for network transmission
                    if let Ok(serialized_event) = serde_json::to_vec(event) {
                        // Create a network message for GORC event
                        let gorc_message = serde_json::json!({
                            "type": "gorc_event",
                            "object_id": object_id.0,
                            "channel": channel,
                            "event_name": event_name,
                            "data": general_purpose::STANDARD.encode(&serialized_event),
                            "timestamp": crate::utils::current_timestamp()
                        });
                        
                        if let Ok(message_bytes) = serde_json::to_vec(&gorc_message) {
                            // Send to each subscriber individually
                            for subscriber_id in &subscribers {
                                if let Err(e) = client_sender.send_to_client(*subscriber_id, message_bytes.clone()).await {
                                    warn!("Failed to send GORC event to subscriber {}: {}", subscriber_id, e);
                                }
                            }
                            
                            debug!(
                                "📡 Sent serialized GORC event {} to {} subscribers for object {}",
                                event_name, subscribers.len(), object_id
                            );
                        } else {
                            warn!("Failed to serialize GORC message for transmission");
                        }
                    } else {
                        warn!("Failed to serialize GORC event data for transmission");
                    }
                } else {
                    debug!(
                        "📡 Broadcasted GORC event {} to {} subscribers for object {} (handlers only - no network sender)",
                        event_name, subscribers.len(), object_id
                    );
                }
                
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