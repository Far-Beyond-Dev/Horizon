use crate::types::*;

pub fn clear_inventory_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    events: &Arc<EventSystem>,
    event: ClearInventoryRequest,
) {
    let success = clear_player_inventory(players, event.id);

    if success {
        println!("🗑️ Cleared inventory for player {:?}", event.id);

        // Emit inventory cleared event
        let _ = events.emit_plugin(
            "InventorySystem",
            "inventory_cleared",
            &serde_json::json!({
                "player_id": event.id,
                "timestamp": current_timestamp()
            }),
        );
    } else {
        println!("❌ Failed to clear inventory for player {:?}", event.id);
    }
}

fn clear_player_inventory(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_id: PlayerId,
) -> bool {
    let mut players_guard = players.lock().unwrap();
    if let Some(ref mut players_map) = *players_guard {
        if let Some(player) = players_map.get_mut(&player_id) {
            player.inventory.clear();
            player.item_count = 0;
            return true;
        }
    }
    false
}