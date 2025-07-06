//! # Event System Core Implementation
//!
//! This module contains the core [`EventSystem`] implementation that manages
//! event routing, handler execution, and system statistics. It provides the
//! central hub for all event processing in the Horizon Event System.
//!
//! ## Key Components
//!
//! - [`EventSystem`] - The main event system that handles registration and emission
//! - [`EventSystemStats`] - Performance and usage statistics tracking
//!
//! ## Event Categories
//!
//! The system organizes events into three main categories:
//! - **Core Events** (`core:*`) - Server infrastructure events
//! - **Client Events** (`client:namespace:event`) - Messages from game clients
//! - **Plugin Events** (`plugin:plugin_name:event`) - Inter-plugin communication
//!
//! ## Performance Characteristics
//!
//! - Handler lookup: O(1) using HashMap
//! - Sequential handler execution for the same event
//! - Failure isolation: one handler failure doesn't affect others
//! - Built-in statistics for monitoring and debugging

use crate::{events::{Event, EventError, EventHandler, TypedEventHandler}, CompleteGorcSystem, GorcManager};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use serde::{Deserialize, Serialize};

// ============================================================================
// Refined Event System with Cleaner API
// ============================================================================

/// The core event system that manages event routing and handler execution.
/// 
/// This is the central hub for all event processing in the system. It provides
/// type-safe event registration and emission with support for different event
/// categories (core, client, plugin).
/// 
/// # Thread Safety
/// 
/// The EventSystem is fully thread-safe and can be shared across multiple
/// threads using `Arc<EventSystem>`. All operations are protected by async
/// read-write locks to ensure consistency.
/// 
/// # Performance
/// 
/// - Handler lookup is O(1) using HashMap
/// - Multiple handlers for the same event are executed sequentially
/// - Failed handlers don't prevent other handlers from running
/// - Built-in statistics tracking for monitoring
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::*;
/// 
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let events = create_horizon_event_system();
/// 
///     // Register handlers
///     events.on_core("player_connected", |event: PlayerConnectedEvent| {
///         println!("Player connected!");
///         Ok(())
///     }).await?;
/// 
///     // Emit events
///     events.emit_core("player_connected", &PlayerConnectedEvent {
///         player_id: PlayerId::new(),
///         connection_id: "conn_123".to_string(),
///         remote_addr: "127.0.0.1:8080".to_string(),
///         timestamp: current_timestamp(),
///     }).await?;
///     
///     Ok(())
/// }
/// ```
#[derive(Debug)]
pub struct EventSystem {
    /// Map of event keys to their registered handlers
    handlers: RwLock<HashMap<String, Vec<Arc<dyn EventHandler>>>>,
    /// System statistics for monitoring
    stats: RwLock<EventSystemStats>,
    /// GORC integration for object replication events
    gorc: Option<RwLock<CompleteGorcSystem>>,
}

