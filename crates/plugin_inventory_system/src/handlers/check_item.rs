use crate::types::*;

pub fn check_item_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    events: &Arc<EventSystem>,
    event: CheckItemRequest,
) {
    let has_item = check_player_has_item(
        players,
        event.id,
        event.item_id as u64,
        event.required_amount,
    );

    println!(
        "🔍 Player {:?} {} {} of item {}",
        event.id,
        if has_item { "has" } else { "doesn't have" },
        event.required_amount,
        event.item_id
    );

    // Emit check result
    let _ = events.emit_plugin(
        "InventorySystem",
        "item_check_result",
        &serde_json::json!({
            "player_id": event.id,
            "item_id": event.item_id,
            "required_amount": event.required_amount,
            "has_item": has_item,
            "timestamp": current_timestamp()
        }),
    );
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