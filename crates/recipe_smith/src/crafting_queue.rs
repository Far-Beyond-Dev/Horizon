//! Crafting queue management system for RecipeSmith
//! 
//! Handles:
//! - Active crafting operation tracking
//! - Queue management per player
//! - Progress monitoring and updates
//! - Operation completion and cancellation
//! - Resource locking and validation
//! - Performance optimization for large numbers of operations

use crate::types::*;
use dashmap::DashMap;
use event_system::types::PlayerId;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

// ============================================================================
// Crafting Queue Manager Implementation
// ============================================================================

/// Manages all active crafting operations and player queues
pub struct CraftingQueueManager {
    /// All active operations by ID
    active_operations: DashMap<CraftingOperationId, Arc<RwLock<CraftingOperation>>>,
    /// Player-specific operation queues
    player_queues: DashMap<PlayerId, Arc<RwLock<PlayerQueue>>>,
    /// Global queue statistics
    global_stats: Arc<RwLock<QueueStats>>,
    /// Operation lookup by player (for quick player-based queries)
    player_operations: DashMap<PlayerId, Vec<CraftingOperationId>>,
    /// Completed operations cache (for history/statistics)
    completed_operations: Arc<RwLock<VecDeque<CompletedOperation>>>,
    /// Maximum completed operations to keep in memory
    max_completed_history: usize,
}

impl CraftingQueueManager {
    /// Create a new crafting queue manager
    pub fn new() -> Self {
        Self {
            active_operations: DashMap::new(),
            player_queues: DashMap::new(),
            global_stats: Arc::new(RwLock::new(QueueStats::default())),
            player_operations: DashMap::new(),
            completed_operations: Arc::new(RwLock::new(VecDeque::new())),
            max_completed_history: 10000, // Keep last 10k completed operations
        }
    }

    /// Add a new crafting operation to the queue
    pub async fn add_operation(&self, operation: CraftingOperation) -> Result<(), CraftingError> {
        let operation_id = operation.id;
        let player_id = operation.player_id;

        info!("➕ Adding crafting operation {} for player {}", operation_id, player_id);

        // Initialize player queue if it doesn't exist
        self.ensure_player_queue(player_id).await;

        // Check if player has reached maximum concurrent operations
        let can_start_immediately = {
            let queue = self.player_queues.get(&player_id).unwrap();
            let queue_guard = queue.read().await;
            queue_guard.active_operations.len() < queue_guard.max_concurrent_operations as usize
        };

        let wrapped_operation = Arc::new(RwLock::new(operation));

        if can_start_immediately {
            // Start the operation immediately
            {
                let mut op_guard = wrapped_operation.write().await;
                op_guard.status = CraftingStatus::InProgress;
                op_guard.started_at = SystemTime::now();
            }

            // Add to active operations
            self.active_operations.insert(operation_id, wrapped_operation.clone());

            // Update player queue
            {
                let queue = self.player_queues.get(&player_id).unwrap();
                let mut queue_guard = queue.write().await;
                queue_guard.active_operations.push(operation_id);
                queue_guard.last_activity = SystemTime::now();
                queue_guard.stats.total_operations_started += 1;
            }

            // Update player operations lookup
            self.player_operations
                .entry(player_id)
                .or_insert_with(Vec::new)
                .push(operation_id);

            info!("▶️ Started crafting operation {} immediately", operation_id);
        } else {
            // Queue the operation
            {
                let mut op_guard = wrapped_operation.write().await;
                op_guard.status = CraftingStatus::Queued;
            }

            // Add to player queue
            {
                let queue = self.player_queues.get(&player_id).unwrap();
                let mut queue_guard = queue.write().await;
                queue_guard.queued_operations.push_back(operation_id);
                queue_guard.last_activity = SystemTime::now();
            }

            // Store the queued operation
            self.active_operations.insert(operation_id, wrapped_operation);

            info!("⏳ Queued crafting operation {} (player at capacity)", operation_id);
        }

        // Update global statistics
        {
            let mut stats = self.global_stats.write().await;
            stats.total_operations_processed += 1;
            stats.active_operations_count = self.active_operations.len() as u64;
            stats.peak_concurrent_operations = stats.peak_concurrent_operations.max(stats.active_operations_count);
        }

        Ok(())
    }

