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
//! ## Usage Examples
//!
//! ### Basic Event System
//!
//! ```rust,no_run
//! use universal_plugin_system::*;
//!
//! // Define your event types
//! #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
//! struct PlayerJoinedEvent {
//!     player_id: u64,
//!     name: String,
//! }
//!
//! impl Event for PlayerJoinedEvent {
//!     fn event_type() -> &'static str { "player_joined" }
//! }
//!
//! // Create an event bus with default propagation
//! let mut event_bus = EventBus::new();
//!
//! // Register a handler
//! event_bus.on("player", "joined", |event: PlayerJoinedEvent| {
//!     println!("Player {} joined!", event.name);
//!     Ok(())
//! }).await?;
//!
//! // Emit an event
//! event_bus.emit("player", "joined", &PlayerJoinedEvent {
//!     player_id: 123,
//!     name: "Alice".to_string(),
//! }).await?;
//! ```
//!
//! ### Custom Propagation Logic
//!
//! ```rust,no_run
//! use universal_plugin_system::*;
//!
//! // Define custom propagation logic (like GORC spatial propagation)
//! struct SpatialPropagator {
//!     // Your spatial logic here
//! }
//!
//! impl EventPropagator for SpatialPropagator {
//!     async fn should_propagate(&self, event_key: &str, context: &PropagationContext) -> bool {
//!         // Custom logic to determine if event should reach specific handlers
//!         true
//!     }
//!
//!     async fn transform_event(&self, event: Arc<EventData>, context: &PropagationContext) -> Option<Arc<EventData>> {
//!         // Optional event transformation based on context
//!         Some(event)
//!     }
//! }
//!
//! // Use custom propagator
//! let propagator = SpatialPropagator::new();
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

// Re-exports for convenience
pub use event::{
    Event, EventData, EventHandler, EventBus, EventKey, EventKeyType, 
    StructuredEventKey, EventNamespace, TypedEventKey
};
pub use plugin::{Plugin, SimplePlugin, PluginWrapper};
pub use manager::{PluginManager, PluginConfig, LoadedPlugin};
pub use context::{PluginContext, ContextProvider};
pub use propagation::{
    EventPropagator, DefaultPropagator, AllEqPropagator, NamespacePropagator, 
    PropagationContext
};
pub use error::{PluginSystemError, EventError};
// pub use macros::*; // TODO: Fix macros

/// Version information for ABI compatibility
pub const UNIVERSAL_PLUGIN_SYSTEM_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default event bus type with AllEq propagation (most common use case)
pub type DefaultEventBus = EventBus<StructuredEventKey, AllEqPropagator>;

/// Result type used throughout the system
pub type Result<T> = std::result::Result<T, PluginSystemError>;