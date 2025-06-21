//! RecipeSmith - Advanced Crafting System Plugin
//! 
//! A comprehensive crafting system that provides:
//! - Recipe management with dynamic loading
//! - Multi-stage crafting processes
//! - Resource validation and consumption
//! - Crafting queues with progress tracking
//! - Integration points for other plugins
//! - Optional client event handling
//! - Extensible architecture for custom crafting mechanics

use async_trait::async_trait;
use dashmap::DashMap;
use event_system::types::*;
use event_system::{EventSystemExt, ClientEventSystemExt};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

pub mod types;
pub mod recipe_manager;
pub mod crafting_queue;
pub mod validation;

#[cfg(feature = "client_events")]
pub mod client_integration;

// Re-export all types for easy access
pub use crate::types::*;
use recipe_manager::RecipeManager;
use crafting_queue::CraftingQueueManager;
use validation::CraftingValidator;

// ============================================================================
// Core Plugin Implementation
// ============================================================================

/// RecipeSmith crafting system plugin
pub struct RecipeSmithPlugin {
    /// Plugin name
    name: String,
    /// Plugin version
    version: String,
    /// Recipe management system
    recipe_manager: Arc<RecipeManager>,
    /// Crafting queue manager
    queue_manager: Arc<CraftingQueueManager>,
    /// Crafting validator
    validator: Arc<CraftingValidator>,
    /// Player crafting states
    player_states: Arc<DashMap<PlayerId, PlayerCraftingState>>,
    /// Plugin configuration
    config: CraftingConfig,
    /// Statistics tracking
    stats: Arc<RwLock<CraftingStats>>,
    /// Integration registry for other plugins
    integrations: Arc<DashMap<String, PluginIntegration>>,
}

impl RecipeSmithPlugin {
    /// Create a new RecipeSmith plugin instance
    pub fn new() -> Self {
        Self {
            name: "recipe_smith".to_string(),
            version: "1.0.0".to_string(),
            recipe_manager: Arc::new(RecipeManager::new()),
            queue_manager: Arc::new(CraftingQueueManager::new()),
            validator: Arc::new(CraftingValidator::new()),
            player_states: Arc::new(DashMap::new()),
            config: CraftingConfig::default(),
            stats: Arc::new(RwLock::new(CraftingStats::default())),
            integrations: Arc::new(DashMap::new()),
        }
    }

    /// Register integration with another plugin
    pub async fn register_integration(
        &self,
        plugin_name: String,
        integration: PluginIntegration,
    ) -> Result<(), CraftingError> {
        info!("🔗 Registering integration with plugin: {}", plugin_name);
        self.integrations.insert(plugin_name.clone(), integration);
        
        // Emit integration registered event
        Ok(())
    }

    /// Load recipes from configuration
    async fn load_recipes(&self, context: Arc<dyn ServerContext>) -> Result<(), CraftingError> {
        info!("📋 Loading crafting recipes...");
        
        // Load default recipes
        let default_recipes = self.get_default_recipes();
        for recipe in default_recipes {
            self.recipe_manager.add_recipe(recipe).await?;
        }

        // Emit recipes loaded event
        let event = RecipesLoadedEvent {
            recipe_count: self.recipe_manager.get_recipe_count().await,
            timestamp: current_timestamp(),
        };

        context.events().emit_namespaced(
            EventId::plugin("recipe_smith", "recipes_loaded"),
            &event,
        ).await.map_err(|e| CraftingError::SystemError(e.to_string()))?;

        info!("✅ Loaded {} recipes", event.recipe_count);
        Ok(())
    }

