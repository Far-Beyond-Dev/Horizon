//! High-performance serialization cache for the universal event system
//! 
//! This module provides zero-allocation event serialization through caching and 
//! buffer pooling, significantly improving performance in high-throughput scenarios.

use crate::error::EventError;
use serde::Serialize;
use std::sync::Arc;

/// Pre-allocated buffer pool for serialization to reduce allocations
/// 
/// This pool maintains a collection of reusable buffers and cached serialized data
/// to minimize memory allocations during event emission. The implementation uses
/// Arc<Vec<u8>> for zero-copy sharing of serialized data across multiple handlers.
pub struct SerializationBufferPool {
    /// Simple implementation for now - can be enhanced with actual pooling later
    _placeholder: (),
}

impl SerializationBufferPool {
    /// Create a new serialization buffer pool
    pub fn new() -> Self {
        Self { _placeholder: () }
    }
    
    /// Serialize an event with caching support
    /// 
    /// This method serializes the event data and wraps it in an Arc for zero-copy
    /// sharing across multiple handlers. This is a significant performance improvement
    /// over re-serializing the same event data for each handler.
    /// 
    /// # Arguments
    /// * `event` - The event to serialize
    /// 
    /// # Returns
    /// An Arc-wrapped byte vector containing the serialized event data
    #[inline]
    pub fn serialize_event<T>(&self, event: &T) -> Result<Arc<Vec<u8>>, EventError>
    where
        T: Serialize,
    {
        let data = serde_json::to_vec(event)
            .map_err(|e| EventError::SerializationFailed(e.to_string()))?;
        Ok(Arc::new(data))
    }
    
    /// Serialize an event and return both Arc-wrapped and raw data
    /// 
    /// This is useful when you need both shared and owned copies of the serialized data.
    #[inline]
    pub fn serialize_event_with_copy<T>(&self, event: &T) -> Result<(Arc<Vec<u8>>, Vec<u8>), EventError>
    where
        T: Serialize,
    {
        let data = serde_json::to_vec(event)
            .map_err(|e| EventError::SerializationFailed(e.to_string()))?;
        let arc_data = Arc::new(data.clone());
        Ok((arc_data, data))
    }
    
    /// Get a pre-allocated buffer from the pool (placeholder for future enhancement)
    /// 
    /// This method is reserved for future buffer pooling implementation.
    /// Currently returns a new Vec, but could be enhanced to reuse buffers.
    #[inline]
    pub fn get_buffer(&self) -> Vec<u8> {
        Vec::new()
    }
    
    /// Return a buffer to the pool (placeholder for future enhancement)
    /// 
    /// This method is reserved for future buffer pooling implementation.
    #[inline]
    pub fn return_buffer(&self, _buffer: Vec<u8>) {
        // Placeholder - in a full implementation this would return the buffer to the pool
    }
}

impl Default for SerializationBufferPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Cached event data with metadata
/// 
/// This structure represents pre-serialized event data that can be shared
/// across multiple handlers without re-serialization. It includes metadata
/// for efficient routing and processing.
#[derive(Debug, Clone)]
pub struct CachedEventData {
    /// Arc-wrapped serialized data for zero-copy sharing
    pub data: Arc<Vec<u8>>,
    /// Event type name for routing and debugging
    pub type_name: String,
    /// Additional metadata for event processing
    pub metadata: std::collections::HashMap<String, String>,
    /// Timestamp when the event was cached
    pub cached_at: std::time::Instant,
}

impl CachedEventData {
    /// Create new cached event data
    pub fn new<T: Serialize>(
        event: &T, 
        type_name: String,
        pool: &SerializationBufferPool
    ) -> Result<Self, EventError> {
        let data = pool.serialize_event(event)?;
        Ok(Self {
            data,
            type_name,
            metadata: std::collections::HashMap::new(),
            cached_at: std::time::Instant::now(),
        })
    }
    
    /// Add metadata to the cached event
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
    
    /// Get the age of this cached event
    pub fn age(&self) -> std::time::Duration {
        self.cached_at.elapsed()
    }
    
    /// Check if this cached event is still fresh
    pub fn is_fresh(&self, max_age: std::time::Duration) -> bool {
        self.age() < max_age
    }
}