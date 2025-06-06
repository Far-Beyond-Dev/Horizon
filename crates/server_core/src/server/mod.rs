//! Server module containing core game server functionality
//! 
//! This module provides the main game server implementation, connection handling,
//! message processing, and event management.

pub mod core;
pub mod connection;
pub mod message_handler;
pub mod event_processor;
pub mod config;

pub use core::GameServer;
pub use config::ServerConfig;
pub use connection::{ConnectionManager, ConnectionId};
pub use message_handler::MessageHandler;
pub use event_processor::EventProcessor;