impl EventSystem {
    /// Creates a new event system with no registered handlers.
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
            stats: RwLock::new(EventSystemStats::default()),
            gorc: None,
        }
    }

    /// With GORC integration, allowing the event system to handle
    pub fn with_gorc(gorc: CompleteGorcSystem) -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
            stats: RwLock::new(EventSystemStats::default()),
            gorc: Some(RwLock::new(gorc)),
        }
    }

    /// Registers a handler for core server events.
    /// 
    /// Core events are fundamental server infrastructure events like player
    /// connections, plugin lifecycle, and region management.
    /// 
    /// # Arguments
    /// 
    /// * `event_name` - Name of the core event (e.g., "server_started")
    /// * `handler` - Function to handle events of type T
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if registration succeeds, or `Err(EventError)` if it fails.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use horizon_event_system::*;
    /// 
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// #     let events = create_horizon_event_system();
    /// events.on_core("player_connected", |event: PlayerConnectedEvent| {
    ///     println!("Player {} connected from {}", event.player_id, event.remote_addr);
    ///     Ok(())
    /// }).await?;
    /// #     Ok(())
    /// # }
    /// ```
    pub async fn on_core<T, F>(&self, event_name: &str, handler: F) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static,
    {
        let event_key = format!("core:{event_name}");
        self.register_typed_handler(event_key, event_name, handler)
            .await
    }

    /// Registers a handler for client events with namespace.
    /// 
    /// Client events are organized by namespace to provide logical grouping
    /// (e.g., "movement", "chat", "ui", "inventory").
    /// 
    /// # Arguments
    /// 
    /// * `namespace` - Logical grouping for the event (e.g., "movement")
    /// * `event_name` - Specific event name (e.g., "player_moved")
    /// * `handler` - Function to handle events of type T
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if registration succeeds, or `Err(EventError)` if it fails.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use horizon_event_system::*;
    /// 
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// #     let events = create_horizon_event_system();
    /// events.on_client("movement", "player_moved", |event: RawClientMessageEvent| {
    ///     // Parse and validate movement data
    ///     Ok(())
    /// }).await?;
    /// 
    /// events.on_client("chat", "message", |event: RawClientMessageEvent| {
    ///     // Process chat message
    ///     Ok(())
    /// }).await?;
    /// #     Ok(())
    /// # }
    /// ```
    pub async fn on_client<T, F>(
        &self,
        namespace: &str,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static,
    {
        let event_key = format!("client:{namespace}:{event_name}");
        self.register_typed_handler(event_key, event_name, handler)
            .await
    }

    /// Registers a handler for plugin-to-plugin events.
    /// 
    /// Plugin events enable communication between different plugins without
    /// tight coupling. Each plugin can emit events that other plugins can
    /// listen for.
    /// 
    /// # Arguments
    /// 
    /// * `plugin_name` - Name of the plugin emitting the event
    /// * `event_name` - Name of the event being emitted
    /// * `handler` - Function to handle events of type T
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if registration succeeds, or `Err(EventError)` if it fails.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use horizon_event_system::*;
    /// use serde::{Serialize, Deserialize};
    /// 
    /// #[derive(Serialize, Deserialize, Clone, Debug)]
    /// struct ItemEquippedEvent { item_id: String }
    /// impl Event for ItemEquippedEvent {}
    /// 
    /// #[derive(Serialize, Deserialize, Clone, Debug)]
    /// struct EnemyDefeatedEvent { enemy_id: String }
    /// impl Event for EnemyDefeatedEvent {}
    /// 
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// #     let events = create_horizon_event_system();
    /// // Combat plugin listening for inventory events
    /// events.on_plugin("inventory", "item_equipped", |event: ItemEquippedEvent| {
    ///     // Update combat stats when items are equipped
    ///     Ok(())
    /// }).await?;
    /// 
    /// // Quest plugin listening for combat events
    /// events.on_plugin("combat", "enemy_defeated", |event: EnemyDefeatedEvent| {
    ///     // Progress quest objectives
    ///     Ok(())
    /// }).await?;
    /// #     Ok(())
    /// # }
    /// ```
    pub async fn on_plugin<T, F>(
        &self,
        plugin_name: &str,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static,
    {
        let event_key = format!("plugin:{plugin_name}:{event_name}");
        self.register_typed_handler(event_key, event_name, handler)
            .await
    }

    /// Internal helper for registering typed handlers.
    /// 
    /// This method handles the common logic for all handler registration types.
    /// It creates a TypedEventHandler wrapper and stores it in the handlers map.
    /// 
    /// # Arguments
    /// 
    /// * `event_key` - Full event key (e.g., "core:server_started")
    /// * `_event_name` - Event name for debugging (currently unused)
    /// * `handler` - Function to handle events of type T
    async fn register_typed_handler<T, F>(
        &self,
        event_key: String,
        _event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static,
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
    /// 
    /// # Arguments
    /// 
    /// * `event_name` - Name of the core event
    /// * `event` - The event instance to emit
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if emission succeeds, or `Err(EventError)` if serialization fails.
    /// Individual handler failures are logged but don't cause the emission to fail.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use horizon_event_system::*;
    /// 
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// #     let events = create_horizon_event_system();
    /// #     let player_id = PlayerId::new();
    /// #     let conn_id = "conn_123".to_string();
    /// #     let addr = "127.0.0.1:8080".to_string();
    /// events.emit_core("player_connected", &PlayerConnectedEvent {
    ///     player_id: player_id,
    ///     connection_id: conn_id,
    ///     remote_addr: addr,
    ///     timestamp: current_timestamp(),
    /// }).await?;
    /// #     Ok(())
    /// # }
    /// ```
    pub async fn emit_core<T>(&self, event_name: &str, event: &T) -> Result<(), EventError>
    where
        T: Event,
    {
        let event_key = format!("core:{event_name}");
        self.emit_event(&event_key, event).await
    }

    /// Emits a client event to all registered handlers.
    /// 
    /// # Arguments
    /// 
    /// * `namespace` - Namespace for the event
    /// * `event_name` - Name of the event
    /// * `event` - The event instance to emit
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if emission succeeds, or `Err(EventError)` if serialization fails.
    /// Individual handler failures are logged but don't cause the emission to fail.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use horizon_event_system::*;
    /// 
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// #     let events = create_horizon_event_system();
    /// #     let player_id = PlayerId::new();
    /// #     let movement_data = serde_json::json!({"x": 100, "y": 200});
    /// events.emit_client("movement", "player_moved", &RawClientMessageEvent {
    ///     player_id: player_id,
    ///     message_type: "move".to_string(),
    ///     data: movement_data,
    ///     timestamp: current_timestamp(),
    /// }).await?;
    /// #     Ok(())
    /// # }
    /// ```
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
    /// 
    /// # Arguments
    /// 
    /// * `plugin_name` - Name of the plugin emitting the event
    /// * `event_name` - Name of the event
    /// * `event` - The event instance to emit
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if emission succeeds, or `Err(EventError)` if serialization fails.
    /// Individual handler failures are logged but don't cause the emission to fail.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use horizon_event_system::*;
    /// use serde::{Serialize, Deserialize};
    /// 
    /// #[derive(Serialize, Deserialize, Clone, Debug)]
    /// struct DamageDealtEvent { attacker: String, target: String, damage: u32, timestamp: u64 }
    /// impl Event for DamageDealtEvent {}
    /// 
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// #     let events = create_horizon_event_system();
    /// #     let attacker_id = "player1".to_string();
    /// #     let target_id = "enemy1".to_string();
    /// #     let damage_amount = 50;
    /// events.emit_plugin("combat", "damage_dealt", &DamageDealtEvent {
    ///     attacker: attacker_id,
    ///     target: target_id,
    ///     damage: damage_amount,
    ///     timestamp: current_timestamp(),
    /// }).await?;
    /// #     Ok(())
    /// # }
    /// ```
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

    /// Internal emit implementation that handles the actual event dispatch.
    /// 
    /// This method serializes the event once and then dispatches it to all
    /// registered handlers. Handler failures are logged but don't prevent
    /// other handlers from running.
    /// 
    /// # Arguments
    /// 
    /// * `event_key` - Full event key for handler lookup
    /// * `event` - The event instance to emit
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

            // We will not use par_iter here because we want to ensure that all events are
            // executed as quickly as possible and cannot tolerate any delays from setting up
            // parallel execution.
            for handler in event_handlers {
                if let Err(e) = handler.handle(&data).await {
                    error!("❌ Handler {} failed: {}", handler.handler_name(), e);
                }
            }

            let mut stats = self.stats.write().await;
            stats.events_emitted += 1;
        } else {
            warn!("⚠️ No handlers for event: {}", event_key);
        }

        Ok(())
    }

    /// Returns current system statistics.
    /// 
    /// # Returns
    /// 
    /// Returns a clone of the current `EventSystemStats` containing
    /// information about handlers and emitted events.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use horizon_event_system::*;
    /// 
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// #     let events = create_horizon_event_system();
    /// let stats = events.get_stats().await;
    /// println!("Handlers: {}, Events: {}", stats.total_handlers, stats.events_emitted);
    /// #     Ok(())
    /// # }
    /// ```
    pub async fn get_stats(&self) -> EventSystemStats {
        let stats = self.stats.read().await;
        stats.clone()
    }

    // ============================================================================
    // GORC (Game Object Replication Channels) Integration
    // ============================================================================

    // TODO: @tristanpoland @haywoodspartan We need to implement the GORC listeners
    // such that they can properly handle an instance of an object by reference.
    // Events should be able to be emitted for a specific object instance, and the
    // handlers should be able to handle that instance directly.

    /// Registers a handler for GORC (Game Object Replication Channels) events.
    /// 
    /// GORC events are specialized events for game object replication with
    /// support for channels and layers. This provides a clean API for listening
    /// to object state changes at different priority levels.
    /// 
    /// # Arguments
    /// 
    /// * `object_type` - Type of the game object (e.g., "Asteroid", "Player")
    /// * `channel` - Replication channel (0=Critical, 1=Detailed, 2=Cosmetic, 3=Metadata)
    /// * `event_name` - Specific event name (e.g., "position_update", "health_change")
    /// * `handler` - Function to handle GORC events
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if registration succeeds, or `Err(EventError)` if it fails.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use horizon_event_system::*;
    /// 
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// #     let events = create_horizon_event_system();
    /// // Listen for critical position updates on asteroids
    /// events.on_gorc("Asteroid", 0, "position_update", |event: GorcEvent| {
    ///     // Handle asteroid position updates in critical channel
    ///     Ok(())
    /// }).await?;
    /// 
    /// // Listen for metadata updates on players
    /// events.on_gorc("Player", 3, "name_change", |event: GorcEvent| {
    ///     // Handle player name changes in metadata channel
    ///     Ok(())
    /// }).await?;
    /// #     Ok(())
    /// # }
    /// ```
    pub async fn on_gorc<T, F>(
        &self,
        object_type: &str,
        channel: u8,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + 'static,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static,
    {
        let event_key = format!("gorc:{}:{}:{}", object_type, channel, event_name);
        self.register_typed_handler(event_key, event_name, handler)
            .await
    }

    // TODO: @tristanpoland @haywoodspartan We need to implement the GORC listeners such that they can properly handle an instance of an object by reference. Events should be able to be emitted for a specific object instance, and the handlers should be able to handle that instance directly.

    /// Emits a GORC (Game Object Replication Channels) event.
    /// 
    /// This method emits events specifically for game object replication,
    /// allowing precise targeting of handlers based on object type, channel,
    /// and event name.
    /// 
    /// # Arguments
    /// 
    /// * `object_type` - Type of the game object emitting the event
    /// * `channel` - Replication channel for the event
    /// * `event_name` - Name of the specific event
    /// * `event` - The event data to emit
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if emission succeeds, or `Err(EventError)` if serialization fails.
    /// Individual handler failures are logged but don't cause the emission to fail.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use horizon_event_system::*;
    /// 
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// #     let events = create_horizon_event_system();
    /// #     let asteroid_id = "asteroid_123".to_string();
    /// #     let player_id = "player_456".to_string();
    /// #     let position_data = serde_json::json!({"x": 100, "y": 200, "z": 300});
    /// #     let achievement_data = serde_json::json!({"type": "first_kill", "points": 100});
    /// // Emit a critical position update for an asteroid
    /// events.emit_gorc("Asteroid", 0, "position_update", &GorcEvent {
    ///     object_id: asteroid_id,
    ///     object_type: "Asteroid".to_string(),
    ///     channel: 0,
    ///     data: position_data,
    ///     priority: ReplicationPriority::Critical,
    ///     timestamp: current_timestamp(),
    /// }).await?;
    /// 
    /// // Emit a metadata update for a player
    /// events.emit_gorc("Player", 3, "achievement_earned", &GorcEvent {
    ///     object_id: player_id,
    ///     object_type: "Player".to_string(),
    ///     channel: 3,
    ///     data: achievement_data,
    ///     priority: ReplicationPriority::Low,
    ///     timestamp: current_timestamp(),
    /// }).await?;
    /// #     Ok(())
    /// # }
    /// ```
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
}

// ============================================================================
// Statistics
// ============================================================================

/// Statistics about the event system's performance and usage.
/// 
/// These statistics are useful for monitoring system health, debugging
/// performance issues, and understanding event system usage patterns.
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::*;
/// 
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// #     let events = create_horizon_event_system();
/// let stats = events.get_stats().await;
/// println!("Event system has {} handlers and has processed {} events", 
///          stats.total_handlers, stats.events_emitted);
/// #     Ok(())
/// # }
/// ```
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct EventSystemStats {
    /// Total number of registered event handlers
    pub total_handlers: usize,
    /// Total number of events emitted since system start
    pub events_emitted: u64,
}

impl Default for EventSystem {
    fn default() -> Self {
        Self::new()
    }
}