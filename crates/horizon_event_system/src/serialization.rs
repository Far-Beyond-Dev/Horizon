//! Serialization layer abstractions for JSON and Binary formats.
//!
//! This module provides compile-time abstractions for different serialization formats,
//! enabling zero-cost selection between JSON and binary with full type safety.

use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

/// Marker trait for serialization formats with compile-time guarantees.
pub trait SerializationFormat: Send + Sync + 'static {
    /// The name of the serialization format for debugging/logging.
    const NAME: &'static str;
    
    /// Whether this format is human-readable (JSON = true, Binary = false).
    const HUMAN_READABLE: bool;
    
    /// The MIME type for this format.
    const MIME_TYPE: &'static str;
}

/// JSON serialization format marker.
#[derive(Debug, Clone, Copy)]
pub struct JsonFormat;

impl SerializationFormat for JsonFormat {
    const NAME: &'static str = "JSON";
    const HUMAN_READABLE: bool = true;
    const MIME_TYPE: &'static str = "application/json";
}

/// Binary serialization format marker (using MessagePack).
#[derive(Debug, Clone, Copy)]
pub struct BinaryFormat;

impl SerializationFormat for BinaryFormat {
    const NAME: &'static str = "Binary";
    const HUMAN_READABLE: bool = false;
    const MIME_TYPE: &'static str = "application/msgpack";
}

/// Errors that can occur during serialization/deserialization.
#[derive(Debug, thiserror::Error)]
pub enum SerializationError {
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("Binary serialization error: {0}")]
    Binary(String),
    
    #[error("Invalid data format: {0}")]
    InvalidFormat(String),
}

/// Type-erased serializer that can handle any serializable type.
/// This avoids the dyn compatibility issues with generic methods.
pub trait TypeErasedSerializer: Send + Sync {
    /// Serialize a value to bytes using serde_json::Value as intermediate.
    fn serialize_value(&self, value: &serde_json::Value) -> Result<Vec<u8>, SerializationError>;
    
    /// Deserialize bytes to a serde_json::Value.
    fn deserialize_value(&self, data: &[u8]) -> Result<serde_json::Value, SerializationError>;
    
    /// Get format name for debugging.
    fn format_name(&self) -> &'static str;
}

/// JSON serializer implementation.
pub struct JsonSerializer {
    _phantom: PhantomData<JsonFormat>,
}

impl JsonSerializer {
    pub fn new() -> Self {
        Self { _phantom: PhantomData }
    }
    
    /// Serialize a typed value directly.
    pub fn serialize<T: Serialize>(&self, value: &T) -> Result<Vec<u8>, SerializationError> {
        serde_json::to_vec(value).map_err(SerializationError::Json)
    }
    
    /// Deserialize bytes to a typed value directly.
    pub fn deserialize<T: for<'de> Deserialize<'de>>(&self, data: &[u8]) -> Result<T, SerializationError> {
        serde_json::from_slice(data).map_err(SerializationError::Json)
    }
}

impl Default for JsonSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeErasedSerializer for JsonSerializer {
    fn serialize_value(&self, value: &serde_json::Value) -> Result<Vec<u8>, SerializationError> {
        serde_json::to_vec(value).map_err(SerializationError::Json)
    }
    
    fn deserialize_value(&self, data: &[u8]) -> Result<serde_json::Value, SerializationError> {
        serde_json::from_slice(data).map_err(SerializationError::Json)
    }
    
    fn format_name(&self) -> &'static str {
        JsonFormat::NAME
    }
}

/// Binary serializer implementation (placeholder - would use MessagePack in production).
pub struct BinarySerializer {
    _phantom: PhantomData<BinaryFormat>,
}

impl BinarySerializer {
    pub fn new() -> Self {
        Self { _phantom: PhantomData }
    }
    
