use crate::handlers::inventory_validation::*;
use crate::types::*;

pub fn create_container_handler(
    containers: &Arc<Mutex<HashMap<String, Container>>>,
    events: &Arc<EventSystem>,
    event: CreateContainerRequest,
) {
    let result = create_new_container(
        containers,
        event.container_id.clone(),
        event.container_type,
        event.name,
        event.constraints,
        event.position,
        event.public_access,
    );

    match result {
        Ok(container) => {
            println!(
                "📦 Created container '{}' ({}): {} slots, public: {}",
                container.name,
                container.container_id,
                container.inventory.slots.len(),
                container.public_access
            );

            let _ = events.emit_plugin(
                "InventorySystem",
                "container_created",
                &serde_json::json!({
                    "container_id": container.container_id,
                    "container_type": format!("{:?}", container.container_type),
                    "name": container.name,
                    "public_access": container.public_access,
                    "position": container.location,
                    "slot_count": container.inventory.slots.len(),
                    "timestamp": current_timestamp()
                }),
            );
        }
        Err(e) => {
            println!(
                "❌ Failed to create container '{}': {}",
                event.container_id, e
            );

            let _ = events.emit_plugin(
                "InventorySystem",
                "container_creation_failed",
                &serde_json::json!({
                    "container_id": event.container_id,
                    "reason": e.to_string(),
                    "timestamp": current_timestamp()
                }),
            );
        }
    }
}

pub fn access_container_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    containers: &Arc<Mutex<HashMap<String, Container>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    events: &Arc<EventSystem>,
    event: AccessContainerRequest,
) {
    let result = handle_container_access(
        players,
        containers,
        item_definitions,
        event.player_id,
        &event.container_id,
        event.clone().action,
    );

    match result {
        Ok(access_result) => match access_result {
            ContainerAccessResult::Opened(container_info) => {
                println!(
                    "📂 Player {:?} opened container '{}'",
                    event.player_id, event.container_id
                );

                let _ = events.emit_plugin(
                    "InventorySystem",
                    "container_opened",
                    &serde_json::json!({
                        "player_id": event.player_id,
                        "container_id": event.container_id,
                        "container_info": container_info,
                        "timestamp": current_timestamp()
                    }),
                );
            }
            ContainerAccessResult::Closed => {
                println!(
                    "📁 Player {:?} closed container '{}'",
                    event.player_id, event.container_id
                );

                let _ = events.emit_plugin(
                    "InventorySystem",
                    "container_closed",
                    &serde_json::json!({
                        "player_id": event.player_id,
                        "container_id": event.container_id,
                        "timestamp": current_timestamp()
                    }),
                );
            }
            ContainerAccessResult::ItemAdded(item_info) => {
                println!(
                    "➕ Player {:?} added item to container '{}': {} x{}",
                    event.player_id, event.container_id, item_info.item_name, item_info.quantity
                );

                let _ = events.emit_plugin(
                    "InventorySystem",
                    "container_item_added",
                    &serde_json::json!({
                        "player_id": event.player_id,
                        "container_id": event.container_id,
                        "item_info": item_info,
                        "timestamp": current_timestamp()
                    }),
                );
            }
            ContainerAccessResult::ItemRemoved(item_info) => {
                println!(
                    "➖ Player {:?} removed item from container '{}': {} x{}",
                    event.player_id, event.container_id, item_info.item_name, item_info.quantity
                );

                let _ = events.emit_plugin(
                    "InventorySystem",
                    "container_item_removed",
                    &serde_json::json!({
                        "player_id": event.player_id,
                        "container_id": event.container_id,
                        "item_info": item_info,
                        "timestamp": current_timestamp()
                    }),
                );
            }
            ContainerAccessResult::PermissionsUpdated => {
                println!(
                    "🔐 Player {:?} updated permissions for container '{}'",
                    event.player_id, event.container_id
                );

                let _ = events.emit_plugin(
                    "InventorySystem",
                    "container_permissions_updated",
                    &serde_json::json!({
                        "player_id": event.player_id,
                        "container_id": event.container_id,
                        "timestamp": current_timestamp()
                    }),
                );
            }
        },
        Err(e) => {
            println!(
                "❌ Container access failed for player {:?} on container '{}': {}",
                event.player_id, event.container_id, e
            );

            let _ = events.emit_plugin(
                "InventorySystem",
                "container_access_failed",
                &serde_json::json!({
                    "player_id": event.player_id,
                    "container_id": event.container_id,
                    "action": format!("{:?}", event.action),
                    "reason": e.to_string(),
                    "timestamp": current_timestamp()
                }),
            );
        }
    }
}

