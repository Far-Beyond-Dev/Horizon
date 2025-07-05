//! # GORC Network Replication Engine
//!
//! This module handles the actual network transmission of replication data,
//! including batching, compression, prioritization, and delivery guarantees.

use crate::types::PlayerId;
use crate::gorc::instance::{GorcObjectId, GorcInstanceManager};
use crate::gorc::channels::{ReplicationPriority, CompressionType};
use crate::context::ServerContext;
use crate::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};

/// A single replication update for network transmission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationUpdate {
    /// Object being replicated
    pub object_id: GorcObjectId,
    /// Object type name
    pub object_type: String,
    /// Replication channel
    pub channel: u8,
    /// Serialized object data
    pub data: Vec<u8>,
    /// Update priority
    pub priority: ReplicationPriority,
    /// Update sequence number for ordering
    pub sequence: u32,
    /// Timestamp when update was created
    pub timestamp: u64,
    /// Compression used for the data
    pub compression: CompressionType,
}

/// Batch of replication updates for efficient transmission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationBatch {
    /// Batch identifier
    pub batch_id: u32,
    /// Updates in this batch
    pub updates: Vec<ReplicationUpdate>,
    /// Target player for this batch
    pub target_player: PlayerId,
    /// Batch priority (highest priority of contained updates)
    pub priority: ReplicationPriority,
    /// Total compressed size
    pub compressed_size: usize,
    /// Creation timestamp
    pub timestamp: u64,
}

/// Network transmission statistics
#[derive(Debug, Default, Clone)]
pub struct NetworkStats {
    /// Total updates sent
    pub updates_sent: u64,
    /// Total batches sent
    pub batches_sent: u64,
    /// Total bytes transmitted
    pub bytes_transmitted: u64,
    /// Updates dropped due to bandwidth limits
    pub updates_dropped: u64,
    /// Average batch size
    pub avg_batch_size: f32,
    /// Average compression ratio
    pub avg_compression_ratio: f32,
    /// Network utilization percentage
    pub network_utilization: f32,
}

/// Configuration for the network replication engine
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// Maximum bandwidth per player (bytes per second)
    pub max_bandwidth_per_player: u32,
    /// Maximum batch size in updates
    pub max_batch_size: usize,
    /// Maximum batch age before forced transmission
    pub max_batch_age_ms: u64,
    /// Target update frequency per channel
    pub target_frequencies: HashMap<u8, f32>,
    /// Enable compression
    pub compression_enabled: bool,
    /// Minimum compression threshold (don't compress smaller payloads)
    pub compression_threshold: usize,
    /// Priority queue sizes
    pub priority_queue_sizes: HashMap<ReplicationPriority, usize>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        let mut target_frequencies = HashMap::new();
        target_frequencies.insert(0, 60.0); // Critical - 60Hz
        target_frequencies.insert(1, 30.0); // Detailed - 30Hz  
        target_frequencies.insert(2, 15.0); // Cosmetic - 15Hz
        target_frequencies.insert(3, 5.0);  // Metadata - 5Hz

        let mut priority_queue_sizes = HashMap::new();
        priority_queue_sizes.insert(ReplicationPriority::Critical, 1000);
        priority_queue_sizes.insert(ReplicationPriority::High, 500);
        priority_queue_sizes.insert(ReplicationPriority::Normal, 250);
        priority_queue_sizes.insert(ReplicationPriority::Low, 100);

        Self {
            max_bandwidth_per_player: 1024 * 1024, // 1MB/s
            max_batch_size: 50,
            max_batch_age_ms: 16, // ~60 FPS
            target_frequencies,
            compression_enabled: true,
            compression_threshold: 64,
            priority_queue_sizes,
        }
    }
}

/// Priority queue for replication updates
#[derive(Debug)]
pub struct PriorityUpdateQueue {
    /// Queues for each priority level
    queues: HashMap<ReplicationPriority, VecDeque<ReplicationUpdate>>,
    /// Maximum size per priority queue
    max_sizes: HashMap<ReplicationPriority, usize>,
    /// Total updates in all queues
    total_updates: usize,
}

