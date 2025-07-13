/// Network replication data types and structures
use crate::types::PlayerId;
use crate::gorc::instance::GorcObjectId;
use crate::gorc::channels::{ReplicationPriority, CompressionType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
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
            max_bandwidth_per_player: 1024 * 1024, // 1MB/s default
            max_batch_size: 50,
            max_batch_age_ms: 16, // ~60 FPS
            target_frequencies,
            compression_enabled: true,
            compression_threshold: 128, // Don't compress < 128 bytes
            priority_queue_sizes,
        }
    }
}

/// Network error types
#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("Serialization failed: {0}")]
    SerializationError(String),

    #[error("Compression failed: {0}")]
    CompressionError(String),

    #[error("Network transmission failed: {0}")]
    TransmissionError(String),

    #[error("Bandwidth limit exceeded for player {player_id}")]
    BandwidthExceeded { player_id: PlayerId },

    #[error("Queue capacity exceeded for priority {priority:?}")]
    QueueCapacityExceeded { priority: ReplicationPriority },

    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),
}

/// Replication statistics for monitoring
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ReplicationStats {
    pub network_stats: NetworkStats,
    pub queue_sizes: HashMap<ReplicationPriority, usize>,
    pub active_players: usize,
    pub updates_per_second: f32,
}