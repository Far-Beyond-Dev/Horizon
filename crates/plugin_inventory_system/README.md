# 🎒 Advanced Inventory Management System

A generic all inclusive inventory system with full support for modern game mechanics including equipment, trading, crafting, enchantments, and containers.

[![Version](https://img.shields.io/badge/version-0.2.0-blue.svg)](https://github.com/your-repo/inventory-system)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org/)

## 📑 Table of Contents

- [Features](#-features)
- [Quick Start](#-quick-start)
- [API Reference](#-api-reference)
- [Event System](#-event-system)
- [Configuration](#-configuration)
- [Examples](#-examples)
- [Error Handling](#-error-handling)
- [Integration Guide](#-integration-guide)
- [Performance & Scaling](#-performance--scaling)

## 🌟 Features

### Core Inventory Management
- **Multi-Inventory Support** - Players can have multiple named inventories (general, bank, quest, etc.)
- **Advanced Item Stacking** - Intelligent stacking with customizable stack limits
- **Weight & Space Constraints** - Realistic inventory limitations with weight and slot management
- **Item Categories & Filtering** - Organize items by type (weapons, armor, consumables, materials, etc.)

### Equipment System
- **Comprehensive Equipment Slots** - Helmet, chest, legs, boots, gloves, weapons, rings, necklace
- **Custom Equipment Slots** - Define game-specific equipment slots
- **Equipment Effects** - Automatic stat application when items are equipped
- **Equipment Validation** - Ensure items can only be equipped in appropriate slots

### Trading System
- **Secure Player-to-Player Trading** - Anti-duplication with transaction validation
- **Trade Offers** - Create offers with requested items and expiration times
- **Trade History** - Track completed and failed trades
- **Material Validation** - Ensure traded items exist and are owned

### Advanced Features
- **Crafting System** - Recipe-based crafting with skill requirements
- **Search & Organization** - Advanced search, sorting, and pagination
- **Container System** - World containers with permission management
- **Item Enhancement** - Enchantment and repair systems
- **Effect Management** - Track temporary and permanent item effects
- **Statistics & Monitoring** - Comprehensive system health tracking

## 🚀 Quick Start

### Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
plugin_inventory_system = { path = "path/to/inventory_system" }
event_system = { path = "../event_system" }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.0", features = ["full"] }
async-trait = "0.1"
uuid = { version = "1.0", features = ["v4", "serde"] }
```

### Basic Setup

```rust
use plugin_inventory_system::*;

// Initialize the inventory system
let mut inventory_system = InventorySystem::new();

// Register with your event system
inventory_system.register_handlers(events).await?;
```

## 📖 API Reference

### Core Inventory Operations

---

#### `PickupItem`
Add items to a player's inventory.

**Request:**
```json
{
  "id": 12345,
  "item_count": 5,
  "item_id": 1001,
  "inventory_name": "general",
  "target_slot": 10
}
```

**Success Response Event:** `item_picked_up`
```json
{
  "player_id": 12345,
  "item_id": 1001,
  "amount": 5,
  "item_instance_id": "550e8400-e29b-41d4-a716-446655440000",
  "inventory_name": "general",
  "target_slot": 10,
  "source": "pickup",
  "timestamp": 1640995200
}
```

**Additional Events:**
- `inventory_updated` - Signals inventory state change
```json
{
  "player_id": 12345,
  "inventory_name": "general",
  "action": "item_added",
  "timestamp": 1640995200
}
```

**Error Response Event:** `pickup_failed`
```json
{
  "player_id": 12345,
  "item_id": 1001,
  "amount": 5,
  "reason": "Inventory full",
  "timestamp": 1640995200
}
```

---

#### `DropItem`
Remove items from inventory and drop them in the world.

**Request:**
```json
{
  "id": 12345,
  "item_count": 3,
  "item_id": 1001,
  "inventory_name": "general",
  "slot_id": 5,
  "position": {
    "x": 100.0,
    "y": 50.0,
    "z": 200.0,
    "world": "main"
  }
}
```

**Success Response Event:** `item_dropped`
```json
{
  "player_id": 12345,
  "item_id": 1001,
  "amount": 3,
  "item_instance_id": "550e8400-e29b-41d4-a716-446655440001",
  "inventory_name": "general",
  "slot_id": 5,
  "position": {
    "x": 100.0,
    "y": 50.0,
    "z": 200.0,
    "world": "main"
  },
  "reason": "drop",
  "timestamp": 1640995200
}
```

**World System Event:** `item_world_drop`
```json
{
  "item_instance": {
    "definition_id": 1001,
    "instance_id": "550e8400-e29b-41d4-a716-446655440001",
    "stack": 3,
    "durability": 100,
    "enchantments": [],
    "bound_to_player": null,
    "acquired_timestamp": 1640995100,
    "custom_data": {}
  },
  "position": {
    "x": 100.0,
    "y": 50.0,
    "z": 200.0,
    "world": "main"
  },
  "dropped_by": 12345,
  "timestamp": 1640995200
}
```

---

#### `CheckItem`
Check if a player has sufficient items.

**Request:**
```json
{
  "id": 12345,
  "item_id": 1001,
  "required_amount": 5,
  "inventory_name": "general",
  "include_equipped": false
}
```

**Response Event:** `item_check_result`
```json
{
  "player_id": 12345,
  "item_id": 1001,
  "required_amount": 5,
  "available_amount": 8,
  "has_item": true,
  "locations": [
    {
      "location_type": "Inventory",
      "location_name": "general",
      "slot_id": 5,
      "amount": 3,
      "item_instance_id": "550e8400-e29b-41d4-a716-446655440002",
      "condition": "Excellent"
    },
    {
      "location_type": "Inventory",
      "location_name": "general", 
      "slot_id": 12,
      "amount": 5,
      "item_instance_id": "550e8400-e29b-41d4-a716-446655440003",
      "condition": "Good"
    }
  ],
  "inventory_checked": "general",
  "include_equipped": false,
  "total_weight": 28.0,
  "total_value": 1200,
  "timestamp": 1640995200
}
```

---

#### `GetInventory`
Retrieve detailed inventory information.

**Request:**
```json
{
  "id": 12345,
  "inventory_name": "general",
  "include_equipment": true
}
```

**Response Event:** `inventory_data`
```json
{
  "player_id": 12345,
  "inventory_name": "general",
  "inventory_data": {
    "player_id": 12345,
    "inventory_name": "general",
    "slots": {
      "0": {
        "slot_id": 0,
        "item": {
          "instance": {
            "definition_id": 1001,
            "instance_id": "550e8400-e29b-41d4-a716-446655440000",
            "stack": 1,
            "durability": 85,
            "enchantments": [
              {
                "enchantment_id": "sharpness",
                "name": "Sharpness",
                "level": 2,
                "effects": [
                  {
                    "effect_type": "damage_bonus",
                    "value": 10.0,
                    "duration": null
                  }
                ]
              }
            ],
            "bound_to_player": null,
            "acquired_timestamp": 1640995100,
            "custom_data": {}
          },
          "definition": {
            "id": 1001,
            "name": "Iron Sword",
            "description": "A sturdy iron sword",
            "category": "Weapon",
            "rarity": "Common",
            "max_stack": 1,
            "weight": 3.5,
            "tradeable": true,
            "value": 150
          },
          "total_value": 350,
          "weight": 3.5,
          "condition": "Good"
        },
        "locked": false
      }
    },
    "equipment": {
      "helmet": null,
      "chest": null,
      "weapon_main": {
        "definition_id": 1001,
        "instance_id": "550e8400-e29b-41d4-a716-446655440004",
        "stack": 1,
        "durability": 92
      }
    },
    "total_items": 15,
    "unique_items": 8,
    "total_weight": 45.2,
    "weight_limit": 100.0,
    "slot_count": 27,
    "last_modified": 1640995150
  },
  "include_equipment": true,
  "timestamp": 1640995200
}
```

---

### Equipment System

#### `EquipItem`
Equip an item to an equipment slot.

**Request:**
```json
{
  "id": 12345,
  "item_instance_id": "550e8400-e29b-41d4-a716-446655440000",
  "equipment_slot": "weapon_main",
  "from_inventory": "general"
}
```

**Success Response Event:** `item_equipped`
```json
{
  "player_id": 12345,
  "item_instance_id": "550e8400-e29b-41d4-a716-446655440000",
  "equipment_slot": "weapon_main",
  "from_inventory": "general",
  "replaced_item": "550e8400-e29b-41d4-a716-446655440005",
  "timestamp": 1640995200
}
```

**Stats System Event:** `equipment_changed`
```json
{
  "player_id": 12345,
  "equipped_item": {
    "definition_id": 1001,
    "instance_id": "550e8400-e29b-41d4-a716-446655440000",
    "enchantments": [
      {
        "enchantment_id": "sharpness",
        "level": 2,
        "effects": [
          {
            "effect_type": "damage_bonus",
            "value": 10.0
          }
        ]
      }
    ]
  },
  "equipment_slot": "weapon_main",
  "action": "equipped"
}
```

---

### Trading System

#### `CreateTrade`
Create a trade offer between players.

**Request:**
```json
{
  "from_player": 12345,
  "to_player": 67890,
  "offered_items": [
    {
      "item_instance": {
        "definition_id": 1001,
        "instance_id": "550e8400-e29b-41d4-a716-446655440000",
        "stack": 1
      },
      "quantity": 1
    }
  ],
  "requested_items": [
    {
      "item_instance": {
        "definition_id": 3001,
        "instance_id": "550e8400-e29b-41d4-a716-446655440006", 
        "stack": 5
      },
      "quantity": 5
    }
  ],
  "expires_in_seconds": 300
}
```

**Success Response Event:** `trade_created`
```json
{
  "trade_id": "trade-550e8400-e29b-41d4-a716-446655440000",
  "from_player": 12345,
  "to_player": 67890,
  "offered_items": [
    {
      "item_instance": {
        "definition_id": 1001,
        "instance_id": "550e8400-e29b-41d4-a716-446655440000",
        "stack": 1
      },
      "quantity": 1
    }
  ],
  "requested_items": [
    {
      "item_instance": {
        "definition_id": 3001,
        "instance_id": "550e8400-e29b-41d4-a716-446655440006",
        "stack": 5
      },
      "quantity": 5
    }
  ],
  "expires_at": 1640995500,
  "timestamp": 1640995200
}
```

**Notification Event:** `trade_request`
```json
{
  "target_player": 67890,
  "from_player": 12345,
  "trade_id": "trade-550e8400-e29b-41d4-a716-446655440000",
  "message": "You have received a trade offer from player 12345"
}
```

#### `TradeAction`
Perform an action on a trade.

**Request:**
```json
{
  "player_id": 67890,
  "trade_id": "trade-550e8400-e29b-41d4-a716-446655440000",
  "action": "Accept"
}
```

**Success Response Event:** `trade_completed`
```json
{
  "trade_id": "trade-550e8400-e29b-41d4-a716-446655440000",
  "from_player": 12345,
  "to_player": 67890,
  "completed_at": 1640995250
}
```

---

### Search & Sorting

#### `SearchInventory`
Search through a player's inventories.

**Request:**
```json
{
  "query": {
    "player_id": 12345,
    "inventory_types": ["general", "bank"],
    "category_filter": "Weapon",
    "name_search": "sword",
    "rarity_filter": "Rare",
    "sort_by": "Name",
    "sort_order": "Ascending",
    "pagination": {
      "page": 1,
      "per_page": 20
    },
    "include_equipped": false
  }
}
```

**Response Event:** `search_results`
```json
{
  "player_id": 12345,
  "query": {
    "player_id": 12345,
    "inventory_types": ["general", "bank"],
    "category_filter": "Weapon",
    "name_search": "sword"
  },
  "results": {
    "items": [
      {
        "definition_id": 1001,
        "instance_id": "550e8400-e29b-41d4-a716-446655440000",
        "stack": 1,
        "durability": 85,
        "enchantments": []
      },
      {
        "definition_id": 1002,
        "instance_id": "550e8400-e29b-41d4-a716-446655440007",
        "stack": 1,
        "durability": 100,
        "enchantments": []
      }
    ],
    "total_count": 2,
    "page": 1,
    "per_page": 20,
    "total_pages": 1
  },
  "timestamp": 1640995200
}
```

---

### Crafting System

#### `CraftItem`
Craft items using a recipe.

**Request:**
```json
{
  "player_id": 12345,
  "recipe_id": "craft_iron_sword",
  "quantity": 1,
  "use_inventory": "general"
}
```

**Success Response Event:** `item_crafted`
```json
{
  "player_id": 12345,
  "recipe_id": "craft_iron_sword",
  "quantity": 1,
  "crafted_items": [
    {
      "definition_id": 1001,
      "instance_id": "550e8400-e29b-41d4-a716-446655440008",
      "stack": 1,
      "durability": 100,
      "enchantments": []
    }
  ],
  "consumed_items": [
    {
      "definition_id": 4001,
      "instance_id": "550e8400-e29b-41d4-a716-446655440009",
      "stack": 3
    }
  ],
  "experience_gained": 50,
  "crafting_time": 30,
  "inventory_used": "general",
  "timestamp": 1640995200
}
```

**Skills System Event:** `experience_gained`
```json
{
  "player_id": 12345,
  "skill_type": "crafting",
  "experience": 50,
  "source": "crafting",
  "recipe_id": "craft_iron_sword"
}
```

---

### Item Enhancement

#### `EnchantItem`
Add an enchantment to an item.

**Request:**
```json
{
  "player_id": 12345,
  "item_instance_id": "550e8400-e29b-41d4-a716-446655440000",
  "enchantment": {
    "enchantment_id": "sharpness",
    "name": "Sharpness",
    "level": 1,
    "effects": [
      {
        "effect_type": "damage_bonus",
        "value": 5.0,
        "duration": null
      }
    ]
  },
  "enchantment_materials": [
    {
      "item_instance": {
        "definition_id": 7001,
        "instance_id": "550e8400-e29b-41d4-a716-446655440010",
        "stack": 1
      },
      "quantity": 1
    }
  ]
}
```

**Success Response Event:** `item_enchanted`
```json
{
  "player_id": 12345,
  "item_instance_id": "550e8400-e29b-41d4-a716-446655440000",
  "enchantment": {
    "enchantment_id": "sharpness",
    "name": "Sharpness",
    "level": 1,
    "effects": [
      {
        "effect_type": "damage_bonus",
        "value": 5.0,
        "duration": null
      }
    ]
  },
  "materials_consumed": [
    {
      "item_id": 7001,
      "item_name": "Enchantment Crystal",
      "quantity": 1,
      "value": 100
    }
  ],
  "success_chance": 0.85,
  "item_value_increase": 100,
  "previous_enchantment": null,
  "timestamp": 1640995200
}
```

#### `RepairItem`
Repair a damaged item.

**Request:**
```json
{
  "player_id": 12345,
  "item_instance_id": "550e8400-e29b-41d4-a716-446655440000",
  "repair_materials": [
    {
      "item_instance": {
        "definition_id": 4001,
        "instance_id": "550e8400-e29b-41d4-a716-446655440011",
        "stack": 2
      },
      "quantity": 2
    }
  ],
  "use_currency": 50
}
```

**Success Response Event:** `item_repaired`
```json
{
  "player_id": 12345,
  "item_instance_id": "550e8400-e29b-41d4-a716-446655440000",
  "repair_result": {
    "durability_restored": 25,
    "durability_before": 60,
    "durability_after": 85,
    "durability_before_percent": 60.0,
    "durability_after_percent": 85.0,
    "materials_consumed": [
      {
        "item_id": 4001,
        "item_name": "Iron Ore",
        "quantity": 2,
        "repair_value": 10
      }
    ],
    "currency_spent": 50,
    "total_cost": 120,
    "repair_type": "Mixed",
    "item_was_equipped": true,
    "repair_quality": "Good"
  },
  "timestamp": 1640995200
}
```

---

## 🎯 Event System

The inventory system emits comprehensive events for integration with other game systems. All events follow a consistent format and include timestamps and relevant context.

### Event Categories

#### Core Inventory Events
| Event | Description | When Emitted |
|-------|-------------|--------------|
| `item_picked_up` | Item successfully added to inventory | After successful pickup |
| `item_dropped` | Item removed and dropped in world | After successful drop |
| `item_transferred` | Item moved between players | After successful transfer |
| `item_consumed` | Item used/consumed | After consumption |
| `inventory_cleared` | Inventory wiped | After clearing operation |
| `inventory_updated` | General inventory state change | After any modification |
| `pickup_failed` | Failed to add item | When pickup fails |
| `drop_failed` | Failed to drop item | When drop fails |

#### Equipment Events
| Event | Description | When Emitted |
|-------|-------------|--------------|
| `item_equipped` | Item equipped to slot | After successful equip |
| `item_unequipped` | Item removed from slot | After successful unequip |
| `equip_failed` | Failed to equip item | When equip fails |
| `unequip_failed` | Failed to unequip item | When unequip fails |

#### Trading Events  
| Event | Description | When Emitted |
|-------|-------------|--------------|
| `trade_created` | New trade offer created | After trade creation |
| `trade_completed` | Trade successfully finished | After trade completion |
| `trade_declined` | Trade offer rejected | When trade declined |
| `trade_cancelled` | Trade offer cancelled | When trade cancelled |
| `trade_modified` | Trade offer changed | When offer modified |

### Integration Events

The system also emits events specifically for integration with other game systems:

**World System Integration:**
- `item_world_drop` - Notifies world system of dropped items
- `container_access_request` - Validates container access distance

**Stats System Integration:**
- `equipment_changed` - Updates player stats when equipment changes
- `enchantment_effect_applied` - Applies enchantment effects to player

**Skills System Integration:**
- `experience_gained` - Awards experience for crafting/enchanting
- `skill_check_required` - Validates skill requirements

**Notification System Integration:**
- `trade_request` - Notifies players of incoming trade offers
- `inventory_backup_created` - Informs about backup creation

## ⚙️ Configuration

### System Configuration

```rust
let config = InventoryConfig {
    // Basic settings
    default_inventory_size: 27,
    max_player_inventories: 5,
    
    // Feature toggles
    enable_weight_system: true,
    enable_durability: true,
    allow_item_stacking: true,
    auto_stack_items: true,
    
    // Economic settings
    currency_items: vec![5001], // Gold Coin
    trade_timeout_seconds: 300,
    
    // World interaction
    container_access_distance: 10.0,
    
    // Custom rules
    custom_rules: HashMap::new(),
};
```

### Runtime Configuration Updates

```json
{
  "slot_count": 36,
  "inventory_count": 8,
  "enable_weight_system": true,
  "enable_durability": true,
  "auto_stack_items": true
}
```

## 📋 Error Handling

### Error Types

The system defines comprehensive error types for different failure scenarios:

```rust
pub enum InventoryError {
    PlayerNotFound(PlayerId),
    ItemNotFound(u64),
    InsufficientItems { needed: u32, available: u32 },
    InventoryFull,
    InvalidSlot(u32),
    ItemNotStackable,
    WeightLimitExceeded,
    ItemNotTradeable,
    ContainerNotFound(String),
    AccessDenied,
    TradeNotFound(String),
    RecipeNotFound(String),
    DurabilityTooLow,
    Custom(String),
}
```

### Error Response Format

All errors are emitted as events with consistent structure:

```json
{
  "operation": "pickup_item",
  "player_id": 12345,
  "error_type": "InventoryFull",
  "message": "Player inventory is full",
  "context": {
    "item_id": 1001,
    "attempted_amount": 5,
    "available_slots": 0
  },
  "timestamp": 1640995200
}
```

### Common Error Scenarios

| Error | Cause | Resolution |
|-------|-------|------------|
| `InventoryFull` | No available slots | Clear inventory or expand slots |
| `InsufficientItems` | Not enough items for operation | Acquire more items |
| `WeightLimitExceeded` | Item too heavy for inventory | Remove heavy items or increase limit |
| `AccessDenied` | Permission failure | Verify player permissions |
| `ItemNotTradeable` | Item cannot be traded | Check item definition |

## 🔗 Integration Guide

### Basic Integration

```rust
// Initialize the system
let inventory_system = InventorySystem::new();

// Register event handlers
inventory_system.register_handlers(event_system).await?;

// Listen for events
event_system.on_plugin("InventorySystem", "item_picked_up", |event| {
    // Handle pickup event
    let data: serde_json::Value = event;
    println!("Item picked up: {}", data);
    Ok(())
}).await?;
```

### Advanced Integration Example

```rust
// Custom inventory validation
event_system.on_plugin("InventorySystem", "pickup_failed", |event| {
    let error_data: serde_json::Value = event;
    
    if error_data["error_type"] == "InventoryFull" {
        // Auto-sell oldest items to make space
        auto_sell_old_items(error_data["player_id"].as_u64().unwrap()).await?;
        
        // Retry the pickup
        retry_pickup_operation(error_data).await?;
    }
    
    Ok(())
}).await?;

// Stats system integration
event_system.on_plugin("InventorySystem", "equipment_changed", |event| {
    let equip_data: serde_json::Value = event;
    
    // Update player combat stats
    update_combat_stats(
        equip_data["player_id"].as_u64().unwrap(),
        &equip_data["equipped_item"]
    ).await?;
    
    Ok(())
}).await?;
```

### UI Integration

```rust
// Inventory UI updates
event_system.on_plugin("InventorySystem", "inventory_updated", |event| {
    let update_data: serde_json::Value = event;
    
    // Refresh UI for affected player
    send_ui_update(
        update_data["player_id"].as_u64().unwrap(),
        "inventory_refresh",
        update_data
    ).await?;
    
    Ok(())
}).await?;

// Real-time search results
event_system.on_plugin("InventorySystem", "search_results", |event| {
    let search_data: serde_json::Value = event;
    
    // Send search results to client
    send_search_results_to_client(
        search_data["player_id"].as_u64().unwrap(),
        search_data["results"]
    ).await?;
    
    Ok(())
}).await?;
```

## 🚀 Performance & Scaling

### Optimization Features

- **Efficient Data Structures** - Uses HashMap and IndexMap for O(1) lookups
- **Lazy Loading** - Item definitions loaded on demand
- **Event Batching** - Multiple operations can be batched
- **Memory Management** - Automatic cleanup of expired effects and trades
- **Rate Limiting** - Prevents abuse and ensures system stability

### Scaling Recommendations

- **Database Integration** - Store persistent data in external database
- **Caching** - Use Redis or similar for frequently accessed data
- **Sharding** - Distribute players across multiple inventory instances
- **Monitoring** - Track system statistics and performance metrics

### Performance Metrics

```json
{
  "operations_per_second": 1000,
  "average_response_time_ms": 15,
  "memory_usage_mb": 256,
  "active_players": 500,
  "items_in_circulation": 50000,
  "cache_hit_rate": 0.95
}
```

## 🧪 Testing

### Unit Test Example

```rust
#[tokio::test]
async fn test_pickup_item() {
    let inventory_system = InventorySystem::new();
    let event_system = create_test_event_system();
    
    // Test successful pickup
    let pickup_request = PickupItemRequest {
        id: 12345,
        item_count: 5,
        item_id: 1001,
        inventory_name: Some("general".to_string()),
        target_slot: None,
    };
    
    pickup_item_handler(
        &inventory_system.players,
        &inventory_system.player_count,
        &inventory_system.item_definitions,
        &inventory_system.config,
        &event_system,
        pickup_request,
    );
    
    // Verify event was emitted
    assert_event_emitted(&event_system, "item_picked_up").await;
}
```

## 📊 Monitoring & Analytics

### System Statistics

The system provides comprehensive statistics for monitoring:

```json
{
  "online_players": 150,
  "total_items_in_circulation": 25000,
  "active_trades": 12,
  "containers_created": 500,
  "items_crafted_today": 350,
  "items_enchanted_today": 45,
  "items_repaired_today": 128,
  "total_item_definitions": 200,
  "total_crafting_recipes": 85,
  "average_inventory_utilization": 75.5,
  "most_popular_items": [
    "Health Potion",
    "Iron Sword", 
    "Gold Coin"
  ],
  "system_health": "Excellent"
}
```

### Health Checks

```rust
// Get system health
event_system.emit_plugin(
    "InventorySystem",
    "GetSystemStats",
    &serde_json::json!({})
).await?;

// Response: system_statistics event with health data
```

## 🐛 Troubleshooting

### Common Issues

**Items not stacking properly:**
- Check `auto_stack_items` configuration
- Verify items have same enchantments and custom data
- Ensure stack limits are not exceeded

**Trade failures:**
- Verify both players are online
- Check item tradability settings  
- Ensure items still exist in inventory

**Container access denied:**
- Check distance from container
- Verify permission settings
- Ensure container still exists

**Performance issues:**
- Monitor system statistics
- Check for memory leaks in active effects
- Review rate limiting settings

### Debug Events

Enable debug events for troubleshooting:

```json
{
  "enable_debug_events": true,
  "debug_level": "verbose",
  "log_all_operations": true
}
```

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass
5. Submit a pull request

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 📞 Support

- **Documentation**: [GitHub Wiki](https://github.com/your-repo/inventory-system/wiki)
- **Issues**: [GitHub Issues](https://github.com/your-repo/inventory-system/issues)
- **Discord**: [Community Server](https://discord.gg/your-server)
- **Email**: support@your-domain.com