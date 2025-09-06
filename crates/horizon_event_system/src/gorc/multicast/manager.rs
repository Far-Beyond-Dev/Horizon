/// Multicast manager for coordinating groups and LOD rooms
use super::group::MulticastGroup;
use super::lod::LodRoom;
use super::types::{MulticastGroupId, LodLevel, MulticastError, MulticastStats};
use crate::types::{PlayerId, Position};
use crate::gorc::channels::ReplicationPriority;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Central manager for all multicast operations
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
        stats.groups_created += 1;
        
        group_id
    }

    /// Creates a new LOD room
    pub async fn create_lod_room(&self, center: Position, lod_level: LodLevel) -> MulticastGroupId {
        let room = LodRoom::new(center, lod_level);
        let room_id = room.id;
        
        let mut lod_rooms = self.lod_rooms.write().await;
        lod_rooms.insert(room_id, room);
        
        let mut stats = self.stats.write().await;
        stats.total_groups += 1; // LOD rooms count as groups
        
        room_id
    }

    /// Creates a hierarchical LOD room with multiple levels
    pub async fn create_hierarchical_lod_room(&self, center: Position, base_lod: LodLevel) -> MulticastGroupId {
        let room = LodRoom::with_nested_levels(center, base_lod);
        let room_id = room.id;
        
        let mut lod_rooms = self.lod_rooms.write().await;
        lod_rooms.insert(room_id, room);
        
        let mut stats = self.stats.write().await;
        stats.total_groups += 1;
        
        room_id
    }

    /// Adds a player to a multicast group
    pub async fn add_player_to_group(&self, player_id: PlayerId, group_id: MulticastGroupId) -> Result<bool, MulticastError> {
        let mut groups = self.groups.write().await;
        if let Some(group) = groups.get_mut(&group_id) {
            match group.add_member(player_id) {
                Ok(true) => {
                    // Update player mappings
                    drop(groups);
                    let mut player_groups = self.player_groups.write().await;
                    player_groups.entry(player_id).or_insert_with(HashSet::new).insert(group_id);
                    
                    let mut stats = self.stats.write().await;
                    stats.total_players += 1;
                    Ok(true)
                }
                Ok(false) => Ok(false), // Player already in group
                Err(e) => Err(e),
            }
        } else {
            Err(MulticastError::GroupNotFound { id: group_id })
        }
    }

    /// Removes a player from a multicast group
    pub async fn remove_player_from_group(&self, player_id: PlayerId, group_id: MulticastGroupId) -> Result<bool, MulticastError> {
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
                stats.total_players = stats.total_players.saturating_sub(1);
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Err(MulticastError::GroupNotFound { id: group_id })
        }
    }

    /// Adds a player to a LOD room based on their position
    pub async fn add_player_to_lod_room(&self, player_id: PlayerId, room_id: MulticastGroupId, position: Position) -> Result<(), MulticastError> {
        let mut lod_rooms = self.lod_rooms.write().await;
        if let Some(room) = lod_rooms.get_mut(&room_id) {
            room.add_member(player_id, position);
            
            // Update player mappings
            drop(lod_rooms);
            let mut player_groups = self.player_groups.write().await;
            player_groups.entry(player_id).or_insert_with(HashSet::new).insert(room_id);
            
            Ok(())
        } else {
            Err(MulticastError::GroupNotFound { id: room_id })
        }
    }

    /// Updates a player's position in LOD rooms
    pub async fn update_player_position_in_lod_rooms(&self, player_id: PlayerId, new_position: Position) -> Result<(), MulticastError> {
        let mut lod_rooms = self.lod_rooms.write().await;
        
        // Update position in all LOD rooms the player is in
        let player_groups = self.player_groups.read().await;
        if let Some(group_ids) = player_groups.get(&player_id) {
            for group_id in group_ids {
                if let Some(room) = lod_rooms.get_mut(group_id) {
                    room.update_member_position(player_id, new_position);
                }
            }
        }
        
        Ok(())
    }

    /// Removes a player from all groups and rooms
    pub async fn remove_player(&self, player_id: PlayerId) -> Result<usize, MulticastError> {
        let mut removed_count = 0;
        
        // Get all groups the player is in
        let player_groups = {
            let player_groups_lock = self.player_groups.read().await;
            player_groups_lock.get(&player_id).cloned().unwrap_or_default()
        };
        
        // Remove from all groups
        for group_id in player_groups {
            if self.remove_player_from_group(player_id, group_id).await? {
                removed_count += 1;
            }
        }
        
        // Remove from LOD rooms
        let mut lod_rooms = self.lod_rooms.write().await;
        for room in lod_rooms.values_mut() {
            room.remove_member(player_id);
        }
        
        // Clear player mappings
        let mut player_groups_lock = self.player_groups.write().await;
        player_groups_lock.remove(&player_id);
        
        Ok(removed_count)
    }

    /// Gets all groups a player is subscribed to
    pub async fn get_player_groups(&self, player_id: PlayerId) -> Vec<MulticastGroupId> {
        let player_groups = self.player_groups.read().await;
        player_groups.get(&player_id).map(|set| set.iter().copied().collect()).unwrap_or_default()
    }

    /// Gets all groups handling a specific channel
    pub async fn get_groups_for_channel(&self, channel: u8) -> Vec<MulticastGroupId> {
        let channel_groups = self.channel_groups.read().await;
        channel_groups.get(&channel).map(|set| set.iter().copied().collect()).unwrap_or_default()
    }

    /// Broadcasts data to all members of a group
    pub async fn broadcast_to_group(&self, group_id: MulticastGroupId, data: &[u8]) -> Result<usize, MulticastError> {
        let mut groups = self.groups.write().await;
        if let Some(group) = groups.get_mut(&group_id) {
            let member_count = group.member_count();
            group.record_broadcast(data.len());
            
            // Update statistics
            drop(groups);
            let mut stats = self.stats.write().await;
            stats.messages_sent += 1;
            stats.bytes_sent += data.len() as u64;
            
            Ok(member_count)
        } else {
            Err(MulticastError::GroupNotFound { id: group_id })
        }
    }

    /// Destroys a multicast group
    pub async fn destroy_group(&self, group_id: MulticastGroupId) -> Result<(), MulticastError> {
        // Remove from groups
        let group = {
            let mut groups = self.groups.write().await;
            groups.remove(&group_id)
        };
        
        if let Some(group) = group {
            // Remove from channel mappings
            let mut channel_groups = self.channel_groups.write().await;
            for channel in &group.channels {
                if let Some(channel_set) = channel_groups.get_mut(channel) {
                    channel_set.remove(&group_id);
                    if channel_set.is_empty() {
                        channel_groups.remove(channel);
                    }
                }
            }
            
            // Remove from player mappings
            let mut player_groups = self.player_groups.write().await;
            for member in &group.members {
                if let Some(player_group_set) = player_groups.get_mut(member) {
                    player_group_set.remove(&group_id);
                    if player_group_set.is_empty() {
                        player_groups.remove(member);
                    }
                }
            }
            
            // Update statistics
            let mut stats = self.stats.write().await;
            stats.total_groups = stats.total_groups.saturating_sub(1);
            stats.groups_destroyed += 1;
            stats.total_players = stats.total_players.saturating_sub(group.members.len());
            
            Ok(())
        } else {
            // Try LOD rooms
            let mut lod_rooms = self.lod_rooms.write().await;
            if lod_rooms.remove(&group_id).is_some() {
                let mut stats = self.stats.write().await;
                stats.total_groups = stats.total_groups.saturating_sub(1);
                Ok(())
            } else {
                Err(MulticastError::GroupNotFound { id: group_id })
            }
        }
    }

    /// Optimizes all groups and rooms by removing empty ones
    pub async fn optimize(&self) -> usize {
        let mut removed_count = 0;
        
        // Optimize regular groups
        let empty_groups: Vec<MulticastGroupId> = {
            let groups = self.groups.read().await;
            groups.iter()
                .filter(|(_, group)| group.is_empty())
                .map(|(id, _)| *id)
                .collect()
        };
        
        for group_id in empty_groups {
            if self.destroy_group(group_id).await.is_ok() {
                removed_count += 1;
            }
        }
        
        // Optimize LOD rooms
        let mut lod_rooms = self.lod_rooms.write().await;
        for room in lod_rooms.values_mut() {
            room.optimize();
        }
        
        removed_count
    }

    /// Gets comprehensive multicast statistics
    pub async fn get_stats(&self) -> MulticastStats {
        let mut stats = self.stats.read().await.clone();
        
        // Update calculated fields
        if stats.total_groups > 0 {
            stats.avg_group_size = stats.total_players as f64 / stats.total_groups as f64;
        }
        
        stats
    }
}

impl Default for MulticastManager {
    fn default() -> Self {
        Self::new()
    }
}