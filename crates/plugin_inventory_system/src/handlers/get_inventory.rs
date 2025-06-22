use crate::types::*;

pub fn get_inventory_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    events: &Arc<EventSystem>,
    event: GetInventoryRequest,
) {
    let inventory = get_player_inventory(players, event.id);

    if let Some(inv) = inventory {
        println!("📋 Retrieved inventory for player {:?}: {} unique items", event.id, inv.len());

        // Emit inventory data
        let _ = events.emit_plugin(
            "InventorySystem",
            "inventory_data",
            &serde_json::json!({
                "player_id": event.id,
                "inventory": inv,
                "total_unique_items": inv.len(),
                "timestamp": current_timestamp()
            }),
        );
    } else {
        println!("❌ No inventory found for player {:?}", event.id);
    }
}

fn get_player_inventory(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_id: PlayerId,
) -> Option<Vec<InventorySlot>> {
    let players_guard = players.lock().unwrap();
    if let Some(ref players_map) = *players_guard {
        if let Some(player) = players_map.get(&player_id) {
            return Some(player.inventory.values().cloned().collect());
        }
    }
    None
}