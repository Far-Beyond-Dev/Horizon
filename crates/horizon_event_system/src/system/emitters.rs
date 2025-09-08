/// Event emission methods
use crate::events::{Event, EventError};
use crate::gorc::instance::GorcObjectId;
use crate::{PlayerId, Vec3};
use super::core::EventSystem;
use super::stats::{DetailedEventSystemStats, HandlerCategoryStats};
use futures::{self, stream::{FuturesUnordered, StreamExt}};
use tracing::{debug, error, info, warn};
use compact_str::CompactString;

/// Helper function to extract namespace from event key for debugging
fn namespace_from_key(event_key: &str) -> &str {
    if let Some(colon_pos) = event_key.find(':') {
        if let Some(second_colon) = event_key[colon_pos + 1..].find(':') {
            &event_key[colon_pos + 1..colon_pos + 1 + second_colon]
        } else {
            &event_key[colon_pos + 1..]
        }
    } else {
        ""
    }
}

impl EventSystem {
    /// Emits a core server event to all registered handlers.
    #[inline]
    pub async fn emit_core<T>(&self, event_name: &str, event: &T) -> Result<(), EventError>
    where
        T: Event,
    {
        let event_key = CompactString::new_inline("core:") + event_name;
        self.emit_event(&event_key, event).await
    }