    /// Register all event handlers
    async fn register_event_handlers(&self, context: Arc<dyn ServerContext>) -> Result<(), CraftingError> {
        let events = context.events();
        
        // === Core Crafting Events ===
        
        // Handle crafting requests (from other plugins or internal systems)
        {
            let plugin = self.clone_for_handler();
            events.on_plugin("recipe_smith", "crafting_request", move |event: CraftingRequestEvent| {
                let plugin = plugin.clone();
                tokio::spawn(async move {
                    if let Err(e) = plugin.handle_crafting_request(event).await {
                        error!("Failed to handle crafting request: {}", e);
                    }
                });
                Ok(())
            }).await.map_err(|e| CraftingError::SystemError(e.to_string()))?;
        }

        // Handle cancel crafting requests
        {
            let plugin = self.clone_for_handler();
            events.on_plugin("recipe_smith", "cancel_crafting", move |event: CancelCraftingEvent| {
                let plugin = plugin.clone();
                tokio::spawn(async move {
                    if let Err(e) = plugin.handle_cancel_crafting(event).await {
                        error!("Failed to handle cancel crafting: {}", e);
                    }
                });
                Ok(())
            }).await.map_err(|e| CraftingError::SystemError(e.to_string()))?;
        }

        // Handle inventory updates (to validate ongoing crafting)
        {
            let plugin = self.clone_for_handler();
            events.on("inventory_updated", move |event: InventoryUpdatedEvent| {
                let plugin = plugin.clone();
                tokio::spawn(async move {
                    plugin.handle_inventory_update(event).await;
                });
                Ok(())
            }).await.map_err(|e| CraftingError::SystemError(e.to_string()))?;
        }

        // === Player Lifecycle Events ===
        
        // Handle player joined
        {
            let plugin = self.clone_for_handler();
            events.on("player_joined", move |event: PlayerJoinedEvent| {
                let plugin = plugin.clone();
                tokio::spawn(async move {
                    plugin.handle_player_joined(event).await;
                });
                Ok(())
            }).await.map_err(|e| CraftingError::SystemError(e.to_string()))?;
        }

        // Handle player left
        {
            let plugin = self.clone_for_handler();
            events.on("player_left", move |event: PlayerLeftEvent| {
                let plugin = plugin.clone();
                tokio::spawn(async move {
                    plugin.handle_player_left(event).await;
                });
                Ok(())
            }).await.map_err(|e| CraftingError::SystemError(e.to_string()))?;
        }

        // === Integration Events ===
        
        // Handle skill level updates (if skill system is present)
        {
            let plugin = self.clone_for_handler();
            events.on("skill_level_updated", move |event: SkillLevelUpdatedEvent| {
                let plugin = plugin.clone();
                tokio::spawn(async move {
                    plugin.handle_skill_update(event).await;
                });
                Ok(())
            }).await.map_err(|e| CraftingError::SystemError(e.to_string()))?;
        }

        // Handle item quality updates (if quality system is present)
        {
            let plugin = self.clone_for_handler();
            events.on("item_quality_updated", move |event: ItemQualityUpdatedEvent| {
                let plugin = plugin.clone();
                tokio::spawn(async move {
                    plugin.handle_quality_update(event).await;
                });
                Ok(())
            }).await.map_err(|e| CraftingError::SystemError(e.to_string()))?;
        }

        // === Client Events (if feature enabled) ===
        
        #[cfg(feature = "client_events")]
        {
            // Handle client crafting requests
            {
                let plugin = self.clone_for_handler();
                events.on_client("craft_item", move |event: ClientCraftingRequestEvent| {
                    let plugin = plugin.clone();
                    tokio::spawn(async move {
                        if let Err(e) = plugin.handle_client_crafting_request(event).await {
                            error!("Failed to handle client crafting request: {}", e);
                        }
                    });
                    Ok(())
                }).await.map_err(|e| CraftingError::SystemError(e.to_string()))?;
            }

            // Handle client cancel requests
            {
                let plugin = self.clone_for_handler();
                events.on_client("cancel_craft", move |event: ClientCancelCraftingEvent| {
                    let plugin = plugin.clone();
                    tokio::spawn(async move {
                        if let Err(e) = plugin.handle_client_cancel_request(event).await {
                            error!("Failed to handle client cancel request: {}", e);
                        }
                    });
                    Ok(())
                }).await.map_err(|e| CraftingError::SystemError(e.to_string()))?;
            }

            // Handle client recipe discovery requests
            {
                let plugin = self.clone_for_handler();
                events.on_client("discover_recipe", move |event: ClientRecipeDiscoveryEvent| {
                    let plugin = plugin.clone();
                    tokio::spawn(async move {
                        if let Err(e) = plugin.handle_client_recipe_discovery(event).await {
                            error!("Failed to handle client recipe discovery: {}", e);
                        }
                    });
                    Ok(())
                }).await.map_err(|e| CraftingError::SystemError(e.to_string()))?;
            }
        }

        info!("✅ Registered all crafting event handlers");
        Ok(())
    }

