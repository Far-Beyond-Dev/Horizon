/// Helper trait for easily implementing GORC objects
/// 
/// This trait provides default implementations for common GORC object patterns,
/// reducing boilerplate code for simple object types.
pub trait SimpleGorcObject: Clone + Send + Sync + std::fmt::Debug + 'static {
    /// Get the object's current position
    fn position(&self) -> crate::Vec3;
    
    /// Update the object's position
    fn set_position(&mut self, position: crate::Vec3);
    
    /// Get the object's type name
    fn object_type() -> &'static str where Self: Sized;
    
    /// Get properties to replicate for a given channel
    fn channel_properties(channel: u8) -> Vec<String> where Self: Sized;
    
    /// Get replication configuration for this object type
    fn replication_config() -> SimpleReplicationConfig where Self: Sized {
        SimpleReplicationConfig::default()
    }
}

/// Simple configuration for GORC objects
#[derive(Debug, Clone)]
pub struct SimpleReplicationConfig {
    /// Radius for each channel
    pub channel_radii: [f32; 4],
    /// Frequency for each channel
    pub channel_frequencies: [f32; 4],
    /// Compression for each channel
    pub channel_compression: [crate::CompressionType; 4],
}

impl Default for SimpleReplicationConfig {
    fn default() -> Self {
        Self {
            channel_radii: [50.0, 150.0, 300.0, 1000.0],
            channel_frequencies: [30.0, 15.0, 10.0, 2.0],
            channel_compression: [
                crate::CompressionType::Delta,
                crate::CompressionType::Lz4,
                crate::CompressionType::Lz4,
                crate::CompressionType::High,
            ],
        }
    }
}

/// Automatic implementation of GorcObject for types implementing SimpleGorcObject
impl<T> crate::GorcObject for T 
where 
    T: SimpleGorcObject + serde::Serialize + for<'de> serde::Deserialize<'de>,
{
    fn type_name(&self) -> &'static str {
        T::object_type()
    }
    
    fn position(&self) -> crate::Vec3 {
        SimpleGorcObject::position(self)
    }
    
    fn get_priority(&self, observer_pos: crate::Vec3) -> crate::ReplicationPriority {
        let distance = self.position().distance(observer_pos);
        match distance {
            d if d < 100.0 => crate::ReplicationPriority::Critical,
            d if d < 300.0 => crate::ReplicationPriority::High,
            d if d < 1000.0 => crate::ReplicationPriority::Normal,
            _ => crate::ReplicationPriority::Low,
        }
    }
    
    fn serialize_for_layer(&self, layer: &crate::ReplicationLayer) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // For SimpleGorcObject, we serialize the entire object and let the layer filter properties
        let serialized = serde_json::to_value(self)?;
        
        if let serde_json::Value::Object(mut map) = serialized {
            // Keep only properties specified in the layer
            map.retain(|key, _| layer.properties.contains(key));
            Ok(serde_json::to_vec(&map)?)
        } else {
            Ok(serde_json::to_vec(&serialized)?)
        }
    }
    
    fn get_layers(&self) -> Vec<crate::ReplicationLayer> {
        let config = T::replication_config();
        let mut layers = Vec::new();
        
        for channel in 0..4 {
            let properties = T::channel_properties(channel);
            if !properties.is_empty() {
                layers.push(crate::ReplicationLayer::new(
                    channel,
                    config.channel_radii[channel as usize] as f64,
                    config.channel_frequencies[channel as usize] as f64,
                    properties,
                    config.channel_compression[channel as usize],
                ));
            }
        }
        
        layers
    }
    
    fn update_position(&mut self, new_position: crate::Vec3) {
        self.set_position(new_position);
    }
    
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    
    fn clone_object(&self) -> Box<dyn crate::GorcObject> {
        Box::new(self.clone())
    }
}