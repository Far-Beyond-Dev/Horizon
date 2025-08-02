# Event Propagation

Event propagation is what makes the Universal Plugin System truly flexible. Instead of simple broadcast messaging, you can implement sophisticated routing logic that determines exactly which handlers receive which events.

## Understanding Propagation

### The Problem
In traditional event systems, events are either:
- **Broadcast to everyone** (inefficient, noisy)
- **Point-to-point** (inflexible, tightly coupled)

### The Solution
Event propagators act as intelligent filters that decide:
- Which handlers should receive each event
- How events should be transformed before delivery
- When to apply special routing logic

```rust
#[async_trait]
pub trait EventPropagator<K: EventKeyType>: Send + Sync + 'static {
    // Core decision: should this handler get this event?
    async fn should_propagate(&self, event_key: &K, context: &PropagationContext<K>) -> bool;
    
    // Optional: modify the event before delivery
    async fn transform_event(&self, event: Arc<EventData>, context: &PropagationContext<K>) -> Option<Arc<EventData>>;
    
    // Lifecycle hooks
    async fn on_propagation_start(&self, event_key: &K, context: &PropagationContext<K>) {}
    async fn on_propagation_end(&self, event_key: &K, context: &PropagationContext<K>) {}
}
```

## Built-in Propagators

### AllEqPropagator (Most Common)
Only propagates to handlers with exactly matching event keys:

```rust
let propagator = AllEqPropagator::new();
let event_bus = EventBus::with_propagator(propagator);

// Handler A
let key_a = StructuredEventKey::Client { 
    namespace: "chat".into(), 
    event_name: "message".into() 
};

// Handler B (different namespace)
let key_b = StructuredEventKey::Client { 
    namespace: "game".into(), 
    event_name: "message".into() 
};

// Emitting to key_a will NOT trigger handler B
// This is the "AllEq" behavior - ALL fields must match
```

**Use AllEq when:**
- You want precise event routing
- Plugins should only receive events they specifically registered for
- You're building traditional event systems
- Performance is critical (fastest propagator)

### DefaultPropagator (Broadcast)
Delivers all events to all handlers:

```rust
let propagator = DefaultPropagator::new();
let event_bus = EventBus::with_propagator(propagator);

// ALL handlers receive ALL events (like traditional pub/sub)
```

**Use Default when:**
- You want simple broadcast behavior
- Debugging event flow
- Rapid prototyping
- Legacy system compatibility

### NamespacePropagator (Filtering)
Filters events by namespace with allow/block lists:

```rust
let propagator = NamespacePropagator::new()
    .allow_namespaces(vec![
        EventNamespace::Core,
        EventNamespace::Client,
    ])
    .block_namespaces(vec![
        EventNamespace::Plugin, // Block inter-plugin events
    ]);

let event_bus = EventBus::with_propagator(propagator);
```

**Use Namespace when:**
- You want coarse-grained filtering
- Implementing security boundaries
- Separating system vs user events
- Debugging specific event categories

## Advanced Propagators

### SpatialPropagator (Games/IoT)
Perfect for games, simulations, or IoT systems where events should only reach nearby entities:

```rust
let spatial = SpatialPropagator::new(100.0); // 100 unit radius
let event_bus = EventBus::with_propagator(spatial);

// Update player position (for propagation calculations)
spatial.update_player_position("player_123", 10.0, 20.0, 5.0).await;
spatial.update_player_position("player_456", 50.0, 25.0, 8.0).await;

// Emit spatial event
let context = PropagationContext::new(event_key)
    .with_metadata("source_x", "10.0")
    .with_metadata("source_y", "20.0")
    .with_metadata("source_z", "5.0")
    .with_metadata("target_player", "player_456");

// Only players within 100 units receive the event
event_bus.emit_with_context(key, &event, context).await?;
```

