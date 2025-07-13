//! Replication Channels and Layer Management
//!
//! This module defines the core structures for GORC replication channels,
//! including channel configuration, replication layers, and the main GORC manager.

mod channel;
mod layer;
mod manager;
mod registry;
mod types;

// Re-export public types and functions
pub use channel::{ReplicationChannel, ChannelStats};
pub use layer::{ReplicationLayer, ReplicationLayers};
pub use manager::{
    GorcManager, GorcConfig, GorcStats, 
    ChannelPerformanceReport, PerformanceReport
};
pub use registry::{GorcObjectRegistry, Replication, RegistryStats};
pub use types::{CompressionType, ReplicationPriority, GorcError, MineralType};