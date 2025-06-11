use async_trait::async_trait;
use shared_types::*;
use std::collections::HashMap;
use serde::{Serialize, Deserialize}; // Added for Recipe and RecipeOutput serialization
use std::fmt; // Import for Display trait

/// RecipeSmith plugin - tracks recipes, their results, and byproducts.
///
/// This plugin demonstrates the callback-based event system by registering
/// for specific game events and handling them through direct callbacks.
pub struct RecipeSmith {
    name: &'static str,
    version: &'static str,
    // Stores known recipes, mapping recipe ID to Recipe struct
    recipes: HashMap<RecipeId, Recipe>,
    // Stores crafting outcomes for players, mapping PlayerId to a list of crafted items
    player_crafting_history: HashMap<PlayerId, Vec<CraftingOutcome>>,
    initialized: bool,
}

impl RecipeSmith {
    pub fn new() -> Self {
        Self {
            name: env!("CARGO_PKG_NAME"),
            version: env!("CARGO_PKG_VERSION"),
            recipes: HashMap::new(),
            player_crafting_history: HashMap::new(),
            initialized: false, // Initialize as false, will be set to true in `initialize`
        }
    }

    /// Add or update a recipe in the plugin's internal state.
    async fn add_or_update_recipe(&mut self, recipe: Recipe, context: &dyn ServerContext) -> Result<(), PluginError> {
        context.log(LogLevel::Debug, &format!("Attempting to add or update recipe: {}", recipe.name));
        let recipe_id = recipe.id.clone();
        self.recipes.insert(recipe_id.clone(), recipe.clone());

        let recipe_for_event = recipe.clone();
        let event = RecipeSmithEvent::RecipeUpdated { recipe: recipe_for_event };
        let namespace = EventNamespace::plugin_default(self.name);
        context.emit_event(namespace, Box::new(event)).await
            .map_err(|e| PluginError::ExecutionError(format!("Failed to emit RecipeUpdated event: {}", e)))?;

        // FIX: Use {:?} for RecipeId
        context.log(LogLevel::Info, &format!("Recipe '{}' (ID: {:?}) added/updated successfully.", recipe.name, recipe_id));
        Ok(())
    }

    /// Record a crafting outcome for a player.
    async fn record_crafting_outcome(&mut self, player_id: PlayerId, outcome: CraftingOutcome, context: &dyn ServerContext) -> Result<(), PluginError> {
        context.log(LogLevel::Debug, &format!("Recording crafting outcome for player {}.", player_id));
        self.player_crafting_history.entry(player_id).or_default().push(outcome.clone());

        let event = RecipeSmithEvent::CraftingCompleted {
            player_id,
            outcome,
        };
        let namespace = EventNamespace::plugin_default(self.name);
        context.emit_event(namespace, Box::new(event)).await
            .map_err(|e| PluginError::ExecutionError(format!("Failed to emit CraftingCompleted event: {}", e)))?;

        context.log(LogLevel::Info, &format!("Crafting outcome recorded for player {}.", player_id));
        Ok(())
    }

    /// Get all known recipes.
    async fn get_all_recipes(&self, player_id: PlayerId, context: &dyn ServerContext) -> Result<(), PluginError> {
        context.log(LogLevel::Debug, &format!("Player {} requested all recipes.", player_id));
        let recipes_vec: Vec<Recipe> = self.recipes.values().cloned().collect();

        let event = RecipeSmithEvent::AllRecipesInfo {
            player_id,
            recipes: recipes_vec,
        };
        let namespace = EventNamespace::plugin_default(self.name);
        context.emit_event(namespace, Box::new(event)).await
            .map_err(|e| PluginError::ExecutionError(format!("Failed to emit AllRecipesInfo event: {}", e)))?;

        context.log(LogLevel::Info, &format!("Sent all recipe info to player {}.", player_id));
        Ok(())
    }

