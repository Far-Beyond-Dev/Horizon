//! Comprehensive GORC Replication Testing
//!
//! This module tests the event-driven GORC system by predicting exactly what events
//! each virtual player should receive based on their positions relative to other players.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use crate::{Vec3, PlayerId};
use crate::gorc::{GorcObjectId, ReplicationLayer};
use serde::{Serialize, Deserialize};

/// Test configuration for GORC replication scenarios
#[derive(Debug, Clone)]
pub struct ReplicationTestConfig {
    /// Players and their positions
    pub players: HashMap<PlayerId, Vec3>,
    /// Objects and their configurations
    pub objects: HashMap<GorcObjectId, TestObject>,
    /// Expected event deliveries: (sender, event, expected_recipients)
    pub expected_events: Vec<(PlayerId, TestEvent, HashSet<PlayerId>)>,
}

/// Test object with defined replication layers
#[derive(Debug, Clone)]
pub struct TestObject {
    pub position: Vec3,
    pub layers: Vec<ReplicationLayer>,
    pub object_type: String,
}

/// Test events that can be emitted
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TestEvent {
    Move,
    Attack,
    Chat,
    LevelUp,
}

impl TestEvent {
    pub fn channel(&self) -> u8 {
        match self {
            TestEvent::Move => 0,        // Critical channel
            TestEvent::Attack => 1,      // Detailed channel  
            TestEvent::Chat => 2,        // Social channel
            TestEvent::LevelUp => 3,     // Metadata channel
        }
    }
}

/// Creates a standard test scenario with 5 players positioned in a cross pattern
pub fn create_cross_pattern_test() -> ReplicationTestConfig {
    let mut players = HashMap::new();
    let mut objects = HashMap::new();
    let mut expected_events = Vec::new();

    // Player positions in a cross pattern
    let player_center = PlayerId::new();
    let player_north = PlayerId::new();
    let player_south = PlayerId::new(); 
    let player_east = PlayerId::new();
    let player_west = PlayerId::new();

    players.insert(player_center, Vec3::new(0.0, 0.0, 0.0));      // Center
    players.insert(player_north, Vec3::new(0.0, 0.0, 50.0));     // 50m north
    players.insert(player_south, Vec3::new(0.0, 0.0, -50.0));    // 50m south
    players.insert(player_east, Vec3::new(150.0, 0.0, 0.0));     // 150m east
    players.insert(player_west, Vec3::new(-300.0, 0.0, 0.0));    // 300m west

    // Each player has a GORC object with standard layers
    for (_player_id, position) in &players {
        let object_id = GorcObjectId::new();
        objects.insert(object_id, TestObject {
            position: *position,
            object_type: "TestPlayer".to_string(),
            layers: vec![
                ReplicationLayer::new(0, 75.0, 20.0, vec!["position".to_string()], crate::gorc::channels::CompressionType::Delta),     // 75m critical
                ReplicationLayer::new(1, 200.0, 10.0, vec!["health".to_string()], crate::gorc::channels::CompressionType::Lz4),       // 200m detailed
                ReplicationLayer::new(2, 300.0, 5.0, vec!["chat".to_string()], crate::gorc::channels::CompressionType::Lz4),          // 300m social
                ReplicationLayer::new(3, 1000.0, 2.0, vec!["level".to_string()], crate::gorc::channels::CompressionType::High),       // 1000m metadata
            ],
        });
    }

    // Test 1: Center player moves (channel 0, 75m radius)
    // Should reach: north (50m), south (50m)
    // Should NOT reach: east (150m), west (300m)
    let mut move_recipients = HashSet::new();
    move_recipients.insert(player_north);
    move_recipients.insert(player_south);
    expected_events.push((
        player_center,
        TestEvent::Move,
        move_recipients,
    ));

    // Test 2: Center player attacks (channel 1, 200m radius) 
    // Should reach: north (50m), south (50m), east (150m)
    // Should NOT reach: west (300m)
    let mut attack_recipients = HashSet::new();
    attack_recipients.insert(player_north);
    attack_recipients.insert(player_south);
    attack_recipients.insert(player_east);
    expected_events.push((
        player_center,
        TestEvent::Attack,
        attack_recipients,
    ));

    // Test 3: Center player chats (channel 2, 300m radius)
    // Should reach: north (50m), south (50m), east (150m)
    // Should NOT reach: west (300m) - exactly at boundary
    let mut chat_recipients = HashSet::new();
    chat_recipients.insert(player_north);
    chat_recipients.insert(player_south);
    chat_recipients.insert(player_east);
    expected_events.push((
        player_center,
        TestEvent::Chat,
        chat_recipients,
    ));

    // Test 4: Center player levels up (channel 3, 1000m radius)
    // Should reach: ALL players
    let mut level_recipients = HashSet::new();
    level_recipients.insert(player_north);
    level_recipients.insert(player_south);
    level_recipients.insert(player_east);
    level_recipients.insert(player_west);
    expected_events.push((
        player_center,
        TestEvent::LevelUp,
        level_recipients,
    ));

    ReplicationTestConfig {
        players,
        objects,
        expected_events,
    }
}

