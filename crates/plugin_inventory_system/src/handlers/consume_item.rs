use crate::types::*;
use crate::handlers::inventory_validation::*;
use crate::handlers::item_management::*;

pub fn consume_item_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    events: &Arc<EventSystem>,
    event: ConsumeItemRequest,
) {
    let inventory_name = event.inventory_name.clone();
    let result = consume_item_from_player(
        players,
        item_definitions,
        event.id,
        event.item_id as u64,
        event.amount,
        inventory_name.clone(),
    );

    match result {
        Ok(consumption_result) => {
            println!(
                "🍽️ Player {:?} consumed {} of {} from inventory '{}'",
                event.id,
                event.amount,
                consumption_result.item_name,
                inventory_name.as_ref().unwrap_or(&"general".to_string())
            );

            // Emit consumption event
            let _ = events.emit_plugin(
                "InventorySystem",
                "item_consumed",
                &serde_json::json!({
                    "player_id": event.id,
                    "item_id": event.item_id,
                    "amount": event.amount,
                    "item_name": consumption_result.item_name,
                    "inventory_name": inventory_name.clone().unwrap_or("general".to_string()),
                    "consumed_instances": consumption_result.consumed_instances,
                    "effects_applied": consumption_result.effects_applied,
                    "timestamp": current_timestamp()
                }),
            );


            // Apply consumption effects
            for effect in &consumption_result.effects_applied {
                let _ = events.emit_plugin(
                    "EffectsSystem",
                    "apply_effect",
                    &serde_json::json!({
                        "player_id": event.id,
                        "effect": effect,
                        "source": "item_consumption",
                        "item_id": event.item_id
                    }),
                );
            }
            // Emit inventory updated event
            let _ = events.emit_plugin(
                "InventorySystem",
                "inventory_updated",
                &serde_json::json!({
                    "player_id": event.id,
                    "inventory_name": inventory_name.unwrap_or("general".to_string()),
                    "action": "item_consumed",
                    "timestamp": current_timestamp()
                }),
            );

            // Check for achievements/quests
            let _ = events.emit_plugin(
                "QuestSystem",
                "item_consumed",
                &serde_json::json!({
                    "player_id": event.id,
                    "item_id": event.item_id,
                    "amount": event.amount,
                    "total_consumed": consumption_result.total_consumed_in_session
                }),
            );
        }
        Err(e) => {
            println!(
                "❌ Player {:?} failed to consume item {}: {}",
                event.id, event.item_id, e
            );

            let _ = events.emit_plugin(
                "InventorySystem",
                "consumption_failed",
                &serde_json::json!({
                    "player_id": event.id,
                    "item_id": event.item_id,
                    "amount": event.amount,
                    "reason": e.to_string(),
                    "timestamp": current_timestamp()
                }),
            );
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConsumptionResult {
    pub item_name: String,
    pub consumed_instances: Vec<String>, // instance IDs
    pub effects_applied: Vec<ConsumptionEffect>,
    pub total_consumed_in_session: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ConsumptionEffect {
    pub effect_type: String,
    pub value: f32,
    pub duration: Option<u64>, // duration in seconds
    pub description: String,
}

fn consume_item_from_player(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    item_id: u64,
    amount: u32,
    inventory_name: Option<String>,
) -> Result<ConsumptionResult, InventoryError> {
    // Validate item definition and consumption properties
    let item_def = {
        let defs_guard = item_definitions.lock().unwrap();
        let def = defs_guard.get(&item_id).cloned()
            .ok_or(InventoryError::ItemNotFound(item_id))?;
        
        if !def.consumable {
            return Err(InventoryError::Custom(format!("Item '{}' is not consumable", def.name)));
        }
        
        def
    };

    // Validate player has sufficient items
    validate_sufficient_items(players, player_id, item_id, amount, inventory_name.clone())?;

    // Remove items from inventory
    let removed_items = remove_items_from_player_inventory(
        players,
        item_definitions,
        player_id,
        item_id,
        amount,
        inventory_name,
    )?;

    // Apply consumption effects
    let effects_applied = apply_consumption_effects(
        players,
        &item_def,
        player_id,
        amount,
    )?;

    // Update consumption statistics
    let total_consumed = update_consumption_statistics(players, player_id, item_id, amount)?;

    let consumed_instances: Vec<String> = removed_items.iter()
        .map(|item| item.instance_id.clone())
        .collect();

    Ok(ConsumptionResult {
        item_name: item_def.name,
        consumed_instances,
        effects_applied,
        total_consumed_in_session: total_consumed,
    })
}

fn remove_items_from_player_inventory(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    item_id: u64,
    amount: u32,
    inventory_name: Option<String>,
) -> Result<Vec<ItemInstance>, InventoryError> {
    let mut players_guard = players.lock().unwrap();
    let players_map = players_guard.as_mut()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;
    
    let player = players_map.get_mut(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let inventory_name = inventory_name.unwrap_or_else(|| "general".to_string());
    let inventory = player.inventories.inventories
        .get_mut(&inventory_name)
        .ok_or_else(|| InventoryError::Custom(format!("Inventory '{}' not found", inventory_name)))?;

    let item_def = {
        let defs_guard = item_definitions.lock().unwrap();
        defs_guard.get(&item_id).cloned()
            .ok_or(InventoryError::ItemNotFound(item_id))?
    };

    let mut removed_items = Vec::new();
    let mut remaining_to_remove = amount;

    // Find and remove items
    let mut slots_to_process: Vec<_> = inventory.slots.iter()
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

    // Sort by expiration/age for FIFO consumption
    slots_to_process.sort_by(|&a, &b| {
        let item_a = &inventory.slots[&a].item.as_ref().unwrap();
        let item_b = &inventory.slots[&b].item.as_ref().unwrap();
        item_a.acquired_timestamp.cmp(&item_b.acquired_timestamp)
    });

    for slot_id in slots_to_process {
        if remaining_to_remove == 0 { break; }

        if let Some(slot) = inventory.slots.get_mut(&slot_id) {
            if let Some(ref mut item_instance) = slot.item {
                let remove_amount = std::cmp::min(remaining_to_remove, item_instance.stack);
                
                // Validate item condition for consumption
                validate_item_consumable(item_instance, &item_def)?;
                
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

fn validate_item_consumable(
    item_instance: &ItemInstance,
    item_def: &ItemDefinition,
) -> Result<(), InventoryError> {
    // Check if item is broken (if it has durability)
    if let Some(durability) = item_instance.durability {
        if durability == 0 {
            return Err(InventoryError::Custom(format!(
                "Cannot consume broken {}",
                item_def.name
            )));
        }
    }

    // Check custom consumption requirements
    if let Some(requires_condition) = item_def.custom_properties.get("requires_condition") {
        if let Some(condition_str) = requires_condition.as_str() {
            // This would integrate with a conditions system
            println!("🔍 Checking consumption condition: {}", condition_str);
        }
    }

    Ok(())
}

fn apply_consumption_effects(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_def: &ItemDefinition,
    player_id: PlayerId,
    amount_consumed: u32,
) -> Result<Vec<ConsumptionEffect>, InventoryError> {
    let mut effects_applied = Vec::new();

    // Parse effects from item definition
    let base_effects = parse_consumption_effects_from_definition(item_def);
    
    for base_effect in base_effects {
        let scaled_effect = ConsumptionEffect {
            effect_type: base_effect.effect_type.clone(),
            value: base_effect.value * amount_consumed as f32,
            duration: base_effect.duration,
            description: format!(
                "{} (x{} from consuming {} {})",
                base_effect.description,
                amount_consumed,
                amount_consumed,
                item_def.name
            ),
        };
        
        effects_applied.push(scaled_effect.clone());
        
        // Apply effect to player
        apply_effect_to_player(players, player_id, &scaled_effect)?;
    }

    // Handle special consumption types
    match &item_def.category {
        ItemCategory::Consumable(ConsumableType::Food) => {
            effects_applied.push(ConsumptionEffect {
                effect_type: "hunger_restore".to_string(),
                value: 20.0 * amount_consumed as f32,
                duration: None,
                description: "Hunger restored".to_string(),
            });
        }
        ItemCategory::Consumable(ConsumableType::Potion) => {
            effects_applied.push(ConsumptionEffect {
                effect_type: "potion_effect".to_string(),
                value: 1.0,
                duration: Some(300), // 5 minutes
                description: "Potion effect active".to_string(),
            });
        }
        _ => {}
    }

    Ok(effects_applied)
}

fn parse_consumption_effects_from_definition(item_def: &ItemDefinition) -> Vec<ConsumptionEffect> {
    let mut effects = Vec::new();

    // Parse effects from custom properties
    if let Some(effects_data) = item_def.custom_properties.get("consumption_effects") {
        if let Some(effects_array) = effects_data.as_array() {
            for effect_value in effects_array {
                if let Ok(effect) = serde_json::from_value::<ConsumptionEffect>(effect_value.clone()) {
                    effects.push(effect);
                }
            }
        }
    }

    // Default effects based on item type
    if effects.is_empty() {
        match &item_def.category {
            ItemCategory::Consumable(ConsumableType::Food) => {
                effects.push(ConsumptionEffect {
                    effect_type: "health_restore".to_string(),
                    value: 10.0,
                    duration: None,
                    description: "Restores health".to_string(),
                });
            }
            ItemCategory::Consumable(ConsumableType::Potion) => {
                effects.push(ConsumptionEffect {
                    effect_type: "mana_restore".to_string(),
                    value: 25.0,
                    duration: None,
                    description: "Restores mana".to_string(),
                });
            }
            _ => {}
        }
    }

    effects
}

fn apply_effect_to_player(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_id: PlayerId,
    effect: &ConsumptionEffect,
) -> Result<(), InventoryError> {
    let mut players_guard = players.lock().unwrap();
    let players_map = players_guard.as_mut()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;
    
    let player = players_map.get_mut(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    // Add to active effects if it has duration
    if let Some(duration) = effect.duration {
        let item_effect = ItemEffect {
            effect_id: uuid::Uuid::new_v4().to_string(),
            source_item: 0, // Would be set to actual item ID
            effect_type: effect.effect_type.clone(),
            value: effect.value,
            duration: Some(current_timestamp() + duration),
            applied_at: current_timestamp(),
        };
        
        player.inventories.active_effects.push(item_effect);
    }

    // Apply immediate effects
    match effect.effect_type.as_str() {
        "health_restore" => {
            println!("💚 Player {:?} restored {} health", player_id, effect.value);
        }
        "mana_restore" => {
            println!("💙 Player {:?} restored {} mana", player_id, effect.value);
        }
        "hunger_restore" => {
            println!("🍖 Player {:?} restored {} hunger", player_id, effect.value);
        }
        "speed_boost" => {
            println!("🏃 Player {:?} gained speed boost: {}", player_id, effect.value);
        }
        "strength_boost" => {
            println!("💪 Player {:?} gained strength boost: {}", player_id, effect.value);
        }
        _ => {
            println!("✨ Player {:?} gained effect '{}': {}", player_id, effect.effect_type, effect.value);
        }
    }

    player.last_activity = current_timestamp();

    Ok(())
}

fn update_consumption_statistics(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_id: PlayerId,
    item_id: u64,
    amount: u32,
) -> Result<u32, InventoryError> {
    let mut players_guard = players.lock().unwrap();
    let players_map = players_guard.as_mut()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;
    
    let player = players_map.get_mut(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    // Update consumption statistics in custom data
    let stats_key = format!("consumption_stats_{}", item_id);
    let current_total = player.inventories.inventories
        .get("general")
        .and_then(|inv| inv.constraints.grid_size) // Misusing this field for demo
        .unwrap_or((0, 0)).0 + amount;

    // This would normally be stored in a proper statistics system
    println!("📊 Player {:?} total consumption of item {}: {}", player_id, item_id, current_total);

    Ok(current_total)
}

// Utility function for checking consumption cooldowns
pub fn check_consumption_cooldown(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_id: PlayerId,
    item_id: u64,
    cooldown_seconds: u64,
) -> Result<bool, InventoryError> {
    let players_guard = players.lock().unwrap();
    let players_map = players_guard.as_ref()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;
    
    let player = players_map.get(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    // Check last consumption time (would be stored in player data)
    let current_time = current_timestamp();
    let cooldown_key = format!("last_consumption_{}", item_id);
    
    // This is a simplified check - in reality you'd store this properly
    let last_consumption = player.last_activity; // Simplified
    
    if current_time - last_consumption < cooldown_seconds {
        return Ok(false); // Still on cooldown
    }

    Ok(true) // Cooldown finished
}

// Function to clean up expired effects
pub fn cleanup_expired_effects(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    events: &Arc<EventSystem>,
) {
    let mut players_guard = players.lock().unwrap();
    if let Some(ref mut players_map) = *players_guard {
        let current_time = current_timestamp();
        
        for (player_id, player) in players_map.iter_mut() {
            let initial_count = player.inventories.active_effects.len();
            
            player.inventories.active_effects.retain(|effect| {
                if let Some(expires_at) = effect.duration {
                    if current_time >= expires_at {
                        // Effect expired, emit event
                        let _ = events.emit_plugin(
                            "EffectsSystem",
                            "effect_expired",
                            &serde_json::json!({
                                "player_id": player_id,
                                "effect_id": effect.effect_id,
                                "effect_type": effect.effect_type,
                                "source_item": effect.source_item,
                                "timestamp": current_time
                            }),
                        );
                        return false; // Remove from list
                    }
                }
                true // Keep effect
            });
            
            let removed_count = initial_count - player.inventories.active_effects.len();
            if removed_count > 0 {
                println!("🧹 Cleaned up {} expired effects for player {:?}", removed_count, player_id);
            }
        }
    }
}