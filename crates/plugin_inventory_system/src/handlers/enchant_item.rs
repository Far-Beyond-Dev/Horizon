use crate::types::*;
use crate::handlers::inventory_validation::*;
use crate::handlers::item_management::*;

pub fn enchant_item_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    events: &Arc<EventSystem>,
    event: EnchantItemRequest,
) {
    let result = enchant_player_item(
        players,
        item_definitions,
        event.player_id,
        &event.item_instance_id,
        event.enchantment.clone(),
        event.enchantment_materials,
    );

    match result {
        Ok(enchantment_result) => {
            println!(
                "✨ Player {:?} successfully enchanted item {} with '{}' (level {})",
                event.player_id,
                event.item_instance_id,
                enchantment_result.enchantment_applied.name,
                enchantment_result.enchantment_applied.level
            );

            // Emit enchantment success event
            let _ = events.emit_plugin(
                "InventorySystem",
                "item_enchanted",
                &serde_json::json!({
                    "player_id": event.player_id,
                    "item_instance_id": event.item_instance_id,
                    "enchantment": enchantment_result.enchantment_applied,
                    "materials_consumed": enchantment_result.materials_consumed,
                    "success_chance": enchantment_result.success_chance,
                    "item_value_increase": enchantment_result.value_increase,
                    "previous_enchantment": enchantment_result.replaced_enchantment,
                    "timestamp": current_timestamp()
                }),
            );

            // Apply enchantment effects if item is equipped
            if enchantment_result.item_was_equipped {
                let _ = events.emit_plugin(
                    "StatsSystem",
                    "enchantment_effect_applied",
                    &serde_json::json!({
                        "player_id": event.player_id,
                        "enchantment": enchantment_result.enchantment_applied,
                        "item_instance_id": event.item_instance_id
                    }),
                );
            }

            // Achievement/progression tracking
            let _ = events.emit_plugin(
                "ProgressionSystem",
                "enchantment_completed",
                &serde_json::json!({
                    "player_id": event.player_id,
                    "enchantment_type": enchantment_result.enchantment_applied.enchantment_id,
                    "enchantment_level": enchantment_result.enchantment_applied.level
                }),
            );
        }
        Err(e) => {
            println!(
                "❌ Enchantment failed for player {:?} on item {}: {}",
                event.player_id, event.item_instance_id, e
            );

            let _ = events.emit_plugin(
                "InventorySystem",
                "enchantment_failed",
                &serde_json::json!({
                    "player_id": event.player_id,
                    "item_instance_id": event.item_instance_id,
                    "enchantment_id": event.enchantment.clone().enchantment_id,
                    "reason": e.to_string(),
                    "timestamp": current_timestamp()
                }),
            );
        }
    }
}

