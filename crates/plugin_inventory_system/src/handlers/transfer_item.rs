use crate::types::*;
use crate::handlers::inventory_validation::*;
use crate::handlers::item_management::*;

pub fn transfer_item_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    events: &Arc<EventSystem>,
    event: TransferItemRequest,
) {
    let result = transfer_item_between_players(
        players,
        item_definitions,
        event.from_player,
        event.to_player,
        event.item_id as u64,
        event.amount,
        event.from_inventory.clone(),
        event.to_inventory.clone(),
    );

    match result {
        Ok(transfer_result) => {
            info!(
                "🔄 Transferred {} x{} from {:?} to {:?} (from: {}, to: {})",
                transfer_result.item_name,
                event.amount,
                event.from_player,
                event.to_player,
                event.from_inventory.as_ref().unwrap_or(&"general".to_string()),
                event.to_inventory.as_ref().unwrap_or(&"general".to_string())
            );

            // Emit transfer success event
            let _ = events.emit_plugin(
                "InventorySystem",
                "item_transferred",
                &serde_json::json!({
                    "from_player": event.from_player,
                    "to_player": event.to_player,
                    "item_id": event.item_id,
                    "amount": event.amount,
                    "item_name": transfer_result.item_name,
                    "from_inventory": event.from_inventory.clone().unwrap_or("general".to_string()),
                    "to_inventory": event.clone().to_inventory.unwrap_or("general".to_string()),
                    "transferred_instances": transfer_result.transferred_instances,
                    "transaction_id": transfer_result.transaction_id,
                    "timestamp": current_timestamp()
                }),
            );

            // Emit inventory updated events for both players
            let _ = events.emit_plugin(
                "InventorySystem",
                "inventory_updated",
                &serde_json::json!({
                    "player_id": event.from_player,
                    "inventory_name": event.from_inventory.clone().unwrap_or("general".to_string()),
                    "action": "item_transferred_out",
                    "timestamp": current_timestamp()
                }),
            );

            let _ = events.emit_plugin(
                "InventorySystem",
                "inventory_updated",
                &serde_json::json!({
                    "player_id": event.to_player,
                    "inventory_name": event.to_inventory.unwrap_or("general".to_string()),
                    "action": "item_transferred_in",
                    "timestamp": current_timestamp()
                }),
            );

            // Notification for receiving player
            let _ = events.emit_plugin(
                "NotificationSystem",
                "item_received",
                &serde_json::json!({
                    "target_player": event.to_player,
                    "from_player": event.from_player,
                    "item_name": transfer_result.item_name,
                    "amount": event.amount,
                    "message": format!("You received {} {} from player {:?}", event.amount, transfer_result.item_name, event.from_player)
                }),
            );
        }
        Err(e) => {
            info!(
                "❌ Failed to transfer {} of item {} from {:?} to {:?}: {}",
                event.amount, event.item_id, event.from_player, event.to_player, e
            );

            let _ = events.emit_plugin(
                "InventorySystem",
                "transfer_failed",
                &serde_json::json!({
                    "from_player": event.from_player,
                    "to_player": event.to_player,
                    "item_id": event.item_id,
                    "amount": event.amount,
                    "reason": e.to_string(),
                    "timestamp": current_timestamp()
                }),
            );
        }
    }
}

#[derive(Debug, Clone)]
pub struct TransferResult {
    pub item_name: String,
    pub transferred_instances: Vec<String>, // instance IDs
    pub transaction_id: String,
}

