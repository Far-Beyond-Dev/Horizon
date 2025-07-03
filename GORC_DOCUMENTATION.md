# Game Object Replication Channels (GORC) Documentation

## Overview

GORC (Game Object Replication Channels) is an advanced replication system integrated into the Horizon Event System that provides fine-grained control over multiplayer game state distribution. It enables developers to optimize network traffic through intelligent channel-based replication with dynamic subscription management.

## Architecture

### Four-Channel System

GORC organizes replication into four distinct channels, each optimized for different types of data:

#### Channel 0: Critical Updates (30-60Hz)
- **Purpose**: Essential game state that must be delivered with minimal latency
- **Data Types**: Player positions, health, collision detection, core gameplay mechanics
- **Priority**: Highest
- **Compression**: Minimal (prioritizing speed over size)
- **Use Cases**: Player movement, combat actions, physics-critical objects

#### Channel 1: Detailed Updates (15-30Hz)
- **Purpose**: Important non-critical information for gameplay quality
- **Data Types**: Animations, weapon states, detailed interactions, AI behaviors
- **Priority**: High
- **Compression**: Balanced (LZ4 recommended)
- **Use Cases**: Character animations, complex object interactions, AI decision updates

#### Channel 2: Cosmetic Updates (5-15Hz)
- **Purpose**: Visual enhancements that improve experience but aren't gameplay-critical
- **Data Types**: Particle effects, environmental animations, audio cues, visual feedback
- **Priority**: Normal
- **Compression**: Higher (Zlib acceptable)
- **Use Cases**: Particle systems, environmental effects, cosmetic animations

#### Channel 3: Metadata (1-5Hz)
- **Purpose**: Informational data with low update frequency requirements
- **Data Types**: Player names, achievements, statistics, session information
- **Priority**: Low
- **Compression**: Maximum (custom compression encouraged)
- **Use Cases**: Player profiles, scoreboards, persistent data, social features

## Core Components

### 1. GorcManager

The central coordinator for all GORC operations.

```rust
use horizon_event_system::{GorcManager, ReplicationLayer, CompressionType};

// Initialize GORC
let gorc = GorcManager::new();

// Create a custom replication layer
let layer = ReplicationLayer::new(
    0,                              // Channel 0 (Critical)
    100.0,                          // 100 unit radius
    60.0,                           // 60Hz frequency
    vec!["position".to_string(), "health".to_string()],
    CompressionType::Lz4,
);

gorc.add_layer("player_core".to_string(), layer).await;
```

### 2. SubscriptionManager

Handles dynamic subscription logic based on proximity, relationships, and interest.

```rust
use horizon_event_system::{SubscriptionManager, PlayerId, Position};

let subscription_mgr = SubscriptionManager::new();

// Add a player to the subscription system
let player_id = PlayerId::new();
let position = Position::new(100.0, 50.0, 200.0);
subscription_mgr.add_player(player_id, position).await;

// Update player position (triggers proximity recalculation if significant movement)
let new_position = Position::new(150.0, 50.0, 200.0);
let should_recalc = subscription_mgr.update_player_position(player_id, new_position).await;

// Add relationship-based subscription (team members get higher priority)
subscription_mgr.add_relationship(
    player_id,
    "team".to_string(),
    vec![teammate1_id, teammate2_id],
).await;
```

### 3. MulticastManager

Efficiently distributes data to groups of players.

```rust
use horizon_event_system::{MulticastManager, ReplicationPriority};
use std::collections::HashSet;

let multicast_mgr = MulticastManager::new();

// Create a multicast group for a team
let channels: HashSet<u8> = vec![0, 1, 2].into_iter().collect();
let group_id = multicast_mgr.create_group(
    "team_alpha".to_string(),
    channels,
    ReplicationPriority::High,
).await;

// Add players to the group
multicast_mgr.add_player_to_group(player1_id, group_id).await;
multicast_mgr.add_player_to_group(player2_id, group_id).await;

// Broadcast data to the entire group
let data = b"team coordination data";
multicast_mgr.broadcast_to_group(group_id, data, 1).await?;
```

