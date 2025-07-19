/// Replication coordination and scheduling
use super::types::{NetworkError, ReplicationStats, ReplicationUpdate};
use crate::gorc::channels::{ReplicationPriority, CompressionType, ReplicationLayer};
use super::engine::NetworkReplicationEngine;
use crate::types::PlayerId;
use crate::gorc::instance::{GorcObjectId, GorcInstanceManager};
use crate::Vec3;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::time::{Duration, Instant};
use serde::{Deserialize, Serialize};

/// High-level coordinator that manages the entire replication system
#[derive(Debug, Clone)]
pub struct ReplicationCoordinator {
    /// Network engine for transmission
    network_engine: Arc<NetworkReplicationEngine>,
    /// Instance manager for objects
    instance_manager: Arc<GorcInstanceManager>,
    /// Update scheduler
    update_scheduler: UpdateScheduler,
    /// Sequence counter for updates
    sequence_counter: u32,
}

impl ReplicationCoordinator {
    /// Creates a new replication coordinator
    pub fn new(
        network_engine: Arc<NetworkReplicationEngine>,
        instance_manager: Arc<GorcInstanceManager>,
    ) -> Self {
        Self {
            network_engine,
            instance_manager,
            update_scheduler: UpdateScheduler::new(),
            sequence_counter: 0,
        }
    }

    /// Main replication tick - called regularly to process updates
    pub async fn tick(&mut self) -> Result<(), NetworkError> {
        // Generate updates for objects that need them
        let objects_needing_updates = self.update_scheduler.get_objects_needing_updates().await;
        
        for object_id in objects_needing_updates {
            // Get the object instance from the instance manager
            if let Some(object_instance) = self.instance_manager.get_object(object_id).await {
                // Serialize the object data for the core replication layer
                let core_layer = ReplicationLayer {
                    channel: 0,
                    radius: 1000.0, // Default large radius
                    frequency: 30.0, // 30 Hz
                    properties: vec![], // Use all properties
                    compression: CompressionType::None,
                    priority: ReplicationPriority::Normal,
                };
                let serialized_data = match object_instance.object.serialize_for_layer(&core_layer) {
                    Ok(data) => data,
                    Err(_) => {
                        // Skip objects that can't be serialized
                        self.update_scheduler.mark_object_updated(object_id).await;
                        continue;
                    }
                };
                
                // Create replication update
                let update = ReplicationUpdate {
                    object_id,
                    object_type: object_instance.type_name.clone(),
                    channel: 0, // Default to channel 0
                    data: serialized_data,
                    priority: ReplicationPriority::Normal,
                    sequence: {
                        self.sequence_counter += 1;
                        self.sequence_counter
                    },
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                    compression: CompressionType::None,
                };
                
                // Get all players subscribed to the default channel (0)
                let target_players: Vec<PlayerId> = object_instance.subscribers
                    .get(&0)
                    .map(|set| set.iter().copied().collect())
                    .unwrap_or_default();
                
                // Queue the update in the network engine
                self.network_engine.queue_update(target_players, update).await;
            }
            
            // Mark the object as updated regardless of whether we found data
            self.update_scheduler.mark_object_updated(object_id).await;
        }

        // Process and send network updates
        self.network_engine.process_updates().await?;

        Ok(())
    }

    /// Adds a player to the replication system
    pub async fn add_player(&self, player_id: PlayerId, position: Vec3) {
        self.network_engine.add_player(player_id).await;
        self.instance_manager.update_player_position(player_id, position).await;
    }

    /// Removes a player from the replication system
    pub async fn remove_player(&self, player_id: PlayerId) {
        self.network_engine.remove_player(player_id).await;
        self.instance_manager.remove_player(player_id).await;
    }

    /// Updates a player's position
    pub async fn update_player_position(&self, player_id: PlayerId, position: Vec3) {
        self.instance_manager.update_player_position(player_id, position).await;
    }