    /// Get a specific recipe by ID.
    async fn get_recipe_info(&self, player_id: PlayerId, recipe_id: RecipeId, context: &dyn ServerContext) -> Result<(), PluginError> {
        // FIX: Use {:?} for RecipeId
        context.log(LogLevel::Debug, &format!("Player {} requested info for recipe ID: {:?}.", player_id, recipe_id));
        let recipe = self.recipes.get(&recipe_id).cloned()
            .ok_or_else(|| PluginError::ExecutionError(format!("Recipe with ID {:?} not found.", recipe_id)))?; // FIX: Use {:?} for RecipeId

        let event = RecipeSmithEvent::RecipeInfo {
            player_id,
            recipe,
        };
        let namespace = EventNamespace::plugin_default(self.name);
        context.emit_event(namespace, Box::new(event)).await
            .map_err(|e| PluginError::ExecutionError(format!("Failed to emit RecipeInfo event: {}", e)))?;

        // FIX: Use {:?} for RecipeId
        context.log(LogLevel::Info, &format!("Sent recipe info for ID {:?} to player {}.", recipe_id, player_id));
        Ok(())
    }

    /// Get crafting history for a specific player.
    async fn get_player_crafting_history(&self, player_id: PlayerId, target_player_id: PlayerId, context: &dyn ServerContext) -> Result<(), PluginError> {
        context.log(LogLevel::Debug, &format!("Player {} requested crafting history for player {}.", player_id, target_player_id));
        let history = self.player_crafting_history.get(&target_player_id).cloned().unwrap_or_default();

        let event = RecipeSmithEvent::PlayerCraftingHistory {
            player_id: target_player_id,
            history,
        };
        let namespace = EventNamespace::plugin_default(self.name);
        context.emit_event(namespace, Box::new(event)).await
            .map_err(|e| PluginError::ExecutionError(format!("Failed to emit PlayerCraftingHistory event: {}", e)))?;

        context.log(LogLevel::Info, &format!("Sent crafting history for player {} to player {}.", target_player_id, player_id));
        Ok(())
    }
}

impl Default for RecipeSmith {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for RecipeSmith {
    fn name(&self) -> &'static str {
        self.name
    }

