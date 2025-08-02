//! Simple UDP demonstration with binary serialization.
//!
//! This example shows UDP-specific functionality and how it differs from TCP,
//! including message boundaries and connectionless operation.

use horizon_event_system::{
    CommunicationFactory, UdpBinaryEndpoint, TransportFactory, SerializerFactory,
    UdpConnection, TransportProtocol, UdpTransport, SerializationFormat, BinaryFormat,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::time::{sleep, Duration, timeout};

/// High-frequency game state update for UDP.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct GameStateUpdate {
    sequence: u32,
    player_id: u32,
    position: [f32; 3],
    velocity: [f32; 3],
    timestamp: u64,
}

/// Low-level UDP operations demonstration.
async fn demo_udp_characteristics() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== UDP Characteristics Demo ===");
    
    // Show transport characteristics
    println!("UDP Transport Info:");
    println!("  Name: {}", UdpTransport::NAME);
    println!("  Connection-oriented: {}", UdpTransport::CONNECTION_ORIENTED);
    println!("  Reliable: {}", UdpTransport::RELIABLE);
    
    println!("Binary Format Info:");
    println!("  Name: {}", BinaryFormat::NAME);
    println!("  Human-readable: {}", BinaryFormat::HUMAN_READABLE);
    println!("  MIME Type: {}", BinaryFormat::MIME_TYPE);
    
    Ok(())
}

/// Demonstrates message boundaries and packet loss simulation.
async fn demo_message_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== UDP Message Boundaries Demo ===");
    
    let addr: SocketAddr = "127.0.0.1:0".parse()?;
    let mut listener = TransportFactory::udp_listener(addr).await?;
    let server_addr = listener.local_addr()?;
    
    println!("UDP Server listening on {}", server_addr);
    
    // Spawn server that processes individual messages
    let server_task = tokio::spawn(async move {
        let mut message_count = 0;
        
        // Accept multiple "connections" (really just different client addresses)
        for _ in 0..3 {
            if let Ok(connection) = listener.accept().await {
                let mut endpoint = CommunicationFactory::udp_binary()
                    .build(Box::new(connection), Box::new(SerializerFactory::binary()));
                
                while let Ok(Some(update)) = timeout(
                    Duration::from_millis(100),
                    endpoint.receive_message::<GameStateUpdate>()
                ).await {
                    message_count += 1;
                    println!("  Received message #{}: seq={}, pos={:?}", 
                            message_count, update.sequence, update.position);
                    
                    if message_count >= 5 {
                        break;
                    }
                }
            }
        }
        
        println!("Server processed {} messages", message_count);
    });
    
    sleep(Duration::from_millis(50)).await;
    
    // Create client and send multiple discrete messages
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await?;
    let serializer = SerializerFactory::binary();
    
    println!("Sending discrete UDP messages...");
    for i in 0..5 {
        let update = GameStateUpdate {
            sequence: i,
            player_id: 100,
            position: [i as f32 * 10.0, 20.0, 30.0],
            velocity: [1.0, 0.0, 0.0],
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis() as u64,
        };
        
        let data = serializer.serialize(&update)?;
        socket.send_to(&data, server_addr).await?;
        println!("  Sent message #{}: seq={}", i + 1, update.sequence);
        
        // Small delay to ensure message boundaries
        sleep(Duration::from_millis(10)).await;
    }
    
    // Give server time to process
    sleep(Duration::from_millis(200)).await;
    server_task.abort();
    
    Ok(())
}

