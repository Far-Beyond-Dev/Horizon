use crate::handlers::inventory_validation::*;
use crate::handlers::item_management::*;
use crate::types::*;

pub fn craft_item_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    crafting_recipes: &Arc<Mutex<HashMap<String, CraftingRecipe>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    events: &Arc<EventSystem>,
    event: CraftItemRequest,
) {
    let use_inventory = event.use_inventory.clone();
    let result = execute_crafting(
        players,
        crafting_recipes,
        item_definitions,
        event.player_id,
        &event.recipe_id,
        event.quantity,
        use_inventory.clone(),
    );

    match result {
        Ok(craft_result) => {
            println!(
                "🔨 Player {:?} crafted {} x{} using recipe '{}'",
                event.player_id,
                craft_result.crafted_items.len(),
                event.quantity,
                event.recipe_id
            );

            // Emit crafting success event
            let _ = events.emit_plugin(
                "InventorySystem",
                "item_crafted",
                &serde_json::json!({
                    "player_id": event.player_id,
                    "recipe_id": event.recipe_id,
                    "quantity": event.quantity,
                    "crafted_items": craft_result.crafted_items,
                    "consumed_items": craft_result.consumed_items,
                    "experience_gained": craft_result.experience_gained,
                    "crafting_time": craft_result.crafting_time,
                    "inventory_used": use_inventory.clone().unwrap_or("general".to_string()),
                    "timestamp": current_timestamp()
                }),
            );

            // Emit experience gain event for skills system
            if craft_result.experience_gained > 0 {
                let _ = events.emit_plugin(
                    "SkillsSystem",
                    "experience_gained",
                    &serde_json::json!({
                        "player_id": event.player_id,
                        "skill_type": "crafting",
                        "experience": craft_result.experience_gained,
                        "source": "crafting",
                        "recipe_id": event.recipe_id
                    }),
                );
            }
            // Update inventory event
            let inventory_name = use_inventory.unwrap_or("general".to_string());
            let _ = events.emit_plugin(
                "InventorySystem",
                "inventory_updated",
                &serde_json::json!({
                    "player_id": event.player_id,
                    "inventory_name": inventory_name,
                    "action": "crafting_completed",
                    "timestamp": current_timestamp()
                }),
            );
        }
        Err(e) => {
            println!(
                "❌ Player {:?} failed to craft recipe '{}': {}",
                event.player_id, event.recipe_id, e
            );

            let _ = events.emit_plugin(
                "InventorySystem",
                "crafting_failed",
                &serde_json::json!({
                    "player_id": event.player_id,
                    "recipe_id": event.recipe_id,
                    "quantity": event.quantity,
                    "reason": e.to_string(),
                    "timestamp": current_timestamp()
                }),
            );
        }
    }
}

#[derive(Debug, Clone)]
struct CraftingResult {
    crafted_items: Vec<ItemInstance>,
    consumed_items: Vec<ItemInstance>,
    experience_gained: u32,
    crafting_time: u32,
}

fn execute_crafting(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    crafting_recipes: &Arc<Mutex<HashMap<String, CraftingRecipe>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    recipe_id: &str,
    quantity: u32,
    use_inventory: Option<String>,
) -> Result<CraftingResult, InventoryError> {
    // Get recipe
    let recipe = {
        let recipes_guard = crafting_recipes.lock().unwrap();
        recipes_guard
            .get(recipe_id)
            .cloned()
            .ok_or_else(|| InventoryError::RecipeNotFound(recipe_id.to_string()))?
    };

    // Validate player has required items and tools
    validate_crafting_requirements(players, &recipe, player_id, quantity, use_inventory.clone())?;

    // Check skill requirements (would integrate with skills system)
    validate_skill_requirements(&recipe, player_id)?;

    // Consume required items
    let consumed_items = consume_crafting_materials(
        players,
        item_definitions,
        &recipe,
        player_id,
        quantity,
        use_inventory.clone(),
    )?;

    // Create crafted items
    let crafted_items = create_crafted_items(
        players,
        item_definitions,
        &recipe,
        player_id,
        quantity,
        use_inventory,
    )?;

    let experience_gained = recipe.experience_reward * quantity;
    let crafting_time = recipe.crafting_time * quantity;

    Ok(CraftingResult {
        crafted_items,
        consumed_items,
        experience_gained,
        crafting_time,
    })
}

