//! Simple protocol demonstration showing TCP/UDP with JSON/Binary serialization.
//!
//! This example demonstrates the compile-time selection of transport and serialization
//! combinations using the generalized communication stack.

use horizon_event_system::{
    CommunicationFactory, TcpJsonEndpoint, TcpBinaryEndpoint, 
    UdpJsonEndpoint, UdpBinaryEndpoint, TransportFactory, SerializerFactory,
    TcpConnection, UdpConnection,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::time::{sleep, Duration};

/// Example message that can be sent through any protocol combination.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct GameMessage {
    player_id: u32,
    message_type: String,
    data: serde_json::Value,
}

/// Demonstrates TCP + JSON communication (default web-compatible).
async fn demo_tcp_json() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== TCP + JSON Demo ===");
    
    let addr: SocketAddr = "127.0.0.1:0".parse()?;
    let mut listener = TransportFactory::tcp_listener(addr).await?;
    let server_addr = listener.local_addr()?;
    
    println!("Server listening on {}", server_addr);
    
    // Spawn server task
    let server_task = tokio::spawn(async move {
        if let Ok(connection) = listener.accept().await {
            let mut endpoint = CommunicationFactory::tcp_json()
                .build(Box::new(connection));
            
            // Receive message
            if let Ok(Some(msg)) = endpoint.receive_message::<GameMessage>().await {
                println!("TCP+JSON Server received: {:?}", msg);
                
                // Echo back
                let response = GameMessage {
                    player_id: msg.player_id,
                    message_type: "echo".to_string(),
                    data: serde_json::json!({"original": msg.data, "echoed": true}),
                };
                
                let _ = endpoint.send_message(&response).await;
            }
        }
    });
    
    // Give server time to start
    sleep(Duration::from_millis(100)).await;
    
    // Create client
    let stream = tokio::net::TcpStream::connect(server_addr).await?;
    let mut client_endpoint = CommunicationFactory::tcp_json()
        .build(Box::new(TcpConnection::new(stream, server_addr)));
    
    // Send message
    let message = GameMessage {
        player_id: 123,
        message_type: "greeting".to_string(),
        data: serde_json::json!({"text": "Hello, TCP+JSON!", "timestamp": 1234567890}),
    };
    
    client_endpoint.send_message(&message).await?;
    println!("TCP+JSON Client sent: {:?}", message);
    
    // Receive echo
    if let Some(response) = client_endpoint.receive_message::<GameMessage>().await? {
        println!("TCP+JSON Client received echo: {:?}", response);
    }
    
    server_task.abort();
    println!("TCP+JSON demo completed\n");
    Ok(())
}

/// Demonstrates TCP + Binary communication (high performance).
async fn demo_tcp_binary() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== TCP + Binary Demo ===");
    
    let addr: SocketAddr = "127.0.0.1:0".parse()?;
    let mut listener = TransportFactory::tcp_listener(addr).await?;
    let server_addr = listener.local_addr()?;
    
    println!("Server listening on {}", server_addr);
    
    // Print compatibility notes
    let builder = CommunicationFactory::tcp_binary();
    println!("Compatibility notes: {:?}", builder.compatibility_notes());
    
    // Spawn server task
    let server_task = tokio::spawn(async move {
        if let Ok(connection) = listener.accept().await {
            let mut endpoint = CommunicationFactory::tcp_binary()
                .build(Box::new(connection));
            
            if let Ok(Some(msg)) = endpoint.receive_message::<GameMessage>().await {
                println!("TCP+Binary Server received: {:?}", msg);
                
                let response = GameMessage {
                    player_id: msg.player_id,
                    message_type: "binary_echo".to_string(),
                    data: serde_json::json!({"compressed": true, "format": "binary"}),
                };
                
                let _ = endpoint.send_message(&response).await;
            }
        }
    });
    
    sleep(Duration::from_millis(100)).await;
    
    // Create client
    let stream = tokio::net::TcpStream::connect(server_addr).await?;
    let mut client_endpoint = CommunicationFactory::tcp_binary()
        .build(Box::new(TcpConnection::new(stream, server_addr)));
    
    let message = GameMessage {
        player_id: 456,
        message_type: "binary_greeting".to_string(),
        data: serde_json::json!({"text": "Hello, TCP+Binary!", "efficient": true}),
    };
    
    client_endpoint.send_message(&message).await?;
    println!("TCP+Binary Client sent: {:?}", message);
    
    if let Some(response) = client_endpoint.receive_message::<GameMessage>().await? {
        println!("TCP+Binary Client received echo: {:?}", response);
    }
    
    server_task.abort();
    println!("TCP+Binary demo completed\n");
    Ok(())
}

