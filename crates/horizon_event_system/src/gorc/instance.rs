//! # GORC Object Instance Manager
//!
//! This module manages individual instances of replicated objects, providing
//! the foundation for instance-specific replication and event handling.
//! Each object instance has its own zones that revolve around it for efficient
//! proximity-based replication.

use crate::types::{PlayerId, Position, Vec3};
use crate::gorc::channels::{ReplicationPriority, ReplicationLayer};
use crate::gorc::zones::{ObjectZone, ZoneManager};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::any::Any;
use tokio::sync::RwLock;
use tokio::time::Instant;
use uuid::Uuid;

/// Universal identifier for replicated object instances
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GorcObjectId(pub Uuid);

impl GorcObjectId {
    /// Creates a new random object ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Creates an object ID from a string
    pub fn from_str(s: &str) -> Result<Self, uuid::Error> {
        Uuid::parse_str(s).map(Self)
    }
}

impl Default for GorcObjectId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for GorcObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Trait for objects that can be replicated through GORC instances
pub trait GorcObject: Send + Sync + Any + std::fmt::Debug {
    /// Get the type name of this object
    fn type_name(&self) -> &'static str;
    
    /// Get the current position of this object
    fn position(&self) -> Vec3;
    
    /// Get replication priority based on observer position
    fn get_priority(&self, observer_pos: Vec3) -> ReplicationPriority;
    
    /// Serialize data for a specific replication layer
    fn serialize_for_layer(&self, layer: &ReplicationLayer) -> Result<Vec<u8>, Box<dyn std::error::Error>>;
    
    /// Get all replication layers for this object type
    fn get_layers(&self) -> Vec<ReplicationLayer>;
    
    /// Called when the object is registered with GORC
    fn on_register(&mut self, object_id: GorcObjectId) {
        let _ = object_id; // Default implementation does nothing
    }
    
    /// Called when the object is unregistered from GORC
    fn on_unregister(&mut self) {
        // Default implementation does nothing
    }
    
    /// Called when replication data is received for this object
    fn on_replicated_data(&mut self, channel: u8, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let _ = (channel, data);
        Ok(()) // Default implementation does nothing
    }

    /// Update the object's position (called by the game logic)
    fn update_position(&mut self, new_position: Vec3);

    /// Get the object as Any for downcasting
    fn as_any(&self) -> &dyn Any;
    
    /// Get the object as Any for mutable downcasting
    fn as_any_mut(&mut self) -> &mut dyn Any;
    
    /// Clone this object - required for GorcObject but implemented differently for dyn compatibility
    fn clone_object(&self) -> Box<dyn GorcObject>;
}

/// Information about a registered GORC object instance
#[derive(Debug)]
pub struct ObjectInstance {
    /// Unique identifier for this object instance
    pub object_id: GorcObjectId,
    /// Type name of the object
    pub type_name: String,
    /// The actual object instance
    pub object: Box<dyn GorcObject>,
    /// Zone manager for this object's replication zones
    pub zone_manager: ZoneManager,
    /// Current subscribers for each channel
    pub subscribers: HashMap<u8, HashSet<PlayerId>>,
    /// Last update timestamps per channel
    pub last_updates: HashMap<u8, Instant>,
    /// Replication statistics
    pub stats: ObjectStats,
    /// Whether this object needs a replication update
    pub needs_update: HashMap<u8, bool>,
}

impl ObjectInstance {
    /// Creates a new object instance
    pub fn new(object_id: GorcObjectId, mut object: Box<dyn GorcObject>) -> Self {
        let type_name = object.type_name().to_string();
        let position = object.position();
        let layers = object.get_layers();
        
        // Notify object of registration
        object.on_register(object_id);
        
        // Create zone manager with the object's layers
        let zone_manager = ZoneManager::new(position, layers);
        
        Self {
            object_id,
            type_name,
            object,
            zone_manager,
            subscribers: HashMap::new(),
            last_updates: HashMap::new(),
            stats: ObjectStats::default(),
            needs_update: HashMap::new(),
        }
    }

