//! # Multicast Management System
//!
//! This module implements efficient multicast groups and LOD-based rooms
//! for optimized replication data distribution in GORC.

use crate::types::{PlayerId, Position};
use crate::gorc::channels::ReplicationPriority;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};

/// Unique identifier for multicast groups
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MulticastGroupId(pub u64);

impl MulticastGroupId {
    /// Creates a new multicast group ID
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

/// Level of Detail settings for multicast rooms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LodLevel {
    /// Highest detail - full replication
    Ultra = 0,
    /// High detail - most properties replicated
    High = 1,
    /// Medium detail - important properties only
    Medium = 2,
    /// Low detail - basic properties only
    Low = 3,
    /// Minimal detail - essential properties only
    Minimal = 4,
}

impl LodLevel {
    /// Gets the replication radius for this LOD level
    pub fn radius(&self) -> f32 {
        match self {
            LodLevel::Ultra => 50.0,
            LodLevel::High => 150.0,
            LodLevel::Medium => 300.0,
            LodLevel::Low => 600.0,
            LodLevel::Minimal => 1200.0,
        }
    }

    /// Gets the update frequency for this LOD level
    pub fn frequency(&self) -> f32 {
        match self {
            LodLevel::Ultra => 60.0,
            LodLevel::High => 30.0,
            LodLevel::Medium => 15.0,
            LodLevel::Low => 7.5,
            LodLevel::Minimal => 3.0,
        }
    }

    /// Converts LOD level to replication priority
    pub fn to_priority(&self) -> ReplicationPriority {
        match self {
            LodLevel::Ultra => ReplicationPriority::Critical,
            LodLevel::High => ReplicationPriority::High,
            LodLevel::Medium => ReplicationPriority::Normal,
            LodLevel::Low => ReplicationPriority::Low,
            LodLevel::Minimal => ReplicationPriority::Low,
        }
    }
}

/// A multicast group for efficient data distribution
#[derive(Debug, Clone)]
pub struct MulticastGroup {
    /// Unique identifier for this group
    pub id: MulticastGroupId,
    /// Name of the group
    pub name: String,
    /// Members of this multicast group
    pub members: HashSet<PlayerId>,
    /// Replication channels this group handles
    pub channels: HashSet<u8>,
    /// Priority level for this group
    pub priority: ReplicationPriority,
    /// Maximum number of members
    pub max_members: usize,
    /// Geographic bounds for this group (optional)
    pub bounds: Option<GroupBounds>,
    /// Creation timestamp
    pub created_at: Instant,
    /// Last update timestamp
    pub last_update: Option<Instant>,
    /// Group statistics
    pub stats: GroupStats,
}

impl MulticastGroup {
    /// Creates a new multicast group
    pub fn new(name: String, channels: HashSet<u8>, priority: ReplicationPriority) -> Self {
        Self {
            id: MulticastGroupId::new(),
            name,
            members: HashSet::new(),
            channels,
            priority,
            max_members: 1000, // Default max members
            bounds: None,
            created_at: Instant::now(),
            last_update: None,
            stats: GroupStats::default(),
        }
    }

    /// Adds a member to the group
    pub fn add_member(&mut self, player_id: PlayerId) -> bool {
        if self.members.len() < self.max_members {
            let added = self.members.insert(player_id);
            if added {
                self.stats.member_additions += 1;
                self.last_update = Some(Instant::now());
            }
            added
        } else {
            false
        }
    }

    /// Removes a member from the group
    pub fn remove_member(&mut self, player_id: PlayerId) -> bool {
        let removed = self.members.remove(&player_id);
        if removed {
            self.stats.member_removals += 1;
            self.last_update = Some(Instant::now());
        }
        removed
    }

    /// Checks if the group contains a member
    pub fn contains_member(&self, player_id: PlayerId) -> bool {
        self.members.contains(&player_id)
    }

    /// Gets the current member count
    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    /// Checks if the group is full
    pub fn is_full(&self) -> bool {
        self.members.len() >= self.max_members
    }

    /// Checks if the group is empty
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// Sets geographic bounds for this group
    pub fn set_bounds(&mut self, bounds: GroupBounds) {
        self.bounds = Some(bounds);
    }

    /// Checks if a position is within the group's bounds
    pub fn contains_position(&self, position: Position) -> bool {
        if let Some(bounds) = &self.bounds {
            bounds.contains(position)
        } else {
            true // No bounds means infinite bounds
        }
    }

    /// Records a message broadcast to this group
    pub fn record_broadcast(&mut self, bytes_sent: usize) {
        self.stats.messages_sent += 1;
        self.stats.bytes_sent += bytes_sent as u64;
        self.last_update = Some(Instant::now());
    }
}

