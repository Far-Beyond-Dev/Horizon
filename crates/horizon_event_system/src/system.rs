//! # Event System Core Implementation
//!
//! This module contains the core [`EventSystem`] implementation that manages
//! event routing, handler execution, and system statistics. It provides the
//! central hub for all event processing in the Horizon Event System with
//! full GORC integration for object instance events.

use crate::events::{Event, EventHandler, TypedEventHandler, EventError, GorcEvent};
use crate::gorc::instance::{GorcObjectId, GorcInstanceManager, ObjectInstance};
use crate::types::PlayerId;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// Connection-aware client reference that provides handlers with access to the client connection
/// and methods to respond directly to that specific client.
#[derive(Clone)]
pub struct ClientConnectionRef {
    /// The player ID associated with this connection
    pub player_id: PlayerId,
    /// The remote address of the client
    pub remote_addr: SocketAddr,
    /// Connection ID for internal tracking
    pub connection_id: String,
    /// Timestamp when the connection was established
    pub connected_at: u64,
    /// Sender for direct response to this specific client
    response_sender: Arc<dyn ClientResponseSender + Send + Sync>,
}

impl std::fmt::Debug for ClientConnectionRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientConnectionRef")
            .field("player_id", &self.player_id)
            .field("remote_addr", &self.remote_addr)
            .field("connection_id", &self.connection_id)
            .field("connected_at", &self.connected_at)
            .field("response_sender", &"[response_sender]")
            .finish()
    }
}

impl ClientConnectionRef {
    /// Creates a new client connection reference
    pub fn new(
        player_id: PlayerId,
        remote_addr: SocketAddr,
        connection_id: String,
        connected_at: u64,
        response_sender: Arc<dyn ClientResponseSender + Send + Sync>,
    ) -> Self {
        Self {
            player_id,
            remote_addr,
            connection_id,
            connected_at,
            response_sender,
        }
    }

    /// Send a direct response to this specific client
    pub async fn respond(&self, data: &[u8]) -> Result<(), EventError> {
        self.response_sender
            .send_to_client(self.player_id, data.to_vec())
            .await
            .map_err(|e| EventError::HandlerExecution(format!("Failed to send response: {}", e)))
    }

    /// Send a JSON response to this specific client
    pub async fn respond_json<T: serde::Serialize>(&self, data: &T) -> Result<(), EventError> {
        let json = serde_json::to_vec(data)
            .map_err(|e| EventError::HandlerExecution(format!("JSON serialization failed: {}", e)))?;
        self.respond(&json).await
    }

    /// Check if this connection is still active
    pub async fn is_active(&self) -> bool {
        self.response_sender.is_connection_active(self.player_id).await
    }
}

/// Trait for sending responses to clients - implemented by the server/connection manager
pub trait ClientResponseSender: std::fmt::Debug {
    /// Send data to a specific client
    fn send_to_client(&self, player_id: PlayerId, data: Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>>;
    
    /// Check if a client connection is still active
    fn is_connection_active(&self, player_id: PlayerId) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + '_>>;
}

/// The core event system that manages event routing and handler execution.
/// 
/// This is the central hub for all event processing in the system. It provides
/// type-safe event registration and emission with support for different event
/// categories (core, client, plugin, and GORC instance events).
pub struct EventSystem {
    /// Map of event keys to their registered handlers
    handlers: RwLock<HashMap<String, Vec<Arc<dyn EventHandler>>>>,
    /// System statistics for monitoring
    stats: RwLock<EventSystemStats>,
    /// GORC instance manager for object-specific events
    gorc_instances: Option<Arc<GorcInstanceManager>>,
    /// Client response sender for connection-aware handlers
    client_response_sender: Option<Arc<dyn ClientResponseSender + Send + Sync>>,
}

impl std::fmt::Debug for EventSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventSystem")
            .field("handlers", &"[handlers]")
            .field("stats", &"[stats]")
            .field("gorc_instances", &self.gorc_instances.is_some())
            .field("client_response_sender", &self.client_response_sender.is_some())
            .finish()
    }
}

