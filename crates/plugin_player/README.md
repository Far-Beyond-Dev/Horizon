# Player Plugin for Horizon GORC System

A comprehensive player management plugin for the Horizon game engine, implementing the GORC (Game Object Replication and Communication) architecture for scalable, distributed multiplayer gaming.

## 🚀 Overview

The Player Plugin provides complete player lifecycle management, real-time movement replication, combat event handling, communication systems, and detailed ship scanning capabilities. It's designed for high-performance multiplayer games requiring hundreds of concurrent players with smooth, responsive interactions.

## 🏗️ Architecture

### GORC Zone-Based Replication

The plugin utilizes GORC's innovative three-zone replication system for optimal network performance:

| Zone | Purpose | Range | Frequency | Data Types |
|------|---------|-------|-----------|------------|
| **Zone 0** | Critical Movement | 25m | 60Hz | Position, velocity, health |
| **Zone 1** | Detailed State | 100m | 30Hz | Movement state, level |
| **Zone 2** | Social Data | 200m | 15Hz | Name, chat bubble |

### Multi-Channel Communication

The plugin handles four specialized communication channels:

| Channel | Purpose | Range | Priority | Examples |
|---------|---------|-------|----------|----------|
| **0** | Movement | 25m | Critical | Position updates, velocity |
| **1** | Combat | 500m | High | Weapon fire, explosions |
| **2** | Communication | 300m | Medium | Chat, voice, emotes |
| **3** | Scanning | 100m | Low | Ship specs, cargo data |

## 📦 Core Components

### 🎮 Player Object (`player.rs`)

The `GorcPlayer` struct implements the complete player entity with zone-based data organization:

```rust
use plugin_player::player::GorcPlayer;
use horizon_event_system::{PlayerId, Vec3};

// Create a new player
let player = GorcPlayer::new(
    PlayerId(42),
    "SpaceExplorer".to_string(),
    Vec3::new(1000.0, 0.0, 500.0)
);

// Update position with anti-cheat validation
let result = player.validate_and_apply_movement(
    Vec3::new(1005.0, 0.0, 502.0),
    Vec3::new(5.0, 0.0, 2.0)
);
```

### 📡 Event Structures (`events.rs`)

Type-safe event structures for all player interactions:

```rust
use plugin_player::events::*;
use chrono::Utc;

// Movement request (Channel 0)
let movement = PlayerMoveRequest {
    player_id: PlayerId(42),
    new_position: Vec3::new(100.0, 0.0, 50.0),
    velocity: Vec3::new(8.0, 0.0, 4.0),
    movement_state: 2, // Running
    client_timestamp: Utc::now(),
};

// Combat request (Channel 1)
let attack = PlayerAttackRequest {
    player_id: PlayerId(42),
    target_position: Vec3::new(150.0, 0.0, 75.0),
    attack_type: "laser".to_string(),
    client_timestamp: Utc::now(),
};

// Communication request (Channel 2)
let chat = PlayerChatRequest {
    player_id: PlayerId(42),
    message: "Request docking clearance".to_string(),
    channel: "general".to_string(),
    target_player: None,
};
```

### 🔧 Handler Modules (`handlers/`)

Specialized event handlers for different game systems:

- **`connection.rs`** - Player lifecycle management (connect/disconnect)
- **`movement.rs`** - Real-time position updates with validation
- **`combat.rs`** - Weapon firing and combat event distribution
- **`communication.rs`** - Multi-channel chat and messaging
- **`scanning.rs`** - Close-range ship information sharing

## 🛡️ Security Features

### Anti-Cheat Protection
- **Movement Validation**: Maximum distance limits prevent teleportation
- **Velocity Bounds**: Speed limits prevent hacking exploits
- **Ownership Verification**: Players can only control their own entities
- **Timestamp Validation**: Prevents replay attacks and time manipulation

### Input Sanitization
- **Message Length Limits**: 500 character maximum for chat messages
- **Content Filtering**: Profanity and abuse prevention (configurable)
- **Rate Limiting**: Prevents spam and DoS attacks
- **Data Validation**: All inputs validated before processing

## ⚡ Performance Characteristics

### Scalability
- **Spatial Culling**: Events only replicate to relevant nearby players
- **Frequency Optimization**: Different data types use appropriate update rates
- **Memory Efficiency**: Zero-allocation steady-state operation
- **Async Processing**: Non-blocking event handling for high concurrency

### Network Optimization
- **Binary Protocol**: GORC uses efficient binary encoding with JSON payloads
- **Compression**: Automatic compression for large data transfers
- **Batching**: Multiple events batched per network frame
- **Predictive Networking**: Client-side prediction with server reconciliation

## 🚀 Getting Started

### Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
plugin_player = { path = "../plugin_player" }
horizon_event_system = { workspace = true }
```

### Basic Usage

```rust
use plugin_player::PlayerPlugin;
use horizon_event_system::EventSystem;

// The plugin registers automatically with the event system
let plugin = PlayerPlugin::new();

