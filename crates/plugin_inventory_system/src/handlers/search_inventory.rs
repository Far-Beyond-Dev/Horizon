use crate::handlers::get_inventory::ItemInstanceWithDefinition;
use crate::types::*;

pub fn search_inventory_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    events: &Arc<EventSystem>,
    event: SearchInventoryRequest,
) {
    let result = execute_inventory_search(players, item_definitions, &event.query);

    match result {
        Ok(search_result) => {
            println!(
                "🔍 Search completed for player {:?}: {} results found (page {}/{})",
                event.query.player_id,
                search_result.total_count,
                search_result.page,
                search_result.total_pages
            );

            // Emit search results
            let _ = events.emit_plugin(
                "InventorySystem",
                "search_results",
                &serde_json::json!({
                    "player_id": event.query.player_id,
                    "query": event.query,
                    "results": search_result,
                    "timestamp": current_timestamp()
                }),
            );
        }
        Err(e) => {
            println!(
                "❌ Search failed for player {:?}: {}",
                event.query.player_id, e
            );

            let _ = events.emit_plugin(
                "InventorySystem",
                "search_failed",
                &serde_json::json!({
                    "player_id": event.query.player_id,
                    "query": event.query,
                    "reason": e.to_string(),
                    "timestamp": current_timestamp()
                }),
            );
        }
    }
}

