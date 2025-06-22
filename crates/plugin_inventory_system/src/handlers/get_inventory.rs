use crate::types::*;

pub fn get_inventory_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    events: &Arc<EventSystem>,
    event: GetInventoryRequest,
) {
    let result = get_player_inventory(
        players,
        item_definitions,
        event.id,
        event.inventory_name.clone(),
        event.include_equipment,
    );

    match result {
        Ok(inventory_data) => {
            let inventory_name = event.inventory_name.unwrap_or_else(|| "general".to_string());
            
            println!(
                "📋 Retrieved inventory '{}' for player {:?}: {} items, {:.2}kg total weight",
                inventory_name,
                event.id,
                inventory_data.total_items,
                inventory_data.total_weight
            );

            // Emit comprehensive inventory data
            let _ = events.emit_plugin(
                "InventorySystem",
                "inventory_data",
                &serde_json::json!({
                    "player_id": event.id,
                    "inventory_name": inventory_name,
                    "inventory_data": inventory_data,
                    "include_equipment": event.include_equipment,
                    "timestamp": current_timestamp()
                }),
            );
        }
        Err(e) => {
            println!("❌ Failed to retrieve inventory for player {:?}: {}", event.id, e);

            // Emit failure event
            let _ = events.emit_plugin(
                "InventorySystem",
                "inventory_fetch_failed",
                &serde_json::json!({
                    "player_id": event.id,
                    "inventory_name": event.inventory_name,
                    "reason": e.to_string(),
                    "timestamp": current_timestamp()
                }),
            );
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct InventoryData {
    pub player_id: PlayerId,
    pub inventory_name: String,
    pub slots: HashMap<u32, InventorySlotData>,
    pub equipment: Option<EquipmentSlots>,
    pub total_items: u32,
    pub unique_items: u32,
    pub total_weight: f32,
    pub weight_limit: Option<f32>,
    pub slot_count: u32,
    pub constraints: InventoryConstraints,
    pub last_modified: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct InventorySlotData {
    pub slot_id: u32,
    pub item: Option<ItemInstanceWithDefinition>,
    pub locked: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct ItemInstanceWithDefinition {
    pub instance: ItemInstance,
    pub definition: ItemDefinition,
    pub total_value: u32,
    pub weight: f32,
    pub condition: ItemCondition,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub enum ItemCondition {
    Perfect,
    Excellent,
    Good,
    Fair,
    Poor,
    Broken,
}

fn get_player_inventory(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    inventory_name: Option<String>,
    include_equipment: bool,
) -> Result<InventoryData, InventoryError> {
    let players_guard = players.lock().unwrap();
    let players_map = players_guard.as_ref()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;
    
    let player = players_map.get(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let inventory_name = inventory_name.unwrap_or_else(|| "general".to_string());
    let inventory = player.inventories.inventories
        .get(&inventory_name)
        .ok_or_else(|| InventoryError::Custom(format!("Inventory '{}' not found", inventory_name)))?;

    let item_defs_guard = item_definitions.lock().unwrap();
    
    let mut slot_data = HashMap::new();
    let mut total_items = 0;
    let mut unique_items = 0;
    let mut total_weight = 0.0;

    for (slot_id, slot) in &inventory.slots {
        let slot_info = if let Some(ref item_instance) = slot.item {
            if let Some(item_def) = item_defs_guard.get(&item_instance.definition_id) {
                total_items += item_instance.stack;
                unique_items += 1;
                total_weight += item_def.weight * item_instance.stack as f32;

                let condition = calculate_item_condition(item_instance, item_def);
                let total_value = item_def.value * item_instance.stack;
                let weight = item_def.weight * item_instance.stack as f32;

                Some(ItemInstanceWithDefinition {
                    instance: item_instance.clone(),
                    definition: item_def.clone(),
                    total_value,
                    weight,
                    condition,
                })
            } else {
                None
            }
        } else {
            None
        };

        slot_data.insert(*slot_id, InventorySlotData {
            slot_id: *slot_id,
            item: slot_info,
            locked: slot.locked,
        });
    }

    let equipment = if include_equipment {
        Some(player.inventories.equipped_items.clone())
    } else {
        None
    };

    Ok(InventoryData {
        player_id,
        inventory_name,
        slots: slot_data,
        equipment,
        total_items,
        unique_items,
        total_weight,
        weight_limit: inventory.constraints.max_weight,
        slot_count: inventory.slots.len() as u32,
        constraints: inventory.constraints.clone(),
        last_modified: inventory.last_modified,
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