fn transfer_item_between_players(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    from_player: PlayerId,
    to_player: PlayerId,
    item_id: u64,
    amount: u32,
    from_inventory: Option<String>,
    to_inventory: Option<String>,
) -> Result<TransferResult, InventoryError> {
    // Validate both players exist
    {
        let players_guard = players.lock().unwrap();
        let players_map = players_guard.as_ref()
            .ok_or(InventoryError::PlayerNotFound(from_player))?;
        
        players_map.get(&from_player)
            .ok_or(InventoryError::PlayerNotFound(from_player))?;
        players_map.get(&to_player)
            .ok_or(InventoryError::PlayerNotFound(to_player))?;
    }

    // Get item definition for validation
    let item_def = {
        let defs_guard = item_definitions.lock().unwrap();
        defs_guard.get(&item_id).cloned()
            .ok_or(InventoryError::ItemNotFound(item_id))?
    };

    // Check if item is tradeable
    if !item_def.tradeable {
        return Err(InventoryError::ItemNotTradeable);
    }

    // Validate source player has enough items
    validate_player_has_item_amount(
        players,
        from_player,
        item_id,
        amount,
        from_inventory.clone(),
    )?;

    // Validate destination has space
    validate_player_has_inventory_space(
        players,
        item_definitions,
        to_player,
        &[(item_id, amount)],
        to_inventory.clone(),
    )?;

    // Execute the transfer
    let transaction_id = uuid::Uuid::new_v4().to_string();
    
    // Remove from source player
    let removed_items = remove_items_from_player(
        players,
        item_definitions,
        from_player,
        item_id,
        amount,
        from_inventory,
    )?;

    // Add to destination player
    let transferred_instances = add_items_to_player(
        players,
        item_definitions,
        to_player,
        &removed_items,
        to_inventory,
    )?;

    Ok(TransferResult {
        item_name: item_def.name,
        transferred_instances,
        transaction_id,
    })
}