    fn version(&self) -> &'static str {
        self.version
    }

    async fn pre_initialize(&mut self, context: &dyn ServerContext) -> Result<(), PluginError> {
        if self.initialized {
            return Err(PluginError::InitializationFailed("Plugin already initialized".to_string()));
        }

        context.log(LogLevel::Info, &format!("Initializing RecipeSmith plugin v{}", self.version));
        println!("RecipeSmith plugin initializing with callback-based event system");

        // Example: Pre-load some dummy recipes for demonstration
        self.recipes.insert(RecipeId(1), Recipe {
            id: RecipeId(1),
            name: "Wooden Pickaxe".to_string(),
            ingredients: vec![
                RecipeIngredient { item_id: ItemId(101), quantity: 3, name: "Wood".to_string() },
                RecipeIngredient { item_id: ItemId(102), quantity: 2, name: "Stick".to_string() },
            ],
            main_product: RecipeProduct { item_id: ItemId(201), quantity: 1, name: "Wooden Pickaxe".to_string() },
            byproducts: vec![],
        });
        self.recipes.insert(RecipeId(2), Recipe {
            id: RecipeId(2),
            name: "Stone Sword".to_string(),
            ingredients: vec![
                RecipeIngredient { item_id: ItemId(103), quantity: 2, name: "Stone".to_string() },
                RecipeIngredient { item_id: ItemId(102), quantity: 1, name: "Stick".to_string() },
            ],
            main_product: RecipeProduct { item_id: ItemId(202), quantity: 1, name: "Stone Sword".to_string() },
            byproducts: vec![
                RecipeProduct { item_id: ItemId(301), quantity: 1, name: "Stone Shard".to_string() },
            ],
        });
        context.log(LogLevel::Info, &format!("Loaded {} initial recipes.", self.recipes.len()));


        // Emit initialization event through callback system
        let init_event = RecipeSmithEvent::PluginInitialized {
            recipe_count: self.recipes.len() as u32,
        };

        println!("Emitting plugin initialized event through callback system");

        let namespace = EventNamespace::plugin_default(self.name);
        context.emit_event(namespace, Box::new(init_event)).await
            .map_err(|e| PluginError::InitializationFailed(format!("Failed to emit init event through callback system: {}", e)))?;

        context.log(LogLevel::Info, "RecipeSmith plugin initialization complete using callback-based events");

        
        self.initialized = true;
        context.log(LogLevel::Info, "RecipeSmith plugin initialized successfully with callback dispatch");
        Ok(())
    }

    /// Handle events through callback-based dispatch
    ///
    /// This method is called directly by the event processor when events
    /// that this plugin has subscribed to are emitted. No polling required!
    async fn handle_event(&mut self, event_id: &EventId, event: &dyn GameEvent, context: &dyn ServerContext) -> Result<(), PluginError> {
        self.initialized = true;
        if !self.initialized {
            return Err(PluginError::ExecutionError("Plugin not initialized".to_string()));
        } else {
            context.log(LogLevel::Debug, &format!("RecipeSmith plugin handling event {} via callback dispatch", event_id));
            println!("RecipeSmith plugin handling event {} via callback dispatch", event_id);
        }


        // Handle core events
        if event_id.namespace.0 == "core" {
            if let Some(core_event) = event.as_any().downcast_ref::<CoreEvent>() {
                match core_event {
                    CoreEvent::PlayerJoined { player } => {
                        context.log(LogLevel::Info, &format!("Player {} joined (callback dispatch).", player.name));
                        // Optionally initialize crafting history for new player if needed
                        self.player_crafting_history.entry(player.id).or_default();
                    }

                    CoreEvent::PlayerLeft { player_id } => {
                        context.log(LogLevel::Info, &format!("Player {} left (callback dispatch), cleaning up crafting data.", player_id));
                        self.player_crafting_history.remove(player_id);
                    }

                    CoreEvent::CustomMessage { data } => {
                        // Try to parse the message as a JSON string first 
                        if let Some(message_str) = data.as_str() {
                            // Try to deserialize directly from string to our message type
                            if let Ok(recipe_message) = serde_json::from_str::<RecipeSmithMessage>(message_str) {
                                context.log(LogLevel::Debug, "Processing RecipeSmith message via callback dispatch");
                                match recipe_message {
                                    RecipeSmithMessage::AddOrUpdateRecipe { recipe } => {
                                        context.log(LogLevel::Info, &format!("Received request to add/update recipe: {}", recipe.name));
                                        println!("Received AddOrUpdateRecipe message: {:?}", recipe);
                                        self.add_or_update_recipe(recipe, context).await?;
                                    }
                                    RecipeSmithMessage::RecordCraftingOutcome { player_id, outcome } => {
                                        context.log(LogLevel::Info, &format!("Received request to record crafting outcome for player {}.", player_id));
                                        self.record_crafting_outcome(player_id, outcome, context).await?;
                                    }
                                    RecipeSmithMessage::GetAllRecipes { player_id } => {
                                        context.log(LogLevel::Info, &format!("Received request for all recipes from player {}.", player_id));
                                        self.get_all_recipes(player_id, context).await?;
                                    }
                                    RecipeSmithMessage::GetRecipeInfo { player_id, recipe_id } => {
                                        context.log(LogLevel::Info, &format!("Received request for recipe info for ID {:?} from player {}.", recipe_id, player_id));
                                        self.get_recipe_info(player_id, recipe_id, context).await?;
                                    }
                                    RecipeSmithMessage::GetPlayerCraftingHistory { player_id, target_player_id } => {
                                        context.log(LogLevel::Info, &format!("Received request for player {}'s crafting history from player {}.", target_player_id, player_id));
                                        self.get_player_crafting_history(player_id, target_player_id, context).await?;
                                    }
                                }
                            }
                        }
                    }

                    _ => { /* Ignore other core events not relevant to RecipeSmith */ }
                }
            }
        }

        // Handle plugin-specific events (also via callback dispatch)
        if event_id.namespace == EventNamespace::plugin_default(self.name) {
            if let Some(recipe_smith_event) = event.as_any().downcast_ref::<RecipeSmithEvent>() {
                match recipe_smith_event {
                    RecipeSmithEvent::RecipeUpdated { recipe } => {
                        context.log(LogLevel::Debug, &format!(
                            "Recipe updated event processed via callback: {}", recipe.name
                        ));
                    }
                    RecipeSmithEvent::CraftingCompleted { player_id, outcome } => {
                        context.log(LogLevel::Debug, &format!(
                            "Crafting completed event processed via callback for player {}: produced {}",
                            player_id, outcome.main_product.name
                        ));
                    }
                    RecipeSmithEvent::PluginInitialized { recipe_count } => {
                        context.log(LogLevel::Info, &format!("RecipeSmith plugin initialization event processed via callback (recipes: {})", recipe_count));
                    }
                    RecipeSmithEvent::AllRecipesInfo { player_id, recipes } => {
                         context.log(LogLevel::Debug, &format!("Sent {} recipes to player {} in AllRecipesInfo event.", recipes.len(), player_id));
                    }
                    RecipeSmithEvent::RecipeInfo { player_id, recipe } => {
                         context.log(LogLevel::Debug, &format!("Sent recipe {} info to player {}.", recipe.name, player_id));
                    }
                    RecipeSmithEvent::PlayerCraftingHistory { player_id, history } => {
                        context.log(LogLevel::Debug, &format!("Sent crafting history for player {} with {} entries.", player_id, history.len()));
                    }
                    _ => {
                        context.log(LogLevel::Debug, "Other RecipeSmith event processed via callback dispatch");
                    }
                }
            }
        }

        Ok(())
    }

    /// Return events this plugin wants to receive via callback dispatch
    ///
    /// The event processor will register callbacks for these events,
    /// eliminating the need for polling.
    fn subscribed_events(&self) -> Vec<EventId> {
        vec![
            // Core events - will be delivered via callback dispatch
            EventId::new(EventNamespace::new("core"), "player_joined"),
            EventId::new(EventNamespace::new("core"), "player_left"),
            EventId::new(EventNamespace::new("core"), "custom_message"), // For receiving RecipeSmith messages

            // Plugin-specific events - also delivered via callback dispatch
            EventId::new(EventNamespace::plugin_default(self.name), "recipe_updated"),
            EventId::new(EventNamespace::plugin_default(self.name), "crafting_completed"),
            EventId::new(EventNamespace::plugin_default(self.name), "plugin_initialized"),
            EventId::new(EventNamespace::plugin_default(self.name), "all_recipes_info"),
            EventId::new(EventNamespace::plugin_default(self.name), "recipe_info"),
            EventId::new(EventNamespace::plugin_default(self.name), "player_crafting_history"),
            EventId::new(EventNamespace::plugin_default(self.name), "plugin_shutdown"), // Subscribe to own shutdown event
        ]
    }

    async fn shutdown(&mut self, context: &dyn ServerContext) -> Result<(), PluginError> {
        if !self.initialized {
            return Ok(()); // Already shut down
        }

        context.log(LogLevel::Info, "Shutting down RecipeSmith plugin (callback-based events)");

        // Clear all data
        self.recipes.clear();
        self.player_crafting_history.clear();
        self.initialized = false;

        // Emit shutdown event through callback system
        let shutdown_event = RecipeSmithEvent::PluginShutdown;
        let namespace = EventNamespace::plugin_default(self.name);
        context.emit_event(namespace, Box::new(shutdown_event)).await
            .map_err(|e| PluginError::ExecutionError(format!("Failed to emit shutdown event through callback system: {}", e)))?;

        context.log(LogLevel::Info, "RecipeSmith plugin shutdown complete (callbacks will be automatically unregistered)");

        Ok(())
    }
}