    /// Start the crafting tick system
    async fn start_crafting_system(&self, context: Arc<dyn ServerContext>) -> Result<(), CraftingError> {
        info!("⚙️ Starting crafting tick system...");
        
        let queue_manager = self.queue_manager.clone();
        let player_states = self.player_states.clone();
        let stats = self.stats.clone();
        let events = context.events();

        // Start the crafting progress ticker
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(100)); // 10 FPS
            
            loop {
                interval.tick().await;
                
                // Process all active crafting operations
                match queue_manager.get_active_operations().await {
                    Ok(active_operations) => {
                        for operation in active_operations {
                            if operation.is_complete() {
                                // Complete the crafting operation
                                if let Err(e) = Self::complete_crafting_operation(
                                    &operation,
                                    &player_states,
                                    &stats,
                                    &events,
                                ).await {
                                    error!("Failed to complete crafting operation {}: {}", operation.id, e);
                                }
                            } else {
                                // Update progress
                                if let Err(e) = Self::update_crafting_progress(
                                    &operation,
                                    &events,
                                ).await {
                                    error!("Failed to update crafting progress for {}: {}", operation.id, e);
                                }
                            }
                        }
                    },
                    Err(e) => {
                        error!("Failed to get active operations: {}", e);
                    }
                }
            }
        });

        info!("✅ Crafting tick system started");
        Ok(())
    }

    /// Helper method to clone plugin for event handlers
    fn clone_for_handler(&self) -> Arc<Self> {
        Arc::new(Self {
            name: self.name.clone(),
            version: self.version.clone(),
            recipe_manager: self.recipe_manager.clone(),
            queue_manager: self.queue_manager.clone(),
            validator: self.validator.clone(),
            player_states: self.player_states.clone(),
            config: self.config.clone(),
            stats: self.stats.clone(),
            integrations: self.integrations.clone(),
        })
    }

    /// Get default recipes
    fn get_default_recipes(&self) -> Vec<Recipe> {
        vec![
            Recipe {
                id: RecipeId(1001),
                name: "Iron Ingot".to_string(),
                description: "Smelt iron ore into a pure iron ingot".to_string(),
                category: "Metallurgy".to_string(),
                ingredients: vec![
                    Ingredient {
                        item_id: 5001,
                        quantity: 1,
                        quality_requirement: None,
                    }
                ],
                main_product: CraftingResult {
                    item_id: 6001,
                    quantity: 1,
                    quality_bonus: Some(0.1),
                },
                byproducts: vec![],
                requirements: CraftingRequirements {
                    skill_requirements: HashMap::new(),
                    station_requirement: None,
                    tool_requirements: vec![],
                    environmental_requirements: vec![],
                },
                timing: CraftingTiming {
                    base_duration: Duration::from_secs(5),
                    skill_speed_bonus: Some(0.05),
                    station_speed_bonus: Some(0.2),
                },
                unlock_conditions: vec![],
                experience_rewards: HashMap::new(),
                failure_chance: 0.05,
                critical_success_chance: 0.1,
            },
            Recipe {
                id: RecipeId(1002),
                name: "Iron Plate".to_string(),
                description: "Forge iron ingots into sturdy plates".to_string(),
                category: "Metallurgy".to_string(),
                ingredients: vec![
                    Ingredient {
                        item_id: 6001,
                        quantity: 2,
                        quality_requirement: None,
                    }
                ],
                main_product: CraftingResult {
                    item_id: 6002,
                    quantity: 1,
                    quality_bonus: Some(0.15),
                },
                byproducts: vec![],
                requirements: CraftingRequirements {
                    skill_requirements: {
                        let mut skills = HashMap::new();
                        skills.insert("Blacksmithing".to_string(), 25);
                        skills
                    },
                    station_requirement: Some("Forge".to_string()),
                    tool_requirements: vec!["Hammer".to_string()],
                    environmental_requirements: vec![],
                },
                timing: CraftingTiming {
                    base_duration: Duration::from_secs(8),
                    skill_speed_bonus: Some(0.03),
                    station_speed_bonus: Some(0.25),
                },
                unlock_conditions: vec![
                    UnlockCondition::SkillLevel("Blacksmithing".to_string(), 20),
                    UnlockCondition::RecipeKnown(RecipeId(1001)),
                ],
                experience_rewards: {
                    let mut exp = HashMap::new();
                    exp.insert("Blacksmithing".to_string(), 15);
                    exp
                },
                failure_chance: 0.08,
                critical_success_chance: 0.15,
            },
        ]
    }
}

