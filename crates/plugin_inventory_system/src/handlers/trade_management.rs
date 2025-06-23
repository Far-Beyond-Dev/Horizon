use crate::handlers::inventory_validation::*;
use crate::types::*;

pub fn create_trade_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    active_trades: &Arc<Mutex<HashMap<String, TradeOffer>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    config: &Arc<Mutex<InventoryConfig>>,
    events: &Arc<EventSystem>,
    event: CreateTradeRequest,
) {
    let result = create_trade_offer(
        players,
        active_trades,
        item_definitions,
        config,
        event.from_player,
        event.to_player,
        event.offered_items,
        event.requested_items,
        event.expires_in_seconds,
    );

    match result {
        Ok(trade_offer) => {
            println!(
                "🤝 Trade created: {} offers {} items to {} for {} items",
                trade_offer.from_player,
                trade_offer.offered_items.len(),
                trade_offer.to_player,
                trade_offer.requested_items.len()
            );

            // Emit trade created event
            let _ = events.emit_plugin(
                "InventorySystem",
                "trade_created",
                &serde_json::json!({
                    "trade_id": trade_offer.trade_id,
                    "from_player": trade_offer.from_player,
                    "to_player": trade_offer.to_player,
                    "offered_items": trade_offer.offered_items,
                    "requested_items": trade_offer.requested_items,
                    "expires_at": trade_offer.expires_at,
                    "timestamp": current_timestamp()
                }),
            );

            // Notify target player
            let _ = events.emit_plugin(
                "NotificationSystem",
                "trade_request",
                &serde_json::json!({
                    "target_player": trade_offer.to_player,
                    "from_player": trade_offer.from_player,
                    "trade_id": trade_offer.trade_id,
                    "message": format!("You have received a trade offer from player {:?}", trade_offer.from_player)
                }),
            );
        }
        Err(e) => {
            println!(
                "❌ Failed to create trade between {:?} and {:?}: {}",
                event.from_player, event.to_player, e
            );

            let _ = events.emit_plugin(
                "InventorySystem",
                "trade_creation_failed",
                &serde_json::json!({
                    "from_player": event.from_player,
                    "to_player": event.to_player,
                    "reason": e.to_string(),
                    "timestamp": current_timestamp()
                }),
            );
        }
    }
}

pub fn trade_action_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    active_trades: &Arc<Mutex<HashMap<String, TradeOffer>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    events: &Arc<EventSystem>,
    event: TradeActionRequest,
) {
    let result = handle_trade_action(
        players,
        active_trades,
        item_definitions,
        &event.trade_id,
        event.player_id,
        event.clone().action,
    );

    match result {
        Ok(trade_result) => match trade_result {
            TradeResult::Accepted(trade) => {
                println!("✅ Trade {} accepted and completed", trade.trade_id);

                let _ = events.emit_plugin(
                    "InventorySystem",
                    "trade_completed",
                    &serde_json::json!({
                        "trade_id": trade.trade_id,
                        "from_player": trade.from_player,
                        "to_player": trade.to_player,
                        "completed_at": current_timestamp()
                    }),
                );
            }
            TradeResult::Declined(trade_id) => {
                println!("❌ Trade {} declined", trade_id);

                let _ = events.emit_plugin(
                    "InventorySystem",
                    "trade_declined",
                    &serde_json::json!({
                        "trade_id": trade_id,
                        "declined_by": event.player_id,
                        "timestamp": current_timestamp()
                    }),
                );
            }
            TradeResult::Cancelled(trade_id) => {
                println!("🚫 Trade {} cancelled", trade_id);

                let _ = events.emit_plugin(
                    "InventorySystem",
                    "trade_cancelled",
                    &serde_json::json!({
                        "trade_id": trade_id,
                        "cancelled_by": event.player_id,
                        "timestamp": current_timestamp()
                    }),
                );
            }
            TradeResult::Modified(trade) => {
                println!("🔄 Trade {} modified", trade.trade_id);

                let _ = events.emit_plugin(
                    "InventorySystem",
                    "trade_modified",
                    &serde_json::json!({
                        "trade_id": trade.trade_id,
                        "modified_by": event.player_id,
                        "new_offer": trade.offered_items,
                        "timestamp": current_timestamp()
                    }),
                );
            }
        },
        Err(e) => {
            println!(
                "❌ Trade action failed for player {:?} on trade {}: {}",
                event.player_id, event.trade_id, e
            );

            let _ = events.emit_plugin(
                "InventorySystem",
                "trade_action_failed",
                &serde_json::json!({
                    "trade_id": event.trade_id,
                    "player_id": event.player_id,
                    "action": format!("{:?}", event.action),
                    "reason": e.to_string(),
                    "timestamp": current_timestamp()
                }),
            );
        }
    }
}

