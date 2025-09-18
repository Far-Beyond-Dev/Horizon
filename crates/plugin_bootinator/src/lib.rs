//! Bootinator is a plugin used for banning unwanted people

use async_trait::async_trait;
use std::sync::Arc;
use serde::{Deserialize, Serialize};

use horizon_event_system::{
    create_simple_plugin, EventSystem, PlayerId, PluginError, ServerContext, SimplePlugin
};

pub struct BootinatorPlugin {
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    id: PlayerId,
    name: String,
    reason: String,
}

impl BootinatorPlugin {
    pub fn new() -> Self {
        Self {
            name: "bootinator".to_string(),
        }
    }
}

impl Default for BootinatorPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SimplePlugin for BootinatorPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    async fn register_handlers(
        &mut self,
        events: Arc<EventSystem>,
        _context: Arc<dyn ServerContext>,
    ) -> Result<(), PluginError> {
        println!("📝 BootinatorPlugin: Registering event handlers...");
        let _ = events;
        Ok(())
    }

    async fn on_init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        println!("⚙️ BootinatorPlugin: Initializing with server context");
        let context_clone = context.clone();
        Ok(())
    }

    async fn on_shutdown(&mut self, _context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        println!("🛑 BootinatorPlugin: Shutting down");
        let _ = _context;
        Ok(())
    }
}

create_simple_plugin!(BootinatorPlugin);