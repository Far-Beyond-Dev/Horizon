//! # Event Handler Modules
//! 
//! This module contains specialized event handlers for different aspects of player interaction
//! in the GORC (Game Object Replication and Communication) system.
//! 
//! ## Architecture Overview
//! 
//! The plugin_player crate implements a complete player management system that handles:
//! - Player connection/disconnection lifecycle
//! - Real-time movement and position updates
//! - Combat and weapon firing events
//! - Communication and chat systems
//! - Advanced ship scanning and metadata sharing
//! 
//! ## Handler Organization
//! 
//! Each handler module is responsible for a specific channel in the GORC system:
//! - [`connection`] - Player lifecycle events (connect/disconnect)
//! - [`movement`] - Real-time ship movement on channel 0
//! - [`combat`] - Weapon firing and combat events on channel 1
//! - [`communication`] - Chat and messaging on channel 2
//! - [`scanning`] - Ship scanning and metadata on channel 3
//! 
//! ## Security Model
//! 
//! All handlers implement strict security validation:
//! - Authentication checks for all client requests
//! - Player ownership validation (players can only control their own ships)
//! - Input sanitization and bounds checking
//! - Unauthorized access prevention with detailed error logging
//! 
//! ## Performance Characteristics
//! 
//! - **Movement**: High-frequency updates (60Hz) with 25m replication range
//! - **Combat**: Medium-frequency events with 500m broadcast range
//! - **Communication**: Social events with 300m range
//! - **Scanning**: Low-frequency detailed data with 100m intimate range
//! 
//! ## Example Usage
//! 
//! ```rust
//! use plugin_player::handlers::*;
//! 
//! // Handlers are automatically registered by PlayerPlugin
//! // Each handler processes specific GORC channels and event types
//! ```

pub mod connection;
pub mod movement;
pub mod combat;
pub mod communication;
pub mod scanning;

// Re-export common handler utilities
pub use connection::*;
pub use movement::*;
pub use combat::*;
pub use communication::*;
pub use scanning::*;