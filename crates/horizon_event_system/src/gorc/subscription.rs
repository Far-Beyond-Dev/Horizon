//! # Subscription Management System
//!
//! This module handles dynamic subscription management for GORC, including
//! proximity-based, relationship-based, and interest-based subscriptions.

use crate::types::{PlayerId, Position};
use crate::gorc::channels::ReplicationPriority;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::Duration;

// Constants for interest-based subscription calculation
const CRITICAL_CHANNEL: u8 = 0;
const FREQUENCY_THRESHOLD: f32 = 0.8;

/// Types of subscription relationships
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SubscriptionType {
    /// Subscription based on spatial proximity
    Proximity,
    /// Subscription based on relationships (team, guild, etc.)
    Relationship(String),
    /// Subscription based on player interest and activity
    Interest,
}

/// Proximity-based subscription configuration
#[derive(Debug, Clone)]
pub struct ProximitySubscription {
    /// Player's current position
    pub position: Position,
    /// Subscription radius per channel
    pub channel_radii: HashMap<u8, f32>,
    /// Last position update time (using std::time::Instant since it's not serializable)
    pub last_update: Option<std::time::Instant>,
    /// Movement threshold to trigger subscription updates
    pub movement_threshold: f32,
}

impl ProximitySubscription {
    /// Creates a new proximity subscription
    pub fn new(position: Position) -> Self {
        let mut channel_radii = HashMap::new();
        // Default radii for each channel
        channel_radii.insert(0, 100.0); // Critical - close range
        channel_radii.insert(1, 250.0); // Detailed - medium range
        channel_radii.insert(2, 500.0); // Cosmetic - long range
        channel_radii.insert(3, 1000.0); // Metadata - very long range

        Self {
            position,
            channel_radii,
            last_update: Some(std::time::Instant::now()),
            movement_threshold: 5.0,
        }
    }

    /// Updates position and returns whether subscriptions should be recalculated
    pub fn update_position(&mut self, new_position: Position) -> bool {
        let distance_moved = Self::calculate_distance(self.position, new_position);
        
        if distance_moved >= self.movement_threshold {
            self.position = new_position;
            self.last_update = Some(std::time::Instant::now());
            true
        } else {
            false
        }
    }

    /// Calculates distance between two positions
    fn calculate_distance(pos1: Position, pos2: Position) -> f32 {
        let dx = pos1.x - pos2.x;
        let dy = pos1.y - pos2.y;
        let dz = pos1.z - pos2.z;
        ((dx * dx + dy * dy + dz * dz) as f32).sqrt()
    }

    /// Checks if another position is within subscription range for a channel
    pub fn is_in_range(&self, other_position: Position, channel: u8) -> bool {
        if let Some(&radius) = self.channel_radii.get(&channel) {
            Self::calculate_distance(self.position, other_position) <= radius
        } else {
            false
        }
    }
}

/// Relationship-based subscription (teams, guilds, friends, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipSubscription {
    /// Type of relationship
    pub relationship_type: String,
    /// Related players
    pub related_players: HashSet<PlayerId>,
    /// Channel priorities for this relationship
    pub channel_priorities: HashMap<u8, ReplicationPriority>,
    /// Maximum distance for relationship-based updates
    pub max_distance: Option<f32>,
}

impl RelationshipSubscription {
    /// Creates a new relationship subscription
    pub fn new(relationship_type: String) -> Self {
        let mut channel_priorities = HashMap::new();
        
        // Default priorities based on relationship type
        match relationship_type.as_str() {
            "team" => {
                // Team members get high priority on all channels
                channel_priorities.insert(0, ReplicationPriority::Critical);
                channel_priorities.insert(1, ReplicationPriority::High);
                channel_priorities.insert(2, ReplicationPriority::Normal);
                channel_priorities.insert(3, ReplicationPriority::High);
            }
            "guild" => {
                // Guild members get medium priority
                channel_priorities.insert(0, ReplicationPriority::High);
                channel_priorities.insert(1, ReplicationPriority::Normal);
                channel_priorities.insert(2, ReplicationPriority::Low);
                channel_priorities.insert(3, ReplicationPriority::Normal);
            }
            "friend" => {
                // Friends get normal priority with enhanced metadata
                channel_priorities.insert(0, ReplicationPriority::Normal);
                channel_priorities.insert(1, ReplicationPriority::Normal);
                channel_priorities.insert(2, ReplicationPriority::Low);
                channel_priorities.insert(3, ReplicationPriority::High);
            }
            _ => {
                // Default low priority for unknown relationships
                for channel in 0..4 {
                    channel_priorities.insert(channel, ReplicationPriority::Low);
                }
            }
        }

        Self {
            relationship_type,
            related_players: HashSet::new(),
            channel_priorities,
            max_distance: None,
        }
    }

