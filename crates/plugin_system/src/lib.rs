//! Plugin system for dynamic loading and management of game plugins.
//!
//! This crate provides infrastructure for loading, initializing, and managing
//! plugins at runtime. It handles the lifecycle of plugins including discovery,
//! loading, initialization, and cleanup.

mod manager;
mod error;

pub use manager::{PluginManager, PluginSafetyConfig};
pub use error::PluginSystemError;


/// Re-export commonly used types for plugin development
pub use horizon_event_system::{EventSystem, plugin::PluginError};
pub use libloading::Library;