impl PriorityUpdateQueue {
    /// Creates a new priority queue
    pub fn new(max_sizes: HashMap<ReplicationPriority, usize>) -> Self {
        let mut queues = HashMap::new();
        queues.insert(ReplicationPriority::Critical, VecDeque::new());
        queues.insert(ReplicationPriority::High, VecDeque::new());
        queues.insert(ReplicationPriority::Normal, VecDeque::new());
        queues.insert(ReplicationPriority::Low, VecDeque::new());

        Self {
            queues,
            max_sizes,
            total_updates: 0,
        }
    }

    /// Adds an update to the appropriate priority queue
    pub fn push(&mut self, update: ReplicationUpdate) -> bool {
        let priority = update.priority;
        
        if let Some(queue) = self.queues.get_mut(&priority) {
            let max_size = self.max_sizes.get(&priority).copied().unwrap_or(100);
            
            if queue.len() >= max_size {
                // Queue full, drop oldest update
                queue.pop_front();
                self.total_updates = self.total_updates.saturating_sub(1);
            }
            
            queue.push_back(update);
            self.total_updates += 1;
            true
        } else {
            false
        }
    }

    /// Pops the highest priority update
    pub fn pop(&mut self) -> Option<ReplicationUpdate> {
        // Check priorities in order: Critical -> High -> Normal -> Low
        for priority in [ReplicationPriority::Critical, ReplicationPriority::High, 
                        ReplicationPriority::Normal, ReplicationPriority::Low] {
            if let Some(queue) = self.queues.get_mut(&priority) {
                if let Some(update) = queue.pop_front() {
                    self.total_updates = self.total_updates.saturating_sub(1);
                    return Some(update);
                }
            }
        }
        None
    }

    /// Peeks at the highest priority update without removing it
    pub fn peek(&self) -> Option<&ReplicationUpdate> {
        for priority in [ReplicationPriority::Critical, ReplicationPriority::High,
                        ReplicationPriority::Normal, ReplicationPriority::Low] {
            if let Some(queue) = self.queues.get(&priority) {
                if let Some(update) = queue.front() {
                    return Some(update);
                }
            }
        }
        None
    }

    /// Returns the total number of updates in all queues
    pub fn len(&self) -> usize {
        self.total_updates
    }

    /// Returns true if all queues are empty
    pub fn is_empty(&self) -> bool {
        self.total_updates == 0
    }

    /// Clears all queues
    pub fn clear(&mut self) {
        for queue in self.queues.values_mut() {
            queue.clear();
        }
        self.total_updates = 0;
    }

    /// Gets the number of updates in a specific priority queue
    pub fn priority_len(&self, priority: ReplicationPriority) -> usize {
        self.queues.get(&priority).map(|q| q.len()).unwrap_or(0)
    }

    /// Drains up to `count` updates from the highest priority queues
    pub fn drain(&mut self, count: usize) -> Vec<ReplicationUpdate> {
        let mut updates = Vec::with_capacity(count);
        
        for _ in 0..count {
            if let Some(update) = self.pop() {
                updates.push(update);
            } else {
                break;
            }
        }
        
        updates
    }
}

/// Per-player network state
#[derive(Debug)]
pub struct PlayerNetworkState {
    /// Player identifier
    pub player_id: PlayerId,
    /// Update queue for this player
    pub update_queue: PriorityUpdateQueue,
    /// Current bandwidth usage (bytes per second)
    pub bandwidth_usage: u32,
    /// Bandwidth usage history for smoothing
    pub bandwidth_history: VecDeque<(Instant, u32)>,
    /// Last batch sent timestamp per channel
    pub last_batch_times: HashMap<u8, Instant>,
    /// Sequence number for ordering
    pub sequence_counter: u32,
    /// Batch identifier counter
    pub batch_counter: u32,
    /// Network statistics for this player
    pub stats: NetworkStats,
    /// Current batch being assembled
    pub current_batch: Option<ReplicationBatch>,
}

impl PlayerNetworkState {
    /// Creates new network state for a player
    pub fn new(player_id: PlayerId, config: &NetworkConfig) -> Self {
        Self {
            player_id,
            update_queue: PriorityUpdateQueue::new(config.priority_queue_sizes.clone()),
            bandwidth_usage: 0,
            bandwidth_history: VecDeque::new(),
            last_batch_times: HashMap::new(),
            sequence_counter: 0,
            batch_counter: 0,
            stats: NetworkStats::default(),
            current_batch: None,
        }
    }

    /// Adds an update to this player's queue
    pub fn queue_update(&mut self, update: ReplicationUpdate) -> bool {
        self.update_queue.push(update)
    }

