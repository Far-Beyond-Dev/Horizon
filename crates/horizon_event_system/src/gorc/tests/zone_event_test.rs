//! Test for the improved GORC zone event system
//!
//! This test verifies that zone events are properly emitted when:
//! 1. Players move into/out of object zones
//! 2. Objects move toward/away from stationary players
//! 3. New objects are created near existing players

use crate::gorc::instance::{GorcInstanceManager, GorcObject};
use crate::gorc::channels::{ReplicationLayer, CompressionType};
use crate::system::{EventSystem, ClientResponseSender};
use crate::types::{PlayerId, Vec3};
use std::sync::Arc;
use std::any::Any;
use tokio::time::{sleep, Duration};

/// Simple test object for GORC testing
#[derive(Debug, Clone)]
struct TestGorcObject {
    position: Vec3,
    object_type: String,
}

impl TestGorcObject {
    fn new(position: Vec3, object_type: String) -> Self {
        Self { position, object_type }
    }
}

impl GorcObject for TestGorcObject {
    fn type_name(&self) -> &str {
        "TestObject"
    }

    fn position(&self) -> Vec3 {
        self.position
    }

    fn get_priority(&self, _observer_pos: Vec3) -> crate::gorc::channels::ReplicationPriority {
        crate::gorc::channels::ReplicationPriority::High
    }

    fn serialize_for_layer(&self, _layer: &ReplicationLayer) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let data = serde_json::json!({
            "position": {
                "x": self.position.x,
                "y": self.position.y,
                "z": self.position.z
            },
            "object_type": self.object_type
        });
        Ok(serde_json::to_vec(&data)?)
    }

    fn get_layers(&self) -> Vec<ReplicationLayer> {
        vec![
            ReplicationLayer::new(0, 50.0, 60.0, vec!["position".to_string()], CompressionType::Delta),
            ReplicationLayer::new(1, 150.0, 30.0, vec!["animation".to_string()], CompressionType::Lz4),
            ReplicationLayer::new(2, 300.0, 15.0, vec!["metadata".to_string()], CompressionType::None),
        ]
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

/// Mock client response sender for testing
struct MockClientSender {
    sent_messages: Arc<tokio::sync::Mutex<Vec<(PlayerId, Vec<u8>)>>>,
}

impl MockClientSender {
    fn new() -> Self {
        Self {
            sent_messages: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }

    async fn get_sent_messages(&self) -> Vec<(PlayerId, Vec<u8>)> {
        self.sent_messages.lock().await.clone()
    }
}

impl std::fmt::Debug for MockClientSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockClientSender").finish()
    }
}

impl ClientResponseSender for MockClientSender {
    fn send_to_client(&self, player_id: PlayerId, data: Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async move {
            let mut messages = self.sent_messages.lock().await;
            messages.push((player_id, data));
            Ok(())
        })
    }

    fn is_connection_active(&self, _player_id: PlayerId) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + '_>> {
        Box::pin(async move { true })
    }

    fn get_auth_status(&self, _player_id: PlayerId) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<crate::types::AuthenticationStatus>> + Send + '_>> {
        Box::pin(async move { Some(crate::types::AuthenticationStatus::Authenticated) })
    }

    fn kick(&self, _player_id: PlayerId, _reason: Option<String>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async move { Ok(()) })
    }

    fn broadcast_to_all(&self, data: Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<usize, String>> + Send + '_>> {
        Box::pin(async move {
            let mut messages = self.sent_messages.lock().await;
            messages.push((PlayerId::new(), data));
            Ok(1)
        })
    }
}

#[tokio::test]
async fn test_player_movement_zone_events() {
    let mut events = EventSystem::new();
    let gorc_manager = Arc::new(GorcInstanceManager::new());
    let client_sender = Arc::new(MockClientSender::new());

    // Set up the event system with GORC and client sender
    events.set_gorc_instances(gorc_manager.clone());
    events.set_client_response_sender(client_sender.clone());

    // Create a test object at origin
    let test_object = TestGorcObject::new(Vec3::new(0.0, 0.0, 0.0), "asteroid".to_string());
    let object_id = gorc_manager.register_object(test_object, Vec3::new(0.0, 0.0, 0.0)).await;

    // Add a player far from the object
    let player_id = PlayerId::new();
    gorc_manager.add_player(player_id, Vec3::new(1000.0, 1000.0, 0.0)).await;

    // Move player close to the object - should trigger zone entry events
    events.update_player_position(player_id, Vec3::new(25.0, 25.0, 0.0)).await.unwrap();

    // Give some time for async processing
    sleep(Duration::from_millis(10)).await;

    // Check that zone entry messages were sent
    let messages = client_sender.get_sent_messages().await;
    assert!(!messages.is_empty(), "Should have received zone entry messages");

    // Verify the messages contain zone entry events
    let mut zone_entry_found = false;
    for (sent_player_id, data) in messages {
        assert_eq!(sent_player_id, player_id);

        if let Ok(event) = serde_json::from_slice::<serde_json::Value>(&data) {
            if event.get("type").and_then(|t| t.as_str()) == Some("gorc_zone_enter") {
                zone_entry_found = true;
                assert_eq!(event.get("object_id").and_then(|id| id.as_str()).unwrap(), object_id.to_string());
                println!("✅ Zone entry event verified: {:?}", event);
            }
        }
    }

    assert!(zone_entry_found, "Should have found at least one zone entry event");
}

