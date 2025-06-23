use crate::handlers::inventory_validation::*;
use crate::types::*;

/// Creates a new item instance with proper initialization
pub fn create_item_instance(
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    item_id: u64,
    stack: u32,
    bound_to_player: Option<PlayerId>,
) -> Result<ItemInstance, InventoryError> {
    let item_def = validate_item_definition(item_definitions, item_id)?;

    // Validate stack size
    if stack > item_def.max_stack {
        return Err(InventoryError::Custom(format!(
            "Stack size {} exceeds maximum {} for item {}",
            stack, item_def.max_stack, item_def.name
        )));
    }

    Ok(ItemInstance {
        definition_id: item_id,
        instance_id: uuid::Uuid::new_v4().to_string(),
        stack,
        durability: item_def.durability_max,
        enchantments: Vec::new(),
        bound_to_player,
        acquired_timestamp: current_timestamp(),
        custom_data: HashMap::new(),
    })
}

/// Finds the best slot for an item in an inventory
pub fn find_best_slot_for_item(
    inventory: &Inventory,
    item_def: &ItemDefinition,
    item_instance: &ItemInstance,
    config: &InventoryConfig,
) -> Option<u32> {
    // Try to stack with existing items first if auto-stacking is enabled
    if config.auto_stack_items {
        for (slot_id, slot) in &inventory.slots {
            if let Some(ref existing_item) = slot.item {
                if can_stack_items_together(existing_item, item_instance, item_def) {
                    let available_space = item_def.max_stack - existing_item.stack;
                    if available_space >= item_instance.stack {
                        return Some(*slot_id);
                    }
                }
            }
        }
    }

    // Find first empty slot
    inventory
        .slots
        .iter()
        .find(|(_, slot)| slot.item.is_none() && !slot.locked)
        .map(|(slot_id, _)| *slot_id)
}

/// Checks if two items can be stacked together
pub fn can_stack_items_together(
    existing_item: &ItemInstance,
    new_item: &ItemInstance,
    item_def: &ItemDefinition,
) -> bool {
    // Must be same item type
    if existing_item.definition_id != new_item.definition_id {
        return false;
    }

    // Must be stackable
    if item_def.max_stack <= 1 {
        return false;
    }

    // Must have same durability
    if existing_item.durability != new_item.durability {
        return false;
    }

    // Must have same enchantments
    if existing_item.enchantments != new_item.enchantments {
        return false;
    }

    // Must have same binding
    if existing_item.bound_to_player != new_item.bound_to_player {
        return false;
    }

    // Must have same custom data
    if existing_item.custom_data != new_item.custom_data {
        return false;
    }

    true
}

/// Splits an item stack into multiple stacks
pub fn split_item_stack(
    item_instance: &ItemInstance,
    split_amount: u32,
) -> Result<(ItemInstance, ItemInstance), InventoryError> {
    if split_amount >= item_instance.stack {
        return Err(InventoryError::Custom(
            "Cannot split more items than available in stack".to_string(),
        ));
    }

    if split_amount == 0 {
        return Err(InventoryError::Custom(
            "Cannot split zero items".to_string(),
        ));
    }

    let mut original = item_instance.clone();
    original.stack -= split_amount;

    let mut split = item_instance.clone();
    split.instance_id = uuid::Uuid::new_v4().to_string();
    split.stack = split_amount;
    split.acquired_timestamp = current_timestamp();

    Ok((original, split))
}

/// Merges two item stacks together
pub fn merge_item_stacks(
    item1: &ItemInstance,
    item2: &ItemInstance,
    item_def: &ItemDefinition,
) -> Result<ItemInstance, InventoryError> {
    if !can_stack_items_together(item1, item2, item_def) {
        return Err(InventoryError::Custom(
            "Items cannot be stacked together".to_string(),
        ));
    }

    let total_stack = item1.stack + item2.stack;
    if total_stack > item_def.max_stack {
        return Err(InventoryError::Custom(format!(
            "Merged stack size {} would exceed maximum {}",
            total_stack, item_def.max_stack
        )));
    }

    let mut merged = item1.clone();
    merged.stack = total_stack;
    // Keep the earlier acquisition timestamp
    merged.acquired_timestamp = std::cmp::min(item1.acquired_timestamp, item2.acquired_timestamp);

    Ok(merged)
}

/// Damages an item's durability
pub fn damage_item_durability(
    item_instance: &mut ItemInstance,
    damage_amount: u32,
) -> Result<bool, InventoryError> {
    if let Some(ref mut durability) = item_instance.durability {
        *durability = durability.saturating_sub(damage_amount);
        Ok(*durability == 0) // Returns true if item is broken
    } else {
        Ok(false) // Item doesn't have durability
    }
}

