/// Event system module - broken down into manageable components
mod client;
mod core;
mod emitters;
mod handlers;
mod management;
mod stats;
mod tests;

// Re-export all public items from submodules
pub use client::{ClientConnectionRef, ClientResponseSender};
pub use core::EventSystem;
pub use stats::{EventSystemStats, DetailedEventSystemStats, HandlerCategoryStats};

// Re-export utility functions
use crate::gorc::instance::GorcInstanceManager;
use std::sync::Arc;

/// Helper function to create an event system with GORC integration
pub fn create_event_system_with_gorc(
    gorc_instances: Arc<GorcInstanceManager>
) -> Arc<EventSystem> {
    Arc::new(EventSystem::with_gorc(gorc_instances))
}