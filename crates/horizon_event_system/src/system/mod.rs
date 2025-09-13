/// Event system module - broken down into manageable components
mod client;
mod core;
mod emitters;
mod handlers;
mod management;
mod stats;
mod cache;
mod tests;
mod path_router;

// Re-export all public items from submodules
pub use client::{ClientConnectionRef, ClientResponseSender, ClientConnectionInfo};
pub use core::EventSystem;
pub use emitters::*;
pub use handlers::*;
pub use stats::{EventSystemStats, DetailedEventSystemStats, HandlerCategoryStats};
pub use path_router::PathRouter;

// Re-export utility functions
use crate::gorc::instance::GorcInstanceManager;
use std::sync::Arc;

/// Helper function to create an event system with GORC integration
pub fn create_event_system_with_gorc(
    gorc_instances: Arc<GorcInstanceManager>
) -> Arc<EventSystem> {
    Arc::new(EventSystem::with_gorc(gorc_instances))
}