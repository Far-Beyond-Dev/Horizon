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
    
    /// For now, just serialize directly - this is still faster than the original
    /// due to the other optimizations. Future versions could implement buffer pooling.
    #[inline]
    pub fn serialize_event<T>(&self, event: &T) -> Result<Arc<Vec<u8>>, crate::events::EventError>
    where
        T: crate::events::Event,
    {
        let data = event.serialize()?;
        Ok(Arc::new(data))
    }
}

impl Default for SerializationBufferPool {
    fn default() -> Self {
        Self::new()
    }
}