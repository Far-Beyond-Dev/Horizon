//! RecipeSmith - Production-grade crafting system plugin for Horizon

#![warn(missing_docs, missing_debug_implementations, rust_2018_idioms)]
#![deny(unused_must_use)]
#![allow(improper_ctypes_definitions)] // For FFI functions

use async_trait::async_trait;
use event_system::{
    types::*,
    EventId, EventSystemImpl,
    PluginError as EventPluginError, EventError,
};
use crate::error::PluginError;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::{
    sync::{RwLock, Semaphore},
    time,
};
use tracing::{error, info, instrument, warn};
use uuid::Uuid;

mod api;
mod error;
mod events;
mod storage;
mod validation;

pub use api::*;
pub use error::*;
pub use events::*;
pub use storage::*;
pub use validation::*;

/// Default crafting time in milliseconds
const DEFAULT_CRAFT_TIME: u64 = 3000;
/// Maximum concurrent crafts per player
const DEFAULT_MAX_CONCURRENT: usize = 3;
/// Recipe file extension
const RECIPE_FILE_EXT: &str = ".recipe.json";

/// Plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeSmithConfig {
    /// Directory to load recipes from
    pub recipe_dir: PathBuf,
    /// Default crafting duration in milliseconds
    #[serde(default = "default_craft_time")]
    pub default_craft_time: u64,
    /// Maximum concurrent crafts per player
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_crafts: usize,
    /// Enable validation of recipes on load
    #[serde(default = "default_true")]
    pub validate_recipes: bool,
    /// Enable direct client event handling
    #[cfg(feature = "client_events")]
    #[serde(default)]
    pub enable_client_events: bool,
}

fn default_craft_time() -> u64 { DEFAULT_CRAFT_TIME }
fn default_max_concurrent() -> usize { DEFAULT_MAX_CONCURRENT }
fn default_true() -> bool { true }

/// Response for get recipes request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRecipesResponse {
    pub recipes: Vec<Recipe>,
}

/// Main plugin implementation
pub struct RecipeSmith {
    config: RecipeSmithConfig,
    recipes: RwLock<HashMap<RecipeId, Recipe>>,
    active_crafts: RwLock<HashMap<PlayerId, ActiveCraftSession>>,
    storage: Arc<dyn RecipeStorage + Send + Sync>,
    event_system: RwLock<Option<Arc<EventSystemImpl>>>,
    validator: RecipeValidator,
}

impl std::fmt::Debug for RecipeSmith {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecipeSmith")
            .field("config", &self.config)
            .field("recipes", &"RwLock<HashMap<RecipeId, Recipe>>")
            .field("active_crafts", &"RwLock<HashMap<PlayerId, ActiveCraftSession>>")
            .field("storage", &"Arc<dyn RecipeStorage>")
            .field("event_system", &"RwLock<Option<Arc<EventSystemImpl>>>")
            .field("validator", &self.validator)
            .finish()
    }
}

/// Player's active crafting session
#[derive(Debug, Clone)]
struct ActiveCraftSession {
    crafts: HashMap<CraftId, ActiveCraft>,
    semaphore: Arc<Semaphore>,
}

impl RecipeSmith {
    /// Create a new RecipeSmith instance
    pub fn new(
        config: RecipeSmithConfig,
        storage: Arc<dyn RecipeStorage + Send + Sync>,
        validator: RecipeValidator,
    ) -> Self {
        Self {
            config,
            recipes: RwLock::new(HashMap::new()),
            active_crafts: RwLock::new(HashMap::new()),
            storage,
            event_system: RwLock::new(None),
            validator,
        }
    }

    /// Load all recipes from storage
    #[instrument(skip(self))]
    async fn load_recipes(&self) -> Result<(), PluginError> {
        let mut recipes = self.storage.load_all().await.map_err(|e| {
            PluginError::InitializationFailed(format!("Failed to load recipes: {}", e))
        })?;

        if self.config.validate_recipes {
            recipes = self.validator.validate_all(recipes).await?;
        }

        *self.recipes.write().await = recipes
            .into_iter()
            .map(|r| (r.id, r))
            .collect();

        Ok(())
    }

