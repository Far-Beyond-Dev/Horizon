# Migration Guide

This guide helps you migrate from existing plugin systems to the Universal Plugin System, with specific examples for common architectures.

## From String-Based Event Systems

### Before: String Keys
```rust
// Old system with string-based routing
event_bus.on("player:joined", |event| { ... });
event_bus.on("core:server_started", |event| { ... });
event_bus.emit("player:joined", &event);

// Problems:
// - Runtime string parsing
// - No compile-time validation
// - Typos cause silent failures
// - Performance overhead
```

### After: Structured Keys
```rust
// New system with structured keys
let key = StructuredEventKey::Client {
    namespace: "player".into(),
    event_name: "joined".into(),
};

event_bus.on_key(key.clone(), |event: PlayerJoinedEvent| { ... });
event_bus.emit_key(key, &event);

// Benefits:
// - No string parsing
// - Compile-time type safety
// - Better performance
// - IDE autocomplete
```

### Migration Steps

1. **Create Helper Functions**:
```rust
// Helper to ease migration
pub fn on_client<T, F>(
    event_bus: &Arc<EventBus<StructuredEventKey, AllEqPropagator>>,
    namespace: &str,
    event_name: &str,
    handler: F,
) -> impl Future<Output = Result<(), EventError>>
where
    T: Event + for<'de> Deserialize<'de>,
    F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
{
    let key = StructuredEventKey::Client {
        namespace: namespace.into(),
        event_name: event_name.into(),
    };
    event_bus.on_key(key, handler)
}

// Usage - looks almost identical to old system
on_client(&event_bus, "player", "joined", |event: PlayerJoinedEvent| {
    Ok(())
}).await?;
```

2. **Gradual Replacement**:
```rust
// Phase 1: Replace one namespace at a time
// Old: event_bus.on("player:*", ...)
// New: Use StructuredEventKey::Client with "player" namespace

// Phase 2: Replace event names
// Old: "player:joined", "player:left"  
// New: PlayerJoinedEvent, PlayerLeftEvent with proper types

// Phase 3: Remove helper functions
// Directly use event_bus.on_key() with proper key types
```

## From Horizon Plugin System

### Before: Horizon's System
```rust
// Horizon's current approach
impl HorizonPlugin for MyPlugin {
    fn on_core(&mut self, event: CoreEvent) { ... }
    fn on_client(&mut self, namespace: &str, event: ClientEvent) { ... }
    fn on_plugin(&mut self, plugin: &str, event: PluginEvent) { ... }
    fn on_gorc(&mut self, object_type: &str, channel: u8, event: GorcEvent) { ... }
}

// Events parsed from strings like "gorc:Player:0:position_update"
```

### After: Universal Plugin System
```rust
// Universal system with exact same functionality
#[async_trait::async_trait]
impl SimplePlugin<StructuredEventKey, AllEqPropagator> for MyPlugin {
    async fn register_handlers(
        &mut self,
        event_bus: Arc<EventBus<StructuredEventKey, AllEqPropagator>>,
        _context: Arc<PluginContext<StructuredEventKey, AllEqPropagator>>,
    ) -> Result<(), PluginSystemError> {
        // Core events
        let core_key = StructuredEventKey::Core { 
            event_name: "my_event".into() 
        };
        event_bus.on_key(core_key, |event: MyEvent| { ... }).await?;

        // Client events  
        let client_key = StructuredEventKey::Client { 
            namespace: "my_namespace".into(),
            event_name: "my_event".into() 
        };
        event_bus.on_key(client_key, |event: MyEvent| { ... }).await?;

        // Plugin events
        let plugin_key = StructuredEventKey::Plugin { 
            plugin_name: "other_plugin".into(),
            event_name: "my_event".into() 
        };
        event_bus.on_key(plugin_key, |event: MyEvent| { ... }).await?;

        // GORC events
        let gorc_key = StructuredEventKey::Gorc { 
            object_type: "Player".into(),
            channel: 0,
            event_name: "position_update".into() 
        };
        event_bus.on_key(gorc_key, |event: PositionEvent| { ... }).await?;

        Ok(())
    }
}
```

