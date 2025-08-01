//! Health check and monitoring endpoints for production deployment.

use crate::GameServer;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use sysinfo::{System, Pid};

pub mod metrics;
pub mod circuit_breaker;

/// Health check manager for monitoring server status
#[derive(Debug)]
pub struct HealthManager {
    server_start_time: Instant,
    last_health_check: Arc<RwLock<Option<HealthCheckResult>>>,
    circuit_breakers: Arc<RwLock<Vec<circuit_breaker::CircuitBreaker>>>,
}

/// Health check result containing system status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub status: HealthStatus,
    pub timestamp: u64,
    pub uptime_seconds: u64,
    pub memory_usage_mb: u64,
    pub active_connections: usize,
    pub plugin_count: usize,
    pub event_system_health: EventSystemHealth,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Overall health status of the server
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

/// Health information for the event system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSystemHealth {
    pub total_handlers: usize,
    pub events_processed: u64,
    pub failed_events: u64,
    pub average_event_time_ms: f64,
}

impl HealthManager {
    /// Creates a new health manager
    pub fn new() -> Self {
        Self {
            server_start_time: Instant::now(),
            last_health_check: Arc::new(RwLock::new(None)),
            circuit_breakers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Performs a comprehensive health check of the server
    pub async fn perform_health_check(&self, server: &GameServer) -> HealthCheckResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        
        // Calculate uptime
        let uptime_seconds = self.server_start_time.elapsed().as_secs();
        
        // Get memory usage
        let memory_usage_mb = self.get_memory_usage().await;
        
        // Get plugin information
        let plugin_manager = server.get_plugin_manager();
        let plugin_count = plugin_manager.plugin_count();
        
        // Get event system statistics
        let event_system = server.get_horizon_event_system();
        let event_stats = event_system.get_stats().await;
        
        let event_system_health = EventSystemHealth {
            total_handlers: event_stats.total_handlers,
            events_processed: 0, // Would need to track this in event system
            failed_events: 0,    // Would need to track this in event system
            average_event_time_ms: 0.0, // Would need performance metrics
        };
        
        // Check for issues
        if plugin_count == 0 {
            warnings.push("No plugins loaded".to_string());
        }
        
        if event_stats.total_handlers == 0 {
            warnings.push("No event handlers registered".to_string());
        }
        
        if memory_usage_mb > 1024 { // More than 1GB
            warnings.push(format!("High memory usage: {}MB", memory_usage_mb));
        }
        
        if memory_usage_mb > 2048 { // More than 2GB
            errors.push(format!("Critical memory usage: {}MB", memory_usage_mb));
        }
        
        // Check circuit breakers
        let circuit_breakers = self.circuit_breakers.read().await;
        for cb in circuit_breakers.iter() {
            if cb.is_open().await {
                errors.push(format!("Circuit breaker '{}' is open", cb.name()));
            }
        }
        
        // Determine overall health status
        let status = if !errors.is_empty() {
            HealthStatus::Unhealthy
        } else if !warnings.is_empty() {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };
        
        let result = HealthCheckResult {
            status,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            uptime_seconds,
            memory_usage_mb,
            active_connections: 0, // Would need connection manager stats
            plugin_count,
            event_system_health,
            errors,
            warnings,
        };
        
        // Cache the result
        *self.last_health_check.write().await = Some(result.clone());
        
        result
    }

    /// Gets the last cached health check result
    pub async fn get_last_health_check(&self) -> Option<HealthCheckResult> {
        self.last_health_check.read().await.clone()
    }

    /// Performs a quick liveness check (minimal overhead)
    pub async fn liveness_check(&self) -> bool {
        // Basic check - server is running if we can execute this code
        true
    }

    /// Performs a readiness check (can handle traffic)
    pub async fn readiness_check(&self, server: &GameServer) -> bool {
        let plugin_manager = server.get_plugin_manager();
        let event_system = server.get_horizon_event_system();
        
        // Check if core systems are ready
        plugin_manager.plugin_count() > 0 && 
        event_system.get_stats().await.total_handlers > 0
    }

    /// Gets current memory usage in MB
    async fn get_memory_usage(&self) -> u64 {
        #[cfg(target_os = "linux")]
        {
            self.get_linux_memory_usage().await
        }
        #[cfg(target_os = "windows")]
        {
            let mut sys = System::new_all();
            sys.refresh_all();
            if let Some(proc) = sys.process(Pid::from(std::process::id() as usize)) {
                (proc.memory() / 1024 / 1024) as u64 // memory() returns bytes, convert to MB
            } else {
                64 // Fallback value
            }
        }
        #[cfg(target_os = "macos")]
        {
            let mut sys = System::new_all();
            sys.refresh_all();
            if let Some(proc) = sys.process(Pid::from(std::process::id() as usize)) {
                (proc.memory() / 1024 / 1024) as u64 // memory() returns bytes, convert to MB
            } else {
                64
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        {
            64 // Fallback value
        }
    }

    #[cfg(target_os = "linux")]
    async fn get_linux_memory_usage(&self) -> u64 {
        use std::fs;
        
        if let Ok(status) = fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    if let Some(kb_str) = line.split_whitespace().nth(1) {
                        if let Ok(kb) = kb_str.parse::<u64>() {
                            return kb / 1024; // Convert KB to MB
                        }
                    }
                    break;
                }
            }
        }
        
        64 // Fallback value
    }