/// Events specific to the RecipeSmith plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum RecipeSmithEvent {
    PluginInitialized { recipe_count: u32 },
    PluginShutdown,
    RecipeUpdated { recipe: Recipe },
    CraftingCompleted {
        player_id: PlayerId,
        outcome: CraftingOutcome,
    },
    AllRecipesInfo {
        player_id: PlayerId,
        recipes: Vec<Recipe>,
    },
    RecipeInfo {
        player_id: PlayerId,
        recipe: Recipe,
    },
    PlayerCraftingHistory {
        player_id: PlayerId,
        history: Vec<CraftingOutcome>,
    },
}

impl GameEvent for RecipeSmithEvent {
    fn event_type(&self) -> &'static str {
        match self {
            RecipeSmithEvent::PluginInitialized { .. } => "plugin_initialized",
            RecipeSmithEvent::PluginShutdown => "plugin_shutdown",
            RecipeSmithEvent::RecipeUpdated { .. } => "recipe_updated",
            RecipeSmithEvent::CraftingCompleted { .. } => "crafting_completed",
            RecipeSmithEvent::AllRecipesInfo { .. } => "all_recipes_info",
            RecipeSmithEvent::RecipeInfo { .. } => "recipe_info",
            RecipeSmithEvent::PlayerCraftingHistory { .. } => "player_crafting_history",
        }
    }

    fn serialize(&self) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(serde_json::to_vec(self)?)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Messages that can be sent to the RecipeSmith plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum RecipeSmithMessage {
    AddOrUpdateRecipe { recipe: Recipe },
    RecordCraftingOutcome { player_id: PlayerId, outcome: CraftingOutcome },
    GetAllRecipes { player_id: PlayerId },
    GetRecipeInfo { player_id: PlayerId, recipe_id: RecipeId },
    GetPlayerCraftingHistory { player_id: PlayerId, target_player_id: PlayerId },
}