/// Creates an edge case test with players exactly at zone boundaries
pub fn create_boundary_test() -> ReplicationTestConfig {
    let mut players = HashMap::new();
    let mut objects = HashMap::new();
    let mut expected_events = Vec::new();

    let player_center = PlayerId::new();
    let player_edge_in = PlayerId::new();     // Just inside boundary
    let player_edge_out = PlayerId::new();    // Just outside boundary
    let player_exact = PlayerId::new();       // Exactly on boundary

    players.insert(player_center, Vec3::new(0.0, 0.0, 0.0));
    players.insert(player_edge_in, Vec3::new(74.9, 0.0, 0.0));   // Just inside 75m
    players.insert(player_edge_out, Vec3::new(75.1, 0.0, 0.0));  // Just outside 75m  
    players.insert(player_exact, Vec3::new(75.0, 0.0, 0.0));     // Exactly 75m

    // Center player has standard layers
    let object_id = GorcObjectId::new();
    objects.insert(object_id, TestObject {
        position: Vec3::new(0.0, 0.0, 0.0),
        object_type: "TestPlayer".to_string(),
        layers: vec![
            ReplicationLayer::new(0, 75.0, 20.0, vec!["position".to_string()], crate::gorc::channels::CompressionType::Delta),
        ],
    });

    // Movement should reach edge_in and exact, but NOT edge_out
    let mut move_recipients = HashSet::new();
    move_recipients.insert(player_edge_in);
    move_recipients.insert(player_exact);  // <= should be included
    expected_events.push((
        player_center,
        TestEvent::Move,
        move_recipients,
    ));

    ReplicationTestConfig {
        players,
        objects,
        expected_events,
    }
}

/// Test runner that validates GORC replication behavior
pub struct ReplicationTestRunner {
    config: ReplicationTestConfig,
    actual_deliveries: HashMap<(PlayerId, TestEvent), HashSet<PlayerId>>,
}

impl ReplicationTestRunner {
    pub fn new(config: ReplicationTestConfig) -> Self {
        Self {
            config,
            actual_deliveries: HashMap::new(),
        }
    }

    /// Records that an event was delivered to a player (called by test harness)
    pub fn record_delivery(&mut self, sender: PlayerId, event: TestEvent, recipient: PlayerId) {
        let key = (sender, event.clone());
        self.actual_deliveries.entry(key).or_insert_with(HashSet::new).insert(recipient);
    }

    /// Validates all expected vs actual deliveries
    pub fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        for (sender, expected_event, expected_recipients) in &self.config.expected_events {
            let key = (*sender, expected_event.clone());
            let actual_recipients = self.actual_deliveries.get(&key).cloned().unwrap_or_default();

            // Check for missing deliveries
            for expected_recipient in expected_recipients {
                if !actual_recipients.contains(expected_recipient) {
                    errors.push(ValidationError::MissingDelivery {
                        sender: *sender,
                        event: expected_event.clone(),
                        missing_recipient: *expected_recipient,
                    });
                }
            }

            // Check for unexpected deliveries
            for actual_recipient in &actual_recipients {
                if !expected_recipients.contains(actual_recipient) {
                    errors.push(ValidationError::UnexpectedDelivery {
                        sender: *sender,
                        event: expected_event.clone(),
                        unexpected_recipient: *actual_recipient,
                    });
                }
            }
        }

        errors
    }

    /// Get summary of test results
    pub fn get_summary(&self) -> TestSummary {
        let total_expected = self.config.expected_events.len();
        let validation_errors = self.validate();
        let passed = validation_errors.is_empty();

        TestSummary {
            total_events: total_expected,
            validation_errors,
            passed,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ValidationError {
    MissingDelivery {
        sender: PlayerId,
        event: TestEvent,
        missing_recipient: PlayerId,
    },
    UnexpectedDelivery {
        sender: PlayerId,
        event: TestEvent,
        unexpected_recipient: PlayerId,
    },
}

#[derive(Debug)]
pub struct TestSummary {
    pub total_events: usize,
    pub validation_errors: Vec<ValidationError>,
    pub passed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cross_pattern_predictions() {
        let config = create_cross_pattern_test();
        
        // Verify we have 5 players
        assert_eq!(config.players.len(), 5);
        
        // Verify we have 4 expected events (move, attack, chat, level)
        assert_eq!(config.expected_events.len(), 4);
        
        // Verify level up event should reach all 4 other players
        let level_event = &config.expected_events[3];
        assert_eq!(level_event.2.len(), 4); // 4 recipients
    }

    #[test] 
    fn test_boundary_calculations() {
        let config = create_boundary_test();
        
        // Verify boundary event has correct recipients
        let move_event = &config.expected_events[0];
        assert_eq!(move_event.2.len(), 2); // edge_in and exact players
    }
}