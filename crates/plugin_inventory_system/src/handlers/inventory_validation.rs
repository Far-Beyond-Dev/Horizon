use crate::types::*;

/// Validates if a player exists and has access to perform inventory operations
pub fn validate_player_access(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_id: PlayerId,
) -> Result<(), InventoryError> {
    let players_guard = players.lock().unwrap();
    let players_map = players_guard.as_ref()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;
    
    players_map.get(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    Ok(())
}

/// Validates if an inventory exists for a player
pub fn validate_inventory_exists(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_id: PlayerId,
    inventory_name: &str,
) -> Result<(), InventoryError> {
    let players_guard = players.lock().unwrap();
    let players_map = players_guard.as_ref()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;
    
    let player = players_map.get(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    player.inventories.inventories.get(inventory_name)
        .ok_or_else(|| InventoryError::Custom(format!("Inventory '{}' not found", inventory_name)))?;

    Ok(())
}

/// Validates if an item definition exists
pub fn validate_item_definition(
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    item_id: u64,
) -> Result<ItemDefinition, InventoryError> {
    let defs_guard = item_definitions.lock().unwrap();
    defs_guard.get(&item_id).cloned()
        .ok_or(InventoryError::ItemNotFound(item_id))
}

/// Validates if a slot exists and is accessible
pub fn validate_slot_access(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_id: PlayerId,
    inventory_name: &str,
    slot_id: u32,
) -> Result<(), InventoryError> {
    let players_guard = players.lock().unwrap();
    let players_map = players_guard.as_ref()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;
    
    let player = players_map.get(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let inventory = player.inventories.inventories.get(inventory_name)
        .ok_or_else(|| InventoryError::Custom(format!("Inventory '{}' not found", inventory_name)))?;

    let slot = inventory.slots.get(&slot_id)
        .ok_or(InventoryError::InvalidSlot(slot_id))?;

    if slot.locked {
        return Err(InventoryError::Custom(format!("Slot {} is locked", slot_id)));
    }

    Ok(())
}

/// Validates weight constraints for adding items
pub fn validate_weight_constraints(
    inventory: &Inventory,
    item_def: &ItemDefinition,
    amount: u32,
) -> Result<(), InventoryError> {
    if let Some(max_weight) = inventory.constraints.max_weight {
        let additional_weight = item_def.weight * amount as f32;
        if inventory.current_weight + additional_weight > max_weight {
            return Err(InventoryError::WeightLimitExceeded);
        }
    }
    Ok(())
}

/// Validates category constraints for an inventory
pub fn validate_category_constraints(
    inventory: &Inventory,
    item_def: &ItemDefinition,
) -> Result<(), InventoryError> {
    if let Some(ref allowed_categories) = inventory.constraints.allowed_categories {
        if !allowed_categories.contains(&item_def.category) {
            return Err(InventoryError::Custom(format!(
                "Item category {:?} not allowed in this inventory",
                item_def.category
            )));
        }
    }
    Ok(())
}

/// Validates if a player has sufficient items for an operation
pub fn validate_sufficient_items(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_id: PlayerId,
    item_id: u64,
    required_amount: u32,
    inventory_name: Option<String>,
) -> Result<u32, InventoryError> {
    let players_guard = players.lock().unwrap();
    let players_map = players_guard.as_ref()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;
    
    let player = players_map.get(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let available_amount = if let Some(inv_name) = inventory_name {
        let inventory = player.inventories.inventories.get(&inv_name)
            .ok_or_else(|| InventoryError::Custom(format!("Inventory '{}' not found", inv_name)))?;
        
        count_item_in_inventory(inventory, item_id)
    } else {
        // Count across all inventories
        player.inventories.inventories.values()
            .map(|inv| count_item_in_inventory(inv, item_id))
            .sum()
    };

    if available_amount < required_amount {
        return Err(InventoryError::InsufficientItems {
            needed: required_amount,
            available: available_amount,
        });
    }

    Ok(available_amount)
}

/// Validates if an item can be stacked
pub fn validate_item_stackable(
    item_def: &ItemDefinition,
    current_stack: u32,
    additional_amount: u32,
) -> Result<(), InventoryError> {
    if item_def.max_stack <= 1 {
        return Err(InventoryError::ItemNotStackable);
    }

    if current_stack + additional_amount > item_def.max_stack {
        return Err(InventoryError::Custom(format!(
            "Cannot stack {} more items (current: {}, max: {})",
            additional_amount, current_stack, item_def.max_stack
        )));
    }

    Ok(())
}

/// Validates durability requirements for item usage
pub fn validate_item_durability(
    item_instance: &ItemInstance,
    item_def: &ItemDefinition,
    minimum_durability: Option<u32>,
) -> Result<(), InventoryError> {
    if let (Some(current_durability), Some(min_required)) = (item_instance.durability, minimum_durability) {
        if current_durability < min_required {
            return Err(InventoryError::DurabilityTooLow);
        }
    } else if item_def.durability_max.is_some() && item_instance.durability == Some(0) {
        return Err(InventoryError::DurabilityTooLow);
    }

    Ok(())
}

/// Validates if an item is tradeable
pub fn validate_item_tradeable(
    item_def: &ItemDefinition,
    item_instance: &ItemInstance,
    requesting_player: PlayerId,
) -> Result<(), InventoryError> {
    if !item_def.tradeable {
        return Err(InventoryError::ItemNotTradeable);
    }

    // Check if item is bound to another player
    if let Some(bound_player) = item_instance.bound_to_player {
        if bound_player != requesting_player {
            return Err(InventoryError::AccessDenied);
        }
    }

    Ok(())
}

/// Validates inventory space availability
pub fn validate_inventory_space(
    inventory: &Inventory,
    items_to_add: &[(u64, u32)], // (item_id, amount)
    item_definitions: &HashMap<u64, ItemDefinition>,
) -> Result<(), InventoryError> {
    let empty_slots = inventory.slots.values()
        .filter(|slot| slot.item.is_none() && !slot.locked)
        .count();

    // Simple validation: check if we have enough empty slots
    // This could be more sophisticated to account for stacking
    if empty_slots < items_to_add.len() {
        return Err(InventoryError::InventoryFull);
    }

    // Check weight constraints
    if let Some(max_weight) = inventory.constraints.max_weight {
        let additional_weight: f32 = items_to_add.iter()
            .map(|(item_id, amount)| {
                if let Some(item_def) = item_definitions.get(item_id) {
                    item_def.weight * (*amount as f32)
                } else {
                    0.0
                }
            })
            .sum();

        if inventory.current_weight + additional_weight > max_weight {
            return Err(InventoryError::WeightLimitExceeded);
        }
    }

    Ok(())
}

/// Validates level requirements for items
pub fn validate_level_requirements(
    item_def: &ItemDefinition,
    player_level: Option<u32>,
) -> Result<(), InventoryError> {
    if let (Some(required_level), Some(current_level)) = (item_def.level_requirement, player_level) {
        if current_level < required_level {
            return Err(InventoryError::Custom(format!(
                "Level {} required to use this item (current level: {})",
                required_level, current_level
            )));
        }
    }

    Ok(())
}

/// Validates container access permissions
pub fn validate_container_access(
    container: &Container,
    player_id: PlayerId,
) -> Result<(), InventoryError> {
    if container.public_access {
        return Ok(());
    }

    if container.access_permissions.contains(&player_id) {
        return Ok(());
    }

    Err(InventoryError::AccessDenied)
}

/// Validates if a container exists and is accessible
pub fn validate_container_exists_and_accessible(
    containers: &Arc<Mutex<HashMap<String, Container>>>,
    container_id: &str,
    player_id: PlayerId,
) -> Result<(), InventoryError> {
    let containers_guard = containers.lock().unwrap();
    let container = containers_guard.get(container_id)
        .ok_or_else(|| InventoryError::ContainerNotFound(container_id.to_string()))?;

    validate_container_access(container, player_id)
}

/// Validates if a recipe exists and can be used
pub fn validate_recipe_access(
    recipes: &Arc<Mutex<HashMap<String, CraftingRecipe>>>,
    recipe_id: &str,
    player_level: Option<u32>,
) -> Result<CraftingRecipe, InventoryError> {
    let recipes_guard = recipes.lock().unwrap();
    let recipe = recipes_guard.get(recipe_id).cloned()
        .ok_or_else(|| InventoryError::RecipeNotFound(recipe_id.to_string()))?;

    // Check unlock level
    if let (Some(unlock_level), Some(current_level)) = (recipe.unlock_level, player_level) {
        if current_level < unlock_level {
            return Err(InventoryError::Custom(format!(
                "Level {} required to use this recipe (current level: {})",
                unlock_level, current_level
            )));
        }
    }

    Ok(recipe)
}

/// Validates transaction integrity for complex operations
pub fn validate_transaction_integrity(
    transaction_items: &[ItemInstance],
    expected_total: u32,
) -> Result<(), InventoryError> {
    let actual_total: u32 = transaction_items.iter()
        .map(|item| item.stack)
        .sum();

    if actual_total != expected_total {
        return Err(InventoryError::Custom(format!(
            "Transaction integrity check failed: expected {}, got {}",
            expected_total, actual_total
        )));
    }

    Ok(())
}

/// Helper function to count items in an inventory
fn count_item_in_inventory(inventory: &Inventory, item_id: u64) -> u32 {
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
        .sum()
}

/// Validates rate limiting for inventory operations
pub fn validate_rate_limit(
    player_id: PlayerId,
    operation: &str,
    rate_limits: &mut HashMap<PlayerId, HashMap<String, RateLimit>>,
) -> Result<(), InventoryError> {
    let current_time = current_timestamp();
    
    let player_limits = rate_limits.entry(player_id).or_insert_with(HashMap::new);
    let limit = player_limits.entry(operation.to_string()).or_insert_with(|| RateLimit {
        last_operation: 0,
        operation_count: 0,
        window_start: current_time,
    });

    // Simple rate limiting: max 10 operations per second
    if current_time - limit.window_start < 1000 {
        limit.operation_count += 1;
        if limit.operation_count > 10 {
            return Err(InventoryError::Custom("Rate limit exceeded".to_string()));
        }
    } else {
        // Reset window
        limit.window_start = current_time;
        limit.operation_count = 1;
    }

    limit.last_operation = current_time;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct RateLimit {
    pub last_operation: u64,
    pub operation_count: u32,
    pub window_start: u64,
}