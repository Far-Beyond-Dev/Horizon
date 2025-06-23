use crate::types::*;

pub fn sort_inventory_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    events: &Arc<EventSystem>,
    event: SortInventoryRequest,
) {
    let result = sort_player_inventory(
        players,
        item_definitions,
        event.player_id,
        event.inventory_name.clone(),
        &event.sort_criteria,
        &event.sort_order,
    );

    match result {
        Ok(sort_result) => {
            println!(
                "📋 Sorted inventory '{}' for player {:?} by {:?} ({:?}): {} items reorganized",
                event
                    .inventory_name
                    .as_ref()
                    .unwrap_or(&"general".to_string()),
                event.player_id,
                event.sort_criteria,
                event.sort_order,
                sort_result.items_moved
            );

            // Emit sort completion event
            let _ = events.emit_plugin(
                "InventorySystem",
                "inventory_sorted",
                &serde_json::json!({
                    "player_id": event.player_id,
                    "inventory_name": event.clone().inventory_name.unwrap_or("general".to_string()),
                    "sort_criteria": format!("{:?}", event.sort_criteria),
                    "sort_order": format!("{:?}", event.sort_order),
                    "items_moved": sort_result.items_moved,
                    "execution_time_ms": sort_result.execution_time_ms,
                    "timestamp": current_timestamp()
                }),
            );

            // Emit inventory updated event
            let _ = events.emit_plugin(
                "InventorySystem",
                "inventory_updated",
                &serde_json::json!({
                    "player_id": event.player_id,
                    "inventory_name": event.inventory_name.unwrap_or("general".to_string()),
                    "action": "sorted",
                    "timestamp": current_timestamp()
                }),
            );
        }
        Err(e) => {
            println!(
                "❌ Failed to sort inventory for player {:?}: {}",
                event.player_id, e
            );

            let _ = events.emit_plugin(
                "InventorySystem",
                "sort_failed",
                &serde_json::json!({
                    "player_id": event.player_id,
                    "inventory_name": event.inventory_name,
                    "reason": e.to_string(),
                    "timestamp": current_timestamp()
                }),
            );
        }
    }
}

#[derive(Debug, Clone)]
pub struct SortResult {
    pub items_moved: u32,
    pub execution_time_ms: u64,
}

