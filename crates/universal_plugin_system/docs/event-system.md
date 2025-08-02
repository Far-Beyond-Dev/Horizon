# Event System

The event system is the heart of the Universal Plugin System, providing type-safe, high-performance event routing with configurable propagation logic.

## Core Concepts

### Events
Events represent things that happen in your application. They must implement the `Event` trait:

```rust
use serde::{Deserialize, Serialize};
use universal_plugin_system::Event;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerMovedEvent {
    pub player_id: u64,
    pub old_position: (f32, f32, f32),
    pub new_position: (f32, f32, f32),
    pub timestamp: u64,
}

impl Event for PlayerMovedEvent {
    fn event_type() -> &'static str {
        "player_moved"
    }
}
```

### Event Keys
Event keys determine how events are routed. The system provides structured keys that eliminate string parsing:

```rust
use universal_plugin_system::StructuredEventKey;

// Core server events
let core_key = StructuredEventKey::Core { 
    event_name: "server_started".into() 
};

// Client events with namespace
let client_key = StructuredEventKey::Client { 
    namespace: "game".into(),
    event_name: "player_moved".into() 
};

// Plugin events
let plugin_key = StructuredEventKey::Plugin { 
    plugin_name: "physics".into(),
    event_name: "collision_detected".into() 
};

// GORC spatial events
let gorc_key = StructuredEventKey::Gorc { 
    object_type: "Player".into(),
    channel: 0,
    event_name: "position_update".into() 
};
```

### Event Handlers
Handlers respond to events. They're type-safe and async:

```rust
use universal_plugin_system::{EventBus, AllEqPropagator};
use std::sync::Arc;

let event_bus = Arc::new(EventBus::with_propagator(AllEqPropagator::new()));

// Register a typed handler
let key = StructuredEventKey::Client { 
    namespace: "game".into(),
    event_name: "player_moved".into() 
};

event_bus.on_key(key, |event: PlayerMovedEvent| {
    println!("Player {} moved from {:?} to {:?}", 
        event.player_id, 
        event.old_position, 
        event.new_position
    );
    
    // Your game logic here
    // - Update spatial index
    // - Check for collisions
    // - Broadcast to nearby players
    
    Ok(())
}).await?;
```

## Event Lifecycle

### 1. Event Creation
Events are created and emitted by your application:

```rust
let move_event = PlayerMovedEvent {
    player_id: 12345,
    old_position: (10.0, 5.0, 20.0),
    new_position: (11.0, 5.0, 21.0),
    timestamp: current_timestamp(),
};

let key = StructuredEventKey::Client { 
    namespace: "game".into(),
    event_name: "player_moved".into() 
};

event_bus.emit_key(key, &move_event).await?;
```

### 2. Serialization
Events are serialized once and shared:

```rust
// Internally, the event is serialized to bytes
let event_data = EventData::new(&move_event)?;

// EventData contains:
// - data: Arc<Vec<u8>>     (shared, not copied)
// - type_name: String      (for type checking)
// - metadata: HashMap      (additional context)
```

### 3. Handler Lookup
The event bus finds all handlers registered for that key:

```rust
// O(1) lookup by event key
let handlers = self.handlers.get(&key)?;
```

### 4. Propagation Check
Each handler is checked by the propagator:

```rust
for handler in handlers {
    if propagator.should_propagate(&key, &context).await {
        // Handler will receive the event
    } else {
        // Handler is filtered out
    }
}
```

### 5. Event Transformation
Propagators can modify events before delivery:

```rust
let final_event = propagator
    .transform_event(event_data, &context)
    .await
    .unwrap_or(event_data);
```

### 6. Handler Execution
All allowed handlers execute concurrently:

```rust
let mut futures = FuturesUnordered::new();

for handler in allowed_handlers {
    futures.push(async move {
        handler.handle(&final_event).await
    });
}

// Collect results as they complete
while let Some(result) = futures.next().await {
    // Handle success/failure
}
```

## Event Key Types

### Core Events
System-level events that don't belong to any specific namespace:

```rust
let server_start = StructuredEventKey::Core { 
    event_name: "server_started".into() 
};

let shutdown = StructuredEventKey::Core { 
    event_name: "shutdown_requested".into() 
};
```

