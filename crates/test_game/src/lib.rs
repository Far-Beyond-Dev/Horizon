use async_trait::async_trait;
use serde_json::json;
use serde_json::Value;
use shared_types::*;
use std::collections::HashMap;

pub struct TestGamePlugin {
    name: &'static str,
    version: &'static str,
    // Optionally keep recipes locally as JSON values or strings
    recipes_json: Vec<Value>,
    initialized: bool,
}

impl TestGamePlugin {
    pub fn new() -> Self {
        Self {
            name: env!("CARGO_PKG_NAME"),
            version: env!("CARGO_PKG_VERSION"),
            recipes_json: Vec::new(),
            initialized: false,
        }
    }

    // Load recipes JSON - for demo, hardcoded string parsed to serde_json::Value
    fn load_recipes_json(&mut self) {
        // Example single recipe JSON matching RecipeSmithMessage::AddOrUpdateRecipe
        let recipe_json_str = r#"
        {
            "type": "AddOrUpdateRecipe",
            "data": {
                "recipe": {
                    "id": [1],
                    "name": "Test Wooden Pickaxe",
                    "ingredients": [
                        { "item_id": [101], "quantity": 3, "name": "Wood" },
                        { "item_id": [102], "quantity": 2, "name": "Stick" }
                    ],
                    "main_product": { "item_id": [201], "quantity": 1, "name": "Wooden Pickaxe" },
                    "byproducts": []
                }
            }
        }
        "#;

        if let Ok(val) = serde_json::from_str(recipe_json_str) {
            self.recipes_json.push(val);
        }
    }
}

#[async_trait]
impl Plugin for TestGamePlugin {
    fn name(&self) -> &'static str { self.name }
    fn version(&self) -> &'static str { self.version }

    async fn initialize(&mut self, context: &dyn ServerContext) -> Result<(), PluginError> {
        if self.initialized {
            return Err(PluginError::InitializationFailed("Already initialized".to_string()));
        }
        context.log(LogLevel::Info, "TestGamePlugin initializing and loading recipes JSON");

        self.load_recipes_json();

        // Send all loaded recipes to RecipeSmith via CustomMessage events
        for recipe_msg_json in &self.recipes_json {
            println!("Sending recipe JSON: {}", recipe_msg_json);
            let event = CoreEvent::CustomMessage { data: recipe_msg_json.clone() };
            context.emit_event(EventNamespace::new("core"), Box::new(event)).await
                .map_err(|e| PluginError::ExecutionError(format!("Failed to emit recipe JSON event: {}", e)))?;
            context.log(LogLevel::Info, "Sent AddOrUpdateRecipe JSON message to RecipeSmith");
        }

        self.initialized = true;
        Ok(())
    }

    async fn handle_event(&mut self, _event_id: &EventId, _event: &dyn GameEvent, _context: &dyn ServerContext) -> Result<(), PluginError> {
        // For simplicity, this test plugin does not handle incoming events
        Ok(())
    }

    fn subscribed_events(&self) -> Vec<EventId> {
        vec![
            // Listen for any events you want; here we just listen to none or some core events if needed
            EventId::new(EventNamespace::new("core"), "plugin_initialized"),
        ]
    }

    async fn shutdown(&mut self, context: &dyn ServerContext) -> Result<(), PluginError> {
        if !self.initialized {
            return Ok(());
        }
        context.log(LogLevel::Info, "TestGamePlugin shutting down");
        self.initialized = false;
        Ok(())
    }
}

impl Default for TestGamePlugin {
    fn default() -> Self { Self::new() }
}

/// FFI exports
#[no_mangle]
pub unsafe extern "C" fn create_plugin() -> *mut dyn Plugin {
    let plugin = Box::new(TestGamePlugin::new());
    Box::into_raw(plugin)
}

#[no_mangle]
pub unsafe extern "C" fn destroy_plugin(plugin: *mut dyn Plugin) {
    if !plugin.is_null() {
        let _ = Box::from_raw(plugin);
    }
}
