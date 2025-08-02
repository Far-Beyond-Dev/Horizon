# Universal Plugin System Documentation

Welcome to the Universal Plugin System - a flexible, reusable plugin architecture that can be used across multiple applications.

## Table of Contents

- [Quick Start](quick-start.md) - Get up and running in 5 minutes
- [Architecture](architecture.md) - Core concepts and design principles
- [Event System](event-system.md) - Event handling and propagation
- [Plugin Development](plugin-development.md) - Creating plugins
- [Event Propagation](event-propagation.md) - Custom propagation logic
- [Advanced Usage](advanced-usage.md) - Advanced patterns and techniques
- [Migration Guide](migration-guide.md) - Migrating from other plugin systems
- [API Reference](api-reference.md) - Complete API documentation
- [Examples](examples/) - Code examples and tutorials

## Overview

The Universal Plugin System provides a complete foundation for building plugin-based applications with:

- **Type-Safe Event Handling**: Compile-time guarantees for event routing
- **Flexible Propagation Logic**: Custom event filtering (spatial, network, etc.)
- **Dynamic Plugin Loading**: Runtime plugin management with version compatibility
- **High Performance**: Optimized for high-throughput event processing
- **Memory Safety**: Comprehensive panic handling and safe FFI boundaries

## Key Features

### 🎯 **No Boilerplate Plugin Development**
Create custom event handlers like `on_core`, `on_client`, `on_plugin` without repetitive code.

### 🔑 **Structured Event Keys**
Use typed event keys instead of string parsing for better performance and type safety.

### ⚡ **High-Performance Event Routing**
Concurrent event processing with configurable propagation logic.

### 🛡️ **Memory Safety**
Panic isolation and safe plugin boundaries prevent crashes.

### 🔧 **Flexible Architecture**
Easily recreate existing plugin systems or build entirely new ones.

## Quick Example

```rust
use universal_plugin_system::*;

// Define your events
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PlayerJoinedEvent {
    player_id: u64,
    name: String,
}

impl Event for PlayerJoinedEvent {
    fn event_type() -> &'static str { "player_joined" }
}

// Create event bus with AllEq propagation
let event_bus = Arc::new(EventBus::with_propagator(AllEqPropagator::new()));

// Register handler for client events
let key = StructuredEventKey::Client { 
    namespace: "game".into(), 
    event_name: "player_joined".into() 
};

event_bus.on_key(key.clone(), |event: PlayerJoinedEvent| {
    println!("Player {} joined!", event.name);
    Ok(())
}).await?;

// Emit event
let event = PlayerJoinedEvent {
    player_id: 123,
    name: "Alice".to_string(),
};

event_bus.emit_key(key, &event).await?;
```

## Getting Started

1. **[Quick Start Guide](quick-start.md)** - Basic setup and first plugin
2. **[Architecture Overview](architecture.md)** - Understanding the system design
3. **[Plugin Development](plugin-development.md)** - Writing your first plugin
4. **[Event System](event-system.md)** - Working with events and handlers

## Use Cases

- **Game Servers**: Plugin-based game logic with spatial event propagation
- **Web Applications**: Modular feature systems with custom event routing
- **Desktop Applications**: Plugin ecosystems with hot-swappable components
- **Microservices**: Event-driven service communication
- **IoT Systems**: Device plugin management with network-aware propagation

## License

This project is licensed under the same terms as the parent Horizon project.