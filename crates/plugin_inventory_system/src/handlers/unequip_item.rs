use crate::types::*;
use crate::handlers::equip_item::get_equipped_item_mut;

pub fn unequip_item_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    events: &Arc<EventSystem>,
    event: UnequipItemRequest,
) {
    let result = unequip_item_for_player(
        players,
        event.id,
        &event.equipment_slot,
        event.to_inventory.clone(),
    );

    match result {
        Ok(unequipped_item) => {
            println!(
                "🛡️ Player {:?} unequipped {} from slot '{}'",
                event.id, unequipped_item.instance_id, event.equipment_slot
            );

            // Emit unequipped event
            let _ = events.emit_plugin(
                "InventorySystem",
                "item_unequipped",
                &serde_json::json!({
                    "player_id": event.id,
                    "item_instance_id": unequipped_item.instance_id,
                    "equipment_slot": event.equipment_slot,
                    "to_inventory": event.to_inventory.unwrap_or("general".to_string()),
                    "reason": "manual",
                    "timestamp": current_timestamp()
                }),
            );

            // Remove item effects
            remove_equipment_effects(players, &unequipped_item, &event.equipment_slot);

            // Update player stats event
            let _ = events.emit_plugin(
                "StatsSystem",
                "equipment_changed",
                &serde_json::json!({
                    "player_id": event.id,
                    "unequipped_item": unequipped_item,
                    "equipment_slot": event.equipment_slot,
                    "action": "unequipped"
                }),
            );
        }
        Err(e) => {
            println!(
                "❌ Player {:?} failed to unequip from slot '{}': {}",
                event.id, event.equipment_slot, e
            );

            let _ = events.emit_plugin(
                "InventorySystem",
                "unequip_failed",
                &serde_json::json!({
                    "player_id": event.id,
                    "equipment_slot": event.equipment_slot,
                    "reason": e.to_string(),
                    "timestamp": current_timestamp()
                }),
            );
        }
    }
}

fn unequip_item_for_player(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_id: PlayerId,
    equipment_slot: &str,
    to_inventory: Option<String>,
) -> Result<ItemInstance, InventoryError> {
    let mut players_guard = players.lock().unwrap();
    let players_map = players_guard.as_mut()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;
    
    let player = players_map.get_mut(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    // Get currently equipped item
    let equipped_item_slot = get_equipped_item_mut(&mut player.inventories.equipped_items, equipment_slot)?;
    let equipped_item = equipped_item_slot.take()
        .ok_or_else(|| InventoryError::Custom(format!("No item equipped in slot '{}'", equipment_slot)))?;

    // Find target inventory
    let to_inventory = to_inventory.unwrap_or_else(|| "general".to_string());
    let inventory = player.inventories.inventories
        .get_mut(&to_inventory)
        .ok_or_else(|| InventoryError::Custom(format!("Inventory '{}' not found", to_inventory)))?;

    // Find empty slot in inventory
    let empty_slot = inventory.slots.iter()
        .find(|(_, slot)| slot.item.is_none() && !slot.locked)
        .map(|(slot_id, _)| *slot_id)
        .ok_or(InventoryError::InventoryFull)?;

    // Place item in inventory
    inventory.slots.get_mut(&empty_slot).unwrap().item = Some(equipped_item.clone());
    inventory.last_modified = current_timestamp();
    player.last_activity = current_timestamp();

    Ok(equipped_item)
}

fn remove_equipment_effects(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item: &ItemInstance,
    _equipment_slot: &str,
) {
    // Remove enchantment effects
    for enchantment in &item.enchantments {
        for effect in &enchantment.effects {
            println!(
                "📊 Removing effect '{}' with value {} from enchantment '{}'",
                effect.effect_type, effect.value, enchantment.name
            );
        }
    }

    // Remove from player's active effects
    let mut players_guard = players.lock().unwrap();
    if let Some(ref mut players_map) = *players_guard {
        if let Some(player) = players_map.get_mut(&item.bound_to_player.unwrap_or_default()) {
            player.inventories.active_effects.retain(|effect| {
                effect.source_item != item.definition_id
            });
        }
    }
}