use crate::types::*;

pub fn pickup_item_handler(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_count: &Arc<Mutex<Option<u32>>>,
    events: &Arc<EventSystem>,
    event: PickupItemRequest,
) {
    let success = add_item_to_player(
        players,
        player_count,
        event.id,
        event.item_id as u64,
        event.item_count,
    );

    if success {
        println!(
            "📦 Player {:?} picked up {} of item {}",
            event.id, event.item_count, event.item_id
        );

        // Get player info for logging
        let players_guard = players.lock().unwrap();
        if let Some(ref players_map) = *players_guard {
            if let Some(player) = players_map.get(&event.id) {
                println!(
                    "📊 Player {:?} now has {} total items in inventory",
                    event.id, player.item_count
                );
            }
        }

        // Emit item picked up event
        let _ = events.emit_plugin(
            "InventorySystem",
            "item_picked_up",
            &serde_json::json!({
                "player_id": event.id,
                "item_id": event.item_id,
                "amount": event.item_count,
                "timestamp": current_timestamp()
            }),
        );
    } else {
        println!(
            "❌ Failed to add item {} to player {:?}",
            event.item_id, event.id
        );
    }
}

fn ensure_players_initialized(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_count: &Arc<Mutex<Option<u32>>>,
) {
    let mut players_guard = players.lock().unwrap();
    if players_guard.is_none() {
        *players_guard = Some(HashMap::new());
        *player_count.lock().unwrap() = Some(0);
    }
}

fn add_item_to_player(
    players: &Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    player_count: &Arc<Mutex<Option<u32>>>,
    player_id: PlayerId,
    item_id: u64,
    amount: u32,
) -> bool {
    ensure_players_initialized(players, player_count);

    let mut players_guard = players.lock().unwrap();
    let players_map = players_guard.as_mut().unwrap();

    // Get or create player
    let player = players_map.entry(player_id).or_insert_with(|| {
        let mut count_guard = player_count.lock().unwrap();
        *count_guard = Some(count_guard.unwrap_or(0) + 1);
        Player {
            id: player_id,
            item_count: 0,
            inventory: HashMap::new(),
        }
    });

    // Check if player already has this item
    if let Some(existing_slot) = player.inventory.get_mut(&item_id) {
        // Stack with existing item
        existing_slot.stack += amount;
    } else {
        // Create new inventory slot
        player.inventory.insert(
            item_id,
            InventorySlot {
                item: item_id,
                stack: amount,
            },
        );
    }

    player.item_count += amount;
    true
}