### Creating Horizon Compatibility Layer
```rust
// Create helpers that mimic Horizon's API exactly
pub struct HorizonCompatPlugin<P> {
    inner: P,
    event_bus: Arc<EventBus<StructuredEventKey, AllEqPropagator>>,
}

impl<P> HorizonCompatPlugin<P> 
where 
    P: HorizonPlugin + Send + Sync + 'static
{
    pub async fn on_core<T>(&self, handler: impl Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static)
    where 
        T: Event + for<'de> Deserialize<'de>
    {
        let key = StructuredEventKey::Core { 
            event_name: T::event_type().into() 
        };
        self.event_bus.on_key(key, handler).await.unwrap();
    }

    pub async fn on_client<T>(
        &self, 
        namespace: &str,
        handler: impl Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static
    )
    where 
        T: Event + for<'de> Deserialize<'de>
    {
        let key = StructuredEventKey::Client { 
            namespace: namespace.into(),
            event_name: T::event_type().into() 
        };
        self.event_bus.on_key(key, handler).await.unwrap();
    }

    pub async fn on_plugin<T>(
        &self, 
        plugin_name: &str,
        handler: impl Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static
    )
    where 
        T: Event + for<'de> Deserialize<'de>
    {
        let key = StructuredEventKey::Plugin { 
            plugin_name: plugin_name.into(),
            event_name: T::event_type().into() 
        };
        self.event_bus.on_key(key, handler).await.unwrap();
    }

    pub async fn on_gorc<T>(
        &self, 
        object_type: &str,
        channel: u8,
        handler: impl Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static
    )
    where 
        T: Event + for<'de> Deserialize<'de>
    {
        let key = StructuredEventKey::Gorc { 
            object_type: object_type.into(),
            channel,
            event_name: T::event_type().into() 
        };
        self.event_bus.on_key(key, handler).await.unwrap();
    }
}

// Now existing Horizon plugins can migrate with minimal changes:
impl HorizonPlugin for MyExistingPlugin {
    async fn register_handlers(&mut self, compat: &HorizonCompatPlugin<Self>) {
        compat.on_core::<ServerStartedEvent>(|event| {
            // Existing handler code unchanged
            Ok(())
        }).await;

        compat.on_client::<ChatMessageEvent>("chat", |event| {
            // Existing handler code unchanged  
            Ok(())
        }).await;
    }
}
```

## From Traditional Pub/Sub Systems

### Before: Basic Pub/Sub
```rust
// Traditional observer pattern
struct EventBus {
    subscribers: HashMap<String, Vec<Box<dyn Fn(&Event)>>>,
}

impl EventBus {
    fn subscribe(&mut self, topic: &str, handler: Box<dyn Fn(&Event)>) {
        self.subscribers.entry(topic.to_string()).or_default().push(handler);
    }
    
    fn publish(&self, topic: &str, event: &Event) {
        if let Some(handlers) = self.subscribers.get(topic) {
            for handler in handlers {
                handler(event);
            }
        }
    }
}
```

### After: Universal Plugin System
```rust
// Type-safe, async, with propagation control
let event_bus = Arc::new(EventBus::with_propagator(AllEqPropagator::new()));

// Type-safe subscription
let key = StructuredEventKey::Custom { 
    fields: vec!["my_topic".into()] 
};

event_bus.on_key(key.clone(), |event: MyTypedEvent| {
    // Async handler with proper error handling
    async move {
        process_event(event).await?;
        Ok(())
    }
}).await?;

// Type-safe publishing
event_bus.emit_key(key, &my_event).await?;
```

### Migration Benefits
```rust
// Before: Runtime errors, no type safety
bus.publish("typo_in_topic_name", &event);  // Silent failure
bus.subscribe("topic", |event| {
    let data = event.as_any().downcast::<WrongType>();  // Runtime panic
});

// After: Compile-time safety
let key = StructuredEventKey::Custom { 
    fields: vec!["topic".into()] 
};
event_bus.emit_key(key.clone(), &event).await?;  // Compile-time type checking
event_bus.on_key(key, |event: CorrectType| {     // Compile-time type checking
    // event is guaranteed to be CorrectType
    Ok(())
}).await?;
```

