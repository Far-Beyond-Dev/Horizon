use crate::types::*;

pub fn consume_item_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    events: &Arc<EventSystem>,
    event: ConsumeItemRequest,
) {
    let success = remove_item_from_player(
        players,
        event.id,
        event.item_id as u64,
        event.amount,
    );

    if success {
        println!(
            "🍽️ Player {:?} consumed {} of item {}",
            event.id, event.amount, event.item_id
        );

        // Emit consumption event
        let _ = events.emit_plugin(
            "InventorySystem",
            "item_consumed",
            &serde_json::json!({
                "player_id": event.id,
                "item_id": event.item_id,
                "amount": event.amount,
                "timestamp": current_timestamp()
            }),
        );
    } else {
        println!(
            "❌ Player {:?} doesn't have enough {} to consume",
            event.id, event.item_id
        );
    }
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