    /// Get an active operation by ID
    pub async fn get_operation(&self, operation_id: &CraftingOperationId) -> Option<CraftingOperation> {
        if let Some(operation_ref) = self.active_operations.get(operation_id) {
            let operation_guard = operation_ref.read().await;
            Some(operation_guard.clone())
        } else {
            None
        }
    }

    /// Cancel a crafting operation
    pub async fn cancel_operation(&self, operation_id: &CraftingOperationId) -> Option<CraftingOperation> {
        info!("❌ Cancelling crafting operation {}", operation_id);

        if let Some((_, operation_ref)) = self.active_operations.remove(operation_id) {
            let operation = {
                let mut operation_guard = operation_ref.write().await;
                operation_guard.status = CraftingStatus::Cancelled;
                operation_guard.clone()
            };

            let player_id = operation.player_id;

            // Remove from player queue
            if let Some(queue_ref) = self.player_queues.get(&player_id) {
                let mut queue_guard = queue_ref.write().await;
                queue_guard.active_operations.retain(|&id| id != *operation_id);
                queue_guard.queued_operations.retain(|&id| id != *operation_id);
                queue_guard.last_activity = SystemTime::now();
                queue_guard.stats.total_operations_cancelled += 1;
            }

            // Remove from player operations lookup
            if let Some(mut player_ops) = self.player_operations.get_mut(&player_id) {
                player_ops.retain(|&id| id != *operation_id);
            }

            // Add to completed operations
            self.record_completed_operation(&operation, CraftingStatus::Cancelled).await;

            // Try to start next queued operation for this player
            self.try_start_next_operation(player_id).await;

            // Update global statistics
            {
                let mut stats = self.global_stats.write().await;
                stats.active_operations_count = self.active_operations.len() as u64;
            }

            Some(operation)
        } else {
            warn!("Attempted to cancel non-existent operation: {}", operation_id);
            None
        }
    }

    /// Complete a crafting operation
    pub async fn complete_operation(
        &self,
        operation_id: &CraftingOperationId,
        success: bool,
        quality_achieved: Option<f32>,
        experience_gained: HashMap<String, u32>,
    ) -> Option<CraftingOperation> {
        info!("✅ Completing crafting operation {} (success: {})", operation_id, success);

        if let Some((_, operation_ref)) = self.active_operations.remove(operation_id) {
            let operation = {
                let mut operation_guard = operation_ref.write().await;
                operation_guard.status = if success { CraftingStatus::Completed } else { CraftingStatus::Failed };
                operation_guard.progress = 1.0;
                operation_guard.clone()
            };

            let player_id = operation.player_id;

            // Update player queue statistics
            if let Some(queue_ref) = self.player_queues.get(&player_id) {
                let mut queue_guard = queue_ref.write().await;
                queue_guard.active_operations.retain(|&id| id != *operation_id);
                queue_guard.last_activity = SystemTime::now();

                let duration = SystemTime::now().duration_since(operation.started_at).unwrap_or_default();
                queue_guard.stats.total_time_crafting += duration;

                if success {
                    queue_guard.stats.total_operations_completed += 1;
                } else {
                    queue_guard.stats.total_operations_failed += 1;
                }

                // Update timing statistics
                if queue_guard.stats.total_operations_completed > 0 {
                    queue_guard.stats.average_operation_time = Duration::from_millis(
                        queue_guard.stats.total_time_crafting.as_millis() as u64 / queue_guard.stats.total_operations_completed
                    );
                }

                if duration > queue_guard.stats.longest_operation_time {
                    queue_guard.stats.longest_operation_time = duration;
                }

                if queue_guard.stats.shortest_operation_time.is_none() || duration < queue_guard.stats.shortest_operation_time.unwrap() {
                    queue_guard.stats.shortest_operation_time = Some(duration);
                }
            }

            // Remove from player operations lookup
            if let Some(mut player_ops) = self.player_operations.get_mut(&player_id) {
                player_ops.retain(|&id| id != *operation_id);
            }

            // Add to completed operations
            let final_status = if success { CraftingStatus::Completed } else { CraftingStatus::Failed };
            self.record_completed_operation(&operation, final_status).await;

            // Try to start next queued operation for this player
            self.try_start_next_operation(player_id).await;

            // Update global statistics
            {
                let mut stats = self.global_stats.write().await;
                stats.active_operations_count = self.active_operations.len() as u64;
            }

            Some(operation)
        } else {
            warn!("Attempted to complete non-existent operation: {}", operation_id);
            None
        }
    }