**Implementation Details:**
```rust
#[async_trait]
impl<K: EventKeyType> EventPropagator<K> for SpatialPropagator<K> {
    async fn should_propagate(&self, _event_key: &K, context: &PropagationContext<K>) -> bool {
        // Extract spatial information from context
        let source_pos = match self.extract_position(context, "source") {
            Some(pos) => pos,
            None => return true, // No spatial info = allow
        };

        let target_player = match context.get_metadata("target_player") {
            Some(player) => player,
            None => return true, // No target = allow
        };

        // Get target position
        let positions = self.player_positions.read().await;
        let target_pos = match positions.get(target_player) {
            Some(pos) => *pos,
            None => return true, // Player not found = allow
        };

        // Check distance
        let distance = self.calculate_distance(source_pos, target_pos);
        distance <= self.max_distance
    }

    async fn transform_event(&self, event: Arc<EventData>, context: &PropagationContext<K>) -> Option<Arc<EventData>> {
        // Add distance information to the event
        if let Some(distance) = self.calculate_distance_from_context(context) {
            let mut new_event = (*event).clone();
            new_event.metadata.insert("distance".to_string(), distance.to_string());
            return Some(Arc::new(new_event));
        }
        Some(event)
    }
}
```

**Use Spatial when:**
- Building games with area-of-effect events
- IoT systems with geographic constraints
- Simulation systems
- Any system where proximity matters

### ChannelPropagator (GORC-style)
Implements channel-based routing like Horizon's GORC system:

```rust
let channel_config = ChannelConfig {
    max_frequency: 30.0,  // 30 Hz max
    max_distance: 50.0,   // 50 unit range
    priority: 1,          // High priority
};

let propagator = ChannelPropagator::new()
    .add_channel(0, channel_config.clone())  // Position updates
    .add_channel(1, ChannelConfig {          // Chat messages
        max_frequency: 10.0,
        max_distance: 1000.0,
        priority: 3,
    });

let event_bus = EventBus::with_propagator(propagator);

// Events automatically filtered by channel rules
let gorc_key = StructuredEventKey::Gorc {
    object_type: "Player".into(),
    channel: 0,  // Position channel
    event_name: "position_update".into(),
};
```

**Use Channel when:**
- Implementing replication systems
- Network bandwidth optimization
- Priority-based event routing
- Frequency-limited events

### CompositePropagator (Combining Logic)
Combine multiple propagators with AND/OR logic:

```rust
// AND logic: ALL propagators must allow the event
let strict_propagator = CompositePropagator::new_and()
    .add_propagator(Box::new(AllEqPropagator::new()))
    .add_propagator(Box::new(NamespacePropagator::new().allow_namespaces(vec![
        EventNamespace::Core,
        EventNamespace::Client,
    ])))
    .add_propagator(Box::new(SpatialPropagator::new(100.0)));

// OR logic: ANY propagator can allow the event
let permissive_propagator = CompositePropagator::new_or()
    .add_propagator(Box::new(admin_propagator))     // Admins get everything
    .add_propagator(Box::new(debug_propagator))     // Debug mode gets everything
    .add_propagator(Box::new(spatial_propagator));  // Normal spatial filtering
```

**Use Composite when:**
- Complex routing requirements
- Security + performance constraints
- Gradual system migration
- A/B testing different propagation strategies

## Custom Propagators

### Creating Your Own
Implement the `EventPropagator` trait for your specific needs:

```rust
pub struct NetworkAwarePropagator {
    // Track which handlers are on which network nodes
    node_handlers: Arc<RwLock<HashMap<String, Vec<String>>>>,
    // Track network topology
    network_topology: Arc<RwLock<NetworkGraph>>,
    // Latency thresholds
    max_latency_ms: u64,
}

#[async_trait]
impl EventPropagator<StructuredEventKey> for NetworkAwarePropagator {
    async fn should_propagate(&self, event_key: &StructuredEventKey, context: &PropagationContext<StructuredEventKey>) -> bool {
        // Get source and target nodes
        let source_node = context.get_metadata("source_node")?;
        let target_node = context.get_metadata("target_node")?;
        
        // Check network reachability
        let topology = self.network_topology.read().await;
        if !topology.is_reachable(source_node, target_node) {
            return false;
        }
        
        // Check latency constraints
        let latency = topology.get_latency(source_node, target_node)?;
        if latency > self.max_latency_ms {
            return false;
        }
        
        // Check bandwidth constraints
        match event_key {
            StructuredEventKey::Gorc { channel: 0, .. } => {
                // High-frequency position updates need good bandwidth
                topology.get_bandwidth(source_node, target_node)? > 1_000_000 // 1Mbps
            }
            StructuredEventKey::Client { namespace, .. } if namespace == "chat" => {
                // Chat messages are less demanding
                topology.get_bandwidth(source_node, target_node)? > 10_000 // 10Kbps
            }
            _ => true, // Other events always allowed if reachable
        }
    }
    
    async fn transform_event(&self, event: Arc<EventData>, context: &PropagationContext<StructuredEventKey>) -> Option<Arc<EventData>> {
        // Add network metadata
        let mut new_event = (*event).clone();
        
        if let (Some(source), Some(target)) = (
            context.get_metadata("source_node"),
            context.get_metadata("target_node")
        ) {
            let topology = self.network_topology.read().await;
            if let Some(latency) = topology.get_latency(source, target) {
                new_event.metadata.insert("network_latency".to_string(), latency.to_string());
            }
            if let Some(bandwidth) = topology.get_bandwidth(source, target) {
                new_event.metadata.insert("network_bandwidth".to_string(), bandwidth.to_string());
            }
        }
        
        Some(Arc::new(new_event))
    }
}
```

