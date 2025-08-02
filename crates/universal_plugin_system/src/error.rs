//! Error types for the universal plugin system


/// Main error type for the plugin system
#[derive(Debug, thiserror::Error)]
pub enum PluginSystemError {
    /// Plugin loading failed
    #[error("Plugin loading failed: {0}")]
    LoadingFailed(String),

    /// Plugin initialization failed
    #[error("Plugin initialization failed: {0}")]
    InitializationFailed(String),

    /// Plugin already exists
    #[error("Plugin already exists: {0}")]
    PluginAlreadyExists(String),

    /// Plugin not found
    #[error("Plugin not found: {0}")]
    PluginNotFound(String),

    /// Version mismatch between plugin and system
    #[error("Version mismatch: {0}")]
    VersionMismatch(String),

    /// Event system error
    #[error("Event system error: {0}")]
    EventError(#[from] EventError),

    /// Library loading error
    #[error("Library loading error: {0}")]
    LibraryError(String),

    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Context error
    #[error("Context error: {0}")]
    ContextError(String),

    /// Runtime error (panics, etc.)
    #[error("Runtime error: {0}")]
    RuntimeError(String),
}

/// Errors that can occur during event handling
#[derive(Debug, thiserror::Error)]
pub enum EventError {
    /// Handler execution failed
    #[error("Handler execution failed: {0}")]
    HandlerExecutionFailed(String),

    /// Handler not found
    #[error("Handler not found for event: {0}")]
    HandlerNotFound(String),

    /// Event serialization failed
    #[error("Event serialization failed: {0}")]
    SerializationFailed(String),

    /// Event deserialization failed
    #[error("Event deserialization failed: {0}")]
    DeserializationFailed(String),

    /// Propagation error
    #[error("Event propagation failed: {0}")]
    PropagationFailed(String),

    /// Context missing
    #[error("Required context missing: {0}")]
    ContextMissing(String),

    /// Invalid event format
    #[error("Invalid event format: {0}")]
    InvalidEventFormat(String),
}

impl From<serde_json::Error> for EventError {
    fn from(err: serde_json::Error) -> Self {
        EventError::SerializationFailed(err.to_string())
    }
}

impl From<serde_json::Error> for PluginSystemError {
    fn from(err: serde_json::Error) -> Self {
        PluginSystemError::SerializationError(err.to_string())
    }
}