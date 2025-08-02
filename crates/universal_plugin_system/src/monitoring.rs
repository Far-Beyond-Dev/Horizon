//! Performance monitoring and statistics for the universal plugin system
//! 
//! This module provides comprehensive monitoring capabilities to track event system
//! performance, similar to horizon_event_system's monitoring capabilities.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Detailed statistics about event system performance
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DetailedEventStats {
    /// Basic event statistics
    pub events_emitted: u64,
    pub events_handled: u64,
    pub handler_failures: u64,
    pub total_handlers: usize,
    
    /// Performance metrics
    pub average_emit_time_ns: u64,
    pub max_emit_time_ns: u64,
    pub min_emit_time_ns: u64,
    pub total_emit_time_ns: u64,
    
    /// Handler-specific statistics
    pub handler_stats: HashMap<String, HandlerStats>,
    
    /// Event type statistics
    pub event_type_stats: HashMap<String, EventTypeStats>,
    
    /// Timing statistics
    pub uptime_seconds: u64,
    pub last_reset: Option<u64>,
}

/// Statistics for individual handlers
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HandlerStats {
    pub events_handled: u64,
    pub failures: u64,
    pub average_execution_time_ns: u64,
    pub max_execution_time_ns: u64,
    pub min_execution_time_ns: u64,
    pub total_execution_time_ns: u64,
}

/// Statistics for individual event types
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventTypeStats {
    pub events_emitted: u64,
    pub handlers_triggered: u64,
    pub average_propagation_time_ns: u64,
    pub serialization_cache_hits: u64,
    pub serialization_cache_misses: u64,
}

/// Performance monitor for tracking event system metrics
pub struct PerformanceMonitor {
    /// Statistics data
    stats: Arc<tokio::sync::RwLock<DetailedEventStats>>,
    /// Start time for uptime calculation
    start_time: Instant,
}

impl PerformanceMonitor {
    /// Create a new performance monitor
    pub fn new() -> Self {
        Self {
            stats: Arc::new(tokio::sync::RwLock::new(DetailedEventStats::default())),
            start_time: Instant::now(),
        }
    }
    
    /// Record an event emission
    #[inline]
    pub async fn record_emit(&self, event_type: &str, duration: Duration) {
        let mut stats = self.stats.write().await;
        stats.events_emitted += 1;
        
        let duration_ns = duration.as_nanos() as u64;
        stats.total_emit_time_ns += duration_ns;
        
        if stats.min_emit_time_ns == 0 || duration_ns < stats.min_emit_time_ns {
            stats.min_emit_time_ns = duration_ns;
        }
        if duration_ns > stats.max_emit_time_ns {
            stats.max_emit_time_ns = duration_ns;
        }
        
        stats.average_emit_time_ns = stats.total_emit_time_ns / stats.events_emitted;
        
        // Update event type statistics
        let event_stats = stats.event_type_stats.entry(event_type.to_string())
            .or_insert_with(EventTypeStats::default);
        event_stats.events_emitted += 1;
    }
    
    /// Record a handler execution
    #[inline]
    pub async fn record_handler_execution(
        &self, 
        handler_name: &str, 
        duration: Duration, 
        success: bool
    ) {
        let mut stats = self.stats.write().await;
        
        if success {
            stats.events_handled += 1;
        } else {
            stats.handler_failures += 1;
        }
        
        // Update handler-specific statistics
        let handler_stats = stats.handler_stats.entry(handler_name.to_string())
            .or_insert_with(HandlerStats::default);
            
        let duration_ns = duration.as_nanos() as u64;
        handler_stats.total_execution_time_ns += duration_ns;
        
        if success {
            handler_stats.events_handled += 1;
        } else {
            handler_stats.failures += 1;
        }
        
        if handler_stats.min_execution_time_ns == 0 || duration_ns < handler_stats.min_execution_time_ns {
            handler_stats.min_execution_time_ns = duration_ns;
        }
        if duration_ns > handler_stats.max_execution_time_ns {
            handler_stats.max_execution_time_ns = duration_ns;
        }
        
        let total_executions = handler_stats.events_handled + handler_stats.failures;
        if total_executions > 0 {
            handler_stats.average_execution_time_ns = 
                handler_stats.total_execution_time_ns / total_executions;
        }
    }
    
    /// Record a serialization cache hit
    #[inline]
    pub async fn record_cache_hit(&self, event_type: &str) {
        let mut stats = self.stats.write().await;
        let event_stats = stats.event_type_stats.entry(event_type.to_string())
            .or_insert_with(EventTypeStats::default);
        event_stats.serialization_cache_hits += 1;
    }
    
    /// Record a serialization cache miss
    #[inline]
    pub async fn record_cache_miss(&self, event_type: &str) {
        let mut stats = self.stats.write().await;
        let event_stats = stats.event_type_stats.entry(event_type.to_string())
            .or_insert_with(EventTypeStats::default);
        event_stats.serialization_cache_misses += 1;
    }
    
    /// Get current statistics
    pub async fn get_stats(&self) -> DetailedEventStats {
        let mut stats = self.stats.read().await.clone();
        stats.uptime_seconds = self.start_time.elapsed().as_secs();
        stats
    }
    
    /// Reset all statistics
    pub async fn reset_stats(&self) {
        let mut stats = self.stats.write().await;
        *stats = DetailedEventStats::default();
        stats.last_reset = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );
    }
    
    /// Get a performance report summary
    pub async fn get_performance_report(&self) -> String {
        let stats = self.get_stats().await;
        
        format!(
            "Universal Plugin System Performance Report\n\
            ==========================================\n\
            Uptime: {}s\n\
            Events Emitted: {}\n\
            Events Handled: {}\n\
            Handler Failures: {}\n\
            Total Handlers: {}\n\
            Average Emit Time: {}μs\n\
            Max Emit Time: {}μs\n\
            Min Emit Time: {}μs\n\
            \n\
            Top Event Types by Volume:\n\
            {}",
            stats.uptime_seconds,
            stats.events_emitted,
            stats.events_handled,
            stats.handler_failures,
            stats.total_handlers,
            stats.average_emit_time_ns / 1000,
            stats.max_emit_time_ns / 1000,
            stats.min_emit_time_ns / 1000,
            format_top_event_types(&stats.event_type_stats)
        )
    }
}

impl Default for PerformanceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to format top event types
fn format_top_event_types(event_stats: &HashMap<String, EventTypeStats>) -> String {
    let mut event_vec: Vec<_> = event_stats.iter().collect();
    event_vec.sort_by(|a, b| b.1.events_emitted.cmp(&a.1.events_emitted));
    
    event_vec.iter()
        .take(10)
        .map(|(name, stats)| {
            format!("  {}: {} events, {}μs avg", 
                name, 
                stats.events_emitted, 
                stats.average_propagation_time_ns / 1000)
        })
        .collect::<Vec<_>>()
        .join("\n")
}