#[tokio::test]
async fn test_object_movement_zone_events() {
    let mut events = EventSystem::new();
    let gorc_manager = Arc::new(GorcInstanceManager::new());
    let client_sender = Arc::new(MockClientSender::new());

    // Set up the event system
    events.set_gorc_instances(gorc_manager.clone());
    events.set_client_response_sender(client_sender.clone());

    // Create a test object far from origin
    let test_object = TestGorcObject::new(Vec3::new(1000.0, 1000.0, 0.0), "moving_asteroid".to_string());
    let object_id = gorc_manager.register_object(test_object, Vec3::new(1000.0, 1000.0, 0.0)).await;

    // Add a stationary player at origin
    let player_id = PlayerId::new();
    gorc_manager.add_player(player_id, Vec3::new(0.0, 0.0, 0.0)).await;
    
    // CRITICAL: Set player position (add_player no longer does this to avoid runtime issues)
    events.update_player_position(player_id, Vec3::new(0.0, 0.0, 0.0)).await.unwrap();

    // Move the object close to the stationary player - should trigger zone entry events
    events.update_object_position(object_id, Vec3::new(25.0, 25.0, 0.0)).await.unwrap();

    // Give some time for async processing
    sleep(Duration::from_millis(10)).await;

    // Check that zone entry messages were sent
    let messages = client_sender.get_sent_messages().await;
    assert!(!messages.is_empty(), "Should have received zone entry messages from object movement");

    // Verify zone entry events
    let mut zone_entry_found = false;
    for (sent_player_id, data) in messages {
        assert_eq!(sent_player_id, player_id);

        if let Ok(event) = serde_json::from_slice::<serde_json::Value>(&data) {
            if event.get("type").and_then(|t| t.as_str()) == Some("gorc_zone_enter") {
                zone_entry_found = true;
                assert_eq!(event.get("object_id").and_then(|id| id.as_str()).unwrap(), object_id.to_string());
                println!("✅ Object movement zone entry event verified: {:?}", event);
            }
        }
    }

    assert!(zone_entry_found, "Should have found zone entry event from object movement");
}

#[tokio::test]
async fn test_new_object_creation_zone_events() {
    let mut events = EventSystem::new();
    let gorc_manager = Arc::new(GorcInstanceManager::new());
    let client_sender = Arc::new(MockClientSender::new());

    // Set up the event system
    events.set_gorc_instances(gorc_manager.clone());
    events.set_client_response_sender(client_sender.clone());

    // Add a player at a specific position first
    let player_id = PlayerId::new();
    gorc_manager.add_player(player_id, Vec3::new(25.0, 25.0, 0.0)).await;
    
    // CRITICAL: Set player position (add_player no longer does this to avoid runtime issues)
    events.update_player_position(player_id, Vec3::new(25.0, 25.0, 0.0)).await.unwrap();

    // Create a new object near the existing player - should trigger zone entry events
    let test_object = TestGorcObject::new(Vec3::new(0.0, 0.0, 0.0), "new_asteroid".to_string());
    let object_id = gorc_manager.register_object(test_object, Vec3::new(0.0, 0.0, 0.0)).await;

    // Notify existing players about the new object
    events.notify_players_for_new_gorc_object(object_id).await.unwrap();

    // Give some time for async processing
    sleep(Duration::from_millis(10)).await;

    // Check that zone entry messages were sent
    let messages = client_sender.get_sent_messages().await;
    assert!(!messages.is_empty(), "Should have received zone entry messages for new object");

    // Verify zone entry events
    let mut zone_entry_found = false;
    for (sent_player_id, data) in messages {
        assert_eq!(sent_player_id, player_id);

        if let Ok(event) = serde_json::from_slice::<serde_json::Value>(&data) {
            if event.get("type").and_then(|t| t.as_str()) == Some("gorc_zone_enter") {
                zone_entry_found = true;
                assert_eq!(event.get("object_id").and_then(|id| id.as_str()).unwrap(), object_id.to_string());
                println!("✅ New object creation zone entry event verified: {:?}", event);
            }
        }
    }

    assert!(zone_entry_found, "Should have found zone entry event for new object creation");
}

#[tokio::test]
async fn test_zone_size_warnings() {
    let gorc_manager = Arc::new(GorcInstanceManager::new());

    // Create an object with a very large zone (should trigger warning)
    let large_zone_object = TestGorcLargeZoneObject::new(Vec3::new(0.0, 0.0, 0.0));
    let _object_id = gorc_manager.register_object(large_zone_object, Vec3::new(0.0, 0.0, 0.0)).await;

    // Check stats to verify warning was recorded
    let stats = gorc_manager.get_stats().await;
    assert!(stats.large_zone_warnings > 0, "Should have recorded large zone warning");

    println!("✅ Large zone warning system verified");
}

/// Test object with large zones for warning testing
#[derive(Debug, Clone)]
struct TestGorcLargeZoneObject {
    position: Vec3,
}

impl TestGorcLargeZoneObject {
    fn new(position: Vec3) -> Self {
        Self { position }
    }
}

impl GorcObject for TestGorcLargeZoneObject {
    fn type_name(&self) -> &str {
        "LargeZoneTestObject"
    }

    fn position(&self) -> Vec3 {
        self.position
    }

    fn get_priority(&self, _observer_pos: Vec3) -> crate::gorc::channels::ReplicationPriority {
        crate::gorc::channels::ReplicationPriority::High
    }

    fn serialize_for_layer(&self, _layer: &ReplicationLayer) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        Ok(vec![])
    }

    fn get_layers(&self) -> Vec<ReplicationLayer> {
        // Create a layer with a very large zone that should trigger a warning
        vec![
            ReplicationLayer::new(0, 2000.0, 60.0, vec!["position".to_string()], CompressionType::Delta),
        ]
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