/// Repairs an item's durability
pub fn repair_item_durability(
    item_instance: &mut ItemInstance,
    item_def: &ItemDefinition,
    repair_amount: Option<u32>,
) -> Result<u32, InventoryError> {
    if let (Some(ref mut durability), Some(max_durability)) =
        (&mut item_instance.durability, item_def.durability_max)
    {
        let old_durability = *durability;
        *durability = if let Some(amount) = repair_amount {
            std::cmp::min(*durability + amount, max_durability)
        } else {
            max_durability // Full repair
        };

        Ok(*durability - old_durability) // Return amount repaired
    } else {
        Err(InventoryError::Custom(
            "Item does not have durability".to_string(),
        ))
    }
}

/// Adds an enchantment to an item
pub fn add_enchantment_to_item(
    item_instance: &mut ItemInstance,
    enchantment: Enchantment,
    allow_overwrite: bool,
) -> Result<(), InventoryError> {
    // Check if enchantment already exists
    if let Some(existing_index) = item_instance
        .enchantments
        .iter()
        .position(|e| e.enchantment_id == enchantment.enchantment_id)
    {
        if allow_overwrite {
            item_instance.enchantments[existing_index] = enchantment;
        } else {
            return Err(InventoryError::Custom(format!(
                "Enchantment '{}' already exists on item",
                enchantment.name
            )));
        }
    } else {
        item_instance.enchantments.push(enchantment);
    }

    Ok(())
}

/// Removes an enchantment from an item
pub fn remove_enchantment_from_item(
    item_instance: &mut ItemInstance,
    enchantment_id: &str,
) -> Result<Enchantment, InventoryError> {
    if let Some(index) = item_instance
        .enchantments
        .iter()
        .position(|e| e.enchantment_id == enchantment_id)
    {
        Ok(item_instance.enchantments.remove(index))
    } else {
        Err(InventoryError::Custom(format!(
            "Enchantment '{}' not found on item",
            enchantment_id
        )))
    }
}

/// Calculates the total value of an item including enchantments
pub fn calculate_total_item_value(item_instance: &ItemInstance, item_def: &ItemDefinition) -> u32 {
    let base_value = item_def.value * item_instance.stack;
    let enchantment_value: u32 = item_instance
        .enchantments
        .iter()
        .map(|enchantment| {
            // Simple calculation: level * 100 per enchantment
            enchantment.level * 100
        })
        .sum();

    base_value + enchantment_value
}

/// Gets the effective weight of an item
pub fn calculate_item_weight(item_instance: &ItemInstance, item_def: &ItemDefinition) -> f32 {
    item_def.weight * item_instance.stack as f32
}

/// Binds an item to a player (makes it non-tradeable to others)
pub fn bind_item_to_player(
    item_instance: &mut ItemInstance,
    player_id: PlayerId,
) -> Result<(), InventoryError> {
    if item_instance.bound_to_player.is_some() {
        return Err(InventoryError::Custom("Item is already bound".to_string()));
    }

    item_instance.bound_to_player = Some(player_id);
    Ok(())
}

/// Unbinds an item from a player (if allowed)
pub fn unbind_item_from_player(
    item_instance: &mut ItemInstance,
    requesting_player: PlayerId,
) -> Result<(), InventoryError> {
    match item_instance.bound_to_player {
        Some(bound_player) if bound_player == requesting_player => {
            item_instance.bound_to_player = None;
            Ok(())
        }
        Some(_) => Err(InventoryError::AccessDenied),
        None => Err(InventoryError::Custom("Item is not bound".to_string())),
    }
}

/// Sets custom data on an item
pub fn set_item_custom_data(
    item_instance: &mut ItemInstance,
    key: String,
    value: serde_json::Value,
) -> Result<(), InventoryError> {
    item_instance.custom_data.insert(key, value);
    Ok(())
}

/// Gets custom data from an item
pub fn get_item_custom_data<'a>(
    item_instance: &'a ItemInstance,
    key: &str,
) -> Option<&'a serde_json::Value> {
    item_instance.custom_data.get(key)
}

/// Creates a copy of an item instance for duplication/cloning
pub fn clone_item_instance(original: &ItemInstance, new_stack: Option<u32>) -> ItemInstance {
    let mut cloned = original.clone();
    cloned.instance_id = uuid::Uuid::new_v4().to_string();
    cloned.acquired_timestamp = current_timestamp();

    if let Some(stack) = new_stack {
        cloned.stack = stack;
    }

    cloned
}

