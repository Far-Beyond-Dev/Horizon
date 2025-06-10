use async_trait::async_trait;
use serde_json::json;
use shared_types::*;
use std::collections::HashMap;

pub struct TestGamePlugin {
    name: &'static str,
    version: &'static str,
    recipes_json: Vec<String>, // Store as serialized JSON strings
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

    // Load recipes JSON - with correct format for RecipeId and ItemId
    fn load_recipes_json(&mut self) {
        // Correct format for RecipeSmith - notice the {"0": 1} format for IDs
        let recipe_json_str = r#"
        {
            "type": "AddOrUpdateRecipe",
            "data": {
                "recipe": {
                    "id": {"0": 1},
                    "name": "Test Wooden Pickaxe",
                    "ingredients": [
                        { "item_id": {"0": 101}, "quantity": 3, "name": "Wood" },
                        { "item_id": {"0": 102}, "quantity": 2, "name": "Stick" }
                    ],
                    "main_product": { "item_id": {"0": 201}, "quantity": 1, "name": "Wooden Pickaxe" },
                    "byproducts": []
                }
            }
        }
        "#;

        // Add a second recipe to demonstrate multiple recipes
        let recipe2_json_str = r#"
        {
            "type": "AddOrUpdateRecipe",
            "data": {
                "recipe": {
                    "id": {"0": 3},
                    "name": "Iron Sword",
                    "ingredients": [
                        { "item_id": {"0": 104}, "quantity": 2, "name": "Iron Ingot" },
                        { "item_id": {"0": 102}, "quantity": 1, "name": "Stick" }
                    ],
                    "main_product": { "item_id": {"0": 203}, "quantity": 1, "name": "Iron Sword" },
                    "byproducts": [
                        { "item_id": {"0": 302}, "quantity": 1, "name": "Iron Shavings" }
                    ]
                }
            }
        }
        "#;

        self.recipes_json.push(recipe_json_str.to_string());
        self.recipes_json.push(recipe2_json_str.to_string());
    }
    
    // Example of sending a crafting outcome
    async fn send_crafting_outcome(&self, context: &dyn ServerContext, player_id: &str) -> Result<(), PluginError> {
        let crafting_outcome = json!({
            "type": "RecordCraftingOutcome",
            "data": {
                "player_id": player_id,
                "outcome": {
                    "recipe_id": {"0": 1},
                    "main_product": {
                        "item_id": {"0": 201},
                        "quantity": 1,
                        "name": "Wooden Pickaxe"
                    },
                    "byproducts": [],
                    "timestamp": 1654321000
                }
            }
        }).to_string();
        
        let event = CoreEvent::CustomMessage { 
            data: serde_json::Value::String(crafting_outcome) 
        };
        
        context.emit_event(EventNamespace::new("core"), Box::new(event)).await
            .map_err(|e| PluginError::ExecutionError(format!("Failed to emit crafting outcome event: {}", e)))
    }
    
    // Example of requesting recipe info
    async fn request_recipe_info(&self, context: &dyn ServerContext, player_id: &str, recipe_id: u32) -> Result<(), PluginError> {
        let get_recipe_info = json!({
            "type": "GetRecipeInfo",
            "data": {
                "player_id": player_id,
                "recipe_id": {"0": recipe_id}
            }
        }).to_string();
        
        let event = CoreEvent::CustomMessage { 
            data: serde_json::Value::String(get_recipe_info) 
        };
        
        context.emit_event(EventNamespace::new("core"), Box::new(event)).await
            .map_err(|e| PluginError::ExecutionError(format!("Failed to emit get recipe info event: {}", e)))
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
            
            // IMPORTANT: We need to pass a String as a Value::String 
            let event = CoreEvent::CustomMessage { 
                data: serde_json::Value::String(recipe_msg_json.clone()) 
            };
            
            context.emit_event(EventNamespace::new("recipesmith"), Box::new(event)).await
                .map_err(|e| PluginError::ExecutionError(format!("Failed to emit recipe JSON event: {}", e)))?;
            context.log(LogLevel::Info, "Sent AddOrUpdateRecipe JSON message to RecipeSmith");
        }
        
        // Example: Send a crafting outcome for player "player123"
        self.send_crafting_outcome(context, "player123").await?;
        
        // Example: Request info for recipe ID 1
        self.request_recipe_info(context, "player123", 1).await?;

        self.initialized = true;
        Ok(())
    }

    async fn handle_event(&mut self, event_id: &EventId, event: &dyn GameEvent, context: &dyn ServerContext) -> Result<(), PluginError> {
        // Handle RecipeSmith events if needed
        if event_id.namespace.0 == "recipesmith" {
            context.log(LogLevel::Info, &format!("Received RecipeSmith event: {}", event_id.event_type));
            // Add handling for specific RecipeSmith events if needed
        }
        Ok(())
    }

    fn subscribed_events(&self) -> Vec<EventId> {
        vec![
            // Listen for RecipeSmith events if needed
            EventId::new(EventNamespace::new("recipesmith"), "recipe_updated"),
            EventId::new(EventNamespace::new("recipesmith"), "crafting_completed"),
            EventId::new(EventNamespace::new("recipesmith"), "all_recipes_info"),
            EventId::new(EventNamespace::new("recipesmith"), "recipe_info"),
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