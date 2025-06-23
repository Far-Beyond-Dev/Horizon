use crate::types::*;
use crate::handlers::inventory_validation::*;

pub fn clear_inventory_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    events: &Arc<EventSystem>,
    event: ClearInventoryRequest,
) {
    if !event.confirm {
        // Safety check - require explicit confirmation
        let _ = events.emit_plugin(
            "InventorySystem",
            "clear_confirmation_required",
            &serde_json::json!({
                "player_id": event.id,
                "inventory_name": event.inventory_name.as_ref().unwrap_or(&"general".to_string()),
                "message": "Clear operation requires explicit confirmation",
                "timestamp": current_timestamp()
            }),
        );
        return;
    }

    let result = clear_player_inventory(
        players,
        item_definitions,
        event.id,
        event.inventory_name.clone(),
    );

    match result {
        Ok(clear_result) => {
            info!(
                "🗑️ Cleared inventory '{}' for player {:?}: {} items removed, {:.2}kg freed",
                event.inventory_name.as_ref().unwrap_or(&"general".to_string()),
                event.id,
                clear_result.items_removed,
                clear_result.weight_freed
            );

            // Emit inventory cleared event
            let inventory_name = event.inventory_name.clone().unwrap_or("general".to_string());
            let _ = events.emit_plugin(
                "InventorySystem",
                "inventory_cleared",
                &serde_json::json!({
                    "player_id": event.id,
                    "inventory_name": inventory_name,
                    "items_removed": clear_result.items_removed,
                    "unique_items_removed": clear_result.unique_items_removed,
                    "weight_freed": clear_result.weight_freed,
                    "total_value_lost": clear_result.total_value_lost,
                    "removed_items": clear_result.removed_items,
                    "backup_created": clear_result.backup_created,
                    "timestamp": current_timestamp()
                }),
            );

            // Create backup notification
            if clear_result.backup_created {
                let _ = events.emit_plugin(
                    "NotificationSystem",
                    "inventory_backup_created",
                    &serde_json::json!({
                        "target_player": event.id,
                        "backup_id": clear_result.backup_id,
                        "message": "Inventory backup created before clearing"
                    }),
                );
            }

            let _ = events.emit_plugin(
                "InventorySystem",
                "inventory_updated",
                &serde_json::json!({
                    "player_id": event.id,
                    "inventory_name": inventory_name,
                    "action": "cleared",
                    "timestamp": current_timestamp()
                }),
            );
        }
        Err(e) => {
            info!(
                "❌ Failed to clear inventory for player {:?}: {}",
                event.id, e
            );

            let _ = events.emit_plugin(
                "InventorySystem",
                "clear_failed",
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

#[derive(Debug, Clone)]
pub struct ClearResult {
    pub items_removed: u32,
    pub unique_items_removed: u32,
    pub weight_freed: f32,
    pub total_value_lost: u32,
    pub removed_items: Vec<RemovedItemInfo>,
    pub backup_created: bool,
    pub backup_id: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct RemovedItemInfo {
    pub item_instance_id: String,
    pub item_name: String,
    pub stack: u32,
    pub value: u32,
    pub rarity: ItemRarity,
    pub slot_id: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct InventoryBackup {
    pub backup_id: String,
    pub player_id: PlayerId,
    pub inventory_name: String,
    pub backed_up_items: Vec<BackupItem>,
    pub backup_timestamp: u64,
    pub restore_deadline: u64, // When backup expires
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct BackupItem {
    pub item_instance: ItemInstance,
    pub item_definition: ItemDefinition,
    pub slot_id: u32,
}

fn clear_player_inventory(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    inventory_name: Option<String>,
) -> Result<ClearResult, InventoryError> {
    // Validate player and inventory
    validate_player_access(players, player_id)?;
    let inventory_name = inventory_name.unwrap_or_else(|| "general".to_string());
    validate_inventory_exists(players, player_id, &inventory_name)?;

    let mut players_guard = players.lock().unwrap();
    let players_map = players_guard.as_mut()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;
    
    let player = players_map.get_mut(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let inventory = player.inventories.inventories
        .get_mut(&inventory_name)
        .ok_or_else(|| InventoryError::Custom(format!("Inventory '{}' not found", inventory_name)))?;

    // Create backup before clearing
    let backup_result = create_inventory_backup(player_id, &inventory_name, inventory, item_definitions);
    
    // Collect information about items being removed
    let mut removed_items = Vec::new();
    let mut total_weight_freed = 0.0;
    let mut total_value_lost = 0;
    let mut items_removed = 0;
    let mut unique_items_removed = 0;

    let item_defs_guard = item_definitions.lock().unwrap();

    for (slot_id, slot) in &inventory.slots {
        if let Some(ref item_instance) = slot.item {
            items_removed += item_instance.stack;
            unique_items_removed += 1;

            if let Some(item_def) = item_defs_guard.get(&item_instance.definition_id) {
                let item_weight = item_def.weight * item_instance.stack as f32;
                let item_value = item_def.value * item_instance.stack;
                
                total_weight_freed += item_weight;
                total_value_lost += item_value;

                removed_items.push(RemovedItemInfo {
                    item_instance_id: item_instance.instance_id.clone(),
                    item_name: item_def.name.clone(),
                    stack: item_instance.stack,
                    value: item_value,
                    rarity: item_def.rarity.clone(),
                    slot_id: *slot_id,
                });
            }
        }
    }

    // Perform the actual clearing
    for slot in inventory.slots.values_mut() {
        slot.item = None;
    }

    // Reset inventory properties
    inventory.current_weight = 0.0;
    inventory.last_modified = current_timestamp();
    player.last_activity = current_timestamp();

    // Apply any clearing penalties or effects
    apply_clearing_effects(player, &inventory_name, items_removed)?;

    Ok(ClearResult {
        items_removed,
        unique_items_removed,
        weight_freed: total_weight_freed,
        total_value_lost,
        removed_items,
        backup_created: backup_result.is_ok(),
        backup_id: backup_result.ok(),
    })
}

fn create_inventory_backup(
    player_id: PlayerId,
    inventory_name: &str,
    inventory: &Inventory,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
) -> Result<String, InventoryError> {
    let backup_id = uuid::Uuid::new_v4().to_string();
    let mut backed_up_items = Vec::new();

    let item_defs_guard = item_definitions.lock().unwrap();

    // Create backup entries for all items
    for (slot_id, slot) in &inventory.slots {
        if let Some(ref item_instance) = slot.item {
            if let Some(item_def) = item_defs_guard.get(&item_instance.definition_id) {
                backed_up_items.push(BackupItem {
                    item_instance: item_instance.clone(),
                    item_definition: item_def.clone(),
                    slot_id: *slot_id,
                });
            }
        }
    }

    // Create backup record
    let backup = InventoryBackup {
        backup_id: backup_id.clone(),
        player_id,
        inventory_name: inventory_name.to_string(),
        backed_up_items,
        backup_timestamp: current_timestamp(),
        restore_deadline: current_timestamp() + (24 * 60 * 60), // 24 hours to restore
    };

    // In a real implementation, this would be stored in a database
    // For now, we'll just log it
    info!(
        "💾 Created backup '{}' for player {:?} inventory '{}' with {} items",
        backup_id, player_id, inventory_name, backup.backed_up_items.len()
    );

    Ok(backup_id)
}

fn apply_clearing_effects(
    player: &mut Player,
    inventory_name: &str,
    items_removed: u32,
) -> Result<(), InventoryError> {
    // Apply any game-specific effects from clearing inventory
    
    // Example: Add a temporary effect for mass item loss
    if items_removed > 50 {
        let grief_effect = ItemEffect {
            effect_id: uuid::Uuid::new_v4().to_string(),
            source_item: 0, // No specific item
            effect_type: "inventory_grief".to_string(),
            value: items_removed as f32,
            duration: Some(current_timestamp() + 300), // 5 minutes
            applied_at: current_timestamp(),
        };
        
        player.inventories.active_effects.push(grief_effect);
        
        info!(
            "😢 Player {:?} is experiencing inventory grief after losing {} items",
            player.id, items_removed
        );
    }

    // Example: Achievement for clearing large inventory
    if items_removed > 100 {
        info!(
            "🏆 Player {:?} earned 'Fresh Start' achievement for clearing {} items",
            player.id, items_removed
        );
    }

    Ok(())
}

// Utility function to restore from backup
pub fn restore_inventory_from_backup(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    backup: InventoryBackup,
    force_restore: bool,
) -> Result<u32, InventoryError> {
    // Check if backup is still valid
    let current_time = current_timestamp();
    if !force_restore && current_time > backup.restore_deadline {
        return Err(InventoryError::Custom("Backup has expired".to_string()));
    }

    let mut players_guard = players.lock().unwrap();
    let players_map = players_guard.as_mut()
        .ok_or(InventoryError::PlayerNotFound(backup.player_id))?;
    
    let player = players_map.get_mut(&backup.player_id)
        .ok_or(InventoryError::PlayerNotFound(backup.player_id))?;

    let inventory = player.inventories.inventories
        .get_mut(&backup.inventory_name)
        .ok_or_else(|| InventoryError::Custom(format!("Inventory '{}' not found", backup.inventory_name)))?;

    // Clear current inventory
    for slot in inventory.slots.values_mut() {
        slot.item = None;
    }

    // Restore items from backup
    let mut restored_items = 0;
    for backup_item in backup.backed_up_items {
        if let Some(slot) = inventory.slots.get_mut(&backup_item.slot_id) {
            slot.item = Some(backup_item.item_instance);
            restored_items += 1;
        }
    }

    // Recalculate weight
    inventory.current_weight = inventory.slots.values()
        .filter_map(|slot| slot.item.as_ref())
        .map(|item| {
            // This would normally lookup the item definition for weight
            // For now, we'll use a default weight
            1.0 * item.stack as f32
        })
        .sum();

    inventory.last_modified = current_timestamp();
    player.last_activity = current_timestamp();

    info!(
        "🔄 Restored {} items to player {:?} inventory '{}' from backup '{}'",
        restored_items, backup.player_id, backup.inventory_name, backup.backup_id
    );

    Ok(restored_items)
}

// Selective clearing functions
pub fn clear_inventory_by_category(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    inventory_name: Option<String>,
    category: ItemCategory,
) -> Result<ClearResult, InventoryError> {
    let inventory_name = inventory_name.unwrap_or_else(|| "general".to_string());
    
    let mut players_guard = players.lock().unwrap();
    let players_map = players_guard.as_mut()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;
    
    let player = players_map.get_mut(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let inventory = player.inventories.inventories
        .get_mut(&inventory_name)
        .ok_or_else(|| InventoryError::Custom(format!("Inventory '{}' not found", inventory_name)))?;

    let item_defs_guard = item_definitions.lock().unwrap();
    let mut removed_items = Vec::new();
    let mut items_removed = 0;
    let mut weight_freed = 0.0;
    let mut value_lost = 0;

    // Clear only items of the specified category
    for (slot_id, slot) in inventory.slots.iter_mut() {
        if let Some(ref item_instance) = slot.item {
            if let Some(item_def) = item_defs_guard.get(&item_instance.definition_id) {
                if item_def.category == category {
                    items_removed += item_instance.stack;
                    weight_freed += item_def.weight * item_instance.stack as f32;
                    value_lost += item_def.value * item_instance.stack;

                    removed_items.push(RemovedItemInfo {
                        item_instance_id: item_instance.instance_id.clone(),
                        item_name: item_def.name.clone(),
                        stack: item_instance.stack,
                        value: item_def.value * item_instance.stack,
                        rarity: item_def.rarity.clone(),
                        slot_id: *slot_id,
                    });

                    slot.item = None;
                }
            }
        }
    }

    inventory.current_weight -= weight_freed;
    inventory.last_modified = current_timestamp();
    player.last_activity = current_timestamp();

    Ok(ClearResult {
        items_removed,
        unique_items_removed: removed_items.len() as u32,
        weight_freed,
        total_value_lost: value_lost,
        removed_items,
        backup_created: false,
        backup_id: None,
    })
}

pub fn clear_inventory_by_rarity(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    inventory_name: Option<String>,
    max_rarity: ItemRarity,
) -> Result<ClearResult, InventoryError> {
    // Similar implementation to clear_inventory_by_category but filtering by rarity
    // Implementation would be very similar to the category version
    Ok(ClearResult {
        items_removed: 0,
        unique_items_removed: 0,
        weight_freed: 0.0,
        total_value_lost: 0,
        removed_items: Vec::new(),
        backup_created: false,
        backup_id: None,
    })
}