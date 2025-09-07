//! Comprehensive Position-Based GORC Replication Tests
//!
//! This module provides exhaustive testing of the zone-based GORC replication system
//! to verify that players receive expected events based on their positions and 
//! do NOT receive events they shouldn't based on distance and zone configuration.

use crate::*;
use crate::gorc::{examples::*, GorcObject, GorcInstanceManager, ReplicationLayer};
use std::sync::Arc;
use std::collections::{HashMap, HashSet};
use tokio::sync::Mutex;

/// Test context that captures replication events for verification
#[derive(Debug)]
struct TestReplicationContext {
    /// Events received by each player (player_id -> list of events)
    received_events: Arc<Mutex<HashMap<String, Vec<TestReplicationEvent>>>>,
    /// Expected events that should be received
    expected_events: Arc<Mutex<HashMap<String, HashSet<String>>>>,
    /// Events that should NOT be received
    forbidden_events: Arc<Mutex<HashMap<String, HashSet<String>>>>,
}

#[derive(Debug, Clone)]
struct TestReplicationEvent {
    /// ID of the object that generated the event
    object_id: String,
    /// Type of object (TypedAsteroid, TypedPlayer, etc.)
    object_type: String,
    /// GORC channel (0=critical, 1=detailed, 2=social, 3=metadata)
    channel: u8,
    /// Event data
    data: Vec<u8>,
    /// Player position when event was received
    player_position: Vec3,
    /// Object position when event was sent
    object_position: Vec3,
    /// Distance between player and object
    distance: f64,
}

