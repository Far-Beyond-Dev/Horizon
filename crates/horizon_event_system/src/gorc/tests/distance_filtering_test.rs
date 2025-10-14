//! Test to verify distance-based filtering works correctly after position updates
//!
//! This test specifically checks the scenario where:
//! 1. Two players start at position (0,0,0) - within 25m range
//! 2. Player 1 moves far away (500km+)
//! 3. Player 2 should NO LONGER receive position updates from Player 1
//!
//! This was a bug where the GORC spatial tracking wasn't being updated when players moved.

use crate::{EventSystem, PlayerId, Vec3, ClientResponseSender, AuthenticationStatus};
use crate::gorc::instance::{GorcInstanceManager, GorcObject};
use crate::gorc::channels::{ReplicationLayer, CompressionType, ReplicationPriority};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use std::any::Any;

/// Mock client response sender that captures all sent messages
#[derive(Debug, Default)]
pub struct MockClientSender {
    /// Maps player_id -> list of received messages
    pub sent_messages: Arc<Mutex<HashMap<PlayerId, Vec<Vec<u8>>>>>,
}

impl MockClientSender {
    pub fn new() -> Self {
        Self {
            sent_messages: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    pub fn get_message_count(&self, player_id: PlayerId) -> usize {
        let messages = self.sent_messages.lock().unwrap();
        messages.get(&player_id).map(|v| v.len()).unwrap_or(0)
    }
    
    pub fn clear_messages(&self) {
        let mut messages = self.sent_messages.lock().unwrap();
        messages.clear();
    }
}

impl ClientResponseSender for MockClientSender {
    fn send_to_client(&self, player_id: PlayerId, data: Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        let sent_messages = self.sent_messages.clone();
        Box::pin(async move {
            let mut messages = sent_messages.lock().unwrap();
            messages.entry(player_id).or_insert_with(Vec::new).push(data);
            Ok(())
        })
    }
    
    fn kick(&self, _player_id: PlayerId, _reason: Option<String>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }
    
    fn broadcast_to_all(&self, _data: Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<usize, String>> + Send + '_>> {
        Box::pin(async { Ok(0) })
    }
    
    fn is_connection_active(&self, _player_id: PlayerId) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + '_>> {
        Box::pin(async { true })
    }
    
    fn get_auth_status(&self, _player_id: PlayerId) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<AuthenticationStatus>> + Send + '_>> {
        Box::pin(async { Some(AuthenticationStatus::Authenticated) })
    }
}

/// Simple test player object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestPlayer {
    pub position: Vec3,
    pub name: String,
}

impl TestPlayer {
    pub fn new(position: Vec3, name: String) -> Self {
        Self { position, name }
    }
}

impl GorcObject for TestPlayer {
    fn type_name(&self) -> &'static str {
        "TestPlayer"
    }
    
    fn position(&self) -> Vec3 {
        self.position
    }
    
    fn get_priority(&self, observer_pos: Vec3) -> ReplicationPriority {
        let distance = self.position.distance(observer_pos);
        if distance < 50.0 {
            ReplicationPriority::Critical
        } else if distance < 200.0 {
            ReplicationPriority::High
        } else {
            ReplicationPriority::Normal
        }
    }
    
    fn get_layers(&self) -> Vec<ReplicationLayer> {
        vec![
            // Channel 0: 25m radius for critical position updates (matching PlayerCriticalData)
            ReplicationLayer::new(0, 25.0, 60.0, vec!["position".to_string()], CompressionType::None),
        ]
    }
    
    fn serialize_for_layer(&self, _layer: &ReplicationLayer) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let data = serde_json::json!({
            "position": self.position,
            "name": self.name,
        });
        Ok(serde_json::to_vec(&data)?)
    }
    
    fn update_position(&mut self, new_position: Vec3) {
        self.position = new_position;
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }
    
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    
    fn clone_object(&self) -> Box<dyn GorcObject> {
        Box::new(self.clone())
    }
}

