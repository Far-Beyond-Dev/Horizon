/// Event handler registration methods
use crate::events::{Event, EventHandler, TypedEventHandler, EventError, GorcEvent};
use crate::gorc::instance::{GorcObjectId, ObjectInstance};
use super::core::EventSystem;
use super::client::ClientConnectionRef;
use std::sync::Arc;
use tracing::{error, info};
use compact_str::CompactString;

impl EventSystem {
    /// Registers a handler for core server events.
    pub async fn on_core<T, F>(&self, event_name: &str, handler: F) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let event_key = CompactString::new_inline("core:") + event_name;
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
        let event_key = CompactString::new_inline("client:") + namespace + ":" + event_name;
        self.register_typed_handler(event_key, event_name, handler)
            .await
    }

    /// Registers a connection-aware handler for client events with namespace.
    /// 
    /// This variant provides the handler with a `ClientConnectionRef` that allows
    /// direct response to the specific client that triggered the event. This enables
    /// easy responses without needing to use global broadcast methods.
    /// 
    /// # Arguments
    /// 
    /// * `namespace` - The client event namespace (e.g., "chat", "movement")
    /// * `event_name` - The specific event name within the namespace
    /// * `handler` - Function that receives both the event and client connection reference
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// // Handler with direct client response capability
    /// events.on_client_with_connection("chat", "send_message", 
    ///     |event: ChatMessageEvent, client: &ClientConnectionRef| async move {
    ///         // Process the chat message
    ///         let response = ChatResponse {
    ///             message_id: event.id,
    ///             status: "received".to_string(),
    ///         };
    ///         
    ///         // Respond directly to this client
    ///         client.respond_json(&response).await?;
    ///         Ok(())
    ///     }
    /// ).await?;
    /// ```
    pub async fn on_client_with_connection<T, F>(
        &self,
        namespace: &str,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + serde::Serialize + 'static,
        F: Fn(T, ClientConnectionRef) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let event_key = CompactString::new_inline("client:") + namespace + ":" + event_name;
        self.register_connection_aware_handler(event_key, event_name, handler)
            .await
    }

    /// Registers an async handler for client events with namespace.
    /// 
    /// This is similar to `on_client` but the handler function is async,
    /// allowing for async operations inside the handler without connection awareness.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// // Async handler without connection awareness
    /// events.on_client_async("inventory", "use_item", 
    ///     |event: UseItemEvent| {
    ///         // Sync handler that can use block_on for async work
    ///         if let Ok(handle) = tokio::runtime::Handle::try_current() {
    ///             handle.block_on(async {
    ///                 // Async database operations, etc.
    ///                 tokio::time::sleep(Duration::from_millis(10)).await;
    ///             });
    ///         }
    ///         Ok(())
    ///     }
    /// ).await?;
    /// ```
    pub async fn on_client_async<T, F>(
        &self,
        namespace: &str,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let event_key = CompactString::new_inline("client:") + namespace + ":" + event_name;
        self.register_async_handler(event_key, event_name, handler)
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
        let event_key = CompactString::new_inline("plugin:") + plugin_name + ":" + event_name;
        self.register_typed_handler(event_key, event_name, handler)
            .await
    }

    /// Registers a handler for GORC object events on a specific channel.
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
        let event_key = CompactString::new_inline("gorc:") + object_type + ":" + &channel.to_string() + ":" + event_name;
        self.register_typed_handler(event_key, event_name, handler)
            .await
    }

    /// On Core Async handler registration.
    ///
    /// This registers a handler for core events that will be executed in async context.
    /// The handler function should be synchronous but will be wrapped in async execution.
    /// If you need to do async work inside the handler, use tokio::runtime::Handle::current().block_on().
    pub async fn on_core_async<T, F>(
        &self,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let event_key = CompactString::new_inline("core:") + event_name;
        self.register_async_handler(event_key, event_name, handler)
            .await
    }

    /// Registers a handler for GORC instance events with direct object access.
    /// 
    /// This handler type provides access to the specific object instance that
    /// triggered the event, allowing for direct state modification and inspection.
    /// This is particularly useful for per-object logic and state management.
    /// 
    /// # Arguments
    /// 
    /// * `object_type` - The type name of the object (e.g., "Player", "Asteroid")
    /// * `channel` - The replication channel (0-3)
    /// * `event_name` - The specific event name within the channel
    /// * `handler` - Function that receives the event and mutable object instance
    /// 
    /// # Examples
    /// 
    /// ```rust,no_run
    /// use horizon_event_system::{EventSystem, GorcEvent, gorc::ObjectInstance, EventError};
    /// use std::sync::Arc;
    /// 
    /// #[derive(Debug, Clone)]
    /// struct Player {
    ///     health: f32,
    ///     dead: bool,
    /// }
    /// 
    /// impl Player {
    ///     fn set_dead(&mut self, dead: bool) { self.dead = dead; }
    /// }
    /// 
    /// async fn example() -> Result<(), EventError> {
    ///     let events = Arc::new(EventSystem::new());
    ///     
    ///     // Handler with direct object instance access
    ///     events.on_gorc_instance("Player", 0, "health_changed", 
    ///         |event: GorcEvent, instance: &mut ObjectInstance| {
    ///             println!("Received health changed event for {}", event.object_id);
    ///             Ok(())
    ///         }
    ///     ).await?;
    ///     
    ///     Ok(())
    /// }
    /// ```
    pub async fn on_gorc_instance<F>(
        &self,
        object_type: &str,
        channel: u8,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        F: Fn(GorcEvent, &mut ObjectInstance) -> Result<(), EventError>
            + Send
            + Sync
            + Clone
            + 'static,
    {
        let event_key = CompactString::new_inline("gorc_instance:") + object_type + ":" + &channel.to_string() + ":" + event_name;
        self.register_gorc_instance_handler(event_key, event_name, handler)
            .await
    }

    /// Internal helper for registering typed handlers.
    async fn register_typed_handler<T, F>(
        &self,
        event_key: CompactString,
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

        // Lock-free insertion using DashMap with SmallVec optimization
        self.handlers
            .entry(event_key.clone())
            .or_insert_with(smallvec::SmallVec::new)
            .push(handler_arc);

        // Update stats atomically
        let mut stats = self.stats.write().await;
        stats.total_handlers += 1;

        info!("📝 Registered handler for {}", event_key);
        Ok(())
    }

    /// Internal helper for registering async handlers.
    /// 
    /// Takes a sync handler from plugin and wraps it in async context on our side.
    /// This keeps DLL boundaries safe while still providing async execution.
    async fn register_async_handler<T, F>(
        &self,
        event_key: CompactString,
        _event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let handler_name = format!("{}::{}", event_key, T::type_name());
        
        // Wrap the sync handler in async context - this happens on our side of the DLL
        let async_wrapper = move |event: T| -> Result<(), EventError> {
            // Execute the sync handler
            let result = handler(event);
            
            // Log any errors but don't fail the event system
            if let Err(ref e) = result {
                error!("❌ Async handler failed: {}", e);
            }
            
            result
        };
        
        let typed_handler = TypedEventHandler::new(handler_name, async_wrapper);
        let handler_arc: Arc<dyn EventHandler> = Arc::new(typed_handler);

        // Lock-free insertion using DashMap with SmallVec optimization
        self.handlers
            .entry(event_key.clone())
            .or_insert_with(smallvec::SmallVec::new)
            .push(handler_arc);

        // Update stats atomically
        let mut stats = self.stats.write().await;
        stats.total_handlers += 1;

        info!("📝 Registered async handler for {}", event_key);
        Ok(())
    }

    /// Internal helper for registering connection-aware handlers.
    async fn register_connection_aware_handler<T, F>(
        &self,
        event_key: CompactString,
        _event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + serde::Serialize + 'static,
        F: Fn(T, ClientConnectionRef) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let handler_name = format!("{}::{}", event_key, T::type_name());
        let client_response_sender = self.client_response_sender.clone();
        
        // Create a wrapper that extracts connection info and calls the connection-aware handler
        let conn_aware_wrapper = move |event: T| -> Result<(), EventError> {
            let sender = client_response_sender.as_ref().ok_or_else(|| {
                EventError::HandlerExecution("Client response sender not configured".to_string())
            })?;
            
            // Extract player ID from the event data by attempting to serialize/deserialize
            // This works for events that have a player_id field (wrapped by emit_client_with_context)
            let player_id = match serde_json::to_value(&event) {
                Ok(json_value) => {
                    if let Some(player_id_value) = json_value.get("player_id") {
                        if let Ok(player_id) = serde_json::from_value::<crate::types::PlayerId>(player_id_value.clone()) {
                            tracing::debug!("🔧 ConnectionAwareHandler: Extracted player ID: {}", player_id);
                            player_id
                        } else {
                            tracing::warn!("🔧 ConnectionAwareHandler: Failed to deserialize player_id, using new ID");
                            // Fallback to new ID if deserialization fails
                            crate::types::PlayerId::new()
                        }
                    } else {
                        tracing::warn!("🔧 ConnectionAwareHandler: No player_id field found, using new ID");
                        // Event doesn't have player_id field, use new ID
                        crate::types::PlayerId::new()
                    }
                }
                Err(_) => {
                    tracing::warn!("🔧 ConnectionAwareHandler: Event is not serializable, using new ID");
                    // Event is not serializable, use new ID
                    crate::types::PlayerId::new()
                }
            };
            
            // Create client connection ref with extracted player ID
            // For now, use default values for other fields - these could be made async in the future
            const UNSPECIFIED_ADDR: &str = "0.0.0.0:0"; // Placeholder for unspecified address
            let default_addr = UNSPECIFIED_ADDR.parse()
                .unwrap_or_else(|_| std::net::SocketAddr::from(([0, 0, 0, 0], 0)));
            
            let client_ref = ClientConnectionRef::new(
                player_id,
                default_addr, // Default unknown address
                format!("conn_{}", player_id.0),    // Connection ID based on player ID
                crate::utils::current_timestamp(),
                crate::types::AuthenticationStatus::default(),
                sender.clone(),
            );
            
            // Call the sync handler directly - no async spawning needed
            handler(event, client_ref)
        };
        
        let typed_handler = TypedEventHandler::new(handler_name, conn_aware_wrapper);
        let handler_arc: Arc<dyn EventHandler> = Arc::new(typed_handler);

        // Lock-free insertion using DashMap with SmallVec optimization
        self.handlers
            .entry(event_key.clone())
            .or_insert_with(smallvec::SmallVec::new)
            .push(handler_arc);

        // Update stats atomically
        let mut stats = self.stats.write().await;
        stats.total_handlers += 1;

        info!("📝 Registered connection-aware handler for {}", event_key);
        Ok(())
    }

    /// Internal helper for registering GORC instance handlers.
    async fn register_gorc_instance_handler<F>(
        &self,
        event_key: CompactString,
        _event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        F: Fn(GorcEvent, &mut ObjectInstance) -> Result<(), EventError>
            + Send
            + Sync
            + Clone
            + 'static,
    {
        let gorc_instances = self.gorc_instances.as_ref().ok_or_else(|| {
            EventError::HandlerExecution("GORC instance manager not available".to_string())
        })?;

        let instances_ref = gorc_instances.clone();
        let handler_name = format!("{}::GorcInstance", event_key);

        let gorc_handler = TypedEventHandler::new(handler_name, move |event: GorcEvent| {
            let instances = instances_ref.clone();
            let handler_fn = handler.clone();

            // Execute the handler with the instance
            // For now, we'll parse the object_id and get the instance
            // In the future, we should implement with_instance_mut method
            let object_id = match GorcObjectId::from_str(&event.object_id) {
                Ok(id) => id,
                Err(_) => {
                    error!("❌ Invalid object ID format: {}", event.object_id);
                    return Err(EventError::HandlerExecution("Invalid object ID".to_string()));
                }
            };

            // TODO: This blocking call is not ideal - we should implement this in a non-blocking way
            let result = tokio::task::block_in_place(move || {
                let runtime = tokio::runtime::Handle::current();
                runtime.block_on(async move {
                    if let Some(mut instance) = instances.get_object(object_id).await {
                        handler_fn(event, &mut instance)
                    } else {
                        Err(EventError::HandlerExecution("Object instance not found".to_string()))
                    }
                })
            });

            result
        });

        let handler_arc: Arc<dyn EventHandler> = Arc::new(gorc_handler);

        // Lock-free insertion using DashMap with SmallVec optimization
        self.handlers
            .entry(event_key.clone())
            .or_insert_with(smallvec::SmallVec::new)
            .push(handler_arc);

        // Update stats atomically
        let mut stats = self.stats.write().await;
        stats.total_handlers += 1;

        info!("📝 Registered GORC instance handler for {}", event_key);
        Ok(())
    }
}