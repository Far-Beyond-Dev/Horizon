mod types;

use types::*;

impl House {
    pub fn new() -> Self {
        println!("🏠 HousingPlugin: Initializing housing management system...");

        Self {
            house_id: None,
            owner_id: None,
            house_name: None,
            dimensions: None,
            rooms: None,
            location: None,
            created_at: None,
            last_modified: None,
        }
    }
}

#[async_trait]
impl SimplePlugin for House {
    fn name(&self) -> &str {
        "housing"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    async fn register_handlers(&mut self, events: Arc<EventSystem>) -> Result<(), PluginError> {
        println!("🏠 HousingPlugin: Registering event handlers...");

        {
            events
                .on_plugin(
                    "Housing",
                    "CreateHouse",
                    move |json_event: serde_json::Value| {
                        let event: House =
                            serde_json::from_value(json_event).expect("Failed to read json");

                        println!("New house created!: {:?}", event);

                        Ok(())
                    },
                )
                .await
                .unwrap()
        }

        {
            events
                .on_plugin(
                    "Housing",
                    "UpdateHouse",
                    move |json_event: serde_json::Value| {
                        let event: House =
                            serde_json::from_value(json_event).expect("Failed to read json");

                        println!("House updated!: {:?}", event);

                        Ok(())
                    },
                )
                .await
                .unwrap()
        }

        {
            events
                .on_plugin(
                    "Housing",
                    "DeleteHouse",
                    move |json_event: serde_json::Value| {
                        let event: House =
                            serde_json::from_value(json_event).expect("Failed to read json");

                        println!("House deleted!: {:?}", event);

                        Ok(())
                    },
                )
                .await
                .unwrap()
        }

        {
            events
                .on_plugin(
                    "Housing",
                    "AddRoom",
                    move |json_event: serde_json::Value| {
                        let event: Room =
                            serde_json::from_value(json_event).expect("Failed to read json");

                        println!("New room added!: {:?}", event);

                        Ok(())
                    },
                )
                .await
                .unwrap()
        }

        println!("🏠 HousingPlugin: ✅ All handlers registered successfully!");
        Ok(())
    }

    async fn on_init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        context.log(
            LogLevel::Info,
            "🏠 HousingPlugin: Starting up! Ready to manage houses!",
        );

        println!("🏠 HousingPlugin: ✅ Initialization complete!");
        Ok(())
    }

    async fn on_shutdown(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        context.log(
            LogLevel::Info,
            "🏠 HousingPlugin: Shutting down housing management system!",
        );

        println!("🏠 HousingPlugin: ✅ Shutdown complete!");
        Ok(())
    }
}

create_simple_plugin!(House);
