/// Object type registry for GORC replication
use super::layer::{ReplicationLayer, ReplicationLayers};
use super::types::GorcError;
use crate::gorc::instance::GorcObject;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Legacy trait for backwards compatibility - now just creates a default instance to get layers
pub trait Replication {
    /// Initialize the replication layers for this object type
    fn init_layers() -> ReplicationLayers;
}

/// Registry for tracking object types and their replication configurations
pub struct GorcObjectRegistry {
    /// Map of object type names to their replication layers
    registered_objects: Arc<RwLock<HashMap<String, Vec<ReplicationLayer>>>>,
    /// Statistics about registered objects
    stats: Arc<RwLock<RegistryStats>>,
}

impl GorcObjectRegistry {
    /// Creates a new object registry
    pub fn new() -> Self {
        Self {
            registered_objects: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(RegistryStats::default())),
        }
    }

    /// Registers an object type with its replication layers using GorcObject
    pub async fn register_object_type<T: GorcObject + Default + 'static>(&self, object_name: String) {
        let default_obj = T::default();
        let layers = default_obj.get_layers();
        
        {
            let mut objects = self.registered_objects.write().await;
            objects.insert(object_name.clone(), layers.clone());
        }
        
        {
            let mut stats = self.stats.write().await;
            stats.registered_objects += 1;
            stats.total_layers += layers.len();
        }
        
        info!("📝 Registered GORC object type: {}", object_name);
    }
    
    /// Legacy method - Registers an object type with its replication layers
    pub async fn register_object<T: Replication + 'static>(&self, object_name: String) {
        let layers = T::init_layers().layers;
        
        {
            let mut objects = self.registered_objects.write().await;
            objects.insert(object_name.clone(), layers.clone());
        }

        {
            let mut stats = self.stats.write().await;
            stats.registered_objects += 1;
            stats.total_layers += layers.len();
        }

        info!("📦 Registered GORC object type: {}", object_name);
    }

    /// Registers an object type with explicit layers
    pub async fn register_object_with_layers(&self, object_name: String, layers: Vec<ReplicationLayer>) {
        {
            let mut objects = self.registered_objects.write().await;
            objects.insert(object_name.clone(), layers.clone());
        }

        {
            let mut stats = self.stats.write().await;
            stats.registered_objects += 1;
            stats.total_layers += layers.len();
        }

        info!("📦 Registered GORC object type: {}", object_name);
    }

    /// Gets the replication layers for a registered object type
    pub async fn get_layers(&self, object_name: &str) -> Option<Vec<ReplicationLayer>> {
        let objects = self.registered_objects.read().await;
        objects.get(object_name).cloned()
    }

    /// Lists all registered object types
    pub async fn list_objects(&self) -> Vec<String> {
        let objects = self.registered_objects.read().await;
        objects.keys().cloned().collect()
    }

    /// Gets registry statistics
    pub async fn get_stats(&self) -> RegistryStats {
        let mut stats = self.stats.read().await.clone();
        
        // Update calculated fields
        if stats.registered_objects > 0 {
            stats.avg_layers_per_object = stats.total_layers as f32 / stats.registered_objects as f32;
        }
        
        stats
    }

    /// Validates all registered object configurations
    pub async fn validate_all(&self) -> Result<(), GorcError> {
        let objects = self.registered_objects.read().await;
        
        for (object_name, layers) in objects.iter() {
            let layer_builder = ReplicationLayers {
                layers: layers.clone(),
            };
            
            if let Err(e) = self.validate_layers(&layer_builder) {
                return Err(GorcError::Configuration(format!("{}: {}", object_name, e)));
            }
        }
        
        Ok(())
    }

    /// Validates a set of replication layers
    fn validate_layers(&self, layers: &ReplicationLayers) -> Result<(), String> {
        // Check for duplicate channels
        let mut channels = std::collections::HashSet::new();
        for layer in &layers.layers {
            if !channels.insert(layer.channel) {
                return Err(format!("Duplicate channel: {}", layer.channel));
            }
        }

        // Check for valid channel numbers
        for layer in &layers.layers {
            if layer.channel > 3 {
                return Err(format!("Invalid channel number: {}", layer.channel));
            }
        }

        // Check for valid frequencies
        for layer in &layers.layers {
            if layer.frequency <= 0.0 || layer.frequency > 120.0 {
                return Err(format!("Invalid frequency: {}", layer.frequency));
            }
        }

        // Check for valid radii
        for layer in &layers.layers {
            if layer.radius <= 0.0 {
                return Err(format!("Invalid radius: {}", layer.radius));
            }
        }

        Ok(())
    }

    /// Removes an object type from the registry
    pub async fn unregister_object(&self, object_name: &str) -> bool {
        let removed = {
            let mut objects = self.registered_objects.write().await;
            objects.remove(object_name).is_some()
        };

        if removed {
            let mut stats = self.stats.write().await;
            stats.registered_objects = stats.registered_objects.saturating_sub(1);
            info!("📦 Unregistered GORC object type: {}", object_name);
        }

        removed
    }

    /// Checks if an object type is registered
    pub async fn is_registered(&self, object_name: &str) -> bool {
        let objects = self.registered_objects.read().await;
        objects.contains_key(object_name)
    }

    /// Gets the number of registered object types
    pub async fn count(&self) -> usize {
        let objects = self.registered_objects.read().await;
        objects.len()
    }

    /// Clears all registered objects
    pub async fn clear(&self) {
        let mut objects = self.registered_objects.write().await;
        objects.clear();
        
        let mut stats = self.stats.write().await;
        stats.registered_objects = 0;
        stats.total_layers = 0;
        stats.avg_layers_per_object = 0.0;
    }
}

impl Default for GorcObjectRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the object registry
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RegistryStats {
    /// Number of registered object types
    pub registered_objects: usize,
    /// Total number of replication layers across all objects
    pub total_layers: usize,
    /// Average layers per object type
    pub avg_layers_per_object: f32,
    /// Most recently registered object timestamp
    pub last_registration_timestamp: u64,
}