### Security-Aware Propagator
```rust
pub struct SecurityPropagator {
    // User permissions
    user_permissions: Arc<RwLock<HashMap<u64, HashSet<String>>>>,
    // Event security levels
    event_security: HashMap<String, SecurityLevel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityLevel {
    Public,
    User,
    Admin,
    System,
}

#[async_trait]
impl EventPropagator<StructuredEventKey> for SecurityPropagator {
    async fn should_propagate(&self, event_key: &StructuredEventKey, context: &PropagationContext<StructuredEventKey>) -> bool {
        // Get required security level for this event
        let event_type = event_key.to_string();
        let required_level = self.event_security.get(&event_type)
            .copied()
            .unwrap_or(SecurityLevel::Public);
        
        // Get user ID from context
        let user_id = match context.get_metadata("user_id").and_then(|s| s.parse::<u64>().ok()) {
            Some(id) => id,
            None => return required_level == SecurityLevel::Public,
        };
        
        // Check permissions
        let permissions = self.user_permissions.read().await;
        let user_perms = permissions.get(&user_id).cloned().unwrap_or_default();
        
        match required_level {
            SecurityLevel::Public => true,
            SecurityLevel::User => user_perms.contains("user"),
            SecurityLevel::Admin => user_perms.contains("admin"),
            SecurityLevel::System => user_perms.contains("system"),
        }
    }
    
    async fn transform_event(&self, event: Arc<EventData>, context: &PropagationContext<StructuredEventKey>) -> Option<Arc<EventData>> {
        // Remove sensitive data based on user permissions
        let user_id = context.get_metadata("user_id")?.parse::<u64>().ok()?;
        let permissions = self.user_permissions.read().await;
        let user_perms = permissions.get(&user_id)?;
        
        if !user_perms.contains("admin") {
            // Strip sensitive fields for non-admin users
            let mut new_event = (*event).clone();
            new_event.metadata.remove("server_ip");
            new_event.metadata.remove("internal_id");
            return Some(Arc::new(new_event));
        }
        
        Some(event)
    }
}
```

## Propagation Context

### Understanding Context
The `PropagationContext` carries information about the event emission:

```rust
pub struct PropagationContext<K: EventKeyType> {
    pub event_key: K,                           // The event key being propagated
    pub metadata: HashMap<String, String>,      // Additional context data
}

// Create context with metadata
let context = PropagationContext::new(event_key)
    .with_metadata("user_id", "12345")
    .with_metadata("source_ip", "192.168.1.100")
    .with_metadata("timestamp", &current_timestamp().to_string())
    .with_metadata("priority", "high");
```

### Using Context in Handlers
Handlers can access context metadata:

```rust
event_bus.on_key(key, |event: MyEvent| {
    async move {
        // Context is automatically available in the event metadata
        if let Some(user_id) = event.metadata.get("user_id") {
            println!("Event from user: {}", user_id);
        }
        
        if let Some(priority) = event.metadata.get("priority") {
            if priority == "high" {
                // Handle high-priority events differently
                process_urgently(&event).await?;
            } else {
                process_normally(&event).await?;
            }
        }
        
        Ok(())
    }
}).await?;
```

## Performance Considerations

### Propagator Performance
Different propagators have different performance characteristics:

```rust
// Fastest: O(1) hash lookup
AllEqPropagator           // ~1-5 nanoseconds per check

// Fast: O(1) with simple logic
DefaultPropagator         // ~1 nanosecond per check
NamespacePropagator       // ~10-50 nanoseconds per check

// Moderate: O(1) with computation
SpatialPropagator         // ~100-1000 nanoseconds per check
ChannelPropagator         // ~50-500 nanoseconds per check

// Variable: Depends on composition
CompositePropagator       // Sum of component costs
```