    /// Start a new crafting process
    #[instrument(skip(self, ingredients))]
    async fn start_craft(
        &self,
        player_id: PlayerId,
        recipe_id: RecipeId,
        ingredients: Vec<ItemStack>,
    ) -> Result<CraftStarted, CraftingError> {
        // Verify recipe exists
        let recipe = self.recipes.read().await.get(&recipe_id)
            .ok_or(CraftingError::RecipeNotFound(recipe_id))?
            .clone();

        // Get or create player session
        let mut sessions = self.active_crafts.write().await;
        let session = sessions.entry(player_id).or_insert_with(|| {
            ActiveCraftSession {
                crafts: HashMap::new(),
                semaphore: Arc::new(Semaphore::new(self.config.max_concurrent_crafts)),
            }
        });

        // Check craft limit
        let _permit = session.semaphore.clone().try_acquire_owned()
            .map_err(|_| CraftingError::TooManyActiveCrafts)?;

        // Validate ingredients
        self.validator.validate_ingredients(&recipe, &ingredients).await?;

        // Create craft
        let craft_id = Uuid::new_v4();
        let duration = recipe.craft_time.unwrap_or(self.config.default_craft_time);

        let craft = ActiveCraft {
            craft_id,
            player_id,
            recipe_id: recipe.id,
            started_at: SystemTime::now(),
            duration,
            ingredients,
        };

        // For now, we'll skip the tokio::spawn completion logic
        // This needs to be redesigned to work with Rust's ownership system
        // Schedule completion
        // tokio::spawn(async move {
        //     time::sleep(Duration::from_millis(duration)).await;
        //     // Complete craft logic would go here
        // });

        // Track active craft
        session.crafts.insert(craft_id, craft);

        Ok(CraftStarted {
            craft_id,
            recipe_id,
            duration,
        })
    }

    /// Static method to complete a craft (for use in tokio::spawn)
    async fn complete_craft_static(
        _craft_id: CraftId,
        // You'll need to implement this properly based on your architecture
    ) -> Result<(), PluginError> {
        // Implementation needed
        todo!("Implement craft completion logic")
    }

    /// Handle player left event
    pub async fn handle_player_left(&self, event: PlayerLeftEvent) -> Result<(), EventPluginError> {
        let mut sessions = self.active_crafts.write().await;
        if let Some(session) = sessions.remove(&event.player_id) {
            info!("Cleaned up {} active crafts for player {}", session.crafts.len(), event.player_id);
        }
        Ok(())
    }

    /// Handle start craft request
    pub async fn handle_start_craft(&self, event: StartCraftRequest) -> Result<(), EventPluginError> {
        match self.start_craft(event.player_id, event.recipe_id, event.ingredients).await {
            Ok(result) => {
                if let Some(events) = &*self.event_system.read().await {
                    events.emit_plugin(
                        "recipe_smith",
                        "craft_started",
                        &result,
                    ).await.map_err(|e| EventPluginError::ExecutionError(format!("Failed to emit event: {}", e)))?;
                }
                Ok(())
            }
            Err(e) => {
                error!("Failed to start craft: {}", e);
                Err(EventPluginError::ExecutionError(format!("Craft failed: {}", e)))
            }
        }
    }

    /// Handle cancel craft request
    pub async fn handle_cancel_craft(&self, _event: CancelCraftRequest) -> Result<(), EventPluginError> {
        // Implementation needed
        todo!("Implement cancel craft logic")
    }

    /// Handle get recipes request
    pub async fn handle_get_recipes(&self, _event: GetRecipesRequest) -> Result<(), EventPluginError> {
        let recipes: Vec<Recipe> = self.recipes.read().await.values().cloned().collect();
        
        if let Some(events) = &*self.event_system.read().await {
            events.emit_plugin(
                "recipe_smith",
                "recipes_response",
                &GetRecipesResponse { recipes },
            ).await.map_err(|e| EventPluginError::ExecutionError(format!("Failed to emit event: {}", e)))?;
        }
        
        Ok(())
    }