/// Geographic bounds for multicast groups
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupBounds {
    /// Center position of the group
    pub center: Position,
    /// Radius from center
    pub radius: f32,
    /// Minimum bounds (optional for rectangular bounds)
    pub min_bounds: Option<Position>,
    /// Maximum bounds (optional for rectangular bounds)
    pub max_bounds: Option<Position>,
}

impl GroupBounds {
    /// Creates circular bounds around a center point
    pub fn circular(center: Position, radius: f32) -> Self {
        Self {
            center,
            radius,
            min_bounds: None,
            max_bounds: None,
        }
    }

    /// Creates rectangular bounds
    pub fn rectangular(min: Position, max: Position) -> Self {
        let center = Position::new(
            (min.x + max.x) / 2.0,
            (min.y + max.y) / 2.0,
            (min.z + max.z) / 2.0,
        );
        
        Self {
            center,
            radius: 0.0, // Not used for rectangular bounds
            min_bounds: Some(min),
            max_bounds: Some(max),
        }
    }

    /// Checks if a position is within these bounds
    pub fn contains(&self, position: Position) -> bool {
        if let (Some(min), Some(max)) = (&self.min_bounds, &self.max_bounds) {
            // Rectangular bounds
            position.x >= min.x && position.x <= max.x &&
            position.y >= min.y && position.y <= max.y &&
            position.z >= min.z && position.z <= max.z
        } else {
            // Circular bounds
            let dx = position.x - self.center.x;
            let dy = position.y - self.center.y;
            let dz = position.z - self.center.z;
            let distance = ((dx * dx + dy * dy + dz * dz) as f32).sqrt();
            distance <= self.radius
        }
    }
}

/// Statistics for a multicast group
#[derive(Debug, Clone, Default)]
pub struct GroupStats {
    /// Number of messages sent to this group
    pub messages_sent: u64,
    /// Total bytes sent to this group
    pub bytes_sent: u64,
    /// Number of member additions
    pub member_additions: u64,
    /// Number of member removals
    pub member_removals: u64,
    /// Average message size
    pub avg_message_size: f64,
}

/// LOD-based multicast room for spatial replication
#[derive(Debug)]
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
        let threshold = if self.members.contains(&PlayerId::new()) { // Simplified check
            base_radius + self.hysteresis.exit_threshold
        } else {
            base_radius - self.hysteresis.enter_threshold
        };
        
        if distance <= threshold {
            self.lod_level
        } else {
            // Find the next appropriate LOD level
            match self.lod_level {
                LodLevel::Ultra => LodLevel::High,
                LodLevel::High => LodLevel::Medium,
                LodLevel::Medium => LodLevel::Low,
                LodLevel::Low => LodLevel::Minimal,
                LodLevel::Minimal => LodLevel::Minimal,
            }
        }
    }

    /// Calculates distance from room center
    fn distance_from_center(&self, position: Position) -> f32 {
        let dx = position.x - self.center.x;
        let dy = position.y - self.center.y;
        let dz = position.z - self.center.z;
        ((dx * dx + dy * dy + dz * dz) as f32).sqrt()
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
        let target_lod = self.get_lod_for_position(new_position);
        
        // Check if member needs to be moved between rooms
        let current_room = self.find_member_room(player_id);
        
        if let Some(current_lod) = current_room {
            if current_lod != target_lod {
                // Move member to new room
                self.remove_member(player_id);
                self.add_member(player_id, new_position);
            }
        } else {
            // Member not found, add them
            self.add_member(player_id, new_position);
        }
    }

    /// Finds which room a member is currently in
    fn find_member_room(&self, player_id: PlayerId) -> Option<LodLevel> {
        if self.members.contains(&player_id) {
            return Some(self.lod_level);
        }
        
        for room in self.nested_rooms.values() {
            if let Some(lod) = room.find_member_room(player_id) {
                return Some(lod);
            }
        }
        
        None
    }
}

/// Hysteresis settings for smooth LOD transitions
#[derive(Debug, Clone)]
pub struct HysteresisSettings {
    /// Additional distance before entering a higher LOD
    pub enter_threshold: f32,
    /// Additional distance before exiting to lower LOD
    pub exit_threshold: f32,
    /// Minimum time between LOD changes for the same player
    pub min_change_interval: Duration,
}

impl Default for HysteresisSettings {
    fn default() -> Self {
        Self {
            enter_threshold: 5.0,
            exit_threshold: 15.0,
            min_change_interval: Duration::from_millis(500),
        }
    }
}

/// Statistics for LOD rooms
#[derive(Debug, Clone, Default)]
pub struct RoomStats {
    /// Current number of members
    pub member_count: usize,
    /// Number of LOD transitions
    pub lod_transitions: u64,
    /// Average update frequency achieved
    pub avg_update_frequency: f32,
    /// Total messages sent from this room
    pub messages_sent: u64,
}