    /// Adds a player to this relationship
    pub fn add_player(&mut self, player_id: PlayerId) {
        self.related_players.insert(player_id);
    }

    /// Removes a player from this relationship
    pub fn remove_player(&mut self, player_id: PlayerId) {
        self.related_players.remove(&player_id);
    }

    /// Checks if a player is part of this relationship
    pub fn contains_player(&self, player_id: PlayerId) -> bool {
        self.related_players.contains(&player_id)
    }

    /// Gets the priority for a specific channel
    pub fn get_channel_priority(&self, channel: u8) -> ReplicationPriority {
        self.channel_priorities.get(&channel)
            .copied()
            .unwrap_or(ReplicationPriority::Low)
    }
}

/// Interest-based subscription tracking player focus and activity
#[derive(Debug, Clone)]
pub struct InterestSubscription {
    /// Objects the player has shown interest in
    pub interested_objects: HashMap<String, InterestLevel>,
    /// Activity patterns (frequency of interactions with object types)
    pub activity_patterns: HashMap<String, ActivityPattern>,
    /// Focus point (where the player is looking/interacting)
    pub focus_position: Option<Position>,
    /// Focus radius
    pub focus_radius: f32,
    /// Last activity timestamp (using std::time::Instant)
    pub last_activity: Option<std::time::Instant>,
}

/// Level of interest in an object or area
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum InterestLevel {
    /// No interest
    None,
    /// Low interest
    Low,
    /// Medium interest
    Medium,
    /// High interest
    High,
    /// Very high interest (actively interacting)
    VeryHigh,
}

impl InterestLevel {
    /// Converts interest level to replication priority
    pub fn to_priority(self) -> ReplicationPriority {
        match self {
            InterestLevel::None => ReplicationPriority::Low,
            InterestLevel::Low => ReplicationPriority::Low,
            InterestLevel::Medium => ReplicationPriority::Normal,
            InterestLevel::High => ReplicationPriority::High,
            InterestLevel::VeryHigh => ReplicationPriority::Critical,
        }
    }
}

/// Activity pattern tracking for predictive subscriptions
#[derive(Debug, Clone)]
pub struct ActivityPattern {
    /// Number of interactions
    pub interaction_count: u32,
    /// Average interaction duration (in milliseconds as u64 for simplicity)
    pub avg_duration_ms: u64,
    /// Last interaction time (using std::time::Instant)
    pub last_interaction: Option<std::time::Instant>,
    /// Frequency of interactions (interactions per hour)
    pub frequency: f32,
}

impl ActivityPattern {
    /// Creates a new activity pattern
    pub fn new() -> Self {
        Self {
            interaction_count: 0,
            avg_duration_ms: 0,
            last_interaction: None,
            frequency: 0.0,
        }
    }

    /// Records a new interaction
    pub fn record_interaction(&mut self, duration: Duration) {
        self.interaction_count += 1;
        self.last_interaction = Some(std::time::Instant::now());
        
        // Update average duration
        let duration_ms = duration.as_millis() as u64;
        let total_duration = self.avg_duration_ms * (self.interaction_count - 1) as u64 + duration_ms;
        self.avg_duration_ms = total_duration / self.interaction_count as u64;
        
        // Update frequency (simplified calculation)
        self.frequency = self.interaction_count as f32 / 1.0; // interactions per hour (simplified)
    }
}

impl Default for ActivityPattern {
    fn default() -> Self {
        Self::new()
    }
}

impl InterestSubscription {
    /// Creates a new interest subscription
    pub fn new() -> Self {
        Self {
            interested_objects: HashMap::new(),
            activity_patterns: HashMap::new(),
            focus_position: None,
            focus_radius: 50.0,
            last_activity: None,
        }
    }