    /// Get all active operations
    pub async fn get_active_operations(&self) -> Vec<CraftingOperation> {
        let mut operations = Vec::new();
        
        for operation_ref in self.active_operations.iter() {
            let operation_guard = operation_ref.read().await;
            if operation_guard.status == CraftingStatus::InProgress {
                operations.push(operation_guard.clone());
            }
        }
        
        operations
    }

    /// Get all active operations (for shutdown)
    pub async fn get_all_active_operations(&self) -> Vec<CraftingOperation> {
        let mut operations = Vec::new();
        
        for operation_ref in self.active_operations.iter() {
            let operation_guard = operation_ref.read().await;
            operations.push(operation_guard.clone());
        }
        
        operations
    }

    /// Get operations for a specific player
    pub async fn get_player_operations(&self, player_id: &PlayerId) -> Vec<CraftingOperation> {
        let mut operations = Vec::new();

        if let Some(operation_ids) = self.player_operations.get(player_id) {
            for &operation_id in operation_ids.iter() {
                if let Some(operation_ref) = self.active_operations.get(&operation_id) {
                    let operation_guard = operation_ref.read().await;
                    operations.push(operation_guard.clone());
                }
            }
        }

        operations
    }

    /// Get player queue information
    pub async fn get_player_queue(&self, player_id: &PlayerId) -> Option<PlayerQueue> {
        if let Some(queue_ref) = self.player_queues.get(player_id) {
            let queue_guard = queue_ref.read().await;
            Some(queue_guard.clone())
        } else {
            None
        }
    }

    /// Update operation progress
    pub async fn update_operation_progress(
        &self,
        operation_id: &CraftingOperationId,
        progress: f32,
    ) -> Result<(), CraftingError> {
        if let Some(operation_ref) = self.active_operations.get(operation_id) {
            let mut operation_guard = operation_ref.write().await;
            operation_guard.progress = progress.clamp(0.0, 1.0);
            Ok(())
        } else {
            Err(CraftingError::OperationNotFound(*operation_id))
        }
    }

    /// Pause an operation (e.g., due to missing resources)
    pub async fn pause_operation(&self, operation_id: &CraftingOperationId) -> Result<(), CraftingError> {
        if let Some(operation_ref) = self.active_operations.get(operation_id) {
            let mut operation_guard = operation_ref.write().await;
            if operation_guard.status == CraftingStatus::InProgress {
                operation_guard.status = CraftingStatus::Paused;
                info!("⏸️ Paused crafting operation {}", operation_id);
                Ok(())
            } else {
                Err(CraftingError::ValidationFailed("Operation is not in progress".to_string()))
            }
        } else {
            Err(CraftingError::OperationNotFound(*operation_id))
        }
    }

    /// Resume a paused operation
    pub async fn resume_operation(&self, operation_id: &CraftingOperationId) -> Result<(), CraftingError> {
        if let Some(operation_ref) = self.active_operations.get(operation_id) {
            let mut operation_guard = operation_ref.write().await;
            if operation_guard.status == CraftingStatus::Paused {
                operation_guard.status = CraftingStatus::InProgress;
                info!("▶️ Resumed crafting operation {}", operation_id);
                Ok(())
            } else {
                Err(CraftingError::ValidationFailed("Operation is not paused".to_string()))
            }
        } else {
            Err(CraftingError::OperationNotFound(*operation_id))
        }
    }

    /// Get global queue statistics
    pub async fn get_global_stats(&self) -> QueueStats {
        let stats = self.global_stats.read().await;
        stats.clone()
    }