**Use for**: Server lifecycle, system configuration, global state changes

### Client Events
Events related to client connections and user actions:

```rust
let player_join = StructuredEventKey::Client { 
    namespace: "auth".into(),
    event_name: "player_joined".into() 
};

let chat_message = StructuredEventKey::Client { 
    namespace: "chat".into(),
    event_name: "message_sent".into() 
};

let inventory_update = StructuredEventKey::Client { 
    namespace: "inventory".into(),
    event_name: "item_added".into() 
};
```

**Use for**: User actions, client state changes, UI events

### Plugin Events
Communication between plugins:

```rust
let economy_transaction = StructuredEventKey::Plugin { 
    plugin_name: "economy".into(),
    event_name: "transaction_completed".into() 
};

let quest_progress = StructuredEventKey::Plugin { 
    plugin_name: "quest_system".into(),
    event_name: "objective_completed".into() 
};
```

**Use for**: Inter-plugin communication, plugin-specific events

### GORC Events
Spatial events for game objects (like Horizon's GORC system):

```rust
// Regular GORC events (broadcast to all)
let position_update = StructuredEventKey::Gorc { 
    object_type: "Player".into(),
    channel: 0,  // Replication channel
    event_name: "position_changed".into() 
};

// Instance-specific events (targeted)
let player_action = StructuredEventKey::GorcInstance { 
    object_type: "Player".into(),
    channel: 1,
    event_name: "cast_spell".into() 
};
```

**Use for**: Game object events, spatial updates, replication

### Custom Events
Flexible events for specialized use cases:

```rust
let custom = StructuredEventKey::Custom { 
    fields: vec![
        "metrics".into(),
        "performance".into(), 
        "cpu_usage".into()
    ] 
};

// Equivalent to "metrics:performance:cpu_usage"
```

**Use for**: Domain-specific events, complex hierarchies

## Event Propagation

### AllEq Propagator (Default)
Only delivers events to handlers with exactly matching keys:

```rust
let propagator = AllEqPropagator::new();
let event_bus = EventBus::with_propagator(propagator);

// Handler A
let key_a = StructuredEventKey::Client { 
    namespace: "chat".into(), 
    event_name: "message".into() 
};

// Handler B  
let key_b = StructuredEventKey::Client { 
    namespace: "game".into(),  // Different namespace!
    event_name: "message".into() 
};

// Emitting to key_a will NOT trigger handler B
// Only exact matches trigger handlers
```

### Spatial Propagator
Filters events based on distance (perfect for games):

```rust
let spatial = SpatialPropagator::new(100.0); // 100 unit radius
let event_bus = EventBus::with_propagator(spatial);

// Events include spatial metadata
let context = PropagationContext::new(key)
    .with_metadata("source_x", "10.0")
    .with_metadata("source_y", "20.0") 
    .with_metadata("source_z", "30.0")
    .with_metadata("target_player", "player_123");

// Only handlers within 100 units receive the event
```

### Namespace Propagator
Filters by event namespace:

```rust
let namespace_filter = NamespacePropagator::new()
    .allow_namespaces(vec![
        EventNamespace::Core,
        EventNamespace::Client,
    ])
    .block_namespaces(vec![
        EventNamespace::Plugin, // Block plugin events
    ]);

let event_bus = EventBus::with_propagator(namespace_filter);
```

### Composite Propagator
Combine multiple propagators:

```rust
// AND logic: ALL propagators must allow
let composite = CompositePropagator::new_and()
    .add_propagator(Box::new(AllEqPropagator::new()))
    .add_propagator(Box::new(NamespacePropagator::new()));

// OR logic: ANY propagator can allow  
let composite = CompositePropagator::new_or()
    .add_propagator(Box::new(spatial_prop))
    .add_propagator(Box::new(admin_prop));
```

## Advanced Event Handling

### Event Metadata
Add context information to events:

```rust
let event_data = EventData::new(&my_event)?
    .with_metadata("priority", "high")
    .with_metadata("source", "web_api")
    .with_metadata("user_id", "12345");
```

### Type-Safe Deserialization
Events are automatically type-checked:

```rust
// This will compile-time error if types don't match
event_bus.on_key(key, |event: WrongEventType| {
    // Compiler error: expected PlayerMovedEvent
    Ok(())
}).await?;
```

### Error Handling
Handlers can return errors without stopping other handlers:

```rust
event_bus.on_key(key, |event: MyEvent| {
    if event.is_invalid() {
        return Err(EventError::InvalidData("Bad event data".into()));
    }
    
    // Process event
    Ok(())
}).await?;

// Failed handlers are logged, other handlers continue
```

### Handler Statistics
Monitor event system performance:

```rust
let stats = event_bus.stats().await;
println!("Events emitted: {}", stats.events_emitted);
println!("Events handled: {}", stats.events_handled);
println!("Handler failures: {}", stats.handler_failures);
println!("Total handlers: {}", stats.total_handlers);
```

## Performance Optimization

### Event Key Design
Use the right key type for your use case:

```rust
// Good: Specific, structured
StructuredEventKey::Client { 
    namespace: "inventory".into(),
    event_name: "item_added".into() 
}

// Avoid: Too generic  
StructuredEventKey::Core { 
    event_name: "generic_event".into() 
}

// Good: Efficient spatial routing
StructuredEventKey::Gorc { 
    object_type: "Player".into(),
    channel: 0,
    event_name: "position_update".into() 
}
```

### Handler Registration
Register handlers early and avoid frequent changes:

```rust
// Good: Register once at startup
async fn setup_handlers(event_bus: &Arc<EventBus<_, _>>) {
    event_bus.on_key(key1, handler1).await?;
    event_bus.on_key(key2, handler2).await?;
    // ...
}

// Avoid: Frequent registration/unregistration
// (though the system supports it)
```

### Event Emission
Emit events efficiently:

```rust
// Good: Reuse event keys
static CHAT_KEY: Lazy<StructuredEventKey> = Lazy::new(|| {
    StructuredEventKey::Client { 
        namespace: "chat".into(),
        event_name: "message".into() 
    }
});

event_bus.emit_key(CHAT_KEY.clone(), &event).await?;

// Avoid: Creating keys every time (still works, just less efficient)
let key = StructuredEventKey::Client { /* ... */ };
event_bus.emit_key(key, &event).await?;
```

### Batch Processing
For high-frequency events, consider batching:

```rust
// Collect events
let mut batch = Vec::new();
batch.push(event1);
batch.push(event2);
batch.push(event3);

// Emit batch
let batch_event = EventBatch { events: batch };
event_bus.emit_key(batch_key, &batch_event).await?;
```

## Common Patterns

### Request-Response
Implement request-response patterns:

```rust
// Request event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPlayerInfoRequest {
    pub player_id: u64,
    pub request_id: String,
}

// Response event  
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPlayerInfoResponse {
    pub request_id: String,
    pub player_info: Option<PlayerInfo>,
    pub error: Option<String>,
}

// Handler
event_bus.on_key(request_key, |req: GetPlayerInfoRequest| {
    let response = GetPlayerInfoResponse {
        request_id: req.request_id,
        player_info: get_player_info(req.player_id),
        error: None,
    };
    
    emit_response(response).await?;
    Ok(())
}).await?;
```

### Event Chaining
Chain events together:

```rust
// Primary event
event_bus.on_key(player_died_key, |event: PlayerDiedEvent| {
    // Trigger secondary events
    emit_update_score(event.player_id).await?;
    emit_drop_items(event.player_id, event.position).await?;
    emit_notify_party(event.player_id).await?;
    
    Ok(())
}).await?;
```

### Conditional Handlers
Use metadata for conditional processing:

```rust
event_bus.on_key(key, |event: MyEvent| {
    // Check event metadata
    if let Some(priority) = event.metadata.get("priority") {
        if priority == "low" {
            // Skip low priority events during high load
            return Ok(());
        }
    }
    
    // Process high/normal priority events
    process_event(event).await?;
    Ok(())
}).await?;
```

This event system provides the foundation for building sophisticated, type-safe event-driven applications with maximum flexibility and performance.