use crate::types::*;

pub fn check_item_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    events: &Arc<EventSystem>,
    event: CheckItemRequest,
) {
    let result = check_player_has_item(
        players,
        item_definitions,
        event.id,
        event.item_id as u64,
        event.required_amount,
        event.inventory_name.clone(),
        event.include_equipped,
    );

    match result {
        Ok(check_result) => {
            info!(
                "🔍 Player {:?} {} {} of item {} (required: {}, available: {})",
                event.id,
                if check_result.has_item { "has" } else { "doesn't have" },
                check_result.available_amount,
                event.item_id,
                event.required_amount,
                check_result.available_amount
            );

            // Emit comprehensive check result
            let _ = events.emit_plugin(
                "InventorySystem",
                "item_check_result",
                &serde_json::json!({
                    "player_id": event.id,
                    "item_id": event.item_id,
                    "required_amount": event.required_amount,
                    "available_amount": check_result.available_amount,
                    "has_item": check_result.has_item,
                    "locations": check_result.locations,
                    "inventory_checked": event.inventory_name.unwrap_or("all".to_string()),
                    "include_equipped": event.include_equipped,
                    "total_weight": check_result.total_weight,
                    "total_value": check_result.total_value,
                    "timestamp": current_timestamp()
                }),
            );
        }
        Err(e) => {
            info!("❌ Failed to check item for player {:?}: {}", event.id, e);

            let _ = events.emit_plugin(
                "InventorySystem",
                "item_check_failed",
                &serde_json::json!({
                    "player_id": event.id,
                    "item_id": event.item_id,
                    "reason": e.to_string(),
                    "timestamp": current_timestamp()
                }),
            );
        }
    }
}

#[derive(Debug, Clone)]
pub struct ItemCheckResult {
    pub has_item: bool,
    pub available_amount: u32,
    pub locations: Vec<ItemLocation>,
    pub total_weight: f32,
    pub total_value: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ItemLocation {
    pub location_type: LocationType,
    pub location_name: String,
    pub slot_id: Option<u32>,
    pub amount: u32,
    pub item_instance_id: String,
    pub condition: Option<ItemCondition>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum LocationType {
    Inventory,
    Equipment,
    Container,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum ItemCondition {
    Perfect,
    Excellent,
    Good,
    Fair,
    Poor,
    Broken,
}

fn check_player_has_item(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    item_id: u64,
    required_amount: u32,
    inventory_name: Option<String>,
    include_equipped: bool,
) -> Result<ItemCheckResult, InventoryError> {
    let players_guard = players.lock().unwrap();
    let players_map = players_guard.as_ref()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;
    
    let player = players_map.get(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let item_def = {
        let defs_guard = item_definitions.lock().unwrap();
        defs_guard.get(&item_id).cloned()
            .ok_or(InventoryError::ItemNotFound(item_id))?
    };

    let mut total_amount = 0;
    let mut locations = Vec::new();
    let mut total_weight = 0.0;
    let mut total_value = 0;

    // Check specific inventory or all inventories
    let inventories_to_check: Vec<_> = if let Some(ref inv_name) = inventory_name {
        if let Some(inventory) = player.inventories.inventories.get(inv_name) {
            vec![(inv_name.clone(), inventory)]
        } else {
            return Err(InventoryError::Custom(format!("Inventory '{}' not found", inv_name)));
        }
    } else {
        player.inventories.inventories.iter()
            .map(|(name, inv)| (name.clone(), inv))
            .collect()
    };

    // Check inventories
    for (inv_name, inventory) in inventories_to_check {
        for (slot_id, slot) in &inventory.slots {
            if let Some(ref item_instance) = slot.item {
                if item_instance.definition_id == item_id {
                    total_amount += item_instance.stack;
                    total_weight += item_def.weight * item_instance.stack as f32;
                    total_value += item_def.value * item_instance.stack;

                    let condition = calculate_item_condition(item_instance, &item_def);

                    locations.push(ItemLocation {
                        location_type: LocationType::Inventory,
                        location_name: inv_name.clone(),
                        slot_id: Some(*slot_id),
                        amount: item_instance.stack,
                        item_instance_id: item_instance.instance_id.clone(),
                        condition: Some(condition),
                    });
                }
            }
        }
    }

    // Check equipped items if requested
    if include_equipped {
        let equipment_checks = [
            ("helmet", &player.inventories.equipped_items.helmet),
            ("chest", &player.inventories.equipped_items.chest),
            ("legs", &player.inventories.equipped_items.legs),
            ("boots", &player.inventories.equipped_items.boots),
            ("gloves", &player.inventories.equipped_items.gloves),
            ("weapon_main", &player.inventories.equipped_items.weapon_main),
            ("weapon_off", &player.inventories.equipped_items.weapon_off),
            ("ring_1", &player.inventories.equipped_items.ring_1),
            ("ring_2", &player.inventories.equipped_items.ring_2),
            ("necklace", &player.inventories.equipped_items.necklace),
        ];

        for (slot_name, equipped_item) in equipment_checks {
            if let Some(ref item_instance) = equipped_item {
                if item_instance.definition_id == item_id {
                    total_amount += item_instance.stack;
                    total_weight += item_def.weight * item_instance.stack as f32;
                    total_value += item_def.value * item_instance.stack;

                    let condition = calculate_item_condition(item_instance, &item_def);

                    locations.push(ItemLocation {
                        location_type: LocationType::Equipment,
                        location_name: slot_name.to_string(),
                        slot_id: None,
                        amount: item_instance.stack,
                        item_instance_id: item_instance.instance_id.clone(),
                        condition: Some(condition),
                    });
                }
            }
        }

        // Check custom equipment slots
        for (slot_name, item_instance) in &player.inventories.equipped_items.custom_slots {
            if item_instance.definition_id == item_id {
                total_amount += item_instance.stack;
                total_weight += item_def.weight * item_instance.stack as f32;
                total_value += item_def.value * item_instance.stack;

                let condition = calculate_item_condition(item_instance, &item_def);

                locations.push(ItemLocation {
                    location_type: LocationType::Equipment,
                    location_name: slot_name.clone(),
                    slot_id: None,
                    amount: item_instance.stack,
                    item_instance_id: item_instance.instance_id.clone(),
                    condition: Some(condition),
                });
            }
        }
    }

    let has_item = total_amount >= required_amount;

    Ok(ItemCheckResult {
        has_item,
        available_amount: total_amount,
        locations,
        total_weight,
        total_value,
    })
}

fn calculate_item_condition(item_instance: &ItemInstance, item_def: &ItemDefinition) -> ItemCondition {
    if let (Some(current_durability), Some(max_durability)) = (item_instance.durability, item_def.durability_max) {
        let condition_ratio = current_durability as f32 / max_durability as f32;
        
        match condition_ratio {
            ratio if ratio >= 0.9 => ItemCondition::Perfect,
            ratio if ratio >= 0.75 => ItemCondition::Excellent,
            ratio if ratio >= 0.5 => ItemCondition::Good,
            ratio if ratio >= 0.25 => ItemCondition::Fair,
            ratio if ratio > 0.0 => ItemCondition::Poor,
            _ => ItemCondition::Broken,
        }
    } else {
        ItemCondition::Perfect
    }
}