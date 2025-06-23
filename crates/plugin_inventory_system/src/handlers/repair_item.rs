use crate::handlers::inventory_validation::*;
use crate::handlers::item_management::*;
use crate::types::*;

pub fn repair_item_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    events: &Arc<EventSystem>,
    event: RepairItemRequest,
) {
    let result = repair_player_item(
        players,
        item_definitions,
        event.player_id,
        &event.item_instance_id,
        event.repair_materials.clone(),
        event.use_currency,
    );

    match result {
        Ok(repair_result) => {
            println!(
                "🔧 Player {:?} repaired item {} for {} durability ({}% -> {}%)",
                event.player_id,
                event.item_instance_id,
                repair_result.durability_restored,
                repair_result.durability_before_percent,
                repair_result.durability_after_percent
            );

            // Emit repair success event
            let _ = events.emit_plugin(
                "InventorySystem",
                "item_repaired",
                &serde_json::json!({
                    "player_id": event.player_id,
                    "item_instance_id": event.item_instance_id,
                    "repair_result": repair_result,
                    "timestamp": current_timestamp()
                }),
            );

            // Update item stats if equipped
            if repair_result.item_was_equipped {
                let _ = events.emit_plugin(
                    "StatsSystem",
                    "equipped_item_repaired",
                    &serde_json::json!({
                        "player_id": event.player_id,
                        "item_instance_id": event.item_instance_id,
                        "durability_restored": repair_result.durability_restored
                    }),
                );
            }

            // Achievement tracking
            let _ = events.emit_plugin(
                "ProgressionSystem",
                "item_repair_completed",
                &serde_json::json!({
                    "player_id": event.player_id,
                    "repair_type": repair_result.repair_type,
                    "cost": repair_result.total_cost
                }),
            );
        }
        Err(e) => {
            println!(
                "❌ Repair failed for player {:?} on item {}: {}",
                event.player_id, event.item_instance_id, e
            );

            let _ = events.emit_plugin(
                "InventorySystem",
                "repair_failed",
                &serde_json::json!({
                    "player_id": event.player_id,
                    "item_instance_id": event.item_instance_id,
                    "reason": e.to_string(),
                    "timestamp": current_timestamp()
                }),
            );
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct RepairResult {
    pub durability_restored: u32,
    pub durability_before: u32,
    pub durability_after: u32,
    pub durability_before_percent: f32,
    pub durability_after_percent: f32,
    pub materials_consumed: Vec<MaterialConsumed>,
    pub currency_spent: u32,
    pub total_cost: u32,
    pub repair_type: RepairType,
    pub item_was_equipped: bool,
    pub repair_quality: RepairQuality,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct MaterialConsumed {
    pub item_id: u64,
    pub item_name: String,
    pub quantity: u32,
    pub repair_value: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum RepairType {
    Materials,
    Currency,
    Mixed,
    Free, // Special cases like magical repair
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum RepairQuality {
    Perfect, // 100% repair
    Good,    // 80-99% repair
    Fair,    // 60-79% repair
    Poor,    // 40-59% repair
    Minimal, // 20-39% repair
}

fn repair_player_item(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    item_instance_id: &str,
    repair_materials: Option<Vec<TradeItem>>,
    use_currency: Option<u32>,
) -> Result<RepairResult, InventoryError> {
    // Validate player and find item
    validate_player_access(players, player_id)?;

    let (item_location, item_was_equipped) =
        find_item_location_for_repair(players, player_id, item_instance_id)?;

    // Get item details and validate repairability
    let (item_def, current_durability, max_durability) =
        validate_item_repairable(players, item_definitions, player_id, item_instance_id)?;

    // Calculate repair parameters
    let repair_plan = calculate_repair_plan(
        &item_def,
        current_durability,
        max_durability,
        &repair_materials,
        use_currency,
    )?;

    // Validate player has required resources
    validate_repair_resources(players, item_definitions, player_id, &repair_plan)?;

    // Consume resources
    let materials_consumed =
        consume_repair_resources(players, item_definitions, player_id, &repair_plan)?;

    // Apply repair to item
    let durability_restored = apply_repair_to_item(
        players,
        player_id,
        item_instance_id,
        repair_plan.durability_to_restore,
        &item_location,
    )?;

    let durability_after = current_durability + durability_restored;
    let durability_before_percent = (current_durability as f32 / max_durability as f32) * 100.0;
    let durability_after_percent = (durability_after as f32 / max_durability as f32) * 100.0;

    let repair_quality =
        calculate_repair_quality(durability_restored, repair_plan.durability_to_restore);

    Ok(RepairResult {
        durability_restored,
        durability_before: current_durability,
        durability_after,
        durability_before_percent,
        durability_after_percent,
        materials_consumed,
        currency_spent: repair_plan.currency_cost,
        total_cost: repair_plan.total_cost,
        repair_type: repair_plan.repair_type,
        item_was_equipped,
        repair_quality,
    })
}

#[derive(Debug, Clone)]
enum ItemLocationForRepair {
    Inventory {
        inventory_name: String,
        slot_id: u32,
    },
    Equipment {
        slot_name: String,
    },
}

#[derive(Debug, Clone)]
struct RepairPlan {
    durability_to_restore: u32,
    materials_needed: Vec<MaterialNeeded>,
    currency_cost: u32,
    total_cost: u32,
    repair_type: RepairType,
    efficiency: f32, // 0.0 to 1.0
}

#[derive(Debug, Clone)]
struct MaterialNeeded {
    item_id: u64,
    quantity: u32,
    repair_value: u32,
}

fn find_item_location_for_repair(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_id: PlayerId,
    item_instance_id: &str,
) -> Result<(ItemLocationForRepair, bool), InventoryError> {
    let players_guard = players.lock().unwrap();
    let players_map = players_guard
        .as_ref()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let player = players_map
        .get(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    // Search in inventories
    for (inv_name, inventory) in &player.inventories.inventories {
        for (slot_id, slot) in &inventory.slots {
            if let Some(ref item) = slot.item {
                if item.instance_id == item_instance_id {
                    return Ok((
                        ItemLocationForRepair::Inventory {
                            inventory_name: inv_name.clone(),
                            slot_id: *slot_id,
                        },
                        false,
                    ));
                }
            }
        }
    }

    // Search in equipped items
    let equipment_checks = [
        ("helmet", &player.inventories.equipped_items.helmet),
        ("chest", &player.inventories.equipped_items.chest),
        ("legs", &player.inventories.equipped_items.legs),
        ("boots", &player.inventories.equipped_items.boots),
        ("gloves", &player.inventories.equipped_items.gloves),
        (
            "weapon_main",
            &player.inventories.equipped_items.weapon_main,
        ),
        ("weapon_off", &player.inventories.equipped_items.weapon_off),
        ("ring_1", &player.inventories.equipped_items.ring_1),
        ("ring_2", &player.inventories.equipped_items.ring_2),
        ("necklace", &player.inventories.equipped_items.necklace),
    ];

    for (slot_name, equipped_item) in equipment_checks {
        if let Some(ref item) = equipped_item {
            if item.instance_id == item_instance_id {
                return Ok((
                    ItemLocationForRepair::Equipment {
                        slot_name: slot_name.to_string(),
                    },
                    true,
                ));
            }
        }
    }

    Err(InventoryError::Custom(format!(
        "Item {} not found",
        item_instance_id
    )))
}

fn validate_item_repairable(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    item_instance_id: &str,
) -> Result<(ItemDefinition, u32, u32), InventoryError> {
    let players_guard = players.lock().unwrap();
    let players_map = players_guard
        .as_ref()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let player = players_map
        .get(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    // Find the item instance
    let item_instance = find_item_instance_in_player_for_repair(&player, item_instance_id)?;

    // Get item definition
    let item_def = {
        let defs_guard = item_definitions.lock().unwrap();
        defs_guard
            .get(&item_instance.definition_id)
            .cloned()
            .ok_or(InventoryError::ItemNotFound(item_instance.definition_id))?
    };

    // Check if item has durability
    let (current_durability, max_durability) =
        if let (Some(current), Some(max)) = (item_instance.durability, item_def.durability_max) {
            (current, max)
        } else {
            return Err(InventoryError::Custom(
                "Item cannot be repaired (no durability)".to_string(),
            ));
        };

    // Check if item needs repair
    if current_durability >= max_durability {
        return Err(InventoryError::Custom(
            "Item is already at full durability".to_string(),
        ));
    }

    // Check if item is completely broken
    if current_durability == 0 {
        // Some items might be unrepairable when completely broken
        if let Some(unrepairable_when_broken) =
            item_def.custom_properties.get("unrepairable_when_broken")
        {
            if unrepairable_when_broken.as_bool().unwrap_or(false) {
                return Err(InventoryError::Custom(
                    "Item is too damaged to repair".to_string(),
                ));
            }
        }
    }

    Ok((item_def, current_durability, max_durability))
}

fn find_item_instance_in_player_for_repair<'a>(
    player: &'a Player,
    item_instance_id: &'a str,
) -> Result<&'a ItemInstance, InventoryError> {
    // Search in inventories
    for inventory in player.inventories.inventories.values() {
        for slot in inventory.slots.values() {
            if let Some(ref item) = slot.item {
                if item.instance_id == item_instance_id {
                    return Ok(item);
                }
            }
        }
    }

    // Search in equipped items
    let all_equipped = vec![
        &player.inventories.equipped_items.helmet,
        &player.inventories.equipped_items.chest,
        &player.inventories.equipped_items.legs,
        &player.inventories.equipped_items.boots,
        &player.inventories.equipped_items.gloves,
        &player.inventories.equipped_items.weapon_main,
        &player.inventories.equipped_items.weapon_off,
        &player.inventories.equipped_items.ring_1,
        &player.inventories.equipped_items.ring_2,
        &player.inventories.equipped_items.necklace,
    ];

    for equipped_item in all_equipped {
        if let Some(ref item) = equipped_item {
            if item.instance_id == item_instance_id {
                return Ok(item);
            }
        }
    }

    Err(InventoryError::Custom(format!(
        "Item {} not found",
        item_instance_id
    )))
}

fn calculate_repair_plan(
    item_def: &ItemDefinition,
    current_durability: u32,
    max_durability: u32,
    repair_materials: &Option<Vec<TradeItem>>,
    use_currency: Option<u32>,
) -> Result<RepairPlan, InventoryError> {
    let durability_missing = max_durability - current_durability;

    let repair_type = match (repair_materials.as_ref(), use_currency) {
        (Some(_), Some(_)) => RepairType::Mixed,
        (Some(_), None) => RepairType::Materials,
        (None, Some(_)) => RepairType::Currency,
        (None, None) => {
            return Err(InventoryError::Custom(
                "No repair method specified".to_string(),
            ))
        }
    };

    let mut durability_to_restore = 0;
    let mut materials_needed = Vec::new();
    let mut currency_cost = 0;
    let mut efficiency = 1.0;

    // Calculate repair from materials
    if let Some(ref materials) = repair_materials {
        for material in materials {
            let repair_value =
                calculate_material_repair_value(item_def, material.item_instance.definition_id);
            let durability_from_material = repair_value * material.quantity;
            durability_to_restore += durability_from_material;

            materials_needed.push(MaterialNeeded {
                item_id: material.item_instance.definition_id,
                quantity: material.quantity,
                repair_value,
            });
        }
    }

    // Calculate repair from currency
    if let Some(currency_amount) = use_currency {
        let durability_per_coin = calculate_currency_repair_efficiency(item_def);
        let durability_from_currency = (currency_amount as f32 * durability_per_coin) as u32;
        durability_to_restore += durability_from_currency;
        currency_cost = currency_amount;
    }

    // Apply repair efficiency (materials might be less efficient)
    if matches!(repair_type, RepairType::Materials | RepairType::Mixed) {
        efficiency = calculate_repair_efficiency(item_def, &materials_needed);
        durability_to_restore = (durability_to_restore as f32 * efficiency) as u32;
    }

    // Cap at missing durability
    durability_to_restore = std::cmp::min(durability_to_restore, durability_missing);

    let total_cost = calculate_total_repair_cost(&materials_needed, currency_cost);

    Ok(RepairPlan {
        durability_to_restore,
        materials_needed,
        currency_cost,
        total_cost,
        repair_type,
        efficiency,
    })
}

fn calculate_material_repair_value(item_def: &ItemDefinition, material_item_id: u64) -> u32 {
    // Different materials have different repair values
    match material_item_id {
        4001 => 10, // Iron Ore - good for metal items
        _ => 5,     // Default repair value
    }
}

fn calculate_currency_repair_efficiency(item_def: &ItemDefinition) -> f32 {
    // Currency repair efficiency based on item value
    let base_efficiency = 0.5; // 1 coin = 0.5 durability
    let value_modifier = (item_def.value as f32).log10() * 0.1;
    base_efficiency + value_modifier
}

fn calculate_repair_efficiency(item_def: &ItemDefinition, materials: &[MaterialNeeded]) -> f32 {
    // Material repair efficiency based on item type and materials used
    let base_efficiency = 0.8; // 80% efficiency with materials

    // Bonus for using appropriate materials
    let material_bonus = materials
        .iter()
        .map(|material| {
            match (&item_def.category, material.item_id) {
                (ItemCategory::Weapon(_), 4001) => 0.1, // Iron ore for weapons
                (ItemCategory::Armor(_), 4001) => 0.1,  // Iron ore for armor
                (ItemCategory::Tool, 4001) => 0.15,     // Iron ore for tools
                _ => 0.0,
            }
        })
        .sum::<f32>();

    (base_efficiency + material_bonus).min(1.0)
}

fn calculate_total_repair_cost(materials: &[MaterialNeeded], currency_cost: u32) -> u32 {
    let material_cost: u32 = materials
        .iter()
        .map(|material| material.repair_value * material.quantity)
        .sum();

    material_cost + currency_cost
}

fn validate_repair_resources(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    repair_plan: &RepairPlan,
) -> Result<(), InventoryError> {
    let players_guard = players.lock().unwrap();
    let players_map = players_guard
        .as_ref()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let player = players_map
        .get(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    // Check materials
    for material_needed in &repair_plan.materials_needed {
        let available = count_item_in_player_inventories_by_id(&player, material_needed.item_id);
        if available < material_needed.quantity {
            return Err(InventoryError::InsufficientItems {
                needed: material_needed.quantity,
                available,
            });
        }
    }

    // Check currency (simplified - would check specific currency items)
    if repair_plan.currency_cost > 0 {
        // For demo, assume player always has enough currency
        println!("💰 Repair cost: {} coins", repair_plan.currency_cost);
    }

    Ok(())
}

fn consume_repair_resources(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    repair_plan: &RepairPlan,
) -> Result<Vec<MaterialConsumed>, InventoryError> {
    let mut materials_consumed = Vec::new();

    // Consume materials
    for material_needed in &repair_plan.materials_needed {
        // Remove materials from player inventory
        let removed_count = remove_item_from_player_inventories(
            players,
            player_id,
            material_needed.item_id,
            material_needed.quantity,
        )?;

        // Get material name
        let material_name = {
            let defs_guard = item_definitions.lock().unwrap();
            if let Some(def) = defs_guard.get(&material_needed.item_id) {
                def.name.clone()
            } else {
                format!("Material_{}", material_needed.item_id)
            }
        };

        materials_consumed.push(MaterialConsumed {
            item_id: material_needed.item_id,
            item_name: material_name,
            quantity: removed_count,
            repair_value: material_needed.repair_value,
        });
    }

    // Consume currency would be handled here
    if repair_plan.currency_cost > 0 {
        println!("💸 Spent {} coins for repair", repair_plan.currency_cost);
    }

    Ok(materials_consumed)
}

fn apply_repair_to_item(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_id: PlayerId,
    item_instance_id: &str,
    durability_to_restore: u32,
    item_location: &ItemLocationForRepair,
) -> Result<u32, InventoryError> {
    let mut players_guard = players.lock().unwrap();
    let players_map = players_guard
        .as_mut()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let player = players_map
        .get_mut(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    // Find and modify the item
    let item_instance = match item_location {
        ItemLocationForRepair::Inventory {
            inventory_name,
            slot_id,
        } => {
            let inventory = player
                .inventories
                .inventories
                .get_mut(inventory_name)
                .ok_or_else(|| {
                    InventoryError::Custom(format!("Inventory '{}' not found", inventory_name))
                })?;

            let slot = inventory
                .slots
                .get_mut(slot_id)
                .ok_or(InventoryError::InvalidSlot(*slot_id))?;

            slot.item
                .as_mut()
                .ok_or_else(|| InventoryError::Custom("Item not found in slot".to_string()))?
        }
        ItemLocationForRepair::Equipment { slot_name } => match slot_name.as_str() {
            "helmet" => player.inventories.equipped_items.helmet.as_mut(),
            "chest" => player.inventories.equipped_items.chest.as_mut(),
            "legs" => player.inventories.equipped_items.legs.as_mut(),
            "boots" => player.inventories.equipped_items.boots.as_mut(),
            "gloves" => player.inventories.equipped_items.gloves.as_mut(),
            "weapon_main" => player.inventories.equipped_items.weapon_main.as_mut(),
            "weapon_off" => player.inventories.equipped_items.weapon_off.as_mut(),
            "ring_1" => player.inventories.equipped_items.ring_1.as_mut(),
            "ring_2" => player.inventories.equipped_items.ring_2.as_mut(),
            "necklace" => player.inventories.equipped_items.necklace.as_mut(),
            custom_slot => player
                .inventories
                .equipped_items
                .custom_slots
                .get_mut(custom_slot),
        }
        .ok_or_else(|| InventoryError::Custom("Equipped item not found".to_string()))?,
    };

    // Apply repair
    if let Some(ref mut durability) = item_instance.durability {
        let old_durability = *durability;
        *durability += durability_to_restore;
        let actual_restored = *durability - old_durability;

        println!(
            "🔧 Repaired item: {} -> {} durability",
            old_durability, *durability
        );

        player.last_activity = current_timestamp();

        Ok(actual_restored)
    } else {
        Err(InventoryError::Custom(
            "Item has no durability to repair".to_string(),
        ))
    }
}

fn calculate_repair_quality(actual_restored: u32, planned_restored: u32) -> RepairQuality {
    if actual_restored == 0 {
        return RepairQuality::Minimal;
    }

    let efficiency_ratio = actual_restored as f32 / planned_restored as f32;

    match efficiency_ratio {
        ratio if ratio >= 0.95 => RepairQuality::Perfect,
        ratio if ratio >= 0.8 => RepairQuality::Good,
        ratio if ratio >= 0.6 => RepairQuality::Fair,
        ratio if ratio >= 0.4 => RepairQuality::Poor,
        _ => RepairQuality::Minimal,
    }
}

fn count_item_in_player_inventories_by_id(player: &Player, item_id: u64) -> u32 {
    player
        .inventories
        .inventories
        .values()
        .map(|inventory| {
            inventory
                .slots
                .values()
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
                .sum::<u32>()
        })
        .sum()
}

fn remove_item_from_player_inventories(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_id: PlayerId,
    item_id: u64,
    quantity: u32,
) -> Result<u32, InventoryError> {
    // Simplified implementation for demo
    println!("🔧 Removing {} of item {} for repair", quantity, item_id);
    Ok(quantity)
}

// Utility function for bulk repair
pub fn repair_all_equipped_items(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    max_cost: Option<u32>,
) -> Result<Vec<RepairResult>, InventoryError> {
    let mut repair_results = Vec::new();

    // This would iterate through all equipped items and repair them
    // For demo purposes, return empty list

    Ok(repair_results)
}
