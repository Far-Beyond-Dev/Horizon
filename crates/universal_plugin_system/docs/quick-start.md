# Quick Start Guide

Get up and running with the Universal Plugin System in just a few minutes.

## Installation

Add the universal plugin system to your `Cargo.toml`:

```toml
[dependencies]
universal_plugin_system = { path = "../universal_plugin_system" }
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
```

## Basic Setup

### 1. Define Your Events

Events are the core of the system. Define what happens in your application:

```rust
use serde::{Deserialize, Serialize};
use universal_plugin_system::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerJoinedEvent {
    pub player_id: u64,
    pub name: String,
    pub timestamp: u64,
}

impl Event for PlayerJoinedEvent {
    fn event_type() -> &'static str {
        "player_joined"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageEvent {
    pub player_id: u64,
    pub channel: String,
    pub message: String,
}

impl Event for ChatMessageEvent {
    fn event_type() -> &'static str {
        "chat_message"
    }
}
```

### 2. Create an Event Bus

The event bus handles all event routing and propagation:

```rust
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create event bus with AllEq propagation (most common)
    let event_bus = Arc::new(EventBus::with_propagator(AllEqPropagator::new()));
    
    // Your application logic here...
    
    Ok(())
}
```

### 3. Register Event Handlers

Create handlers that respond to specific events:

```rust
// Handler for player join events
let player_key = StructuredEventKey::Client { 
    namespace: "game".into(), 
    event_name: "player_joined".into() 
};

event_bus.on_key(player_key, |event: PlayerJoinedEvent| {
    println!("🎮 Player {} (ID: {}) joined the game!", event.name, event.player_id);
    
    // Add your game logic here
    // - Update player count
    // - Send welcome message
    // - Log to database
    
    Ok(())
}).await?;

// Handler for chat messages
let chat_key = StructuredEventKey::Client { 
    namespace: "chat".into(), 
    event_name: "message".into() 
};

event_bus.on_key(chat_key, |event: ChatMessageEvent| {
    println!("💬 [{}] Player {}: {}", event.channel, event.player_id, event.message);
    
    // Add chat logic here
    // - Broadcast to other players
    // - Apply filters/moderation
    // - Log messages
    
    Ok(())
}).await?;
```

### 4. Emit Events

Trigger events from anywhere in your application:

```rust
// Player joins
let join_event = PlayerJoinedEvent {
    player_id: 12345,
    name: "Alice".to_string(),
    timestamp: std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs(),
};

let join_key = StructuredEventKey::Client { 
    namespace: "game".into(), 
    event_name: "player_joined".into() 
};

event_bus.emit_key(join_key, &join_event).await?;

// Chat message
let chat_event = ChatMessageEvent {
    player_id: 12345,
    channel: "general".to_string(),
    message: "Hello everyone!".to_string(),
};

let chat_key = StructuredEventKey::Client { 
    namespace: "chat".into(), 
    event_name: "message".into() 
};

event_bus.emit_key(chat_key, &chat_event).await?;
```

## Complete Example

Here's a complete working example:

```rust
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use universal_plugin_system::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerJoinedEvent {
    pub player_id: u64,
    pub name: String,
}

impl Event for PlayerJoinedEvent {
    fn event_type() -> &'static str { "player_joined" }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting Universal Plugin System Demo");
    
    // Create event bus
    let event_bus = Arc::new(EventBus::with_propagator(AllEqPropagator::new()));
    
    // Register handler
    let key = StructuredEventKey::Client { 
        namespace: "game".into(), 
        event_name: "player_joined".into() 
    };
    
    event_bus.on_key(key.clone(), |event: PlayerJoinedEvent| {
        println!("🎮 Welcome {}! (Player ID: {})", event.name, event.player_id);
        Ok(())
    }).await?;
    
    // Emit event
    let event = PlayerJoinedEvent {
        player_id: 123,
        name: "Alice".to_string(),
    };
    
    event_bus.emit_key(key, &event).await?;
    
    // Check stats
    let stats = event_bus.stats().await;
    println!("📊 Events processed: {}", stats.events_handled);
    
    Ok(())
}
```

## Event Key Types

The system supports several types of structured event keys:

### Core Events
```rust
let key = StructuredEventKey::Core { 
    event_name: "server_started".into() 
};
```

### Client Events  
```rust
let key = StructuredEventKey::Client { 
    namespace: "chat".into(),
    event_name: "message".into() 
};
```

### Plugin Events
```rust
let key = StructuredEventKey::Plugin { 
    plugin_name: "economy".into(),
    event_name: "transaction".into() 
};
```

### GORC Events (for spatial systems)
```rust
let key = StructuredEventKey::Gorc { 
    object_type: "Player".into(),
    channel: 0,
    event_name: "position_update".into() 
};
```

### Custom Events
```rust
let key = StructuredEventKey::Custom { 
    fields: vec!["my_app".into(), "custom_event".into()] 
};
```

## AllEq Propagation

The `AllEqPropagator` ensures that events only reach handlers with exactly matching event keys:

```rust
// This handler will ONLY receive events with the exact same key
let key1 = StructuredEventKey::Client { 
    namespace: "chat".into(), 
    event_name: "message".into() 
};

// This is a DIFFERENT key - handlers won't interact
let key2 = StructuredEventKey::Client { 
    namespace: "game".into(),  // Different namespace
    event_name: "message".into() 
};
```

## Next Steps

Now that you have the basics working:

1. **[Learn the Architecture](architecture.md)** - Understand how the system works
2. **[Create Plugins](plugin-development.md)** - Build modular functionality  
3. **[Custom Propagation](event-propagation.md)** - Implement spatial/network filtering
4. **[Advanced Patterns](advanced-usage.md)** - Complex event routing scenarios

## Common Patterns

### On/Emit Helpers

You can create helper functions to simplify your API:

```rust
// Helper for client events
pub async fn on_client<T, F>(
    event_bus: &Arc<EventBus<StructuredEventKey, AllEqPropagator>>,
    namespace: &str,
    event_name: &str,
    handler: F,
) -> Result<(), EventError>
where
    T: Event + for<'de> Deserialize<'de>,
    F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
{
    let key = StructuredEventKey::Client {
        namespace: namespace.into(),
        event_name: event_name.into(),
    };
    event_bus.on_key(key, handler).await
}

pub async fn emit_client<T>(
    event_bus: &Arc<EventBus<StructuredEventKey, AllEqPropagator>>,
    namespace: &str,
    event_name: &str,
    event: &T,
) -> Result<(), EventError>
where
    T: Event + Serialize,
{
    let key = StructuredEventKey::Client {
        namespace: namespace.into(),
        event_name: event_name.into(),
    };
    event_bus.emit_key(key, event).await
}

// Usage
on_client(&event_bus, "chat", "message", |event: ChatMessageEvent| {
    println!("Chat: {}", event.message);
    Ok(())
}).await?;

emit_client(&event_bus, "chat", "message", &chat_event).await?;
```

This gives you the same clean API as `on_client()` and `emit_client()` while maintaining full flexibility!