impl EventSystem {
    /// Creates a new event system with no registered handlers.
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
            stats: RwLock::new(EventSystemStats::default()),
            gorc_instances: None,
            client_response_sender: None,
        }
    }

    /// Creates a new event system with GORC instance manager integration
    pub fn with_gorc(gorc_instances: Arc<GorcInstanceManager>) -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
            stats: RwLock::new(EventSystemStats::default()),
            gorc_instances: Some(gorc_instances),
            client_response_sender: None,
        }
    }

    /// Sets the GORC instance manager for this event system
    pub fn set_gorc_instances(&mut self, gorc_instances: Arc<GorcInstanceManager>) {
        self.gorc_instances = Some(gorc_instances);
    }

    /// Sets the client response sender for connection-aware handlers
    pub fn set_client_response_sender(&mut self, sender: Arc<dyn ClientResponseSender + Send + Sync>) {
        self.client_response_sender = Some(sender);
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
    pub async fn on_client_with_connection<T, F, Fut>(
        &self,
        namespace: &str,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T, ClientConnectionRef) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = Result<(), EventError>> + Send + 'static,
    {
        let event_key = format!("client_conn_aware:{namespace}:{event_name}");
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
    ///     |event: UseItemEvent| async move {
    ///         // Async database operations, etc.
    ///         tokio::time::sleep(Duration::from_millis(10)).await;
    ///         Ok(())
    ///     }
    /// ).await?;
    /// ```
    pub async fn on_client_async<T, F, Fut>(
        &self,
        namespace: &str,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = Result<(), EventError>> + Send + 'static,
    {
        let event_key = format!("client_async:{namespace}:{event_name}");
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
        let event_key = format!("plugin:{plugin_name}:{event_name}");
        self.register_typed_handler(event_key, event_name, handler)
            .await
    }

    /// Registers an async handler for plugin-to-plugin events.
    pub async fn on_plugin_async<T, F, Fut>(
        &self,
        plugin_name: &str,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = Result<(), EventError>> + Send + 'static,
    {
        let event_key = format!("plugin_async:{plugin_name}:{event_name}");
        self.register_async_handler(event_key, event_name, handler)
            .await
    }

    /// Registers an async handler for core server events.
    pub async fn on_core_async<T, F, Fut>(&self, event_name: &str, handler: F) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = Result<(), EventError>> + Send + 'static,
    {
        let event_key = format!("core_async:{event_name}");
        self.register_async_handler(event_key, event_name, handler)
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

    /// Internal helper for registering async handlers.
    async fn register_async_handler<T, F, Fut>(
        &self,
        event_key: String,
        _event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = Result<(), EventError>> + Send + 'static,
    {
        let handler_name = format!("{}::{}", event_key, T::type_name());
        
        // Wrap the async handler to be callable from sync context
        // High-performance async spawning for better throughput
        let async_wrapper = move |event: T| -> Result<(), EventError> {
            let future = handler(event);
            
            // Spawn the async handler without blocking
            let runtime = tokio::runtime::Handle::current();
            runtime.spawn(async move {
                if let Err(e) = future.await {
                    error!("❌ Async handler failed: {}", e);
                }
            });
            
            Ok(())
        };
        
        let typed_handler = TypedEventHandler::new(handler_name, async_wrapper);
        let handler_arc: Arc<dyn EventHandler> = Arc::new(typed_handler);

        let mut handlers = self.handlers.write().await;
        handlers
            .entry(event_key.clone())
            .or_insert_with(Vec::new)
            .push(handler_arc);

        let mut stats = self.stats.write().await;
        stats.total_handlers += 1;

        info!("📝 Registered async handler for {}", event_key);
        Ok(())
    }

    /// Internal helper for registering connection-aware handlers.
    async fn register_connection_aware_handler<T, F, Fut>(
        &self,
        event_key: String,
        _event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T, ClientConnectionRef) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = Result<(), EventError>> + Send + 'static,
    {
        let handler_name = format!("{}::{}", event_key, T::type_name());
        let client_response_sender = self.client_response_sender.clone();
        
        // Create a wrapper that extracts connection info and calls the connection-aware handler
        let conn_aware_wrapper = move |event: T| -> Result<(), EventError> {
            let sender = client_response_sender.as_ref().ok_or_else(|| {
                EventError::HandlerExecution("Client response sender not configured".to_string())
            })?;
            
            // TODO: Extract actual connection info from event context
            // For now, create a placeholder - this will need to be extracted from event metadata
            let client_ref = ClientConnectionRef::new(
                crate::types::PlayerId::new(), // Will be extracted from event context
                "127.0.0.1:8080".parse().unwrap(),
                "placeholder".to_string(),
                crate::utils::current_timestamp(),
                sender.clone(),
            );
            
            let future = handler(event, client_ref);
            let runtime = tokio::runtime::Handle::current();
            
            // Spawn the async connection-aware handler
            runtime.spawn(async move {
                if let Err(e) = future.await {
                    error!("❌ Connection-aware handler failed: {}", e);
                }
            });
            
            Ok(())
        };
        
        let typed_handler = TypedEventHandler::new(handler_name, conn_aware_wrapper);
        let handler_arc: Arc<dyn EventHandler> = Arc::new(typed_handler);

        let mut handlers = self.handlers.write().await;
        handlers
            .entry(event_key.clone())
            .or_insert_with(Vec::new)
            .push(handler_arc);

        let mut stats = self.stats.write().await;
        stats.total_handlers += 1;

        info!("📝 Registered connection-aware handler for {}", event_key);
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