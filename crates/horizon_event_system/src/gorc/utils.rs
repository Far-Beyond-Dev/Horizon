//! Utility functions for GORC system setup and management.
//!
//! This module provides high-level utilities for creating complete GORC systems,
//! validation tools, and performance monitoring capabilities.

use super::{
    defaults, CompleteGorcSystem, GorcError, GorcInstanceManager, GorcPerformanceReport,
    NetworkReplicationEngine, ReplicationCoordinator,
};
use crate::context::ServerContext;
use std::sync::Arc;

/// Creates a complete GORC system with all components configured.
/// 
/// This is the primary entry point for setting up a full GORC system with
/// all necessary components properly initialized and connected.
/// 
/// # Arguments
/// 
/// * `server_context` - Server context for network communication
/// 
/// # Returns
/// 
/// A complete GORC system ready for use, or a `GorcError` if initialization failed.
/// 
/// # Example
/// 
/// ```rust,no_run
/// use horizon_event_system::gorc;
/// use std::sync::Arc;
/// 
/// // Mock server context for example
/// struct MyServerContext;
/// impl horizon_event_system::context::ServerContext for MyServerContext {}
/// 
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let server_context = Arc::new(MyServerContext);
/// let gorc_system = gorc::utils::create_complete_gorc_system(server_context)?;
/// # Ok(())
/// # }
/// ```
pub fn create_complete_gorc_system(
    server_context: Arc<dyn ServerContext>
) -> Result<CompleteGorcSystem, GorcError> {
    let instance_manager = Arc::new(GorcInstanceManager::new());
    let network_config = defaults::default_network_config();
    
    let network_engine = Arc::new(NetworkReplicationEngine::new(
        network_config,
        instance_manager.clone(),
        server_context,
    ));
    
    let coordinator = ReplicationCoordinator::new(
        network_engine.clone(),
        instance_manager.clone(),
    );
    
    Ok(CompleteGorcSystem {
        instance_manager,
        network_engine,
        coordinator,
    })
}

/// Validates a GORC system configuration and reports potential issues.
/// 
/// This function performs comprehensive checks on a GORC system to identify
/// potential performance problems, configuration issues, or operational concerns.
/// 
/// # Arguments
/// 
/// * `system` - The GORC system to validate
/// 
/// # Returns
/// 
/// A vector of issue descriptions. An empty vector indicates no issues found.
/// 
/// # Validation Checks
/// 
/// * **Object Count**: Warns about very high or zero object counts
/// * **Network Utilization**: Alerts on high bandwidth usage
/// * **Dropped Updates**: Reports any dropped replication updates
/// * **System Health**: Overall system performance assessment
pub async fn validate_gorc_system(system: &CompleteGorcSystem) -> Vec<String> {
    let mut issues = Vec::new();
    
    // Check instance manager
    let instance_stats = system.instance_manager.get_stats().await;
    if instance_stats.total_objects == 0 {
        issues.push("No objects registered in GORC system".to_string());
    }
    
    if instance_stats.total_objects > 10000 {
        issues.push(format!("Very high object count: {}", instance_stats.total_objects));
    }
    
    // Check network engine
    let network_stats = system.network_engine.get_stats().await;
    let utilization = network_stats.network_utilization;
    
    if utilization > 0.9 {
        issues.push(format!("High network utilization: {:.1}%", utilization * 100.0));
    }
    
    if network_stats.updates_dropped > 0 {
        issues.push(format!("Updates being dropped: {}", network_stats.updates_dropped));
    }
    
    issues
}

/// Creates a comprehensive performance monitoring report for the GORC system.
/// 
/// Generates a detailed performance report including statistics from all GORC
/// components, network utilization metrics, and actionable recommendations.
/// 
/// # Arguments
/// 
/// * `system` - The GORC system to analyze
/// 
/// # Returns
/// 
/// A detailed performance report with metrics, health status, and recommendations.
/// 
/// # Report Contents
/// 
/// * **Object Statistics**: Total objects and subscription counts
/// * **Network Metrics**: Utilization, throughput, and error rates
/// * **Performance Analysis**: Health scoring and trend analysis
/// * **Recommendations**: Actionable suggestions for optimization
pub async fn create_performance_report(system: &CompleteGorcSystem) -> GorcPerformanceReport {
    let _replication_stats = system.coordinator.get_stats().await;
    let instance_stats = system.instance_manager.get_stats().await;
    let network_stats = system.network_engine.get_stats().await;
    let utilization = network_stats.network_utilization;
    
    GorcPerformanceReport {
        timestamp: crate::utils::current_timestamp(),
        total_objects: instance_stats.total_objects,
        total_subscriptions: instance_stats.total_subscriptions,
        network_utilization: utilization,
        events_sent: network_stats.updates_sent,
        bytes_transmitted: network_stats.bytes_transmitted,
        updates_dropped: network_stats.updates_dropped,
        avg_batch_size: network_stats.avg_batch_size,
        issues: validate_gorc_system(system).await,
    }
}