    /// Handle client crafting request (conditional compilation)
    #[cfg(feature = "client_events")]
    pub async fn handle_client_request(&self, _event: ClientCraftingRequest) -> Result<(), EventPluginError> {
        // Implementation needed
        todo!("Implement client request handling")
    }
}

// Convert our plugin error to event system error
impl From<PluginError> for EventPluginError {
    fn from(err: PluginError) -> Self {
        match err {
            PluginError::InitializationFailed(msg) => EventPluginError::InitializationFailed(msg),
            PluginError::ValidationFailed(msg) => EventPluginError::ExecutionError(msg),
            PluginError::StorageError(msg) => EventPluginError::ExecutionError(msg),
            PluginError::CraftingError(e) => EventPluginError::ExecutionError(format!("Crafting error: {}", e)),
            PluginError::EventSystem(_) => EventPluginError::ExecutionError("Event system error occurred".to_string()),
        }
    }
}

#[async_trait]
impl Plugin for RecipeSmith {
    fn name(&self) -> &str { "recipe_smith" }

    fn version(&self) -> &str { env!("CARGO_PKG_VERSION") }

    #[instrument(skip(self, context))]
    async fn pre_init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), EventPluginError> {
        *self.event_system.write().await = Some(context.events());

        // Note: The event registration pattern needs to be redesigned
        // The current approach with closures that capture self won't work
        // You'll need to use a different pattern, possibly with Arc<Mutex<Self>>
        // or message passing channels

        Ok(())
    }

    #[instrument(skip(self, _context))]
    async fn init(&mut self, _context: Arc<dyn ServerContext>) -> Result<(), EventPluginError> {
        self.load_recipes().await.map_err(EventPluginError::from)?;
        info!("Loaded {} recipes", self.recipes.read().await.len());

        // Announce readiness
        if let Some(events) = &*self.event_system.read().await {
            events.emit_plugin(
                "recipe_smith",
                "ready",
                &PluginReadyEvent {
                    recipe_count: self.recipes.read().await.len(),
                },
            ).await.map_err(|e| EventPluginError::ExecutionError(format!("Failed to emit ready event: {}", e)))?;
        }

        Ok(())
    }

    #[instrument(skip(self, _context))]
    async fn shutdown(&mut self, _context: Arc<dyn ServerContext>) -> Result<(), EventPluginError> {
        // Cancel all active crafts
        let sessions = self.active_crafts.read().await;
        for (player_id, session) in sessions.iter() {
            for craft in session.crafts.values() {
                if let Some(events) = &*self.event_system.read().await {
                    events.emit_plugin(
                        "recipe_smith",
                        "craft_event",
                        &CraftEventNotification {
                            player_id: *player_id,
                            event: CraftEvent::Cancelled {
                                craft_id: craft.craft_id,
                                recipe_id: craft.recipe_id,
                                reason: "Server shutdown".to_string(),
                            },
                        },
                    ).await.map_err(|e| EventPluginError::ExecutionError(format!("Failed to emit shutdown event: {}", e)))?;
                }
            }
        }

        Ok(())
    }
}

// Required plugin exports
/// Creates a new RecipeSmith plugin instance with default configuration
#[no_mangle]
pub extern "C" fn create_plugin() -> *mut dyn Plugin {
    let config = RecipeSmithConfig {
        recipe_dir: PathBuf::from("./recipes"),
        default_craft_time: DEFAULT_CRAFT_TIME,
        max_concurrent_crafts: DEFAULT_MAX_CONCURRENT,
        validate_recipes: true,
        #[cfg(feature = "client_events")]
        enable_client_events: false,
    };

    let storage = Arc::new(JsonRecipeStorage::new(config.recipe_dir.clone()));
    let validator = RecipeValidator::new();
    let plugin = Box::new(RecipeSmith::new(config, storage, validator));
    
    Box::into_raw(plugin)
}

#[no_mangle]
pub extern "C" fn destroy_plugin(plugin: *mut dyn Plugin) {
    if !plugin.is_null() {
        let _ = unsafe { Box::from_raw(plugin) };
    }
}