    /// Update the object's position and recalculate zones
    pub fn update_position(&mut self, new_position: Vec3) {
        self.object.update_position(new_position);
        self.zone_manager.update_position(new_position);
        
        // Mark all channels as needing updates due to position change
        for layer in self.object.get_layers() {
            self.needs_update.insert(layer.channel, true);
        }
    }

    /// Add a subscriber to a specific channel
    pub fn add_subscriber(&mut self, channel: u8, player_id: PlayerId) -> bool {
        let added = self.subscribers
            .entry(channel)
            .or_insert_with(HashSet::new)
            .insert(player_id);
        
        if added {
            self.stats.total_subscribers += 1;
        }
        
        added
    }

    /// Remove a subscriber from a specific channel
    pub fn remove_subscriber(&mut self, channel: u8, player_id: PlayerId) -> bool {
        if let Some(channel_subs) = self.subscribers.get_mut(&channel) {
            let removed = channel_subs.remove(&player_id);
            if removed {
                self.stats.total_subscribers = self.stats.total_subscribers.saturating_sub(1);
            }
            removed
        } else {
            false
        }
    }

    /// Check if a player is subscribed to a channel
    pub fn is_subscribed(&self, channel: u8, player_id: PlayerId) -> bool {
        self.subscribers
            .get(&channel)
            .map(|subs| subs.contains(&player_id))
            .unwrap_or(false)
    }

    /// Get all subscribers for a channel
    pub fn get_subscribers(&self, channel: u8) -> Vec<PlayerId> {
        self.subscribers
            .get(&channel)
            .map(|subs| subs.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Mark a channel as needing an update
    pub fn mark_needs_update(&mut self, channel: u8) {
        self.needs_update.insert(channel, true);
    }

    /// Check if a channel needs an update
    pub fn needs_channel_update(&self, channel: u8) -> bool {
        self.needs_update.get(&channel).copied().unwrap_or(false)
    }

    /// Mark a channel as updated
    pub fn mark_updated(&mut self, channel: u8) {
        self.needs_update.insert(channel, false);
        self.last_updates.insert(channel, Instant::now());
        self.stats.updates_sent += 1;
    }

    /// Get the object as a specific type (read-only)
    pub fn get_object<T: GorcObject + 'static>(&self) -> Option<&T> {
        self.object.as_any().downcast_ref::<T>()
    }

    /// Get the object as a specific type (mutable)
    pub fn get_object_mut<T: GorcObject + 'static>(&mut self) -> Option<&mut T> {
        self.object.as_any_mut().downcast_mut::<T>()
    }
}

impl Clone for ObjectInstance {
    fn clone(&self) -> Self {
        let cloned_object = self.object.clone_object();
        
        Self {
            object_id: self.object_id,
            type_name: self.type_name.clone(),
            object: cloned_object,
            zone_manager: self.zone_manager.clone(),
            subscribers: self.subscribers.clone(),
            last_updates: self.last_updates.clone(),
            stats: self.stats.clone(),
            needs_update: self.needs_update.clone(),
        }
    }
}

/// Statistics for an object instance
#[derive(Debug, Default, Clone)]
pub struct ObjectStats {
    /// Total replication updates sent
    pub updates_sent: u64,
    /// Total bytes transmitted
    pub bytes_transmitted: u64,
    /// Number of current subscribers across all channels
    pub total_subscribers: usize,
    /// Average update frequency per channel
    pub avg_frequencies: HashMap<u8, f32>,
    /// Zone transition events
    pub zone_transitions: u64,
}

/// Manager for all GORC object instances
#[derive(Debug)]
pub struct GorcInstanceManager {
    /// All registered object instances
    objects: Arc<RwLock<HashMap<GorcObjectId, ObjectInstance>>>,
    /// Type name to object IDs mapping
    type_registry: Arc<RwLock<HashMap<String, HashSet<GorcObjectId>>>>,
    /// Spatial index for efficient proximity queries
    spatial_index: Arc<RwLock<HashMap<GorcObjectId, Vec3>>>,
    /// Player positions for subscription management
    player_positions: Arc<RwLock<HashMap<PlayerId, Vec3>>>,
    /// Global statistics
    stats: Arc<RwLock<InstanceManagerStats>>,
}

