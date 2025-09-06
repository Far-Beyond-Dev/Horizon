/// Core types for multicast system
use crate::types::PlayerId;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// Unique identifier for multicast groups
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MulticastGroupId(pub u64);

impl MulticastGroupId {
    /// Creates a new multicast group ID
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

/// Level of Detail settings for multicast rooms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LodLevel {
    /// Highest detail - full replication
    Ultra = 0,
    /// High detail - most properties replicated
    High = 1,
    /// Medium detail - important properties only
    Medium = 2,
    /// Low detail - basic properties only
    Low = 3,
    /// Minimal detail - essential properties only
    Minimal = 4,
}

impl LodLevel {
    /// Gets the replication radius for this LOD level
    pub fn radius(&self) -> f64 {
        match self {
            LodLevel::Ultra => 50.0,
            LodLevel::High => 150.0,
            LodLevel::Medium => 300.0,
            LodLevel::Low => 600.0,
            LodLevel::Minimal => 1200.0,
        }
    }

    /// Gets the update frequency for this LOD level
    pub fn frequency(&self) -> f64 {
        match self {
            LodLevel::Ultra => 60.0,
            LodLevel::High => 30.0,
            LodLevel::Medium => 15.0,
            LodLevel::Low => 10.0,
            LodLevel::Minimal => 2.0,
        }
    }

    /// Gets the priority for this LOD level
    pub fn priority(&self) -> crate::gorc::channels::ReplicationPriority {
        match self {
            LodLevel::Ultra => crate::gorc::channels::ReplicationPriority::Critical,
            LodLevel::High => crate::gorc::channels::ReplicationPriority::High,
            LodLevel::Medium => crate::gorc::channels::ReplicationPriority::Normal,
            LodLevel::Low => crate::gorc::channels::ReplicationPriority::Low,
            LodLevel::Minimal => crate::gorc::channels::ReplicationPriority::Low,
        }
    }

    /// Gets the next higher LOD level
    pub fn upgrade(&self) -> Option<LodLevel> {
        match self {
            LodLevel::Minimal => Some(LodLevel::Low),
            LodLevel::Low => Some(LodLevel::Medium),
            LodLevel::Medium => Some(LodLevel::High),
            LodLevel::High => Some(LodLevel::Ultra),
            LodLevel::Ultra => None,
        }
    }

    /// Gets the next lower LOD level
    pub fn downgrade(&self) -> Option<LodLevel> {
        match self {
            LodLevel::Ultra => Some(LodLevel::High),
            LodLevel::High => Some(LodLevel::Medium),
            LodLevel::Medium => Some(LodLevel::Low),
            LodLevel::Low => Some(LodLevel::Minimal),
            LodLevel::Minimal => None,
        }
    }
}

/// Error types for multicast operations
#[derive(Debug, thiserror::Error)]
pub enum MulticastError {
    #[error("Group not found: {id:?}")]
    GroupNotFound { id: MulticastGroupId },
    
    #[error("Player not found: {player_id}")]
    PlayerNotFound { player_id: PlayerId },
    
    #[error("Invalid LOD level")]
    InvalidLodLevel,
    
    #[error("Group capacity exceeded")]
    GroupCapacityExceeded,
    
    #[error("Configuration error: {0}")]
    Configuration(String),
    
    #[error("Network error: {0}")]
    Network(String),
}

/// Statistics for multicast operations
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MulticastStats {
    /// Total number of multicast groups
    pub total_groups: usize,
    /// Total number of players across all groups
    pub total_players: usize,
    /// Average group size
    pub avg_group_size: f64,
    /// Total messages multicast
    pub messages_sent: u64,
    /// Total bytes multicast
    pub bytes_sent: u64,
    /// Groups created since start
    pub groups_created: u64,
    /// Groups destroyed since start
    pub groups_destroyed: u64,
}