use crate::handlers::inventory_validation::*;
use crate::handlers::item_management::*;
use crate::types::*;

pub fn pickup_item_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_count: &Arc<Mutex<Option<u32>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    config: &Arc<Mutex<InventoryConfig>>,
    events: &Arc<EventSystem>,
    event: PickupItemRequest,
) {
    let result = add_item_to_player(
        players,
        player_count,
        item_definitions,
        config,
        event.id,
        event.item_id as u64,
        event.item_count,
        event.inventory_name.clone(),
        event.target_slot,
    );

    match result {
        Ok(item_instance) => {
            println!(
                "📦 Player {:?} picked up {} of item {} (instance: {})",
                event.id, event.item_count, event.item_id, item_instance.instance_id
            );

            // Get player info for detailed logging
            let players_guard = players.lock().unwrap();
            if let Some(ref players_map) = *players_guard {
                if let Some(player) = players_map.get(&event.id) {
                    let total_items: u32 = player
                        .inventories
                        .inventories
                        .values()
                        .map(|inv| inv.slots.len() as u32)
                        .sum();

                    println!(
                        "📊 Player {:?} now has {} total items across all inventories",
                        event.id, total_items
                    );
                }
            }

            // Emit comprehensive pickup event
            let _ = events.emit_plugin(
                "InventorySystem",
                "item_picked_up",
                &serde_json::json!({
                    "player_id": event.id,
                    "item_id": event.item_id,
                    "amount": event.item_count,
                    "item_instance_id": item_instance.instance_id,
                    "inventory_name": event.clone().inventory_name.unwrap_or("general".to_string()),
                    "target_slot": event.target_slot,
                    "source": "pickup",
                    "timestamp": current_timestamp()
                }),
            );

            // Emit inventory updated event
            let _ = events.emit_plugin(
                "InventorySystem",
                "inventory_updated",
                &serde_json::json!({
                    "player_id": event.id,
                    "inventory_name": event.inventory_name.unwrap_or("general".to_string()),
                    "action": "item_added",
                    "timestamp": current_timestamp()
                }),
            );
        }
        Err(e) => {
            println!(
                "❌ Failed to add item {} to player {:?}: {}",
                event.item_id, event.id, e
            );

            // Emit failure event
            let _ = events.emit_plugin(
                "InventorySystem",
                "pickup_failed",
                &serde_json::json!({
                    "player_id": event.id,
                    "item_id": event.item_id,
                    "amount": event.item_count,
                    "reason": e.to_string(),
                    "timestamp": current_timestamp()
                }),
            );
        }
    }
}

fn ensure_players_initialized(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_count: &Arc<Mutex<Option<u32>>>,
) {
    let mut players_guard = players.lock().unwrap();
    if players_guard.is_none() {
        *players_guard = Some(HashMap::new());
        *player_count.lock().unwrap() = Some(0);
    }
}

fn add_item_to_player(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_count: &Arc<Mutex<Option<u32>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    config: &Arc<Mutex<InventoryConfig>>,
    player_id: PlayerId,
    item_id: u64,
    amount: u32,
    inventory_name: Option<String>,
    target_slot: Option<u32>,
) -> Result<ItemInstance, InventoryError> {
    ensure_players_initialized(players, player_count);

    let item_def = {
        let defs_guard = item_definitions.lock().unwrap();
        defs_guard
            .get(&item_id)
            .cloned()
            .ok_or(InventoryError::ItemNotFound(item_id))?
    };

    let config_guard = config.lock().unwrap();
    let mut players_guard = players.lock().unwrap();
    let players_map = players_guard.as_mut().unwrap();

    // Get or create player
    let player = players_map.entry(player_id).or_insert_with(|| {
        let mut count_guard = player_count.lock().unwrap();
        *count_guard = Some(count_guard.unwrap_or(0) + 1);

        let mut general_inventory = Inventory {
            inventory_type: InventoryType::General,
            slots: HashMap::new(),
            constraints: InventoryConstraints {
                max_slots: Some(config_guard.default_inventory_size),
                max_weight: if config_guard.enable_weight_system {
                    Some(100.0)
                } else {
                    None
                },
                allowed_categories: None,
                grid_size: None,
                auto_sort: config_guard.auto_stack_items,
                allow_overflow: false,
            },
            current_weight: 0.0,
            last_modified: current_timestamp(),
        };

        // Initialize slots
        for i in 0..config_guard.default_inventory_size {
            general_inventory.slots.insert(
                i,
                InventorySlot {
                    slot_id: i,
                    item: None,
                    locked: false,
                },
            );
        }

        let mut inventories = HashMap::new();
        inventories.insert("general".to_string(), general_inventory);

        Player {
            id: player_id,
            inventories: PlayerInventories {
                player_id,
                inventories,
                equipped_items: EquipmentSlots {
                    helmet: None,
                    chest: None,
                    legs: None,
                    boots: None,
                    gloves: None,
                    weapon_main: None,
                    weapon_off: None,
                    ring_1: None,
                    ring_2: None,
                    necklace: None,
                    custom_slots: HashMap::new(),
                },
                active_effects: Vec::new(),
            },
            online: true,
            last_activity: current_timestamp(),
        }
    });

    let inventory_name = inventory_name.unwrap_or_else(|| "general".to_string());
    let inventory = player
        .inventories
        .inventories
        .get_mut(&inventory_name)
        .ok_or_else(|| {
            InventoryError::Custom(format!("Inventory '{}' not found", inventory_name))
        })?;

    // Check weight constraints
    if let Some(max_weight) = inventory.constraints.max_weight {
        let item_weight = item_def.weight * amount as f32;
        if inventory.current_weight + item_weight > max_weight {
            return Err(InventoryError::WeightLimitExceeded);
        }
    }

    // Create item instance
    let item_instance = ItemInstance {
        definition_id: item_id,
        instance_id: uuid::Uuid::new_v4().to_string(),
        stack: amount,
        durability: item_def.durability_max,
        enchantments: Vec::new(),
        bound_to_player: None,
        acquired_timestamp: current_timestamp(),
        custom_data: HashMap::new(),
    };

    // Try to stack with existing items if auto-stacking is enabled
    if config_guard.auto_stack_items && amount <= item_def.max_stack {
        for slot in inventory.slots.values_mut() {
            if let Some(ref mut existing_item) = slot.item {
                if existing_item.definition_id == item_id
                    && existing_item.stack + amount <= item_def.max_stack
                {
                    existing_item.stack += amount;
                    inventory.current_weight += item_def.weight * amount as f32;
                    inventory.last_modified = current_timestamp();
                    return Ok(item_instance);
                }
            }
        }
    }

    // Find available slot
    let slot_to_use = if let Some(target) = target_slot {
        if let Some(slot) = inventory.slots.get_mut(&target) {
            if slot.item.is_none() && !slot.locked {
                target
            } else {
                return Err(InventoryError::InvalidSlot(target));
            }
        } else {
            return Err(InventoryError::InvalidSlot(target));
        }
    } else {
        // Find first empty slot
        inventory
            .slots
            .iter()
            .find(|(_, slot)| slot.item.is_none() && !slot.locked)
            .map(|(slot_id, _)| *slot_id)
            .ok_or(InventoryError::InventoryFull)?
    };

    // Place item in slot
    if let Some(slot) = inventory.slots.get_mut(&slot_to_use) {
        slot.item = Some(item_instance.clone());
        inventory.current_weight += item_def.weight * amount as f32;
        inventory.last_modified = current_timestamp();
        player.last_activity = current_timestamp();
    }

    Ok(item_instance)
}
