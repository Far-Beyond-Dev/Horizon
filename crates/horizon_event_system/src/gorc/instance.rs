//! # GORC Object Instance Manager
//!
//! This module manages individual instances of replicated objects, providing
//! the foundation for instance-specific replication and event handling.
//! Each object instance has its own zones that revolve around it for efficient
//! proximity-based replication.

use crate::types::{PlayerId, Position, Vec3};
use crate::gorc::channels::{ReplicationPriority, ReplicationLayer};
use crate::gorc::zones::ZoneManager;
use crate::gorc::spatial::SpatialPartition;
use crate::gorc::virtualization::{VirtualizationManager, VirtualizationConfig};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::any::Any;
use async_lock::RwLock;
use dashmap::DashMap;
use tokio::time::Instant;
use uuid::Uuid;
use tracing::{debug, info, warn};

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
    fn type_name(&self) -> &str;
    
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
    /// All registered object instances (lock-free for concurrent access)
    objects: Arc<DashMap<GorcObjectId, ObjectInstance>>,
    /// Type name to object IDs mapping
    type_registry: Arc<RwLock<HashMap<String, HashSet<GorcObjectId>>>>,
    /// Spatial index using an R-tree for efficient proximity queries
    spatial_index: Arc<RwLock<SpatialPartition>>,
    /// Object positions for spatial tracking (lock-free for fast reads)
    object_positions: Arc<DashMap<GorcObjectId, Vec3>>,
    /// Player positions for subscription management
    player_positions: Arc<RwLock<HashMap<PlayerId, Vec3>>>,
    /// Zone size warnings tracking (object_id -> largest_zone_radius)
    zone_size_warnings: Arc<RwLock<HashMap<GorcObjectId, f64>>>,
    /// Zone virtualization manager for high-density optimization
    virtualization_manager: Arc<VirtualizationManager>,
    /// Global statistics
    stats: Arc<RwLock<InstanceManagerStats>>,
}

impl GorcInstanceManager {
    /// Creates a new instance manager
    pub fn new() -> Self {
        Self::new_with_config(VirtualizationConfig::default())
    }

    /// Creates a new instance manager with custom virtualization configuration
    pub fn new_with_config(virtualization_config: VirtualizationConfig) -> Self {
        let spatial_index = SpatialPartition::new();
        let virtualization_manager = Arc::new(VirtualizationManager::new(virtualization_config));

        let manager = Self {
            objects: Arc::new(DashMap::new()),
            type_registry: Arc::new(RwLock::new(HashMap::new())),
            spatial_index: Arc::new(RwLock::new(spatial_index)),
            object_positions: Arc::new(DashMap::new()),
            player_positions: Arc::new(RwLock::new(HashMap::new())),
            zone_size_warnings: Arc::new(RwLock::new(HashMap::new())),
            virtualization_manager,
            stats: Arc::new(RwLock::new(InstanceManagerStats::default())),
        };

        // Initialize spatial index with default region in the background
        let spatial_index_ref = manager.spatial_index.clone();
        tokio::spawn(async move {
            let spatial_index = spatial_index_ref.write().await;
            spatial_index.add_region(
                "default".to_string(),
                Vec3::new(-10000.0, -10000.0, -1000.0),
                Vec3::new(10000.0, 10000.0, 1000.0)
            ).await;
        });

        manager
    }