fn sort_player_inventory(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    inventory_name: Option<String>,
    sort_criteria: &SortCriteria,
    sort_order: &SortOrder,
) -> Result<SortResult, InventoryError> {
    let start_time = current_timestamp();

    let mut players_guard = players.lock().unwrap();
    let players_map = players_guard
        .as_mut()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let player = players_map
        .get_mut(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let inventory_name = inventory_name.unwrap_or_else(|| "general".to_string());
    let inventory = player
        .inventories
        .inventories
        .get_mut(&inventory_name)
        .ok_or_else(|| {
            InventoryError::Custom(format!("Inventory '{}' not found", inventory_name))
        })?;

    let item_defs_guard = item_definitions.lock().unwrap();

    // Collect all items with their slot information
    let mut items_with_slots: Vec<_> = inventory
        .slots
        .iter()
        .filter_map(|(slot_id, slot)| {
            if let Some(ref item) = slot.item {
                if let Some(item_def) = item_defs_guard.get(&item.definition_id) {
                    Some(SortableItem {
                        slot_id: *slot_id,
                        item_instance: item.clone(),
                        item_definition: item_def.clone(),
                    })
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    if items_with_slots.is_empty() {
        return Ok(SortResult {
            items_moved: 0,
            execution_time_ms: current_timestamp() - start_time,
        });
    }

    // Sort items based on criteria
    sort_items(&mut items_with_slots, sort_criteria, sort_order);

    // Stack similar items together if possible
    let stacked_items = stack_similar_items(items_with_slots);

    // Clear inventory slots and reorganize
    let items_moved = reorganize_inventory_slots(inventory, stacked_items)?;

    inventory.last_modified = current_timestamp();
    player.last_activity = current_timestamp();

    let execution_time_ms = current_timestamp() - start_time;

    Ok(SortResult {
        items_moved,
        execution_time_ms,
    })
}

#[derive(Debug, Clone)]
struct SortableItem {
    slot_id: u32,
    item_instance: ItemInstance,
    item_definition: ItemDefinition,
}

fn sort_items(items: &mut Vec<SortableItem>, sort_criteria: &SortCriteria, sort_order: &SortOrder) {
    items.sort_by(|a, b| {
        let comparison = match sort_criteria {
            SortCriteria::Name => a.item_definition.name.cmp(&b.item_definition.name),
            SortCriteria::Category => {
                let cat_order_a = get_category_order(&a.item_definition.category);
                let cat_order_b = get_category_order(&b.item_definition.category);
                cat_order_a
                    .cmp(&cat_order_b)
                    .then_with(|| a.item_definition.name.cmp(&b.item_definition.name))
            }
            SortCriteria::Rarity => {
                let rarity_order_a = get_rarity_order(&a.item_definition.rarity);
                let rarity_order_b = get_rarity_order(&b.item_definition.rarity);
                rarity_order_a
                    .cmp(&rarity_order_b)
                    .then_with(|| a.item_definition.name.cmp(&b.item_definition.name))
            }
            SortCriteria::Quantity => a
                .item_instance
                .stack
                .cmp(&b.item_instance.stack)
                .then_with(|| a.item_definition.name.cmp(&b.item_definition.name)),
            SortCriteria::Value => {
                let value_a = a.item_definition.value * a.item_instance.stack;
                let value_b = b.item_definition.value * b.item_instance.stack;
                value_a
                    .cmp(&value_b)
                    .then_with(|| a.item_definition.name.cmp(&b.item_definition.name))
            }
            SortCriteria::DateAcquired => a
                .item_instance
                .acquired_timestamp
                .cmp(&b.item_instance.acquired_timestamp)
                .then_with(|| a.item_definition.name.cmp(&b.item_definition.name)),
            SortCriteria::Weight => {
                let weight_a = a.item_definition.weight * a.item_instance.stack as f32;
                let weight_b = b.item_definition.weight * b.item_instance.stack as f32;
                weight_a
                    .partial_cmp(&weight_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.item_definition.name.cmp(&b.item_definition.name))
            }
            SortCriteria::Durability => {
                let durability_a = a.item_instance.durability.unwrap_or(u32::MAX);
                let durability_b = b.item_instance.durability.unwrap_or(u32::MAX);
                durability_a
                    .cmp(&durability_b)
                    .then_with(|| a.item_definition.name.cmp(&b.item_definition.name))
            }
            SortCriteria::Custom(field) => {
                let value_a = a
                    .item_instance
                    .custom_data
                    .get(field)
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let value_b = b
                    .item_instance
                    .custom_data
                    .get(field)
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                value_a
                    .cmp(value_b)
                    .then_with(|| a.item_definition.name.cmp(&b.item_definition.name))
            }
        };

        match sort_order {
            SortOrder::Ascending => comparison,
            SortOrder::Descending => comparison.reverse(),
        }
    });
}

fn get_category_order(category: &ItemCategory) -> u32 {
    match category {
        ItemCategory::Weapon(_) => 0,
        ItemCategory::Armor(_) => 1,
        ItemCategory::Tool => 2,
        ItemCategory::Consumable(_) => 3,
        ItemCategory::Material => 4,
        ItemCategory::Currency => 5,
        ItemCategory::Quest => 6,
        ItemCategory::Container => 7,
        ItemCategory::Enchantment => 8,
        ItemCategory::Custom(_) => 9,
    }
}

fn get_rarity_order(rarity: &ItemRarity) -> u32 {
    match rarity {
        ItemRarity::Common => 0,
        ItemRarity::Uncommon => 1,
        ItemRarity::Rare => 2,
        ItemRarity::Epic => 3,
        ItemRarity::Legendary => 4,
        ItemRarity::Mythic => 5,
        ItemRarity::Custom(_) => 6,
    }
}

fn stack_similar_items(items: Vec<SortableItem>) -> Vec<SortableItem> {
    let mut result = Vec::new();
    let mut current_group: Option<SortableItem> = None;

    for item in items {
        match &mut current_group {
            Some(ref mut group) => {
                // Check if items can be stacked
                if can_stack_items(
                    &group.item_instance,
                    &item.item_instance,
                    &item.item_definition,
                ) {
                    // Combine stacks
                    let new_stack = group.item_instance.stack + item.item_instance.stack;
                    if new_stack <= item.item_definition.max_stack {
                        group.item_instance.stack = new_stack;
                        continue;
                    } else {
                        // Split the stack
                        let remaining = new_stack - item.item_definition.max_stack;
                        group.item_instance.stack = item.item_definition.max_stack;
                        result.push(group.clone());

                        let mut new_item = item.clone();
                        new_item.item_instance.stack = remaining;
                        current_group = Some(new_item);
                    }
                } else {
                    // Different item, can't stack
                    result.push(group.clone());
                    current_group = Some(item);
                }
            }
            None => {
                current_group = Some(item);
            }
        }
    }

    if let Some(group) = current_group {
        result.push(group);
    }

    result
}

fn can_stack_items(item1: &ItemInstance, item2: &ItemInstance, item_def: &ItemDefinition) -> bool {
    // Items must be the same type
    if item1.definition_id != item2.definition_id {
        return false;
    }

    // Items must be stackable
    if item_def.max_stack <= 1 {
        return false;
    }

    // Items must have same durability (if applicable)
    if item1.durability != item2.durability {
        return false;
    }

    // Items must have same enchantments
    if item1.enchantments != item2.enchantments {
        return false;
    }

    // Items must have same custom data
    if item1.custom_data != item2.custom_data {
        return false;
    }

    // Items must have same binding
    if item1.bound_to_player != item2.bound_to_player {
        return false;
    }

    true
}

fn reorganize_inventory_slots(
    inventory: &mut Inventory,
    sorted_items: Vec<SortableItem>,
) -> Result<u32, InventoryError> {
    // Clear all current items
    for slot in inventory.slots.values_mut() {
        slot.item = None;
    }

    let mut items_moved = 0;
    let mut slot_iterator = inventory.slots.keys().cloned().collect::<Vec<_>>();
    slot_iterator.sort();

    // Place sorted items in order
    for (index, sorted_item) in sorted_items.into_iter().enumerate() {
        if index < slot_iterator.len() {
            let slot_id = slot_iterator[index];
            if let Some(slot) = inventory.slots.get_mut(&slot_id) {
                if !slot.locked {
                    if sorted_item.slot_id != slot_id {
                        items_moved += 1;
                    }
                    slot.item = Some(sorted_item.item_instance);
                }
            }
        } else {
            // No more slots available
            return Err(InventoryError::InventoryFull);
        }
    }

    Ok(items_moved)
}

// Auto-sort functionality for when items are added
pub fn auto_sort_inventory(
    inventory: &mut Inventory,
    item_definitions: &HashMap<u64, ItemDefinition>,
) -> Result<(), InventoryError> {
    if !inventory.constraints.auto_sort {
        return Ok(());
    }

    // Collect all items
    let mut items_with_slots: Vec<_> = inventory
        .slots
        .iter()
        .filter_map(|(slot_id, slot)| {
            if let Some(ref item) = slot.item {
                if let Some(item_def) = item_definitions.get(&item.definition_id) {
                    Some(SortableItem {
                        slot_id: *slot_id,
                        item_instance: item.clone(),
                        item_definition: item_def.clone(),
                    })
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    if items_with_slots.is_empty() {
        return Ok(());
    }

    // Sort by category first, then by name
    sort_items(
        &mut items_with_slots,
        &SortCriteria::Category,
        &SortOrder::Ascending,
    );

    // Stack similar items
    let stacked_items = stack_similar_items(items_with_slots);

    // Reorganize
    reorganize_inventory_slots(inventory, stacked_items)?;

    inventory.last_modified = current_timestamp();

    Ok(())
}
