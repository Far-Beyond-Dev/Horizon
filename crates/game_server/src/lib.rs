//! Distributed Games Server
//! 
//! A high-performance game server with plugin support and configurable regions.

pub mod config;
pub mod logging;
pub mod plugins;
pub mod shutdown;

pub use config::{Args, Config};