// --- New Structs for RecipeSmith Logic ---

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecipeId(pub u32);

// OPTION 2: Implement Display for RecipeId (if you want a specific string representation)
impl fmt::Display for RecipeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemId(pub u32);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeIngredient {
    pub item_id: ItemId,
    pub quantity: u32,
    pub name: String, // For display/logging purposes
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeProduct {
    pub item_id: ItemId,
    pub quantity: u32,
    pub name: String, // For display/logging purposes
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    pub id: RecipeId,
    pub name: String,
    pub ingredients: Vec<RecipeIngredient>,
    pub main_product: RecipeProduct,
    pub byproducts: Vec<RecipeProduct>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftingOutcome {
    pub recipe_id: RecipeId,
    pub main_product: RecipeProduct,
    pub byproducts: Vec<RecipeProduct>,
    pub timestamp: u64, // Unix timestamp of crafting
}


/// CORRECT FFI EXPORTS - These return trait objects directly for safe Rust-to-Rust FFI

/// Required export for plugin creation - returns *mut dyn Plugin directly
#[no_mangle]
pub unsafe extern "C" fn create_plugin() -> *mut dyn Plugin {
    let plugin = Box::new(RecipeSmith::new());
    // Convert concrete plugin to trait object
    let trait_object: Box<dyn Plugin> = plugin;
    // Return the raw pointer to the trait object
    Box::into_raw(trait_object)
}

/// Required export for plugin destruction - takes *mut dyn Plugin directly
#[no_mangle]
pub unsafe extern "C" fn destroy_plugin(plugin: *mut dyn Plugin) {
    if !plugin.is_null() {
        // Convert back to Box and let it drop
        let _ = Box::from_raw(plugin);
    }
}