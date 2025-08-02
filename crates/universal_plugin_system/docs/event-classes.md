# Event Classes Guide

The universal plugin system supports different **event classes** - structured patterns for organizing events with custom metadata parameters. This allows host applications to define their own domain-specific event structures while maintaining type safety and enabling class-specific propagation logic.

## Overview

An event class defines:
- **Structure**: The number and meaning of metadata segments in the event key
- **Semantics**: What each segment represents (domain, priority, flags, etc.)
- **Propagation**: Custom logic for how events of this class should be distributed

The system provides several pre-defined event classes and allows applications to define their own.

## Pre-defined Event Classes

### 1. Basic Event Class
**Structure**: `[domain, event_name]`
**Use case**: Simple domain-based events

```rust
// Registration
event_system.on("core", "user_login", |event: UserLoginEvent| {
    println!("User login: {}", event.username);
    Ok(())
}).await?;

// Emission
event_system.emit("core", "user_login", &login_event).await?;
```

### 2. Categorized Event Class
**Structure**: `[domain, category, event_name]`
**Use case**: Events with sub-categories

```rust
// Registration
event_system.on_category("client", "chat", "message", |event: ChatEvent| {
    println!("Chat message: {}", event.content);
    Ok(())
}).await?;

// Emission
event_system.emit_category("client", "chat", "message", &chat_event).await?;
```

### 3. GORC Event Class
**Structure**: `[domain, object_type, channel, event_name, spatial_aware]`
**Use case**: Game object replication with spatial awareness

```rust
// Registration
event_system.on_gorc_class("gorc", "Player", 0, "position_update", true, |event: PosEvent| {
    println!("Player moved spatially");
    Ok(())
}).await?;

// Emission
event_system.emit_gorc_class("gorc", "Player", 0, "position_update", true, &pos_event).await?;
```

### 4. Custom Event Class
**Structure**: `[domain, event_name, metadata, flag]`
**Use case**: Application-specific patterns

```rust
// Registration
event_system.on_custom_evt_class("core", "user_login", "session_id", true, |event: LoginEvent| {
    println!("Priority login from session");
    Ok(())
}).await?;

// Emission
event_system.emit_custom_evt_class("core", "user_login", "session_123", true, &login_event).await?;
```

### 5. Extended Event Class
**Structure**: `[domain, category, event_name, priority, region, persistent]`
**Use case**: Complex events with multiple metadata fields

```rust
// Registration
event_system.on_extended_class("game", "combat", "damage", 255, "region_1", true, |event: DamageEvent| {
    println!("Critical damage in region 1");
    Ok(())
}).await?;

// Emission
event_system.emit_extended_class("game", "combat", "damage", 255, "region_1", true, &damage_event).await?;
```

## Class-Aware Propagation

Different event classes can use specialized propagation logic:

### GORC Spatial Propagator
Only processes GORC class events with `spatial_aware=true`:

```rust
let gorc_propagator = GorcSpatialPropagator::new(100.0); // 100 unit range
gorc_propagator.update_player_position("player1", 50.0, 0.0, 50.0).await;

// Only spatially-aware GORC events use distance-based filtering
```

### Priority Propagator
Filters extended class events by priority threshold:

```rust
let priority_propagator = PriorityPropagator::new(128); // Medium priority and above

// Only events with priority >= 128 will be propagated
```

### Region Propagator
Filters extended class events by allowed regions:

```rust
let region_propagator = RegionPropagator::new(vec!["region_1", "region_2"]);

// Only events from allowed regions will be propagated
```

### Class-Aware Composite
Routes events to different propagators based on their class:

```rust
let class_aware = ClassAwarePropagator::new()
    .with_gorc_propagator(Box::new(gorc_spatial_propagator))
    .with_extended_propagator(Box::new(priority_propagator))
    .with_custom_propagator(Box::new(AllEqPropagator::new()));

let event_system = EventBus::with_propagator(class_aware);
```

## Performance Considerations

### Zero-Allocation Runtime
Event keys are constructed once during registration/emission and cached. No allocation occurs during hot-path propagation decisions.

### Class-Specific Optimization
Each event class can have optimized propagation logic:
- GORC events use spatial indexing
- Priority events use fast integer comparison
- Custom events use exact matching

### Compile-Time Safety
All event classes maintain compile-time type safety. Invalid class structures are caught at build time.

## Creating Custom Event Classes

You can extend the system with your own event class methods:

```rust
impl<P: EventPropagator<StructuredEventKey>> EventBus<StructuredEventKey, P> {
    /// Custom event class for your application
    pub async fn on_my_custom_class<T, F>(
        &self,
        domain: &str,
        namespace: &str,
        priority: u16,
        encrypted: bool,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let key = StructuredEventKey {
            segments: vec![
                domain.into(),
                namespace.into(),
                priority.to_string().into(),
                encrypted.to_string().into(),
            ],
        };
        self.on_key(key, handler).await
    }
}
```

Then create a matching propagator:

```rust
pub struct MyCustomPropagator {
    min_priority: u16,
    allow_encrypted: bool,
}

impl MyCustomPropagator {
    fn is_my_custom_event(&self, key: &StructuredEventKey) -> bool {
        key.segments.len() == 4 && 
        key.segments[2].parse::<u16>().is_ok() &&
        (key.segments[3] == "true" || key.segments[3] == "false")
    }
    
    fn extract_priority(&self, key: &StructuredEventKey) -> Option<u16> {
        key.segments.get(2)?.parse().ok()
    }
    
    fn is_encrypted(&self, key: &StructuredEventKey) -> bool {
        key.segments.get(3).map(|s| s == "true").unwrap_or(false)
    }
}

#[async_trait]
impl EventPropagator<StructuredEventKey> for MyCustomPropagator {
    async fn should_propagate(&self, key: &StructuredEventKey, _context: &PropagationContext<StructuredEventKey>) -> bool {
        if !self.is_my_custom_event(key) {
            return true; // Pass through non-custom events
        }
        
        // Apply custom logic
        let priority_ok = self.extract_priority(key)
            .map(|p| p >= self.min_priority)
            .unwrap_or(false);
            
        let encryption_ok = !self.is_encrypted(key) || self.allow_encrypted;
        
        priority_ok && encryption_ok
    }
}
```

## Best Practices

1. **Choose the Right Class**: Use the simplest class that meets your needs
2. **Consistent Structure**: Keep the same segment count and meanings within a class
3. **Meaningful Names**: Use descriptive segment values for debugging
4. **Propagator Efficiency**: Design propagators to fail fast for non-matching events
5. **Documentation**: Document your custom event classes and their semantics

## Migration from Simple Events

If you have existing simple events, they automatically work as Basic or Categorized event classes:

```rust
// Old style (still works)
event_system.on_key(StructuredEventKey::domain_event("core", "login"), handler).await?;

// New style (equivalent)
event_system.on("core", "login", handler).await?;
```

The event class system is fully backward compatible while providing enhanced functionality for applications that need it.