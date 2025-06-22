use crate::types::*;
use crate::handlers::inventory_validation::*;
use crate::handlers::item_management::*;

pub fn drop_item_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    events: &Arc<EventSystem>,
    event: DropItemRequest,
) {
    let result = remove_item_from_player(
        players,
        item_definitions,
        event.id,
        event.item_id as u64,
        event.item_count,
        event.inventory_name.clone(),
        event.slot_id,
    );

    match result {
        Ok(removed_items) => {
            println!(
                "📤 Player {:?} dropped {} of item {} from inventory '{}'",
                event.id, 
                event.item_count, 
                event.item_id,
                event.inventory_name.as_ref().unwrap_or(&"general".to_string())
            );

            let position = event.position.clone();
            for item_instance in &removed_items {
                // Emit item dropped event
                let _ = events.emit_plugin(
                    "InventorySystem",
                    "item_dropped",
                    &serde_json::json!({
                        "player_id": event.id,
                        "item_id": event.item_id,
                        "amount": item_instance.stack,
                        "item_instance_id": item_instance.instance_id,
                        "inventory_name": event.inventory_name.as_ref().unwrap_or(&"general".to_string()),
                        "slot_id": event.slot_id,
                        "position": position.clone(),
                        "reason": "drop",
                        "timestamp": current_timestamp()
                    }),
                );

                // Create world drop event for other systems to handle
                let _ = events.emit_plugin(
                    "WorldSystem",
                    "item_world_drop",
                    &serde_json::json!({
                        "item_instance": item_instance,
                        "position": position.clone().unwrap_or_else(|| WorldPosition {
                            x: 0.0,
                            y: 0.0,
                            z: 0.0,
                            world: "default".to_string()
                        }),
                        "dropped_by": event.id,
                        "timestamp": current_timestamp()
                    }),
                );
            }

            // Emit inventory updated event
            let _ = events.emit_plugin(
                "InventorySystem",
                "inventory_updated",
                &serde_json::json!({
                    "player_id": event.id,
                    "inventory_name": event.inventory_name.as_ref().unwrap_or(&"general".to_string()),
                    "action": "item_removed",
                    "timestamp": current_timestamp()
                }),
            );
        }
        Err(e) => {
            println!(
                "❌ Player {:?} failed to drop item {}: {}",
                event.id, event.item_id, e
            );

            // Emit failure event
            let _ = events.emit_plugin(
                "InventorySystem",
                "drop_failed",
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

fn remove_item_from_player(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    item_id: u64,
    amount: u32,
    inventory_name: Option<String>,
    slot_id: Option<u32>,
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

    if let Some(target_slot) = slot_id {
        // Remove from specific slot
        if let Some(slot) = inventory.slots.get_mut(&target_slot) {
            if let Some(ref mut item_instance) = slot.item {
                if item_instance.definition_id == item_id {
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
                } else {
                    return Err(InventoryError::ItemNotFound(item_id));
                }
            } else {
                return Err(InventoryError::ItemNotFound(item_id));
            }
        } else {
            return Err(InventoryError::InvalidSlot(target_slot));
        }
    } else {
        // Remove from any slots with this item
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
