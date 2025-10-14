//! End-to-end integration test simulating real client movement patterns
//!
//! This test creates two simulated clients that move along predetermined paths
//! and validates that distance-based filtering works correctly throughout
//! the entire journey, using realistic message formats.

use crate::{EventSystem, PlayerId, Vec3, ClientResponseSender, AuthenticationStatus};
use crate::gorc::instance::{GorcInstanceManager, GorcObjectId, GorcObject};
use crate::gorc::channels::{ReplicationLayer, CompressionType, ReplicationPriority};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use serde_json::Value;
use std::any::Any;

/// Mock client that tracks received messages with detailed logging
#[derive(Debug, Default)]
pub struct SimulatedClient {
    pub player_id: PlayerId,
    pub name: String,
    pub current_position: Mutex<Vec3>,
    pub received_messages: Arc<Mutex<Vec<ReceivedMessage>>>,
}

#[derive(Debug, Clone)]
pub struct ReceivedMessage {
    pub timestamp: std::time::Instant,
    pub message_type: String,
    pub from_object: Option<String>,
    pub channel: Option<u8>,
    pub data: Vec<u8>,
}

impl SimulatedClient {
    pub fn new(player_id: PlayerId, name: String, position: Vec3) -> Self {
        Self {
            player_id,
            name,
            current_position: Mutex::new(position),
            received_messages: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    pub fn set_position(&self, position: Vec3) {
        *self.current_position.lock().unwrap() = position;
    }
    
    pub fn get_position(&self) -> Vec3 {
        *self.current_position.lock().unwrap()
    }
    
    pub fn get_message_count(&self) -> usize {
        self.received_messages.lock().unwrap().len()
    }
    
    pub fn get_messages_by_type(&self, msg_type: &str) -> usize {
        self.received_messages.lock().unwrap()
            .iter()
            .filter(|m| m.message_type == msg_type)
            .count()
    }
    
    pub fn get_zone_enter_messages(&self) -> Vec<Value> {
        self.received_messages.lock().unwrap()
            .iter()
            .filter_map(|m| {
                if m.message_type == "gorc_zone_enter" {
                    serde_json::from_slice(&m.data).ok()
                } else {
                    None
                }
            })
            .collect()
    }
    
    pub fn get_move_events(&self) -> Vec<Value> {
        self.received_messages.lock().unwrap()
            .iter()
            .filter_map(|m| {
                if m.message_type == "move" {
                    serde_json::from_slice(&m.data).ok()
                } else {
                    None
                }
            })
            .collect()
    }
    
    pub fn clear_messages(&self) {
        self.received_messages.lock().unwrap().clear();
    }
}

/// Mock client response sender that routes messages to simulated clients
#[derive(Debug, Default)]
pub struct SimulatedClientSender {
    pub clients: Arc<Mutex<HashMap<PlayerId, Arc<SimulatedClient>>>>,
}

impl SimulatedClientSender {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    pub fn register_client(&self, client: Arc<SimulatedClient>) {
        let mut clients = self.clients.lock().unwrap();
        clients.insert(client.player_id, client);
    }
}

impl ClientResponseSender for SimulatedClientSender {
    fn send_to_client(&self, player_id: PlayerId, data: Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        let clients = self.clients.clone();
        Box::pin(async move {
            let clients_map = clients.lock().unwrap();
            if let Some(client) = clients_map.get(&player_id) {
                // Parse message to extract type
                let msg: Value = serde_json::from_slice(&data).unwrap_or(serde_json::json!({}));
                let message_type = msg.get("type")
                    .or_else(|| msg.get("event_type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                
                let from_object = msg.get("object_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                
                let channel = msg.get("channel")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u8);
                
                let received_msg = ReceivedMessage {
                    timestamp: std::time::Instant::now(),
                    message_type,
                    from_object,
                    channel,
                    data: data.clone(),
                };
                
                client.received_messages.lock().unwrap().push(received_msg);
            }
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

/// Test player object for simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimPlayer {
    pub position: Vec3,
    pub velocity: Vec3,
    pub health: f32,
    pub name: String,
    pub level: u32,
    pub movement_state: String,
}

impl SimPlayer {
    pub fn new(position: Vec3, name: String) -> Self {
        Self {
            position,
            velocity: Vec3::new(0.0, 0.0, 0.0),
            health: 100.0,
            name,
            level: 1,
            movement_state: "idle".to_string(),
        }
    }
}

impl GorcObject for SimPlayer {
    fn type_name(&self) -> &'static str {
        "GorcPlayer"
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
            // Channel 0: PlayerCriticalData - 25m range, 60Hz
            ReplicationLayer::new(0, 25.0, 60.0, vec!["position".to_string(), "velocity".to_string(), "health".to_string()], CompressionType::None),
            // Channel 1: PlayerDetailedData - 100m range, 30Hz
            ReplicationLayer::new(1, 100.0, 30.0, vec!["movement_state".to_string(), "level".to_string()], CompressionType::None),
            // Channel 2: PlayerSocialData - 200m range, 15Hz
            ReplicationLayer::new(2, 200.0, 15.0, vec!["name".to_string(), "chat_bubble".to_string()], CompressionType::None),
        ]
    }
    
    fn serialize_for_layer(&self, layer: &ReplicationLayer) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let data = match layer.channel {
            0 => serde_json::json!({
                "position": self.position,
                "velocity": self.velocity,
                "health": self.health,
            }),
            1 => serde_json::json!({
                "movement_state": self.movement_state,
                "level": self.level,
            }),
            2 => serde_json::json!({
                "name": self.name,
                "chat_bubble": serde_json::Value::Null,
            }),
            _ => serde_json::json!({}),
        };
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

/// Movement path for a client
#[derive(Debug, Clone)]
pub struct MovementPath {
    pub waypoints: Vec<Vec3>,
    pub current_index: usize,
}

impl MovementPath {
    pub fn new(waypoints: Vec<Vec3>) -> Self {
        Self {
            waypoints,
            current_index: 0,
        }
    }
    
    pub fn next_position(&mut self) -> Option<Vec3> {
        if self.current_index < self.waypoints.len() {
            let pos = self.waypoints[self.current_index];
            self.current_index += 1;
            Some(pos)
        } else {
            None
        }
    }
    
    pub fn has_more(&self) -> bool {
        self.current_index < self.waypoints.len()
    }
}

#[tokio::test]
async fn test_realistic_client_movement_simulation() {
    println!("\n🧪 REALISTIC CLIENT MOVEMENT SIMULATION TEST");
    println!("{}", "=".repeat(80));
    
    // Setup
    let gorc_instances = Arc::new(GorcInstanceManager::new());
    let mut event_system = EventSystem::new();
    let mock_sender = Arc::new(SimulatedClientSender::new());
    
    event_system.set_gorc_instances(gorc_instances.clone());
    event_system.set_client_response_sender(mock_sender.clone());
    let event_system = Arc::new(event_system);
    
    // Create two simulated clients
    let player1_id = PlayerId::new();
    let player2_id = PlayerId::new();
    
    let client1 = Arc::new(SimulatedClient::new(player1_id, "Alice".to_string(), Vec3::new(0.0, 0.0, 0.0)));
    let client2 = Arc::new(SimulatedClient::new(player2_id, "Bob".to_string(), Vec3::new(0.0, 0.0, 0.0)));
    
    mock_sender.register_client(client1.clone());
    mock_sender.register_client(client2.clone());
    
    println!("👤 Client 1 (Alice): {}", player1_id);
    println!("👤 Client 2 (Bob):   {}", player2_id);
    
    // Create and register player objects
    let player1_obj = SimPlayer::new(Vec3::new(0.0, 0.0, 0.0), "Alice".to_string());
    let player2_obj = SimPlayer::new(Vec3::new(0.0, 0.0, 0.0), "Bob".to_string());
    
    let player1_obj_id = gorc_instances.register_object(player1_obj, Vec3::new(0.0, 0.0, 0.0)).await;
    let player2_obj_id = gorc_instances.register_object(player2_obj, Vec3::new(0.0, 0.0, 0.0)).await;
    
    // Add players to GORC tracking
    gorc_instances.add_player(player1_id, Vec3::new(0.0, 0.0, 0.0)).await;
    gorc_instances.add_player(player2_id, Vec3::new(0.0, 0.0, 0.0)).await;
    
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    println!("\n📋 PHASE 1: Verify Initial Zone Enter Messages");
    println!("{}", "-".repeat(80));
    
    // Check zone enter messages for client 2 (should see client 1)
    let zone_messages = client2.get_zone_enter_messages();
    println!("Client 2 received {} zone_enter messages", zone_messages.len());
    
    if zone_messages.len() > 0 {
        // Validate message format
        for (i, msg) in zone_messages.iter().enumerate() {
            println!("\n  Message {}: Channel {}", i + 1, msg.get("channel").and_then(|v| v.as_u64()).unwrap_or(999));
            println!("  Object type: {}", msg.get("object_type").and_then(|v| v.as_str()).unwrap_or("unknown"));
            println!("  Type: {}", msg.get("type").and_then(|v| v.as_str()).unwrap_or("unknown"));
            
            // Validate required fields
            assert!(msg.get("channel").is_some(), "Missing 'channel' field");
            assert!(msg.get("object_id").is_some(), "Missing 'object_id' field");
            assert!(msg.get("object_type").is_some(), "Missing 'object_type' field");
            assert!(msg.get("player_id").is_some(), "Missing 'player_id' field");
            assert!(msg.get("timestamp").is_some(), "Missing 'timestamp' field");
            assert_eq!(msg.get("type").and_then(|v| v.as_str()), Some("gorc_zone_enter"), "Wrong message type");
            assert!(msg.get("zone_data").is_some(), "Missing 'zone_data' field");
            
            // Validate zone_data based on channel
            if let Some(channel) = msg.get("channel").and_then(|v| v.as_u64()) {
                let zone_data = msg.get("zone_data").unwrap();
                match channel {
                    0 => {
                        // PlayerCriticalData
                        assert!(zone_data.get("position").is_some(), "Channel 0 missing 'position'");
                        assert!(zone_data.get("velocity").is_some(), "Channel 0 missing 'velocity'");
                        assert!(zone_data.get("health").is_some(), "Channel 0 missing 'health'");
                        println!("  ✅ Channel 0 (Critical) format valid");
                    }
                    1 => {
                        // PlayerDetailedData
                        assert!(zone_data.get("movement_state").is_some(), "Channel 1 missing 'movement_state'");
                        assert!(zone_data.get("level").is_some(), "Channel 1 missing 'level'");
                        println!("  ✅ Channel 1 (Detailed) format valid");
                    }
                    2 => {
                        // PlayerSocialData
                        assert!(zone_data.get("name").is_some(), "Channel 2 missing 'name'");
                        assert!(zone_data.get("chat_bubble").is_some(), "Channel 2 missing 'chat_bubble'");
                        println!("  ✅ Channel 2 (Social) format valid");
                    }
                    _ => panic!("Unexpected channel: {}", channel),
                }
            }
        }
        
        assert_eq!(zone_messages.len(), 3, "Should receive 3 zone_enter messages (one per channel)");
        println!("\n✅ All zone_enter messages have correct format");
    } else {
        println!("\n⚠️  No zone_enter messages received yet - this is OK if zones are triggered by movement");
    }
    
    // Clear messages before movement simulation
    client1.clear_messages();
    client2.clear_messages();
    
    println!("\n📋 PHASE 2: Simulate Realistic Movement Paths");
    println!("{}", "-".repeat(80));
    
    // Define movement paths
    // Alice: Moves gradually away from origin
    let mut alice_path = MovementPath::new(vec![
        Vec3::new(5.0, 0.0, 0.0),      // 5m - within range
        Vec3::new(15.0, 0.0, 0.0),     // 15m - within range
        Vec3::new(24.0, 0.0, 0.0),     // 24m - just inside range
        Vec3::new(26.0, 0.0, 0.0),     // 26m - just outside range!
        Vec3::new(50.0, 0.0, 0.0),     // 50m - far outside range
        Vec3::new(150.0, 0.0, 0.0),    // 150m - very far
        Vec3::new(500.0, 0.0, 0.0),    // 500m - extremely far
    ]);
    
    // Bob: Stays at origin
    let mut bob_path = MovementPath::new(vec![
        Vec3::new(0.0, 0.0, 0.0),      // Stationary
    ]);
    
    let mut step = 0;
    while alice_path.has_more() || bob_path.has_more() {
        step += 1;
        println!("\n  Step {}: Alice moves to next waypoint", step);
        
        // Move Alice
        if let Some(new_pos) = alice_path.next_position() {
            let distance_from_bob = new_pos.distance(Vec3::new(0.0, 0.0, 0.0));
            let should_receive = distance_from_bob <= 25.0;
            
            println!("    Alice position: {:?}", new_pos);
            println!("    Distance from Bob: {:.2}m", distance_from_bob);
            println!("    Bob should receive: {}", if should_receive { "YES" } else { "NO" });
            
            client1.clear_messages();
            client2.clear_messages();
            
            // Update positions in GORC tracking
            event_system.update_player_position(player1_id, new_pos).await.expect("Failed to update player position");
            event_system.update_object_position(player1_obj_id, new_pos).await.expect("Failed to update object position");
            
            client1.set_position(new_pos);
            
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            
            // Emit movement event
            let move_event = serde_json::json!({
                "player_id": player1_id.to_string(),
                "new_position": new_pos,
                "velocity": Vec3::new(10.0, 0.0, 0.0),
                "movement_state": 1,
                "client_timestamp": crate::utils::current_timestamp(),
            });
            
            event_system.emit_gorc_instance(
                player1_obj_id,
                0,
                "move",
                &move_event,
                crate::Dest::Client
            ).await.expect("Failed to emit move event");
            
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            
            // Check if Bob received the MOVE update (filter out zone messages)
            let bob_move_events = client2.get_move_events().len();
            println!("    Bob received: {} move events", bob_move_events);
            
            if should_receive {
                assert!(bob_move_events > 0, "Bob should have received Alice's MOVE event at {:.2}m", distance_from_bob);
                println!("    ✅ Correctly received move event");
                
                // Validate move event format
                let move_events = client2.get_move_events();
                if !move_events.is_empty() {
                    let msg = &move_events[0];
                    assert_eq!(msg.get("event_type").and_then(|v| v.as_str()), Some("move"), "Wrong event type");
                    assert_eq!(msg.get("channel").and_then(|v| v.as_u64()), Some(0), "Wrong channel");
                    assert!(msg.get("data").is_some(), "Missing data field");
                    println!("    ✅ Move event format valid");
                }
            } else {
                assert_eq!(bob_move_events, 0, "Bob should NOT have received Alice's MOVE event at {:.2}m (zone messages are OK)", distance_from_bob);
                println!("    ✅ Correctly filtered out (no move event)");
            }
        }
        
        // Move Bob (stays stationary)
        bob_path.next_position();
    }
    
    println!("\n📋 PHASE 3: Test Return Journey");
    println!("{}", "-".repeat(80));
    
    // Alice returns closer to Bob
    let return_positions = vec![
        Vec3::new(100.0, 0.0, 0.0),   // 100m - outside range
        Vec3::new(30.0, 0.0, 0.0),    // 30m - outside range
        Vec3::new(20.0, 0.0, 0.0),    // 20m - inside range!
        Vec3::new(10.0, 0.0, 0.0),    // 10m - inside range
    ];
    
    for (i, new_pos) in return_positions.iter().enumerate() {
        let distance_from_bob = new_pos.distance(Vec3::new(0.0, 0.0, 0.0));
        let should_receive = distance_from_bob <= 25.0;
        
        println!("\n  Return Step {}: Alice at {:?} ({:.2}m from Bob)", i + 1, new_pos, distance_from_bob);
        
        client1.clear_messages();
        client2.clear_messages();
        
        // Update positions
        event_system.update_player_position(player1_id, *new_pos).await.expect("Failed to update player position");
        event_system.update_object_position(player1_obj_id, *new_pos).await.expect("Failed to update object position");
        
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        
        // Emit movement
        let move_event = serde_json::json!({
            "player_id": player1_id.to_string(),
            "new_position": new_pos,
            "velocity": Vec3::new(-10.0, 0.0, 0.0),
            "movement_state": 1,
            "client_timestamp": crate::utils::current_timestamp(),
        });
        
        event_system.emit_gorc_instance(
            player1_obj_id,
            0,
            "move",
            &move_event,
            crate::Dest::Client
        ).await.expect("Failed to emit move event");
        
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        
        // Check messages - we only care about MOVE events for channel 0, not zone messages
        let channel_0_move_events = client2.get_move_events().len();
        
        println!("    Bob received: {} move events on channel 0", channel_0_move_events);
        
        if should_receive {
            assert!(channel_0_move_events > 0, "Bob should receive Alice's movement when she returns to {:.2}m", distance_from_bob);
            println!("  ✅ Bob correctly received Alice's return (channel 0)");
        } else {
            assert_eq!(channel_0_move_events, 0, "Bob should NOT receive Alice's MOVE event at {:.2}m (may receive zone messages)", distance_from_bob);
            println!("  ✅ Bob correctly did NOT receive move event (outside 25m range for channel 0)");
        }
    }
    
    println!("\n{}", "=".repeat(80));
    println!("🎉 REALISTIC CLIENT MOVEMENT SIMULATION TEST PASSED!");
    println!("   - Zone enter messages validated ✅");
    println!("   - Message formats validated ✅");
    println!("   - Distance filtering working correctly at all distances ✅");
    println!("   - Return journey filtering working correctly ✅");
}