/// Main multicast manager coordinating all multicast operations
#[derive(Debug)]
pub struct MulticastManager {
    /// All active multicast groups
    groups: Arc<RwLock<HashMap<MulticastGroupId, MulticastGroup>>>,
    /// LOD-based spatial rooms
    lod_rooms: Arc<RwLock<HashMap<MulticastGroupId, LodRoom>>>,
    /// Player to group mappings
    player_groups: Arc<RwLock<HashMap<PlayerId, HashSet<MulticastGroupId>>>>,
    /// Channel to group mappings
    channel_groups: Arc<RwLock<HashMap<u8, HashSet<MulticastGroupId>>>>,
    /// Global multicast statistics
    stats: Arc<RwLock<MulticastStats>>,
}

impl MulticastManager {
    /// Creates a new multicast manager
    pub fn new() -> Self {
        Self {
            groups: Arc::new(RwLock::new(HashMap::new())),
            lod_rooms: Arc::new(RwLock::new(HashMap::new())),
            player_groups: Arc::new(RwLock::new(HashMap::new())),
            channel_groups: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(MulticastStats::default())),
        }
    }

    /// Creates a new multicast group
    pub async fn create_group(
        &self,
        name: String,
        channels: HashSet<u8>,
        priority: ReplicationPriority,
    ) -> MulticastGroupId {
        let group = MulticastGroup::new(name, channels.clone(), priority);
        let group_id = group.id;
        
        // Add to groups
        let mut groups = self.groups.write().await;
        groups.insert(group_id, group);
        
        // Update channel mappings
        let mut channel_groups = self.channel_groups.write().await;
        for channel in channels {
            channel_groups.entry(channel).or_insert_with(HashSet::new).insert(group_id);
        }
        
        // Update statistics
        let mut stats = self.stats.write().await;
        stats.total_groups += 1;
        
        group_id
    }

    /// Creates a new LOD room
    pub async fn create_lod_room(&self, center: Position, lod_level: LodLevel) -> MulticastGroupId {
        let room = LodRoom::new(center, lod_level);
        let room_id = room.id;
        
        let mut lod_rooms = self.lod_rooms.write().await;
        lod_rooms.insert(room_id, room);
        
        let mut stats = self.stats.write().await;
        stats.lod_rooms += 1;
        
        room_id
    }

    /// Adds a player to a multicast group
    pub async fn add_player_to_group(&self, player_id: PlayerId, group_id: MulticastGroupId) -> bool {
        let mut groups = self.groups.write().await;
        if let Some(group) = groups.get_mut(&group_id) {
            if group.add_member(player_id) {
                // Update player mappings
                drop(groups);
                let mut player_groups = self.player_groups.write().await;
                player_groups.entry(player_id).or_insert_with(HashSet::new).insert(group_id);
                
                let mut stats = self.stats.write().await;
                stats.total_subscriptions += 1;
                return true;
            }
        }
        false
    }

    /// Removes a player from a multicast group
    pub async fn remove_player_from_group(&self, player_id: PlayerId, group_id: MulticastGroupId) -> bool {
        let mut groups = self.groups.write().await;
        if let Some(group) = groups.get_mut(&group_id) {
            if group.remove_member(player_id) {
                // Update player mappings
                drop(groups);
                let mut player_groups = self.player_groups.write().await;
                if let Some(player_group_set) = player_groups.get_mut(&player_id) {
                    player_group_set.remove(&group_id);
                    if player_group_set.is_empty() {
                        player_groups.remove(&player_id);
                    }
                }
                
                let mut stats = self.stats.write().await;
                stats.total_subscriptions = stats.total_subscriptions.saturating_sub(1);
                return true;
            }
        }
        false
    }

    /// Broadcasts data to a multicast group
    pub async fn broadcast_to_group(
        &self,
        group_id: MulticastGroupId,
        data: &[u8],
        channel: u8,
    ) -> Result<usize, MulticastError> {
        let mut groups = self.groups.write().await;
        if let Some(group) = groups.get_mut(&group_id) {
            if group.channels.contains(&channel) {
                let member_count = group.member_count();
                group.record_broadcast(data.len());
                
                // In a real implementation, this would send data via the actual network layer
                // For now, we just record the statistics
                
                let mut stats = self.stats.write().await;
                stats.messages_sent += 1;
                stats.bytes_sent += data.len() as u64 * member_count as u64;
                
                Ok(member_count)
            } else {
                Err(MulticastError::ChannelNotSupported(channel))
            }
        } else {
            Err(MulticastError::GroupNotFound(group_id))
        }
    }

    /// Gets groups for a specific channel
    pub async fn get_groups_for_channel(&self, channel: u8) -> Vec<MulticastGroupId> {
        let channel_groups = self.channel_groups.read().await;
        channel_groups.get(&channel)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Updates player position in LOD rooms
    pub async fn update_player_position_in_lod(&self, player_id: PlayerId, position: Position) {
        let mut lod_rooms = self.lod_rooms.write().await;
        for room in lod_rooms.values_mut() {
            room.update_member_position(player_id, position);
        }
    }

    /// Gets current multicast statistics
    pub async fn get_stats(&self) -> MulticastStats {
        self.stats.read().await.clone()
    }

    /// Removes empty groups to free up resources
    pub async fn cleanup_empty_groups(&self) {
        let mut groups = self.groups.write().await;
        let empty_groups: Vec<MulticastGroupId> = groups
            .iter()
            .filter(|(_, group)| group.is_empty())
            .map(|(&id, _)| id)
            .collect();
        
        for group_id in empty_groups {
            groups.remove(&group_id);
            
            // Update channel mappings
            let mut channel_groups = self.channel_groups.write().await;
            for group_set in channel_groups.values_mut() {
                group_set.remove(&group_id);
            }
        }
        
        let mut stats = self.stats.write().await;
        stats.total_groups = groups.len();
    }
}