fn execute_inventory_search(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    query: &InventoryQuery,
) -> Result<InventorySearchResult, InventoryError> {
    let players_guard = players.lock().unwrap();
    let players_map = players_guard
        .as_ref()
        .ok_or(InventoryError::PlayerNotFound(query.player_id))?;

    let player = players_map
        .get(&query.player_id)
        .ok_or(InventoryError::PlayerNotFound(query.player_id))?;

    let item_defs_guard = item_definitions.lock().unwrap();

    // Collect all items from specified inventories
    let mut all_items = Vec::new();

    // Search in specified inventories or all if empty
    let inventories_to_search: Vec<_> = if query.inventory_types.is_empty() {
        player.inventories.inventories.keys().cloned().collect()
    } else {
        query.inventory_types.clone()
    };

    // Search in inventories
    for inventory_name in &inventories_to_search {
        if let Some(inventory) = player.inventories.inventories.get(inventory_name) {
            for slot in inventory.slots.values() {
                if let Some(ref item_instance) = slot.item {
                    if let Some(item_def) = item_defs_guard.get(&item_instance.definition_id) {
                        if matches_search_criteria(item_instance, item_def, query) {
                            all_items.push(create_search_item(
                                item_instance,
                                item_def,
                                inventory_name,
                                Some(slot.slot_id),
                            ));
                        }
                    }
                }
            }
        }
    }

    // Search in equipment if requested
    if query.include_equipped {
        let equipment_items = [
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

        for (slot_name, equipped_item) in equipment_items {
            if let Some(ref item_instance) = equipped_item {
                if let Some(item_def) = item_defs_guard.get(&item_instance.definition_id) {
                    if matches_search_criteria(item_instance, item_def, query) {
                        all_items.push(create_search_item(
                            item_instance,
                            item_def,
                            &format!("equipment_{}", slot_name),
                            None,
                        ));
                    }
                }
            }
        }

        // Check custom equipment slots
        for (slot_name, item_instance) in &player.inventories.equipped_items.custom_slots {
            if let Some(item_def) = item_defs_guard.get(&item_instance.definition_id) {
                if matches_search_criteria(item_instance, item_def, query) {
                    all_items.push(create_search_item(
                        item_instance,
                        item_def,
                        &format!("equipment_{}", slot_name),
                        None,
                    ));
                }
            }
        }
    }

    // Sort results
    sort_search_results(&mut all_items, &query.sort_by, &query.sort_order);

    // Apply pagination
    let total_count = all_items.len() as u32;
    let per_page = query.pagination.per_page.max(1);
    let total_pages = (total_count + per_page - 1) / per_page;
    let start_idx = (query.pagination.page.saturating_sub(1) * per_page) as usize;
    let end_idx = std::cmp::min(start_idx + per_page as usize, all_items.len());

    let items = if start_idx < all_items.len() {
        all_items[start_idx..end_idx]
            .iter()
            .map(|item| item.instance.clone())
            .collect()
    } else {
        Vec::new()
    };

    Ok(InventorySearchResult {
        items,
        total_count,
        page: query.pagination.page,
        per_page,
        total_pages,
    })
}

#[derive(Debug, Clone)]
struct SearchItem {
    instance: ItemInstance,
    definition: ItemDefinition,
    location: String,
    slot_id: Option<u32>,
    total_value: u32,
    weight: f32,
}

fn matches_search_criteria(
    item_instance: &ItemInstance,
    item_def: &ItemDefinition,
    query: &InventoryQuery,
) -> bool {
    // Category filter
    if let Some(ref category_filter) = query.category_filter {
        if &item_def.category != category_filter {
            return false;
        }
    }

    // Rarity filter
    if let Some(ref rarity_filter) = query.rarity_filter {
        if &item_def.rarity != rarity_filter {
            return false;
        }
    }

    // Name search (case-insensitive)
    if let Some(ref name_search) = query.name_search {
        let search_lower = name_search.to_lowercase();
        if !item_def.name.to_lowercase().contains(&search_lower)
            && !item_def.description.to_lowercase().contains(&search_lower)
        {
            return false;
        }
    }

    true
}

fn create_search_item(
    item_instance: &ItemInstance,
    item_def: &ItemDefinition,
    location: &str,
    slot_id: Option<u32>,
) -> SearchItem {
    SearchItem {
        instance: item_instance.clone(),
        definition: item_def.clone(),
        location: location.to_string(),
        slot_id,
        total_value: item_def.value * item_instance.stack,
        weight: item_def.weight * item_instance.stack as f32,
    }
}

fn sort_search_results(
    items: &mut Vec<SearchItem>,
    sort_criteria: &SortCriteria,
    sort_order: &SortOrder,
) {
    items.sort_by(|a, b| {
        let comparison = match sort_criteria {
            SortCriteria::Name => a.definition.name.cmp(&b.definition.name),
            SortCriteria::Category => {
                format!("{:?}", a.definition.category).cmp(&format!("{:?}", b.definition.category))
            }
            SortCriteria::Rarity => {
                let rarity_order_a = get_rarity_order(&a.definition.rarity);
                let rarity_order_b = get_rarity_order(&b.definition.rarity);
                rarity_order_a.cmp(&rarity_order_b)
            }
            SortCriteria::Quantity => a.instance.stack.cmp(&b.instance.stack),
            SortCriteria::Value => a.total_value.cmp(&b.total_value),
            SortCriteria::DateAcquired => a
                .instance
                .acquired_timestamp
                .cmp(&b.instance.acquired_timestamp),
            SortCriteria::Weight => a
                .weight
                .partial_cmp(&b.weight)
                .unwrap_or(std::cmp::Ordering::Equal),
            SortCriteria::Durability => {
                let durability_a = a.instance.durability.unwrap_or(u32::MAX);
                let durability_b = b.instance.durability.unwrap_or(u32::MAX);
                durability_a.cmp(&durability_b)
            }
            SortCriteria::Custom(field) => {
                // Handle custom sorting based on custom properties
                let value_a = a
                    .instance
                    .custom_data
                    .get(field)
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let value_b = b
                    .instance
                    .custom_data
                    .get(field)
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                value_a.cmp(value_b)
            }
        };

        match sort_order {
            SortOrder::Ascending => comparison,
            SortOrder::Descending => comparison.reverse(),
        }
    });
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

// Additional utility function for advanced search features
pub fn get_search_suggestions(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    partial_name: &str,
) -> Result<Vec<String>, InventoryError> {
    let players_guard = players.lock().unwrap();
    let players_map = players_guard
        .as_ref()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let player = players_map
        .get(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let item_defs_guard = item_definitions.lock().unwrap();
    let partial_lower = partial_name.to_lowercase();

    let mut suggestions = std::collections::HashSet::new();

    // Collect item names from player's inventories
    for inventory in player.inventories.inventories.values() {
        for slot in inventory.slots.values() {
            if let Some(ref item_instance) = slot.item {
                if let Some(item_def) = item_defs_guard.get(&item_instance.definition_id) {
                    if item_def.name.to_lowercase().contains(&partial_lower) {
                        suggestions.insert(item_def.name.clone());
                    }
                }
            }
        }
    }

    // Include equipped items
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
        if let Some(ref item_instance) = equipped_item {
            if let Some(item_def) = item_defs_guard.get(&item_instance.definition_id) {
                if item_def.name.to_lowercase().contains(&partial_lower) {
                    suggestions.insert(item_def.name.clone());
                }
            }
        }
    }

    // Add custom equipped items
    for item_instance in player.inventories.equipped_items.custom_slots.values() {
        if let Some(item_def) = item_defs_guard.get(&item_instance.definition_id) {
            if item_def.name.to_lowercase().contains(&partial_lower) {
                suggestions.insert(item_def.name.clone());
            }
        }
    }

    let mut result: Vec<_> = suggestions.into_iter().collect();
    result.sort();
    result.truncate(10); // Limit to 10 suggestions

    Ok(result)
}
