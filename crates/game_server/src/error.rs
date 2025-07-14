//! Error types and handling for the game server.
//!
//! This module defines the error types that can occur during server operations,
//! providing clear categorization of different failure modes.

/// Enumeration of possible server errors.
/// 
/// Categorizes errors into network-related and internal server errors
/// to help with debugging and error handling.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// Network-related errors such as binding failures or connection issues
    #[error("Network error: {0}")]
    Network(String),
    
    /// Internal server errors including plugin failures and event system issues
    #[error("Internal error: {0}")]
    Internal(String),
}