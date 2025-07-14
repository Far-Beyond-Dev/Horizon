//! Core server implementation and connection handling.
//!
//! This module contains the main game server structure and the logic
//! for handling client connections and server lifecycle management.

pub mod core;
pub mod handlers;

pub use core::GameServer;