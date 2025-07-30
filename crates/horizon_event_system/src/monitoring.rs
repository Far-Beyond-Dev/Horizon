/// Performance monitoring utilities
use crate::*;
use std::time::Instant;

/// Performance monitor for the entire Horizon system
pub struct HorizonMonitor {
    start_time: Instant,
    last_report: Instant,
    event_system: Arc<EventSystem>,
    gorc_system: Option<Arc<CompleteGorcSystem>>,
}

impl HorizonMonitor {
    /// Creates a new performance monitor
    pub fn new(event_system: Arc<EventSystem>) -> Self {
        Self {
            start_time: Instant::now(),
            last_report: Instant::now(),
            event_system,
            gorc_system: None,
        }
    }
    
    /// Creates a monitor with GORC system integration
    pub fn with_gorc(event_system: Arc<EventSystem>, gorc_system: Arc<CompleteGorcSystem>) -> Self {
        Self {
            start_time: Instant::now(),
            last_report: Instant::now(),
            event_system,
            gorc_system: Some(gorc_system),
        }
    }
    
    /// Generates a comprehensive system report
    pub async fn generate_report(&mut self) -> HorizonSystemReport {
        let now = Instant::now();
        let uptime = now.duration_since(self.start_time);
        let time_since_last = now.duration_since(self.last_report);
        self.last_report = now;
        
        let event_stats = self.event_system.as_ref().get_stats().await;
        let gorc_report = if let Some(ref gorc) = self.gorc_system {
            Some(gorc.get_performance_report().await)
        } else {
            None
        };
        
        HorizonSystemReport {
            timestamp: current_timestamp(),
            uptime_seconds: uptime.as_secs(),
            report_interval_seconds: time_since_last.as_secs(),
            event_system_stats: event_stats.clone(),
            gorc_performance: gorc_report.clone(),
            system_health: self.calculate_system_health(&event_stats, &gorc_report).await,
        }
    }
    
    /// Calculates overall system health score (0.0 to 1.0)
    async fn calculate_system_health(
        &self,
        event_stats: &EventSystemStats,
        gorc_report: &Option<GorcPerformanceReport>
    ) -> f32 {
        let mut health_score = 1.0;
        
        // Factor in event system health
        if event_stats.total_handlers == 0 {
            health_score -= 0.2; // No handlers is concerning
        }
        
        // Factor in GORC health if available
        if let Some(gorc) = gorc_report {
            let gorc_health = gorc.health_score();
            health_score = (health_score + gorc_health) / 2.0;
        }
        
        health_score.clamp(0.0, 1.0)
    }
    
    /// Checks if the system should trigger alerts
    pub async fn should_alert(&self) -> Vec<String> {
        let mut alerts = Vec::new();
        
        let event_stats = self.event_system.get_stats().await;
        
        // Check for event system issues
        if event_stats.total_handlers > 10000 {
            alerts.push("Very high number of event handlers registered".to_string());
        }
        
        // Check GORC system if available
        if let Some(ref gorc) = self.gorc_system {
            let gorc_report = gorc.get_performance_report().await;
            if !gorc_report.is_healthy() {
                alerts.push("GORC system health issues detected".to_string());
            }
            
            if gorc_report.network_utilization > 0.9 {
                alerts.push(format!("Critical network utilization: {:.1}%", 
                                  gorc_report.network_utilization * 100.0));
            }
        }
        
        alerts
    }
}

/// Comprehensive system health report
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HorizonSystemReport {
    pub timestamp: u64,
    pub uptime_seconds: u64,
    pub report_interval_seconds: u64,
    pub event_system_stats: EventSystemStats,
    pub gorc_performance: Option<GorcPerformanceReport>,
    pub system_health: f32,
}

impl HorizonSystemReport {
    /// Returns true if the system is operating normally
    pub fn is_healthy(&self) -> bool {
        self.system_health > 0.7 && 
        (self.gorc_performance.as_ref().map(|g| g.is_healthy()).unwrap_or(true))
    }
    
    /// Gets actionable recommendations for system improvement
    pub fn get_recommendations(&self) -> Vec<String> {
        let mut recommendations = Vec::new();
        
        if self.system_health < 0.5 {
            recommendations.push("System health is poor - investigate event system and GORC performance".to_string());
        }
        
        if let Some(ref gorc) = self.gorc_performance {
            recommendations.extend(gorc.get_recommendations());
        }
        
        if self.event_system_stats.total_handlers == 0 {
            recommendations.push("No event handlers registered - system may not be functioning".to_string());
        }
        
        recommendations
    }
}