    /// Registers an object for replication
    pub async fn register_object<T: crate::gorc::instance::GorcObject + 'static>(
        &mut self,
        object: T,
        position: Vec3,
    ) -> GorcObjectId {
        let object_id = self.instance_manager.register_object(object, position).await;
        self.update_scheduler.add_object(object_id).await;
        object_id
    }

    /// Unregisters an object from replication
    pub async fn unregister_object(&mut self, object_id: GorcObjectId) {
        self.instance_manager.unregister_object(object_id).await;
        self.update_scheduler.remove_object(object_id).await;
    }

    /// Gets comprehensive replication statistics
    pub async fn get_stats(&self) -> ReplicationStats {
        let network_stats = self.network_engine.get_stats().await;
        let scheduler_stats = self.update_scheduler.get_stats().await;

        ReplicationStats {
            network_stats,
            queue_sizes: HashMap::new(), // Would be populated from actual queue states
            active_players: self.network_engine.get_active_player_count().await,
            updates_per_second: scheduler_stats.updates_per_second,
        }
    }
}

/// Simple update scheduler for determining when objects need updates
#[derive(Debug, Clone)]
pub struct UpdateScheduler {
    /// Objects and their last update times
    object_update_times: HashMap<GorcObjectId, Instant>,
    /// Objects that have been modified and need updates
    dirty_objects: HashSet<GorcObjectId>,
    /// Scheduler statistics
    stats: SchedulerStats,
}

impl UpdateScheduler {
    /// Creates a new update scheduler
    pub fn new() -> Self {
        Self {
            object_update_times: HashMap::new(),
            dirty_objects: HashSet::new(),
            stats: SchedulerStats::default(),
        }
    }

    /// Adds an object to the scheduler
    pub async fn add_object(&mut self, object_id: GorcObjectId) {
        self.object_update_times.insert(object_id, Instant::now());
        self.dirty_objects.insert(object_id);
    }

    /// Removes an object from the scheduler
    pub async fn remove_object(&mut self, object_id: GorcObjectId) {
        self.object_update_times.remove(&object_id);
        self.dirty_objects.remove(&object_id);
    }

    /// Marks an object as needing an update
    pub async fn mark_object_dirty(&mut self, object_id: GorcObjectId) {
        self.dirty_objects.insert(object_id);
    }

    /// Marks an object as updated
    pub async fn mark_object_updated(&mut self, object_id: GorcObjectId) {
        self.object_update_times.insert(object_id, Instant::now());
        self.dirty_objects.remove(&object_id);
        self.stats.objects_updated += 1;
    }

    /// Gets objects that need updates based on time and dirty state
    pub async fn get_objects_needing_updates(&self) -> Vec<GorcObjectId> {
        let now = Instant::now();
        let mut objects_needing_updates = Vec::new();

        // Always include dirty objects
        objects_needing_updates.extend(self.dirty_objects.iter().copied());

        // Check for objects that haven't been updated in a while
        for (object_id, last_update) in &self.object_update_times {
            if now.duration_since(*last_update) > Duration::from_millis(33) { // ~30 FPS minimum
                if !self.dirty_objects.contains(object_id) {
                    objects_needing_updates.push(*object_id);
                }
            }
        }

        objects_needing_updates
    }

    /// Gets scheduler statistics
    pub async fn get_stats(&self) -> SchedulerStats {
        let mut stats = self.stats.clone();
        stats.objects_tracked = self.object_update_times.len();
        stats.dirty_objects = self.dirty_objects.len();
        
        // Calculate updates per second (simplified)
        stats.updates_per_second = stats.objects_updated as f32 / 60.0; // Assuming 60 second window
        
        stats
    }
}

/// Statistics for the update scheduler
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SchedulerStats {
    /// Number of objects being tracked
    pub objects_tracked: usize,
    /// Number of objects marked as dirty
    pub dirty_objects: usize,
    /// Total objects updated since start
    pub objects_updated: u64,
    /// Current updates per second rate
    pub updates_per_second: f32,
    /// Average time between updates per object
    pub avg_update_interval_ms: f32,
}