/// Demonstrates UDP + JSON communication (suboptimal but valid).
async fn demo_udp_json() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== UDP + JSON Demo ===");
    
    let addr: SocketAddr = "127.0.0.1:0".parse()?;
    let mut listener = TransportFactory::udp_listener(addr).await?;
    let server_addr = listener.local_addr()?;
    
    println!("Server listening on {}", server_addr);
    
    // Print compatibility notes
    let builder = CommunicationFactory::udp_json();
    println!("Compatibility notes: {:?}", builder.compatibility_notes());
    
    // Spawn server task
    let server_task = tokio::spawn(async move {
        if let Ok(connection) = listener.accept().await {
            let mut endpoint = CommunicationFactory::udp_json()
                .build(Box::new(connection));
            
            if let Ok(Some(msg)) = endpoint.receive_message::<GameMessage>().await {
                println!("UDP+JSON Server received: {:?}", msg);
                
                let response = GameMessage {
                    player_id: msg.player_id,
                    message_type: "udp_echo".to_string(),
                    data: serde_json::json!({"transport": "udp", "format": "json"}),
                };
                
                let _ = endpoint.send_message(&response).await;
            }
        }
    });
    
    sleep(Duration::from_millis(100)).await;
    
    // For UDP, we need to send first to establish the "connection"
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await?;
    let message = GameMessage {
        player_id: 789,
        message_type: "udp_greeting".to_string(),
        data: serde_json::json!({"text": "Hello, UDP+JSON!", "unreliable": true}),
    };
    
    let json_data = serde_json::to_vec(&message)?;
    socket.send_to(&json_data, server_addr).await?;
    println!("UDP+JSON Client sent: {:?}", message);
    
    server_task.abort();
    println!("UDP+JSON demo completed\n");
    Ok(())
}

/// Demonstrates UDP + Binary communication (optimal for UDP).
async fn demo_udp_binary() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== UDP + Binary Demo ===");
    
    let addr: SocketAddr = "127.0.0.1:0".parse()?;
    let mut listener = TransportFactory::udp_listener(addr).await?;
    let server_addr = listener.local_addr()?;
    
    println!("Server listening on {}", server_addr);
    
    // Print compatibility notes
    let builder = CommunicationFactory::udp_binary();
    println!("Compatibility notes: {:?}", builder.compatibility_notes());
    
    // Print transport and format info
    println!("Transport: {:?}", UdpBinaryEndpoint::transport_info());
    println!("Format: {:?}", UdpBinaryEndpoint::format_info());
    
    // Spawn server task
    let server_task = tokio::spawn(async move {
        if let Ok(connection) = listener.accept().await {
            let mut endpoint = CommunicationFactory::udp_binary()
                .build(Box::new(connection));
            
            if let Ok(Some(msg)) = endpoint.receive_message::<GameMessage>().await {
                println!("UDP+Binary Server received: {:?}", msg);
                
                let response = GameMessage {
                    player_id: msg.player_id,
                    message_type: "udp_binary_echo".to_string(),
                    data: serde_json::json!({"transport": "udp", "format": "binary", "optimal": true}),
                };
                
                let _ = endpoint.send_message(&response).await;
            }
        }
    });
    
    sleep(Duration::from_millis(100)).await;
    
    // Send binary UDP message
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await?;
    let message = GameMessage {
        player_id: 999,
        message_type: "udp_binary_greeting".to_string(),
        data: serde_json::json!({"text": "Hello, UDP+Binary!", "compact": true}),
    };
    
    let serializer = SerializerFactory::binary();
    let binary_data = serializer.serialize(&message)?;
    socket.send_to(&binary_data, server_addr).await?;
    println!("UDP+Binary Client sent: {:?}", message);
    
    server_task.abort();
    println!("UDP+Binary demo completed\n");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Simple Protocol Demo - TCP/UDP with JSON/Binary");
    println!("===================================================\n");
    
    // Demonstrate all four combinations
    demo_tcp_json().await?;
    demo_tcp_binary().await?;
    demo_udp_json().await?;
    demo_udp_binary().await?;
    
    println!("✅ All protocol combinations demonstrated successfully!");
    println!("\nKey features shown:");
    println!("- Compile-time protocol validation");
    println!("- Zero-cost abstractions");
    println!("- Type-safe serialization");
    println!("- Transport protocol flexibility");
    println!("- Backward compatibility with TCP+JSON");
    
    Ok(())
}