    /// Get completed operations history
    pub async fn get_completed_operations(&self, limit: usize) -> Vec<CompletedOperation> {
        let completed = self.completed_operations.read().await;
        completed.iter().rev().take(limit).cloned().collect()
    }

    /// Get completed operations for a specific player
    pub async fn get_player_completed_operations(
        &self,
        player_id: &PlayerId,
        limit: usize,
    ) -> Vec<CompletedOperation> {
        let completed = self.completed_operations.read().await;
        completed
            .iter()
            .rev()
            .filter(|op| op.player_id == *player_id)
            .take(limit)
            .cloned()
            .collect()
    }

    /// Clear completed operations history (admin function)
    pub async fn clear_completed_history(&self) {
        let mut completed = self.completed_operations.write().await;
        completed.clear();
        info!("🧹 Cleared completed operations history");
    }

    /// Get operation count by status
    pub async fn get_operation_counts_by_status(&self) -> HashMap<CraftingStatus, u64> {
        let mut counts = HashMap::new();

        for operation_ref in self.active_operations.iter() {
            let operation_guard = operation_ref.read().await;
            *counts.entry(operation_guard.status.clone()).or_insert(0) += 1;
        }

        counts
    }

    /// Remove player queue (called when player disconnects)
    pub async fn remove_player_queue(&self, player_id: &PlayerId) {
        // Cancel all active operations for this player
        if let Some(operation_ids) = self.player_operations.get(player_id) {
            let operation_ids_copy = operation_ids.clone();
            for operation_id in operation_ids_copy {
                self.cancel_operation(&operation_id).await;
            }
        }

        // Remove player data
        self.player_queues.remove(player_id);
        self.player_operations.remove(player_id);

        info!("🗑️ Removed player queue for {}", player_id);
    }

    /// Set player maximum concurrent operations
    pub async fn set_player_max_operations(&self, player_id: PlayerId, max_operations: u32) {
        self.ensure_player_queue(player_id).await;
        
        if let Some(queue_ref) = self.player_queues.get(&player_id) {
            let mut queue_guard = queue_ref.write().await;
            queue_guard.max_concurrent_operations = max_operations;
            info!("⚙️ Set max concurrent operations for player {} to {}", player_id, max_operations);
        }

        // Try to start queued operations if limit was increased
        self.try_start_next_operation(player_id).await;
    }

    // ========================================================================
    // Private Helper Methods
    // ========================================================================

    /// Ensure a player queue exists
    async fn ensure_player_queue(&self, player_id: PlayerId) {
        if !self.player_queues.contains_key(&player_id) {
            let now = SystemTime::now();
            let queue = PlayerQueue {
                player_id,
                active_operations: Vec::new(),
                queued_operations: VecDeque::new(),
                max_concurrent_operations: 3, // Default limit
                created_at: now,
                last_activity: now,
                stats: PlayerQueueStats::default(),
            };

            self.player_queues.insert(player_id, Arc::new(RwLock::new(queue)));
            self.player_operations.insert(player_id, Vec::new());
            debug!("Created new player queue for {}", player_id);
        }
    }

    /// Try to start the next queued operation for a player
    async fn try_start_next_operation(&self, player_id: PlayerId) {
        if let Some(queue_ref) = self.player_queues.get(&player_id) {
            let operation_to_start = {
                let mut queue_guard = queue_ref.write().await;
                
                // Check if we can start more operations
                if queue_guard.active_operations.len() >= queue_guard.max_concurrent_operations as usize {
                    return;
                }
                
                // Get next queued operation
                queue_guard.queued_operations.pop_front()
            };

            if let Some(operation_id) = operation_to_start {
                // Move operation from queued to active
                if let Some(operation_ref) = self.active_operations.get(&operation_id) {
                    {
                        let mut operation_guard = operation_ref.write().await;
                        operation_guard.status = CraftingStatus::InProgress;
                        operation_guard.started_at = SystemTime::now();
                    }

                    // Update player queue
                    {
                        let mut queue_guard = queue_ref.write().await;
                        queue_guard.active_operations.push(operation_id);
                        queue_guard.last_activity = SystemTime::now();
                        queue_guard.stats.total_operations_started += 1;
                    }

                    info!("▶️ Started queued operation {} for player {}", operation_id, player_id);
                }
            }
        }
    }

