//! Multicast Management System
//!
//! This module implements efficient multicast groups and LOD-based rooms
//! for optimized replication data distribution in GORC.

mod group;
mod lod;
mod manager;
mod types;

// Re-export public types and functions
pub use group::{MulticastGroup, GroupBounds, GroupStats};
pub use lod::{LodRoom, HysteresisSettings, RoomStats};
pub use manager::MulticastManager;
pub use types::{MulticastGroupId, LodLevel, MulticastError, MulticastStats};