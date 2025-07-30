//! Error types for the plugin system.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum PluginSystemError {
    #[error("Plugin loading error: {0}")]
    LoadingError(String),
    
    #[error("Plugin initialization error: {0}")]
    InitializationError(String),
    
    #[error("Plugin not found: {0}")]
    PluginNotFound(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Library loading error: {0}")]
    LibraryError(String),
    
    #[error("Event system error: {0}")]
    EventSystemError(String),
    
    #[error("Plugin already exists: {0}")]
    PluginAlreadyExists(String),
    
    #[error("Plugin version mismatch: {0}")]
    VersionMismatch(String),
}