    /// Registers a new object instance (convenience - auto-generated UUID)
    pub async fn register_object<T: GorcObject + 'static>(
        &self,
        object: T,
        initial_position: Vec3,
    ) -> GorcObjectId {
        self.register_object_with_uuid(object, initial_position, None).await
    }

    /// Registers a new object instance (optionally provide UUID)
    /// 
    /// OPTIMIZED: Preparation work done before locks, locks held minimally
    /// NO SEMAPHORE - the write locks provide sufficient serialization
    pub async fn register_object_with_uuid<T: GorcObject + 'static>(
        &self,
        object: T,
        initial_position: Vec3,
        uuid: Option<GorcObjectId>,
    ) -> GorcObjectId {
        // === PHASE 1: Preparation (no locks) ===
        let object_id = uuid.unwrap_or_else(GorcObjectId::new);
        let type_name = object.type_name().to_string();
        println!("🔧 REGISTER[{}]: Starting - type={}", object_id, type_name);
        
        let layers_for_warning = object.get_layers();
        let mut instance = ObjectInstance::new(object_id, Box::new(object));
        
        println!("🔧 REGISTER[{}]: Phase 1 - acquiring player_positions read lock", object_id);
        // Snapshot player positions FIRST (short read lock)
        let player_positions_snapshot: Vec<(PlayerId, Vec3)> = {
            let player_positions = self.player_positions.read().await;
            player_positions.iter().map(|(&id, &pos)| (id, pos)).collect()
        };
        println!("🔧 REGISTER[{}]: Phase 1 - got {} players", object_id, player_positions_snapshot.len());
        
        // Pre-calculate subscriptions BEFORE acquiring write lock (no lock needed)
        // This moves the expensive zone checking outside the critical section
        for (player_id, player_pos) in &player_positions_snapshot {
            for channel in 0..4 {
                if instance.zone_manager.is_in_zone(*player_pos, channel) {
                    instance.add_subscriber(channel, *player_id);
                }
            }
        }
        println!("🔧 REGISTER[{}]: Phase 1 - subscriptions calculated", object_id);
        
        // === PHASE 2: Critical section - each lock is independent ===
        // Insert into objects map (lock-free with DashMap)
        println!("🔧 REGISTER[{}]: Phase 2 - inserting into objects (DashMap)", object_id);
        self.objects.insert(object_id, instance);
        println!("🔧 REGISTER[{}]: Phase 2 - objects inserted", object_id);
        
        println!("🔧 REGISTER[{}]: Phase 2 - acquiring type_registry write lock", object_id);
        // Update type registry
        {
            let mut type_registry = self.type_registry.write().await;
            type_registry
                .entry(type_name.clone())
                .or_insert_with(HashSet::new)
                .insert(object_id);
        }
        println!("🔧 REGISTER[{}]: Phase 2 - type_registry updated", object_id);
        
        // Update positions (lock-free with DashMap)
        println!("🔧 REGISTER[{}]: Phase 2 - inserting object_positions (DashMap)", object_id);
        self.object_positions.insert(object_id, initial_position);
        println!("🔧 REGISTER[{}]: Phase 2 - object_positions updated", object_id);
        
        println!("🔧 REGISTER[{}]: Phase 2 - acquiring stats write lock", object_id);
        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.total_objects += 1;
        }
        println!("🔧 REGISTER[{}]: Phase 2 - stats updated", object_id);
        
        // === PHASE 3: Post-registration (no contention-sensitive work) ===
        println!("🔧 REGISTER[{}]: Phase 3 - checking zone warnings", object_id);
        // Zone warnings can happen outside critical section
        self.check_zone_size_warnings(object_id, &layers_for_warning).await;
        
        println!("🔧 REGISTER[{}]: COMPLETE", object_id);
        tracing::info!("🎯 Registered GORC object {} ({})", object_id, type_name);
        object_id
    }

    /// Unregisters an object instance
    pub async fn unregister_object(&self, object_id: GorcObjectId) -> bool {
        // Remove from objects (lock-free with DashMap)
        let type_name = if let Some((_, mut instance)) = self.objects.remove(&object_id) {
            instance.object.on_unregister();
            Some(instance.type_name)
        } else {
            None
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
            
            // Remove from object_positions (lock-free with DashMap)
            self.object_positions.remove(&object_id);

            {
                let mut zone_warnings = self.zone_size_warnings.write().await;
                zone_warnings.remove(&object_id);
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

    /// Update an object's position and return zone membership changes for zone events
    pub async fn update_object_position(&self, object_id: GorcObjectId, new_position: Vec3) -> Option<(Vec3, Vec3, Vec<(PlayerId, u8, bool)>)> {
        // Get old position and update (using DashMap get_mut for lock-free access)
        let old_position = if let Some(mut instance) = self.objects.get_mut(&object_id) {
            let old_pos = instance.object.position();
            instance.update_position(new_position);
            old_pos
        } else {
            return None;
        };

        // Update object position tracking (lock-free with DashMap)
        self.object_positions.insert(object_id, new_position);

        // Check for virtual zone splits due to object movement
        let virtual_zones_to_split = self.virtualization_manager
            .update_object_position(object_id, old_position, new_position)
            .await;

        // Handle virtual zone splits
        for virtual_id in virtual_zones_to_split {
            if let Err(e) = self.virtualization_manager.split_virtual_zone(virtual_id).await {
                warn!("Failed to split virtual zone due to object movement: {}", e);
            }
        }

        // Calculate zone membership changes for all players
        let zone_changes = self.recalculate_subscriptions_for_object_with_events(object_id, old_position, new_position).await;

        Some((old_position, new_position, zone_changes))
    }

    /// Update a player's position and return zone membership changes
    pub async fn update_player_position(&self, player_id: PlayerId, new_position: Vec3) -> (Vec<(GorcObjectId, u8)>, Vec<(GorcObjectId, u8)>) {
        let mut zone_entries = Vec::new();
        let mut zone_exits = Vec::new();
        
        // Get old position and update to new position
        let old_position = {
            let mut player_positions = self.player_positions.write().await;
            let old_pos = player_positions.get(&player_id).copied();
            player_positions.insert(player_id, new_position);
            old_pos
        };

        {
            let spatial_position: Position = new_position.into();
            let partition = self.spatial_index.read().await;
            partition
                .update_player_position(player_id, spatial_position)
                .await;
        }


        // Check all objects for zone membership changes (lock-free iteration over DashMap)
        for entry in self.objects.iter() {
            let object_id = entry.key();
            let instance = entry.value();
            
            // CRITICAL: Get object position from DashMap (lock-free, single source of truth)
            let object_position = match self.object_positions.get(object_id) {
                Some(pos) => *pos,
                None => {
                    warn!("Object {} not found in object_positions tracking", object_id);
                    continue;
                }
            };
            
            let layers = instance.object.get_layers();
            
            for layer in layers {
                let radius_sq = layer.radius * layer.radius;
                let distance_sq = new_position.distance_squared(object_position);
                let was_in_zone = old_position.map_or(false, |pos| pos.distance_squared(object_position) <= radius_sq);
                let is_in_zone = distance_sq <= radius_sq;
                
                
                match (was_in_zone, is_in_zone) {
                    (false, true) => {
                        debug!("🎮 GORC: Zone entry - player {} enters object {} channel {}", player_id, object_id, layer.channel);
                        zone_entries.push((*object_id, layer.channel));
                    },
                    (true, false) => {
                        debug!("🎮 GORC: Zone exit - player {} leaves object {} channel {}", player_id, object_id, layer.channel);
                        zone_exits.push((*object_id, layer.channel));
                    },
                    _ => {
                        // Special case: if this is a first spawn (old_position is None) and player is in range,
                        // force zone entry even if the logic above didn't catch it
                        if old_position.is_none() && is_in_zone {
                            debug!("🎮 GORC: First spawn entry - player {} enters object {} channel {}", player_id, object_id, layer.channel);
                            zone_entries.push((*object_id, layer.channel));
                        }
                    }
                }
            }
        }

        debug!("🎮 GORC: Zone changes for player {} - {} entries, {} exits", player_id, zone_entries.len(), zone_exits.len());

        // If this is a new player or they moved significantly, recalculate subscriptions
        const MOVEMENT_THRESHOLD_SQ: f64 = 25.0; // 5.0 * 5.0
        if old_position.is_none() || 
           old_position.map(|old| old.distance_squared(new_position) > MOVEMENT_THRESHOLD_SQ).unwrap_or(true) {
            self.recalculate_player_subscriptions(player_id, new_position).await;
        }
        
        (zone_entries, zone_exits)
    }

    /// Sets up core event listeners for automatic player position updates
    /// 
    /// This registers GORC to listen for core movement events and automatically
    /// update player positions in the replication system.
    /// 
    /// # Arguments
    /// 
    /// * `event_system` - The event system to register listeners with
    pub async fn setup_core_listeners(self: std::sync::Arc<Self>, event_system: std::sync::Arc<crate::system::EventSystem>) -> Result<(), crate::events::EventError> {
        use crate::events::PlayerMovementEvent;
        
        let instance_manager = self.clone();
        event_system
            .on_core("player_movement", move |event: PlayerMovementEvent| {
                let manager_clone = instance_manager.clone();
                tokio::spawn(async move {
                    manager_clone.update_player_position(event.player_id, event.new_position).await;
                });
                Ok(())
            })
            .await?;
            
        Ok(())
    }

    /// Add a player to the position tracking system
    /// NOTE: This registers the player in both spatial_index AND player_positions.
    /// After calling this, call subscribe_player_to_existing_objects() for zone detection.
    pub async fn add_player(&self, player_id: PlayerId, position: Vec3) {
        debug!("🎮 GORC: Adding player {} at position {:?}", player_id, position);

        // CRITICAL FIX: Insert into player_positions so register_object_with_uuid
        // can find the player and auto-subscribe them to their own object
        {
            let mut player_positions = self.player_positions.write().await;
            player_positions.insert(player_id, position);
        }
        
        {
            let spatial_position: Position = position.into();
            let partition = self.spatial_index.read().await;
            partition
                .update_player_position(player_id, spatial_position)
                .await;
        }
        
        // Update statistics
        let mut stats = self.stats.write().await;
        stats.total_subscriptions += 1;

        let total_players = self.player_positions.read().await.len();
        info!(
            "🎮 GORC: Player {} added. Total tracked players: {}",
            player_id,
            total_players
        );
    }
    
    /// Subscribe a newly added player to all existing objects they are within range of.
    /// Returns list of (object_id, channel) pairs for zone entries.
    /// MUST be called AFTER add_player() and AFTER player's own object is registered.
    pub async fn subscribe_player_to_existing_objects(&self, player_id: PlayerId, player_position: Vec3) -> Vec<(GorcObjectId, u8)> {
        let mut zone_entries = Vec::new();
        
        // Collect object positions from DashMap (lock-free)
        let object_ids_and_positions: Vec<(GorcObjectId, Vec3)> = 
            self.object_positions.iter().map(|entry| (*entry.key(), *entry.value())).collect();
        
        info!(
            "🔍 GORC Subscribe: Player {} at {:?} checking {} existing objects",
            player_id, player_position, object_ids_and_positions.len()
        );
        
        // Use DashMap get_mut for lock-free mutable access
        for (object_id, object_position) in object_ids_and_positions {
            if let Some(mut instance) = self.objects.get_mut(&object_id) {
                let layers = instance.object.get_layers();
                let object_type = instance.type_name.clone();
                
                for layer in layers {
                    let radius_sq = layer.radius * layer.radius;
                    let distance_sq = player_position.distance_squared(object_position);
                    let is_in_zone = distance_sq <= radius_sq;
                    let is_already_subscribed = instance.is_subscribed(layer.channel, player_id);
                    
                    debug!(
                        "🔍 GORC Subscribe Check: Object {} ({}) ch{} - distance²={:.2}, radius²={:.2}, in_zone={}, already_sub={}",
                        object_id, object_type, layer.channel, distance_sq, radius_sq, is_in_zone, is_already_subscribed
                    );
                    
                    if is_in_zone && !is_already_subscribed {
                        instance.add_subscriber(layer.channel, player_id);
                        zone_entries.push((object_id, layer.channel));
                        info!(
                            "✅ GORC Subscribe: Player {} SUBSCRIBED to {} ({}) ch{} (distance: {:.2}m, radius: {:.2}m)",
                            player_id, object_id, object_type, layer.channel, distance_sq.sqrt(), layer.radius
                        );
                    }
                }
            }
        }
        
        info!(
            "🎮 GORC Subscribe COMPLETE: Player {} - {} zone entries to existing objects",
            player_id, zone_entries.len()
        );
        
        zone_entries
    }
    
    /// Remove a player from all subscriptions
    pub async fn remove_player(&self, player_id: PlayerId) {
        {
            let mut player_positions = self.player_positions.write().await;
            player_positions.remove(&player_id);
        }

        {
            let partition = self.spatial_index.read().await;
            partition.remove_player(player_id).await;
        }

        // Use DashMap iter_mut for lock-free mutable iteration
        for mut entry in self.objects.iter_mut() {
            let instance = entry.value_mut();
            for channel in 0..4 {
                instance.remove_subscriber(channel, player_id);
            }
        }
    }

    /// Get an object instance by ID
    pub async fn get_object(&self, object_id: GorcObjectId) -> Option<ObjectInstance> {
        // Use DashMap get for lock-free read access
        self.objects.get(&object_id).map(|entry| entry.value().clone())
    }
    
    /// Get subscriber count for an object channel directly from DashMap (for diagnostics)
    pub async fn get_subscriber_count(&self, object_id: GorcObjectId, channel: u8) -> usize {
        if let Some(entry) = self.objects.get(&object_id) {
            entry.value().subscribers.get(&channel)
                .map(|subs| subs.len())
                .unwrap_or(0)
        } else {
            0
        }
    }

    /// Get all objects of a specific type
    pub async fn get_objects_by_type(&self, type_name: &str) -> Vec<GorcObjectId> {
        let type_registry = self.type_registry.read().await;
        type_registry
            .get(type_name)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Update an object instance (after handlers have modified it)
    pub async fn update_object(&self, object_id: GorcObjectId, instance: ObjectInstance) {
        // Use DashMap insert for lock-free write access
        self.objects.insert(object_id, instance);
    }

    /// Find a player's GORC object by player ID (for message routing)
    /// 
    /// This is a temporary implementation that assumes the first object of type "GorcPlayer"
    /// belongs to the requesting player. A more robust implementation would store player->object mappings.
    pub async fn find_player_object(&self, _player_id: crate::PlayerId) -> Option<GorcObjectId> {
        // For now, just find the first GorcPlayer object
        // TODO: Implement proper player ID to object ID mapping
        let objects_by_type = self.get_objects_by_type("GorcPlayer").await;
        objects_by_type.into_iter().next()
    }

    /// Get objects within range of a position using spatial index optimization
    pub async fn get_objects_in_range(&self, position: Vec3, range: f64) -> Vec<GorcObjectId> {
        let mut result_objects = Vec::new();

        // Get largest zone radius for query optimization
        let query_radius = self.get_max_zone_radius().await.max(range);

        // Use spatial queries for efficiency when available
        let spatial_index = self.spatial_index.read().await;
        let query_results = spatial_index.query_radius(
            crate::types::Position::new(position.x as f64, position.y as f64, position.z as f64),
            query_radius
        ).await;

        // Filter by actual object positions and range (lock-free iteration with DashMap)
        let range_sq = range * range;
        for _query_result in query_results {
            for entry in self.object_positions.iter() {
                let object_id = *entry.key();
                let obj_pos = *entry.value();
                if obj_pos.distance_squared(position) <= range_sq {
                    result_objects.push(object_id);
                }
            }
        }

        // Fallback to direct position checking if spatial index is empty
        if result_objects.is_empty() {
            result_objects = self.object_positions
                .iter()
                .filter(|entry| entry.value().distance_squared(position) <= range_sq)
                .map(|entry| *entry.key())
                .collect();
        }

        result_objects
    }
    
    /// Get the tracked position of an object (lock-free, single source of truth for spatial queries)
    pub fn get_object_position(&self, object_id: GorcObjectId) -> Option<Vec3> {
        self.object_positions.get(&object_id).map(|entry| *entry)
    }
    
    /// Find all players within radius of a position (for event-driven GORC emission)
    pub async fn find_players_in_radius(&self, position: Vec3, radius: f64) -> Vec<PlayerId> {
        let player_positions = self.player_positions.read().await;
        debug!("🔍 GORC: Finding players within {}m of position {:?}", radius, position);
        debug!("🔍 GORC: Total tracked players: {}", player_positions.len());
        
        let radius_sq = radius * radius;
        let subscribers: Vec<PlayerId> = player_positions
            .iter()
            .filter_map(|(&player_id, &player_pos)| {
                let distance_sq = player_pos.distance_squared(position);
                // let distance = distance_sq.sqrt(); // Only for debug logging
                // debug!("🔍 GORC: Player {} at {:?}, distance: {:.2}m", player_id, player_pos, distance);
                if distance_sq <= radius_sq {
                    debug!("  ✅ Within range");
                    Some(player_id)
                } else {
                    debug!("  ❌ Outside range");
                    None
                }
            })
            .collect();
        
        debug!("🔍 GORC: Returning {} subscribers", subscribers.len());
        subscribers
    }
    
    
    /// Get current object state for a specific layer/channel
    pub async fn get_object_state_for_layer(&self, object_id: GorcObjectId, channel: u8) -> Option<Vec<u8>> {
        // Use DashMap get for lock-free read access
        if let Some(instance) = self.objects.get(&object_id) {
            let layers = instance.object.get_layers();
            if let Some(layer) = layers.iter().find(|l| l.channel == channel) {
                // Serialize only the properties defined for this layer
                if let Ok(data) = instance.object.serialize_for_layer(layer) {
                    return Some(data);
                }
            }
        }
        None
    }

    /// Check if a player should be subscribed to an object on a specific channel
    #[allow(dead_code)]
    async fn should_subscribe(&self, player_id: PlayerId, object_id: GorcObjectId, channel: u8) -> bool {
        let player_pos = {
            let player_positions = self.player_positions.read().await;
            player_positions.get(&player_id).copied()
        };

        let Some(player_pos) = player_pos else {
            return false;
        };

        // Use DashMap get for lock-free read access
        let Some(instance) = self.objects.get(&object_id) else {
            return false;
        };

        instance.zone_manager.is_in_zone(player_pos, channel)
    }

    /// Recalculate subscriptions for a player
    async fn recalculate_player_subscriptions(&self, player_id: PlayerId, player_position: Vec3) {
        // Collect object IDs from DashMap (lock-free)
        let object_ids: Vec<GorcObjectId> = 
            self.object_positions.iter().map(|entry| *entry.key()).collect();

        // Use DashMap get_mut for lock-free mutable access
        for object_id in object_ids {
            if let Some(mut instance) = self.objects.get_mut(&object_id) {
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

    /// Recalculate subscriptions when an object moves and return zone changes for events
    async fn recalculate_subscriptions_for_object_with_events(
        &self,
        object_id: GorcObjectId,
        old_position: Vec3,
        new_position: Vec3
    ) -> Vec<(PlayerId, u8, bool)> {
        let mut zone_changes = Vec::new();

        let player_positions: Vec<(PlayerId, Vec3)> = {
            let player_positions = self.player_positions.read().await;
            player_positions.iter().map(|(&id, &pos)| (id, pos)).collect()
        };

        // Use DashMap get_mut for lock-free mutable access
        if let Some(mut instance) = self.objects.get_mut(&object_id) {
            let layers = instance.object.get_layers();

            for (player_id, player_pos) in player_positions {
                // Use inner zone optimization - check smallest zones first
                let mut player_in_inner_zone = false;
                let mut sorted_layers = layers.clone();
                sorted_layers.sort_by(|a, b| a.radius.partial_cmp(&b.radius).unwrap());

                let smallest_radius = sorted_layers.get(0).map(|l| l.radius).unwrap_or(0.0);
                for layer in &sorted_layers {
                    let channel = layer.channel;

                    // Skip larger zones if player is already in a smaller inner zone
                    if player_in_inner_zone && layer.radius > smallest_radius {
                        if instance.is_subscribed(channel, player_id) {
                            // Player is guaranteed to be in this larger zone too
                            continue;
                        }
                    }

                    let radius_sq = layer.radius * layer.radius;
                    let was_in_zone = player_pos.distance_squared(old_position) <= radius_sq;
                    let is_in_zone = player_pos.distance_squared(new_position) <= radius_sq;
                    let is_subbed = instance.is_subscribed(channel, player_id);

                    if is_in_zone && layer.radius == smallest_radius {
                        player_in_inner_zone = true;
                    }

                    match (was_in_zone, is_in_zone, is_subbed) {
                        (false, true, false) => {
                            // Zone entry
                            instance.add_subscriber(channel, player_id);
                            instance.stats.zone_transitions += 1;
                            zone_changes.push((player_id, channel, true)); // true = entry
                            debug!("🎯 GORC Object Movement: Player {} entered zone {} of object {}", player_id, channel, object_id);
                        }
                        (true, false, true) => {
                            // Zone exit
                            instance.remove_subscriber(channel, player_id);
                            instance.stats.zone_transitions += 1;
                            zone_changes.push((player_id, channel, false)); // false = exit
                            debug!("🚪 GORC Object Movement: Player {} exited zone {} of object {}", player_id, channel, object_id);
                        }
                        (false, true, true) | (true, false, false) => {
                            // Subscription state matches zone state - sync if needed
                            if !is_subbed && is_in_zone {
                                instance.add_subscriber(channel, player_id);
                            } else if is_subbed && !is_in_zone {
                                instance.remove_subscriber(channel, player_id);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        zone_changes
    }

    /// Check for large zone sizes and emit warnings
    async fn check_zone_size_warnings(&self, object_id: GorcObjectId, layers: &[ReplicationLayer]) {
        let max_radius = layers.iter()
            .map(|layer| layer.radius)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        // Warning threshold for large zones that might impact performance
        const LARGE_ZONE_WARNING_THRESHOLD: f64 = 500.0;
        const VERY_LARGE_ZONE_WARNING_THRESHOLD: f64 = 1000.0;

        if max_radius > VERY_LARGE_ZONE_WARNING_THRESHOLD {
            warn!("⚠️ GORC: Object {} has very large zone radius {:.1} - this significantly increases spatial query cost. Consider reducing zone size if possible.", object_id, max_radius);

            let mut zone_warnings = self.zone_size_warnings.write().await;
            zone_warnings.insert(object_id, max_radius);
        } else if max_radius > LARGE_ZONE_WARNING_THRESHOLD {
            warn!("⚠️ GORC: Object {} has large zone radius {:.1} - monitor performance impact.", object_id, max_radius);
        }
    }

    /// Get the maximum zone radius across all objects for spatial query optimization
    async fn get_max_zone_radius(&self) -> f64 {
        // Use DashMap iter for lock-free read access
        self.objects.iter()
            .flat_map(|entry| entry.value().object.get_layers())
            .map(|layer| layer.radius)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(100.0) // Default reasonable radius
    }

    /// Notify existing players when a new object is created (handles Issue #1)
    pub async fn notify_existing_players_for_new_object(&self, object_id: GorcObjectId) -> Vec<(PlayerId, u8)> {
        let mut zone_entries = Vec::new();

        // CRITICAL: Get object position from DashMap (lock-free, single source of truth)
        let object_position = match self.object_positions.get(&object_id) {
            Some(entry) => *entry,
            None => return zone_entries,
        };

        // Use DashMap get for lock-free read access
        let layers = if let Some(instance) = self.objects.get(&object_id) {
            instance.object.get_layers()
        } else {
            return zone_entries;
        };

        let player_positions = {
            let player_positions = self.player_positions.read().await;
            player_positions.iter().map(|(&id, &pos)| (id, pos)).collect::<Vec<_>>()
        };

        // Use DashMap get_mut for lock-free mutable access
        if let Some(mut instance) = self.objects.get_mut(&object_id) {
            for (player_id, player_pos) in player_positions {
                // Check if player should be subscribed to any zones of this new object
                for layer in &layers {
                    let channel = layer.channel;
                    let radius_sq = layer.radius * layer.radius;
                    let distance_sq = player_pos.distance_squared(object_position);

                    if distance_sq <= radius_sq {
                        instance.add_subscriber(channel, player_id);
                        zone_entries.push((player_id, channel));
                        debug!("🆕 GORC New Object: Player {} automatically entered zone {} of new object {}", player_id, channel, object_id);
                    }
                }
            }
        }

        zone_entries
    }

    /// Analyzes virtualization opportunities and applies recommendations
    pub async fn process_virtualization(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Collect current objects and their zones using DashMap iter (lock-free)
        let objects_info = {
            let mut info = HashMap::new();
            for entry in self.objects.iter() {
                let object_id = *entry.key();
                let instance = entry.value();
                // Get position from DashMap (lock-free)
                if let Some(position_entry) = self.object_positions.get(&object_id) {
                    let position = *position_entry;
                    let layers = instance.object.get_layers();
                    info.insert(object_id, (position, layers));
                }
            }
            info
        };

        // Get virtualization recommendations
        let recommendations = self.virtualization_manager
            .analyze_virtualization_opportunities(&objects_info)
            .await;

        // Apply merge recommendations
        for merge_request in recommendations.merge_recommendations {
            match self.virtualization_manager.merge_zones(merge_request).await {
                Ok(virtual_id) => {
                    debug!("✅ Successfully created virtual zone {}", virtual_id.0);
                }
                Err(e) => {
                    warn!("❌ Failed to merge zones: {}", e);
                }
            }
        }

        // Apply split recommendations
        for split_request in recommendations.split_recommendations {
            match self.virtualization_manager.split_virtual_zone(split_request.virtual_id).await {
                Ok(liberated_objects) => {
                    debug!("✅ Successfully split virtual zone - liberated {} objects", liberated_objects.len());
                }
                Err(e) => {
                    warn!("❌ Failed to split virtual zone: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Checks if a position is within a virtual zone
    pub async fn is_in_virtual_zone(&self, position: Vec3, channel: u8) -> Option<crate::gorc::virtualization::VirtualZoneId> {
        self.virtualization_manager.is_in_virtual_zone(position, channel).await
    }

    /// Gets all objects in a virtual zone
    pub async fn get_virtual_zone_objects(&self, virtual_id: crate::gorc::virtualization::VirtualZoneId) -> Vec<GorcObjectId> {
        self.virtualization_manager.get_virtual_zone_objects(virtual_id).await
    }

    /// Gets virtualization statistics
    pub async fn get_virtualization_stats(&self) -> crate::gorc::virtualization::VirtualizationStats {
        self.virtualization_manager.get_stats().await
    }

    /// Get statistics for the instance manager
    pub async fn get_stats(&self) -> InstanceManagerStats {
        let mut stats = self.stats.read().await.clone();

        // Add zone warning count to stats
        let zone_warnings = self.zone_size_warnings.read().await;
        stats.large_zone_warnings = zone_warnings.len();

        stats
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
    /// Number of objects with large zone warnings
    pub large_zone_warnings: usize,
}
