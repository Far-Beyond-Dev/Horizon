/// Level of Detail (LOD) room management
use super::types::{MulticastGroupId, LodLevel};
use crate::types::{PlayerId, Position, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tokio::time::Instant;

/// A LOD-based room that manages different levels of detail for replication
pub struct LodRoom {
    /// Unique identifier
    pub id: MulticastGroupId,
    /// Center position of the room
    pub center: Position,
    /// LOD level for this room
    pub lod_level: LodLevel,
    /// Current members in this room
    pub members: HashSet<PlayerId>,
    /// Nested rooms with higher LOD levels
    pub nested_rooms: HashMap<LodLevel, LodRoom>,
    /// Parent room (if this is a nested room)
    pub parent: Option<MulticastGroupId>,
    /// Hysteresis settings for smooth transitions
    pub hysteresis: HysteresisSettings,
    /// Room statistics
    pub stats: RoomStats,
    /// Last update timestamp
    pub last_update: Instant,
}

impl LodRoom {
    /// Creates a new LOD room
    pub fn new(center: Position, lod_level: LodLevel) -> Self {
        Self {
            id: MulticastGroupId::new(),
            center,
            lod_level,
            members: HashSet::new(),
            nested_rooms: HashMap::new(),
            parent: None,
            hysteresis: HysteresisSettings::default(),
            stats: RoomStats::default(),
            last_update: Instant::now(),
        }
    }

    /// Creates a hierarchical LOD room with nested levels
    pub fn with_nested_levels(center: Position, base_lod: LodLevel) -> Self {
        let mut room = Self::new(center, base_lod);
        
        // Create nested rooms for higher LOD levels
        let mut current_lod = base_lod;
        while let Some(higher_lod) = current_lod.upgrade() {
            let nested_room = Self::new(center, higher_lod);
            room.add_nested_room(nested_room);
            current_lod = higher_lod;
        }
        
        room
    }

    /// Adds a nested room with higher LOD
    pub fn add_nested_room(&mut self, mut room: LodRoom) {
        room.parent = Some(self.id);
        self.nested_rooms.insert(room.lod_level, room);
    }

    /// Gets the appropriate LOD level for a position
    pub fn get_lod_for_position(&self, position: Position) -> LodLevel {
        let distance = self.distance_from_center(position);
        
        // Check nested rooms first (higher LOD)
        for (_, room) in &self.nested_rooms {
            if distance <= room.lod_level.radius() {
                return room.get_lod_for_position(position);
            }
        }
        
        // Apply hysteresis for smooth transitions
        let base_radius = self.lod_level.radius();
        let threshold = base_radius; // Simplified hysteresis for now
        
        if distance <= threshold {
            self.lod_level
        } else {
            // Find the next appropriate LOD level
            self.lod_level.downgrade().unwrap_or(LodLevel::Minimal)
        }
    }

    /// Calculates distance from room center
    fn distance_from_center(&self, position: Position) -> f32 {
        self.center.distance(position)
    }

    /// Adds a member to the appropriate room based on their position
    pub fn add_member(&mut self, player_id: PlayerId, position: Position) {
        let target_lod = self.get_lod_for_position(position);
        
        // Add to the appropriate room
        if target_lod == self.lod_level {
            self.members.insert(player_id);
            self.stats.member_count = self.members.len();
        } else {
            // Find the appropriate nested room
            for room in self.nested_rooms.values_mut() {
                if room.lod_level == target_lod {
                    room.add_member(player_id, position);
                    break;
                }
            }
        }
        
        self.last_update = Instant::now();
    }

    /// Removes a member from all rooms
    pub fn remove_member(&mut self, player_id: PlayerId) {
        self.members.remove(&player_id);
        
        for room in self.nested_rooms.values_mut() {
            room.remove_member(player_id);
        }
        
        self.stats.member_count = self.members.len();
        self.last_update = Instant::now();
    }

    /// Updates member position and moves them between rooms if needed
    pub fn update_member_position(&mut self, player_id: PlayerId, new_position: Position) {
        let _target_lod = self.get_lod_for_position(new_position);
        
        // Remove from current rooms
        self.remove_member(player_id);
        
        // Add to the appropriate room
        self.add_member(player_id, new_position);
    }

    /// Gets all members in this room and nested rooms
    pub fn get_all_members(&self) -> HashSet<PlayerId> {
        let mut all_members = self.members.clone();
        
        for room in self.nested_rooms.values() {
            all_members.extend(room.get_all_members());
        }
        
        all_members
    }

    /// Gets members for a specific LOD level
    pub fn get_members_for_lod(&self, lod: LodLevel) -> HashSet<PlayerId> {
        if self.lod_level == lod {
            self.members.clone()
        } else if let Some(room) = self.nested_rooms.get(&lod) {
            room.get_members_for_lod(lod)
        } else {
            HashSet::new()
        }
    }

    /// Optimizes the room structure by removing empty nested rooms
    pub fn optimize(&mut self) {
        self.nested_rooms.retain(|_, room| {
            room.optimize();
            !room.is_empty()
        });
    }

    /// Checks if the room and all nested rooms are empty
    pub fn is_empty(&self) -> bool {
        self.members.is_empty() && self.nested_rooms.values().all(|r| r.is_empty())
    }

    /// Gets the total member count including nested rooms
    pub fn total_member_count(&self) -> usize {
        self.members.len() + self.nested_rooms.values().map(|r| r.total_member_count()).sum::<usize>()
    }

    /// Updates room center position
    pub fn update_center(&mut self, new_center: Position) {
        self.center = new_center;
        
        // Update nested rooms if they should follow
        for room in self.nested_rooms.values_mut() {
            room.update_center(new_center);
        }
        
        self.last_update = Instant::now();
    }
}

/// Hysteresis settings for smooth LOD transitions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HysteresisSettings {
    /// Threshold for entering a higher LOD level
    pub enter_threshold: f32,
    /// Threshold for exiting to a lower LOD level
    pub exit_threshold: f32,
    /// Minimum time between LOD changes
    pub transition_cooldown: std::time::Duration,
}

impl Default for HysteresisSettings {
    fn default() -> Self {
        Self {
            enter_threshold: 5.0,   // 5 units closer to enter
            exit_threshold: 10.0,   // 10 units farther to exit
            transition_cooldown: std::time::Duration::from_millis(500),
        }
    }
}

/// Statistics for a LOD room
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RoomStats {
    /// Current member count
    pub member_count: usize,
    /// Peak member count
    pub peak_member_count: usize,
    /// Number of LOD transitions
    pub lod_transitions: u64,
    /// Total time members have spent in this room
    pub total_occupancy_time: std::time::Duration,
    /// Average member count over time
    pub avg_member_count: f32,
}

impl RoomStats {
    /// Updates peak member count
    pub fn update_peak(&mut self, current_count: usize) {
        if current_count > self.peak_member_count {
            self.peak_member_count = current_count;
        }
    }

    /// Records a LOD transition
    pub fn record_transition(&mut self) {
        self.lod_transitions += 1;
    }

    /// Updates average member count
    pub fn update_average(&mut self, current_count: usize) {
        // Simple exponential moving average
        self.avg_member_count = self.avg_member_count * 0.9 + current_count as f32 * 0.1;
    }
}