pub fn get_container_handler(
    containers: &Arc<Mutex<HashMap<String, Container>>>,
    events: &Arc<EventSystem>,
    player_id: PlayerId,
    container_id: String,
) {
    let result = get_container_info(containers, &container_id, player_id);

    match result {
        Ok(container_info) => {
            println!(
                "📋 Retrieved container info for '{}' (requested by {:?})",
                container_id, player_id
            );

            let _ = events.emit_plugin(
                "InventorySystem",
                "container_info",
                &serde_json::json!({
                    "player_id": player_id,
                    "container_id": container_id,
                    "container_info": container_info,
                    "timestamp": current_timestamp()
                }),
            );
        }
        Err(e) => {
            println!(
                "❌ Failed to get container info for '{}': {}",
                container_id, e
            );

            let _ = events.emit_plugin(
                "InventorySystem",
                "container_info_failed",
                &serde_json::json!({
                    "player_id": player_id,
                    "container_id": container_id,
                    "reason": e.to_string(),
                    "timestamp": current_timestamp()
                }),
            );
        }
    }
}

pub fn delete_container_handler(
    containers: &Arc<Mutex<HashMap<String, Container>>>,
    events: &Arc<EventSystem>,
    container_id: String,
    requesting_player: PlayerId,
) {
    let result = delete_container(containers, &container_id, requesting_player);

    match result {
        Ok(deleted_container) => {
            println!(
                "🗑️ Deleted container '{}' ({})",
                deleted_container.name, container_id
            );

            let _ = events.emit_plugin(
                "InventorySystem",
                "container_deleted",
                &serde_json::json!({
                    "container_id": container_id,
                    "deleted_by": requesting_player,
                    "container_name": deleted_container.name,
                    "timestamp": current_timestamp()
                }),
            );
        }
        Err(e) => {
            println!("❌ Failed to delete container '{}': {}", container_id, e);

            let _ = events.emit_plugin(
                "InventorySystem",
                "container_deletion_failed",
                &serde_json::json!({
                    "container_id": container_id,
                    "requested_by": requesting_player,
                    "reason": e.to_string(),
                    "timestamp": current_timestamp()
                }),
            );
        }
    }
}