    /// Updates the player's focus position
    pub fn update_focus(&mut self, position: Position, radius: f32) {
        self.focus_position = Some(position);
        self.focus_radius = radius;
        self.last_activity = Some(std::time::Instant::now());
    }

    /// Records interest in an object
    pub fn record_interest(&mut self, object_id: String, level: InterestLevel) {
        self.interested_objects.insert(object_id, level);
        self.last_activity = Some(std::time::Instant::now());
    }

    /// Records activity with an object type
    pub fn record_activity(&mut self, object_type: String, duration: Duration) {
        self.activity_patterns
            .entry(object_type)
            .or_insert_with(ActivityPattern::new)
            .record_interaction(duration);
        self.last_activity = Some(std::time::Instant::now());
    }

    /// Gets interest level for an object
    pub fn get_interest_level(&self, object_id: &str) -> InterestLevel {
        self.interested_objects.get(object_id)
            .copied()
            .unwrap_or(InterestLevel::None)
    }

    /// Checks if a position is within the current focus area
    pub fn is_in_focus(&self, position: Position) -> bool {
        if let Some(focus_pos) = self.focus_position {
            let dx = focus_pos.x - position.x;
            let dy = focus_pos.y - position.y;
            let dz = focus_pos.z - position.z;
            let distance = ((dx * dx + dy * dy + dz * dz) as f32).sqrt();
            distance <= self.focus_radius
        } else {
            false
        }
    }
}

impl Default for InterestSubscription {
    fn default() -> Self {
        Self::new()
    }
}

/// Main subscription manager coordinating all subscription types
#[derive(Debug)]
pub struct SubscriptionManager {
    /// Proximity-based subscriptions
    proximity_subs: Arc<RwLock<HashMap<PlayerId, ProximitySubscription>>>,
    /// Relationship-based subscriptions
    relationship_subs: Arc<RwLock<HashMap<PlayerId, Vec<RelationshipSubscription>>>>,
    /// Interest-based subscriptions
    interest_subs: Arc<RwLock<HashMap<PlayerId, InterestSubscription>>>,
    /// Player subscription matrix (who subscribes to whom for which channels)
    subscription_matrix: Arc<RwLock<HashMap<PlayerId, HashMap<PlayerId, HashSet<u8>>>>>,
    /// Subscription update statistics
    stats: Arc<RwLock<SubscriptionStats>>,
}