## From Actor Systems (Actix, Tokio-Actor)

### Before: Actor Model
```rust
// Actor-based approach
#[derive(Message)]
struct MyMessage {
    data: String,
}

impl Handler<MyMessage> for MyActor {
    type Result = ();
    
    fn handle(&mut self, msg: MyMessage, _ctx: &mut Context<Self>) {
        println!("Received: {}", msg.data);
    }
}

// Usage
let addr = MyActor.start();
addr.do_send(MyMessage { data: "hello".to_string() });
```

### After: Plugin System
```rust
// Plugin-based approach with similar benefits
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MyMessage {
    data: String,
}

impl Event for MyMessage {
    fn event_type() -> &'static str { "my_message" }
}

// Handler registration
let key = StructuredEventKey::Plugin {
    plugin_name: "my_plugin".into(),
    event_name: "my_message".into(),
};

event_bus.on_key(key.clone(), |msg: MyMessage| {
    async move {
        println!("Received: {}", msg.data);
        Ok(())
    }
}).await?;

// Usage
event_bus.emit_key(key, &MyMessage { 
    data: "hello".to_string() 
}).await?;
```

### Migration Strategy
1. **Convert Messages to Events**:
```rust
// Before: Actor message
#[derive(Message)]
struct UpdatePlayer { id: u64, position: Vec3 }

// After: Event
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdatePlayerEvent { id: u64, position: Vec3 }
impl Event for UpdatePlayerEvent {
    fn event_type() -> &'static str { "update_player" }
}
```

2. **Convert Actors to Plugins**:
```rust
// Before: Actor
struct PlayerActor { state: PlayerState }
impl Handler<UpdatePlayer> for PlayerActor { ... }

// After: Plugin  
struct PlayerPlugin { state: PlayerState }
impl SimplePlugin<StructuredEventKey, AllEqPropagator> for PlayerPlugin {
    async fn register_handlers(&mut self, event_bus: Arc<...>, _: Arc<...>) {
        let key = StructuredEventKey::Core { event_name: "update_player".into() };
        event_bus.on_key(key, |event: UpdatePlayerEvent| { ... }).await?;
        Ok(())
    }
}
```

## From Microservice Event Systems

### Before: Network-Based Events
```rust
// Network pub/sub (Redis, RabbitMQ, etc.)
redis_client.publish("user:events", serde_json::to_string(&event)?).await?;
redis_client.subscribe("user:events", |msg| {
    let event: UserEvent = serde_json::from_str(&msg)?;
    handle_user_event(event);
}).await?;
```

### After: Unified Local + Network
```rust
// Local events with optional network propagation
let event_bus = Arc::new(EventBus::with_propagator(NetworkAwarePropagator::new()));

// Register handler (works for both local and network events)
let key = StructuredEventKey::Client {
    namespace: "user".into(),
    event_name: "user_event".into(),
};

event_bus.on_key(key.clone(), |event: UserEvent| {
    async move {
        handle_user_event(event).await?;
        Ok(())
    }
}).await?;

// Emit locally or across network
event_bus.emit_key(key, &user_event).await?;

// Or explicitly emit to network
distributed_bus.emit_network(key, &user_event, Some(vec!["node1", "node2"])).await?;
```

## Common Migration Patterns

### 1. Event Definition Migration
```rust
// Before: Loose event definitions
struct Event {
    event_type: String,
    data: serde_json::Value,
}

// After: Strongly typed events
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PlayerJoinedEvent {
    player_id: u64,
    player_name: String,
    timestamp: u64,
}

impl Event for PlayerJoinedEvent {
    fn event_type() -> &'static str { "player_joined" }
}
```