/// Validates if an item instance is still valid
pub fn validate_item_instance(
    item_instance: &ItemInstance,
    item_definitions: &HashMap<u64, ItemDefinition>,
) -> Result<(), InventoryError> {
    let item_def = item_definitions
        .get(&item_instance.definition_id)
        .ok_or(InventoryError::ItemNotFound(item_instance.definition_id))?;

    // Check stack size
    if item_instance.stack > item_def.max_stack {
        return Err(InventoryError::Custom(format!(
            "Invalid stack size: {} > {}",
            item_instance.stack, item_def.max_stack
        )));
    }

    if item_instance.stack == 0 {
        return Err(InventoryError::Custom(
            "Item stack cannot be zero".to_string(),
        ));
    }

    // Check durability
    if let (Some(current_durability), Some(max_durability)) =
        (item_instance.durability, item_def.durability_max)
    {
        if current_durability > max_durability {
            return Err(InventoryError::Custom(format!(
                "Invalid durability: {} > {}",
                current_durability, max_durability
            )));
        }
    } else if item_instance.durability.is_some() && item_def.durability_max.is_none() {
        return Err(InventoryError::Custom(
            "Item has durability but definition doesn't support it".to_string(),
        ));
    }

    Ok(())
}

/// Finds all instances of a specific item in a player's inventories
pub fn find_item_instances(
    player: &Player,
    item_id: u64,
    inventory_name: Option<&str>,
) -> Vec<(String, u32, ItemInstance)> {
    let mut results = Vec::new();

    let inventories_to_search: Vec<_> = if let Some(inv_name) = inventory_name {
        if let Some(inventory) = player.inventories.inventories.get(inv_name) {
            vec![(inv_name.to_string(), inventory)]
        } else {
            vec![]
        }
    } else {
        player
            .inventories
            .inventories
            .iter()
            .map(|(name, inv)| (name.clone(), inv))
            .collect()
    };

    for (inv_name, inventory) in inventories_to_search {
        for (slot_id, slot) in &inventory.slots {
            if let Some(ref item_instance) = slot.item {
                if item_instance.definition_id == item_id {
                    results.push((inv_name.clone(), *slot_id, item_instance.clone()));
                }
            }
        }
    }

    results
}

/// Optimizes inventory by stacking compatible items
pub fn optimize_inventory_stacking(
    inventory: &mut Inventory,
    item_definitions: &HashMap<u64, ItemDefinition>,
) -> Result<u32, InventoryError> {
    let mut optimizations_made = 0;
    let mut items_to_merge: Vec<(u32, u32)> = Vec::new(); // (slot1, slot2) pairs

    // Find items that can be stacked together
    let slot_ids: Vec<_> = inventory.slots.keys().cloned().collect();

    for i in 0..slot_ids.len() {
        for j in (i + 1)..slot_ids.len() {
            let slot_id1 = slot_ids[i];
            let slot_id2 = slot_ids[j];

            let can_merge = {
                let slot1 = &inventory.slots[&slot_id1];
                let slot2 = &inventory.slots[&slot_id2];

                if let (Some(ref item1), Some(ref item2)) = (&slot1.item, &slot2.item) {
                    if let Some(item_def) = item_definitions.get(&item1.definition_id) {
                        can_stack_items_together(item1, item2, item_def)
                            && item1.stack + item2.stack <= item_def.max_stack
                    } else {
                        false
                    }
                } else {
                    false
                }
            };

            if can_merge {
                items_to_merge.push((slot_id1, slot_id2));
            }
        }
    }

    // Perform merging
    for (slot_id1, slot_id2) in items_to_merge {
        let (item1, item2, item_def) = {
            let slot1 = &inventory.slots[&slot_id1];
            let slot2 = &inventory.slots[&slot_id2];

            if let (Some(ref item1), Some(ref item2)) = (&slot1.item, &slot2.item) {
                if let Some(item_def) = item_definitions.get(&item1.definition_id) {
                    (item1.clone(), item2.clone(), item_def.clone())
                } else {
                    continue;
                }
            } else {
                continue;
            }
        };

        if let Ok(merged_item) = merge_item_stacks(&item1, &item2, &item_def) {
            inventory.slots.get_mut(&slot_id1).unwrap().item = Some(merged_item);
            inventory.slots.get_mut(&slot_id2).unwrap().item = None;
            optimizations_made += 1;
        }
    }

    if optimizations_made > 0 {
        inventory.last_modified = current_timestamp();
    }

    Ok(optimizations_made)
}

/// Creates an item definition for runtime item creation
pub fn create_runtime_item_definition(
    id: u64,
    name: String,
    description: String,
    category: ItemCategory,
    rarity: ItemRarity,
    max_stack: u32,
    weight: f32,
    tradeable: bool,
    consumable: bool,
    value: u32,
) -> ItemDefinition {
    ItemDefinition {
        id,
        name,
        description,
        category,
        rarity,
        max_stack,
        weight,
        size: (1, 1), // Default size
        tradeable,
        consumable,
        durability_max: None,
        icon_path: "default_icon.png".to_string(),
        value,
        level_requirement: None,
        custom_properties: HashMap::new(),
    }
}
