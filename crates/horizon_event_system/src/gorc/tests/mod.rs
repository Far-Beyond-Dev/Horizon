//! Test modules for GORC (Game Object Replication Channels)
//!
//! This module contains comprehensive tests for the GORC system including:
//! - Zone event testing (player movement, object movement, new object creation)
//! - Replication system testing
//! - Integration testing
//! - Performance benchmarks
//! - Distance filtering regression tests
//! - Realistic client movement simulation

#[cfg(test)]
pub mod zone_event_test;

#[cfg(test)]
pub mod replication_test;

#[cfg(test)]
pub mod integration_test;

#[cfg(test)]
pub mod performance_test;

#[cfg(test)]
pub mod virtualization_test;

#[cfg(test)]
pub mod distance_filtering_test;

#[cfg(test)]
pub mod realistic_movement_test;