### 2. Handler Migration
```rust
// Before: Generic handlers
fn handle_event(event: &GenericEvent) {
    match event.event_type.as_str() {
        "player_joined" => {
            let data: PlayerData = serde_json::from_value(event.data.clone())?;
            // Handle player joined
        }
        _ => {}
    }
}

// After: Type-safe handlers
event_bus.on_key(player_joined_key, |event: PlayerJoinedEvent| {
    async move {
        // event is guaranteed to be PlayerJoinedEvent
        // No type casting or matching needed
        println!("Player {} joined", event.player_name);
        Ok(())
    }
}).await?;
```

### 3. Error Handling Migration
```rust
// Before: Manual error handling
fn handle_event(event: &Event) -> Result<(), Box<dyn Error>> {
    // Manual deserialization with error handling
    match serde_json::from_value::<PlayerEvent>(event.data.clone()) {
        Ok(player_event) => process_player_event(player_event),
        Err(e) => {
            eprintln!("Failed to deserialize event: {}", e);
            Err(e.into())
        }
    }
}

// After: Automatic error handling
event_bus.on_key(key, |event: PlayerEvent| {
    async move {
        // Deserialization is automatic and type-safe
        // Focus on business logic, not plumbing
        process_player_event(event).await?;
        Ok(())
    }
}).await?;
```

## Migration Checklist

### Phase 1: Setup
- [ ] Add Universal Plugin System dependency
- [ ] Create basic event bus with AllEqPropagator
- [ ] Set up plugin manager and context
- [ ] Create first plugin using SimplePlugin trait

### Phase 2: Event Migration
- [ ] Identify all event types in existing system
- [ ] Create strongly-typed event structs
- [ ] Implement Event trait for each type
- [ ] Define StructuredEventKey variants for each event category

### Phase 3: Handler Migration
- [ ] Convert existing handlers to async functions
- [ ] Register handlers using on_key instead of string-based registration
- [ ] Update error handling to use Result<(), EventError>
- [ ] Test that all handlers receive correct events

### Phase 4: Emission Migration
- [ ] Replace string-based emit calls with emit_key
- [ ] Ensure all events use proper structured keys
- [ ] Verify event routing works correctly
- [ ] Add any necessary metadata to events

### Phase 5: Advanced Features
- [ ] Implement custom propagators if needed
- [ ] Add monitoring and metrics
- [ ] Set up hot-reloading if desired
- [ ] Optimize performance if necessary

### Phase 6: Cleanup
- [ ] Remove old event system code
- [ ] Remove compatibility helpers
- [ ] Update documentation
- [ ] Train team on new system

## Testing Migration

### Parallel Running
Run both systems in parallel during migration:

```rust
// Migration bridge
struct MigrationBridge {
    old_bus: OldEventBus,
    new_bus: Arc<EventBus<StructuredEventKey, AllEqPropagator>>,
}

impl MigrationBridge {
    async fn emit_to_both<T>(&self, key: StructuredEventKey, event: &T) 
    where 
        T: Event + Serialize + Clone
    {
        // Emit to new system
        self.new_bus.emit_key(key.clone(), event).await.ok();
        
        // Emit to old system  
        let old_key = self.convert_key_to_string(&key);
        self.old_bus.emit(&old_key, event).ok();
    }
    
    fn convert_key_to_string(&self, key: &StructuredEventKey) -> String {
        key.to_string()
    }
}
```

### Verification
Compare outputs between systems:

```rust
// Verification handler
struct VerificationHandler {
    old_results: Arc<Mutex<Vec<EventResult>>>,
    new_results: Arc<Mutex<Vec<EventResult>>>,
}

impl VerificationHandler {
    async fn verify_equivalence(&self) -> bool {
        let old = self.old_results.lock().unwrap();
        let new = self.new_results.lock().unwrap();
        
        // Compare results
        old.len() == new.len() && 
        old.iter().zip(new.iter()).all(|(a, b)| a.equivalent(b))
    }
}
```

This migration guide provides a clear path from any existing event system to the Universal Plugin System while maintaining functionality and improving type safety, performance, and maintainability.