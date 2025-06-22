use crate::types::*;

pub fn transfer_item_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    events: &Arc<EventSystem>,
    event: TransferItemRequest,
) {
    let success = transfer_item_between_players(
        players,
        event.from_player,
        event.to_player,
        event.item_id as u64,
        event.amount,
    );

    if success {
        println!(
            "🔄 Transferred {} of item {} from {:?} to {:?}",
            event.amount, event.item_id, event.from_player, event.to_player
        );

        // Emit transfer success
        let _ = events.emit_plugin(
            "InventorySystem",
            "item_transferred",
            &serde_json::json!({
                "from_player": event.from_player,
                "to_player": event.to_player,
                "item_id": event.item_id,
                "amount": event.amount,
                "timestamp": current_timestamp()
            }),
        );
    } else {
        println!(
            "❌ Failed to transfer {} of item {} from {:?} to {:?}",
            event.amount, event.item_id, event.from_player, event.to_player
        );
    }
}

fn check_player_has_item(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_id: PlayerId,
    item_id: u64,
    required_amount: u32,
) -> bool {
    let players_guard = players.lock().unwrap();
    if let Some(ref players_map) = *players_guard {
        if let Some(player) = players_map.get(&player_id) {
            if let Some(slot) = player.inventory.get(&item_id) {
                return slot.stack >= required_amount;
            }
        }
    }
    false
}

fn remove_item_from_player(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_id: PlayerId,
    item_id: u64,
    amount: u32,
) -> bool {
    let mut players_guard = players.lock().unwrap();
    if let Some(ref mut players_map) = *players_guard {
        if let Some(player) = players_map.get_mut(&player_id) {
            if let Some(slot) = player.inventory.get_mut(&item_id) {
                if slot.stack >= amount {
                    slot.stack -= amount;
                    player.item_count -= amount;
                    
                    // Remove slot if empty
                    if slot.stack == 0 {
                        player.inventory.remove(&item_id);
                    }
                    return true;
                }
            }
        }
    }
    false
}

fn transfer_item_between_players(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    from_player: PlayerId,
    to_player: PlayerId,
    item_id: u64,
    amount: u32,
) -> bool {
    // Check if source player has enough items
    if !check_player_has_item(players, from_player, item_id, amount) {
        return false;
    }

    // Remove from source player
    if remove_item_from_player(players, from_player, item_id, amount) {
        // Add to destination player
        let mut players_guard = players.lock().unwrap();
        if let Some(ref mut players_map) = *players_guard {
            if let Some(to_player_obj) = players_map.get_mut(&to_player) {
                if let Some(existing_slot) = to_player_obj.inventory.get_mut(&item_id) {
                    existing_slot.stack += amount;
                } else {
                    to_player_obj.inventory.insert(
                        item_id,
                        InventorySlot {
                            item: item_id,
                            stack: amount,
                        },
                    );
                }
                to_player_obj.item_count += amount;
                return true;
            }
        }
    }
    false
}