### 4. SpatialPartition

Provides efficient spatial queries for proximity-based subscriptions.

```rust
use horizon_event_system::{SpatialPartition, SpatialQuery, QueryFilters, RegionBounds};

let spatial = SpatialPartition::new();

// Add a game region
let bounds = RegionBounds {
    min_x: 0.0, max_x: 1000.0,
    min_y: 0.0, max_y: 100.0,
    min_z: 0.0, max_z: 1000.0,
};
spatial.add_region("main_world".to_string(), bounds).await;

// Update player position in the spatial index
spatial.update_player_position(
    player_id,
    Position::new(500.0, 50.0, 300.0),
    "main_world".to_string(),
).await;

// Query nearby players
let query = SpatialQuery {
    center: Position::new(500.0, 50.0, 300.0),
    radius: 100.0,
    filters: QueryFilters {
        max_results: Some(10),
        min_distance: Some(5.0),
        ..Default::default()
    },
};

let nearby_players = spatial.query_region("main_world", &query).await;
```

## Subscription Types

### Proximity-Based Subscriptions

Automatically manages subscriptions based on spatial distance:

- **Ultra Detail (0-50 units)**: All channels at maximum frequency
- **High Detail (50-150 units)**: Channels 0-2 at high frequency
- **Medium Detail (150-300 units)**: Channels 0-1 at medium frequency
- **Low Detail (300-600 units)**: Channel 0 only at reduced frequency
- **Minimal Detail (600+ units)**: Critical updates only when necessary

### Relationship-Based Subscriptions

Prioritizes updates based on player relationships:

- **Team Members**: Enhanced priority across all channels
- **Guild Members**: Medium priority with social data emphasis
- **Friends**: Normal priority with metadata channel boost
- **Enemies**: Tactical information priority

### Interest-Based Subscriptions

Adapts to player behavior and focus:

- **Active Interaction**: Objects being actively used get highest priority
- **Visual Focus**: Items in the player's view cone receive enhanced updates
- **Predictive**: Machine learning integration for anticipating player needs
- **Activity Patterns**: Historical interaction data influences subscription priorities

## LOD (Level of Detail) System

GORC includes an advanced LOD system with hysteresis to prevent subscription oscillation:

```rust
use horizon_event_system::{LodRoom, LodLevel, HysteresisSettings};

// Create a LOD room
let mut room = LodRoom::new(
    Position::new(0.0, 0.0, 0.0),  // Center position
    LodLevel::High,                 // Base LOD level
);

// Configure hysteresis to prevent rapid switching
room.hysteresis = HysteresisSettings {
    enter_threshold: 5.0,           // 5 units before entering higher LOD
    exit_threshold: 15.0,           // 15 units before dropping to lower LOD
    min_change_interval: Duration::from_millis(500),
};

// Add nested rooms for higher detail levels
let ultra_detail_room = LodRoom::new(
    Position::new(0.0, 0.0, 0.0),
    LodLevel::Ultra,
);
room.add_nested_room(ultra_detail_room);
```

## Integration with Horizon Event System

GORC seamlessly integrates with Horizon's three-tier event model:

```rust
// Access GORC from a game server instance
let server = GameServer::new(config);
let gorc_manager = server.get_gorc_manager();
let subscription_manager = server.get_subscription_manager();
let multicast_manager = server.get_multicast_manager();
let spatial_partition = server.get_spatial_partition();

// Register event handlers for GORC-managed updates
events.on_client("movement", "position_update", |event: serde_json::Value| {
    // Parse position update
    // Update spatial partition
    // Trigger subscription recalculation if needed
    // Distribute updates via appropriate GORC channels
    Ok(())
}).await?;
```

## Performance Characteristics