    /// Record a completed operation in history
    async fn record_completed_operation(&self, operation: &CraftingOperation, final_status: CraftingStatus) {
        let completed_op = CompletedOperation {
            operation_id: operation.id,
            player_id: operation.player_id,
            recipe_id: operation.recipe.id,
            status: final_status,
            started_at: operation.started_at,
            completed_at: SystemTime::now(),
            duration: SystemTime::now().duration_since(operation.started_at).unwrap_or_default(),
            quality_achieved: None, // Would be filled in by calling code
            experience_gained: HashMap::new(), // Would be filled in by calling code
        };

        let mut completed = self.completed_operations.write().await;
        completed.push_back(completed_op);

        // Trim history if it gets too large
        while completed.len() > self.max_completed_history {
            completed.pop_front();
        }
    }
}

// ============================================================================
// Queue Performance Monitoring
// ============================================================================

impl CraftingQueueManager {
    /// Update global performance statistics
    pub async fn update_performance_stats(&self) {
        let mut stats = self.global_stats.write().await;
        
        // Calculate current system load
        let active_count = self.active_operations.len() as f32;
        let total_capacity = self.player_queues.len() as f32 * 3.0; // Assuming average 3 concurrent per player
        
        stats.system_load_factor = if total_capacity > 0.0 {
            (active_count / total_capacity).min(1.0)
        } else {
            0.0
        };

        // Calculate average queue size
        let mut total_queue_size = 0;
        let mut player_count = 0;
        
        for queue_ref in self.player_queues.iter() {
            let queue_guard = queue_ref.read().await;
            total_queue_size += queue_guard.queued_operations.len();
            player_count += 1;
        }

        stats.average_queue_size = if player_count > 0 {
            total_queue_size as f64 / player_count as f64
        } else {
            0.0
        };

        stats.active_operations_count = active_count as u64;
    }

    /// Get performance metrics for monitoring
    pub async fn get_performance_metrics(&self) -> QueuePerformanceMetrics {
        let stats = self.global_stats.read().await;
        let operation_counts = self.get_operation_counts_by_status().await;

        QueuePerformanceMetrics {
            active_operations: stats.active_operations_count,
            queued_operations: operation_counts.get(&CraftingStatus::Queued).unwrap_or(&0).clone(),
            completed_operations: operation_counts.get(&CraftingStatus::Completed).unwrap_or(&0).clone(),
            failed_operations: operation_counts.get(&CraftingStatus::Failed).unwrap_or(&0).clone(),
            cancelled_operations: operation_counts.get(&CraftingStatus::Cancelled).unwrap_or(&0).clone(),
            paused_operations: operation_counts.get(&CraftingStatus::Paused).unwrap_or(&0).clone(),
            system_load_factor: stats.system_load_factor,
            average_queue_size: stats.average_queue_size,
            peak_concurrent_operations: stats.peak_concurrent_operations,
            total_players_with_queues: self.player_queues.len() as u64,
        }
    }
}

/// Performance metrics for monitoring and alerting
#[derive(Debug, Clone)]
pub struct QueuePerformanceMetrics {
    pub active_operations: u64,
    pub queued_operations: u64,
    pub completed_operations: u64,
    pub failed_operations: u64,
    pub cancelled_operations: u64,
    pub paused_operations: u64,
    pub system_load_factor: f32,
    pub average_queue_size: f64,
    pub peak_concurrent_operations: u64,
    pub total_players_with_queues: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn create_test_operation(player_id: PlayerId) -> CraftingOperation {
        CraftingOperation {
            id: CraftingOperationId::new(),
            player_id,
            recipe: create_test_recipe(),
            quantity: 1,
            ingredient_sources: vec![],
            started_at: SystemTime::now(),
            estimated_completion: SystemTime::now() + Duration::from_secs(10),
            progress: 0.0,
            status: CraftingStatus::Queued,
            quality_modifiers: vec![],
            experience_multiplier: 1.0,
        }
    }

