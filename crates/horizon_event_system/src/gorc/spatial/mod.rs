//! Spatial partitioning and querying for GORC
//!
//! This module provides efficient spatial data structures for managing
//! object positions and proximity queries in the GORC system.

mod partition;
mod query;
mod rtree;

// Re-export public types and functions
pub use partition::SpatialPartition;
pub use query::{QueryFilters, QueryResult, SpatialQuery};
pub use rtree::{NodeStats, RegionRTree, SpatialIndexStats, SpatialObject};

/// Statistics for spatial queries
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SpatialStats {
    /// Total queries performed
    pub total_queries: u64,
    /// Average query time in microseconds
    pub avg_query_time_us: f32,
    /// Number of objects tracked
    pub objects_tracked: usize,
}

/// Global spatial system statistics
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct GlobalSpatialStats {
    /// Per-region statistics
    pub region_stats: std::collections::HashMap<String, SpatialStats>,
    /// Total memory usage in bytes
    pub memory_usage: usize,
}