// ============================================================================
// Event Handlers Implementation
// ============================================================================

impl RecipeSmithPlugin {
    /// Handle crafting request from other plugins or internal systems
    async fn handle_crafting_request(&self, event: CraftingRequestEvent) -> Result<(), CraftingError> {
        debug!("📝 Processing crafting request for player {} recipe {}", event.player_id, event.recipe_id);

        // Validate the request
        let recipe = self.recipe_manager.get_recipe(&RecipeId(event.recipe_id)).await
            .ok_or_else(|| CraftingError::RecipeNotFound(event.recipe_id))?;

        let validation_result = self.validator.validate_crafting_request(
            &event,
            &recipe,
            &self.player_states,
            &self.integrations,
        ).await?;

        if !validation_result.is_valid {
            // Emit crafting failed event
            let failed_event = CraftingFailedEvent {
                operation_id: CraftingOperationId::new(),
                player_id: event.player_id,
                recipe_id: RecipeId(event.recipe_id),
                reason: validation_result.failure_reason.unwrap_or_else(|| "Validation failed".to_string()),
                timestamp: current_timestamp(),
            };

            return Err(CraftingError::ValidationFailed(failed_event.reason));
        }

        // Create crafting operation
        let operation = CraftingOperation {
            id: CraftingOperationId::new(),
            player_id: event.player_id,
            recipe: (*recipe).clone(),
            quantity: event.quantity,
            ingredient_sources: event.ingredient_sources,
            started_at: SystemTime::now(),
            estimated_completion: SystemTime::now() + recipe.timing.base_duration,
            progress: 0.0,
            status: CraftingStatus::InProgress,
            quality_modifiers: validation_result.quality_modifiers,
            experience_multiplier: validation_result.experience_multiplier,
        };

        // Add to queue
        self.queue_manager.add_operation(operation.clone()).await?;

        // Update player state
        if let Some(mut state) = self.player_states.get_mut(&event.player_id) {
            state.active_operations.push(operation.id);
            state.total_crafting_attempts += 1;
        }

        // Emit crafting started event
        let _started_event = CraftingStartedEvent {
            operation_id: operation.id,
            player_id: event.player_id,
            recipe_id: RecipeId(event.recipe_id),
            quantity: event.quantity,
            estimated_duration: recipe.timing.base_duration,
            timestamp: current_timestamp(),
        };

        // Note: You might want to emit this event to the event system
        // context.events().emit_namespaced(...).await;

        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.total_crafting_started += 1;
            stats.recipes_attempted.entry(recipe.name.clone()).and_modify(|e| *e += 1).or_insert(1);
        }

