//! UDP event example with binary deserialization and mock UDP handling.
//!
//! This example demonstrates how to integrate UDP communication with the
//! Horizon event system for real-time game events.

use horizon_event_system::{
    EventSystem, create_horizon_event_system, RawClientMessageEvent,
    CommunicationFactory, UdpBinaryEndpoint, TransportFactory, SerializerFactory,
    current_timestamp, PlayerId,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

/// Real-time movement event sent over UDP.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MovementEvent {
    player_id: u32,
    position: [f32; 3],
    velocity: [f32; 3],
    rotation: [f32; 4], // quaternion
    timestamp: u64,
}

/// Combat event that needs reliable delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CombatEvent {
    attacker_id: u32,
    target_id: u32,
    damage: f32,
    weapon_type: String,
    timestamp: u64,
}

/// Chat message that should be human-readable.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatEvent {
    player_id: u32,
    message: String,
    channel: String,
    timestamp: u64,
}

/// UDP event router that integrates with the Horizon event system.
struct UdpEventRouter {
    event_system: Arc<EventSystem>,
    serializer: SerializerFactory,
}

impl UdpEventRouter {
    fn new(event_system: Arc<EventSystem>) -> Self {
        Self {
            event_system,
            serializer: SerializerFactory,
        }
    }
    
    /// Route a UDP binary message to the appropriate event handler.
    async fn route_udp_message(&self, data: &[u8], player_id: PlayerId) -> Result<(), Box<dyn std::error::Error>> {
        let binary_serializer = self.serializer.binary();
        
        // Try to determine message type from binary data
        // In a real implementation, you'd have a message header with type info
        
        // For demo, we'll try to deserialize as different types
        if let Ok(movement) = binary_serializer.deserialize::<MovementEvent>(data) {
            self.handle_movement_event(movement, player_id).await?;
        } else if let Ok(combat) = binary_serializer.deserialize::<CombatEvent>(data) {
            self.handle_combat_event(combat, player_id).await?;
        } else {
            println!("⚠️  Unknown UDP message format from player {}", player_id);
        }
        
        Ok(())
    }
    
    async fn handle_movement_event(&self, event: MovementEvent, player_id: PlayerId) -> Result<(), Box<dyn std::error::Error>> {
        println!("🏃 Movement from player {}: pos={:?}, vel={:?}", 
                event.player_id, event.position, event.velocity);
        
        // Emit to event system for plugins to handle
        let raw_event = RawClientMessageEvent {
            player_id,
            message_type: "movement:update".to_string(),
            data: serde_json::to_vec(&event)?,
            timestamp: current_timestamp(),
        };
        
        self.event_system.emit_core("raw_client_message", &raw_event).await?;
        
        // Also emit directly to movement namespace
        self.event_system.emit_client_with_context(
            "movement", 
            "realtime_update", 
            player_id, 
            &serde_json::to_value(event)?
        ).await?;
        
        Ok(())
    }
    
    async fn handle_combat_event(&self, event: CombatEvent, player_id: PlayerId) -> Result<(), Box<dyn std::error::Error>> {
        println!("⚔️  Combat from player {}: {} dmg to {} with {}", 
                event.attacker_id, event.damage, event.target_id, event.weapon_type);
        
        // Combat events are critical and should be reliable
        // In a real system, you might want to use TCP for these
        println!("⚠️  Combat event received via UDP - consider TCP for reliability");
        
        let raw_event = RawClientMessageEvent {
            player_id,
            message_type: "combat:action".to_string(),
            data: serde_json::to_vec(&event)?,
            timestamp: current_timestamp(),
        };
        
        self.event_system.emit_core("raw_client_message", &raw_event).await?;
        self.event_system.emit_client_with_context(
            "combat", 
            "attack", 
            player_id, 
            &serde_json::to_value(event)?
        ).await?;
        
        Ok(())
    }
}

/// Mock UDP server that handles game events.
async fn run_udp_event_server(addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    let mut listener = TransportFactory::udp_listener(addr).await?;
    let server_addr = listener.local_addr()?;
    println!("🌐 UDP Event Server listening on {}", server_addr);
    
    // Create event system
    let event_system = Arc::new(create_horizon_event_system());
    let router = UdpEventRouter::new(event_system.clone());
    
    // Register event handlers
    event_system.on_client("movement", "realtime_update", |event: RawClientMessageEvent| {
        println!("📍 Plugin received movement update from player {}", event.player_id);
        Ok(())
    }).await?;
    
    event_system.on_client("combat", "attack", |event: RawClientMessageEvent| {
        println!("🗡️  Plugin received combat action from player {}", event.player_id);
        Ok(())
    }).await?;
    
    // Handle incoming UDP connections
    let mut connection_count = 0;
    while connection_count < 3 { // Limit for demo
        if let Ok(connection) = listener.accept().await {
            connection_count += 1;
            let player_id = PlayerId::new();
            let remote_addr = connection.remote_addr();
            println!("🔗 New UDP connection #{} from {} (Player: {})", 
                    connection_count, remote_addr, player_id);
            
            let mut endpoint = CommunicationFactory::udp_binary()
                .build(Box::new(connection), Box::new(SerializerFactory::binary()));
            
            let router = router.clone();
            tokio::spawn(async move {
                while let Ok(Some(data)) = endpoint.receive_message::<Vec<u8>>().await {
                    if let Err(e) = router.route_udp_message(&data, player_id).await {
                        println!("❌ Error routing UDP message: {}", e);
                    }
                }
                println!("🔌 UDP connection for player {} closed", player_id);
            });
        }
    }
    
    Ok(())
}