pub fn get_trades_handler(
    active_trades: &Arc<Mutex<HashMap<String, TradeOffer>>>,
    events: &Arc<EventSystem>,
    player_id: PlayerId,
) {
    let trades = get_player_trades(active_trades, player_id);

    println!(
        "📋 Retrieved {} active trades for player {:?}",
        trades.len(),
        player_id
    );

    let _ = events.emit_plugin(
        "InventorySystem",
        "player_trades",
        &serde_json::json!({
            "player_id": player_id,
            "trades": trades,
            "count": trades.len(),
            "timestamp": current_timestamp()
        }),
    );
}

#[derive(Debug)]
enum TradeResult {
    Accepted(TradeOffer),
    Declined(String),
    Cancelled(String),
    Modified(TradeOffer),
}

fn create_trade_offer(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    active_trades: &Arc<Mutex<HashMap<String, TradeOffer>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    config: &Arc<Mutex<InventoryConfig>>,
    from_player: PlayerId,
    to_player: PlayerId,
    offered_items: Vec<TradeItem>,
    requested_items: Vec<TradeItem>,
    expires_in_seconds: Option<u64>,
) -> Result<TradeOffer, InventoryError> {
    // Validate both players exist
    {
        let players_guard = players.lock().unwrap();
        let players_map = players_guard
            .as_ref()
            .ok_or(InventoryError::PlayerNotFound(from_player))?;

        players_map
            .get(&from_player)
            .ok_or(InventoryError::PlayerNotFound(from_player))?;
        players_map
            .get(&to_player)
            .ok_or(InventoryError::PlayerNotFound(to_player))?;
    }

    // Validate offered items exist and are tradeable
    validate_trade_items(players, item_definitions, from_player, &offered_items)?;

    // Create trade offer
    let trade_id = uuid::Uuid::new_v4().to_string();
    let config_guard = config.lock().unwrap();
    let expires_at = expires_in_seconds
        .map(|seconds| current_timestamp() + seconds.min(config_guard.trade_timeout_seconds));

    let trade_offer = TradeOffer {
        trade_id: trade_id.clone(),
        from_player,
        to_player,
        offered_items,
        requested_items,
        status: TradeStatus::Pending,
        created_at: current_timestamp(),
        expires_at,
    };

    // Store the trade
    let mut trades_guard = active_trades.lock().unwrap();
    trades_guard.insert(trade_id, trade_offer.clone());

    Ok(trade_offer)
}

fn handle_trade_action(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    active_trades: &Arc<Mutex<HashMap<String, TradeOffer>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    trade_id: &str,
    player_id: PlayerId,
    action: TradeAction,
) -> Result<TradeResult, InventoryError> {
    let mut trades_guard = active_trades.lock().unwrap();
    let trade = trades_guard
        .get_mut(trade_id)
        .ok_or_else(|| InventoryError::TradeNotFound(trade_id.to_string()))?;

    // Check if player is involved in this trade
    if trade.from_player != player_id && trade.to_player != player_id {
        return Err(InventoryError::AccessDenied);
    }

    // Check if trade is still valid
    if trade.status != TradeStatus::Pending {
        return Err(InventoryError::Custom(
            "Trade is no longer pending".to_string(),
        ));
    }

    // Check expiration
    if let Some(expires_at) = trade.expires_at {
        if current_timestamp() > expires_at {
            trade.status = TradeStatus::Expired;
            return Err(InventoryError::Custom("Trade has expired".to_string()));
        }
    }

    match action {
        TradeAction::Accept => {
            if trade.to_player != player_id {
                return Err(InventoryError::AccessDenied);
            }

            // Execute the trade
            execute_trade(players, item_definitions, trade)?;
            trade.status = TradeStatus::Completed;

            let completed_trade = trade.clone();
            trades_guard.remove(trade_id);
            Ok(TradeResult::Accepted(completed_trade))
        }
        TradeAction::Decline => {
            if trade.to_player != player_id {
                return Err(InventoryError::AccessDenied);
            }

            trade.status = TradeStatus::Declined;
            let trade_id = trade.trade_id.clone();
            trades_guard.remove(&trade_id);
            Ok(TradeResult::Declined(trade_id))
        }
        TradeAction::Cancel => {
            if trade.from_player != player_id {
                return Err(InventoryError::AccessDenied);
            }

            trade.status = TradeStatus::Cancelled;
            let trade_id = trade.trade_id.clone();
            trades_guard.remove(&trade_id);
            Ok(TradeResult::Cancelled(trade_id))
        }
        TradeAction::ModifyOffer(new_items) => {
            if trade.from_player != player_id {
                return Err(InventoryError::AccessDenied);
            }

            // Validate new items
            validate_trade_items(players, item_definitions, player_id, &new_items)?;
            trade.offered_items = new_items;

            Ok(TradeResult::Modified(trade.clone()))
        }
    }
}

