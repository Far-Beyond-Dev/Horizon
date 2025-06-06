//! Plugin system for the game server
//! 
//! Provides dynamic plugin loading, initialization, and lifecycle management.

pub mod loader;

pub use loader::PluginLoader;