fn validate_crafting_requirements(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    recipe: &CraftingRecipe,
    player_id: PlayerId,
    quantity: u32,
    use_inventory: Option<String>,
) -> Result<(), InventoryError> {
    let players_guard = players.lock().unwrap();
    let players_map = players_guard
        .as_ref()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let player = players_map
        .get(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let inventory_name = use_inventory.unwrap_or_else(|| "general".to_string());
    let inventory = player
        .inventories
        .inventories
        .get(&inventory_name)
        .ok_or_else(|| {
            InventoryError::Custom(format!("Inventory '{}' not found", inventory_name))
        })?;

    // Check required items
    for required_item in &recipe.required_items {
        if required_item.consumed {
            let needed_amount = required_item.quantity * quantity;
            let available_amount = count_item_in_inventory(inventory, required_item.item_id);

            if available_amount < needed_amount {
                return Err(InventoryError::InsufficientItems {
                    needed: needed_amount,
                    available: available_amount,
                });
            }
        }
    }

    // Check required tools (non-consumed items)
    for tool_id in &recipe.required_tools {
        let has_tool = inventory.slots.values().any(|slot| {
            if let Some(ref item) = slot.item {
                item.definition_id == *tool_id
            } else {
                false
            }
        });

        if !has_tool {
            return Err(InventoryError::Custom(format!(
                "Required tool {} not found",
                tool_id
            )));
        }
    }

    // Check if there's space for output items
    let empty_slots = inventory
        .slots
        .values()
        .filter(|slot| slot.item.is_none() && !slot.locked)
        .count();

    let needed_slots = recipe.output_items.len() * quantity as usize;
    if empty_slots < needed_slots {
        return Err(InventoryError::InventoryFull);
    }

    Ok(())
}

fn validate_skill_requirements(
    recipe: &CraftingRecipe,
    _player_id: PlayerId,
) -> Result<(), InventoryError> {
    // This would integrate with a skills system
    // For now, we'll just check if there are any requirements
    for (skill_name, required_level) in &recipe.skill_requirements {
        println!(
            "🎯 Recipe requires {} level {} (skill validation not implemented)",
            skill_name, required_level
        );
    }

    Ok(())
}

fn consume_crafting_materials(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    recipe: &CraftingRecipe,
    player_id: PlayerId,
    quantity: u32,
    use_inventory: Option<String>,
) -> Result<Vec<ItemInstance>, InventoryError> {
    let mut consumed_items = Vec::new();

    for required_item in &recipe.required_items {
        if required_item.consumed {
            let amount_to_consume = required_item.quantity * quantity;

            let removed_items = remove_item_by_id_from_player(
                players,
                item_definitions,
                player_id,
                required_item.item_id,
                amount_to_consume,
                use_inventory.clone(),
            )?;

            consumed_items.extend(removed_items);
        }
    }

    // Check tool durability and reduce it
    apply_tool_durability_loss(players, recipe, player_id, quantity, use_inventory)?;

    Ok(consumed_items)
}

fn create_crafted_items(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    recipe: &CraftingRecipe,
    player_id: PlayerId,
    quantity: u32,
    use_inventory: Option<String>,
) -> Result<Vec<ItemInstance>, InventoryError> {
    let mut crafted_items = Vec::new();

    for output_item in &recipe.output_items {
        let total_amount = output_item.quantity * quantity;

        // Create item instances
        let item_instance = create_item_instance(
            item_definitions,
            output_item.item_id,
            total_amount,
            Some(player_id),
        )?;

        // Add to player inventory
        add_item_instance_to_player_inventory(
            players,
            item_definitions,
            player_id,
            &item_instance,
            use_inventory.clone(),
        )?;

        crafted_items.push(item_instance);
    }

    Ok(crafted_items)
}

fn count_item_in_inventory(inventory: &Inventory, item_id: u64) -> u32 {
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
        .sum()
}

fn remove_item_by_id_from_player(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    item_id: u64,
    amount: u32,
    use_inventory: Option<String>,
) -> Result<Vec<ItemInstance>, InventoryError> {
    let mut players_guard = players.lock().unwrap();
    let players_map = players_guard
        .as_mut()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let player = players_map
        .get_mut(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let inventory_name = use_inventory.unwrap_or_else(|| "general".to_string());
    let inventory = player
        .inventories
        .inventories
        .get_mut(&inventory_name)
        .ok_or_else(|| {
            InventoryError::Custom(format!("Inventory '{}' not found", inventory_name))
        })?;

    let item_def = {
        let defs_guard = item_definitions.lock().unwrap();
        defs_guard
            .get(&item_id)
            .cloned()
            .ok_or(InventoryError::ItemNotFound(item_id))?
    };

    let mut removed_items = Vec::new();
    let mut remaining_to_remove = amount;

    // Find slots with this item
    let mut slots_to_process: Vec<_> = inventory
        .slots
        .iter()
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
        if remaining_to_remove == 0 {
            break;
        }

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

fn apply_tool_durability_loss(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    recipe: &CraftingRecipe,
    player_id: PlayerId,
    quantity: u32,
    use_inventory: Option<String>,
) -> Result<(), InventoryError> {
    let mut players_guard = players.lock().unwrap();
    let players_map = players_guard
        .as_mut()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let player = players_map
        .get_mut(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let inventory_name = use_inventory.unwrap_or_else(|| "general".to_string());
    let inventory = player
        .inventories
        .inventories
        .get_mut(&inventory_name)
        .ok_or_else(|| {
            InventoryError::Custom(format!("Inventory '{}' not found", inventory_name))
        })?;

    // Apply durability loss to tools
    for tool_id in &recipe.required_tools {
        for slot in inventory.slots.values_mut() {
            if let Some(ref mut item) = slot.item {
                if item.definition_id == *tool_id {
                    if let Some(ref mut durability) = item.durability {
                        let durability_loss = quantity; // 1 durability per craft
                        *durability = durability.saturating_sub(durability_loss);

                        if *durability == 0 {
                            println!("🔧 Tool {} broke during crafting", tool_id);
                            slot.item = None; // Tool breaks
                        }
                    }
                    break;
                }
            }
        }
    }

    Ok(())
}

fn create_item_instance(
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    item_id: u64,
    stack: u32,
    bound_to_player: Option<PlayerId>,
) -> Result<ItemInstance, InventoryError> {
    let defs_guard = item_definitions.lock().unwrap();
    let item_def = defs_guard
        .get(&item_id)
        .ok_or(InventoryError::ItemNotFound(item_id))?;

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

fn add_item_instance_to_player_inventory(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    item_instance: &ItemInstance,
    use_inventory: Option<String>,
) -> Result<(), InventoryError> {
    let mut players_guard = players.lock().unwrap();
    let players_map = players_guard
        .as_mut()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let player = players_map
        .get_mut(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let inventory_name = use_inventory.unwrap_or_else(|| "general".to_string());
    let inventory = player
        .inventories
        .inventories
        .get_mut(&inventory_name)
        .ok_or_else(|| {
            InventoryError::Custom(format!("Inventory '{}' not found", inventory_name))
        })?;

    // Find empty slot
    let empty_slot = inventory
        .slots
        .iter()
        .find(|(_, slot)| slot.item.is_none() && !slot.locked)
        .map(|(slot_id, _)| *slot_id)
        .ok_or(InventoryError::InventoryFull)?;

    // Update weight
    let defs_guard = item_definitions.lock().unwrap();
    if let Some(item_def) = defs_guard.get(&item_instance.definition_id) {
        inventory.current_weight += item_def.weight * item_instance.stack as f32;
    }

    inventory.slots.get_mut(&empty_slot).unwrap().item = Some(item_instance.clone());
    inventory.last_modified = current_timestamp();
    player.last_activity = current_timestamp();

    Ok(())
}