// Plugin handles all player events automatically:
// - Player connections and disconnections
// - Real-time movement and position updates
// - Combat events and weapon firing
// - Chat and communication systems
// - Ship scanning and metadata sharing
```

### Integration Example

```rust
use std::sync::Arc;
use plugin_player::PlayerPlugin;
use horizon_event_system::{EventSystem, create_simple_plugin};

#[tokio::main]
async fn main() {
    // Create the event system
    let event_system = Arc::new(EventSystem::new());
    
    // Create and register the player plugin
    let mut plugin = PlayerPlugin::new();
    
    // Plugin automatically registers all handlers
    // No additional configuration required!
    
    println!("Player plugin ready for {} concurrent players", 1000);
}
```

## 🔬 Advanced Features

### Combat System
- **Weapon Types**: Laser, missile, plasma, kinetic, melee
- **Damage Calculation**: Server-authoritative with client prediction
- **Range Validation**: Automatic weapon range enforcement
- **Visual Effects**: Synchronized across all nearby clients

### Communication System
- **Multiple Channels**: General, emergency, trade, fleet, private
- **Spatial Audio**: Range-based communication (300m standard)
- **Direct Messaging**: Player-to-player private communication
- **Emergency Channels**: Extended range (1000m) for distress signals

### Ship Scanning
- **Intimate Range**: 100m range for detailed ship information
- **Rich Metadata**: Ship class, hull integrity, shield strength, cargo
- **Privacy Controls**: Players control what information they share
- **Real-time Updates**: Automatic updates when ship status changes

## 🔧 Configuration

### Environment Variables
```bash
# Movement validation settings
PLAYER_MAX_MOVEMENT_DELTA=100.0
PLAYER_MAX_VELOCITY=1000.0

# Communication settings  
CHAT_MESSAGE_MAX_LENGTH=500
CHAT_RATE_LIMIT_PER_SECOND=1

# Combat settings
WEAPON_MAX_RANGE=1000.0
COMBAT_RATE_LIMIT_MS=100
```

### Plugin Configuration
```rust
// Custom plugin configuration (future enhancement)
let config = PlayerPluginConfig {
    max_players: 1000,
    movement_validation: true,
    anti_cheat_enabled: true,
    chat_moderation: true,
};

let plugin = PlayerPlugin::with_config(config);
```

## 📊 Monitoring and Debugging

### Logging
The plugin provides comprehensive logging with structured log levels:

```rust
// Enable debug logging
RUST_LOG=debug cargo run

// Plugin-specific logging
RUST_LOG=plugin_player=trace cargo run
```

### Metrics
Key performance metrics are automatically tracked:
- **Player Count**: Current connected players
- **Event Frequency**: Events per second by channel
- **Network Bandwidth**: Bytes sent/received by event type
- **Validation Failures**: Security violations and failed requests

### Health Checks
```rust
// Get plugin status
let status = plugin.health_check().await;
println!("Players: {}, Events/sec: {}", status.player_count, status.events_per_second);
```

## 🧪 Testing

### Unit Tests
```bash
cargo test -p plugin_player
```

### Integration Tests
```bash
cargo test -p plugin_player --test integration
```

### Load Testing
```bash
# Test with 1000 concurrent players
cargo run --bin load_test -- --players 1000 --duration 300s
```

## 🔄 Migration Guide

### From Legacy System
1. Replace old player management with `PlayerPlugin`
2. Update event structures to use new typed events
3. Configure GORC zones for your game's requirements
4. Test movement validation and security features

### Breaking Changes
- Movement events now require `client_timestamp` field
- Attack events use structured `attack_type` instead of raw strings
- Chat events support multi-channel architecture

## 📈 Roadmap

### Upcoming Features
- **Advanced Anti-Cheat**: Machine learning-based cheat detection
- **Voice Communication**: Spatial voice chat integration
- **Clan Systems**: Fleet and organization management
- **Persistence**: Player state saving and loading
- **Analytics**: Real-time gameplay analytics and monitoring

### Performance Improvements
- **WebAssembly Plugins**: Client-side validation plugins
- **GPU Acceleration**: Spatial queries using GPU compute
- **Edge Computing**: Regional server deployment
- **Protocol V2**: Next-generation binary protocol

## 🤝 Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

### Development Setup
```bash
git clone https://github.com/horizon-engine/horizon
cd horizon/crates/plugin_player
cargo build
cargo test
```

### Code Style
- Run `cargo fmt` before committing
- Ensure `cargo clippy` passes without warnings
- Add comprehensive documentation for new features
- Include unit tests for all new functionality

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🆘 Support

- **Documentation**: [https://docs.horizon-engine.com](https://docs.horizon-engine.com)
- **Discord**: [https://discord.gg/horizon-engine](https://discord.gg/horizon-engine)
- **Issues**: [GitHub Issues](https://github.com/horizon-engine/horizon/issues)
- **Email**: support@horizon-engine.com

---

**Built with ❤️ by the Horizon Engine Team**

*Empowering the next generation of multiplayer games*