mod handlers;
mod types;

use handlers::*;
use types::*;

impl GuildSystem {
    pub fn new() -> Self {
        println!("📝 GuildPlugin: Initializing comprehensive chat management system...");
        
        let now = Utc::now();
        
        Self {
            chat: None,
        }
    }
}

#[async_trait]
impl SimplePlugin for GuildSystem {
    fn name(&self) -> &str {
        "guild_comms"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    async fn register_handlers(&mut self, events: Arc<EventSystem>) -> Result<(), PluginError> {
        {
            events
                .on_plugin(
                    "GuildComms",
                    "Chat",
                    move |json_event: serde_json::Value| {
                        let event: GuildSystem =
                            serde_json::from_value(json_event).expect("Failed to read json");

                        println!("Chat message received!: {:?}", event);

                        Ok(())
                    },
                )
                .await
                .unwrap()
        }

        Ok(())
    }
}

create_simple_plugin!(GuildSystem);
