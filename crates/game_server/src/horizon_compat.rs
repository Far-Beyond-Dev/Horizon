//! Horizon Event System compatibility layer
//! 
//! This provides the exact same API that existing Horizon code expects,
//! but implemented using the universal plugin system underneath.

use universal_plugin_system::{
    event::{EventBus, StructuredEventKey, Event as UniversalEvent},
    propagation::CompositePropagator,
    EventError as UniversalEventError,
};
use std::sync::Arc;
use serde::{Serialize, Deserialize};

/// Re-export types that Horizon code expects
pub use universal_plugin_system::EventError;

/// Compatible Event trait that Horizon code expects
pub trait Event: UniversalEvent + Serialize + for<'de> Deserialize<'de> {}

// Blanket implementation for any type that implements the required traits
impl<T> Event for T where T: UniversalEvent + Serialize + for<'de> Deserialize<'de> {}

/// Compatible types that Horizon integration tests expect
pub use crate::config::RegionBounds;

/// Current timestamp function that integration tests expect
pub fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Horizon Event System wrapper that provides the familiar API
/// while using the universal plugin system underneath
pub struct HorizonEventSystem {
    event_bus: Arc<EventBus<StructuredEventKey, CompositePropagator<StructuredEventKey>>>,
}

impl HorizonEventSystem {
    /// Create from a universal event bus
    pub fn new(event_bus: Arc<EventBus<StructuredEventKey, CompositePropagator<StructuredEventKey>>>) -> Self {
        Self { event_bus }
    }

    /// Get the underlying universal event bus
    pub fn universal_event_bus(&self) -> &Arc<EventBus<StructuredEventKey, CompositePropagator<StructuredEventKey>>> {
        &self.event_bus
    }
}

// Generate all the familiar Horizon APIs using the universal system macros
universal_plugin_system::impl_domain_methods!(HorizonEventSystem, core);
universal_plugin_system::impl_domain_methods!(HorizonEventSystem, client);
universal_plugin_system::impl_domain_methods!(HorizonEventSystem, plugin);
universal_plugin_system::impl_domain_methods!(HorizonEventSystem, gorc);

// Add Horizon-specific methods that integration tests expect
impl HorizonEventSystem {
    /// Get statistics in the format that integration tests expect
    pub async fn get_stats(&self) -> EventSystemStats {
        let universal_stats = self.event_bus.stats().await;
        
        EventSystemStats {
            events_emitted: universal_stats.events_emitted,
            events_handled: universal_stats.events_handled,
            handler_failures: universal_stats.handler_failures,
            total_handlers: universal_stats.total_handlers,
            gorc_events_emitted: 0, // Could be tracked separately if needed
            handler_registration_time: std::time::Duration::from_nanos(0),
            last_event_time: std::time::SystemTime::now(),
            avg_handler_execution_time: std::time::Duration::from_nanos(0),
            peak_concurrent_handlers: 0,
            memory_usage_bytes: 0,
        }
    }
}

/// Statistics structure expected by integration tests
#[derive(Debug, Clone)]
pub struct EventSystemStats {
    pub events_emitted: u64,
    pub events_handled: u64,
    pub handler_failures: u64,
    pub total_handlers: usize,
    // Horizon-specific stats
    pub gorc_events_emitted: u64,
    pub handler_registration_time: std::time::Duration,
    pub last_event_time: std::time::SystemTime,
    pub avg_handler_execution_time: std::time::Duration,
    pub peak_concurrent_handlers: usize,
    pub memory_usage_bytes: usize,
}