/// Simulate multiple clients sending different types of events.
async fn simulate_udp_clients(server_addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🎮 Simulating UDP game clients...");
    
    let binary_serializer = SerializerFactory::binary();
    
    // Client 1: Rapid movement updates
    let client1_task = {
        let server_addr = server_addr;
        let serializer = binary_serializer.clone();
        tokio::spawn(async move {
            let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await?;
            
            for i in 0..10 {
                let movement = MovementEvent {
                    player_id: 1001,
                    position: [i as f32 * 2.0, 10.0, 20.0],
                    velocity: [2.0, 0.0, 0.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    timestamp: current_timestamp(),
                };
                
                let data = serializer.serialize(&movement)?;
                socket.send_to(&data, server_addr).await?;
                
                sleep(Duration::from_millis(50)).await; // 20 Hz updates
            }
            
            println!("📱 Client 1 sent 10 movement updates");
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    };
    
    // Client 2: Combat events
    let client2_task = {
        let server_addr = server_addr;
        let serializer = binary_serializer.clone();
        tokio::spawn(async move {
            let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await?;
            
            sleep(Duration::from_millis(100)).await; // Offset timing
            
            for i in 0..3 {
                let combat = CombatEvent {
                    attacker_id: 1002,
                    target_id: 1001,
                    damage: 15.0 + i as f32 * 5.0,
                    weapon_type: "sword".to_string(),
                    timestamp: current_timestamp(),
                };
                
                let data = serializer.serialize(&combat)?;
                socket.send_to(&data, server_addr).await?;
                
                sleep(Duration::from_millis(200)).await; // Combat less frequent
            }
            
            println!("⚔️  Client 2 sent 3 combat events");
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    };
    
    // Client 3: Mixed events
    let client3_task = {
        let server_addr = server_addr;
        let serializer = binary_serializer;
        tokio::spawn(async move {
            let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await?;
            
            sleep(Duration::from_millis(75)).await; // Different offset
            
            // Send some movement
            for i in 0..5 {
                let movement = MovementEvent {
                    player_id: 1003,
                    position: [-i as f32, 5.0, 30.0],
                    velocity: [-1.0, 0.0, 0.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    timestamp: current_timestamp(),
                };
                
                let data = serializer.serialize(&movement)?;
                socket.send_to(&data, server_addr).await?;
                
                sleep(Duration::from_millis(60)).await;
            }
            
            // Send a combat event
            let combat = CombatEvent {
                attacker_id: 1003,
                target_id: 1002,
                damage: 25.0,
                weapon_type: "magic".to_string(),
                timestamp: current_timestamp(),
            };
            
            let data = serializer.serialize(&combat)?;
            socket.send_to(&data, server_addr).await?;
            
            println!("🔮 Client 3 sent mixed events");
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    };
    
    // Wait for all clients to finish
    let _ = tokio::try_join!(client1_task, client2_task, client3_task)?;
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 UDP Event Example - Binary Deserialization with Event System");
    println!("================================================================");
    
    let server_addr: SocketAddr = "127.0.0.1:0".parse()?;
    
    // Start the UDP event server
    let server_task = tokio::spawn(async move {
        if let Err(e) = run_udp_event_server(server_addr).await {
            println!("❌ Server error: {}", e);
        }
    });
    
    // Give server time to start
    sleep(Duration::from_millis(100)).await;
    
    // Get the actual server address
    // For demo, we'll use a fixed port
    let actual_addr: SocketAddr = "127.0.0.1:9999".parse()?;
    
    // Start a simple UDP listener for demo
    let socket = tokio::net::UdpSocket::bind(actual_addr).await?;
    let server_addr = socket.local_addr()?;
    
    // Spawn handler
    let handler_task = tokio::spawn(async move {
        let event_system = Arc::new(create_horizon_event_system());
        let router = UdpEventRouter::new(event_system.clone());
        
        let mut buf = vec![0u8; 1024];
        for _ in 0..20 { // Process up to 20 messages
            if let Ok((len, addr)) = tokio::time::timeout(
                Duration::from_secs(2),
                socket.recv_from(&mut buf)
            ).await {
                let data = &buf[..len];
                let player_id = PlayerId::new(); // Would map from addr in real system
                
                if let Err(e) = router.route_udp_message(data, player_id).await {
                    println!("❌ Error processing UDP message from {}: {}", addr, e);
                }
            } else {
                break; // Timeout
            }
        }
    });
    
    // Simulate clients
    sleep(Duration::from_millis(100)).await;
    simulate_udp_clients(server_addr).await?;
    
    // Wait for processing to complete
    sleep(Duration::from_millis(500)).await;
    
    handler_task.abort();
    server_task.abort();
    
    println!("\n✅ UDP Event Example completed!");
    println!("\nKey features demonstrated:");
    println!("- UDP binary message deserialization");
    println!("- Integration with Horizon event system");
    println!("- Real-time movement updates at high frequency");
    println!("- Mixed event types over single UDP connection");
    println!("- Automatic routing to plugin handlers");
    println!("- Type-safe event processing");
    
    Ok(())
}