impl Default for MulticastManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors that can occur during multicast operations
#[derive(Debug, thiserror::Error)]
pub enum MulticastError {
    /// Group not found
    #[error("Multicast group not found: {0:?}")]
    GroupNotFound(MulticastGroupId),
    /// Channel not supported by group
    #[error("Channel {0} not supported by group")]
    ChannelNotSupported(u8),
    /// Group is full
    #[error("Multicast group is full")]
    GroupFull,
    /// Player not in group
    #[error("Player not in multicast group")]
    PlayerNotInGroup,
}

/// Global multicast statistics
#[derive(Debug, Clone, Default)]
pub struct MulticastStats {
    /// Total number of multicast groups
    pub total_groups: usize,
    /// Total number of LOD rooms
    pub lod_rooms: usize,
    /// Total number of active subscriptions
    pub total_subscriptions: usize,
    /// Total messages sent via multicast
    pub messages_sent: u64,
    /// Total bytes sent via multicast
    pub bytes_sent: u64,
    /// Average group size
    pub avg_group_size: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multicast_group_creation() {
        let channels = vec![0, 1].into_iter().collect();
        let mut group = MulticastGroup::new(
            "test_group".to_string(),
            channels,
            ReplicationPriority::High,
        );
        
        assert_eq!(group.name, "test_group");
        assert_eq!(group.priority, ReplicationPriority::High);
        assert_eq!(group.member_count(), 0);
        assert!(group.is_empty());
        
        let player_id = PlayerId::new();
        assert!(group.add_member(player_id));
        assert_eq!(group.member_count(), 1);
        assert!(!group.is_empty());
    }

    #[test]
    fn test_group_bounds() {
        let center = Position::new(100.0, 100.0, 100.0);
        let bounds = GroupBounds::circular(center, 50.0);
        
        assert!(bounds.contains(Position::new(110.0, 110.0, 110.0)));
        assert!(!bounds.contains(Position::new(200.0, 200.0, 200.0)));
        
        let rect_bounds = GroupBounds::rectangular(
            Position::new(0.0, 0.0, 0.0),
            Position::new(100.0, 100.0, 100.0),
        );
        
        assert!(rect_bounds.contains(Position::new(50.0, 50.0, 50.0)));
        assert!(!rect_bounds.contains(Position::new(150.0, 50.0, 50.0)));
    }

    #[test]
    fn test_lod_levels() {
        assert_eq!(LodLevel::Ultra.radius(), 50.0);
        assert_eq!(LodLevel::High.frequency(), 30.0);
        assert_eq!(LodLevel::Low.to_priority(), ReplicationPriority::Low);
    }

    #[test]
    fn test_lod_room() {
        let mut room = LodRoom::new(Position::new(0.0, 0.0, 0.0), LodLevel::High);
        let player_id = PlayerId::new();
        
        room.add_member(player_id, Position::new(10.0, 10.0, 10.0));
        assert_eq!(room.stats.member_count, 1);
        
        room.remove_member(player_id);
        assert_eq!(room.stats.member_count, 0);
    }

    #[tokio::test]
    async fn test_multicast_manager() {
        let manager = MulticastManager::new();
        let channels = vec![0, 1].into_iter().collect();
        
        let group_id = manager.create_group(
            "test".to_string(),
            channels,
            ReplicationPriority::Normal,
        ).await;
        
        let player_id = PlayerId::new();
        assert!(manager.add_player_to_group(player_id, group_id).await);
        
        let data = b"test message";
        let result = manager.broadcast_to_group(group_id, data, 0).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1); // One member received the message
    }
}