//! Test modules for GORC (Game Object Replication Channels)
//!
//! This module contains comprehensive tests for the GORC system including:
//! - Zone event testing (player movement, object movement, new object creation)
//! - Replication system testing
//! - Integration testing
//! - Performance benchmarks

#[cfg(test)]
pub mod zone_event_test;

#[cfg(test)]
pub mod replication_test;

#[cfg(test)]
pub mod integration_test;

#[cfg(test)]
pub mod performance_test;