use std::sync::Arc;
use super::{
    GorcInstanceManager, NetworkReplicationEngine, ReplicationCoordinator,
    GorcObjectId, GorcObject, NetworkError, ReplicationStats, utils
};

/// Current version of the GORC system
pub const GORC_VERSION: &str = "1.0.0";

/// Maximum number of replication channels supported
pub const MAX_CHANNELS: u8 = 4;

/// Complete GORC system with all components.
/// 
/// This struct provides a high-level interface to the entire GORC system,
/// combining all components into a single, easy-to-use API.
/// 
/// # Usage
/// 
/// ```rust,no_run
/// use horizon_event_system::gorc::{self, examples::ExampleAsteroid, MineralType};
/// use horizon_event_system::{Vec3, PlayerId};
/// use std::sync::Arc;
/// use std::time::Duration;
/// 
/// // Mock server context and object for example
/// struct MyServerContext;
/// impl horizon_event_system::context::ServerContext for MyServerContext {}
/// 
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Create system with utilities
/// let server_context = Arc::new(MyServerContext);
/// let mut gorc_system = gorc::utils::create_complete_gorc_system(server_context)?;
/// 
/// // Register objects
/// let my_object = ExampleAsteroid::new(Vec3::new(0.0, 0.0, 0.0), MineralType::Iron);
/// let position = Vec3::new(100.0, 0.0, 200.0);
/// let object_id = gorc_system.register_object(my_object, position).await;
/// 
/// // Add players
/// let player_id = PlayerId::new();
/// let player_position = Vec3::new(50.0, 0.0, 180.0);
/// gorc_system.add_player(player_id, player_position).await;
/// 
/// // Run replication
/// loop {
///     gorc_system.tick().await?;
///     tokio::time::sleep(Duration::from_millis(16)).await;
/// }
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct CompleteGorcSystem {
    /// Instance manager for object lifecycle
    pub instance_manager: Arc<GorcInstanceManager>,
    /// Network engine for replication
    pub network_engine: Arc<NetworkReplicationEngine>,
    /// Coordinator that ties everything together
    pub coordinator: ReplicationCoordinator,
}

impl CompleteGorcSystem {
    /// Registers a new object with the GORC system.
    /// 
    /// # Arguments
    /// 
    /// * `object` - The object to register (must implement `GorcObject`)
    /// * `position` - Initial 3D position of the object
    /// 
    /// # Returns
    /// 
    /// A unique identifier for the registered object.
    pub async fn register_object<T: GorcObject + 'static>(
        &mut self,
        object: T,
        position: crate::types::Vec3,
    ) -> GorcObjectId {
        self.coordinator.register_object(object, position).await
    }
    
    /// Unregisters an object from the GORC system.
    /// 
    /// # Arguments
    /// 
    /// * `object_id` - The ID of the object to remove
    pub async fn unregister_object(&mut self, object_id: GorcObjectId) {
        self.coordinator.unregister_object(object_id).await;
    }
    
    /// Adds a player to the system.
    /// 
    /// # Arguments
    /// 
    /// * `player_id` - Unique identifier for the player
    /// * `position` - Initial 3D position of the player
    pub async fn add_player(&self, player_id: crate::types::PlayerId, position: crate::types::Vec3) {
        self.coordinator.add_player(player_id, position).await;
    }
    
    /// Removes a player from the system.
    /// 
    /// # Arguments
    /// 
    /// * `player_id` - The ID of the player to remove
    pub async fn remove_player(&self, player_id: crate::types::PlayerId) {
        self.coordinator.remove_player(player_id).await;
    }
    
    /// Updates a player's position.
    /// 
    /// # Arguments
    /// 
    /// * `player_id` - The player to update
    /// * `position` - New 3D position
    pub async fn update_player_position(&self, player_id: crate::types::PlayerId, position: crate::types::Vec3) {
        self.coordinator.update_player_position(player_id, position).await;
    }
    
    /// Runs one tick of the replication system.
    /// 
    /// This should be called regularly (typically 60 times per second) to
    /// process replication updates and maintain system state.
    /// 
    /// # Returns
    /// 
    /// `Ok(())` if the tick completed successfully, or a `NetworkError` if there was an issue.
    pub async fn tick(&mut self) -> Result<(), NetworkError> {
        self.coordinator.tick().await
    }
    
    /// Gets comprehensive system statistics.
    /// 
    /// # Returns
    /// 
    /// Current replication statistics including throughput, errors, and performance metrics.
    pub async fn get_stats(&self) -> ReplicationStats {
        self.coordinator.get_stats().await
    }
    
    /// Gets a performance report with analysis and recommendations.
    /// 
    /// # Returns
    /// 
    /// A detailed performance report with health analysis and optimization suggestions.
    pub async fn get_performance_report(&self) -> GorcPerformanceReport {
        utils::create_performance_report(self).await
    }
    
    /// Sets up core event listeners for GORC integration.
    /// 
    /// This registers GORC to listen for core movement events and automatically
    /// update player positions in the replication system.
    /// 
    /// # Arguments
    /// 
    /// * `event_system` - The event system to register listeners with
    pub async fn setup_core_listeners(&self, event_system: std::sync::Arc<crate::system::EventSystem>) -> Result<(), crate::events::EventError> {
        use crate::events::PlayerMovementEvent;
        
        let coordinator = self.coordinator.clone();
        event_system
            .on_core("player_movement", move |event: PlayerMovementEvent| {
                let coordinator_clone = coordinator.clone();
                tokio::spawn(async move {
                    coordinator_clone.update_player_position(event.player_id, event.new_position).await;
                });
                Ok(())
            })
            .await?;
            
        Ok(())
    }
    
    /// Performs a quick health check.
    /// 
    /// # Returns
    /// 
    /// A lightweight health summary for monitoring purposes.
    pub async fn get_health_summary(&self) -> utils::GorcHealthSummary {
        utils::quick_health_check(self).await
    }
}

