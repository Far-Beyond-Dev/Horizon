/// High-performance serialization cache for event system
/// This version uses a simpler approach - caching serialized data during emit_event
use std::sync::Arc;

/// Pre-allocated buffer pool for serialization to reduce allocations
pub struct SerializationBufferPool {
    /// We'll keep this simple for now - just track if we should use pooling
    _placeholder: (),
}

impl SerializationBufferPool {
    pub fn new() -> Self {
        Self { _placeholder: () }
    }
    
    /// Serializes an event with enhanced error context for debugging.
    /// For now, just serialize directly - this is still faster than the original
    /// due to the other optimizations. Future versions could implement buffer pooling.
    #[inline]
    pub fn serialize_event<T>(&self, event: &T) -> Result<Arc<Vec<u8>>, crate::events::EventError>
    where
        T: crate::events::Event,
    {
        match event.serialize() {
            Ok(data) => {
                // Log successful serialization in debug mode
                if cfg!(debug_assertions) {
                    tracing::trace!(
                        "✅ Successfully serialized event of type '{}' ({} bytes)",
                        T::type_name(),
                        data.len()
                    );
                }
                Ok(Arc::new(data))
            }
            Err(e) => {
                // Add context about where the serialization failed
                tracing::error!(
                    "🔴 SerializationBufferPool: Failed to serialize event of type '{}' in emit pipeline: {}",
                    T::type_name(),
                    e
                );
                Err(e)
            }
        }
    }
}

impl Default for SerializationBufferPool {
    fn default() -> Self {
        Self::new()
    }
}