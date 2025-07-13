/// Statistics tracking for the event system
use serde::{Deserialize, Serialize};

/// Core event system statistics for monitoring performance
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