#[tokio::test]
async fn test_distance_filtering_after_movement() {
    println!("\n🧪 TEST: Distance-based filtering after player movement");
    println!("📍 Scenario: Player 1 moves 500km away, Player 2 should stop receiving updates");
    
    // Setup
    let gorc_instances = Arc::new(GorcInstanceManager::new());
    let mut event_system = EventSystem::new();
    let mock_sender = Arc::new(MockClientSender::new());
    
    event_system.set_gorc_instances(gorc_instances.clone());
    event_system.set_client_response_sender(mock_sender.clone());
    let event_system = Arc::new(event_system);
    
    // Create two players at the origin (within 25m range)
    let player1_id = PlayerId::new();
    let player2_id = PlayerId::new();
    
    println!("👤 Player 1: {}", player1_id);
    println!("👤 Player 2: {}", player2_id);
    
    // Register both players at (0,0,0)
    let player1_obj = TestPlayer::new(Vec3::new(0.0, 0.0, 0.0), "Player1".to_string());
    let player2_obj = TestPlayer::new(Vec3::new(0.0, 0.0, 0.0), "Player2".to_string());
    
    let player1_obj_id = gorc_instances.register_object(player1_obj, Vec3::new(0.0, 0.0, 0.0)).await;
    let _player2_obj_id = gorc_instances.register_object(player2_obj, Vec3::new(0.0, 0.0, 0.0)).await;
    
    // Add both players to GORC spatial tracking
    gorc_instances.add_player(player1_id, Vec3::new(0.0, 0.0, 0.0)).await;
    gorc_instances.add_player(player2_id, Vec3::new(0.0, 0.0, 0.0)).await;
    
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    
    println!("\n📡 PHASE 1: Both players at (0,0,0) - should receive each other's updates");
    
    // Clear any initial messages
    mock_sender.clear_messages();
    
    // Emit a move event for player 1 (still at origin)
    let move_event = serde_json::json!({
        "position": Vec3::new(1.0, 0.0, 0.0),
        "name": "Player1"
    });
    
    event_system.emit_gorc_instance(
        player1_obj_id,
        0,
        "move",
        &move_event,
        crate::Dest::Client
    ).await.expect("Failed to emit move event");
    
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    
    let player1_msgs = mock_sender.get_message_count(player1_id);
    let player2_msgs = mock_sender.get_message_count(player2_id);
    
    println!("  Player 1 received: {} messages", player1_msgs);
    println!("  Player 2 received: {} messages", player2_msgs);
    
    assert!(player1_msgs > 0, "Player 1 should receive their own movement echo");
    assert!(player2_msgs > 0, "Player 2 should receive Player 1's movement (within 25m)");
    
    println!("  ✅ Both players received updates as expected");
    
    println!("\n📡 PHASE 2: Player 1 moves to 500km away");
    
    // Clear messages
    mock_sender.clear_messages();
    
    // Move player 1 far away (500km = 500,000m)
    let far_position = Vec3::new(500000.0, 0.0, 0.0);
    
    // CRITICAL: Update the GORC spatial tracking (this is what the fix adds)
    event_system.update_player_position(player1_id, far_position).await
        .expect("Failed to update player position");
    
    println!("  Updated Player 1 position to {:?}", far_position);
    
    // Also update the object's position (simulating what the movement handler does)
    // We need to get a mutable instance and update it through the proper API
    if let Some(mut instance) = gorc_instances.get_object(player1_obj_id).await {
        instance.object.update_position(far_position);
        gorc_instances.update_object(player1_obj_id, instance).await;
    }
    
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    
    // Now emit another move event for player 1 at the far position
    let far_move_event = serde_json::json!({
        "position": far_position,
        "name": "Player1"
    });
    
    event_system.emit_gorc_instance(
        player1_obj_id,
        0,
        "move",
        &far_move_event,
        crate::Dest::Client
    ).await.expect("Failed to emit far move event");
    
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    
    let player1_msgs_after = mock_sender.get_message_count(player1_id);
    let player2_msgs_after = mock_sender.get_message_count(player2_id);
    
    println!("  Player 1 received: {} messages", player1_msgs_after);
    println!("  Player 2 received: {} messages", player2_msgs_after);
    
    assert!(player1_msgs_after > 0, "Player 1 should still receive their own movement echo");
    assert_eq!(player2_msgs_after, 0, 
        "Player 2 should NOT receive Player 1's movement (500km away, outside 25m range)");
    
    println!("  ✅ Distance filtering working correctly!");
    
    println!("\n🎉 TEST PASSED: Distance-based filtering works correctly after movement");
}

