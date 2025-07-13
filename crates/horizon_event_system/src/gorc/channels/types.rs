/// Core types for replication channels
use serde::{Deserialize, Serialize};

/// Compression algorithms available for replication data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionType {
    /// No compression - fastest but largest payload
    None,
    /// LZ4 compression - good balance of speed and size
    Lz4,
    /// Zlib compression - smaller payload but slower
    Zlib,
    /// Delta compression - only send changes from previous state
    Delta,
    /// Quantized compression - reduce precision for smaller payload
    Quantized,
    /// High compression - maximum compression for low-priority data
    High,
    /// Custom game-specific compression
    Custom(u8),
}

/// Priority levels for replication data
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
pub enum ReplicationPriority {
    /// Critical data that must be delivered immediately
    Critical = 0,
    /// Important data with high priority
    High = 1,
    /// Normal priority data
    Normal = 2,
    /// Low priority data that can be delayed
    Low = 3,
}

/// Error types for GORC operations
#[derive(Debug, thiserror::Error)]
pub enum GorcError {
    #[error("Configuration error: {0}")]
    Configuration(String),
    
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    #[error("Object not found: {id}")]
    ObjectNotFound { id: String },
    
    #[error("Channel error: {0}")]
    Channel(String),
    
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
    
    #[error("Capacity exceeded: {0}")]
    CapacityExceeded(String),
}

/// Example mineral type for demo objects
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MineralType {
    Platinum,
    Gold,
    Silver,
    Iron,
    Copper,
    Uranium,
}

impl Default for MineralType {
    fn default() -> Self {
        Self::Iron
    }
}