use crate::types::*;

pub fn equip_item_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    events: &Arc<EventSystem>,
    event: EquipItemRequest,
) {
    let from_inventory_name = event.from_inventory.clone();
    let result = equip_item_for_player(
        players,
        item_definitions,
        event.id,
        &event.item_instance_id,
        &event.equipment_slot,
        event.from_inventory,
    );

    match result {
        Ok((equipped_item, unequipped_item)) => {
            println!(
                "⚔️ Player {:?} equipped {} to slot '{}'",
                event.id, equipped_item.instance_id, event.equipment_slot
            );

            // Emit equipped event
            let _ = events.emit_plugin(
                "InventorySystem",
                "item_equipped",
                &serde_json::json!({
                    "player_id": event.id,
                    "item_instance_id": equipped_item.instance_id,
                    "equipment_slot": event.equipment_slot,
                    "from_inventory": from_inventory_name.clone().unwrap_or("general".to_string()),
                    "replaced_item": unequipped_item.as_ref().map(|item| &item.instance_id),
                    "timestamp": current_timestamp()
                }),
            );

            // If an item was unequipped, emit that event too
            if let Some(unequipped) = unequipped_item {
                let _ = events.emit_plugin(
                    "InventorySystem",
                    "item_unequipped",
                    &serde_json::json!({
                        "player_id": event.id,
                        "item_instance_id": unequipped.instance_id,
                        "equipment_slot": event.equipment_slot,
                        "to_inventory": from_inventory_name.unwrap_or("general".to_string()),
                        "reason": "replaced",
                        "timestamp": current_timestamp()
                    }),
                );
            }

            // Apply item effects
            apply_equipment_effects(players, &equipped_item, &event.equipment_slot, event.id);

            // Update player stats event
            let _ = events.emit_plugin(
                "StatsSystem",
                "equipment_changed",
                &serde_json::json!({
                    "player_id": event.id,
                    "equipped_item": equipped_item,
                    "equipment_slot": event.equipment_slot,
                    "action": "equipped"
                }),
            );
        }
        Err(e) => {
            println!(
                "❌ Player {:?} failed to equip item {}: {}",
                event.id, event.item_instance_id, e
            );

            let _ = events.emit_plugin(
                "InventorySystem",
                "equip_failed",
                &serde_json::json!({
                    "player_id": event.id,
                    "item_instance_id": event.item_instance_id,
                    "equipment_slot": event.equipment_slot,
                    "reason": e.to_string(),
                    "timestamp": current_timestamp()
                }),
            );
        }
    }
}

