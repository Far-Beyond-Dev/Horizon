//! # Game Object Replication Channels (GORC)
//!
//! An advanced replication system for managing complex multiplayer game state distribution.
//! GORC provides fine-grained control over what information reaches which players and at
//! what frequency through a multi-channel architecture.
//!
//! ## Channel Architecture
//!
//! GORC organizes replication into four distinct channels:
//! - **Channel 0 (Critical)**: Essential game state (position, health, collision) at 30-60Hz
//! - **Channel 1 (Detailed)**: Important non-critical info (animations, weapons) at 15-30Hz  
//! - **Channel 2 (Cosmetic)**: Visual enhancements (particles, effects) at 5-15Hz
//! - **Channel 3 (Metadata)**: Informational data (names, achievements) at 1-5Hz
//!
//! ## Subscription Systems
//!
//! Dynamic subscription management based on:
//! - **Proximity**: Different detail levels based on distance
//! - **Relationships**: Team members, guild affiliations
//! - **Interest**: Player focus and activity patterns
//!
//! ## Performance Features
//!
//! - Sub-millisecond event routing for critical channels
//! - Efficient spatial partitioning for subscription management
//! - Multicast groups for optimized distribution
//! - LOD-based room management with hysteresis algorithms

pub mod channels;
pub mod subscription;
pub mod multicast;
pub mod spatial;

pub use channels::{
    ReplicationChannel, ReplicationLayer, ReplicationLayers, ReplicationPriority, 
    CompressionType, GorcManager, MineralType, Replication, GorcObjectRegistry
};
pub use subscription::{
    SubscriptionManager, SubscriptionType, ProximitySubscription,
    RelationshipSubscription, InterestSubscription
};
pub use multicast::{
    MulticastManager, MulticastGroup, LodRoom
};
pub use spatial::{
    SpatialPartition, SpatialQuery, RegionQuadTree
};