/// Comprehensive performance report for the GORC system.
/// 
/// This report provides detailed insights into system performance, health status,
/// and actionable recommendations for optimization.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GorcPerformanceReport {
    /// Report timestamp
    pub timestamp: u64,
    /// Total number of registered objects
    pub total_objects: usize,
    /// Total active subscriptions
    pub total_subscriptions: usize,
    /// Network utilization (0.0 to 1.0)
    pub network_utilization: f32,
    /// Total replication events sent
    pub events_sent: u64,
    /// Total bytes transmitted
    pub bytes_transmitted: u64,
    /// Number of updates dropped due to bandwidth limits
    pub updates_dropped: u64,
    /// Average batch size
    pub avg_batch_size: f32,
    /// System issues detected
    pub issues: Vec<String>,
}

impl GorcPerformanceReport {
    /// Checks if the system is performing well.
    /// 
    /// # Returns
    /// 
    /// `true` if the system is healthy, `false` if there are performance issues.
    pub fn is_healthy(&self) -> bool {
        self.issues.is_empty() && 
        self.network_utilization < 0.8 && 
        self.updates_dropped == 0
    }
    
    /// Gets a health score from 0.0 (poor) to 1.0 (excellent).
    /// 
    /// # Returns
    /// 
    /// A normalized health score based on multiple performance metrics.
    pub fn health_score(&self) -> f32 {
        let mut score = 1.0;
        
        // Penalize high network utilization
        if self.network_utilization > 0.5 {
            score -= (self.network_utilization - 0.5) * 0.5;
        }
        
        // Penalize dropped updates
        if self.updates_dropped > 0 {
            score -= 0.2;
        }
        
        // Penalize issues
        score -= (self.issues.len() as f32 * 0.1).min(0.5);
        
        score.max(0.0)
    }
    
    /// Gets performance recommendations.
    /// 
    /// # Returns
    /// 
    /// A vector of actionable recommendations for improving system performance.
    pub fn get_recommendations(&self) -> Vec<String> {
        let mut recommendations = Vec::new();
        
        if self.network_utilization > 0.8 {
            recommendations.push("Consider reducing update frequencies or enabling more aggressive compression".to_string());
        }
        
        if self.updates_dropped > 0 {
            recommendations.push("Updates are being dropped - increase bandwidth limits or reduce object count".to_string());
        }
        
        if self.total_objects > 5000 {
            recommendations.push("High object count - consider spatial partitioning or object pooling".to_string());
        }
        
        if self.avg_batch_size < 5.0 {
            recommendations.push("Low batch efficiency - consider increasing batch size limits".to_string());
        }
        
        recommendations
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gorc::{examples, MineralType};
    use crate::types::Vec3;
    
    #[tokio::test]
    async fn test_complete_gorc_system() {
        // Test that the basic GORC types compile and work
        let instance_manager = Arc::new(GorcInstanceManager::new());
        let stats = instance_manager.get_stats().await;
        assert_eq!(stats.total_objects, 0);
    }
    
    #[test]
    fn test_example_asteroid() {
        let asteroid = examples::ExampleAsteroid::new(
            Vec3::new(100.0, 0.0, 200.0),
            MineralType::Platinum
        );
        
        assert_eq!(asteroid.type_name(), "ExampleAsteroid");
        assert_eq!(asteroid.position(), Vec3::new(100.0, 0.0, 200.0));
        
        let layers = asteroid.get_layers();
        assert!(layers.len() >= 2);
        
        // Test serialization
        if let Some(layer) = layers.first() {
            let serialized = asteroid.serialize_for_layer(layer);
            assert!(serialized.is_ok());
        }
    }
    
    #[test]
    fn test_performance_report() {
        let report = GorcPerformanceReport {
            timestamp: 123456789,
            total_objects: 100,
            total_subscriptions: 500,
            network_utilization: 0.3,
            events_sent: 10000,
            bytes_transmitted: 1024 * 1024,
            updates_dropped: 0,
            avg_batch_size: 15.0,
            issues: Vec::new(),
        };
        
        assert!(report.is_healthy());
        assert!(report.health_score() > 0.8);
        assert!(report.get_recommendations().is_empty());
    }
    
    #[test]
    fn test_performance_report_with_issues() {
        let report = GorcPerformanceReport {
            timestamp: 123456789,
            total_objects: 100,
            total_subscriptions: 500,
            network_utilization: 0.9, // High utilization
            events_sent: 10000,
            bytes_transmitted: 1024 * 1024,
            updates_dropped: 5, // Some drops
            avg_batch_size: 15.0,
            issues: vec!["Test issue".to_string()],
        };
        
        assert!(!report.is_healthy());
        assert!(report.health_score() <= 0.5);
        assert!(!report.get_recommendations().is_empty());
    }
}