impl SubscriptionManager {
    /// Creates a new subscription manager
    pub fn new() -> Self {
        Self {
            proximity_subs: Arc::new(RwLock::new(HashMap::new())),
            relationship_subs: Arc::new(RwLock::new(HashMap::new())),
            interest_subs: Arc::new(RwLock::new(HashMap::new())),
            subscription_matrix: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(SubscriptionStats::default())),
        }
    }

    /// Adds a player to the subscription system
    pub async fn add_player(&self, player_id: PlayerId, position: Position) {
        let mut proximity_subs = self.proximity_subs.write().await;
        proximity_subs.insert(player_id, ProximitySubscription::new(position));

        let mut interest_subs = self.interest_subs.write().await;
        interest_subs.insert(player_id, InterestSubscription::new());

        let mut relationship_subs = self.relationship_subs.write().await;
        relationship_subs.insert(player_id, Vec::new());

        let mut matrix = self.subscription_matrix.write().await;
        matrix.insert(player_id, HashMap::new());
    }

    /// Removes a player from the subscription system
    pub async fn remove_player(&self, player_id: PlayerId) {
        let mut proximity_subs = self.proximity_subs.write().await;
        proximity_subs.remove(&player_id);

        let mut interest_subs = self.interest_subs.write().await;
        interest_subs.remove(&player_id);

        let mut relationship_subs = self.relationship_subs.write().await;
        relationship_subs.remove(&player_id);

        let mut matrix = self.subscription_matrix.write().await;
        matrix.remove(&player_id);
    }

    /// Updates a player's position and recalculates proximity subscriptions if needed
    pub async fn update_player_position(&self, player_id: PlayerId, position: Position) -> bool {
        let mut proximity_subs = self.proximity_subs.write().await;
        if let Some(sub) = proximity_subs.get_mut(&player_id) {
            if sub.update_position(position) {
                // Position changed significantly, need to recalculate subscriptions
                drop(proximity_subs); // Release the lock before recalculating
                self.recalculate_proximity_subscriptions(player_id).await;
                return true;
            }
        }
        false
    }

    /// Adds a relationship subscription for a player
    pub async fn add_relationship(
        &self,
        player_id: PlayerId,
        relationship_type: String,
        related_players: Vec<PlayerId>,
    ) {
        let mut relationship_subs = self.relationship_subs.write().await;
        if let Some(subs) = relationship_subs.get_mut(&player_id) {
            let mut rel_sub = RelationshipSubscription::new(relationship_type);
            for related_player in related_players {
                rel_sub.add_player(related_player);
            }
            subs.push(rel_sub);
        }
    }

    /// Updates player interest
    pub async fn update_interest(
        &self,
        player_id: PlayerId,
        object_id: String,
        interest_level: InterestLevel,
    ) {
        let mut interest_subs = self.interest_subs.write().await;
        if let Some(sub) = interest_subs.get_mut(&player_id) {
            sub.record_interest(object_id, interest_level);
        }
    }

    /// Gets the combined subscription priority for two players on a specific channel
    pub async fn get_subscription_priority(
        &self,
        subscriber: PlayerId,
        target: PlayerId,
        channel: u8,
    ) -> ReplicationPriority {
        let proximity_priority = self.get_proximity_priority(subscriber, target, channel).await;
        let relationship_priority = self.get_relationship_priority(subscriber, target, channel).await;
        let interest_priority = self.get_interest_priority(subscriber, target, channel).await;

        // Return the highest priority
        [proximity_priority, relationship_priority, interest_priority]
            .iter()
            .min()
            .copied()
            .unwrap_or(ReplicationPriority::Low)
    }

    /// Recalculates proximity subscriptions for a player
    async fn recalculate_proximity_subscriptions(&self, _player_id: PlayerId) {
        // This would implement efficient spatial queries to find nearby players
        // and update the subscription matrix accordingly
        let mut stats = self.stats.write().await;
        stats.proximity_recalculations += 1;
    }

    /// Gets proximity-based priority
    async fn get_proximity_priority(
        &self,
        subscriber: PlayerId,
        target: PlayerId,
        channel: u8,
    ) -> ReplicationPriority {
        let proximity_subs = self.proximity_subs.read().await;
        if let (Some(sub_pos), Some(target_pos)) = (
            proximity_subs.get(&subscriber),
            proximity_subs.get(&target),
        ) {
            if sub_pos.is_in_range(target_pos.position, channel) {
                let distance = ProximitySubscription::calculate_distance(
                    sub_pos.position,
                    target_pos.position,
                );
                // Convert distance to priority
                if distance < 50.0 {
                    ReplicationPriority::Critical
                } else if distance < 150.0 {
                    ReplicationPriority::High
                } else if distance < 300.0 {
                    ReplicationPriority::Normal
                } else {
                    ReplicationPriority::Low
                }
            } else {
                ReplicationPriority::Low
            }
        } else {
            ReplicationPriority::Low
        }
    }

    /// Gets relationship-based priority
    async fn get_relationship_priority(
        &self,
        subscriber: PlayerId,
        target: PlayerId,
        channel: u8,
    ) -> ReplicationPriority {
        let relationship_subs = self.relationship_subs.read().await;
        if let Some(subs) = relationship_subs.get(&subscriber) {
            for rel_sub in subs {
                if rel_sub.contains_player(target) {
                    return rel_sub.get_channel_priority(channel);
                }
            }
        }
        ReplicationPriority::Low
    }

    /// Gets interest-based priority based on subscriber's tracked interests
    async fn get_interest_priority(
        &self,
        subscriber: PlayerId,
        target: PlayerId,
        channel: u8,
    ) -> ReplicationPriority {
        let interest_subs = self.interest_subs.read().await;
        let proximity_subs = self.proximity_subs.read().await;
        
        if let Some(interest) = interest_subs.get(&subscriber) {
            // Check if subscriber has specific interest in target player's object type
            if let Some(interested_level) = interest.interested_objects.get("player") {
                let base_priority = match interested_level {
                    InterestLevel::VeryHigh => ReplicationPriority::Critical,
                    InterestLevel::High => ReplicationPriority::High,
                    InterestLevel::Medium => ReplicationPriority::Normal,
                    InterestLevel::Low => ReplicationPriority::Low,
                    InterestLevel::None => ReplicationPriority::Low,
                };
                
                // Boost priority if target is within focus area
                if let (Some(focus_pos), Some(target_proximity)) = (
                    &interest.focus_position,
                    proximity_subs.get(&target)
                ) {
                    let distance = ProximitySubscription::calculate_distance(*focus_pos, target_proximity.position);
                    if distance <= interest.focus_radius {
                        // Target is within focus area - increase priority
                        return match base_priority {
                            ReplicationPriority::Low => ReplicationPriority::Normal,
                            ReplicationPriority::Normal => ReplicationPriority::High,
                            ReplicationPriority::High => ReplicationPriority::Critical,
                            ReplicationPriority::Critical => ReplicationPriority::Critical,
                        };
                    }
                }
                
                // Consider activity patterns for channel-specific adjustments
                if let Some(activity) = interest.activity_patterns.get("combat") {
                    if channel == CRITICAL_CHANNEL && activity.frequency > FREQUENCY_THRESHOLD { // Critical channel + high combat activity
                        return match base_priority {
                            ReplicationPriority::Low => ReplicationPriority::Normal,
                            ReplicationPriority::Normal => ReplicationPriority::High,
                            _ => base_priority,
                        };
                    }
                }
                
                return base_priority;
            }
        }
        
        // Default priority if no specific interest data
        ReplicationPriority::Normal
    }

    /// Gets current subscription statistics
    pub async fn get_stats(&self) -> SubscriptionStats {
        self.stats.read().await.clone()
    }
}

