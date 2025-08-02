//! # Universal Plugin System
//!
//! A flexible, reusable plugin system that can be used across multiple applications.
//! This crate provides the building blocks to create your own plugin ecosystem with
//! custom event handling, propagation logic, and plugin management.
//!
//! ## Key Features
//!
//! - **Flexible Event System**: Define your own event types and handlers
//! - **Custom Propagation**: Plugin custom event propagation logic (spatial, network, etc.)
//! - **Type-Safe**: Full type safety with compile-time guarantees
//! - **Performance**: Optimized for high-throughput event processing
//! - **Safety**: Comprehensive panic handling and memory safety
//! - **Dynamic Loading**: Runtime plugin loading with version compatibility
//! - **Two-Phase Initialization**: Guaranteed handler registration before plugin initialization
//!
//! ## Architecture
//!
//! The system is built around several core concepts:
//!
//! - **EventBus**: Central event routing and handling
//! - **EventPropagator**: Customizable event propagation logic
//! - **PluginManager**: Dynamic plugin loading and lifecycle management
//! - **Context**: Dependency injection for plugins
//!
//! ## Plugin Lifecycle
//!
//! The plugin system uses a **two-phase initialization** pattern to prevent race conditions
//! and ensure all plugins can communicate properly:
//!
//! ### Phase 1: Handler Registration (`register_handlers` / `pre_init`)
//! - Called first on **ALL** plugins before any plugin proceeds to Phase 2
//! - Plugins register their event handlers during this phase
//! - No plugin can emit events or perform initialization logic yet
//! - Ensures all event handlers are available before any plugin tries to use them
//!
//! ### Phase 2: Full Initialization (`on_init` / `init`)
//! - Called only after ALL plugins have completed Phase 1
//! - Plugins can now safely emit events, knowing all handlers are registered
//! - Plugins perform their main initialization logic here
//! - Access to full system context and all registered event handlers
//!
//! This pattern prevents scenarios where Plugin A emits an event during initialization
//! but Plugin B hasn't registered its handler for that event yet.
//!
//! ### Phase 3: Operation
//! - Normal plugin operation with event processing
//!
//! ### Phase 4: Shutdown (`on_shutdown` / `shutdown`)
//! - Graceful cleanup and resource deallocation
//!
//! ## Usage Examples
//!
//! ### Basic Event System
//!
//! ```rust,no_run
//! use universal_plugin_system::*;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     // Define your event types
//!     #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
//!     struct PlayerJoinedEvent {
//!         player_id: u64,
//!         name: String,
//!     }
//!
//!     impl Event for PlayerJoinedEvent {
//!         fn event_type() -> &'static str { "player_joined" }
//!     }
//!
//!     // Create an event bus with default propagation
//!     let propagator = AllEqPropagator::new();
//!     let mut event_bus = EventBus::with_propagator(propagator);
//!
//!     // Create an event key
//!     let player_joined_key = StructuredEventKey::domain_event("player", "joined");
//!
//!     // Register a handler
//!     event_bus.on_key(player_joined_key.clone(), |event: PlayerJoinedEvent| {
//!         println!("Player {} joined!", event.name);
//!         Ok(())
//!     }).await?;
//!
//!     // Emit an event
//!     event_bus.emit_key(player_joined_key, &PlayerJoinedEvent {
//!         player_id: 123,
//!         name: "Alice".to_string(),
//!     }).await?;
//!     
//!     Ok(())
//! }
//! ```
//!
//! ### Custom Propagation Logic
//!
//! ```rust,no_run
//! use universal_plugin_system::*;
//! use std::sync::Arc;
//!
//! // Define custom propagation logic for your application
//! struct CustomSpatialPropagator;
//!
//! impl CustomSpatialPropagator {
//!     fn new() -> Self { Self }
//! }
//!
//! #[async_trait::async_trait]
//! impl EventPropagator<StructuredEventKey> for CustomSpatialPropagator {
//!     async fn should_propagate(&self, _event_key: &StructuredEventKey, _context: &PropagationContext<StructuredEventKey>) -> bool {
//!         // Custom logic to determine if event should reach specific handlers
//!         // For example: spatial filtering, permission checks, etc.
//!         true
//!     }
//!
//!     async fn transform_event(&self, event: Arc<EventData>, _context: &PropagationContext<StructuredEventKey>) -> Option<Arc<EventData>> {
//!         // Optional event transformation based on context
//!         Some(event)
//!     }
//! }
//!
//! // Use custom propagator
//! let propagator = CustomSpatialPropagator::new();
//! let mut event_bus = EventBus::with_propagator(propagator);
//! ```

pub mod event;
pub mod plugin;
pub mod manager;
pub mod context;
pub mod propagation;
pub mod macros;
pub mod error;
pub mod utils;
pub mod cache;
pub mod monitoring;

// Re-exports for convenience
pub use event::{
    Event, EventData, EventHandler, EventBus, EventKey, EventKeyType, 
    StructuredEventKey, TypedEventKey
};
pub use plugin::{Plugin, SimplePlugin, PluginWrapper};
pub use manager::{PluginManager, PluginConfig, LoadedPlugin};
pub use context::{PluginContext, ContextProvider};
pub use propagation::{
    EventPropagator, DefaultPropagator, AllEqPropagator, DomainPropagator, 
    PropagationContext, CompositePropagator, UniversalAllEqPropagator
};
pub use error::{PluginSystemError, EventError};
pub use cache::{SerializationBufferPool, CachedEventData};
pub use monitoring::{PerformanceMonitor, DetailedEventStats, HandlerStats, EventTypeStats};
// pub use macros::*; // TODO: Fix macros

// Re-export the universal event system
// (defined below)

/// Version information for ABI compatibility
pub const UNIVERSAL_PLUGIN_SYSTEM_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default event bus type with AllEq propagation (most common use case)
pub type DefaultEventBus = EventBus<StructuredEventKey, AllEqPropagator>;

/// Universal event system that can be used by any application
/// 
/// This is the main entry point for the universal plugin system. It provides
/// a complete event system with context and propagation that can be customized
/// by the host application.
pub struct UniversalEventSystem<C, P> 
where
    P: EventPropagator<StructuredEventKey>,
{
    /// The underlying event bus
    pub event_bus: std::sync::Arc<EventBus<StructuredEventKey, P>>,
    /// Application context
    pub context: std::sync::Arc<C>,
}

impl<C, P> UniversalEventSystem<C, P> 
where
    P: EventPropagator<StructuredEventKey>,
{
    /// Create a new universal event system
    pub fn new(context: std::sync::Arc<C>, propagator: P) -> Self {
        let event_bus = std::sync::Arc::new(EventBus::with_propagator(propagator));
        Self {
            event_bus,
            context,
        }
    }

    /// Register an event handler using domain and event name
    pub async fn on<T, F>(
        &self,
        domain: &str,
        event_name: &str,
        handler: F,
    ) -> Result<()>
    where
        T: Event + for<'de> serde::Deserialize<'de>,
        F: Fn(T) -> std::result::Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        self.event_bus.on(domain, event_name, handler).await?;
        Ok(())
    }

    /// Emit an event using domain and event name
    pub async fn emit<T>(
        &self,
        domain: &str,
        event_name: &str,
        event: &T,
    ) -> Result<()>
    where
        T: Event + serde::Serialize,
    {
        self.event_bus.emit(domain, event_name, event).await?;
        Ok(())
    }

    /// Get the context
    pub fn context(&self) -> std::sync::Arc<C> {
        self.context.clone()
    }
}

/// Result type used throughout the system
pub type Result<T> = std::result::Result<T, PluginSystemError>;