impl GorcInstanceManager {
    /// Creates a new instance manager
    pub fn new() -> Self {
        Self {
            objects: Arc::new(RwLock::new(HashMap::new())),
            type_registry: Arc::new(RwLock::new(HashMap::new())),
            spatial_index: Arc::new(RwLock::new(HashMap::new())),
            player_positions: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(InstanceManagerStats::default())),
        }
    }

    /// Registers a new object instance
    pub async fn register_object<T: GorcObject + 'static>(
        &self,
        object: T,
        initial_position: Vec3,
    ) -> GorcObjectId {
        let object_id = GorcObjectId::new();
        let type_name = object.type_name().to_string();
        let type_name_for_registry = type_name.clone();
        let type_name_for_log = type_name.clone();
        
        let instance = ObjectInstance::new(object_id, Box::new(object));
        
        // Register in all mappings
        {
            let mut objects = self.objects.write().await;
            objects.insert(object_id, instance);
        }
        
        {
            let mut type_registry = self.type_registry.write().await;
            type_registry
                .entry(type_name_for_registry)
                .or_insert_with(HashSet::new)
                .insert(object_id);
        }
        
        {
            let mut spatial_index = self.spatial_index.write().await;
            spatial_index.insert(object_id, initial_position);
        }
        
        {
            let mut stats = self.stats.write().await;
            stats.total_objects += 1;
        }
        
        tracing::info!("🎯 Registered GORC object {} ({})", object_id, type_name_for_log);
        object_id
    }

    /// Unregisters an object instance
    pub async fn unregister_object(&self, object_id: GorcObjectId) -> bool {
        let type_name = {
            let mut objects = self.objects.write().await;
            if let Some(mut instance) = objects.remove(&object_id) {
                instance.object.on_unregister();
                Some(instance.type_name)
            } else {
                None
            }
        };

        if let Some(type_name) = type_name {
            {
                let mut type_registry = self.type_registry.write().await;
                if let Some(type_set) = type_registry.get_mut(&type_name) {
                    type_set.remove(&object_id);
                    if type_set.is_empty() {
                        type_registry.remove(&type_name);
                    }
                }
            }
            
            {
                let mut spatial_index = self.spatial_index.write().await;
                spatial_index.remove(&object_id);
            }
            
            {
                let mut stats = self.stats.write().await;
                stats.total_objects = stats.total_objects.saturating_sub(1);
            }
            
            tracing::info!("🗑️ Unregistered GORC object {} ({})", object_id, type_name);
            true
        } else {
            false
        }
    }

    /// Update an object's position
    pub async fn update_object_position(&self, object_id: GorcObjectId, new_position: Vec3) -> bool {
        let mut objects = self.objects.write().await;
        if let Some(instance) = objects.get_mut(&object_id) {
            let old_position = instance.object.position();
            instance.update_position(new_position);
            
            // Update spatial index
            drop(objects);
            let mut spatial_index = self.spatial_index.write().await;
            spatial_index.insert(object_id, new_position);
            
            // Recalculate subscriptions for players that might be affected
            self.recalculate_subscriptions_for_object(object_id, old_position, new_position).await;
            
            true
        } else {
            false
        }
    }

    /// Update a player's position and recalculate their subscriptions
    pub async fn update_player_position(&self, player_id: PlayerId, new_position: Vec3) {
        let old_position = {
            let mut player_positions = self.player_positions.write().await;
            player_positions.insert(player_id, new_position)
        };

        // If this is a new player or they moved significantly, recalculate subscriptions
        if old_position.is_none() || 
           old_position.map(|old| old.distance(new_position) > 5.0).unwrap_or(true) {
            self.recalculate_player_subscriptions(player_id, new_position).await;
        }
    }

    /// Remove a player from all subscriptions
    pub async fn remove_player(&self, player_id: PlayerId) {
        {
            let mut player_positions = self.player_positions.write().await;
            player_positions.remove(&player_id);
        }

        let mut objects = self.objects.write().await;
        for instance in objects.values_mut() {
            for channel in 0..4 {
                instance.remove_subscriber(channel, player_id);
            }
        }
    }

    /// Get an object instance by ID
    pub async fn get_object(&self, object_id: GorcObjectId) -> Option<ObjectInstance> {
        let objects = self.objects.read().await;
        // Note: This clones the entire instance, which might be expensive for large objects
        // In production, you might want to return a reference or use Arc<Mutex<ObjectInstance>>
        objects.get(&object_id).cloned()
    }

    /// Get all objects of a specific type
    pub async fn get_objects_by_type(&self, type_name: &str) -> Vec<GorcObjectId> {
        let type_registry = self.type_registry.read().await;
        type_registry
            .get(type_name)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Get objects within range of a position
    pub async fn get_objects_in_range(&self, position: Vec3, range: f32) -> Vec<GorcObjectId> {
        let spatial_index = self.spatial_index.read().await;
        spatial_index
            .iter()
            .filter(|(_, &obj_pos)| obj_pos.distance(position) <= range)
            .map(|(&obj_id, _)| obj_id)
            .collect()
    }

    /// Check if a player should be subscribed to an object on a specific channel
    async fn should_subscribe(&self, player_id: PlayerId, object_id: GorcObjectId, channel: u8) -> bool {
        let player_pos = {
            let player_positions = self.player_positions.read().await;
            player_positions.get(&player_id).copied()
        };

        let Some(player_pos) = player_pos else {
            return false;
        };

        let objects = self.objects.read().await;
        let Some(instance) = objects.get(&object_id) else {
            return false;
        };

        instance.zone_manager.is_in_zone(player_pos, channel)
    }

    /// Recalculate subscriptions for a player
    async fn recalculate_player_subscriptions(&self, player_id: PlayerId, player_position: Vec3) {
        let object_ids: Vec<GorcObjectId> = {
            let spatial_index = self.spatial_index.read().await;
            spatial_index.keys().copied().collect()
        };

        let mut objects = self.objects.write().await;
        for object_id in object_ids {
            if let Some(instance) = objects.get_mut(&object_id) {
                for channel in 0..4 {
                    let should_sub = instance.zone_manager.is_in_zone(player_position, channel);
                    let is_subbed = instance.is_subscribed(channel, player_id);

                    match (should_sub, is_subbed) {
                        (true, false) => {
                            instance.add_subscriber(channel, player_id);
                            tracing::debug!("➕ Player {} subscribed to object {} channel {}", 
                                          player_id, object_id, channel);
                        }
                        (false, true) => {
                            instance.remove_subscriber(channel, player_id);
                            tracing::debug!("➖ Player {} unsubscribed from object {} channel {}", 
                                          player_id, object_id, channel);
                        }
                        _ => {} // No change needed
                    }
                }
            }
        }
    }

    /// Recalculate subscriptions when an object moves
    async fn recalculate_subscriptions_for_object(
        &self, 
        object_id: GorcObjectId, 
        _old_position: Vec3, 
        _new_position: Vec3
    ) {
        let player_positions: Vec<(PlayerId, Vec3)> = {
            let player_positions = self.player_positions.read().await;
            player_positions.iter().map(|(&id, &pos)| (id, pos)).collect()
        };

        let mut objects = self.objects.write().await;
        if let Some(instance) = objects.get_mut(&object_id) {
            for (player_id, player_pos) in player_positions {
                for channel in 0..4 {
                    let should_sub = instance.zone_manager.is_in_zone(player_pos, channel);
                    let is_subbed = instance.is_subscribed(channel, player_id);

                    match (should_sub, is_subbed) {
                        (true, false) => {
                            instance.add_subscriber(channel, player_id);
                            instance.stats.zone_transitions += 1;
                        }
                        (false, true) => {
                            instance.remove_subscriber(channel, player_id);
                            instance.stats.zone_transitions += 1;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Get statistics for the instance manager
    pub async fn get_stats(&self) -> InstanceManagerStats {
        self.stats.read().await.clone()
    }
}

impl Default for GorcInstanceManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Global statistics for the instance manager
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct InstanceManagerStats {
    /// Total number of registered objects
    pub total_objects: usize,
    /// Total number of active subscriptions across all objects
    pub total_subscriptions: usize,
    /// Total replication events sent
    pub replication_events_sent: u64,
    /// Total bytes transmitted
    pub total_bytes_transmitted: u64,
    /// Average objects per type
    pub avg_objects_per_type: f32,
}