### Optimization Tips

1. **Choose the Right Propagator**:
```rust
// Good: Use AllEq for most cases
let event_bus = EventBus::with_propagator(AllEqPropagator::new());

// Good: Use Spatial only when needed
if game_world.has_spatial_events() {
    let event_bus = EventBus::with_propagator(SpatialPropagator::new(100.0));
}

// Avoid: Using Default for large systems
// let event_bus = EventBus::with_propagator(DefaultPropagator::new()); // Too broad
```

2. **Optimize Context Usage**:
```rust
// Good: Only add necessary metadata
let context = PropagationContext::new(key)
    .with_metadata("user_id", &user_id.to_string());

// Avoid: Excessive metadata
// let context = PropagationContext::new(key)
//     .with_metadata("everything", &serialize_everything());
```

3. **Cache Expensive Calculations**:
```rust
pub struct CachedSpatialPropagator {
    distance_cache: Arc<RwLock<HashMap<(String, String), f32>>>,
    // ... other fields
}

#[async_trait]
impl<K: EventKeyType> EventPropagator<K> for CachedSpatialPropagator {
    async fn should_propagate(&self, event_key: &K, context: &PropagationContext<K>) -> bool {
        let cache_key = (source_player.clone(), target_player.clone());
        
        // Check cache first
        {
            let cache = self.distance_cache.read().await;
            if let Some(&distance) = cache.get(&cache_key) {
                return distance <= self.max_distance;
            }
        }
        
        // Calculate and cache
        let distance = self.calculate_distance(source_pos, target_pos);
        {
            let mut cache = self.distance_cache.write().await;
            cache.insert(cache_key, distance);
        }
        
        distance <= self.max_distance
    }
}
```

## Common Patterns

### Progressive Propagation
Start strict, relax as needed:

```rust
// Start with exact matching
let mut propagator = AllEqPropagator::new();

// Add spatial awareness for game events
if event_key.is_spatial() {
    propagator = CompositePropagator::new_and()
        .add_propagator(Box::new(AllEqPropagator::new()))
        .add_propagator(Box::new(SpatialPropagator::new(100.0)));
}

// Add security for sensitive events
if event_key.is_sensitive() {
    propagator = CompositePropagator::new_and()
        .add_propagator(Box::new(propagator))
        .add_propagator(Box::new(SecurityPropagator::new()));
}
```

### Event Transformation Pipeline
Transform events through multiple stages:

```rust
pub struct TransformationPipeline {
    transformers: Vec<Box<dyn EventTransformer>>,
}

#[async_trait]
impl EventPropagator<StructuredEventKey> for TransformationPipeline {
    async fn should_propagate(&self, _event_key: &StructuredEventKey, _context: &PropagationContext<StructuredEventKey>) -> bool {
        true // Allow all, just transform
    }
    
    async fn transform_event(&self, mut event: Arc<EventData>, context: &PropagationContext<StructuredEventKey>) -> Option<Arc<EventData>> {
        // Apply each transformer in sequence
        for transformer in &self.transformers {
            event = transformer.transform(event, context).await?;
        }
        Some(event)
    }
}

// Use it
let pipeline = TransformationPipeline::new()
    .add_transformer(Box::new(CompressionTransformer::new()))
    .add_transformer(Box::new(EncryptionTransformer::new()))
    .add_transformer(Box::new(MetadataTransformer::new()));
```

### Dynamic Propagator Switching
Change propagation logic at runtime:

```rust
pub struct AdaptivePropagator {
    current: Arc<RwLock<Box<dyn EventPropagator<StructuredEventKey>>>>,
    load_monitor: LoadMonitor,
}

#[async_trait]
impl EventPropagator<StructuredEventKey> for AdaptivePropagator {
    async fn should_propagate(&self, event_key: &StructuredEventKey, context: &PropagationContext<StructuredEventKey>) -> bool {
        // Switch propagator based on system load
        if self.load_monitor.is_high_load() {
            // Use strict filtering under high load
            AllEqPropagator::new().should_propagate(event_key, context).await
        } else {
            // Use normal propagator under normal load
            let current = self.current.read().await;
            current.should_propagate(event_key, context).await
        }
    }
}
```

Event propagation is the key to building flexible, efficient, and scalable plugin systems. Choose the right propagator for your use case, and don't be afraid to implement custom logic when needed!