fn validate_player_has_item_amount(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_id: PlayerId,
    item_id: u64,
    amount: u32,
    inventory_name: Option<String>,
) -> Result<(), InventoryError> {
    let players_guard = players.lock().unwrap();
    let players_map = players_guard.as_ref()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;
    
    let player = players_map.get(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let inventory_name = inventory_name.unwrap_or_else(|| "general".to_string());
    let inventory = player.inventories.inventories
        .get(&inventory_name)
        .ok_or_else(|| InventoryError::Custom(format!("Inventory '{}' not found", inventory_name)))?;

    let available_amount: u32 = inventory.slots.values()
        .filter_map(|slot| {
            if let Some(ref item) = slot.item {
                if item.definition_id == item_id {
                    Some(item.stack)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .sum();

    if available_amount < amount {
        return Err(InventoryError::InsufficientItems {
            needed: amount,
            available: available_amount,
        });
    }

    Ok(())
}

fn validate_player_has_inventory_space(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    items: &[(u64, u32)], // (item_id, amount)
    inventory_name: Option<String>,
) -> Result<(), InventoryError> {
    let players_guard = players.lock().unwrap();
    let players_map = players_guard.as_ref()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;
    
    let player = players_map.get(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let inventory_name = inventory_name.unwrap_or_else(|| "general".to_string());
    let inventory = player.inventories.inventories
        .get(&inventory_name)
        .ok_or_else(|| InventoryError::Custom(format!("Inventory '{}' not found", inventory_name)))?;

    let empty_slots = inventory.slots.values()
        .filter(|slot| slot.item.is_none() && !slot.locked)
        .count();

    // Check weight constraints
    if let Some(max_weight) = inventory.constraints.max_weight {
        let defs_guard = item_definitions.lock().unwrap();
        let additional_weight: f32 = items.iter()
            .map(|(item_id, amount)| {
                if let Some(item_def) = defs_guard.get(item_id) {
                    item_def.weight * (*amount as f32)
                } else {
                    0.0
                }
            })
            .sum();

        if inventory.current_weight + additional_weight > max_weight {
            return Err(InventoryError::WeightLimitExceeded);
        }
    }

    // Simple check: ensure we have at least as many empty slots as unique items
    let unique_items = items.len();
    if empty_slots < unique_items {
        return Err(InventoryError::InventoryFull);
    }

    Ok(())
}

fn remove_items_from_player(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    item_id: u64,
    amount: u32,
    inventory_name: Option<String>,
) -> Result<Vec<ItemInstance>, InventoryError> {
    let mut players_guard = players.lock().unwrap();
    let players_map = players_guard.as_mut()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;
    
    let player = players_map.get_mut(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let inventory_name = inventory_name.unwrap_or_else(|| "general".to_string());
    let inventory = player.inventories.inventories
        .get_mut(&inventory_name)
        .ok_or_else(|| InventoryError::Custom(format!("Inventory '{}' not found", inventory_name)))?;

    let item_def = {
        let defs_guard = item_definitions.lock().unwrap();
        defs_guard.get(&item_id).cloned()
            .ok_or(InventoryError::ItemNotFound(item_id))?
    };

    let mut removed_items = Vec::new();
    let mut remaining_to_remove = amount;

    // Find slots with this item and remove from them
    let mut slots_to_process: Vec<_> = inventory.slots.iter()
        .filter_map(|(slot_id, slot)| {
            if let Some(ref item) = slot.item {
                if item.definition_id == item_id {
                    Some(*slot_id)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    // Sort by stack size (remove from larger stacks first)
    slots_to_process.sort_by(|&a, &b| {
        let stack_a = inventory.slots[&a].item.as_ref().unwrap().stack;
        let stack_b = inventory.slots[&b].item.as_ref().unwrap().stack;
        stack_b.cmp(&stack_a)
    });

    for slot_id in slots_to_process {
        if remaining_to_remove == 0 { break; }

        if let Some(slot) = inventory.slots.get_mut(&slot_id) {
            if let Some(ref mut item_instance) = slot.item {
                let remove_amount = std::cmp::min(remaining_to_remove, item_instance.stack);
                
                if remove_amount == item_instance.stack {
                    // Remove entire stack
                    let removed_item = slot.item.take().unwrap();
                    inventory.current_weight -= item_def.weight * removed_item.stack as f32;
                    removed_items.push(removed_item);
                } else {
                    // Partial removal
                    item_instance.stack -= remove_amount;
                    inventory.current_weight -= item_def.weight * remove_amount as f32;
                    
                    let mut partial_item = item_instance.clone();
                    partial_item.stack = remove_amount;
                    partial_item.instance_id = uuid::Uuid::new_v4().to_string();
                    removed_items.push(partial_item);
                }
                remaining_to_remove -= remove_amount;
            }
        }
    }

    if remaining_to_remove > 0 {
        return Err(InventoryError::InsufficientItems {
            needed: amount,
            available: amount - remaining_to_remove,
        });
    }

    inventory.last_modified = current_timestamp();
    player.last_activity = current_timestamp();

    Ok(removed_items)
}

fn add_items_to_player(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    items: &[ItemInstance],
    inventory_name: Option<String>,
) -> Result<Vec<String>, InventoryError> {
    let mut players_guard = players.lock().unwrap();
    let players_map = players_guard.as_mut()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;
    
    let player = players_map.get_mut(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let inventory_name = inventory_name.unwrap_or_else(|| "general".to_string());
    let inventory = player.inventories.inventories
        .get_mut(&inventory_name)
        .ok_or_else(|| InventoryError::Custom(format!("Inventory '{}' not found", inventory_name)))?;

    let mut transferred_instances = Vec::new();
    let defs_guard = item_definitions.lock().unwrap();

    for item_instance in items {
        // Create new instance for the receiving player
        let mut new_instance = item_instance.clone();
        new_instance.instance_id = uuid::Uuid::new_v4().to_string();
        new_instance.acquired_timestamp = current_timestamp();
        
        // Clear any player binding for transfer
        new_instance.bound_to_player = None;

        // Find empty slot
        let empty_slot = inventory.slots.iter()
            .find(|(_, slot)| slot.item.is_none() && !slot.locked)
            .map(|(slot_id, _)| *slot_id)
            .ok_or(InventoryError::InventoryFull)?;

        // Update weight
        if let Some(item_def) = defs_guard.get(&item_instance.definition_id) {
            inventory.current_weight += item_def.weight * item_instance.stack as f32;
        }

        transferred_instances.push(new_instance.instance_id.clone());
        inventory.slots.get_mut(&empty_slot).unwrap().item = Some(new_instance);
    }

    inventory.last_modified = current_timestamp();
    player.last_activity = current_timestamp();

    Ok(transferred_instances)
}