        info!("🎯 Started crafting operation {} for player {}", operation.id, event.player_id);
        Ok(())
    }

    /// Handle cancel crafting request
    async fn handle_cancel_crafting(&self, event: CancelCraftingEvent) -> Result<(), CraftingError> {
        debug!("❌ Cancelling crafting operation {} for player {}", event.operation_id, event.player_id);

        // Find and cancel the operation - Handle Result type properly
        let operation = match self.queue_manager.cancel_operation(&event.operation_id).await {
            Ok(Some(op)) => op,
            Ok(None) => return Err(CraftingError::OperationNotFound(event.operation_id)),
            Err(e) => return Err(e),
        };

        // Verify ownership
        if operation.player_id != event.player_id {
            return Err(CraftingError::PermissionDenied);
        }

        // Update player state
        if let Some(mut state) = self.player_states.get_mut(&event.player_id) {
            state.active_operations.retain(|&id| id != event.operation_id);
        }

        // Emit crafting cancelled event
        let _cancelled_event = CraftingCancelledEvent {
            operation_id: event.operation_id,
            player_id: event.player_id,
            recipe_id: operation.recipe.id,
            progress_lost: operation.progress,
            timestamp: current_timestamp(),
        };

        // Note: You might want to emit this event to the event system
        // context.events().emit_namespaced(...).await;

        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.total_crafting_cancelled += 1;
        }

        info!("🚫 Cancelled crafting operation {} for player {}", event.operation_id, event.player_id);
        Ok(())
    }

    /// Handle inventory update to validate ongoing crafting
    async fn handle_inventory_update(&self, event: InventoryUpdatedEvent) {
        debug!("📦 Processing inventory update for player {}", event.player_id);

        // Check if this affects any ongoing crafting operations
        if let Some(state) = self.player_states.get(&event.player_id) {
            for &operation_id in &state.active_operations {
                // Handle Result type properly when getting operation
                match self.queue_manager.get_operation(&operation_id).await {
                    Ok(opt_operation) => {
                        match opt_operation {
                            Some(_operation) => {
                                // TODO: Validate the operation against updated inventory
                                // Re-validate the operation with updated inventory
                                // This is important for continuous resource consumption
                                // or when ingredients might be removed/stolen
                                
                                // Implementation would check if required ingredients are still available
                                // and potentially pause/cancel operations if resources are insufficient
                            },
                            None => {
                                warn!("Operation {} not found during inventory update", operation_id);
                            }
                        }
                    },
                    Err(e) => {
                        error!("Failed to get operation {} during inventory update: {}", operation_id, e);
                    }
                }
            }
        }
    }

    /// Handle player joined event
    async fn handle_player_joined(&self, event: PlayerJoinedEvent) {
        info!("👤 Player {} joined - initializing crafting state", event.player.name);

        // Initialize player crafting state
        let state = PlayerCraftingState {
            player_id: event.player.id,
            known_recipes: vec![RecipeId(1001)], // Everyone starts with basic recipes
            active_operations: vec![],
            total_crafting_attempts: 0,
            successful_crafts: 0,
            crafting_level: 1,
            experience_points: HashMap::new(),
            unlocked_categories: vec!["Basic".to_string()],
            preferences: CraftingPreferences::default(),
        };

        self.player_states.insert(event.player.id, state);

        // Emit player crafting initialized event for other plugins
        let _init_event = PlayerCraftingInitializedEvent {
            player_id: event.player.id,
            initial_recipes: vec![RecipeId(1001)],
            crafting_level: 1,
            timestamp: current_timestamp(),
        };

        // Note: You might want to emit this event to the event system
        // context.events().emit_namespaced(...).await;

        debug!("✅ Initialized crafting state for player {}", event.player.id);
    }

    /// Handle player left event
    async fn handle_player_left(&self, event: PlayerLeftEvent) {
        info!("👋 Player {} left - cleaning up crafting state", event.player_id);

        // Cancel all active operations for this player
        if let Some(state) = self.player_states.get(&event.player_id) {
            for &operation_id in &state.active_operations {
                if let Err(e) = self.queue_manager.cancel_operation(&operation_id).await {
                    warn!("Failed to cancel operation {} for leaving player: {}", operation_id, e);
                }
            }
        }

        // Remove player state
        self.player_states.remove(&event.player_id);

        debug!("🧹 Cleaned up crafting state for player {}", event.player_id);
    }

    /// Handle skill level update
    async fn handle_skill_update(&self, event: SkillLevelUpdatedEvent) {
        debug!("📈 Skill {} updated to level {} for player {}", 
               event.skill_name, event.new_level, event.player_id);

        // Update player crafting state
        if let Some(mut state) = self.player_states.get_mut(&event.player_id) {
            // Check for new recipe unlocks
            let newly_unlocked = self.recipe_manager.check_recipe_unlocks(
                &event.player_id,
                &event.skill_name,
                event.new_level,
                &state.known_recipes,
            ).await;

            for recipe_id in newly_unlocked {
                state.known_recipes.push(recipe_id);
                
                // Emit recipe unlocked event
                let _unlock_event = RecipeUnlockedEvent {
                    player_id: event.player_id,
                    recipe_id,
                    unlock_reason: format!("Skill {} reached level {}", event.skill_name, event.new_level),
                    timestamp: current_timestamp(),
                };

                // Note: You might want to emit this event to the event system
                // context.events().emit_namespaced(...).await;

                info!("🔓 Player {} unlocked recipe {:?} via skill {}", 
                     event.player_id, recipe_id, event.skill_name);
            }
        }
    }

    /// Handle item quality update
    async fn handle_quality_update(&self, event: ItemQualityUpdatedEvent) {
        debug!("✨ Item quality updated for player {}: item {} now quality {}", 
               event.player_id, event.item_id, event.new_quality);

        // This could trigger re-evaluation of ongoing crafting operations
        // that might benefit from higher quality ingredients
    }

    #[cfg(feature = "client_events")]
    /// Handle client crafting request
    async fn handle_client_crafting_request(&self, event: ClientCraftingRequestEvent) -> Result<(), CraftingError> {
        info!("📱 Client crafting request from player {}: recipe {}", 
              event.player_id, event.recipe_id);

        // Convert client event to internal event
        let internal_event = CraftingRequestEvent {
            player_id: event.player_id,
            recipe_id: event.recipe_id,
            quantity: event.quantity,
            ingredient_sources: event.ingredient_sources,
        };

        // Process using internal handler
        self.handle_crafting_request(internal_event).await
    }

    #[cfg(feature = "client_events")]
    /// Handle client cancel request
    async fn handle_client_cancel_request(&self, event: ClientCancelCraftingEvent) -> Result<(), CraftingError> {
        info!("📱 Client cancel request from player {}: operation {}", 
              event.player_id, event.operation_id);

        // Convert client event to internal event
        let internal_event = CancelCraftingEvent {
            player_id: event.player_id,
            operation_id: event.operation_id,
            reason: "Client requested".to_string(),
        };

        // Process using internal handler
        self.handle_cancel_crafting(internal_event).await
    }

    #[cfg(feature = "client_events")]
    /// Handle client recipe discovery request
    async fn handle_client_recipe_discovery(&self, event: ClientRecipeDiscoveryEvent) -> Result<(), CraftingError> {
        info!("📱 Client recipe discovery from player {}: attempting to discover recipe", event.player_id);

        // Implementation for recipe discovery mechanic
        // This could involve experimenting with ingredients to discover new recipes
        
        Ok(())
    }

    /// Complete a crafting operation
    async fn complete_crafting_operation(
        operation: &CraftingOperation,
        player_states: &DashMap<PlayerId, PlayerCraftingState>,
        stats: &RwLock<CraftingStats>,
        events: &Arc<EventSystemImpl>,
    ) -> Result<(), CraftingError> {
        info!("✅ Completing crafting operation {} for player {}", operation.id, operation.player_id);

        // Calculate final results
        let success = rand::random::<f32>() > operation.recipe.failure_chance;
        let critical = success && rand::random::<f32>() < operation.recipe.critical_success_chance;

        if success {
            // Emit crafting completed event
            let completed_event = CraftingCompletedEvent {
                operation_id: operation.id,
                player_id: operation.player_id,
                recipe_id: operation.recipe.id,
                quantity_produced: operation.quantity,
                quality_achieved: if critical { Some(2.0) } else { Some(1.0) },
                experience_gained: operation.recipe.experience_rewards.clone(),
                byproducts: operation.recipe.byproducts.clone(),
                critical_success: critical,
                timestamp: current_timestamp(),
            };

            // Update player state
            if let Some(mut state) = player_states.get_mut(&operation.player_id) {
                state.active_operations.retain(|&id| id != operation.id);
                state.successful_crafts += 1;
                
                // Add experience
                for (skill, exp) in &operation.recipe.experience_rewards {
                    *state.experience_points.entry(skill.clone()).or_insert(0) += exp;
                }
            }

            // Update statistics
            {
                let mut stats_guard = stats.write().await;
                stats_guard.total_crafting_completed += 1;
                if critical {
                    stats_guard.total_critical_successes += 1;
                }
            }

            events.emit_namespaced(
                EventId::plugin("recipe_smith", "crafting_completed"),
                &completed_event,
            ).await.map_err(|e| CraftingError::SystemError(e.to_string()))?;

        } else {
            // Emit crafting failed event
            let failed_event = CraftingFailedEvent {
                operation_id: operation.id,
                player_id: operation.player_id,
                recipe_id: operation.recipe.id,
                reason: "Crafting attempt failed".to_string(),
                timestamp: current_timestamp(),
            };

            // Update player state
            if let Some(mut state) = player_states.get_mut(&operation.player_id) {
                state.active_operations.retain(|&id| id != operation.id);
            }

            // Update statistics
            {
                let mut stats_guard = stats.write().await;
                stats_guard.total_crafting_failed += 1;
            }

            events.emit_namespaced(
                EventId::plugin("recipe_smith", "crafting_failed"),
                &failed_event,
            ).await.map_err(|e| CraftingError::SystemError(e.to_string()))?;
        }

        Ok(())
    }

    /// Update crafting progress
    async fn update_crafting_progress(
        operation: &CraftingOperation,
        events: &Arc<EventSystemImpl>,
    ) -> Result<(), CraftingError> {
        // Calculate current progress
        let elapsed = SystemTime::now().duration_since(operation.started_at)
            .unwrap_or_default();
        let total_duration = operation.estimated_completion.duration_since(operation.started_at)
            .unwrap_or_default();
        
        let progress = if total_duration.as_secs() > 0 {
            (elapsed.as_secs_f32() / total_duration.as_secs_f32()).min(1.0)
        } else {
            1.0
        };

        // Emit progress update if significant change
        if (progress * 100.0) as u32 != (operation.progress * 100.0) as u32 {
            let progress_event = CraftingProgressEvent {
                operation_id: operation.id,
                player_id: operation.player_id,
                recipe_id: operation.recipe.id,
                progress_percentage: (progress * 100.0) as u32,
                estimated_completion: operation.estimated_completion,
                timestamp: current_timestamp(),
            };

            events.emit_namespaced(
                EventId::plugin("recipe_smith", "crafting_progress"),
                &progress_event,
            ).await.map_err(|e| CraftingError::SystemError(e.to_string()))?;
        }

        Ok(())
    }
}

