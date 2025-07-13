/// Multicast group management
use super::types::{MulticastGroupId, MulticastError};
use crate::types::{PlayerId, Position, Vec3};
use crate::gorc::channels::ReplicationPriority;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tokio::time::Instant;

/// A multicast group for efficient data distribution
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

    /// Creates a new group with custom capacity
    pub fn with_capacity(name: String, channels: HashSet<u8>, priority: ReplicationPriority, max_members: usize) -> Self {
        let mut group = Self::new(name, channels, priority);
        group.max_members = max_members;
        group
    }

    /// Adds a member to the group
    pub fn add_member(&mut self, player_id: PlayerId) -> Result<bool, MulticastError> {
        if self.members.len() >= self.max_members {
            return Err(MulticastError::GroupCapacityExceeded);
        }

        let added = self.members.insert(player_id);
        if added {
            self.stats.member_additions += 1;
            self.last_update = Some(Instant::now());
        }
        Ok(added)
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

    /// Gets the age of this group
    pub fn age(&self) -> std::time::Duration {
        self.created_at.elapsed()
    }

    /// Gets time since last update
    pub fn time_since_update(&self) -> Option<std::time::Duration> {
        self.last_update.map(|t| t.elapsed())
    }

    /// Checks if the group handles a specific channel
    pub fn handles_channel(&self, channel: u8) -> bool {
        self.channels.contains(&channel)
    }

    /// Adds a channel to this group
    pub fn add_channel(&mut self, channel: u8) {
        self.channels.insert(channel);
    }

    /// Removes a channel from this group
    pub fn remove_channel(&mut self, channel: u8) -> bool {
        self.channels.remove(&channel)
    }

    /// Gets all members as a vector
    pub fn get_members(&self) -> Vec<PlayerId> {
        self.members.iter().copied().collect()
    }

    /// Clears all members from the group
    pub fn clear_members(&mut self) {
        let removed_count = self.members.len();
        self.members.clear();
        self.stats.member_removals += removed_count as u64;
        self.last_update = Some(Instant::now());
    }
}

/// Geographic bounds for a multicast group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupBounds {
    /// Center point of the bounds
    pub center: Vec3,
    /// Radius of the bounds
    pub radius: f32,
    /// Minimum bounds (optional box bounds)
    pub min_bounds: Option<Vec3>,
    /// Maximum bounds (optional box bounds)
    pub max_bounds: Option<Vec3>,
}

impl GroupBounds {
    /// Creates circular bounds
    pub fn circular(center: Vec3, radius: f32) -> Self {
        Self {
            center,
            radius,
            min_bounds: None,
            max_bounds: None,
        }
    }

    /// Creates rectangular bounds
    pub fn rectangular(min: Vec3, max: Vec3) -> Self {
        let center = Vec3::new(
            (min.x + max.x) / 2.0,
            (min.y + max.y) / 2.0,
            (min.z + max.z) / 2.0,
        );
        let radius = center.distance(max);

        Self {
            center,
            radius,
            min_bounds: Some(min),
            max_bounds: Some(max),
        }
    }

    /// Checks if a position is within these bounds
    pub fn contains(&self, position: Position) -> bool {
        let pos: Vec3 = position.into();
        
        // Check circular bounds
        if self.center.distance(pos) > self.radius {
            return false;
        }

        // Check rectangular bounds if present
        if let (Some(min), Some(max)) = (&self.min_bounds, &self.max_bounds) {
            return pos.x >= min.x && pos.x <= max.x &&
                   pos.y >= min.y && pos.y <= max.y &&
                   pos.z >= min.z && pos.z <= max.z;
        }

        true
    }

    /// Gets the area/volume of these bounds
    pub fn area(&self) -> f32 {
        if let (Some(min), Some(max)) = (&self.min_bounds, &self.max_bounds) {
            // Rectangular volume
            (max.x - min.x) * (max.y - min.y) * (max.z - min.z)
        } else {
            // Spherical volume
            (4.0 / 3.0) * std::f32::consts::PI * self.radius.powi(3)
        }
    }

    /// Expands the bounds by a factor
    pub fn expand(&mut self, factor: f32) {
        self.radius *= factor;
        if let (Some(min), Some(max)) = (&mut self.min_bounds, &mut self.max_bounds) {
            let expansion = (factor - 1.0) / 2.0;
            let size = Vec3::new(
                max.x - min.x,
                max.y - min.y,
                max.z - min.z,
            );
            let expansion_vec = Vec3::new(
                size.x * expansion,
                size.y * expansion,
                size.z * expansion,
            );
            *min = Vec3::new(
                min.x - expansion_vec.x,
                min.y - expansion_vec.y,
                min.z - expansion_vec.z,
            );
            *max = Vec3::new(
                max.x + expansion_vec.x,
                max.y + expansion_vec.y,
                max.z + expansion_vec.z,
            );
        }
    }
}

/// Statistics for a multicast group
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct GroupStats {
    /// Number of member additions
    pub member_additions: u64,
    /// Number of member removals
    pub member_removals: u64,
    /// Number of messages sent to this group
    pub messages_sent: u64,
    /// Total bytes sent to this group
    pub bytes_sent: u64,
    /// Peak member count
    pub peak_member_count: usize,
}

impl GroupStats {
    /// Updates peak member count
    pub fn update_peak_members(&mut self, current_count: usize) {
        if current_count > self.peak_member_count {
            self.peak_member_count = current_count;
        }
    }

    /// Gets member churn rate (additions + removals)
    pub fn churn_rate(&self) -> u64 {
        self.member_additions + self.member_removals
    }

    /// Gets average message size
    pub fn avg_message_size(&self) -> f32 {
        if self.messages_sent > 0 {
            self.bytes_sent as f32 / self.messages_sent as f32
        } else {
            0.0
        }
    }
}