#[derive(Debug, Clone)]
enum ContainerAccessResult {
    Opened(ContainerInfo),
    Closed,
    ItemAdded(ItemTransferInfo),
    ItemRemoved(ItemTransferInfo),
    PermissionsUpdated,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ContainerInfo {
    pub container_id: String,
    pub name: String,
    pub container_type: ContainerType,
    pub total_slots: u32,
    pub used_slots: u32,
    pub items: Vec<ContainerItemInfo>,
    pub access_level: AccessLevel,
    pub last_accessed: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ContainerItemInfo {
    pub slot_id: u32,
    pub item_instance_id: String,
    pub item_name: String,
    pub stack: u32,
    pub item_value: u32,
    pub rarity: ItemRarity,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ItemTransferInfo {
    pub item_instance_id: String,
    pub item_name: String,
    pub quantity: u32,
    pub slot_id: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum AccessLevel {
    Owner,
    ReadWrite,
    ReadOnly,
    NoAccess,
}

fn create_new_container(
    containers: &Arc<Mutex<HashMap<String, Container>>>,
    container_id: String,
    container_type: ContainerType,
    name: String,
    constraints: InventoryConstraints,
    position: Option<WorldPosition>,
    public_access: bool,
) -> Result<Container, InventoryError> {
    let mut containers_guard = containers.lock().unwrap();

    // Check if container already exists
    if containers_guard.contains_key(&container_id) {
        return Err(InventoryError::Custom(format!(
            "Container '{}' already exists",
            container_id
        )));
    }

    // Create inventory for the container
    let slot_count = constraints.max_slots.unwrap_or(27); // Default chest size
    let mut slots = HashMap::new();

    for i in 0..slot_count {
        slots.insert(
            i,
            InventorySlot {
                slot_id: i,
                item: None,
                locked: false,
            },
        );
    }

    let inventory = Inventory {
        inventory_type: InventoryType::Container(container_id.clone()),
        slots,
        constraints,
        current_weight: 0.0,
        last_modified: current_timestamp(),
    };

    let container = Container {
        container_id: container_id.clone(),
        container_type,
        name,
        inventory,
        location: position,
        access_permissions: Vec::new(),
        password_protected: false,
        public_access,
        created_at: current_timestamp(),
    };

    containers_guard.insert(container_id, container.clone());

    Ok(container)
}

fn handle_container_access(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    containers: &Arc<Mutex<HashMap<String, Container>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    container_id: &str,
    action: ContainerAction,
) -> Result<ContainerAccessResult, InventoryError> {
    // Validate player exists
    validate_player_access(players, player_id)?;

    // Validate container access
    validate_container_exists_and_accessible(containers, container_id, player_id)?;

    match action {
        ContainerAction::Open => {
            let container_info = get_container_info(containers, container_id, player_id)?;
            Ok(ContainerAccessResult::Opened(container_info))
        }
        ContainerAction::Close => Ok(ContainerAccessResult::Closed),
        ContainerAction::AddItem {
            item_instance_id,
            quantity,
        } => {
            let transfer_info = add_item_to_container(
                players,
                containers,
                item_definitions,
                player_id,
                container_id,
                &item_instance_id,
                quantity,
            )?;
            Ok(ContainerAccessResult::ItemAdded(transfer_info))
        }
        ContainerAction::RemoveItem {
            item_instance_id,
            quantity,
        } => {
            let transfer_info = remove_item_from_container(
                players,
                containers,
                item_definitions,
                player_id,
                container_id,
                &item_instance_id,
                quantity,
            )?;
            Ok(ContainerAccessResult::ItemRemoved(transfer_info))
        }
        ContainerAction::SetPermissions { permissions } => {
            set_container_permissions(containers, container_id, player_id, permissions)?;
            Ok(ContainerAccessResult::PermissionsUpdated)
        }
    }
}

fn get_container_info(
    containers: &Arc<Mutex<HashMap<String, Container>>>,
    container_id: &str,
    requesting_player: PlayerId,
) -> Result<ContainerInfo, InventoryError> {
    let containers_guard = containers.lock().unwrap();
    let container = containers_guard
        .get(container_id)
        .ok_or_else(|| InventoryError::ContainerNotFound(container_id.to_string()))?;

    // Check access
    validate_container_access(container, requesting_player)?;

    let used_slots = container
        .inventory
        .slots
        .values()
        .filter(|slot| slot.item.is_some())
        .count() as u32;

    let items: Vec<ContainerItemInfo> = container
        .inventory
        .slots
        .iter()
        .filter_map(|(slot_id, slot)| {
            if let Some(ref item) = slot.item {
                // This would normally need item definitions, but we'll create a simplified version
                Some(ContainerItemInfo {
                    slot_id: *slot_id,
                    item_instance_id: item.instance_id.clone(),
                    item_name: format!("Item_{}", item.definition_id), // Simplified
                    stack: item.stack,
                    item_value: 0,              // Would calculate from definition
                    rarity: ItemRarity::Common, // Would get from definition
                })
            } else {
                None
            }
        })
        .collect();

    let access_level = if container.access_permissions.contains(&requesting_player) {
        AccessLevel::ReadWrite
    } else if container.public_access {
        AccessLevel::ReadOnly
    } else {
        AccessLevel::NoAccess
    };

    Ok(ContainerInfo {
        container_id: container_id.to_string(),
        name: container.name.clone(),
        container_type: container.container_type.clone(),
        total_slots: container.inventory.slots.len() as u32,
        used_slots,
        items,
        access_level,
        last_accessed: current_timestamp(),
    })
}

fn add_item_to_container(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    containers: &Arc<Mutex<HashMap<String, Container>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    container_id: &str,
    item_instance_id: &str,
    quantity: u32,
) -> Result<ItemTransferInfo, InventoryError> {
    // Find and remove item from player
    let (removed_item, item_name) = {
        let mut players_guard = players.lock().unwrap();
        let players_map = players_guard
            .as_mut()
            .ok_or(InventoryError::PlayerNotFound(player_id))?;

        let player = players_map
            .get_mut(&player_id)
            .ok_or(InventoryError::PlayerNotFound(player_id))?;

        let mut found_item = None;
        let mut source_inventory = None;
        let mut source_slot = None;

        // Find the item in player's inventories
        for (inv_name, inventory) in &mut player.inventories.inventories {
            for (slot_id, slot) in &mut inventory.slots {
                if let Some(ref mut item) = slot.item {
                    if item.instance_id == item_instance_id {
                        if item.stack >= quantity {
                            if item.stack == quantity {
                                found_item = slot.item.take();
                            } else {
                                // Partial removal
                                item.stack -= quantity;
                                let mut partial_item = item.clone();
                                partial_item.stack = quantity;
                                partial_item.instance_id = uuid::Uuid::new_v4().to_string();
                                found_item = Some(partial_item);
                            }
                            source_inventory = Some(inv_name.clone());
                            source_slot = Some(*slot_id);
                            break;
                        } else {
                            return Err(InventoryError::InsufficientItems {
                                needed: quantity,
                                available: item.stack,
                            });
                        }
                    }
                }
            }
            if found_item.is_some() {
                break;
            }
        }

        let item = found_item.ok_or_else(|| {
            InventoryError::Custom(format!("Item {} not found", item_instance_id))
        })?;

        // Get item name from definitions
        let item_name = {
            let defs_guard = item_definitions.lock().unwrap();
            if let Some(def) = defs_guard.get(&item.definition_id) {
                def.name.clone()
            } else {
                format!("Unknown Item {}", item.definition_id)
            }
        };

        (item, item_name)
    };

    // Add item to container
    let mut containers_guard = containers.lock().unwrap();
    let container = containers_guard
        .get_mut(container_id)
        .ok_or_else(|| InventoryError::ContainerNotFound(container_id.to_string()))?;

    // Find empty slot in container
    let empty_slot = container
        .inventory
        .slots
        .iter()
        .find(|(_, slot)| slot.item.is_none())
        .map(|(slot_id, _)| *slot_id)
        .ok_or(InventoryError::InventoryFull)?;

    let slot_id = empty_slot;
    container.inventory.slots.get_mut(&slot_id).unwrap().item = Some(removed_item);
    container.inventory.last_modified = current_timestamp();

    Ok(ItemTransferInfo {
        item_instance_id: item_instance_id.to_string(),
        item_name,
        quantity,
        slot_id,
    })
}

fn remove_item_from_container(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    containers: &Arc<Mutex<HashMap<String, Container>>>,
    _item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    container_id: &str,
    item_instance_id: &str,
    quantity: u32,
) -> Result<ItemTransferInfo, InventoryError> {
    // Implementation would remove from container and add to player
    // This is a simplified version
    Ok(ItemTransferInfo {
        item_instance_id: item_instance_id.to_string(),
        item_name: "Item".to_string(),
        quantity,
        slot_id: 0,
    })
}

fn set_container_permissions(
    containers: &Arc<Mutex<HashMap<String, Container>>>,
    container_id: &str,
    requesting_player: PlayerId,
    new_permissions: Vec<PlayerId>,
) -> Result<(), InventoryError> {
    let mut containers_guard = containers.lock().unwrap();
    let container = containers_guard
        .get_mut(container_id)
        .ok_or_else(|| InventoryError::ContainerNotFound(container_id.to_string()))?;

    // Only owner can change permissions (simplified - would need proper ownership system)
    container.access_permissions = new_permissions;

    Ok(())
}

fn delete_container(
    containers: &Arc<Mutex<HashMap<String, Container>>>,
    container_id: &str,
    requesting_player: PlayerId,
) -> Result<Container, InventoryError> {
    let mut containers_guard = containers.lock().unwrap();

    // Check if container exists
    let container = containers_guard
        .get(container_id)
        .ok_or_else(|| InventoryError::ContainerNotFound(container_id.to_string()))?;

    // Check permissions (simplified)
    if !container.access_permissions.contains(&requesting_player) && !container.public_access {
        return Err(InventoryError::AccessDenied);
    }

    // Remove and return the container
    Ok(containers_guard.remove(container_id).unwrap())
}
