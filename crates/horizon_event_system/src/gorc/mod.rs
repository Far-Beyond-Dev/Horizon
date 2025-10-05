//! # Game Object Replication Channels (GORC)
//!
//! An advanced replication system for managing complex multiplayer game state distribution.
//! GORC provides fine-grained control over what information reaches which players and at
//! what frequency through a multi-channel architecture with instance-based zone management.
//!
//! ## Core Concepts
//!
//! ### Object Instances
//! Each game object is registered as a unique instance with its own replication zones.
//! Objects implement the `GorcObject` trait to define their replication behavior.
//!
//! ### Zone-Based Replication
//! Each object instance has multiple concentric zones corresponding to different channels:
//! - **Channel 0 (Critical)**: Immediate vicinity, 30-60Hz updates
//! - **Channel 1 (Detailed)**: Close interaction range, 15-30Hz updates  
//! - **Channel 2 (Cosmetic)**: Visual range, 5-15Hz updates
//! - **Channel 3 (Metadata)**: Strategic information, 1-5Hz updates
//!
//! ### Dynamic Subscriptions
//! Players are automatically subscribed/unsubscribed from object zones as they move,
//! ensuring optimal bandwidth usage and relevant information delivery.
//!
//! ## Architecture Overview
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                         GORC System                         │
//! │  ┌─────────────────┐  ┌──────────────────┐  ┌─────────────┐ │
//! │  │ Instance Manager│  │   Zone Manager   │  │   Network   │ │
//! │  │   - Objects     │  │   - Proximity    │  │  - Batching │ │
//! │  │   - Registry    │  │   - Hysteresis   │  │  - Priority │ │
//! │  │   - Lifecycle   │  │   - Subscriptions│  │  - Delivery │ │
//! │  └─────────────────┘  └──────────────────┘  └─────────────┘ │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Performance Characteristics
//!
//! - **Sub-millisecond event routing** for critical channels
//! - **Efficient spatial partitioning** with O(log n) proximity queries
//! - **Adaptive frequency scaling** based on network conditions
//! - **Intelligent subscription management** with hysteresis for stability
//! - **Comprehensive statistics** for monitoring and optimization

// Module declarations
pub mod channels;
pub mod instance;
pub mod zones;
pub mod network;
pub mod subscription;
pub mod multicast;
pub mod spatial;
pub mod virtualization;
pub mod config;
pub mod system;

// Utility modules
pub mod defaults;
pub mod utils;
pub mod examples;
pub mod migration_guide;

// Test modules
#[cfg(test)]
pub mod tests;

// Re-export core types for use elsewhere in the core and for use in plugins
pub use channels::{
    ReplicationChannel, ReplicationLayer, ReplicationLayers, ReplicationPriority, 
    CompressionType, GorcManager, MineralType, Replication, GorcObjectRegistry,
    GorcConfig, GorcStats, PerformanceReport, GorcError
};

pub use instance::{
    GorcObject, GorcObjectId, ObjectInstance, GorcInstanceManager, 
    InstanceManagerStats, ObjectStats
};

pub use zones::{
    ObjectZone, ZoneManager, ZoneAnalysis, ZoneConfig, ZoneStats,
    AdvancedZoneManager, ZonePerformanceMetrics
};

pub use network::{
    NetworkReplicationEngine, ReplicationCoordinator, NetworkConfig, NetworkStats,
    ReplicationUpdate, ReplicationBatch, ReplicationStats, NetworkError,
    UpdateScheduler, SchedulerStats
};

pub use subscription::{
    SubscriptionManager, SubscriptionType, ProximitySubscription,
    RelationshipSubscription, InterestSubscription, SubscriptionStats,
    InterestLevel, ActivityPattern
};

pub use multicast::{
    MulticastManager, MulticastGroup, LodRoom, LodLevel, MulticastGroupId,
    GroupBounds, MulticastStats, MulticastError
};

pub use spatial::{
    SpatialPartition, SpatialQuery, RegionRTree, QueryResult, QueryFilters,
    SpatialStats, GlobalSpatialStats, SpatialIndexStats, NodeStats, SpatialObject
};

pub use virtualization::{
    VirtualizationManager, VirtualizationConfig, VirtualZone, VirtualZoneId,
    VirtualizationStats, VirtualizationRecommendations, ZoneMergeRequest, ZoneSplitRequest
};

pub use config::{
    GorcServerConfig, GorcConfigBuilder, GorcGeneralConfig, SpatialConfig,
    NetworkConfig as GorcNetworkConfig, MonitoringConfig, ConfigValidationError
};

pub use system::{
    CompleteGorcSystem, GorcPerformanceReport, GORC_VERSION, MAX_CHANNELS
};