fn equip_item_for_player(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    item_instance_id: &str,
    equipment_slot: &str,
    from_inventory: Option<String>,
) -> Result<(ItemInstance, Option<ItemInstance>), InventoryError> {
    let mut players_guard = players.lock().unwrap();
    let players_map = players_guard
        .as_mut()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let player = players_map
        .get_mut(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    // Find and remove item from inventory
    let from_inventory = from_inventory.unwrap_or_else(|| "general".to_string());
    let inventory = player
        .inventories
        .inventories
        .get_mut(&from_inventory)
        .ok_or_else(|| {
            InventoryError::Custom(format!("Inventory '{}' not found", from_inventory))
        })?;

    let mut item_to_equip = None;
    let mut source_slot = None;

    // Find the item in inventory
    for (slot_id, slot) in inventory.slots.iter_mut() {
        if let Some(ref item) = slot.item {
            if item.instance_id == item_instance_id {
                item_to_equip = slot.item.take();
                source_slot = Some(*slot_id);
                break;
            }
        }
    }

    let item_to_equip = item_to_equip.ok_or_else(|| {
        InventoryError::Custom(format!("Item {} not found in inventory", item_instance_id))
    })?;

    // Validate item can be equipped in this slot
    let item_def = {
        let defs_guard = item_definitions.lock().unwrap();
        defs_guard
            .get(&item_to_equip.definition_id)
            .cloned()
            .ok_or(InventoryError::ItemNotFound(item_to_equip.definition_id))?
    };

    validate_equipment_slot(&item_def, equipment_slot)?;

    // Check if item is bound to another player
    if let Some(bound_player) = item_to_equip.bound_to_player {
        if bound_player != player_id {
            return Err(InventoryError::AccessDenied);
        }
    }

    // Get currently equipped item (if any)
    let currently_equipped =
        get_equipped_item_mut(&mut player.inventories.equipped_items, equipment_slot)?;
    let unequipped_item = currently_equipped.take();

    // If we unequipped something, try to put it in inventory
    if let Some(ref unequipped) = unequipped_item {
        if let Some(empty_slot) = find_empty_slot(&mut inventory.slots) {
            inventory.slots.get_mut(&empty_slot).unwrap().item = Some(unequipped.clone());
        } else {
            // No space in inventory, put the item back and fail
            *currently_equipped = Some(item_to_equip.clone());
            if let Some(slot_id) = source_slot {
                inventory.slots.get_mut(&slot_id).unwrap().item = Some(item_to_equip);
            }
            return Err(InventoryError::InventoryFull);
        }
    }

    // Equip the new item
    *currently_equipped = Some(item_to_equip.clone());

    inventory.last_modified = current_timestamp();
    player.last_activity = current_timestamp();

    Ok((item_to_equip, unequipped_item))
}

fn validate_equipment_slot(
    item_def: &ItemDefinition,
    equipment_slot: &str,
) -> Result<(), InventoryError> {
    let valid = match &item_def.category {
        ItemCategory::Armor(armor_type) => match equipment_slot {
            "helmet" => matches!(armor_type, ArmorType::Helmet),
            "chest" => matches!(armor_type, ArmorType::Chest),
            "legs" => matches!(armor_type, ArmorType::Legs),
            "boots" => matches!(armor_type, ArmorType::Boots),
            "gloves" => matches!(armor_type, ArmorType::Gloves),
            "shield" => matches!(armor_type, ArmorType::Shield),
            "ring_1" | "ring_2" => matches!(armor_type, ArmorType::Ring),
            "necklace" => matches!(armor_type, ArmorType::Necklace),
            _ => equipment_slot.starts_with("custom_"),
        },
        ItemCategory::Weapon(_) => matches!(equipment_slot, "weapon_main" | "weapon_off"),
        _ => equipment_slot.starts_with("custom_"),
    };

    if valid {
        Ok(())
    } else {
        Err(InventoryError::Custom(format!(
            "Item category {:?} cannot be equipped in slot '{}'",
            item_def.category, equipment_slot
        )))
    }
}

pub fn get_equipped_item_mut<'a>(
    equipment: &'a mut EquipmentSlots,
    slot: &str,
) -> Result<&'a mut Option<ItemInstance>, InventoryError> {
    match slot {
        "helmet" => Ok(&mut equipment.helmet),
        "chest" => Ok(&mut equipment.chest),
        "legs" => Ok(&mut equipment.legs),
        "boots" => Ok(&mut equipment.boots),
        "gloves" => Ok(&mut equipment.gloves),
        "weapon_main" => Ok(&mut equipment.weapon_main),
        "weapon_off" => Ok(&mut equipment.weapon_off),
        "ring_1" => Ok(&mut equipment.ring_1),
        "ring_2" => Ok(&mut equipment.ring_2),
        "necklace" => Ok(&mut equipment.necklace),
        custom_slot => {
            if custom_slot.starts_with("custom_") {
                // For custom slots, we need to handle the HashMap<String, ItemInstance> differently
                // Since custom_slots stores ItemInstance directly, not Option<ItemInstance>
                return Err(InventoryError::Custom(format!(
                    "Custom equipment slots not fully implemented yet: {}",
                    slot
                )));
            } else {
                Err(InventoryError::Custom(format!(
                    "Invalid equipment slot: {}",
                    slot
                )))
            }
        }
    }
}

fn find_empty_slot(slots: &mut HashMap<u32, InventorySlot>) -> Option<u32> {
    slots
        .iter()
        .find(|(_, slot)| slot.item.is_none() && !slot.locked)
        .map(|(slot_id, _)| *slot_id)
}

fn apply_equipment_effects(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item: &ItemInstance,
    _equipment_slot: &str,
    player_id: PlayerId,
) {
    // Apply enchantment effects
    for enchantment in &item.enchantments {
        for effect in &enchantment.effects {
            // This would integrate with a stats/effects system
            println!(
                "📊 Applying effect '{}' with value {} from enchantment '{}'",
                effect.effect_type, effect.value, enchantment.name
            );
        }
    }

    // Update player's active effects
    let mut players_guard = players.lock().unwrap();
    if let Some(ref mut players_map) = *players_guard {
        if let Some(player) = players_map.get_mut(&player_id) {
            for enchantment in &item.enchantments {
                for effect in &enchantment.effects {
                    player.inventories.active_effects.push(ItemEffect {
                        effect_id: uuid::Uuid::new_v4().to_string(),
                        source_item: item.definition_id,
                        effect_type: effect.effect_type.clone(),
                        value: effect.value,
                        duration: effect.duration.map(|d| current_timestamp() + d as u64),
                        applied_at: current_timestamp(),
                    });
                }
            }
        }
    }
}