GORC is designed to meet the following performance targets:

- **Scalability**: Support 10,000+ concurrent connections with linear scaling
- **Latency**: Sub-millisecond event routing for critical channels
- **Memory**: Predictable memory usage growth with object count
- **CPU**: No runtime garbage collection overhead
- **Network**: Adaptive compression reduces bandwidth by 30-70%

## Plugin Extensions

GORC provides plugin hooks for game-specific optimizations:

```rust
// Example: Combat-specific replication strategy
impl Replication for CombatObject {
    fn init_layers() -> Vec<ReplicationLayer> {
        vec![
            // Critical: Health, position, active abilities
            ReplicationLayer::new(0, 75.0, 60.0, 
                vec!["health".to_string(), "position".to_string(), "active_ability".to_string()],
                CompressionType::None
            ),
            // Detailed: Animation states, weapon info
            ReplicationLayer::new(1, 150.0, 30.0,
                vec!["animation".to_string(), "weapon_state".to_string()],
                CompressionType::Lz4
            ),
            // Cosmetic: Visual effects, sound cues
            ReplicationLayer::new(2, 300.0, 15.0,
                vec!["effects".to_string(), "audio_cues".to_string()],
                CompressionType::Zlib
            ),
        ]
    }

    fn get_priority(&self, observer_pos: Position) -> ReplicationPriority {
        let distance = calculate_distance(self.position, observer_pos);
        if self.in_combat && distance < 100.0 {
            ReplicationPriority::Critical
        } else if distance < 200.0 {
            ReplicationPriority::High
        } else {
            ReplicationPriority::Normal
        }
    }

    fn serialize_for_layer(&self, layer: &ReplicationLayer) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Custom serialization based on layer properties
        match layer.channel {
            0 => self.serialize_critical_data(),
            1 => self.serialize_detailed_data(),
            2 => self.serialize_cosmetic_data(),
            3 => self.serialize_metadata(),
            _ => Ok(vec![]),
        }
    }
}
```

## Best Practices

### 1. Channel Assignment
- **Critical Channel (0)**: Only data that directly affects gameplay fairness
- **Detailed Channel (1)**: Information that enhances player experience significantly
- **Cosmetic Channel (2)**: Visual/audio enhancements that can be delayed
- **Metadata Channel (3)**: Information that rarely changes but is important for context

### 2. Spatial Optimization
- Use appropriate radius values for each layer
- Consider game-specific distance scaling
- Implement hysteresis to prevent subscription thrashing
- Leverage spatial partitioning for efficient queries

### 3. Relationship Management
- Regularly update team/guild relationships
- Clean up stale relationships to prevent memory leaks
- Use relationship priorities to enhance social gameplay

### 4. Interest Tracking
- Monitor player interaction patterns
- Update focus areas based on camera/interaction data
- Implement predictive algorithms for better anticipation

### 5. Performance Monitoring
- Monitor subscription counts per player
- Track channel utilization and adjust frequencies
- Use built-in statistics for optimization insights
- Profile custom serialization methods

## Future Enhancements

GORC is designed for extensibility with planned features:

- **Machine Learning Integration**: Predictive subscription management
- **Adaptive Compression**: Channel-specific compression algorithms
- **Cross-Platform Optimization**: Device-specific replication strategies
- **Distributed Regions**: Multi-server spatial coordination
- **Real-time Analytics**: Live performance monitoring and adjustment

## Getting Started

1. **Initialize GORC in your server**:
```rust
let server = GameServer::new(config);
let gorc = server.get_gorc_manager();
```

2. **Define your replication layers** based on your game's data types

3. **Set up subscription management** for your player types

4. **Create multicast groups** for teams, guilds, or other player groups

5. **Implement spatial partitioning** for your game world regions

6. **Monitor and tune** using the built-in statistics and profiling tools

For more examples, see the test files in `crates/horizon_event_system/src/gorc/*/tests`.