    /// Emits a client event to all registered handlers.
    #[inline]
    pub async fn emit_client<T>(
        &self,
        namespace: &str,
        event_name: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event,
    {
        let event_key = CompactString::new_inline("client:") + namespace + ":" + event_name;
        self.emit_event(&event_key, event).await
    }

    /// Emits a client event with connection context for connection-aware handlers.
    /// 
    /// This method wraps the event data with player context information, allowing
    /// connection-aware handlers to respond directly to the originating client.
    /// 
    /// # Arguments
    /// 
    /// * `namespace` - The client event namespace
    /// * `event_name` - The specific event name
    /// * `player_id` - The player ID of the client that triggered the event
    /// * `event` - The event data
    pub async fn emit_client_with_context<T>(
        &self,
        namespace: &str,
        event_name: &str,
        player_id: crate::types::PlayerId,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event + serde::Serialize,
    {
        // Create a wrapper that includes the player context
        let context_event = serde_json::json!({
            "player_id": player_id,
            "data": event
        });
        
        let event_key = CompactString::new_inline("client:") + namespace + ":" + event_name;
        self.emit_event(&event_key, &context_event).await
    }

    /// Emits a plugin event to all registered handlers.
    #[inline]
    pub async fn emit_plugin<T>(
        &self,
        plugin_name: &str,
        event_name: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event,
    {
        let event_key = CompactString::new_inline("plugin:") + plugin_name + ":" + event_name;
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
        dest: crate::events::Dest,
    ) -> Result<(), EventError>
    where
        T: Event + serde::Serialize,
    {
        use crate::events::Dest;
        
        // Handle None destination early
        if dest == Dest::None {
            return Ok(());
        }
        
        // Get the object instance to determine its type and position
        if let Some(ref gorc_instances) = self.gorc_instances {
            if let Some(instance) = gorc_instances.get_object(object_id).await {
                let object_type = &instance.type_name;
                
                // Handle Server or Both destinations - emit to server-side handlers
                if dest == Dest::Server || dest == Dest::Both {
                    let instance_key = CompactString::new_inline("gorc_instance:") + object_type + ":" + &channel.to_string() + ":" + event_name;
                    let general_key = CompactString::new_inline("gorc:") + object_type + ":" + &channel.to_string() + ":" + event_name;
                    
                    // Emit to instance-specific handlers first
                    if let Err(e) = self.emit_event(&instance_key, event).await {
                        warn!("Failed to emit instance event: {}", e);
                    }
                    
                    // Emit to general handlers for backward compatibility
                    if let Err(e) = self.emit_event(&general_key, event).await {
                        warn!("Failed to emit general event: {}", e);
                    }
                }
                
                // Handle Client or Both destinations - emit to subscribed clients
                if dest == Dest::Client || dest == Dest::Both {
                    self.emit_to_gorc_subscribers(object_id, channel, event_name, event).await?;
                }
                
                Ok(())
            } else {
                Err(EventError::HandlerNotFound(format!("Object instance {} not found", object_id)))
            }
        } else {
            Err(EventError::HandlerExecution("GORC instance manager not available".to_string()))
        }
    }
    
    /// Emits event data directly to clients subscribed to the object's channel
    async fn emit_to_gorc_subscribers<T>(
        &self,
        object_id: GorcObjectId,
        channel: u8,
        event_name: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event + serde::Serialize,
    {
        // Get the client response sender
        let sender = self.client_response_sender.as_ref().ok_or_else(|| {
            EventError::HandlerExecution("Client response sender not configured for GORC emission".to_string())
        })?;
        
        // Get the GORC instances manager to find subscribers
        let gorc_instances = self.gorc_instances.as_ref().ok_or_else(|| {
            EventError::HandlerExecution("GORC instance manager not available".to_string())
        })?;
        
        // Get the object instance to find its position and layer configuration
        let instance = gorc_instances.get_object(object_id).await.ok_or_else(|| {
            EventError::HandlerNotFound(format!("Object instance {} not found", object_id))
        })?;
        
        // Get the replication layer for this channel
        let layers = instance.object.get_layers();
        let layer = layers.iter().find(|l| l.channel == channel).ok_or_else(|| {
            EventError::HandlerExecution(format!("Channel {} not defined for object {}", channel, object_id))
        })?;
        
        // Find all players within this layer's radius
        let subscribers = gorc_instances.find_players_in_radius(
            instance.object.position(), 
            layer.radius
        ).await;
        
        // Create the event message for clients - just use the event_name directly
        let client_event = serde_json::json!({
            "event_type": event_name,
            "object_id": object_id.to_string(),
            "object_type": instance.type_name,
            "channel": channel,
            "player_id": object_id.to_string(),
            "data": event,
            "timestamp": crate::utils::current_timestamp()
        });
        
        // Serialize the event data
        let data = serde_json::to_vec(&client_event)
            .map_err(|e| EventError::Serialization(e))?;
        
        // Send to all subscribers
        let mut sent_count = 0;
        for player_id in subscribers {
            if let Err(e) = sender.send_to_client(player_id, data.clone()).await {
                warn!("Failed to send GORC event to player {}: {}", player_id, e);
            } else {
                sent_count += 1;
            }
        }
        
        debug!("📡 GORC: Sent {} event to {} clients on channel {} for object {}", 
               event_name, sent_count, channel, object_id);
        
        Ok(())
    }
    
    /// Update player position and handle zone membership changes (event-driven GORC)
    pub async fn update_player_position(&self, player_id: PlayerId, new_position: Vec3) -> Result<(), EventError> {
        // Get the GORC instances manager
        let gorc_instances = self.gorc_instances.as_ref().ok_or_else(|| {
            EventError::HandlerExecution("GORC instance manager not available".to_string())
        })?;
        
        // Update position and get zone changes
        let (zone_entries, zone_exits) = gorc_instances.update_player_position(player_id, new_position).await;
        
        // Handle zone entries - send zone entry messages with current layer state
        for (object_id, channel) in zone_entries {
            self.send_zone_entry_message(player_id, object_id, channel).await?;
        }
        
        // Handle zone exits - send zone exit messages to inform client
        for (object_id, channel) in zone_exits {
            self.send_zone_exit_message(player_id, object_id, channel).await?;
        }
        
        Ok(())
    }
    
    /// Send zone entry message with current object state for a specific layer to a player
    async fn send_zone_entry_message(&self, player_id: PlayerId, object_id: GorcObjectId, channel: u8) -> Result<(), EventError> {
        // Get the client response sender
        let sender = self.client_response_sender.as_ref().ok_or_else(|| {
            EventError::HandlerExecution("Client response sender not configured".to_string())
        })?;
        
        // Get the GORC instances manager
        let gorc_instances = self.gorc_instances.as_ref().ok_or_else(|| {
            EventError::HandlerExecution("GORC instance manager not available".to_string())
        })?;
        
        // Get the object instance
        let instance = gorc_instances.get_object(object_id).await.ok_or_else(|| {
            EventError::HandlerNotFound(format!("Object instance {} not found", object_id))
        })?;
        
        // Get current state for this layer
        if let Some(layer_data) = gorc_instances.get_object_state_for_layer(object_id, channel).await {
            // Create zone entry message with proper format
            let zone_entry_event = serde_json::json!({
                "type": "gorc_zone_enter",
                "object_id": object_id.to_string(),
                "object_type": instance.type_name,
                "channel": channel,
                "player_id": player_id.to_string(),
                "zone_data": serde_json::from_slice::<serde_json::Value>(&layer_data)
                    .unwrap_or(serde_json::Value::Null),
                "timestamp": crate::utils::current_timestamp()
            });
            
            // Serialize and send
            let data = serde_json::to_vec(&zone_entry_event)
                .map_err(|e| EventError::Serialization(e))?;
            
            if let Err(e) = sender.send_to_client(player_id, data).await {
                warn!("❌ Failed to send zone entry message to player {}: {}", player_id, e);
            } else {
                info!("🔔 GORC: Player {} entered zone {} of object {} ({})", 
                      player_id, channel, object_id, instance.type_name);
            }
        } else {
            warn!("❌ GORC: No layer data available for object {} channel {}", object_id, channel);
        }
        
        Ok(())
    }

    /// Send zone exit message to inform player they left an object's zone
    async fn send_zone_exit_message(&self, player_id: PlayerId, object_id: GorcObjectId, channel: u8) -> Result<(), EventError> {
        // Get the client response sender
        let sender = self.client_response_sender.as_ref().ok_or_else(|| {
            EventError::HandlerExecution("Client response sender not configured".to_string())
        })?;
        
        // Get the GORC instances manager for object type lookup
        let gorc_instances = self.gorc_instances.as_ref().ok_or_else(|| {
            EventError::HandlerExecution("GORC instance manager not available".to_string())
        })?;
        
        // Get object type for logging (optional - graceful fallback if object no longer exists)
        let object_type = if let Some(instance) = gorc_instances.get_object(object_id).await {
            instance.type_name.clone()
        } else {
            "Unknown".to_string()
        };
        
        // Create zone exit message
        let zone_exit_event = serde_json::json!({
            "type": "gorc_zone_exit",
            "object_id": object_id.to_string(),
            "object_type": object_type,
            "channel": channel,
            "player_id": player_id.to_string(),
            "timestamp": crate::utils::current_timestamp()
        });
        
        // Serialize and send
        let data = serde_json::to_vec(&zone_exit_event)
            .map_err(|e| EventError::Serialization(e))?;
        
        if let Err(e) = sender.send_to_client(player_id, data).await {
            warn!("❌ Failed to send zone exit message to player {}: {}", player_id, e);
        } else {
            info!("🚪 GORC: Player {} exited zone {} of object {} ({})", 
                  player_id, channel, object_id, object_type);
        }
        
        Ok(())
    }

    /// Emits a GORC event using the legacy API (object type string).
    /// 
    /// This method is kept for backward compatibility but it's recommended to use
    /// `emit_gorc_instance` for better type safety and instance targeting.
    #[inline]
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
        let event_key = CompactString::new_inline("gorc:") + object_type + ":" + &channel.to_string() + ":" + event_name;
        self.emit_event(&event_key, event).await
    }

    /// Routes a client message to GORC instance handlers for a specific player.
    /// 
    /// This method is designed for the message router to handle client messages
    /// that should trigger server-side GORC instance handlers. It finds the player's
    /// GORC object and emits to server handlers only.
    pub async fn route_client_message_to_gorc<T>(
        &self,
        player_id: crate::PlayerId,
        object_type: &str,
        channel: u8,
        event_name: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event + serde::Serialize,
    {
        // Find the player's GORC object ID
        if let Some(gorc_instances) = &self.gorc_instances {
            if let Some(object_id) = gorc_instances.find_player_object(player_id).await {
                // Route to server-side handlers only (client message was already processed)
                return self.emit_gorc_instance(object_id, channel, event_name, event, crate::events::Dest::Server).await;
            }
        }
        
        // If no GORC object found for this player, that's expected for some cases
        tracing::debug!("No GORC object found for player {} (type: {})", player_id, object_type);
        Ok(())
    }

    /// Broadcasts an event to all connected clients.
    /// 
    /// This method sends the event data to every client currently connected to the server.
    /// The event is serialized once and then sent to all clients for optimal performance.
    /// 
    /// # Arguments
    /// 
    /// * `event` - The event data to broadcast
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(usize)` with the number of clients that received the broadcast,
    /// or `Err(EventError)` if the broadcast failed or client response sender is not configured.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// // Broadcast a server announcement to all players
    /// let announcement = ServerAnnouncement {
    ///     message: "Server maintenance in 5 minutes".to_string(),
    ///     priority: "high".to_string(),
    /// };
    /// 
    /// match events.broadcast(&announcement).await {
    ///     Ok(client_count) => println!("Announcement sent to {} clients", client_count),
    ///     Err(e) => println!("Broadcast failed: {}", e),
    /// }
    /// ```
    pub async fn broadcast<T>(&self, event: &T) -> Result<usize, EventError>
    where
        T: Event + serde::Serialize,
    {
        // Check if client response sender is configured
        let sender = self.client_response_sender.as_ref().ok_or_else(|| {
            EventError::HandlerExecution("Client response sender not configured for broadcasting".to_string())
        })?;

        // Serialize the event data using our serialization pool
        let data = self.serialization_pool.serialize_event(event)?;
        
        // Convert Arc<Vec<u8>> to Vec<u8> for the broadcast method
        let broadcast_data = (*data).clone();
        
        // Send to all clients via the client response sender
        match sender.broadcast_to_all(broadcast_data).await {
            Ok(client_count) => {
                if cfg!(debug_assertions) {
                    debug!("📡 Broadcasted event to {} clients", client_count);
                }
                
                // Update stats
                let mut stats = self.stats.write().await;
                stats.events_emitted += 1;
                
                Ok(client_count)
            },
            Err(e) => {
                error!("❌ Broadcast failed: {}", e);
                Err(EventError::HandlerExecution(format!("Broadcast failed: {}", e)))
            }
        }
    }

    /// Internal emit implementation that handles the actual event dispatch.
    /// Optimized for high throughput (500k messages/sec target).
    /// Now uses lock-free DashMap + serialization pool for maximum performance.
    async fn emit_event<T>(&self, event_key: &str, event: &T) -> Result<(), EventError>
    where
        T: Event,
    {
        // Use serialization pool for better performance and shared data
        let data = self.serialization_pool.serialize_event(event)?;
        
        // Lock-free read from DashMap - no contention!
        let event_handlers = self.handlers.get(event_key).map(|entry| entry.value().clone());

        if let Some(event_handlers) = event_handlers {
            // Only log debug info if handlers exist to reduce overhead
            if event_handlers.len() > 0 {
                if cfg!(debug_assertions) {
                    debug!("📤 Emitting {} to {} handlers", event_key, event_handlers.len());
                }

                // Use FuturesUnordered for better memory efficiency and concurrency
                let mut futures = FuturesUnordered::new();
                
                for handler in event_handlers.iter() {
                    let data_arc = data.clone(); // Clone the Arc, not the data for speed
                    let handler_name = handler.handler_name();
                    let handler_clone = handler.clone();
                    
                    futures.push(async move {
                        if let Err(e) = handler_clone.handle(&data_arc).await {
                            error!("❌ Handler {} failed: {}", handler_name, e);
                        }
                    });
                }

                // Execute all handlers concurrently with better memory usage
                while let Some(_) = futures.next().await {};
            }

            // Batch stats updates to reduce lock contention
            let mut stats = self.stats.write().await;
            stats.events_emitted += 1;
            
            // Update GORC-specific stats with branch prediction optimization
            if event_key.as_bytes().get(0) == Some(&b'g') && event_key.starts_with("gorc") {
                stats.gorc_events_emitted += 1;
            }
        } else {
            // Show debugging info for missing handlers (except server_tick spam)
            if event_key != "core:server_tick" && event_key != "core:raw_client_message" {
                // Show available handlers for debugging using DashMap iteration
                let available_keys: Vec<String> = self.handlers
                    .iter()
                    .filter_map(|entry| {
                        let key = entry.key();
                        if key.contains(&namespace_from_key(event_key)) {
                            Some(key.to_string()) // Convert CompactString to String
                        } else {
                            None
                        }
                    })
                    .collect();
                
                if !available_keys.is_empty() {
                    warn!("⚠️ No handlers for event: {} (similar keys available: {:?})", event_key, available_keys);
                } else {
                    warn!("⚠️ No handlers for event: {} (no similar handlers found)", event_key);
                }
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

    /// Gets handler count breakdown by event category using lock-free DashMap
    async fn get_handler_count_by_category(&self) -> HandlerCategoryStats {
        let mut core_handlers = 0;
        let mut client_handlers = 0;
        let mut plugin_handlers = 0;
        let mut gorc_handlers = 0;
        let mut gorc_instance_handlers = 0;

        // Lock-free iteration over DashMap
        for entry in self.handlers.iter() {
            let key = entry.key();
            let count = entry.value().len();
            
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