// ============================================================================
// Plugin Trait Implementation
// ============================================================================

#[async_trait]
impl Plugin for RecipeSmithPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        &self.version
    }

    async fn pre_init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        info!("🔧 Pre-initializing RecipeSmith plugin...");

        // Register all event handlers
        self.register_event_handlers(context.clone()).await
            .map_err(|e| PluginError::InitializationFailed(format!("Failed to register event handlers: {}", e)))?;

        info!("✅ RecipeSmith pre-initialization complete");
        Ok(())
    }

    async fn init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        info!("🚀 Initializing RecipeSmith plugin...");

        // Load recipes
        self.load_recipes(context.clone()).await
            .map_err(|e| PluginError::InitializationFailed(format!("Failed to load recipes: {}", e)))?;

        // Start crafting system
        self.start_crafting_system(context.clone()).await
            .map_err(|e| PluginError::InitializationFailed(format!("Failed to start crafting system: {}", e)))?;

        // Emit plugin ready event
        let ready_event = PluginReadyEvent {
            plugin_name: self.name.clone(),
            capabilities: vec![
                "crafting".to_string(),
                "recipes".to_string(),
                "queues".to_string(),
                #[cfg(feature = "client_events")]
                "client_integration".to_string(),
            ],
            integration_points: vec![
                "inventory_system".to_string(),
                "skill_system".to_string(),
                "quality_system".to_string(),
                "station_system".to_string(),
            ],
            timestamp: current_timestamp(),
        };

        context.events().emit_namespaced(
            EventId::core("plugin_ready"),
            &ready_event,
        ).await.map_err(|e| PluginError::InitializationFailed(e.to_string()))?;

        info!("✅ RecipeSmith plugin fully initialized");
        Ok(())
    }

    async fn shutdown(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        info!("🛑 Shutting down RecipeSmith plugin...");

        // Cancel all active operations
        match self.queue_manager.get_all_active_operations().await {
            Ok(active_operations) => {
                for operation in active_operations {
                    if let Err(e) = self.queue_manager.cancel_operation(&operation.id).await {
                        warn!("Failed to cancel operation {} during shutdown: {}", operation.id, e);
                    }
                }
            },
            Err(e) => {
                warn!("Failed to get active operations during shutdown: {}", e);
            }
        }

        // Clear player states
        self.player_states.clear();

        // Emit plugin shutdown event
        let shutdown_event = PluginShutdownEvent {
            plugin_name: self.name.clone(),
            reason: "Server shutdown".to_string(),
            timestamp: current_timestamp(),
        };

        if let Err(e) = context.events().emit_namespaced(
            EventId::core("plugin_shutdown"),
            &shutdown_event,
        ).await {
            warn!("Failed to emit plugin shutdown event: {}", e);
        }

        info!("✅ RecipeSmith plugin shutdown complete");
        Ok(())
    }
}