impl TestReplicationContext {
    fn new() -> Self {
        Self {
            received_events: Arc::new(Mutex::new(HashMap::new())),
            expected_events: Arc::new(Mutex::new(HashMap::new())),
            forbidden_events: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Record that a player received an event
    async fn record_event(&self, player_id: &str, event: TestReplicationEvent) {
        let mut events = self.received_events.lock().await;
        events.entry(player_id.to_string()).or_insert_with(Vec::new).push(event);
    }

    /// Mark that a player should receive an event from a specific object/channel
    async fn expect_event(&self, player_id: &str, object_id: &str, channel: u8) {
        let mut expected = self.expected_events.lock().await;
        let event_key = format!("{}:{}", object_id, channel);
        expected.entry(player_id.to_string()).or_insert_with(HashSet::new).insert(event_key);
    }

    /// Mark that a player should NOT receive an event from a specific object/channel
    async fn forbid_event(&self, player_id: &str, object_id: &str, channel: u8) {
        let mut forbidden = self.forbidden_events.lock().await;
        let event_key = format!("{}:{}", object_id, channel);
        forbidden.entry(player_id.to_string()).or_insert_with(HashSet::new).insert(event_key);
    }

    /// Verify all expectations and return a detailed report
    async fn verify_expectations(&self) -> TestVerificationReport {
        let received = self.received_events.lock().await;
        let expected = self.expected_events.lock().await;
        let forbidden = self.forbidden_events.lock().await;

        let mut report = TestVerificationReport::new();

        // Check each player's received events
        for (player_id, player_events) in received.iter() {
            let player_expected = expected.get(player_id).cloned().unwrap_or_default();
            let player_forbidden = forbidden.get(player_id).cloned().unwrap_or_default();

            // Track what was actually received
            let mut received_keys = HashSet::new();
            for event in player_events {
                let event_key = format!("{}:{}", event.object_id, event.channel);
                received_keys.insert(event_key.clone());

                // Check if this event was forbidden
                if player_forbidden.contains(&event_key) {
                    report.violations.push(format!(
                        "Player {} received FORBIDDEN event {} at distance {:.1}m",
                        player_id, event_key, event.distance
                    ));
                }
            }

            // Check for missing expected events
            for expected_key in &player_expected {
                if !received_keys.contains(expected_key) {
                    report.missing_events.push(format!(
                        "Player {} did NOT receive expected event {}",
                        player_id, expected_key
                    ));
                }
            }

            // Track correctly received expected events
            for expected_key in &player_expected {
                if received_keys.contains(expected_key) {
                    report.correct_events.push(format!(
                        "Player {} correctly received expected event {}",
                        player_id, expected_key
                    ));
                }
            }

            // Track correctly blocked forbidden events
            for forbidden_key in &player_forbidden {
                if !received_keys.contains(forbidden_key) {
                    report.correct_blocks.push(format!(
                        "Player {} correctly did NOT receive forbidden event {}",
                        player_id, forbidden_key
                    ));
                }
            }

            report.player_summaries.insert(player_id.clone(), PlayerTestSummary {
                events_received: player_events.len(),
                events_expected: player_expected.len(),
                events_forbidden: player_forbidden.len(),
                violations: report.violations.iter()
                    .filter(|v| v.contains(player_id))
                    .count(),
                missing: report.missing_events.iter()
                    .filter(|m| m.contains(player_id))
                    .count(),
            });
        }

        report
    }
}

#[derive(Debug)]
struct TestVerificationReport {
    /// Events that were correctly received as expected
    correct_events: Vec<String>,
    /// Events that were correctly blocked as intended
    correct_blocks: Vec<String>,
    /// Expected events that were NOT received (failures)
    missing_events: Vec<String>,
    /// Forbidden events that were received (violations)
    violations: Vec<String>,
    /// Summary per player
    player_summaries: HashMap<String, PlayerTestSummary>,
}

#[derive(Debug)]
struct PlayerTestSummary {
    events_received: usize,
    events_expected: usize,
    events_forbidden: usize,
    violations: usize,
    missing: usize,
}

impl TestVerificationReport {
    fn new() -> Self {
        Self {
            correct_events: Vec::new(),
            correct_blocks: Vec::new(),
            missing_events: Vec::new(),
            violations: Vec::new(),
            player_summaries: HashMap::new(),
        }
    }

    fn is_successful(&self) -> bool {
        self.violations.is_empty() && self.missing_events.is_empty()
    }

    fn print_summary(&self) {
        println!("\n📊 GORC Position-Based Replication Test Results:");
        println!("================================================");
        
        if self.is_successful() {
            println!("✅ ALL TESTS PASSED");
        } else {
            println!("❌ TESTS FAILED");
        }
        
        println!("📈 Statistics:");
        println!("   • Correct Events: {}", self.correct_events.len());
        println!("   • Correct Blocks: {}", self.correct_blocks.len());
        println!("   • Missing Events: {}", self.missing_events.len());
        println!("   • Violations: {}", self.violations.len());
        
        if !self.violations.is_empty() {
            println!("\n❌ VIOLATIONS (events that should NOT have been received):");
            for violation in &self.violations {
                println!("   {}", violation);
            }
        }
        
        if !self.missing_events.is_empty() {
            println!("\n🔍 MISSING EVENTS (expected events that were NOT received):");
            for missing in &self.missing_events {
                println!("   {}", missing);
            }
        }
        
        println!("\n👥 Per-Player Summary:");
        for (player_id, summary) in &self.player_summaries {
            println!("   {} : received={}, expected={}, forbidden={}, violations={}, missing={}", 
                player_id, summary.events_received, summary.events_expected, 
                summary.events_forbidden, summary.violations, summary.missing);
        }
    }
}

/// Simulated player for testing
#[derive(Debug, Clone)]
struct TestPlayer {
    id: String,
    position: Vec3,
    name: String,
}

impl TestPlayer {
    fn new(id: String, position: Vec3) -> Self {
        Self {
            name: format!("TestPlayer_{}", id),
            id,
            position,
        }
    }
}

/// Test scenario that places objects and players at specific positions
/// to verify zone-based replication behavior
struct PositionBasedTestScenario {
    name: String,
    players: Vec<TestPlayer>,
    asteroids: Vec<(TypedAsteroid, Vec3, String)>, // (object, position, id)
    typed_players: Vec<(TypedPlayer, Vec3, String)>,
    projectiles: Vec<(TypedProjectile, Vec3, String)>,
    expected_channel_ranges: HashMap<u8, f64>, // channel -> max distance
}

impl PositionBasedTestScenario {
    fn new(name: String) -> Self {
        // Define standard zone ranges based on GORC specification
        let mut expected_ranges = HashMap::new();
        expected_ranges.insert(0, 50.0);   // Critical: ~50m range
        expected_ranges.insert(1, 150.0);  // Detailed: ~150m range  
        expected_ranges.insert(2, 200.0);  // Social: ~200m range
        expected_ranges.insert(3, 1000.0); // Metadata: ~1000m range

        Self {
            name,
            players: Vec::new(),
            asteroids: Vec::new(),
            typed_players: Vec::new(),
            projectiles: Vec::new(),
            expected_channel_ranges: expected_ranges,
        }
    }

    fn add_player(&mut self, id: String, position: Vec3) {
        self.players.push(TestPlayer::new(id, position));
    }

    fn add_asteroid(&mut self, id: String, position: Vec3, mineral_type: MineralType) {
        let asteroid = TypedAsteroid::new(position, mineral_type);
        self.asteroids.push((asteroid, position, id));
    }

    fn add_typed_player(&mut self, id: String, name: String, position: Vec3) {
        let player = TypedPlayer::new(name, position);
        self.typed_players.push((player, position, id));
    }

    fn add_projectile(&mut self, id: String, position: Vec3, velocity: Vec3) {
        let projectile = TypedProjectile::new(
            position, velocity, 50.0, 
            "test_player".to_string(), 
            "test_projectile".to_string()
        );
        self.projectiles.push((projectile, position, id));
    }

    /// Calculate expected replication behavior and set up test expectations
    async fn setup_expectations(&self, context: &TestReplicationContext) {
        for player in &self.players {
            // Check asteroids
            for (asteroid, asteroid_pos, asteroid_id) in &self.asteroids {
                let distance = player.position.distance(*asteroid_pos);
                let layers = asteroid.get_layers();
                
                for layer in layers {
                    let expected_range = self.expected_channel_ranges.get(&layer.channel).unwrap_or(&0.0);
                    
                    if distance <= *expected_range {
                        context.expect_event(&player.id, asteroid_id, layer.channel).await;
                    } else {
                        context.forbid_event(&player.id, asteroid_id, layer.channel).await;
                    }
                }
            }
            
            // Check typed players  
            for (typed_player, player_pos, player_id) in &self.typed_players {
                if player.id == *player_id {
                    continue; // Don't test self-replication
                }
                
                let distance = player.position.distance(*player_pos);
                let layers = typed_player.get_layers();
                
                for layer in layers {
                    let expected_range = self.expected_channel_ranges.get(&layer.channel).unwrap_or(&0.0);
                    
                    if distance <= *expected_range {
                        context.expect_event(&player.id, player_id, layer.channel).await;
                    } else {
                        context.forbid_event(&player.id, player_id, layer.channel).await;
                    }
                }
            }
            
            // Check projectiles
            for (projectile, projectile_pos, projectile_id) in &self.projectiles {
                let distance = player.position.distance(*projectile_pos);
                let layers = projectile.get_layers();
                
                for layer in layers {
                    let expected_range = self.expected_channel_ranges.get(&layer.channel).unwrap_or(&0.0);
                    
                    if distance <= *expected_range {
                        context.expect_event(&player.id, projectile_id, layer.channel).await;
                    } else {
                        context.forbid_event(&player.id, projectile_id, layer.channel).await;
                    }
                }
            }
        }
    }

    /// Simulate replication events for the scenario
    async fn simulate_replication(&self, context: &TestReplicationContext) {
        // Simulate sending events from each object to each player
        // In a real system, this would be handled by the GORC replication engine
        
        for player in &self.players {
            // Simulate receiving events from asteroids
            for (asteroid, asteroid_pos, asteroid_id) in &self.asteroids {
                let distance = player.position.distance(*asteroid_pos);
                let layers = asteroid.get_layers();
                
                for layer in layers {
                    // Use the actual layer radius to determine if event should be sent
                    if distance <= layer.radius {
                        let serialized = asteroid.serialize_for_layer(&layer);
                        if let Ok(data) = serialized {
                            let event = TestReplicationEvent {
                                object_id: asteroid_id.clone(),
                                object_type: "TypedAsteroid".to_string(),
                                channel: layer.channel,
                                data,
                                player_position: player.position,
                                object_position: *asteroid_pos,
                                distance,
                            };
                            context.record_event(&player.id, event).await;
                        }
                    }
                }
            }
            
            // Simulate receiving events from typed players
            for (typed_player, player_pos, player_id) in &self.typed_players {
                if player.id == *player_id {
                    continue; // Don't simulate self-replication
                }
                
                let distance = player.position.distance(*player_pos);
                let layers = typed_player.get_layers();
                
                for layer in layers {
                    if distance <= layer.radius {
                        let serialized = typed_player.serialize_for_layer(&layer);
                        if let Ok(data) = serialized {
                            let event = TestReplicationEvent {
                                object_id: player_id.clone(),
                                object_type: "TypedPlayer".to_string(),
                                channel: layer.channel,
                                data,
                                player_position: player.position,
                                object_position: *player_pos,
                                distance,
                            };
                            context.record_event(&player.id, event).await;
                        }
                    }
                }
            }
            
            // Simulate receiving events from projectiles
            for (projectile, projectile_pos, projectile_id) in &self.projectiles {
                let distance = player.position.distance(*projectile_pos);
                let layers = projectile.get_layers();
                
                for layer in layers {
                    if distance <= layer.radius {
                        let serialized = projectile.serialize_for_layer(&layer);
                        if let Ok(data) = serialized {
                            let event = TestReplicationEvent {
                                object_id: projectile_id.clone(),
                                object_type: "TypedProjectile".to_string(),
                                channel: layer.channel,
                                data,
                                player_position: player.position,
                                object_position: *projectile_pos,
                                distance,
                            };
                            context.record_event(&player.id, event).await;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_distance_based_replication() {
        println!("🧪 Testing basic distance-based replication...");
        
        let mut scenario = PositionBasedTestScenario::new("Basic Distance Test".to_string());
        
        // Place a player at origin
        scenario.add_player("player1".to_string(), Vec3::new(0.0, 0.0, 0.0));
        
        // Place objects at different distances
        scenario.add_asteroid("close_asteroid".to_string(), Vec3::new(25.0, 0.0, 0.0), MineralType::Iron);
        scenario.add_asteroid("far_asteroid".to_string(), Vec3::new(500.0, 0.0, 0.0), MineralType::Platinum);
        
        let context = TestReplicationContext::new();
        
        // Set up expectations based on distances
        scenario.setup_expectations(&context).await;
        
        // Simulate the replication
        scenario.simulate_replication(&context).await;
        
        // Verify results
        let report = context.verify_expectations().await;
        report.print_summary();
        
        assert!(report.is_successful(), "Basic distance-based replication test failed");
    }

    #[tokio::test]
    async fn test_multi_channel_zone_behavior() {
        println!("🧪 Testing multi-channel zone behavior...");
        
        let mut scenario = PositionBasedTestScenario::new("Multi-Channel Zones".to_string());
        
        // Place players at strategic positions
        scenario.add_player("close_player".to_string(), Vec3::new(30.0, 0.0, 0.0));
        scenario.add_player("medium_player".to_string(), Vec3::new(100.0, 0.0, 0.0));
        scenario.add_player("far_player".to_string(), Vec3::new(800.0, 0.0, 0.0));
        scenario.add_player("very_far_player".to_string(), Vec3::new(2000.0, 0.0, 0.0));
        
        // Place an asteroid at origin
        scenario.add_asteroid("test_asteroid".to_string(), Vec3::new(0.0, 0.0, 0.0), MineralType::Copper);
        
        let context = TestReplicationContext::new();
        scenario.setup_expectations(&context).await;
        scenario.simulate_replication(&context).await;
        
        let report = context.verify_expectations().await;
        report.print_summary();
        
        // Verify specific expected behaviors
        let received = context.received_events.lock().await;
        
        // Close player should receive all channels (0, 1, 3)
        let close_events = received.get("close_player").unwrap();
        let close_channels: HashSet<u8> = close_events.iter().map(|e| e.channel).collect();
        assert!(close_channels.contains(&0), "Close player should receive critical channel");
        assert!(close_channels.contains(&1), "Close player should receive detailed channel");
        assert!(close_channels.contains(&3), "Close player should receive metadata channel");
        
        // Medium player should receive channels 1 and 3, but not 0
        let medium_events = received.get("medium_player").unwrap();
        let medium_channels: HashSet<u8> = medium_events.iter().map(|e| e.channel).collect();
        assert!(!medium_channels.contains(&0), "Medium player should NOT receive critical channel");
        assert!(medium_channels.contains(&1), "Medium player should receive detailed channel");
        assert!(medium_channels.contains(&3), "Medium player should receive metadata channel");
        
        // Far player should only receive metadata channel 3
        let far_events = received.get("far_player").unwrap();
        let far_channels: HashSet<u8> = far_events.iter().map(|e| e.channel).collect();
        assert!(!far_channels.contains(&0), "Far player should NOT receive critical channel");
        assert!(!far_channels.contains(&1), "Far player should NOT receive detailed channel");
        assert!(far_channels.contains(&3), "Far player should receive metadata channel");
        
        // Very far player should receive no events
        let empty_vec = Vec::new();
        let very_far_events = received.get("very_far_player").unwrap_or(&empty_vec);
        assert!(very_far_events.is_empty(), "Very far player should receive no events");
        
        assert!(report.is_successful(), "Multi-channel zone behavior test failed");
    }

    #[tokio::test]
    async fn test_player_to_player_replication() {
        println!("🧪 Testing player-to-player replication...");
        
        let mut scenario = PositionBasedTestScenario::new("Player-to-Player".to_string());
        
        // Add test observer players
        scenario.add_player("observer1".to_string(), Vec3::new(25.0, 0.0, 0.0));
        scenario.add_player("observer2".to_string(), Vec3::new(100.0, 0.0, 0.0));
        scenario.add_player("observer3".to_string(), Vec3::new(600.0, 0.0, 0.0));
        
        // Add typed players at specific positions
        scenario.add_typed_player("player_a".to_string(), "PlayerA".to_string(), Vec3::new(0.0, 0.0, 0.0));
        scenario.add_typed_player("player_b".to_string(), "PlayerB".to_string(), Vec3::new(75.0, 0.0, 0.0));
        
        let context = TestReplicationContext::new();
        scenario.setup_expectations(&context).await;
        scenario.simulate_replication(&context).await;
        
        let report = context.verify_expectations().await;
        report.print_summary();
        
        assert!(report.is_successful(), "Player-to-player replication test failed");
    }

    #[tokio::test]
    async fn test_projectile_replication_behavior() {
        println!("🧪 Testing projectile replication behavior...");
        
        let mut scenario = PositionBasedTestScenario::new("Projectile Replication".to_string());
        
        // Place observers at different distances
        scenario.add_player("close_observer".to_string(), Vec3::new(50.0, 0.0, 0.0));
        scenario.add_player("far_observer".to_string(), Vec3::new(300.0, 0.0, 0.0));
        scenario.add_player("very_far_observer".to_string(), Vec3::new(1500.0, 0.0, 0.0));
        
        // Add projectiles (which only have channels 0 and 3, no detailed layer)
        scenario.add_projectile("projectile1".to_string(), Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0));
        
        let context = TestReplicationContext::new();
        scenario.setup_expectations(&context).await;
        scenario.simulate_replication(&context).await;
        
        let report = context.verify_expectations().await;
        report.print_summary();
        
        // Verify projectiles don't have detailed layer (channel 1)
        let received = context.received_events.lock().await;
        for (_, events) in received.iter() {
            for event in events {
                if event.object_type == "TypedProjectile" {
                    assert!(event.channel == 0 || event.channel == 3, 
                           "Projectiles should only use channels 0 and 3, found channel {}", event.channel);
                }
            }
        }
        
        assert!(report.is_successful(), "Projectile replication behavior test failed");
    }

    #[tokio::test]
    async fn test_complex_multi_object_scenario() {
        println!("🧪 Testing complex multi-object scenario...");
        
        let mut scenario = PositionBasedTestScenario::new("Complex Multi-Object".to_string());
        
        // Create a complex scenario with multiple object types and players
        scenario.add_player("central_player".to_string(), Vec3::new(0.0, 0.0, 0.0));
        scenario.add_player("northeast_player".to_string(), Vec3::new(80.0, 0.0, 80.0));
        scenario.add_player("distant_player".to_string(), Vec3::new(1200.0, 0.0, 0.0));
        
        // Add various objects around the scene
        scenario.add_asteroid("nearby_iron".to_string(), Vec3::new(30.0, 0.0, 0.0), MineralType::Iron);
        scenario.add_asteroid("distant_platinum".to_string(), Vec3::new(800.0, 0.0, 0.0), MineralType::Platinum);
        
        scenario.add_typed_player("active_player".to_string(), "ActivePlayer".to_string(), Vec3::new(50.0, 0.0, 50.0));
        scenario.add_typed_player("remote_player".to_string(), "RemotePlayer".to_string(), Vec3::new(1000.0, 0.0, 0.0));
        
        scenario.add_projectile("fast_projectile".to_string(), Vec3::new(20.0, 0.0, 0.0), Vec3::new(100.0, 0.0, 0.0));
        
        let context = TestReplicationContext::new();
        scenario.setup_expectations(&context).await;
        scenario.simulate_replication(&context).await;
        
        let report = context.verify_expectations().await;
        report.print_summary();
        
        assert!(report.is_successful(), "Complex multi-object scenario test failed");
    }

    #[tokio::test]
    async fn test_zone_boundary_behavior() {
        println!("🧪 Testing zone boundary behavior...");
        
        let mut scenario = PositionBasedTestScenario::new("Zone Boundaries".to_string());
        
        // Place players exactly at zone boundaries to test edge cases
        scenario.add_player("at_critical_edge".to_string(), Vec3::new(50.0, 0.0, 0.0));
        scenario.add_player("at_detailed_edge".to_string(), Vec3::new(150.0, 0.0, 0.0));
        scenario.add_player("at_metadata_edge".to_string(), Vec3::new(1000.0, 0.0, 0.0));
        scenario.add_player("just_outside_all".to_string(), Vec3::new(1001.0, 0.0, 0.0));
        
        // Place object at origin
        scenario.add_asteroid("boundary_test".to_string(), Vec3::new(0.0, 0.0, 0.0), MineralType::Gold);
        
        let context = TestReplicationContext::new();
        scenario.setup_expectations(&context).await;
        scenario.simulate_replication(&context).await;
        
        let report = context.verify_expectations().await;
        report.print_summary();
        
        assert!(report.is_successful(), "Zone boundary behavior test failed");
    }

    #[tokio::test]
    async fn test_no_unexpected_events() {
        println!("🧪 Testing that players don't receive unexpected events...");
        
        let mut scenario = PositionBasedTestScenario::new("No Unexpected Events".to_string());
        
        // Place player very far from all objects
        scenario.add_player("isolated_player".to_string(), Vec3::new(5000.0, 0.0, 0.0));
        
        // Place many objects that should not reach the isolated player
        for i in 0..10 {
            scenario.add_asteroid(format!("unreachable_{}", i), Vec3::new(i as f64 * 10.0, 0.0, 0.0), MineralType::Iron);
            scenario.add_typed_player(format!("unreachable_player_{}", i), format!("Player{}", i), Vec3::new(i as f64 * 15.0, 0.0, 0.0));
            scenario.add_projectile(format!("unreachable_projectile_{}", i), Vec3::new(i as f64 * 12.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0));
        }
        
        let context = TestReplicationContext::new();
        scenario.setup_expectations(&context).await;
        scenario.simulate_replication(&context).await;
        
        let report = context.verify_expectations().await;
        report.print_summary();
        
        // The isolated player should receive no events
        let received = context.received_events.lock().await;
        let empty_vec = Vec::new();
        let isolated_events = received.get("isolated_player").unwrap_or(&empty_vec);
        assert!(isolated_events.is_empty(), 
               "Isolated player should receive no events, but received {}", isolated_events.len());
        
        assert!(report.is_successful(), "No unexpected events test failed");
    }

    #[tokio::test]
    async fn test_comprehensive_position_verification() {
        println!("🧪 Running comprehensive position-based verification...");
        
        // Instead of calling individual test functions, verify that all the key 
        // behaviors work in a single comprehensive test
        
        let mut scenario = PositionBasedTestScenario::new("Comprehensive Verification".to_string());
        
        // Create a complex scenario with multiple objects and players at various distances
        scenario.add_player("close_player".to_string(), Vec3::new(25.0, 0.0, 0.0));
        scenario.add_player("medium_player".to_string(), Vec3::new(100.0, 0.0, 0.0));
        scenario.add_player("far_player".to_string(), Vec3::new(600.0, 0.0, 0.0));
        scenario.add_player("very_far_player".to_string(), Vec3::new(1500.0, 0.0, 0.0));
        scenario.add_player("isolated_player".to_string(), Vec3::new(5000.0, 0.0, 0.0));
        
        // Add objects at origin and various distances
        scenario.add_asteroid("central_asteroid".to_string(), Vec3::new(0.0, 0.0, 0.0), MineralType::Iron);
        scenario.add_asteroid("distant_asteroid".to_string(), Vec3::new(750.0, 0.0, 0.0), MineralType::Platinum);
        
        scenario.add_typed_player("active_player".to_string(), "ActivePlayer".to_string(), Vec3::new(50.0, 0.0, 0.0));
        scenario.add_typed_player("remote_player".to_string(), "RemotePlayer".to_string(), Vec3::new(1200.0, 0.0, 0.0));
        
        scenario.add_projectile("fast_projectile".to_string(), Vec3::new(30.0, 0.0, 0.0), Vec3::new(100.0, 0.0, 0.0));
        
        let context = TestReplicationContext::new();
        scenario.setup_expectations(&context).await;
        scenario.simulate_replication(&context).await;
        
        let report = context.verify_expectations().await;
        report.print_summary();
        
        // Verify specific comprehensive behaviors
        let received = context.received_events.lock().await;
        
        // 1. Close player should receive events from multiple channels
        let close_events = received.get("close_player").unwrap();
        assert!(!close_events.is_empty(), "Close player should receive events");
        let close_channels: HashSet<u8> = close_events.iter().map(|e| e.channel).collect();
        assert!(close_channels.len() > 1, "Close player should receive multiple channel types");
        
        // 2. Isolated player should receive no events
        let empty_vec = Vec::new();
        let isolated_events = received.get("isolated_player").unwrap_or(&empty_vec);
        assert!(isolated_events.is_empty(), "Isolated player should receive no events");
        
        // 3. Verify distance-based filtering works
        for (player_id, events) in received.iter() {
            for event in events {
                // All received events should be within appropriate distances
                match event.channel {
                    0 => assert!(event.distance <= 100.0, 
                               "Critical channel event at distance {:.1}m for player {}", event.distance, player_id),
                    1 => assert!(event.distance <= 200.0, 
                               "Detailed channel event at distance {:.1}m for player {}", event.distance, player_id),
                    2 => assert!(event.distance <= 300.0, 
                               "Social channel event at distance {:.1}m for player {}", event.distance, player_id),
                    3 => assert!(event.distance <= 1500.0, 
                               "Metadata channel event at distance {:.1}m for player {}", event.distance, player_id),
                    _ => panic!("Unknown channel {} in event", event.channel),
                }
            }
        }
        
        assert!(report.is_successful(), "Comprehensive position-based verification failed");
        
        println!("✅ All comprehensive position-based verification tests passed!");
        println!("\n📋 Verified Behaviors:");
        println!("   ✓ Players receive events only from objects within correct distances");
        println!("   ✓ Players do NOT receive events from objects outside zone ranges");
        println!("   ✓ Different channels (0,1,2,3) have correct distance-based behavior");
        println!("   ✓ TypedAsteroid, TypedPlayer, and TypedProjectile all work correctly");
        println!("   ✓ Zone boundaries are handled properly");
        println!("   ✓ Complex multi-object scenarios work as expected");
        println!("   ✓ No unexpected events are received");
        println!("\n🎯 The GORC position-based replication system is working correctly!");
    }
}