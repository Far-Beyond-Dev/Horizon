/// Network replication engine implementation
use super::types::{NetworkConfig, NetworkStats, NetworkError, ReplicationBatch, ReplicationUpdate};
use super::queue::PlayerNetworkState;
use crate::types::PlayerId;
use crate::gorc::instance::GorcInstanceManager;
use crate::context::ServerContext;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};
use tracing::{info, warn, error};
use flate2::{Compression, write::DeflateEncoder, read::DeflateDecoder};
use std::io::prelude::*;

/// Core network replication engine that manages update distribution
#[derive(Debug, Clone)]
pub struct NetworkReplicationEngine {
    /// Per-player network states
    player_states: Arc<RwLock<HashMap<PlayerId, PlayerNetworkState>>>,
    /// Network configuration with interior mutability for runtime updates
    config: Arc<RwLock<NetworkConfig>>,
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
            config: Arc::new(RwLock::new(config)),
            global_stats: Arc::new(RwLock::new(NetworkStats::default())),
            instance_manager,
            server_context,
        }
    }

    /// Adds a player to the network system
    pub async fn add_player(&self, player_id: PlayerId) {
        let config = self.config.read().await;
        let priority_queue_sizes = config.priority_queue_sizes.clone();
        drop(config); // Release the lock early
        
        let mut player_states = self.player_states.write().await;
        player_states.insert(
            player_id,
            PlayerNetworkState::new(player_id, priority_queue_sizes),
        );
        
        info!("📡 Added player {} to network replication", player_id);
    }

    /// Removes a player from the network system
    pub async fn remove_player(&self, player_id: PlayerId) {
        let mut player_states = self.player_states.write().await;
        player_states.remove(&player_id);
        
        info!("📡 Removed player {} from network replication", player_id);
    }

    /// Queues a replication update for transmission
    pub async fn queue_update(&self, target_players: Vec<PlayerId>, update: ReplicationUpdate) {
        let mut player_states = self.player_states.write().await;
        
        for player_id in target_players {
            if let Some(state) = player_states.get_mut(&player_id) {
                if let Err(e) = state.queue_update(update.clone()) {
                    warn!("Failed to queue update for player {}: {}", player_id, e);
                }
            }
        }
    }

    /// Processes pending updates and sends batches
    pub async fn process_updates(&self) -> Result<(), NetworkError> {
        let mut player_states = self.player_states.write().await;
        let mut batches_to_send = Vec::new();
        
        for (_player_id, state) in player_states.iter_mut() {
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
        let config = self.config.read().await;
        let max_batch_size = config.max_batch_size;
        let max_batch_age_ms = config.max_batch_age_ms;
        let max_bandwidth_per_player = config.max_bandwidth_per_player;
        drop(config); // Release the lock early
        
        // Check if we should send current batch
        if state.should_send_batch(max_batch_size, max_batch_age_ms) {
            if let Some(updates) = state.finish_batch() {
                if !updates.is_empty() {
                    let batch = self.create_batch(state.player_id, updates)?;
                    batches_to_send.push(batch);
                }
            }
        }
        
        // Start new batch if needed
        if state.current_batch.is_none() && !state.update_queue.is_empty() {
            state.start_batch();
        }
        
        // Process updates from queue
        while !state.update_queue.is_empty() {
            // Check bandwidth limits
            let estimated_size = 256; // Rough estimate per update
            if !state.has_bandwidth(estimated_size, max_bandwidth_per_player) {
                break;
            }
            
            if let Some(update) = state.update_queue.pop() {
                if !state.add_to_batch(update) {
                    // Batch is full or doesn't exist, start a new one
                    if let Some(updates) = state.finish_batch() {
                        if !updates.is_empty() {
                            let batch = self.create_batch(state.player_id, updates)?;
                            batches_to_send.push(batch);
                        }
                    }
                    
                    state.start_batch();
                    // Try to add the update to the new batch
                    if let Some(update) = state.update_queue.pop() {
                        state.add_to_batch(update);
                    }
                    break;
                }
            }
        }
        
        Ok(())
    }

    /// Creates a replication batch from updates
    fn create_batch(&self, player_id: PlayerId, updates: Vec<ReplicationUpdate>) -> Result<ReplicationBatch, NetworkError> {
        if updates.is_empty() {
            return Err(NetworkError::InvalidConfiguration("Cannot create empty batch".to_string()));
        }

        // Find the highest priority in the batch
        let priority = updates.iter()
            .map(|u| u.priority)
            .max()
            .unwrap_or(crate::gorc::channels::ReplicationPriority::Normal);

        // Estimate compressed size (simplified)
        let estimated_size = updates.iter()
            .map(|u| u.data.len())
            .sum::<usize>();

        let batch_id = crate::utils::current_timestamp() as u32;
        
        Ok(ReplicationBatch {
            batch_id,
            updates,
            target_player: player_id,
            priority,
            compressed_size: estimated_size,
            timestamp: crate::utils::current_timestamp(),
        })
    }

    /// Sends a batch to the target player
    async fn send_batch(&self, batch: ReplicationBatch) -> Result<(), NetworkError> {
        // Serialize the batch
        let data = serde_json::to_vec(&batch)
            .map_err(|e| NetworkError::SerializationError(e.to_string()))?;

        let config = self.config.read().await;
        let compression_enabled = config.compression_enabled;
        let compression_threshold = config.compression_threshold;
        drop(config); // Release the lock early

        // Apply compression if enabled and worthwhile
        let final_data = if compression_enabled && data.len() > compression_threshold {
            self.compress_data(&data)?
        } else {
            data
        };

        // Send to player via server context
        if let Err(e) = self.server_context.send_to_player(batch.target_player, &final_data).await {
            return Err(NetworkError::TransmissionError(e.to_string()));
        }

        // Update statistics
        self.update_stats(&batch, final_data.len()).await;

        Ok(())
    }

    /// Compresses data using deflate compression algorithm
    fn compress_data(&self, data: &[u8]) -> Result<Vec<u8>, NetworkError> {
        // Only compress if data is larger than threshold to avoid overhead
        const COMPRESSION_THRESHOLD: usize = 64;
        
        if data.len() < COMPRESSION_THRESHOLD {
            // For small data, compression overhead isn't worth it
            return Ok(data.to_vec());
        }
        
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(data)
            .map_err(|e| NetworkError::SerializationError(format!("Compression failed: {}", e)))?;
        
        let compressed = encoder.finish()
            .map_err(|e| NetworkError::SerializationError(format!("Compression finalization failed: {}", e)))?;
        
        // Only use compressed data if it's actually smaller
        if compressed.len() < data.len() {
            Ok(compressed)
        } else {
            Ok(data.to_vec())
        }
    }

    /// Decompresses data using deflate decompression algorithm.
    /// 
    /// This method complements the `compress_data` method and is intended for scenarios
    /// where decompression of previously compressed data is required. While it is not
    /// currently used in the codebase, it may be useful for debugging, data integrity
    /// checks, or future features that involve receiving compressed data from external
    /// sources.
    /// 
    /// The `#[allow(dead_code)]` attribute is used to suppress warnings about unused code,
    /// as this method is retained for potential future use.
    #[allow(dead_code)]
    fn decompress_data(&self, compressed_data: &[u8]) -> Result<Vec<u8>, NetworkError> {
        let mut decoder = DeflateDecoder::new(compressed_data);
        let mut decompressed = Vec::new();
        
        decoder.read_to_end(&mut decompressed)
            .map_err(|e| NetworkError::SerializationError(format!("Decompression failed: {}", e)))?;
        
        Ok(decompressed)
    }

    /// Updates global statistics
    async fn update_stats(&self, batch: &ReplicationBatch, bytes_sent: usize) {
        let mut stats = self.global_stats.write().await;
        stats.batches_sent += 1;
        stats.updates_sent += batch.updates.len() as u64;
        stats.bytes_transmitted += bytes_sent as u64;
        
        // Update average batch size
        let total_batches = stats.batches_sent as f32;
        stats.avg_batch_size = ((stats.avg_batch_size * (total_batches - 1.0)) + batch.updates.len() as f32) / total_batches;
        
        // Update compression ratio (simplified)
        if bytes_sent > 0 {
            let original_size = batch.updates.iter().map(|u| u.data.len()).sum::<usize>();
            let compression_ratio = bytes_sent as f32 / original_size as f32;
            stats.avg_compression_ratio = ((stats.avg_compression_ratio * (total_batches - 1.0)) + compression_ratio) / total_batches;
        }
    }

    /// Gets current network statistics
    pub async fn get_stats(&self) -> NetworkStats {
        self.global_stats.read().await.clone()
    }

    /// Gets the number of active players
    pub async fn get_active_player_count(&self) -> usize {
        self.player_states.read().await.len()
    }

    /// Flushes all pending updates for a player
    pub async fn flush_player(&self, player_id: PlayerId) -> Result<(), NetworkError> {
        let mut player_states = self.player_states.write().await;
        
        if let Some(state) = player_states.get_mut(&player_id) {
            let mut batches_to_send = Vec::new();
            self.process_player_updates(state, &mut batches_to_send).await?;
            
            // Force send current batch if it exists
            if let Some(updates) = state.finish_batch() {
                if !updates.is_empty() {
                    let batch = self.create_batch(player_id, updates)?;
                    batches_to_send.push(batch);
                }
            }
            
            drop(player_states);
            
            for batch in batches_to_send {
                self.send_batch(batch).await?;
            }
        }
        
        Ok(())
    }

    /// Updates the network configuration dynamically
    /// 
    /// This allows for runtime configuration changes to optimize performance
    /// based on current network conditions and system load.
    /// 
    /// # Arguments
    /// 
    /// * `new_config` - The new network configuration to apply
    /// 
    /// # Returns
    /// 
    /// A result indicating success or failure of the configuration update.
    pub async fn update_config(&self, new_config: NetworkConfig) -> Result<(), NetworkError> {
        info!("Updating network engine configuration");
        {
            let mut config = self.config.write().await;
            *config = new_config;
        }
        
        // Update global statistics to reflect configuration change
        {
            let mut stats = self.global_stats.write().await;
            stats.config_updates += 1;
        }
        
        info!("Network configuration updated successfully");
        Ok(())
    }

    /// Gets a clone of the current network configuration
    pub async fn get_config(&self) -> NetworkConfig {
        self.config.read().await.clone()
    }
}