#[derive(Debug, Clone)]
pub struct EnchantmentResult {
    pub enchantment_applied: Enchantment,
    pub replaced_enchantment: Option<Enchantment>,
    pub materials_consumed: Vec<MaterialConsumed>,
    pub success_chance: f32,
    pub value_increase: u32,
    pub item_was_equipped: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct MaterialConsumed {
    pub item_id: u64,
    pub item_name: String,
    pub quantity: u32,
    pub value: u32,
}

fn enchant_player_item(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    item_instance_id: &str,
    enchantment: Enchantment,
    enchantment_materials: Vec<TradeItem>,
) -> Result<EnchantmentResult, InventoryError> {
    // Validate player and find item
    validate_player_access(players, player_id)?;
    
    let (item_location, item_was_equipped) = find_item_location(players, player_id, item_instance_id)?;
    
    // Validate enchantment requirements
    validate_enchantment_requirements(
        players,
        item_definitions,
        player_id,
        item_instance_id,
        &enchantment,
        &enchantment_materials,
    )?;

    // Calculate success chance
    let success_chance = calculate_enchantment_success_chance(&enchantment, &enchantment_materials);
    
    // Determine if enchantment succeeds (for demo, always succeed)
    let enchantment_succeeds = true; // In real implementation: rand::random::<f32>() < success_chance

    if !enchantment_succeeds {
        // Consume materials but don't apply enchantment
        consume_enchantment_materials(players, item_definitions, player_id, &enchantment_materials)?;
        return Err(InventoryError::Custom("Enchantment failed due to chance".to_string()));
    }

    // Consume enchantment materials
    let materials_consumed = consume_enchantment_materials(
        players,
        item_definitions,
        player_id,
        &enchantment_materials,
    )?;

    // Apply enchantment to item
    let (replaced_enchantment, value_increase) = apply_enchantment_to_item(
        players,
        item_definitions,
        player_id,
        item_instance_id,
        enchantment.clone(),
        &item_location,
    )?;

    Ok(EnchantmentResult {
        enchantment_applied: enchantment,
        replaced_enchantment,
        materials_consumed,
        success_chance,
        value_increase,
        item_was_equipped,
    })
}

#[derive(Debug, Clone)]
enum ItemLocation {
    Inventory { inventory_name: String, slot_id: u32 },
    Equipment { slot_name: String },
}

fn find_item_location(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_id: PlayerId,
    item_instance_id: &str,
) -> Result<(ItemLocation, bool), InventoryError> {
    let players_guard = players.lock().unwrap();
    let players_map = players_guard.as_ref()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;
    
    let player = players_map.get(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    // Search in inventories
    for (inv_name, inventory) in &player.inventories.inventories {
        for (slot_id, slot) in &inventory.slots {
            if let Some(ref item) = slot.item {
                if item.instance_id == item_instance_id {
                    return Ok((ItemLocation::Inventory {
                        inventory_name: inv_name.clone(),
                        slot_id: *slot_id,
                    }, false));
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
        ("weapon_main", &player.inventories.equipped_items.weapon_main),
        ("weapon_off", &player.inventories.equipped_items.weapon_off),
        ("ring_1", &player.inventories.equipped_items.ring_1),
        ("ring_2", &player.inventories.equipped_items.ring_2),
        ("necklace", &player.inventories.equipped_items.necklace),
    ];

    for (slot_name, equipped_item) in equipment_checks {
        if let Some(ref item) = equipped_item {
            if item.instance_id == item_instance_id {
                return Ok((ItemLocation::Equipment {
                    slot_name: slot_name.to_string(),
                }, true));
            }
        }
    }

    // Check custom equipment slots
    for (slot_name, item) in &player.inventories.equipped_items.custom_slots {
        if item.instance_id == item_instance_id {
            return Ok((ItemLocation::Equipment {
                slot_name: slot_name.clone(),
            }, true));
        }
    }

    Err(InventoryError::Custom(format!("Item {} not found", item_instance_id)))
}

fn validate_enchantment_requirements(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    item_instance_id: &str,
    enchantment: &Enchantment,
    enchantment_materials: &[TradeItem],
) -> Result<(), InventoryError> {
    // Find the item to be enchanted
    let players_guard = players.lock().unwrap();
    let players_map = players_guard.as_ref()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;
    
    let player = players_map.get(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    // Find the item
    let item_instance = find_item_instance_in_player(&player, item_instance_id)?;
    
    // Get item definition
    let item_def = {
        let defs_guard = item_definitions.lock().unwrap();
        defs_guard.get(&item_instance.definition_id).cloned()
            .ok_or(InventoryError::ItemNotFound(item_instance.definition_id))?
    };

    // Validate item can be enchanted
    validate_item_enchantable(&item_def, &item_instance, enchantment)?;

    // Validate player has required materials
    for material in enchantment_materials {
        let available = count_item_in_player_inventories(&player, material.item_instance.definition_id);
        if available < material.quantity {
            return Err(InventoryError::InsufficientItems {
                needed: material.quantity,
                available,
            });
        }
    }

    // Validate enchantment level progression
    validate_enchantment_level_progression(&item_instance, enchantment)?;

    Ok(())
}

fn find_item_instance_in_player<'a>(
    player: &'a Player,
    item_instance_id: &str,
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

    // Check custom equipment
    for item in player.inventories.equipped_items.custom_slots.values() {
        if item.instance_id == item_instance_id {
            return Ok(item);
        }
    }

    Err(InventoryError::Custom(format!("Item {} not found", item_instance_id)))
}

fn validate_item_enchantable(
    item_def: &ItemDefinition,
    item_instance: &ItemInstance,
    enchantment: &Enchantment,
) -> Result<(), InventoryError> {
    // Check if item type can accept this enchantment
    match &enchantment.enchantment_id.as_str() {
        &"sharpness" | &"fire_aspect" | &"smite" => {
            if !matches!(item_def.category, ItemCategory::Weapon(_)) {
                return Err(InventoryError::Custom("This enchantment can only be applied to weapons".to_string()));
            }
        }
        &"protection" | &"fire_protection" | &"blast_protection" => {
            if !matches!(item_def.category, ItemCategory::Armor(_)) {
                return Err(InventoryError::Custom("This enchantment can only be applied to armor".to_string()));
            }
        }
        &"efficiency" | &"unbreaking" => {
            if !matches!(item_def.category, ItemCategory::Tool) {
                return Err(InventoryError::Custom("This enchantment can only be applied to tools".to_string()));
            }
        }
        _ => {} // Custom enchantments allowed on any item
    }

    // Check item durability
    if let Some(durability) = item_instance.durability {
        if durability == 0 {
            return Err(InventoryError::Custom("Cannot enchant broken items".to_string()));
        }
    }

    // Check if item already has maximum enchantments
    if item_instance.enchantments.len() >= 5 {
        return Err(InventoryError::Custom("Item has maximum number of enchantments".to_string()));
    }

    Ok(())
}

fn validate_enchantment_level_progression(
    item_instance: &ItemInstance,
    new_enchantment: &Enchantment,
) -> Result<(), InventoryError> {
    // Check if this enchantment already exists
    if let Some(existing) = item_instance.enchantments.iter()
        .find(|e| e.enchantment_id == new_enchantment.enchantment_id) {
        
        // Must be exactly one level higher
        if new_enchantment.level != existing.level + 1 {
            return Err(InventoryError::Custom(format!(
                "Enchantment level must progress from {} to {}, not {}",
                existing.level, existing.level + 1, new_enchantment.level
            )));
        }
    } else {
        // New enchantment must start at level 1
        if new_enchantment.level != 1 {
            return Err(InventoryError::Custom("New enchantments must start at level 1".to_string()));
        }
    }

    Ok(())
}

fn calculate_enchantment_success_chance(
    enchantment: &Enchantment,
    materials: &[TradeItem],
) -> f32 {
    let base_chance = 0.7; // 70% base success rate
    
    // Higher level enchantments are harder
    let level_penalty = (enchantment.level - 1) as f32 * 0.1;
    
    // More/better materials increase success rate
    let material_bonus = materials.len() as f32 * 0.05;
    
    let final_chance = (base_chance - level_penalty + material_bonus).clamp(0.1, 0.95);
    
    println!("🎲 Enchantment success chance: {:.1}%", final_chance * 100.0);
    final_chance
}

fn consume_enchantment_materials(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    materials: &[TradeItem],
) -> Result<Vec<MaterialConsumed>, InventoryError> {
    let mut consumed_materials = Vec::new();
    
    let item_defs_guard = item_definitions.lock().unwrap();
    
    for material in materials {
        // Remove items from player inventory
        let removed_items = remove_item_by_id_from_player_all_inventories(
            players,
            player_id,
            material.item_instance.definition_id,
            material.quantity,
        )?;
        
        // Get item name and value
        let (item_name, item_value) = if let Some(item_def) = item_defs_guard.get(&material.item_instance.definition_id) {
            (item_def.name.clone(), item_def.value)
        } else {
            (format!("Item_{}", material.item_instance.definition_id), 0)
        };
        
        consumed_materials.push(MaterialConsumed {
            item_id: material.item_instance.definition_id,
            item_name,
            quantity: material.quantity,
            value: item_value * material.quantity,
        });
    }
    
    Ok(consumed_materials)
}

fn apply_enchantment_to_item(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    item_instance_id: &str,
    enchantment: Enchantment,
    item_location: &ItemLocation,
) -> Result<(Option<Enchantment>, u32), InventoryError> {
    let mut players_guard = players.lock().unwrap();
    let players_map = players_guard.as_mut()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;
    
    let player = players_map.get_mut(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    // Find and modify the item
    let item_instance = match item_location {
        ItemLocation::Inventory { inventory_name, slot_id } => {
            let inventory = player.inventories.inventories
                .get_mut(inventory_name)
                .ok_or_else(|| InventoryError::Custom(format!("Inventory '{}' not found", inventory_name)))?;
            
            let slot = inventory.slots.get_mut(slot_id)
                .ok_or(InventoryError::InvalidSlot(*slot_id))?;
            
            slot.item.as_mut()
                .ok_or_else(|| InventoryError::Custom("Item not found in slot".to_string()))?
        }
        ItemLocation::Equipment { slot_name } => {
            match slot_name.as_str() {
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
                custom_slot => player.inventories.equipped_items.custom_slots.get_mut(custom_slot),
            }.ok_or_else(|| InventoryError::Custom("Equipped item not found".to_string()))?
        }
    };

    // Apply the enchantment
    let replaced_enchantment = if let Some(existing_index) = item_instance.enchantments.iter()
        .position(|e| e.enchantment_id == enchantment.enchantment_id) {
        
        let old_enchantment = item_instance.enchantments[existing_index].clone();
        item_instance.enchantments[existing_index] = enchantment;
        Some(old_enchantment)
    } else {
        item_instance.enchantments.push(enchantment);
        None
    };

    // Calculate value increase
    let value_increase = calculate_enchantment_value_bonus(&item_instance.enchantments);

    player.last_activity = current_timestamp();

    Ok((replaced_enchantment, value_increase))
}

fn calculate_enchantment_value_bonus(enchantments: &[Enchantment]) -> u32 {
    enchantments.iter()
        .map(|enchantment| {
            // Simple calculation: level * 100 per enchantment
            enchantment.level * 100
        })
        .sum()
}

fn count_item_in_player_inventories(player: &Player, item_id: u64) -> u32 {
    player.inventories.inventories.values()
        .map(|inventory| {
            inventory.slots.values()
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

fn remove_item_by_id_from_player_all_inventories(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_id: PlayerId,
    item_id: u64,
    amount: u32,
) -> Result<Vec<ItemInstance>, InventoryError> {
    // This is a simplified version - in reality would need to properly remove items
    // across all inventories while maintaining consistency
    
    println!("🔧 Removing {} of item {} from player {:?}", amount, item_id, player_id);
    
    // For demo purposes, return empty list
    Ok(Vec::new())
}

// Utility function to remove enchantment from item
pub fn remove_enchantment_from_player_item(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_id: PlayerId,
    item_instance_id: &str,
    enchantment_id: &str,
) -> Result<Enchantment, InventoryError> {
    let mut players_guard = players.lock().unwrap();
    let players_map = players_guard.as_mut()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;
    
    let player = players_map.get_mut(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    // Find the item and remove enchantment
    // This would need to search through all inventories and equipment
    // For demo purposes, simplified implementation
    
    Err(InventoryError::Custom("Enchantment removal not fully implemented".to_string()))
}