// ============================================================================
// Plugin Creation Functions (Required for Dynamic Loading)
// ============================================================================

/// Create plugin instance - required export for dynamic loading
#[no_mangle]
pub unsafe extern "C" fn create_plugin() -> *mut dyn Plugin {
    let plugin = Box::new(RecipeSmithPlugin::new());
    Box::into_raw(plugin)
}

/// Destroy plugin instance - required export for dynamic loading
#[no_mangle]
pub unsafe extern "C" fn destroy_plugin(plugin: *mut dyn Plugin) {
    if !plugin.is_null() {
        let _ = Box::from_raw(plugin);
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Get current timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Generate random UUID for operations
fn generate_operation_id() -> CraftingOperationId {
    CraftingOperationId(Uuid::new_v4())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_plugin_creation() {
        let plugin = RecipeSmithPlugin::new();
        assert_eq!(plugin.name(), "recipe_smith");
        assert_eq!(plugin.version(), "1.0.0");
    }

    #[tokio::test]
    async fn test_recipe_loading() {
        let plugin = RecipeSmithPlugin::new();
        let recipes = plugin.get_default_recipes();
        assert!(!recipes.is_empty());
        assert_eq!(recipes[0].name, "Iron Ingot");
    }

    #[tokio::test]
    async fn test_crafting_operation_creation() {
        let plugin = RecipeSmithPlugin::new();
        let recipe = plugin.get_default_recipes()[0].clone();
        
        let operation = CraftingOperation {
            id: CraftingOperationId::new(),
            player_id: PlayerId::new(),
            recipe,
            quantity: 1,
            ingredient_sources: vec![],
            started_at: SystemTime::now(),
            estimated_completion: SystemTime::now() + Duration::from_secs(5),
            progress: 0.0,
            status: CraftingStatus::InProgress,
            quality_modifiers: vec![],
            experience_multiplier: 1.0,
        };
        
        assert_eq!(operation.quantity, 1);
        assert_eq!(operation.progress, 0.0);
    }
}