impl Default for SubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for subscription management
#[derive(Debug, Clone, Default)]
pub struct SubscriptionStats {
    /// Number of active proximity subscriptions
    pub proximity_subscriptions: usize,
    /// Number of active relationship subscriptions
    pub relationship_subscriptions: usize,
    /// Number of active interest subscriptions
    pub interest_subscriptions: usize,
    /// Number of proximity recalculations performed
    pub proximity_recalculations: u64,
    /// Average subscription update time in microseconds
    pub avg_update_time_us: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proximity_subscription() {
        let mut prox_sub = ProximitySubscription::new(Position::new(0.0, 0.0, 0.0));
        
        // Test initial state
        assert!(prox_sub.is_in_range(Position::new(50.0, 0.0, 0.0), 0));
        assert!(!prox_sub.is_in_range(Position::new(200.0, 0.0, 0.0), 0));
        
        // Test position update
        let should_recalc = prox_sub.update_position(Position::new(10.0, 0.0, 0.0));
        assert!(should_recalc);
    }

    #[test]
    fn test_relationship_subscription() {
        let mut rel_sub = RelationshipSubscription::new("team".to_string());
        let player_id = PlayerId::new();
        
        rel_sub.add_player(player_id);
        assert!(rel_sub.contains_player(player_id));
        
        // Team members should get high priority on metadata channel
        assert_eq!(rel_sub.get_channel_priority(3), ReplicationPriority::High);
    }

    #[test]
    fn test_interest_subscription() {
        let mut interest_sub = InterestSubscription::new();
        
        interest_sub.record_interest("weapon_1".to_string(), InterestLevel::High);
        assert_eq!(interest_sub.get_interest_level("weapon_1"), InterestLevel::High);
        
        interest_sub.update_focus(Position::new(100.0, 0.0, 0.0), 25.0);
        assert!(interest_sub.is_in_focus(Position::new(110.0, 0.0, 0.0)));
        assert!(!interest_sub.is_in_focus(Position::new(200.0, 0.0, 0.0)));
    }

    #[tokio::test]
    async fn test_subscription_manager() {
        let manager = SubscriptionManager::new();
        let player_id = PlayerId::new();
        let position = Position::new(0.0, 0.0, 0.0);
        
        manager.add_player(player_id, position).await;
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.proximity_recalculations, 0);
    }
}