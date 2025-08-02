//! Simplified example showing event classes working with the universal plugin system

use universal_plugin_system::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// Sample events
#[derive(Debug, Serialize, Deserialize)]
struct UserLoginEvent {
    user_id: u32,
    username: String,
}

impl event::Event for UserLoginEvent {
    fn event_type() -> &'static str {
        "user_login"
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PositionUpdateEvent {
    object_id: u32,
    x: f32,
    y: f32,
    z: f32,
}

impl event::Event for PositionUpdateEvent {
    fn event_type() -> &'static str {
        "position_update"
    }
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("🎮 Event Classes Working Example");
    println!("=================================");

    // Create a simple propagator
    let propagator = propagation::UniversalAllEqPropagator::new();
    let event_system: Arc<event::EventBus<event::StructuredEventKey, _>> = 
        Arc::new(event::EventBus::with_propagator(propagator));

    println!("\n📝 Registering handlers for different event classes...");

    // Basic event class
    event_system.on::<UserLoginEvent, _>("core", "user_login", |event| {
        println!("🔐 Basic Class - User login: {}", event.username);
        Ok(())
    }).await?;

    // GORC event class
    event_system.on_gorc_class::<PositionUpdateEvent, _>(
        "gorc", "Player", 0, "position_update", true, |event| {
            println!("🎮 GORC Class - Player {} moved to ({}, {}, {})", 
                event.object_id, event.x, event.y, event.z);
            Ok(())
        }
    ).await?;

    // Custom event class
    event_system.on_custom_evt_class::<UserLoginEvent, _>(
        "core", "user_login", "priority_session", true, |event| {
            println!("⭐ Custom Class - Priority login: {}", event.username);
            Ok(())
        }
    ).await?;

    println!("\n🔥 Emitting events...");

    let login_event = UserLoginEvent {
        user_id: 123,
        username: "alice".to_string(),
    };

    let pos_event = PositionUpdateEvent {
        object_id: 1,
        x: 100.0,
        y: 0.0,
        z: 200.0,
    };

    // Emit basic event
    event_system.emit("core", "user_login", &login_event).await?;

    // Emit GORC event
    event_system.emit_gorc_class("gorc", "Player", 0, "position_update", true, &pos_event).await?;

    // Emit custom event
    event_system.emit_custom_evt_class("core", "user_login", "priority_session", true, &login_event).await?;

    println!("\n✅ All event classes are working!");

    Ok(())
}