    /// Gets the next sequence number
    pub fn next_sequence(&mut self) -> u32 {
        self.sequence_counter += 1;
        self.sequence_counter
    }

    /// Gets the next batch ID
    pub fn next_batch_id(&mut self) -> u32 {
        self.batch_counter += 1;
        self.batch_counter
    }

    /// Updates bandwidth usage tracking
    pub fn update_bandwidth_usage(&mut self, bytes_sent: u32) {
        let now = Instant::now();
        self.bandwidth_history.push_back((now, bytes_sent));
        
        // Keep only last second of history
        while let Some((timestamp, _)) = self.bandwidth_history.front() {
            if now.duration_since(*timestamp) > Duration::from_secs(1) {
                self.bandwidth_history.pop_front();
            } else {
                break;
            }
        }
        
        // Calculate current bandwidth usage
        self.bandwidth_usage = self.bandwidth_history
            .iter()
            .map(|(_, bytes)| bytes)
            .sum();
    }

    /// Checks if player has bandwidth available
    pub fn has_bandwidth_available(&self, bytes: u32, max_bandwidth: u32) -> bool {
        self.bandwidth_usage + bytes <= max_bandwidth
    }

    /// Starts a new batch if none exists
    pub fn start_batch_if_needed(&mut self) {
        if self.current_batch.is_none() {
            self.current_batch = Some(ReplicationBatch {
                batch_id: self.next_batch_id(),
                updates: Vec::new(),
                target_player: self.player_id,
                priority: ReplicationPriority::Low,
                compressed_size: 0,
                timestamp: crate::utils::current_timestamp(),
            });
        }
    }

    /// Adds an update to the current batch
    pub fn add_to_batch(&mut self, update: ReplicationUpdate) -> bool {
        if let Some(ref mut batch) = self.current_batch {
            // Update batch priority to highest priority of contained updates
            if update.priority < batch.priority {
                batch.priority = update.priority;
            }
            
            batch.updates.push(update);
            true
        } else {
            false
        }
    }

    /// Finalizes and returns the current batch
    pub fn finalize_batch(&mut self) -> Option<ReplicationBatch> {
        self.current_batch.take()
    }

    /// Checks if current batch should be sent based on size or age
    pub fn should_send_batch(&self, config: &NetworkConfig) -> bool {
        if let Some(ref batch) = self.current_batch {
            // Send if batch is full
            if batch.updates.len() >= config.max_batch_size {
                return true;
            }
            
            // Send if batch is too old
            let age_ms = (crate::utils::current_timestamp() - batch.timestamp) * 1000;
            if age_ms >= config.max_batch_age_ms {
                return true;
            }
            
            // Send immediately for critical updates
            if batch.priority == ReplicationPriority::Critical {
                return true;
            }
        }
        
        false
    }
}

/// Main network replication engine
#[derive(Debug)]
pub struct NetworkReplicationEngine {
    /// Per-player network states
    player_states: Arc<RwLock<HashMap<PlayerId, PlayerNetworkState>>>,
    /// Network configuration
    config: NetworkConfig,
    /// Global network statistics
    global_stats: Arc<RwLock<NetworkStats>>,
    /// Reference to instance manager
    instance_manager: Arc<GorcInstanceManager>,
    /// Reference to server context for network operations
    server_context: Arc<dyn ServerContext>,
}

impl NetworkReplicationEngine {
    /// Creates a new network replication engine
    pub fn new(
        config: NetworkConfig,
        instance_manager: Arc<GorcInstanceManager>,
        server_context: Arc<dyn ServerContext>,
    ) -> Self {
        Self {
            player_states: Arc::new(RwLock::new(HashMap::new())),
            config,
            global_stats: Arc::new(RwLock::new(NetworkStats::default())),
            instance_manager,
            server_context,
        }
    }

    /// Adds a player to the network system
    pub async fn add_player(&self, player_id: PlayerId) {
        let mut player_states = self.player_states.write().await;
        player_states.insert(
            player_id,
            PlayerNetworkState::new(player_id, &self.config),
        );
        
        tracing::info!("📡 Added player {} to network replication", player_id);
    }

    /// Removes a player from the network system
    pub async fn remove_player(&self, player_id: PlayerId) {
        let mut player_states = self.player_states.write().await;
        player_states.remove(&player_id);
        
        tracing::info!("📡 Removed player {} from network replication", player_id);
    }