#[tokio::test]
async fn test_distance_filtering_with_multiple_movements() {
    println!("\n🧪 TEST: Distance filtering with gradual movement");
    
    // Setup
    let gorc_instances = Arc::new(GorcInstanceManager::new());
    let mut event_system = EventSystem::new();
    let mock_sender = Arc::new(MockClientSender::new());
    
    event_system.set_gorc_instances(gorc_instances.clone());
    event_system.set_client_response_sender(mock_sender.clone());
    let event_system = Arc::new(event_system);
    
    // Create two players
    let player1_id = PlayerId::new();
    let player2_id = PlayerId::new();
    
    let player1_obj = TestPlayer::new(Vec3::new(0.0, 0.0, 0.0), "Player1".to_string());
    let player2_obj = TestPlayer::new(Vec3::new(0.0, 0.0, 0.0), "Player2".to_string());
    
    let player1_obj_id = gorc_instances.register_object(player1_obj, Vec3::new(0.0, 0.0, 0.0)).await;
    let _player2_obj_id = gorc_instances.register_object(player2_obj, Vec3::new(0.0, 0.0, 0.0)).await;
    
    gorc_instances.add_player(player1_id, Vec3::new(0.0, 0.0, 0.0)).await;
    gorc_instances.add_player(player2_id, Vec3::new(0.0, 0.0, 0.0)).await;
    
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    
    // Test movements at different distances
    let test_positions = vec![
        (Vec3::new(10.0, 0.0, 0.0), true, "10m - should receive"),
        (Vec3::new(24.0, 0.0, 0.0), true, "24m - should receive (just inside range)"),
        (Vec3::new(26.0, 0.0, 0.0), false, "26m - should NOT receive (just outside range)"),
        (Vec3::new(100.0, 0.0, 0.0), false, "100m - should NOT receive"),
        (Vec3::new(500000.0, 0.0, 0.0), false, "500km - should NOT receive"),
    ];
    
    for (position, should_receive, description) in test_positions {
        println!("\n📍 Testing position: {}", description);
        
        mock_sender.clear_messages();
        
        // Update spatial tracking
        event_system.update_player_position(player1_id, position).await
            .expect("Failed to update position");
        
        // Update object position
        if let Some(mut instance) = gorc_instances.get_object(player1_obj_id).await {
            instance.object.update_position(position);
            gorc_instances.update_object(player1_obj_id, instance).await;
        }
        
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        
        // Emit move event
        let move_event = serde_json::json!({
            "position": position,
            "name": "Player1"
        });
        
        event_system.emit_gorc_instance(
            player1_obj_id,
            0,
            "move",
            &move_event,
            crate::Dest::Client
        ).await.expect("Failed to emit move event");
        
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        
        let player2_msgs = mock_sender.get_message_count(player2_id);
        
        if should_receive {
            assert!(player2_msgs > 0, "  ❌ Expected Player 2 to receive message at {}", description);
            println!("  ✅ Player 2 correctly received update");
        } else {
            assert_eq!(player2_msgs, 0, "  ❌ Expected Player 2 to NOT receive message at {}", description);
            println!("  ✅ Player 2 correctly did NOT receive update");
        }
    }
    
    println!("\n🎉 TEST PASSED: Distance filtering works at all tested distances");
}
