//! Message handling and routing for client-server communication.
//!
//! This module provides the infrastructure for parsing, routing, and handling
//! messages between clients and the server plugin system.

pub mod router;
pub mod types;

pub use router::route_client_message;
pub use types::ClientMessage;