    /// Queues a replication update for transmission
    pub async fn queue_update(&self, target_players: Vec<PlayerId>, update: ReplicationUpdate) {
        let mut player_states = self.player_states.write().await;
        
        for player_id in target_players {
            if let Some(state) = player_states.get_mut(&player_id) {
                if !state.queue_update(update.clone()) {
                    tracing::warn!("Failed to queue update for player {}", player_id);
                }
            }
        }
    }

    /// Processes pending updates and sends batches
    pub async fn process_updates(&self) -> Result<(), NetworkError> {
        let mut player_states = self.player_states.write().await;
        let mut batches_to_send = Vec::new();
        
        for (player_id, state) in player_states.iter_mut() {
            // Process updates for this player
            self.process_player_updates(state, &mut batches_to_send).await?;
        }
        
        // Send all batches
        drop(player_states);
        for batch in batches_to_send {
            self.send_batch(batch).await?;
        }
        
        Ok(())
    }

    /// Processes updates for a single player
    async fn process_player_updates(
        &self,
        state: &mut PlayerNetworkState,
        batches_to_send: &mut Vec<ReplicationBatch>,
    ) -> Result<(), NetworkError> {
        // Check if we should send current batch
        if state.should_send_batch(&self.config) {
            if let Some(batch) = state.finalize_batch() {
                batches_to_send.push(batch);
            }
        }
        
        // Process updates from queue
        while !state.update_queue.is_empty() {
            // Check bandwidth limits
            let estimated_size = 256; // Rough estimate per update
            if !state.has_bandwidth_available(estimated_size, self.config.max_bandwidth_per_player) {
                break;
            }
            
            // Start a new batch if needed
            state.start_batch_if_needed();
            
            // Add updates to current batch
            let updates_to_add = std::cmp::min(
                self.config.max_batch_size,
                state.update_queue.len(),
            );
            
            for _ in 0..updates_to_add {
                if let Some(update) = state.update_queue.pop() {
                    if !state.add_to_batch(update) {
                        tracing::error!("Failed to add update to batch for player {}", state.player_id);
                        break;
                    }
                } else {
                    break;
                }
            }
            
            // Send batch if it's full or should be sent
            if state.should_send_batch(&self.config) {
                if let Some(batch) = state.finalize_batch() {
                    batches_to_send.push(batch);
                }
            }
        }
        
        Ok(())
    }

    /// Sends a batch to its target player
    async fn send_batch(&self, batch: ReplicationBatch) -> Result<(), NetworkError> {
        // Serialize and compress the batch
        let serialized = serde_json::to_vec(&batch)
            .map_err(|e| NetworkError::Serialization(e.to_string()))?;
        
        let final_data = if self.config.compression_enabled && 
                            serialized.len() > self.config.compression_threshold {
            self.compress_data(&serialized)?
        } else {
            serialized
        };
        
        // Send via server context
        if let Err(e) = self.server_context
            .send_to_player(batch.target_player, &final_data)
            .await
        {
            return Err(NetworkError::TransmissionFailed(e.to_string()));
        }
        
        // Update statistics
        self.update_stats(&batch, final_data.len()).await;
        
        tracing::debug!(
            "📤 Sent batch {} to player {} ({} updates, {} bytes)",
            batch.batch_id,
            batch.target_player,
            batch.updates.len(),
            final_data.len()
        );
        
        Ok(())
    }

    /// Compresses data using the configured compression algorithm
    fn compress_data(&self, data: &[u8]) -> Result<Vec<u8>, NetworkError> {
        // For now, just return the original data
        // In a real implementation, this would use LZ4, zlib, or other compression
        Ok(data.to_vec())
    }

    /// Updates network statistics
    async fn update_stats(&self, batch: &ReplicationBatch, bytes_sent: usize) {
        let mut global_stats = self.global_stats.write().await;
        global_stats.batches_sent += 1;
        global_stats.updates_sent += batch.updates.len() as u64;
        global_stats.bytes_transmitted += bytes_sent as u64;
        
        // Update averages
        if global_stats.batches_sent > 0 {
            global_stats.avg_batch_size = global_stats.updates_sent as f32 / global_stats.batches_sent as f32;
        }

        // Update player stats
        let mut player_states = self.player_states.write().await;
        if let Some(state) = player_states.get_mut(&batch.target_player) {
            state.stats.batches_sent += 1;
            state.stats.updates_sent += batch.updates.len() as u64;
            state.stats.bytes_transmitted += bytes_sent as u64;
            state.update_bandwidth_usage(bytes_sent as u32);
        }
    }

