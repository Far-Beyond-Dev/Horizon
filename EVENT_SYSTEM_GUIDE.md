# Horizon Event System Guide

This comprehensive guide demonstrates how Horizon's event system components work together in the context of a space MMO. The system provides secure, scalable event routing between clients, server components, and plugins.

## Table of Contents

1. [System Architecture Overview](#system-architecture-overview)
2. [Event Categories](#event-categories)
3. [Client-to-Server Communication](#client-to-server-communication)
4. [Server Internal Events](#server-internal-events)
5. [GORC Replication System](#gorc-replication-system)
6. [Complete Use Case Examples](#complete-use-case-examples)
7. [Security Model](#security-model)
8. [Performance Optimizations](#performance-optimizations)

## System Architecture Overview

```mermaid
graph TB
    subgraph "Client Side"
        Client[WebSocket Client]
        ClientPlugin[Client Plugin/UI]
    end
    
    subgraph "Server Infrastructure"
        WS[WebSocket Handler]
        MsgRouter[Message Router]
        EventSys[Event System Core]
        PathRouter[PathRouter<br/>Hierarchical Lookup]
    end
    
    subgraph "Server Plugins"
        PlayerPlugin[Player Plugin]
        CombatPlugin[Combat Plugin]
        ChatPlugin[Chat Plugin]
        EconomyPlugin[Economy Plugin]
    end
    
    subgraph "GORC System"
        GorcMgr[GORC Instance Manager]
        GorcObj[GORC Objects<br/>Ships, Asteroids, etc.]
        ZoneMgr[Zone Manager<br/>Proximity Detection]
    end
    
    Client --> WS
    WS --> MsgRouter
    MsgRouter --> EventSys
    EventSys --> PathRouter
    EventSys --> PlayerPlugin
    EventSys --> CombatPlugin
    EventSys --> ChatPlugin
    EventSys --> EconomyPlugin
    EventSys --> GorcMgr
    GorcMgr --> GorcObj
    GorcMgr --> ZoneMgr
    EventSys --> Client
```

## Event Categories

The Horizon event system uses three main event categories, each with distinct security and routing characteristics:

### 1. Client Events (`client:namespace:event`)
**Purpose**: Regular client-server communication for UI updates, user actions, etc.

```rust
// Registration (Plugin side)
events.on_client("chat", "send_message", |event: ChatMessageEvent| {
    println!("Player sent chat message: {}", event.message);
    Ok(())
}).await?;

// Emission (Client → Server via WebSocket)
{
    "namespace": "chat",
    "event": "send_message", 
    "data": { "message": "Hello, universe!" }
}
```

### 2. GORC Client Events (`gorc_client:ObjectType:channel:event`)
**Purpose**: Client interactions with server objects (mining, combat, trading, etc.)

```rust
// Registration (Plugin side)
events.on_gorc_client("Asteroid", 3, "mine", 
    |event: GorcEvent, client_player: PlayerId, instance: &mut ObjectInstance| {
        // Validate mining permissions and proximity
        // Update asteroid resources
        // Award materials to player
        Ok(())
    }
).await?;

// Emission (Client → Server via WebSocket)
{
    "type": "gorc_event",
    "object_id": "GorcObjectId(asteroid-uuid)",
    "channel": 3,
    "event": "mine",
    "data": { "mining_tool": "plasma_drill", "duration": 5000 }
}
```

### 3. GORC Instance Events (`gorc_instance:ObjectType:channel:event`)
**Purpose**: Server-internal object state changes that replicate to nearby clients

```rust
// Registration (Plugin side)
events.on_gorc_instance("SpaceShip", 0, "position_update", 
    |event: GorcEvent, instance: &mut ObjectInstance| {
        // Update internal ship state
        // Physics calculations
        Ok(())
    }
).await?;

// Emission (Server internal only)
events.emit_gorc_instance(ship_id, 0, "position_update", &position_data, Dest::Both).await?;
```

## Client-to-Server Communication

### Example: Player Mining an Asteroid

```mermaid
sequenceDiagram
    participant C as Client
    participant WS as WebSocket Handler
    participant MR as Message Router
    participant ES as Event System
    participant MP as Mining Plugin
    participant AM as Asteroid Manager
    participant GM as GORC Manager

    Note over C,GM: Player clicks "Mine Asteroid" button
    
    C->>WS: WebSocket Message
    Note right of C: {"type": "gorc_event",<br/>"object_id": "GorcObjectId(ast-123)",<br/>"channel": 3, "event": "mine",<br/>"data": {"tool": "drill"}}
    
    WS->>MR: route_client_message()
    MR->>MR: Parse native GORC format
    MR->>ES: emit_gorc_client(player_id, ast-123, 3, "mine", data)
    
    Note over ES: Security: Only gorc_client: handlers can receive this
    
    ES->>MP: Handler: on_gorc_client("Asteroid", 3, "mine")
    Note right of MP: Receives: event, client_player_id, &mut instance
    
    MP->>MP: Validate mining permissions
    MP->>MP: Check proximity to asteroid
    MP->>MP: Verify tool compatibility
    
    alt Mining Valid
        MP->>AM: Update asteroid resources
        MP->>ES: emit_gorc_instance(ast-123, 1, "resource_depleted", data, Dest::Both)
        ES->>GM: Replicate to nearby clients
        GM->>C: WebSocket response
        Note left of GM: {"type": "gorc_zone_entry",<br/>"event_type": "resource_depleted",<br/>"object_id": "ast-123", "channel": 1}
        MP->>ES: emit_client_with_context("inventory", "item_added", player_id, materials)
        ES->>C: WebSocket response
        Note left of ES: Inventory update
    else Mining Invalid
        MP->>ES: emit_client_with_context("error", "mining_failed", player_id, reason)
        ES->>C: Error message
    end
```

### Code Implementation

**Client Side (JavaScript/WebAssembly)**:
```javascript
function mineAsteroid(asteroidId, miningTool) {
    const miningRequest = {
        type: "gorc_event",
        object_id: asteroidId,
        channel: 3, // Interaction channel
        event: "mine",
        data: {
            tool: miningTool,
            duration_ms: 5000
        }
    };
    
    websocket.send(JSON.stringify(miningRequest));
}
```

**Server Plugin**:
```rust
impl Plugin for MiningPlugin {
    async fn register_handlers(&mut self, events: Arc<EventSystem>, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        // Handle client mining requests
        events.on_gorc_client("Asteroid", 3, "mine", {
            let context = context.clone();
            move |event: GorcEvent, client_player: PlayerId, asteroid: &mut ObjectInstance| {
                // Parse mining data
                let mining_data: MiningRequest = serde_json::from_slice(&event.data)?;
                
                // Validate player has the tool
                if !player_has_tool(client_player, &mining_data.tool)? {
                    return Err(EventError::HandlerExecution("Missing required tool".to_string()));
                }
                
                // Check proximity (asteroid position vs player position)
                let player_pos = get_player_position(client_player)?;
                let asteroid_pos = asteroid.object.position();
                if player_pos.distance(asteroid_pos) > MINING_RANGE {
                    return Err(EventError::HandlerExecution("Too far from asteroid".to_string()));
                }
                
                // Extract resources
                let extracted = extract_resources(asteroid, &mining_data.tool, mining_data.duration_ms);
                
                // Update asteroid state
                asteroid.object.set_health(asteroid.object.health() - extracted.damage);
                
                // Award resources to player
                add_to_player_inventory(client_player, extracted.materials)?;
                
                // Emit resource depletion event to nearby players
                context.events().emit_gorc_instance(
                    event.object_id.parse()?, 
                    1, // Detail channel
                    "resource_depleted", 
                    &ResourceDepletedEvent {
                        remaining_health: asteroid.object.health(),
                        extracted_materials: extracted.materials.clone(),
                        extractor_player: client_player,
                    },
                    horizon_event_system::Dest::Both
                )?;
                
                Ok(())
            }
        }).await?;
        
        Ok(())
    }
}
```

## Server Internal Events

### Example: Ship Movement and Combat

```mermaid
sequenceDiagram
    participant P as Physics Engine
    participant ES as Event System
    participant MP as Movement Plugin
    participant CP as Combat Plugin
    participant GM as GORC Manager
    participant C1 as Client 1 (Ship Owner)
    participant C2 as Client 2 (Nearby Ship)

    Note over P,C2: Ship engines fire, updating position
    
    P->>ES: emit_gorc_instance(ship_id, 0, "position_update", pos_data, Dest::Both)
    
    par Server-side handlers
        ES->>MP: on_gorc_instance("SpaceShip", 0, "position_update")
        Note right of MP: Updates internal tracking
        MP->>MP: Update ship trajectory
        MP->>MP: Check for collisions
    and Client replication
        ES->>GM: emit_to_gorc_subscribers(ship_id, channel=0)
        GM->>GM: Find players within radius
        GM->>C1: Position update
        Note right of C1: Ship owner sees movement
        GM->>C2: Position update  
        Note right of C2: Nearby player sees movement
    end
    
    Note over MP,CP: Ship enters weapons range of enemy
    
    MP->>MP: Detect proximity trigger
    MP->>ES: emit_gorc_instance(ship_id, 2, "combat_range_entered", target_data, Dest::Server)
    
    Note over ES: Dest::Server = only server handlers, no clients
    
    ES->>CP: on_gorc_instance("SpaceShip", 2, "combat_range_entered") 
    CP->>CP: Enable combat systems
    CP->>CP: Update threat assessment
    CP->>ES: emit_gorc_instance(ship_id, 1, "combat_status_changed", status, Dest::Both)
    
    ES->>GM: Replicate to clients
    GM->>C1: Combat UI update
    GM->>C2: Target indicators update
```

### Multi-Channel Replication Example

Different channels serve different purposes and have different ranges:

```rust
// Channel 0: Critical (long range) - Position, health
events.emit_gorc_instance(ship_id, 0, "position_update", &pos, Dest::Both).await?;

// Channel 1: Detailed (medium range) - Combat status, shields  
events.emit_gorc_instance(ship_id, 1, "shield_hit", &damage, Dest::Both).await?;

// Channel 2: Social (medium range) - Chat, emotes
events.emit_gorc_instance(ship_id, 2, "player_chat", &msg, Dest::Both).await?;

// Channel 3: Metadata (short range) - Detailed scans, cargo
events.emit_gorc_instance(ship_id, 3, "cargo_scan_result", &cargo, Dest::Both).await?;
```

## GORC Replication System

The GORC (Game Object Replication & Communication) system handles efficient state synchronization between server objects and clients based on proximity.

### Zone Entry/Exit Flow

```mermaid
sequenceDiagram
    participant C as Client
    participant GM as GORC Manager
    participant ZM as Zone Manager
    participant ES as Event System
    participant Ship as Ship Object

    Note over C,Ship: Player moves toward a ship
    
    C->>GM: Position update
    GM->>ZM: update_player_position(player_id, new_pos)
    ZM->>ZM: Calculate proximity to all objects
    
    Note over ZM: Player enters ship's channel 0 radius (1000m)
    
    ZM->>GM: Zone entry: (ship_id, channel=0)
    GM->>Ship: get_object_state_for_layer(ship_id, channel=0)
    Ship->>GM: Current ship state data
    GM->>C: Zone entry message
    Note right of C: {"type": "gorc_zone_entry",<br/>"object_id": "ship-123",<br/>"object_type": "SpaceShip",<br/>"channel": 0,<br/>"zone_data": {...current state...}}
    
    Note over C,Ship: Player moves closer
    
    C->>GM: Position update
    GM->>ZM: update_player_position(player_id, new_pos)
    
    Note over ZM: Player enters ship's channel 1 radius (500m)
    
    ZM->>GM: Zone entry: (ship_id, channel=1)
    GM->>Ship: get_object_state_for_layer(ship_id, channel=1)
    Ship->>GM: Detailed ship state
    GM->>C: Zone entry message (channel 1)
    
    Note over C,Ship: Player moves away
    
    C->>GM: Position update  
    GM->>ZM: update_player_position(player_id, new_pos)
    
    Note over ZM: Player exits ship's channel 1 radius
    
    ZM->>GM: Zone exit: (ship_id, channel=1)
    GM->>C: Zone exit message
    Note right of C: {"type": "gorc_zone_exit",<br/>"object_id": "ship-123",<br/>"channel": 1}
```

### Replication Layer Configuration

```rust
// Example: SpaceShip replication layers
impl GorcObject for SpaceShip {
    fn get_layers(&self) -> Vec<ReplicationLayer> {
        vec![
            ReplicationLayer {
                channel: 0,
                radius: 1000.0,    // 1km - Basic presence
                update_frequency: 10.0, // 10 Hz
                compression_level: CompressionLevel::High,
            },
            ReplicationLayer {
                channel: 1,
                radius: 500.0,     // 500m - Combat details  
                update_frequency: 30.0, // 30 Hz
                compression_level: CompressionLevel::Medium,
            },
            ReplicationLayer {
                channel: 2, 
                radius: 300.0,     // 300m - Chat/social
                update_frequency: 5.0,  // 5 Hz
                compression_level: CompressionLevel::Low,
            },
            ReplicationLayer {
                channel: 3,
                radius: 100.0,     // 100m - Detailed scans
                update_frequency: 60.0, // 60 Hz  
                compression_level: CompressionLevel::None,
            },
        ]
    }
}
```

## Complete Use Case Examples

### 1. Player vs Player Combat Sequence

```mermaid
sequenceDiagram
    participant P1 as Player 1 Client
    participant P2 as Player 2 Client  
    participant ES as Event System
    participant CP as Combat Plugin
    participant GM as GORC Manager
    participant Ship1 as Player 1 Ship
    participant Ship2 as Player 2 Ship

    Note over P1,Ship2: Player 1 targets Player 2 and fires weapons
    
    P1->>ES: gorc_client event
    Note right of P1: {"type": "gorc_event",<br/>"object_id": "ship1-id",<br/>"event": "fire_weapons",<br/>"data": {"target": "ship2-id"}}
    
    ES->>CP: on_gorc_client("SpaceShip", 2, "fire_weapons")
    Note right of CP: Validate: ammo, range, line-of-sight
    
    CP->>CP: Calculate hit probability  
    CP->>CP: Apply damage to target
    
    par Weapon fire effects
        CP->>ES: emit_gorc_instance(ship1_id, 1, "weapon_fired", data, Dest::Both)
        ES->>GM: Replicate to nearby players
        GM->>P1: Weapon animation
        GM->>P2: Incoming projectile warning
    and Target damage
        CP->>ES: emit_gorc_instance(ship2_id, 0, "hull_damage", damage, Dest::Both)  
        ES->>GM: Replicate damage
        GM->>P2: Damage effects, health update
        GM->>P1: Confirm hit
    end
    
    Note over Ship2: Check if ship destroyed
    
    alt Ship 2 Destroyed
        CP->>ES: emit_gorc_instance(ship2_id, 0, "ship_destroyed", data, Dest::Both)
        ES->>GM: Death explosion effects
        GM->>P1: Victory notification
        GM->>P2: Death screen, respawn options
        
        CP->>ES: emit_client_with_context("economy", "loot_dropped", p1_id, loot)
        ES->>P1: Loot pickup notification
    else Ship 2 Survives
        CP->>ES: emit_gorc_instance(ship2_id, 2, "under_attack", attacker_data, Dest::Both)
        ES->>GM: Combat status update
        GM->>P2: Combat UI activation
    end
```

### 2. Trading Between Players

```mermaid
sequenceDiagram
    participant Buyer as Buyer Client
    participant Seller as Seller Client
    participant ES as Event System
    participant TP as Trading Plugin
    participant EP as Economy Plugin
    participant Station as Trade Station

    Note over Buyer,Station: Buyer approaches trade station
    
    Buyer->>ES: gorc_client("TradeStation", 3, "request_trade_list")
    ES->>TP: Handle trade list request
    TP->>Station: Get available offers
    Station->>TP: Current market data
    TP->>ES: emit_client_with_context("trading", "market_data", buyer_id, data)
    ES->>Buyer: Market UI update
    
    Note over Buyer,Seller: Buyer wants to buy from seller
    
    Buyer->>ES: gorc_client("TradeStation", 3, "initiate_trade", seller_offer_id) 
    ES->>TP: Handle trade initiation
    TP->>TP: Validate offer still available
    TP->>TP: Lock seller's items
    
    TP->>ES: emit_client_with_context("trading", "trade_request", seller_id, trade_data)
    ES->>Seller: Trade request notification
    
    Seller->>ES: client("trading", "accept_trade", trade_id)
    ES->>TP: Handle trade acceptance
    
    par Execute Trade
        TP->>EP: Transfer credits: buyer → seller  
        EP->>ES: emit_client_with_context("economy", "credits_changed", buyer_id, new_balance)
        EP->>ES: emit_client_with_context("economy", "credits_changed", seller_id, new_balance)
    and Transfer Items  
        TP->>ES: emit_client_with_context("inventory", "items_removed", seller_id, items)
        TP->>ES: emit_client_with_context("inventory", "items_added", buyer_id, items)
    and Station Updates
        TP->>ES: emit_gorc_instance(station_id, 1, "trade_completed", trade_data, Dest::Both)
        ES->>Station: Update market statistics
    end
    
    ES->>Buyer: Trade completion notification
    ES->>Seller: Trade completion notification
```

### 3. Fleet Coordination

```mermaid
sequenceDiagram
    participant FC as Fleet Commander
    participant M1 as Member 1
    participant M2 as Member 2  
    participant ES as Event System
    participant FP as Fleet Plugin
    participant TacticalAI as Tactical AI

    Note over FC,TacticalAI: Fleet commander issues movement order
    
    FC->>ES: client("fleet", "issue_order", movement_command)
    ES->>FP: Handle fleet command
    FP->>FP: Validate commander authority
    FP->>FP: Parse tactical order
    
    par Broadcast to Fleet Members
        FP->>ES: emit_client_with_context("fleet", "movement_order", m1_id, order)
        FP->>ES: emit_client_with_context("fleet", "movement_order", m2_id, order)  
    and AI Coordination
        FP->>TacticalAI: Calculate formation positions
        TacticalAI->>FP: Optimal ship positions
    end
    
    ES->>M1: Movement order UI
    ES->>M2: Movement order UI
    
    Note over M1,M2: Fleet members acknowledge and execute
    
    par Member Responses
        M1->>ES: client("fleet", "acknowledge_order", order_id)
        M2->>ES: client("fleet", "acknowledge_order", order_id) 
    and Position Updates
        M1->>ES: gorc_client(ship1_id, 0, "update_formation_pos", target_pos)
        M2->>ES: gorc_client(ship2_id, 0, "update_formation_pos", target_pos)
    end
    
    ES->>FP: Collect acknowledgments
    FP->>FP: Track formation compliance
    
    FP->>ES: emit_client_with_context("fleet", "formation_status", fc_id, status)
    ES->>FC: Formation status update
    
    Note over FC,TacticalAI: Formation complete - tactical advantages active
    
    FP->>ES: emit_gorc_instance(fleet_id, 2, "formation_bonus_active", bonus_data, Dest::Both)
    ES->>FC: Fleet combat bonuses UI
    ES->>M1: Fleet combat bonuses UI  
    ES->>M2: Fleet combat bonuses UI
```

## Security Model

The event system enforces strict security boundaries between client and server code:

### Client Security Restrictions

```mermaid
graph TB
    subgraph "Client (Untrusted)"
        ClientCode[Client Code]
        WebSocket[WebSocket Connection]
    end
    
    subgraph "Server (Trusted)"
        MessageRouter[Message Router<br/>Security Gateway]
        EventSystem[Event System]
    end
    
    subgraph "Allowed Paths"
        ClientEvents[<span style="color:black">client: events<br/>✅ UI updates, user actions</span>]
        GorcClientEvents[<span style="color:black">gorc_client: events<br/>✅ Object interactions</span>]
    end
    
    subgraph "Blocked Paths"  
        GorcInstanceEvents[<span style="color:black">gorc_instance: events<br/>❌ Server-only</span>]
        CoreEvents[<span style="color:black">core: events<br/>❌ Server-only</span>]
    end
    
    ClientCode --> WebSocket
    WebSocket --> MessageRouter
    MessageRouter --> EventSystem
    EventSystem --> ClientEvents
    EventSystem --> GorcClientEvents
    
    EventSystem -.->|BLOCKED| GorcInstanceEvents
    EventSystem -.->|BLOCKED| CoreEvents
    
    style GorcInstanceEvents fill:#ffcccc
    style CoreEvents fill:#ffcccc
    style ClientEvents fill:#ccffcc
    style GorcClientEvents fill:#ccffcc
```

### Security Validation Flow

```rust
// Example: Mining validation in gorc_client handler
events.on_gorc_client("Asteroid", 3, "mine", 
    |event: GorcEvent, client_player: PlayerId, asteroid: &mut ObjectInstance| {
        
        // 1. Proximity Check
        let player_pos = get_player_position(client_player)?;
        let asteroid_pos = asteroid.object.position();
        if player_pos.distance(asteroid_pos) > MAX_MINING_RANGE {
            return Err(EventError::HandlerExecution("Player too far from asteroid".into()));
        }
        
        // 2. Equipment Check  
        let player_ship = get_player_ship(client_player)?;
        if !player_ship.has_mining_equipment() {
            return Err(EventError::HandlerExecution("No mining equipment equipped".into()));
        }
        
        // 3. Cooldown Check
        if !check_mining_cooldown(client_player) {
            return Err(EventError::HandlerExecution("Mining still on cooldown".into()));
        }
        
        // 4. Asteroid State Check
        if asteroid.object.health() <= 0.0 {
            return Err(EventError::HandlerExecution("Asteroid is depleted".into()));
        }
        
        // All checks passed - execute mining
        execute_mining_operation(client_player, asteroid)
    }
).await?;
```

### Anti-Cheat Features

1. **Server Authority**: All game state changes happen server-side
2. **Proximity Validation**: Client actions validated against server position data  
3. **Rate Limiting**: Built-in cooldowns prevent spam/exploitation
4. **State Verification**: Server validates all client requests against current game state
5. **Audit Trail**: All client events logged with player ID and timestamp

## Performance Optimizations

### Path-Based Event Routing

The system uses a hierarchical PathRouter for efficient event lookups:

```rust
// Traditional flat lookup: O(n) scan for similar events
// handlers.get("gorc_instance:SpaceShip:0:move") -> None
// scan all handlers for debugging: expensive!

// Path-based lookup: O(log n) tree traversal  
// gorc_instance -> SpaceShip -> 0 -> move
// Find similar paths by tree traversal: efficient!
```

### GORC Replication Optimizations

1. **Proximity-Based Updates**: Only send events to players within range
2. **Channel Layering**: Different update frequencies per detail level
3. **Compression**: Higher compression for distant/less critical updates  
4. **Batching**: Multiple events combined into single WebSocket messages
5. **Delta Compression**: Only send changed state, not full snapshots

### Event Handler Performance

```rust
// Lock-free concurrent access using DashMap
pub struct EventSystem {
    handlers: DashMap<CompactString, SmallVec<[Arc<dyn EventHandler>; 4]>>,
    // ...
}

// SmallVec optimization: no heap allocation for ≤4 handlers per event
// Arc<dyn EventHandler>: shared ownership, efficient cloning
// CompactString: reduced memory usage for short event keys
```

### Serialization Optimizations

```rust
// High-performance serialization pool
pub struct SerializationBufferPool {
    // Pre-allocated buffers, reused across events
    // Reduces allocation pressure under high load
}

// Event data shared via Arc<Vec<u8>>
// Single serialization, multiple handler deliveries
let data = self.serialization_pool.serialize_event(event)?;
for handler in event_handlers.iter() {
    let data_arc = data.clone(); // Clone Arc, not data
    // ...
}
```