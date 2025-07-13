//! GORC Network Replication Engine
//!
//! This module handles the actual network transmission of replication data,
//! including batching, compression, prioritization, and delivery guarantees.

mod coordinator;
mod engine;
mod queue;
mod types;

// Re-export public types and functions
pub use coordinator::{ReplicationCoordinator, UpdateScheduler, SchedulerStats};
pub use engine::NetworkReplicationEngine;
pub use queue::{PriorityUpdateQueue, PlayerNetworkState, PlayerStats};
pub use types::{
    NetworkConfig, NetworkError, NetworkStats, ReplicationBatch, 
    ReplicationStats, ReplicationUpdate
};