    /// Gets global network statistics
    pub async fn get_global_stats(&self) -> NetworkStats {
        self.global_stats.read().await.clone()
    }

    /// Gets statistics for a specific player
    pub async fn get_player_stats(&self, player_id: PlayerId) -> Option<NetworkStats> {
        let player_states = self.player_states.read().await;
        player_states.get(&player_id).map(|state| state.stats.clone())
    }

    /// Generates replication updates for all subscribed players
    pub async fn generate_updates_for_object(&self, object_id: GorcObjectId) -> Result<(), NetworkError> {
        // This would be called when an object needs to send updates
        // It would check all players subscribed to this object and generate appropriate updates
        
        // Get object instance (simplified - in real implementation this would be more efficient)
        if let Some(instance) = self.instance_manager.get_object(object_id).await {
            for (&channel, subscribers) in &instance.subscribers {
                if !subscribers.is_empty() && instance.needs_channel_update(channel) {
                    // Get the layer for this channel
                    let layers = instance.object.get_layers();
                    if let Some(layer) = layers.iter().find(|l| l.channel == channel) {
                        // Serialize data for this layer
                        match instance.object.serialize_for_layer(layer) {
                            Ok(data) => {
                                let update = ReplicationUpdate {
                                    object_id,
                                    object_type: instance.type_name.clone(),
                                    channel,
                                    data,
                                    priority: layer.priority,
                                    sequence: 0, // Will be set per-player
                                    timestamp: crate::utils::current_timestamp(),
                                    compression: layer.compression,
                                };

                                // Queue update for all subscribers
                                let subscriber_list: Vec<PlayerId> = subscribers.iter().copied().collect();
                                self.queue_update(subscriber_list, update).await;
                            }
                            Err(e) => {
                                tracing::error!("Failed to serialize object {} for channel {}: {}", 
                                              object_id, channel, e);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Forces immediate transmission of all pending batches
    pub async fn flush_all_batches(&self) -> Result<(), NetworkError> {
        let mut player_states = self.player_states.write().await;
        let mut batches_to_send = Vec::new();

        for state in player_states.values_mut() {
            if let Some(batch) = state.finalize_batch() {
                batches_to_send.push(batch);
            }
        }

        drop(player_states);
        
        for batch in batches_to_send {
            self.send_batch(batch).await?;
        }

        Ok(())
    }

    /// Updates network configuration
    pub fn update_config(&mut self, new_config: NetworkConfig) {
        self.config = new_config;
        tracing::info!("📡 Updated network replication configuration");
    }

    /// Gets current network utilization across all players
    pub async fn get_network_utilization(&self) -> f32 {
        let player_states = self.player_states.read().await;
        let total_players = player_states.len() as f32;
        
        if total_players == 0.0 {
            return 0.0;
        }

        let total_utilization: f32 = player_states
            .values()
            .map(|state| {
                let max_bandwidth = self.config.max_bandwidth_per_player as f32;
                (state.bandwidth_usage as f32 / max_bandwidth).min(1.0)
            })
            .sum();

        total_utilization / total_players
    }

    /// Performs network optimization based on current conditions
    pub async fn optimize_network_performance(&self) -> Result<(), NetworkError> {
        let utilization = self.get_network_utilization().await;
        
        if utilization > 0.8 {
            // High network utilization - apply optimizations
            tracing::warn!("High network utilization: {:.1}% - applying optimizations", utilization * 100.0);
            
            let mut player_states = self.player_states.write().await;
            for state in player_states.values_mut() {
                // Drop low priority updates if bandwidth is constrained
                if state.bandwidth_usage > (self.config.max_bandwidth_per_player as f32 * 0.9) as u32 {
                    let dropped = self.drop_low_priority_updates(state);
                    if dropped > 0 {
                        state.stats.updates_dropped += dropped;
                        tracing::debug!("Dropped {} low priority updates for player {}", 
                                      dropped, state.player_id);
                    }
                }
            }
        }

        Ok(())
    }

    /// Drops low priority updates from a player's queue
    fn drop_low_priority_updates(&self, state: &mut PlayerNetworkState) -> u64 {
        let initial_count = state.update_queue.len();
        
        // Clear low priority queue first
        if let Some(low_queue) = state.update_queue.queues.get_mut(&ReplicationPriority::Low) {
            let dropped = low_queue.len();
            low_queue.clear();
            state.update_queue.total_updates -= dropped;
            return dropped as u64;
        }

        // If still over bandwidth, clear normal priority
        if state.bandwidth_usage > self.config.max_bandwidth_per_player {
            if let Some(normal_queue) = state.update_queue.queues.get_mut(&ReplicationPriority::Normal) {
                let dropped = normal_queue.len();
                normal_queue.clear();
                state.update_queue.total_updates -= dropped;
                return dropped as u64;
            }
        }

        (initial_count - state.update_queue.len()) as u64
    }
}

/// Errors that can occur during network replication
#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    /// Serialization failed
    #[error("Serialization error: {0}")]
    Serialization(String),
    /// Compression failed
    #[error("Compression error: {0}")]
    Compression(String),
    /// Network transmission failed
    #[error("Transmission failed: {0}")]
    TransmissionFailed(String),
    /// Bandwidth limit exceeded
    #[error("Bandwidth limit exceeded for player")]
    BandwidthLimitExceeded,
    /// Invalid configuration
    #[error("Invalid network configuration: {0}")]
    InvalidConfiguration(String),
    /// Player not found
    #[error("Player not found: {0}")]
    PlayerNotFound(PlayerId),
}

/// Network replication coordinator that ties everything together
#[derive(Debug, Clone)]
pub struct ReplicationCoordinator {
    /// Network engine for transmission
    network_engine: Arc<NetworkReplicationEngine>,
    /// Instance manager for objects
    instance_manager: Arc<GorcInstanceManager>,
    /// Update scheduler
    update_scheduler: UpdateScheduler,
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
        }
    }

    /// Main replication tick - called regularly to process updates
    pub async fn tick(&mut self) -> Result<(), NetworkError> {
        // Generate updates for objects that need them
        let objects_needing_updates = self.update_scheduler.get_objects_needing_updates().await;
        
        for object_id in objects_needing_updates {
            self.network_engine.generate_updates_for_object(object_id).await?;
            self.update_scheduler.mark_object_updated(object_id).await;
        }

        // Process and send network updates
        self.network_engine.process_updates().await?;

        // Perform network optimization if needed
        self.network_engine.optimize_network_performance().await?;

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
        &self,
        object: T,
        position: Vec3,
    ) -> GorcObjectId {
        let object_id = self.instance_manager.register_object(object, position).await;
        self.update_scheduler.add_object(object_id).await;
        object_id
    }

    /// Unregisters an object from replication
    pub async fn unregister_object(&self, object_id: GorcObjectId) {
        self.instance_manager.unregister_object(object_id).await;
        self.update_scheduler.remove_object(object_id).await;
    }

    /// Gets comprehensive replication statistics
    pub async fn get_stats(&self) -> ReplicationStats {
        let network_stats = self.network_engine.get_global_stats().await;
        let instance_stats = self.instance_manager.get_stats().await;
        let scheduler_stats = self.update_scheduler.get_stats().await;

        ReplicationStats {
            network: network_stats,
            instances: instance_stats,
            scheduler: scheduler_stats,
        }
    }
}

/// Combined statistics for the entire replication system
#[derive(Debug, Clone)]
pub struct ReplicationStats {
    pub network: NetworkStats,
    pub instances: crate::gorc::instance::InstanceManagerStats,
    pub scheduler: SchedulerStats,
}

/// Simple update scheduler for determining when objects need updates
#[derive(Debug, Clone)]
pub struct UpdateScheduler {
    /// Objects and their last update times per channel
    object_updates: Arc<RwLock<HashMap<GorcObjectId, HashMap<u8, Instant>>>>,
    /// Target update frequencies per channel
    target_frequencies: HashMap<u8, f32>,
    /// Scheduler statistics
    stats: Arc<RwLock<SchedulerStats>>,
}

impl UpdateScheduler {
    /// Creates a new update scheduler
    pub fn new() -> Self {
        let mut target_frequencies = HashMap::new();
        target_frequencies.insert(0, 60.0); // Critical - 60Hz
        target_frequencies.insert(1, 30.0); // Detailed - 30Hz
        target_frequencies.insert(2, 15.0); // Cosmetic - 15Hz
        target_frequencies.insert(3, 5.0);  // Metadata - 5Hz

        Self {
            object_updates: Arc::new(RwLock::new(HashMap::new())),
            target_frequencies,
            stats: Arc::new(RwLock::new(SchedulerStats::default())),
        }
    }

    /// Adds an object to the scheduler
    pub async fn add_object(&self, object_id: GorcObjectId) {
        let mut object_updates = self.object_updates.write().await;
        object_updates.insert(object_id, HashMap::new());
    }

    /// Removes an object from the scheduler
    pub async fn remove_object(&self, object_id: GorcObjectId) {
        let mut object_updates = self.object_updates.write().await;
        object_updates.remove(&object_id);
    }

    /// Gets objects that need updates based on their frequencies
    pub async fn get_objects_needing_updates(&self) -> Vec<GorcObjectId> {
        let now = Instant::now();
        let object_updates = self.object_updates.read().await;
        let mut objects_needing_updates = Vec::new();

        for (&object_id, channel_updates) in object_updates.iter() {
            for (&channel, &frequency) in &self.target_frequencies {
                let update_interval = Duration::from_millis((1000.0 / frequency) as u64);
                
                let needs_update = if let Some(&last_update) = channel_updates.get(&channel) {
                    now.duration_since(last_update) >= update_interval
                } else {
                    true // Never updated, needs initial update
                };

                if needs_update {
                    objects_needing_updates.push(object_id);
                    break; // Only add object once even if multiple channels need updates
                }
            }
        }

        let mut stats = self.stats.write().await;
        stats.update_checks += 1;
        stats.objects_needing_updates = objects_needing_updates.len();

        objects_needing_updates
    }

    /// Marks an object as updated
    pub async fn mark_object_updated(&self, object_id: GorcObjectId) {
        let now = Instant::now();
        let mut object_updates = self.object_updates.write().await;
        
        if let Some(channel_updates) = object_updates.get_mut(&object_id) {
            for &channel in self.target_frequencies.keys() {
                channel_updates.insert(channel, now);
            }
        }

        let mut stats = self.stats.write().await;
        stats.objects_updated += 1;
    }

    /// Gets scheduler statistics
    pub async fn get_stats(&self) -> SchedulerStats {
        self.stats.read().await.clone()
    }
}

/// Statistics for the update scheduler
#[derive(Debug, Default, Clone)]
pub struct SchedulerStats {
    /// Number of update checks performed
    pub update_checks: u64,
    /// Number of objects that needed updates in last check
    pub objects_needing_updates: usize,
    /// Total objects updated
    pub objects_updated: u64,
    /// Average objects needing updates per check
    pub avg_objects_needing_updates: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Vec3;

    #[test]
    fn test_priority_queue() {
        let mut queue_sizes = HashMap::new();
        queue_sizes.insert(ReplicationPriority::Critical, 10);
        queue_sizes.insert(ReplicationPriority::High, 5);
        queue_sizes.insert(ReplicationPriority::Normal, 5);
        queue_sizes.insert(ReplicationPriority::Low, 5);

        let mut queue = PriorityUpdateQueue::new(queue_sizes);

        let critical_update = ReplicationUpdate {
            object_id: GorcObjectId::new(),
            object_type: "Test".to_string(),
            channel: 0,
            data: vec![1, 2, 3],
            priority: ReplicationPriority::Critical,
            sequence: 1,
            timestamp: 12345,
            compression: CompressionType::None,
        };

        let normal_update = ReplicationUpdate {
            object_id: GorcObjectId::new(),
            object_type: "Test".to_string(),
            channel: 1,
            data: vec![4, 5, 6],
            priority: ReplicationPriority::Normal,
            sequence: 2,
            timestamp: 12346,
            compression: CompressionType::None,
        };

        // Add updates
        assert!(queue.push(normal_update));
        assert!(queue.push(critical_update));

        // Critical should come first
        let first = queue.pop().unwrap();
        assert_eq!(first.priority, ReplicationPriority::Critical);

        let second = queue.pop().unwrap();
        assert_eq!(second.priority, ReplicationPriority::Normal);

        assert!(queue.is_empty());
    }

    #[test]
    fn test_network_config_defaults() {
        let config = NetworkConfig::default();
        assert_eq!(config.max_bandwidth_per_player, 1024 * 1024);
        assert_eq!(config.max_batch_size, 50);
        assert!(config.compression_enabled);
    }
}