fn validate_trade_items(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    items: &[TradeItem],
) -> Result<(), InventoryError> {
    let players_guard = players.lock().unwrap();
    let players_map = players_guard
        .as_ref()
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let player = players_map
        .get(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let defs_guard = item_definitions.lock().unwrap();

    for trade_item in items {
        // Check if item definition exists
        let item_def = defs_guard
            .get(&trade_item.item_instance.definition_id)
            .ok_or(InventoryError::ItemNotFound(
                trade_item.item_instance.definition_id,
            ))?;

        // Check if item is tradeable
        if !item_def.tradeable {
            return Err(InventoryError::ItemNotTradeable);
        }

        // Check if player actually has this item
        let mut found = false;
        for inventory in player.inventories.inventories.values() {
            for slot in inventory.slots.values() {
                if let Some(ref item) = slot.item {
                    if item.instance_id == trade_item.item_instance.instance_id {
                        if item.stack >= trade_item.quantity {
                            found = true;
                            break;
                        }
                    }
                }
            }
            if found {
                break;
            }
        }

        if !found {
            return Err(InventoryError::InsufficientItems {
                needed: trade_item.quantity,
                available: 0,
            });
        }
    }

    Ok(())
}

fn execute_trade(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    trade: &TradeOffer,
) -> Result<(), InventoryError> {
    let mut players_guard = players.lock().unwrap();
    let players_map = players_guard
        .as_mut()
        .ok_or(InventoryError::PlayerNotFound(trade.from_player))?;

    // Remove offered items from from_player
    for trade_item in &trade.offered_items {
        remove_item_instance_from_player(
            players_map,
            trade.from_player,
            &trade_item.item_instance.instance_id,
            trade_item.quantity,
        )?;
    }

    // Add offered items to to_player
    for trade_item in &trade.offered_items {
        add_item_instance_to_player(
            players_map,
            item_definitions,
            trade.to_player,
            &trade_item.item_instance,
            trade_item.quantity,
            None,
        )?;
    }

    // Handle requested items (if any)
    for trade_item in &trade.requested_items {
        remove_item_instance_from_player(
            players_map,
            trade.to_player,
            &trade_item.item_instance.instance_id,
            trade_item.quantity,
        )?;

        add_item_instance_to_player(
            players_map,
            item_definitions,
            trade.from_player,
            &trade_item.item_instance,
            trade_item.quantity,
            None,
        )?;
    }

    Ok(())
}

fn get_player_trades(
    active_trades: &Arc<Mutex<HashMap<String, TradeOffer>>>,
    player_id: PlayerId,
) -> Vec<TradeOffer> {
    let trades_guard = active_trades.lock().unwrap();
    trades_guard
        .values()
        .filter(|trade| trade.from_player == player_id || trade.to_player == player_id)
        .cloned()
        .collect()
}

fn remove_item_instance_from_player(
    players_map: &mut HashMap<PlayerId, Player>,
    player_id: PlayerId,
    instance_id: &str,
    quantity: u32,
) -> Result<(), InventoryError> {
    let player = players_map
        .get_mut(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    for inventory in player.inventories.inventories.values_mut() {
        for slot in inventory.slots.values_mut() {
            if let Some(ref mut item) = slot.item {
                if item.instance_id == instance_id {
                    if item.stack >= quantity {
                        item.stack -= quantity;
                        if item.stack == 0 {
                            slot.item = None;
                        }
                        return Ok(());
                    } else {
                        return Err(InventoryError::InsufficientItems {
                            needed: quantity,
                            available: item.stack,
                        });
                    }
                }
            }
        }
    }

    Err(InventoryError::Custom(format!(
        "Item instance {} not found",
        instance_id
    )))
}

fn add_item_instance_to_player(
    players_map: &mut HashMap<PlayerId, Player>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    player_id: PlayerId,
    item_instance: &ItemInstance,
    quantity: u32,
    target_inventory: Option<String>,
) -> Result<(), InventoryError> {
    let player = players_map
        .get_mut(&player_id)
        .ok_or(InventoryError::PlayerNotFound(player_id))?;

    let inventory_name = target_inventory.unwrap_or_else(|| "general".to_string());
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

    // Create new instance for the receiving player
    let mut new_instance = item_instance.clone();
    new_instance.instance_id = uuid::Uuid::new_v4().to_string();
    new_instance.stack = quantity;
    new_instance.acquired_timestamp = current_timestamp();

    // Update weight
    let defs_guard = item_definitions.lock().unwrap();
    if let Some(item_def) = defs_guard.get(&item_instance.definition_id) {
        inventory.current_weight += item_def.weight * quantity as f32;
    }

    inventory.slots.get_mut(&empty_slot).unwrap().item = Some(new_instance);
    inventory.last_modified = current_timestamp();
    player.last_activity = current_timestamp();

    Ok(())
}