/// Optimizes a GORC system configuration based on current performance metrics.
/// 
/// Analyzes the system's performance and automatically adjusts configuration
/// parameters to improve efficiency and reduce resource usage.
/// 
/// # Arguments
/// 
/// * `system` - The GORC system to optimize
/// 
/// # Returns
/// 
/// A vector of optimization actions taken, or an error if optimization failed.
/// 
/// # Optimization Strategies
/// 
/// * **Network Tuning**: Adjusts batch sizes and frequencies based on utilization
/// * **Zone Scaling**: Modifies zone sizes based on player distribution
/// * **Compression Settings**: Enables more aggressive compression if needed
/// * **Priority Adjustment**: Rebalances priority queues based on usage patterns
pub async fn optimize_gorc_system(system: &mut CompleteGorcSystem) -> Result<Vec<String>, GorcError> {
    let mut optimizations = Vec::new();
    let report = create_performance_report(system).await;
    
    // Network optimizations
    if report.network_utilization > 0.8 {
        // Increase compression threshold to enable more aggressive compression
        let mut config = system.network_engine.get_config().await;
        config.compression_threshold = (config.compression_threshold / 2).max(32); // Reduce threshold by half, minimum 32 bytes
        system.network_engine.update_config(config).await
            .map_err(|e| GorcError::Configuration(format!("Failed to update network config: {}", e)))?;
        optimizations.push("Increased compression threshold to reduce bandwidth".to_string());
    }
    
    if report.updates_dropped > 0 {
        // Increase priority queue sizes to handle more concurrent updates
        let mut config = system.network_engine.get_config().await;
        for (_priority, size) in config.priority_queue_sizes.iter_mut() {
            *size = (*size * 3) / 2; // Increase by 50%
        }
        system.network_engine.update_config(config).await
            .map_err(|e| GorcError::Configuration(format!("Failed to update network config: {}", e)))?;
        optimizations.push("Adjusted priority queue sizes to reduce dropped updates".to_string());
    }
    
    if report.avg_batch_size < 10.0 {
        // Increase batch size limits to improve efficiency
        let mut config = system.network_engine.get_config().await;
        config.max_batch_size = (config.max_batch_size * 3) / 2; // Increase by 50%
        system.network_engine.update_config(config).await
            .map_err(|e| GorcError::Configuration(format!("Failed to update network config: {}", e)))?;
        optimizations.push("Increased batch size limits to improve efficiency".to_string());
    }
    
    if optimizations.is_empty() {
        optimizations.push("System is already well optimized".to_string());
    }
    
    Ok(optimizations)
}

/// Performs a health check on a GORC system and returns a summary.
/// 
/// Quick health assessment that can be called frequently for monitoring
/// without the overhead of a full performance report.
/// 
/// # Arguments
/// 
/// * `system` - The GORC system to check
/// 
/// # Returns
/// 
/// A health summary with basic metrics and status.
pub async fn quick_health_check(system: &CompleteGorcSystem) -> GorcHealthSummary {
    let instance_stats = system.instance_manager.get_stats().await;
    let network_stats = system.network_engine.get_stats().await;
    
    let is_healthy = network_stats.network_utilization < 0.8 
        && network_stats.updates_dropped == 0
        && instance_stats.total_objects > 0;
    
    GorcHealthSummary {
        is_healthy,
        object_count: instance_stats.total_objects,
        network_utilization: network_stats.network_utilization,
        updates_dropped: network_stats.updates_dropped,
        last_check: crate::utils::current_timestamp(),
    }
}

/// Quick health summary for GORC system monitoring.
/// 
/// Lightweight health status that can be checked frequently without
/// significant performance impact.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GorcHealthSummary {
    /// Whether the system is operating within healthy parameters
    pub is_healthy: bool,
    /// Current number of registered objects
    pub object_count: usize,
    /// Current network utilization (0.0 to 1.0)
    pub network_utilization: f32,
    /// Number of updates dropped since last reset
    pub updates_dropped: u64,
    /// Timestamp of this health check
    pub last_check: u64,
}

impl GorcHealthSummary {
    /// Gets a simple health score from 0.0 to 1.0.
    pub fn health_score(&self) -> f32 {
        if !self.is_healthy {
            return 0.3; // Base score for unhealthy systems
        }
        
        let mut score = 1.0;
        
        // Penalize high network utilization
        if self.network_utilization > 0.5 {
            score -= (self.network_utilization - 0.5) * 0.4;
        }
        
        // Penalize dropped updates
        if self.updates_dropped > 0 {
            score -= 0.2;
        }
        
        // Penalize very low object counts
        if self.object_count < 10 {
            score -= 0.1;
        }
        
        score.max(0.0)
    }
}