mod types;
mod handlers;

use types::*;
use handlers::*;

impl InventorySystem {
    pub fn new() -> Self {
        println!("📝 InventoryPlugin: Initializing inventory management system...");
        Self {
            players: Default::default(),
            player_count: Default::default(),
        }
    }
}

#[async_trait]
impl SimplePlugin for InventorySystem {
    fn name(&self) -> &str {
        "InventoryPlugin"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    async fn on_init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        context.log(LogLevel::Info, "📝 InventoryPlugin: Comprehensive inventory management activated!");

        let events = context.events();
        events
            .emit_plugin(
                "InventorySystem",
                "service_started",
                &serde_json::json!({
                    "service": "inventory_management",
                    "version": self.version(),
                    "start_time": current_timestamp(),
                    "features": ["item_pickup", "item_removal", "inventory_query", "item_transfer", "inventory_clear", "item_consumption"],
                    "message": "InventoryPlugin is now managing all inventory operations"
                }),
            )
            .await
            .map_err(|e| PluginError::InitializationFailed(e.to_string()))?;

        println!("📝 InventoryPlugin: ✅ Advanced Inventory Management Active");
        Ok(())
    }

    async fn register_handlers(&mut self, events: Arc<EventSystem>) -> Result<(), PluginError> {
        println!("📝 InventorySystem: Registering comprehensive inventory event handlers...");

        let players = self.players.clone();
        let player_count = self.player_count.clone();

        // PickupItem handler
        {
            let players = players.clone();
            let player_count = player_count.clone();
            let events_for_emit = events.clone();
            
            events.on_plugin("InventorySystem", "PickupItem", move |json_event: serde_json::Value| {
                let event: PickupItemRequest = serde_json::from_value(json_event)?;
                pickup_item_handler(&players, &player_count, &events_for_emit, event);
                Ok(())
            }).await.unwrap();
        }

        // DropItem handler
        {
            let players = players.clone();
            let events_for_emit = events.clone();
            
            events.on_plugin("InventorySystem", "DropItem", move |json_event: serde_json::Value| {
                let event: DropItemRequest = serde_json::from_value(json_event)?;
                drop_item_handler(&players, &events_for_emit, event);
                Ok(())
            }).await.unwrap();
        }

        // GetInventory handler
        {
            let players = players.clone();
            let events_for_emit = events.clone();
            
            events.on_plugin("InventorySystem", "GetInventory", move |json_event: serde_json::Value| {
                let event: GetInventoryRequest = serde_json::from_value(json_event)?;
                get_inventory_handler(&players, &events_for_emit, event);
                Ok(())
            }).await.unwrap();
        }

        // CheckItem handler
        {
            let players = players.clone();
            let events_for_emit = events.clone();
            
            events.on_plugin("InventorySystem", "CheckItem", move |json_event: serde_json::Value| {
                let event: CheckItemRequest = serde_json::from_value(json_event)?;
                check_item_handler(&players, &events_for_emit, event);
                Ok(())
            }).await.unwrap();
        }

        // TransferItem handler
        {
            let players = players.clone();
            let events_for_emit = events.clone();
            
            events.on_plugin("InventorySystem", "TransferItem", move |json_event: serde_json::Value| {
                let event: TransferItemRequest = serde_json::from_value(json_event)?;
                transfer_item_handler(&players, &events_for_emit, event);
                Ok(())
            }).await.unwrap();
        }

        // ConsumeItem handler
        {
            let players = players.clone();
            let events_for_emit = events.clone();
            
            events.on_plugin("InventorySystem", "ConsumeItem", move |json_event: serde_json::Value| {
                let event: ConsumeItemRequest = serde_json::from_value(json_event)?;
                consume_item_handler(&players, &events_for_emit, event);
                Ok(())
            }).await.unwrap();
        }

        // ClearInventory handler
        {
            let players = players.clone();
            let events_for_emit = events.clone();
            
            events.on_plugin("InventorySystem", "ClearInventory", move |json_event: serde_json::Value| {
                let event: ClearInventoryRequest = serde_json::from_value(json_event)?;
                clear_inventory_handler(&players, &events_for_emit, event);
                Ok(())
            }).await.unwrap();
        }

        // SetupInventory handler
        events.on_plugin("InventorySystem", "SetupInventory", |json_event: serde_json::Value| {
            let event: InventorySettingRequest = serde_json::from_value(json_event)?;
            setup_inventory_handler(event);
            Ok(())
        }).await.unwrap();

        println!("InventorySystem: All inventory event handlers registered! 🎮");
        Ok(())
    }
}

create_simple_plugin!(InventorySystem);