    /// Adds a circuit breaker to monitor
    pub async fn add_circuit_breaker(&self, circuit_breaker: circuit_breaker::CircuitBreaker) {
        self.circuit_breakers.write().await.push(circuit_breaker);
    }

    /// Gets health metrics in Prometheus format
    pub async fn get_prometheus_metrics(&self, server: &GameServer) -> String {
        let health_check = self.perform_health_check(server).await;
        
        let status_value = match health_check.status {
            HealthStatus::Healthy => 1.0,
            HealthStatus::Degraded => 0.5,
            HealthStatus::Unhealthy => 0.0,
        };
        
        format!(
            "# HELP horizon_server_health Overall server health status\n\
             # TYPE horizon_server_health gauge\n\
             horizon_server_health {}\n\
             # HELP horizon_server_uptime_seconds Server uptime in seconds\n\
             # TYPE horizon_server_uptime_seconds counter\n\
             horizon_server_uptime_seconds {}\n\
             # HELP horizon_server_memory_usage_mb Memory usage in megabytes\n\
             # TYPE horizon_server_memory_usage_mb gauge\n\
             horizon_server_memory_usage_mb {}\n\
             # HELP horizon_server_plugins_loaded Number of loaded plugins\n\
             # TYPE horizon_server_plugins_loaded gauge\n\
             horizon_server_plugins_loaded {}\n\
             # HELP horizon_server_event_handlers Total event handlers registered\n\
             # TYPE horizon_server_event_handlers gauge\n\
             horizon_server_event_handlers {}\n",
            status_value,
            health_check.uptime_seconds,
            health_check.memory_usage_mb,
            health_check.plugin_count,
            health_check.event_system_health.total_handlers
        )
    }
}

impl Default for HealthManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create_server;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_health_check() {
        let health_manager = HealthManager::new();
        let server = create_server();
        
        let result = health_manager.perform_health_check(&server).await;
        
        // Basic assertions
        assert!(result.uptime_seconds < 60); // Should be very small for new server
        assert_eq!(result.plugin_count, 0); // No plugins loaded in test
        
        // Status should be degraded due to no plugins
        assert_eq!(result.status, HealthStatus::Degraded);
        assert!(!result.warnings.is_empty());
    }

    #[tokio::test]
    async fn test_liveness_check() {
        let health_manager = HealthManager::new();
        assert!(health_manager.liveness_check().await);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_readiness_check() {
        let health_manager = HealthManager::new();
        let server = create_server();
        
        // Should not be ready with no plugins
        assert!(!health_manager.readiness_check(&server).await);
    }
}