/// Demonstrates high-frequency updates suitable for real-time games.
async fn demo_high_frequency_updates() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== High-Frequency Updates Demo ===");
    
    let addr: SocketAddr = "127.0.0.1:0".parse()?;
    let mut listener = TransportFactory::udp_listener(addr).await?;
    let server_addr = listener.local_addr()?;
    
    println!("Starting high-frequency update simulation...");
    
    // Server that tracks update rates
    let server_task = tokio::spawn(async move {
        if let Ok(connection) = listener.accept().await {
            let mut endpoint = CommunicationFactory::udp_binary()
                .build(Box::new(connection));
            
            let mut last_seq = 0u32;
            let mut received_count = 0;
            let start_time = std::time::Instant::now();
            
            while let Ok(Some(update)) = timeout(
                Duration::from_secs(2),
                endpoint.receive_message::<GameStateUpdate>()
            ).await {
                received_count += 1;
                
                // Check for packet loss (sequence gaps)
                if update.sequence > last_seq + 1 {
                    println!("  ⚠️  Detected packet loss: expected seq {}, got {}", 
                            last_seq + 1, update.sequence);
                }
                last_seq = update.sequence;
                
                if received_count % 20 == 0 {
                    let elapsed = start_time.elapsed().as_secs_f32();
                    let rate = received_count as f32 / elapsed;
                    println!("  📊 Received {} updates, rate: {:.1} Hz", received_count, rate);
                }
            }
            
            let total_time = start_time.elapsed().as_secs_f32();
            println!("Final stats: {} updates in {:.2}s = {:.1} Hz", 
                    received_count, total_time, received_count as f32 / total_time);
        }
    });
    
    sleep(Duration::from_millis(50)).await;
    
    // Client sending at ~60 Hz
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await?;
    let serializer = SerializerFactory::binary();
    
    println!("Sending updates at ~60 Hz for 1 second...");
    let start = std::time::Instant::now();
    let mut sequence = 0u32;
    
    while start.elapsed() < Duration::from_secs(1) {
        sequence += 1;
        
        let update = GameStateUpdate {
            sequence,
            player_id: 200,
            position: [
                (sequence as f32 * 0.1).sin() * 50.0,
                (sequence as f32 * 0.05).cos() * 25.0,
                0.0
            ],
            velocity: [1.0, 0.5, 0.0],
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis() as u64,
        };
        
        let data = serializer.serialize(&update)?;
        
        // Simulate occasional packet loss (5% chance)
        if sequence % 20 != 0 { // Skip every 20th packet
            socket.send_to(&data, server_addr).await?;
        }
        
        // Target ~60 FPS
        sleep(Duration::from_millis(16)).await;
    }
    
    println!("Sent {} updates", sequence);
    
    // Give server time to finish processing
    sleep(Duration::from_millis(200)).await;
    server_task.abort();
    
    Ok(())
}

/// Demonstrates binary format efficiency for UDP.
async fn demo_binary_efficiency() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Binary Format Efficiency Demo ===");
    
    let update = GameStateUpdate {
        sequence: 12345,
        player_id: 67890,
        position: [123.456, 789.012, -345.678],
        velocity: [9.876, -5.432, 1.098],
        timestamp: 1234567890123,
    };
    
    // Compare JSON vs Binary sizes
    let json_serializer = SerializerFactory::json();
    let binary_serializer = SerializerFactory::binary();
    
    let json_data = json_serializer.serialize(&update)?;
    let binary_data = binary_serializer.serialize(&update)?;
    
    println!("Serialization comparison for GameStateUpdate:");
    println!("  JSON size: {} bytes", json_data.len());
    println!("  Binary size: {} bytes", binary_data.len());
    println!("  Binary is {:.1}% smaller", 
            (1.0 - binary_data.len() as f32 / json_data.len() as f32) * 100.0);
    
    // Verify round-trip
    let json_roundtrip: GameStateUpdate = json_serializer.deserialize(&json_data)?;
    let binary_roundtrip: GameStateUpdate = binary_serializer.deserialize(&binary_data)?;
    
    assert_eq!(update, json_roundtrip);
    assert_eq!(update, binary_roundtrip);
    println!("  ✅ Both formats maintain data integrity");
    
    // Show binary format structure
    println!("  Binary format structure: {:02X?}...", &binary_data[..8.min(binary_data.len())]);
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Simple UDP Demo - Binary Serialization for Real-time Games");
    println!("============================================================");
    
    demo_udp_characteristics().await?;
    demo_message_boundaries().await?;
    demo_high_frequency_updates().await?;
    demo_binary_efficiency().await?;
    
    println!("\n✅ UDP + Binary demonstration completed!");
    println!("\nKey UDP characteristics shown:");
    println!("- Connectionless operation with message boundaries");
    println!("- High-frequency updates suitable for real-time games");
    println!("- Packet loss detection and handling");
    println!("- Binary format efficiency for bandwidth optimization");
    println!("- Zero-copy message processing where possible");
    
    Ok(())
}