    fn create_test_recipe() -> Recipe {
        use crate::types::*;
        use std::collections::HashMap;

        Recipe {
            id: RecipeId(1001),
            name: "Test Recipe".to_string(),
            description: "A test recipe".to_string(),
            category: "Test".to_string(),
            ingredients: vec![],
            main_product: CraftingResult {
                item_id: 1,
                quantity: 1,
                quality_bonus: None,
            },
            byproducts: vec![],
            requirements: CraftingRequirements {
                skill_requirements: HashMap::new(),
                station_requirement: None,
                tool_requirements: vec![],
                environmental_requirements: vec![],
            },
            timing: CraftingTiming {
                base_duration: Duration::from_secs(10),
                skill_speed_bonus: None,
                station_speed_bonus: None,
            },
            unlock_conditions: vec![],
            experience_rewards: HashMap::new(),
            failure_chance: 0.0,
            critical_success_chance: 0.0,
        }
    }

    #[tokio::test]
    async fn test_add_operation() {
        let manager = CraftingQueueManager::new();
        let player_id = PlayerId::new();
        let operation = create_test_operation(player_id);
        let operation_id = operation.id;

        manager.add_operation(operation).await.unwrap();

        let retrieved = manager.get_operation(&operation_id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().status, CraftingStatus::InProgress);
    }

    #[tokio::test]
    async fn test_cancel_operation() {
        let manager = CraftingQueueManager::new();
        let player_id = PlayerId::new();
        let operation = create_test_operation(player_id);
        let operation_id = operation.id;

        manager.add_operation(operation).await.unwrap();
        let cancelled = manager.cancel_operation(&operation_id).await;

        assert!(cancelled.is_some());
        assert_eq!(cancelled.unwrap().status, CraftingStatus::Cancelled);
        
        let retrieved = manager.get_operation(&operation_id).await;
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_complete_operation() {
        let manager = CraftingQueueManager::new();
        let player_id = PlayerId::new();
        let operation = create_test_operation(player_id);
        let operation_id = operation.id;

        manager.add_operation(operation).await.unwrap();
        let completed = manager.complete_operation(&operation_id, true, Some(1.0), HashMap::new()).await;

        assert!(completed.is_some());
        assert_eq!(completed.unwrap().status, CraftingStatus::Completed);
        
        let retrieved = manager.get_operation(&operation_id).await;
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_player_queue_limits() {
        let manager = CraftingQueueManager::new();
        let player_id = PlayerId::new();

        // Set low limit for testing
        manager.set_player_max_operations(player_id, 1).await;

        // Add first operation (should start immediately)
        let op1 = create_test_operation(player_id);
        let op1_id = op1.id;
        manager.add_operation(op1).await.unwrap();

        let retrieved1 = manager.get_operation(&op1_id).await;
        assert_eq!(retrieved1.unwrap().status, CraftingStatus::InProgress);

        // Add second operation (should be queued)
        let op2 = create_test_operation(player_id);
        let op2_id = op2.id;
        manager.add_operation(op2).await.unwrap();

        let retrieved2 = manager.get_operation(&op2_id).await;
        assert_eq!(retrieved2.unwrap().status, CraftingStatus::Queued);

        // Complete first operation
        manager.complete_operation(&op1_id, true, Some(1.0), HashMap::new()).await;

        // Second operation should now be in progress
        let retrieved2_after = manager.get_operation(&op2_id).await;
        assert_eq!(retrieved2_after.unwrap().status, CraftingStatus::InProgress);
    }

    #[tokio::test]
    async fn test_performance_metrics() {
        let manager = CraftingQueueManager::new();
        let player_id = PlayerId::new();
        let operation = create_test_operation(player_id);

        manager.add_operation(operation).await.unwrap();
        manager.update_performance_stats().await;

        let metrics = manager.get_performance_metrics().await;
        assert_eq!(metrics.active_operations, 1);
        assert_eq!(metrics.total_players_with_queues, 1);
    }
}