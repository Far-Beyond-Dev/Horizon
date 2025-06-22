mod types;
use types::*;

impl InventorySystem {
    fn new() -> Self {
        Self {
            players: Arc::new(Mutex::new(None)),
            player_count: Arc::new(Mutex::new(None)),
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
        Self::ensure_players_initialized(players, player_count);

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

    fn SetupInventory(slot_count: Option<u32>, inventory_count: Option<u8>) -> bool {
        let inventory_settings = InventorySettingRequest {
            slot_count,
            inventory_count,
        };

        match serde_json::to_string(&inventory_settings) {
            Ok(json_string) => {
                println!("Inventory settings: {}", json_string);
                true
            }
            Err(e) => {
                println!("Failed to serialize inventory settings: {}", e);
                false
            }
        }
    }
}

#[async_trait]
impl SimplePlugin for InventorySystem {
    #[doc = " Plugin name"]
    fn name(&self) -> &str {
        "InventoryPlugin"
    }

    #[doc = " Plugin version"]
    fn version(&self) -> &str {
        "0.0.1"
    }

    async fn on_init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        context.log(
            LogLevel::Info,
            "📝 InventoryPlugin: Comprehensive event logging activated!",
        );

        // Announce our logging service to other plugins
        let events = context.events();
        events
            .emit_plugin(
                "InventorySystem",
                "service_started",
                &serde_json::json!({
                    "service": "event_logging",
                    "version": self.version(),
                    "start_time": current_timestamp(),
                    "message": "InventoryPlugin is Feining RN"
                }),
            )
            .await
            .map_err(|e| PluginError::InitializationFailed(e.to_string()))?;

        println!("📝 InventoryPlugin: ✅ Monitoring Inventory");
        Ok(())
    }

    async fn register_handlers(&mut self, events: Arc<EventSystem>) -> Result<(), PluginError> {
        println!("📝 InventorySystem: Registering comprehensive event logging...");

        // Clone the Arc references to move into the closure
        let players = self.players.clone();
        let player_count = self.player_count.clone();

        events
            .on_plugin(
                "InventorySystem",
                "PickupItem",
                move |json_event: serde_json::Value| {
                    let event: PickupItemRequest = serde_json::from_value(json_event.clone())
                        .expect("Invalid json for PickupItem");
                    // Process the pickup request
                    let success = Self::add_item_to_player(
                        &players,
                        &player_count,
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
                    } else {
                        println!(
                            "❌ Failed to add item {} to player {:?}",
                            event.item_id, event.id
                        );
                    }

                    println!("InventorySystem: Message received: {:?}", event);

                    Ok(())
                },
            )
            .await
            .unwrap();

        events
            .on_plugin(
                "InventorySystem",
                "SetupInventory",
                |json_event: serde_json::Value| {
                    let event: InventorySettingRequest = serde_json::from_value(json_event.clone())
                        .expect("Invalid json for SetupInventory");

                    let success = Self::SetupInventory(event.slot_count, event.inventory_count);

                    if success {
                        println!(
                            "🎒 Inventory setup complete: {:?} slots, {:?} inventories",
                            event.slot_count, event.inventory_count
                        );
                    } else {
                        println!("❌ Failed to setup inventory");
                    }

                    println!("InventorySystem: Message received: {:?}", event);

                    Ok(())
                },
            )
            .await
            .unwrap();

        println!("InventorySystem: All events registered!");

        Ok(())
    }
}

create_simple_plugin!(InventorySystem);
