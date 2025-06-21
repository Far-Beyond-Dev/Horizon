//! Comprehensive error types for RecipeSmith

use std::{
    fmt,
    io::Error as IoError,
    path::PathBuf,
    time::SystemTimeError,
};
use thiserror::Error;
use uuid::Uuid;

/// Recipe storage errors
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Failed to read directory {0}: {1}")]
    DirectoryRead(PathBuf, IoError),

    #[error("Failed to read file {0}: {1}")]
    FileRead(PathBuf, IoError),

    #[error("Failed to create file {0}: {1}")]
    FileCreate(PathBuf, IoError),

    #[error("Failed to write to file {0}: {1}")]
    FileWrite(PathBuf, IoError),

    #[error("Failed to sync file {0}: {1}")]
    FileSync(PathBuf, IoError),

    #[error("Failed to rename file from {0} to {1}: {2}")]
    FileRename(PathBuf, PathBuf, IoError),

    #[error("Failed to delete file {0}: {1}")]
    FileDelete(PathBuf, IoError),

    #[error("Failed to get metadata for file {0}: {1}")]
    FileMetadata(PathBuf, Box<dyn std::error::Error + Send + Sync>),

    #[error("Failed to serialize recipe {0}: {1}")]
    Serialization(Uuid, serde_json::Error),

    #[error("Failed to deserialize file {0}: {1}")]
    Deserialization(PathBuf, serde_json::Error),

    #[error("Recipe {0} not found")]
    RecipeNotFound(Uuid),
}

/// Recipe validation errors
#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Invalid recipe ID")]
    InvalidRecipeId,

    #[error("Recipe name cannot be empty")]
    EmptyName,

    #[error("Recipe has no ingredients")]
    NoIngredients,

    #[error("Recipe has no products")]
    NoProducts,

    #[error("Invalid ingredient ID")]
    InvalidIngredientId,

    #[error("Invalid product ID")]
    InvalidProductId,

    #[error("Quantity cannot be zero")]
    ZeroQuantity,

    #[error("Duplicate ingredient: {0}")]
    DuplicateIngredient(Uuid),

    #[error("Duplicate product: {0}")]
    DuplicateProduct(Uuid),

    #[error("Invalid craft time (must be > 0)")]
    InvalidCraftTime,

    #[error("Missing required ingredient: {0}")]
    MissingIngredient(Uuid),

    #[error("Insufficient quantity for item {item_id}: required {required}, provided {provided}")]
    InsufficientQuantity {
        item_id: Uuid,
        required: u32,
        provided: u32,
    },

    #[error("Multiple validation errors occurred: {0:?}")]
    BatchValidationFailed(Vec<ValidationError>),
}

/// Crafting operation errors
#[derive(Debug, Error)]
pub enum CraftingError {
    #[error("Recipe not found: {0}")]
    RecipeNotFound(Uuid),

    #[error("Craft not found")]
    CraftNotFound,

    #[error("Player has too many active crafts")]
    TooManyActiveCrafts,

    #[error("Validation error: {0}")]
    Validation(#[from] ValidationError),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Event system error: {0}")]
    EventSystem(String),
}

/// Plugin initialization and runtime errors
#[derive(Debug, Error)]
pub enum PluginError {
    #[error("Initialization failed: {0}")]
    InitializationFailed(String),

    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Crafting error: {0}")]
    CraftingError(#[from] CraftingError),

    #[error("Event system error: {0}")]
    EventSystem(String),
}

// Implement conversion from event system errors
impl From<event_system::EventError> for CraftingError {
    fn from(e: event_system::EventError) -> Self {
        CraftingError::EventSystem(e.to_string())
    }
}

impl From<event_system::EventError> for PluginError {
    fn from(e: event_system::EventError) -> Self {
        PluginError::EventSystem(e.to_string())
    }
}

// Convert storage errors to plugin errors
impl From<StorageError> for PluginError {
    fn from(e: StorageError) -> Self {
        PluginError::StorageError(e.to_string())
    }
}

// Convert validation errors to plugin errors
impl From<ValidationError> for PluginError {
    fn from(e: ValidationError) -> Self {
        PluginError::ValidationFailed(e.to_string())
    }
}

// Convert string errors to plugin errors (for general error cases)
impl From<String> for PluginError {
    fn from(s: String) -> Self {
        PluginError::EventSystem(s)
    }
}

impl From<&str> for PluginError {
    fn from(s: &str) -> Self {
        PluginError::EventSystem(s.to_string())
    }
}

// Additional conversions for the event system integration
impl From<CraftingError> for event_system::PluginError {
    fn from(e: CraftingError) -> Self {
        match e {
            CraftingError::RecipeNotFound(_) => event_system::PluginError::NotFound(e.to_string()),
            CraftingError::CraftNotFound => event_system::PluginError::NotFound(e.to_string()),
            CraftingError::TooManyActiveCrafts => event_system::PluginError::ExecutionError(e.to_string()),
            CraftingError::Validation(_) => event_system::PluginError::ExecutionError(e.to_string()),
            CraftingError::Storage(_) => event_system::PluginError::ExecutionError(e.to_string()),
            CraftingError::EventSystem(_) => event_system::PluginError::ExecutionError(e.to_string()),
        }
    }
}

impl From<ValidationError> for event_system::PluginError {
    fn from(e: ValidationError) -> Self {
        event_system::PluginError::ExecutionError(e.to_string())
    }
}

impl From<StorageError> for event_system::PluginError {
    fn from(e: StorageError) -> Self {
        event_system::PluginError::ExecutionError(e.to_string())
    }
}

// Result type aliases for convenience
pub type PluginResult<T> = Result<T, PluginError>;
pub type CraftingResult<T> = Result<T, CraftingError>;
pub type ValidationResult<T> = Result<T, ValidationError>;
pub type StorageResult<T> = Result<T, StorageError>;