    /// Serialize a typed value directly.
    pub fn serialize<T: Serialize>(&self, value: &T) -> Result<Vec<u8>, SerializationError> {
        // For demo purposes, we'll use a simple binary format
        // In production, this would use MessagePack, bincode, or similar
        let json_bytes = serde_json::to_vec(value)
            .map_err(|e| SerializationError::Binary(e.to_string()))?;
        
        // Add a simple header to indicate binary format
        let mut result = vec![0xBF, 0x00]; // Binary format magic bytes
        result.extend_from_slice(&json_bytes);
        Ok(result)
    }
    
    /// Deserialize bytes to a typed value directly.
    pub fn deserialize<T: for<'de> Deserialize<'de>>(&self, data: &[u8]) -> Result<T, SerializationError> {
        // Check for binary format header
        if data.len() < 2 || &data[0..2] != [0xBF, 0x00] {
            return Err(SerializationError::InvalidFormat("Missing binary format header".to_string()));
        }
        
        // Remove header and deserialize
        let json_data = &data[2..];
        serde_json::from_slice(json_data)
            .map_err(|e| SerializationError::Binary(e.to_string()))
    }
}

impl Default for BinarySerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeErasedSerializer for BinarySerializer {
    fn serialize_value(&self, value: &serde_json::Value) -> Result<Vec<u8>, SerializationError> {
        let json_bytes = serde_json::to_vec(value)
            .map_err(|e| SerializationError::Binary(e.to_string()))?;
        
        let mut result = vec![0xBF, 0x00]; // Binary format magic bytes
        result.extend_from_slice(&json_bytes);
        Ok(result)
    }
    
    fn deserialize_value(&self, data: &[u8]) -> Result<serde_json::Value, SerializationError> {
        if data.len() < 2 || &data[0..2] != [0xBF, 0x00] {
            return Err(SerializationError::InvalidFormat("Missing binary format header".to_string()));
        }
        
        let json_data = &data[2..];
        serde_json::from_slice(json_data)
            .map_err(|e| SerializationError::Binary(e.to_string()))
    }
    
    fn format_name(&self) -> &'static str {
        BinaryFormat::NAME
    }
}

/// Factory for creating serializers.
pub struct SerializerFactory;

impl SerializerFactory {
    /// Create a JSON serializer.
    pub fn json() -> JsonSerializer {
        JsonSerializer::new()
    }
    
    /// Create a binary serializer.
    pub fn binary() -> BinarySerializer {
        BinarySerializer::new()
    }
}

/// Utility trait for compile-time format validation.
pub trait FormatCompatible<F: SerializationFormat> {
    fn is_compatible() -> bool;
}

/// JSON format is compatible with human-readable data.
impl FormatCompatible<JsonFormat> for String {
    fn is_compatible() -> bool { true }
}

impl FormatCompatible<JsonFormat> for serde_json::Value {
    fn is_compatible() -> bool { true }
}

/// Binary format is compatible with any serializable data.
impl<T: Serialize> FormatCompatible<BinaryFormat> for T {
    fn is_compatible() -> bool { true }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestMessage {
        id: u32,
        content: String,
    }

    #[test]
    fn test_json_serialization() {
        let serializer = JsonSerializer::new();
        let message = TestMessage {
            id: 42,
            content: "Hello, World!".to_string(),
        };

        let bytes = serializer.serialize(&message).unwrap();
        let deserialized: TestMessage = serializer.deserialize(&bytes).unwrap();

        assert_eq!(message, deserialized);
    }

    #[test]
    fn test_binary_serialization() {
        let serializer = BinarySerializer::new();
        let message = TestMessage {
            id: 42,
            content: "Hello, World!".to_string(),
        };

        let bytes = serializer.serialize(&message).unwrap();
        let deserialized: TestMessage = serializer.deserialize(&bytes).unwrap();

        assert_eq!(message, deserialized);
        
        // Check that binary format has header
        assert_eq!(&bytes[0..2], [0xBF, 0x00]);
    }

    #[test]
    fn test_format_constants() {
        assert_eq!(JsonFormat::NAME, "JSON");
        assert_eq!(JsonFormat::HUMAN_READABLE, true);
        assert_eq!(BinaryFormat::NAME, "Binary");
        assert_eq!(BinaryFormat::HUMAN_READABLE, false);
    }
}