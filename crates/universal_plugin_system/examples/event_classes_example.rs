//! Example demonstrating different event classes with custom metadata and class-aware propagators
//!
//! This example shows how host applications can define their own event classes
//! with different metadata parameters and use class-specific propagation logic.
//!
//! Note: Application-specific propagators are implemented in the `common::application_propagators` 
//! module to demonstrate how host apps should implement custom propagators for their 
//! specific event classes outside of the universal system.

use universal_plugin_system::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio;

mod common;

// Sample events for different classes
#[derive(Debug, Serialize, Deserialize)]
struct UserLoginEvent {
    user_id: u32,
    username: String,
    timestamp: u64,
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

#[derive(Debug, Serialize, Deserialize)]
struct CombatEvent {
    attacker_id: u32,
    target_id: u32,
    damage: u32,
    location: String,
}

impl event::Event for CombatEvent {
    fn event_type() -> &'static str {
        "combat_event"
    }
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🚀 Universal Plugin System - Event Classes Example");
    println!("====================================================");

    // Create a class-aware propagator system using application-specific propagators
    let gorc_spatial_propagator = common::application_propagators::GorcSpatialPropagator::new(100.0);
    let priority_propagator = common::application_propagators::PriorityPropagator::new(128); // Medium priority and above
    let region_propagator = common::application_propagators::RegionPropagator::new(vec!["region_1", "region_2"]);

    // Create a composite propagator for extended events (priority AND region filtering)
    let extended_composite = propagation::CompositePropagator::new_and()
        .add_propagator(Box::new(priority_propagator))
        .add_propagator(Box::new(region_propagator));

    // Create the class-aware propagator
    let class_aware_propagator = common::application_propagators::ClassAwarePropagator::new()
        .with_gorc_propagator(Box::new(gorc_spatial_propagator))
        .with_extended_propagator(Box::new(extended_composite))
        .with_custom_propagator(Box::new(propagation::UniversalAllEqPropagator::new()));

    // Create event bus with class-aware propagation
    let event_system: Arc<event::EventBus<event::StructuredEventKey, _>> = 
        Arc::new(event::EventBus::with_propagator(class_aware_propagator));

    println!("\n📝 Registering handlers for different event classes...");

    // 1. Basic event class (2 segments): [domain, event_name]
    event_system.on::<UserLoginEvent, _>("core", "user_login", |event| {
        println!("🔐 Basic Class - User login: {} (ID: {})", event.username, event.user_id);
        Ok(())
    }).await?;

    // 2. GORC event class (5 segments): [domain, object_type, channel, event_name, spatial_aware]
    event_system.on_gorc_class::<PositionUpdateEvent, _>(
        "gorc", "Player", 0, "position_update", true, |event| {
            println!("🎮 GORC Class (Spatial) - Player {} moved to ({}, {}, {})", 
                event.object_id, event.x, event.y, event.z);
            Ok(())
        }
    ).await?;

    event_system.on_gorc_class::<PositionUpdateEvent, _>(
        "gorc", "NPC", 0, "position_update", false, |event| {
            println!("🤖 GORC Class (Non-Spatial) - NPC {} moved to ({}, {}, {})", 
                event.object_id, event.x, event.y, event.z);
            Ok(())
        }
    ).await?;

    // 3. Custom event class (4 segments): [domain, event_name, metadata, flag]
    event_system.on_custom_evt_class::<UserLoginEvent, _>(
        "core", "user_login", "priority_session", true, |event| {
            println!("⭐ Custom Class - Priority login: {} (ID: {})", event.username, event.user_id);
            Ok(())
        }
    ).await?;

    // 4. Extended event class (6 segments): [domain, category, event_name, priority, region, persistent]
    event_system.on_extended_class::<CombatEvent, _>(
        "game", "combat", "damage_dealt", 255, "region_1", true, |event| {
            println!("⚔️  Extended Class - Critical combat: {} damage from {} to {} in {}", 
                event.damage, event.attacker_id, event.target_id, event.location);
            Ok(())
        }
    ).await?;

    event_system.on_extended_class::<CombatEvent, _>(
        "game", "combat", "damage_dealt", 50, "region_2", false, |event| {
            println!("🗡️  Extended Class - Minor combat: {} damage from {} to {} in {}", 
                event.damage, event.attacker_id, event.target_id, event.location);
            Ok(())
        }
    ).await?;

    // Handler that won't receive events due to region filtering
    event_system.on_extended_class::<CombatEvent, _>(
        "game", "combat", "damage_dealt", 200, "region_3", true, |_event| {
            println!("❌ This should not be called (wrong region)");
            Ok(())
        }
    ).await?;

    // Handler that won't receive events due to priority filtering
    event_system.on_extended_class::<CombatEvent, _>(
        "game", "combat", "damage_dealt", 10, "region_1", true, |_event| {
            println!("❌ This should not be called (too low priority)");
            Ok(())
        }
    ).await?;

    println!("\n🔥 Emitting events to demonstrate class-specific propagation...");

    // Test basic events
    println!("\n--- Basic Events ---");
    let login_event = UserLoginEvent {
        user_id: 12345,
        username: "alice".to_string(),
        timestamp: 1234567890,
    };

    event_system.emit("core", "user_login", &login_event).await?;

    // Test GORC events with spatial awareness
    println!("\n--- GORC Events ---");
    let pos_event = PositionUpdateEvent {
        object_id: 1,
        x: 100.0,
        y: 0.0,
        z: 200.0,
    };

    // This will use spatial propagation
    event_system.emit_gorc_class("gorc", "Player", 0, "position_update", true, &pos_event).await?;
    
    // This will not use spatial propagation
    event_system.emit_gorc_class("gorc", "NPC", 0, "position_update", false, &pos_event).await?;

    // Test custom events
    println!("\n--- Custom Events ---");
    event_system.emit_custom_evt_class("core", "user_login", "priority_session", true, &login_event).await?;

    // Test extended events with different priorities and regions
    println!("\n--- Extended Events ---");
    let combat_event = CombatEvent {
        attacker_id: 100,
        target_id: 200,
        damage: 50,
        location: "battlefield".to_string(),
    };

    // High priority in allowed region - should reach handlers
    event_system.emit_extended_class("game", "combat", "damage_dealt", 255, "region_1", true, &combat_event).await?;

    // Low priority in allowed region - should reach some handlers
    event_system.emit_extended_class("game", "combat", "damage_dealt", 50, "region_2", false, &combat_event).await?;

    // High priority in blocked region - should not reach region-filtered handlers
    event_system.emit_extended_class("game", "combat", "damage_dealt", 200, "region_3", true, &combat_event).await?;

    // Low priority in allowed region - should not reach priority-filtered handlers
    event_system.emit_extended_class("game", "combat", "damage_dealt", 10, "region_1", true, &combat_event).await?;

    // Get and display statistics
    println!("\n📊 Event System Statistics:");
    let stats = event_system.stats().await;
    println!("  Events emitted: {}", stats.events_emitted);
    println!("  Events handled: {}", stats.events_handled);
    println!("  Handler failures: {}", stats.handler_failures);
    println!("  Total handlers: {}", stats.total_handlers);

    println!("\n✅ Event classes example completed successfully!");

    Ok(())
}