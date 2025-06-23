use crate::types::*;

pub fn move_item_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    events: &Arc<EventSystem>,
    event: MoveItemRequest,
) {
    let success = move_item_in_player_inventory(
        players,
        event.player_id,
        event.original_slot,
        event.new_slot,
    );

    if success {
        info!(
            "🔄 Player {:?} moved item from slot {} to slot {}",
            event.player_id, event.original_slot, event.new_slot
        );

        // Emit item moved event
        let _ = events.emit_plugin(
            "InventorySystem",
            "item_moved",
            &serde_json::json!({
                "player_id": event.player_id,
                "original_slot": event.original_slot,
                "new_slot": event.new_slot,
                "timestamp": current_timestamp()
            }),
        );
    } else {
        info!(
            "❌ Failed to move item from slot {} to slot {} for player {:?}",
            event.original_slot, event.new_slot, event.player_id
        );
    }
}

fn move_item_in_player_inventory(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_id: PlayerId,
    original_slot: u8,
    new_slot: u8,
) -> bool {
    // Can't move to the same slot
    if original_slot == new_slot {
        return false;
    }

    let mut players_guard = players.lock().unwrap();
    if let Some(ref mut players_map) = *players_guard {
        if let Some(player) = players_map.get_mut(&player_id) {
            // Get the items from both slots
            let original_item = player.inventory.get(&(original_slot as u64)).cloned();
            let new_slot_item = player.inventory.get(&(new_slot as u64)).cloned();

            match (original_item, new_slot_item) {
                // Moving item to empty slot
                (Some(item), None) => {
                    player.inventory.remove(&(original_slot as u64));
                    player.inventory.insert(new_slot as u64, item);
                    return true;
                }
                // Swapping items between slots
                (Some(original_item), Some(new_item)) => {
                    player.inventory.insert(original_slot as u64, new_item);
                    player.inventory.insert(new_slot as u64, original_item);
                    return true;
                }
                // No item in original slot
                (None, _) => {
                    return false;
                }
            }
        }
    }
    false
}
