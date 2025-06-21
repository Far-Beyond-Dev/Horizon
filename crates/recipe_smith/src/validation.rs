//! Crafting validation system for RecipeSmith
//! 
//! Handles:
//! - Recipe requirement validation
//! - Ingredient availability checking
//! - Skill level verification
//! - Tool and station requirements
//! - Environmental condition validation
//! - Quality modifier calculation
//! - Experience multiplier determination
//! - Integration with other plugin systems

use crate::types::*;
use dashmap::DashMap;
use event_system::types::PlayerId;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

// ============================================================================
// Crafting Validator Implementation
// ============================================================================

/// Handler for integration with external systems
pub trait IntegrationHandler: Send + Sync {
    /// Check if integration requirements are met
    fn check_requirements(
        &self,
        player_id: &PlayerId,
        requirements: &serde_json::Value,
    ) -> Result<IntegrationValidationResult, CraftingError>;
    
    /// Get quality modifiers from this integration
    fn get_quality_modifiers(
        &self,
        player_id: &PlayerId,
        context: &serde_json::Value,
    ) -> Vec<QualityModifier>;
    
    /// Get experience multiplier from this integration
    fn get_experience_multiplier(
        &self,
        player_id: &PlayerId,
        context: &serde_json::Value,
    ) -> f32;
}

/// Validates crafting requests against various requirements and conditions
pub struct CraftingValidator {
    /// Cached validation rules
    cached_rules: DashMap<String, ValidationRule>,
    /// Integration handlers for external systems
    integration_handlers: DashMap<String, Box<dyn IntegrationHandler>>,
    /// Global validation settings
    settings: ValidationSettings,
}

impl CraftingValidator {
    /// Create a new crafting validator
    pub fn new() -> Self {
        Self {
            cached_rules: DashMap::new(),
            integration_handlers: DashMap::new(),
            settings: ValidationSettings::default(),
        }
    }

    /// Validate a crafting request comprehensively
    pub async fn validate_crafting_request(
        &self,
        request: &CraftingRequestEvent,
        recipe: &Recipe,
        player_states: &DashMap<PlayerId, PlayerCraftingState>,
        integrations: &DashMap<String, PluginIntegration>,
    ) -> Result<ValidationResult, CraftingError> {
        debug!("🔍 Validating crafting request for player {} recipe {}", 
               request.player_id, request.recipe_id);

        // Check cache first if enabled
        if self.settings.cache_validation_results {
            if let Some(cached) = self.get_cached_validation(request, recipe).await {
                debug!("📋 Using cached validation result");
                return Ok(cached);
            }
        }

        let mut validation_result = ValidationResult {
            is_valid: true,
            failure_reason: None,
            quality_modifiers: Vec::new(),
            experience_multiplier: 1.0,
            warnings: Vec::new(),
        };

        // Get player state
        let player_state = player_states.get(&request.player_id)
            .ok_or_else(|| CraftingError::ValidationFailed("Player state not found".to_string()))?;

        // 1. Validate recipe knowledge
        if !self.validate_recipe_knowledge(&player_state, recipe) {
            validation_result.is_valid = false;
            validation_result.failure_reason = Some("Recipe not known to player".to_string());
            return Ok(validation_result);
        }

        // 2. Validate skill requirements
        if let Err(reason) = self.validate_skill_requirements(&player_state, recipe, integrations).await {
            validation_result.is_valid = false;
            validation_result.failure_reason = Some(reason);
            return Ok(validation_result);
        }

        // 3. Validate ingredient availability
        match self.validate_ingredient_availability(request, recipe, &player_state).await {
            Ok(ingredient_quality_modifiers) => {
                validation_result.quality_modifiers.extend(ingredient_quality_modifiers);
            }
            Err(reason) => {
                validation_result.is_valid = false;
                validation_result.failure_reason = Some(reason.to_string());
                return Ok(validation_result);
            }
        }

        // 4. Validate tool requirements
        if let Err(reason) = self.validate_tool_requirements(&player_state, recipe, integrations).await {
            if self.settings.check_tool_durability {
                validation_result.is_valid = false;
                validation_result.failure_reason = Some(reason);
                return Ok(validation_result);
            } else {
                validation_result.warnings.push(reason);
            }
        }

        // 5. Validate station requirements
        match self.validate_station_requirements(&player_state, recipe, integrations).await {
            Ok(station_modifiers) => {
                validation_result.quality_modifiers.extend(station_modifiers);
            }
            Err(reason) => {
                validation_result.is_valid = false;
                validation_result.failure_reason = Some(reason.to_string());
                return Ok(validation_result);
            }
        }

        // 6. Validate environmental requirements
        if self.settings.check_environmental_conditions {
            match self.validate_environmental_requirements(&player_state, recipe, integrations).await {
                Ok(env_modifiers) => {
                    validation_result.quality_modifiers.extend(env_modifiers);
                }
                Err(reason) => {
                    validation_result.warnings.push(reason.to_string());
                    // Environmental requirements are often warnings, not hard failures
                }
            }
        }

        // 7. Calculate experience multipliers from all sources
        validation_result.experience_multiplier = self.calculate_experience_multiplier(
            &player_state,
            recipe,
            integrations,
            &validation_result.quality_modifiers,
        ).await;

        // 8. Perform integration-specific validations
        for integration in integrations.iter() {
            match self.validate_integration_requirements(
                &request.player_id,
                recipe,
                integration.value(),
            ).await {
                Ok(integration_result) => {
                    validation_result.quality_modifiers.extend(integration_result.quality_modifiers);
                    validation_result.experience_multiplier *= integration_result.experience_multiplier;
                    validation_result.warnings.extend(integration_result.warnings);
                    
                    if !integration_result.is_valid {
                        validation_result.is_valid = false;
                        validation_result.failure_reason = integration_result.failure_reason;
                        return Ok(validation_result);
                    }
                }
                Err(e) => {
                    warn!("Integration validation failed for {}: {}", integration.key(), e);
                    validation_result.warnings.push(format!("Integration {} validation warning: {}", integration.key(), e));
                }
            }
        }

        // 9. Final validation checks
        if request.quantity == 0 {
            validation_result.is_valid = false;
            validation_result.failure_reason = Some("Invalid quantity: must be greater than 0".to_string());
            return Ok(validation_result);
        }

        if request.quantity > 1000 {
            validation_result.warnings.push("Large quantity requested - may take significant time".to_string());
        }

        // Cache the result if caching is enabled
        if self.settings.cache_validation_results && validation_result.is_valid {
            self.cache_validation_result(request, recipe, &validation_result).await;
        }

        if validation_result.is_valid {
            info!("✅ Validation passed for player {} recipe {} (quality modifiers: {}, exp multiplier: {:.2})", 
                  request.player_id, request.recipe_id, 
                  validation_result.quality_modifiers.len(), validation_result.experience_multiplier);
        } else {
            info!("❌ Validation failed for player {} recipe {}: {}", 
                  request.player_id, request.recipe_id, 
                  validation_result.failure_reason.as_deref().unwrap_or("Unknown reason"));
        }

        Ok(validation_result)
    }

    /// Register an integration handler for external system validation
    pub fn register_integration_handler(
        &mut self,
        integration_name: String,
        handler: Box<dyn IntegrationHandler>,
    ) {
        info!("🔗 Registering integration handler for: {}", integration_name);
        self.integration_handlers.insert(integration_name, handler);
    }

    /// Update validation settings
    pub fn update_settings(&mut self, settings: ValidationSettings) {
        self.settings = settings;
        info!("⚙️ Updated validation settings");
    }

    /// Clear validation cache
    pub async fn clear_cache(&self) {
        self.cached_rules.clear();
        info!("🧹 Cleared validation cache");
    }

    /// Get validation statistics
    pub async fn get_validation_stats(&self) -> ValidationStats {
        ValidationStats {
            cached_rules_count: self.cached_rules.len(),
            integration_handlers_count: self.integration_handlers.len(),
            cache_hit_rate: 0.0, // Would need to track hits/misses
            average_validation_time_ms: 0.0, // Would need to track timing
            total_validations_performed: 0, // Would need to track counter
        }
    }

    // ========================================================================
    // Individual Validation Methods
    // ========================================================================

    /// Validate that the player knows the recipe
    fn validate_recipe_knowledge(&self, player_state: &PlayerCraftingState, recipe: &Recipe) -> bool {
        player_state.known_recipes.contains(&recipe.id)
    }

    /// Validate skill requirements
    async fn validate_skill_requirements(
        &self,
        player_state: &PlayerCraftingState,
        recipe: &Recipe,
        integrations: &DashMap<String, PluginIntegration>,
    ) -> Result<(), String> {
        for (skill_name, &required_level) in &recipe.requirements.skill_requirements {
            // Check if we have a skill system integration
            let player_skill_level = if let Some(skill_integration) = integrations.get("skill_system") {
                // Query skill system for player's actual skill level
                self.get_player_skill_level(&player_state.player_id, skill_name, skill_integration.value())
                    .await
                    .unwrap_or(1) // Default to level 1 if not found
            } else {
                // Fallback to crafting level or experience points
                player_state.experience_points.get(skill_name)
                    .map(|&exp| self.experience_to_level(exp))
                    .unwrap_or(player_state.crafting_level)
            };

            if player_skill_level < required_level {
                return Err(format!(
                    "Insufficient {} skill: required {}, have {}",
                    skill_name, required_level, player_skill_level
                ));
            }
        }

        Ok(())
    }

    /// Validate ingredient availability and quality
    async fn validate_ingredient_availability(
        &self,
        request: &CraftingRequestEvent,
        recipe: &Recipe,
        _player_state: &PlayerCraftingState,
    ) -> Result<Vec<QualityModifier>, CraftingError> {
        let mut quality_modifiers = Vec::new();

        // Create a map of provided ingredient sources
        let mut provided_ingredients: HashMap<u32, u32> = HashMap::new();
        for source in &request.ingredient_sources {
            *provided_ingredients.entry(source.item_id).or_insert(0) += source.quantity;
        }

        // Check each required ingredient
        for ingredient in &recipe.ingredients {
            let required_quantity = ingredient.quantity * request.quantity;
            let provided_quantity = provided_ingredients.get(&ingredient.item_id).unwrap_or(&0);

            if *provided_quantity < required_quantity {
                return Err(CraftingError::InsufficientIngredients(
                    format!("Need {} of item {}, have {}", required_quantity, ingredient.item_id, provided_quantity)
                ));
            }

            // Check quality requirements if specified
            if let Some(required_quality) = ingredient.quality_requirement {
                // In a real implementation, you would check the actual quality of the provided items
                // For now, we'll assume they meet requirements and add quality modifiers
                
                // Simulate quality check - in practice this would query inventory system
                let average_quality = 1.0; // Would get from inventory integration
                
                if average_quality >= required_quality {
                    quality_modifiers.push(QualityModifier {
                        source: format!("ingredient_{}", ingredient.item_id),
                        modifier: average_quality,
                        modifier_type: QualityModifierType::BaseQuality,
                    });
                } else if self.settings.allow_low_quality_substitution {
                    // Allow but with penalty
                    quality_modifiers.push(QualityModifier {
                        source: format!("low_quality_ingredient_{}", ingredient.item_id),
                        modifier: 0.8, // 20% penalty
                        modifier_type: QualityModifierType::BaseQuality,
                    });
                } else {
                    return Err(CraftingError::RequirementsNotMet(
                        format!("Ingredient {} quality {} below required {}", 
                               ingredient.item_id, average_quality, required_quality)
                    ));
                }
            }

            // If excess ingredients are not allowed, check for exact match
            if !self.settings.allow_excess_ingredients && *provided_quantity > required_quantity {
                return Err(CraftingError::ValidationFailed(
                    format!("Excess ingredients not allowed: provided {} of item {}, need exactly {}", 
                           provided_quantity, ingredient.item_id, required_quantity)
                ));
            }
        }

        Ok(quality_modifiers)
    }

    /// Validate tool requirements
    async fn validate_tool_requirements(
        &self,
        player_state: &PlayerCraftingState,
        recipe: &Recipe,
        integrations: &DashMap<String, PluginIntegration>,
    ) -> Result<(), String> {
        if recipe.requirements.tool_requirements.is_empty() {
            return Ok(());
        }

        // Check if we have an inventory system integration
        if let Some(inventory_integration) = integrations.get("inventory_system") {
            for required_tool in &recipe.requirements.tool_requirements {
                let has_tool = self.check_player_has_tool(
                    &player_state.player_id,
                    required_tool,
                    inventory_integration.value(),
                ).await?;

                if !has_tool {
                    return Err(format!("Required tool not found: {}", required_tool));
                }

                // Check tool durability if setting is enabled
                if self.settings.check_tool_durability {
                    let durability = self.get_tool_durability(
                        &player_state.player_id,
                        required_tool,
                        inventory_integration.value(),
                    ).await.unwrap_or(1.0);

                    if durability <= 0.1 {
                        return Err(format!("Tool {} is too damaged to use", required_tool));
                    }
                }
            }
        } else {
            // No inventory integration - assume tools are available
            debug!("No inventory integration available - assuming tools are present");
        }

        Ok(())
    }

    /// Validate station requirements
    async fn validate_station_requirements(
        &self,
        player_state: &PlayerCraftingState,
        recipe: &Recipe,
        integrations: &DashMap<String, PluginIntegration>,
    ) -> Result<Vec<QualityModifier>, CraftingError> {
        let mut quality_modifiers = Vec::new();

        if let Some(required_station) = &recipe.requirements.station_requirement {
            // Check if we have a station system integration
            if let Some(station_integration) = integrations.get("station_system") {
                let station_info = self.check_player_station_access(
                    &player_state.player_id,
                    required_station,
                    station_integration.value(),
                ).await?;

                if !station_info.has_access {
                    return Err(CraftingError::RequirementsNotMet(
                        format!("Required crafting station not accessible: {}", required_station)
                    ));
                }

                // Add station quality modifiers
                if let Some(station_quality) = station_info.quality_modifier {
                    quality_modifiers.push(QualityModifier {
                        source: format!("station_{}", required_station),
                        modifier: station_quality,
                        modifier_type: QualityModifierType::BaseQuality,
                    });
                }

                if let Some(speed_bonus) = station_info.speed_modifier {
                    quality_modifiers.push(QualityModifier {
                        source: format!("station_{}_speed", required_station),
                        modifier: speed_bonus,
                        modifier_type: QualityModifierType::SpeedBonus,
                    });
                }
            } else {
                // No station integration - require manual check or fail
                return Err(CraftingError::RequirementsNotMet(
                    format!("Station system not available to verify access to: {}", required_station)
                ));
            }
        }

        Ok(quality_modifiers)
    }

    /// Validate environmental requirements
    async fn validate_environmental_requirements(
        &self,
        player_state: &PlayerCraftingState,
        recipe: &Recipe,
        integrations: &DashMap<String, PluginIntegration>,
    ) -> Result<Vec<QualityModifier>, CraftingError> {
        let mut quality_modifiers = Vec::new();

        if recipe.requirements.environmental_requirements.is_empty() {
            return Ok(quality_modifiers);
        }

        // Check if we have a world/environment system integration
        if let Some(world_integration) = integrations.get("world_system") {
            for env_requirement in &recipe.requirements.environmental_requirements {
                let env_status = self.check_environmental_condition(
                    &player_state.player_id,
                    env_requirement,
                    world_integration.value(),
                ).await?;

                if !env_status.condition_met {
                    return Err(CraftingError::RequirementsNotMet(
                        format!("Environmental requirement not met: {}", env_requirement)
                    ));
                }

                // Add environmental quality modifiers
                if let Some(quality_bonus) = env_status.quality_modifier {
                    quality_modifiers.push(QualityModifier {
                        source: format!("environment_{}", env_requirement),
                        modifier: quality_bonus,
                        modifier_type: QualityModifierType::BaseQuality,
                    });
                }
            }
        } else {
            // No world integration - assume requirements are met with warnings
            debug!("No world integration available - assuming environmental requirements are met");
        }

        Ok(quality_modifiers)
    }

    /// Calculate experience multiplier from all sources
    async fn calculate_experience_multiplier(
        &self,
        player_state: &PlayerCraftingState,
        recipe: &Recipe,
        integrations: &DashMap<String, PluginIntegration>,
        quality_modifiers: &[QualityModifier],
    ) -> f32 {
        let mut multiplier = 1.0;

        // Base multiplier from player crafting level
        let level_bonus = (player_state.crafting_level as f32 - 1.0) * 0.01; // 1% per level above 1
        multiplier += level_bonus;

        // Multiplier from quality modifiers
        for modifier in quality_modifiers {
            if let QualityModifierType::ExperienceBonus = modifier.modifier_type {
                multiplier *= modifier.modifier;
            }
        }

        // Multiplier from recipe difficulty
        let recipe_difficulty = self.calculate_recipe_difficulty(recipe);
        multiplier *= 1.0 + (recipe_difficulty * 0.1); // 10% bonus per difficulty point

        // Multiplier from integrations (skill system, etc.)
        for integration in integrations.iter() {
            if let Ok(integration_multiplier) = self.get_integration_experience_multiplier(
                &player_state.player_id,
                recipe,
                integration.value(),
            ).await {
                multiplier *= integration_multiplier;
            }
        }

        multiplier.max(0.1) // Minimum 10% of base experience
    }

    /// Validate integration-specific requirements
    async fn validate_integration_requirements(
        &self,
        player_id: &PlayerId,
        recipe: &Recipe,
        integration: &PluginIntegration,
    ) -> Result<IntegrationValidationResult, CraftingError> {
        // Check if we have a handler for this integration
        if let Some(handler) = self.integration_handlers.get(&integration.plugin_name) {
            let requirements = serde_json::to_value(&recipe.requirements)
                .map_err(|e| CraftingError::SystemError(format!("Failed to serialize requirements: {}", e)))?;

            handler.check_requirements(player_id, &requirements)
        } else {
            // No specific handler - perform generic validation
            Ok(IntegrationValidationResult {
                is_valid: true,
                failure_reason: None,
                quality_modifiers: vec![],
                experience_multiplier: 1.0,
                warnings: vec![],
            })
        }
    }

    // ========================================================================
    // Helper Methods for Integration Queries
    // ========================================================================

    /// Get player skill level from skill system integration
    async fn get_player_skill_level(
        &self,
        _player_id: &PlayerId,
        _skill_name: &str,
        _integration: &PluginIntegration,
    ) -> Result<u32, CraftingError> {
        // In a real implementation, this would query the skill system plugin
        // For now, return a default value
        Ok(10) // Assume level 10
    }

    /// Check if player has a required tool
    async fn check_player_has_tool(
        &self,
        _player_id: &PlayerId,
        _tool_name: &str,
        _integration: &PluginIntegration,
    ) -> Result<bool, String> {
        // In a real implementation, this would query the inventory system
        Ok(true) // Assume tool is available
    }

    /// Get tool durability
    async fn get_tool_durability(
        &self,
        _player_id: &PlayerId,
        _tool_name: &str,
        _integration: &PluginIntegration,
    ) -> Result<f32, CraftingError> {
        // In a real implementation, this would query the inventory system
        Ok(1.0) // Assume full durability
    }

    /// Check player access to a crafting station
    async fn check_player_station_access(
        &self,
        _player_id: &PlayerId,
        _station_name: &str,
        _integration: &PluginIntegration,
    ) -> Result<StationAccessInfo, CraftingError> {
        // In a real implementation, this would query the station system
        Ok(StationAccessInfo {
            has_access: true,
            quality_modifier: Some(1.2), // 20% quality bonus
            speed_modifier: Some(1.1),   // 10% speed bonus
        })
    }

    /// Check environmental condition
    async fn check_environmental_condition(
        &self,
        _player_id: &PlayerId,
        _condition: &str,
        _integration: &PluginIntegration,
    ) -> Result<EnvironmentalStatus, CraftingError> {
        // In a real implementation, this would query the world system
        Ok(EnvironmentalStatus {
            condition_met: true,
            quality_modifier: Some(1.05), // 5% quality bonus
        })
    }

    /// Get experience multiplier from an integration
    async fn get_integration_experience_multiplier(
        &self,
        _player_id: &PlayerId,
        _recipe: &Recipe,
        _integration: &PluginIntegration,
    ) -> Result<f32, CraftingError> {
        // In a real implementation, this would query the integration
        Ok(1.0) // No bonus
    }

    // ========================================================================
    // Utility Methods
    // ========================================================================

    /// Convert experience points to level
    fn experience_to_level(&self, experience: u32) -> u32 {
        // Simple formula: level = sqrt(experience / 100) + 1
        ((experience as f32 / 100.0).sqrt() + 1.0) as u32
    }

    /// Calculate recipe difficulty score
    fn calculate_recipe_difficulty(&self, recipe: &Recipe) -> f32 {
        let mut difficulty = 0.0;

        // Base difficulty from number of ingredients
        difficulty += recipe.ingredients.len() as f32 * 0.2;

        // Difficulty from skill requirements
        let max_skill_requirement = recipe.requirements.skill_requirements
            .values()
            .max()
            .unwrap_or(&1)
            .clone();
        difficulty += (max_skill_requirement as f32 - 1.0) * 0.1;

        // Difficulty from failure chance
        difficulty += recipe.failure_chance * 2.0;

        // Difficulty from crafting time
        difficulty += recipe.timing.base_duration.as_secs() as f32 / 60.0 * 0.1; // Per minute

        difficulty.max(1.0)
    }

    /// Get cached validation result
    async fn get_cached_validation(&self, request: &CraftingRequestEvent, recipe: &Recipe) -> Option<ValidationResult> {
        let cache_key = format!("{}:{}:{}", request.player_id, recipe.id.0, request.quantity);
        
        if let Some(cached_rule) = self.cached_rules.get(&cache_key) {
            if std::time::SystemTime::now() < cached_rule.expires_at {
                return Some(cached_rule.cached_result.clone());
            } else {
                // Remove expired cache entry
                self.cached_rules.remove(&cache_key);
            }
        }

        None
    }

    /// Cache validation result
    async fn cache_validation_result(
        &self,
        request: &CraftingRequestEvent,
        recipe: &Recipe,
        result: &ValidationResult,
    ) {
        let cache_key = format!("{}:{}:{}", request.player_id, recipe.id.0, request.quantity);
        let now = std::time::SystemTime::now();
        let expires_at = now + std::time::Duration::from_secs(self.settings.cache_ttl_seconds);

        let rule = ValidationRule {
            rule_id: cache_key.clone(),
            cached_result: result.clone(),
            created_at: now,
            expires_at,
        };

        self.cached_rules.insert(cache_key, rule);
    }
}

// ============================================================================
// Supporting Types
// ============================================================================

/// Information about crafting station access
#[derive(Debug, Clone)]
pub struct StationAccessInfo {
    pub has_access: bool,
    pub quality_modifier: Option<f32>,
    pub speed_modifier: Option<f32>,
}

/// Status of environmental conditions
#[derive(Debug, Clone)]
pub struct EnvironmentalStatus {
    pub condition_met: bool,
    pub quality_modifier: Option<f32>,
}

/// Validation statistics
#[derive(Debug, Clone)]
pub struct ValidationStats {
    pub cached_rules_count: usize,
    pub integration_handlers_count: usize,
    pub cache_hit_rate: f64,
    pub average_validation_time_ms: f64,
    pub total_validations_performed: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn create_test_validator() -> CraftingValidator {
        CraftingValidator::new()
    }

    fn create_test_recipe() -> Recipe {
        use std::collections::HashMap;

        Recipe {
            id: RecipeId(1001),
            name: "Test Recipe".to_string(),
            description: "A test recipe".to_string(),
            category: "Test".to_string(),
            ingredients: vec![
                Ingredient {
                    item_id: 1,
                    quantity: 2,
                    quality_requirement: None,
                }
            ],
            main_product: CraftingResult {
                item_id: 2,
                quantity: 1,
                quality_bonus: None,
            },
            byproducts: vec![],
            requirements: CraftingRequirements {
                skill_requirements: {
                    let mut skills = HashMap::new();
                    skills.insert("Crafting".to_string(), 5);
                    skills
                },
                station_requirement: Some("Workbench".to_string()),
                tool_requirements: vec!["Hammer".to_string()],
                environmental_requirements: vec!["Indoor".to_string()],
            },
            timing: CraftingTiming {
                base_duration: Duration::from_secs(10),
                skill_speed_bonus: None,
                station_speed_bonus: None,
            },
            unlock_conditions: vec![],
            experience_rewards: HashMap::new(),
            failure_chance: 0.1,
            critical_success_chance: 0.05,
        }
    }

    fn create_test_player_state() -> PlayerCraftingState {
        PlayerCraftingState {
            player_id: PlayerId::new(),
            known_recipes: vec![RecipeId(1001)],
            active_operations: vec![],
            total_crafting_attempts: 0,
            successful_crafts: 0,
            crafting_level: 10,
            experience_points: {
                let mut exp = HashMap::new();
                exp.insert("Crafting".to_string(), 500);
                exp
            },
            unlocked_categories: vec!["Test".to_string()],
            preferences: CraftingPreferences::default(),
        }
    }

    fn create_test_request(player_id: PlayerId) -> CraftingRequestEvent {
        CraftingRequestEvent {
            player_id,
            recipe_id: RecipeId(1001),
            quantity: 1,
            ingredient_sources: vec![
                InventorySlot {
                    slot_id: 1,
                    item_id: 1,
                    quantity: 2,
                    quality: Some(1.0),
                }
            ],
        }
    }

    #[tokio::test]
    async fn test_recipe_knowledge_validation() {
        let validator = create_test_validator();
        let player_state = create_test_player_state();
        let recipe = create_test_recipe();

        assert!(validator.validate_recipe_knowledge(&player_state, &recipe));

        // Test unknown recipe
        let unknown_recipe = Recipe {
            id: RecipeId(9999),
            ..recipe
        };
        assert!(!validator.validate_recipe_knowledge(&player_state, &unknown_recipe));
    }

    #[tokio::test]
    async fn test_ingredient_availability_validation() {
        let validator = create_test_validator();
        let player_state = create_test_player_state();
        let recipe = create_test_recipe();
        let request = create_test_request(player_state.player_id);

        let result = validator.validate_ingredient_availability(&request, &recipe, &player_state).await;
        assert!(result.is_ok());

        // Test insufficient ingredients
        let mut insufficient_request = request.clone();
        insufficient_request.ingredient_sources[0].quantity = 1; // Need 2, have 1

        let result = validator.validate_ingredient_availability(&insufficient_request, &recipe, &player_state).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_experience_to_level_conversion() {
        let validator = create_test_validator();

        assert_eq!(validator.experience_to_level(0), 1);
        assert_eq!(validator.experience_to_level(100), 2);
        assert_eq!(validator.experience_to_level(400), 3);
        assert_eq!(validator.experience_to_level(900), 4);
    }

    #[tokio::test]
    async fn test_recipe_difficulty_calculation() {
        let validator = create_test_validator();
        let recipe = create_test_recipe();

        let difficulty = validator.calculate_recipe_difficulty(&recipe);
        assert!(difficulty >= 1.0); // Should always be at least 1.0
        assert!(difficulty < 10.0); // Reasonable upper bound for test recipe
    }

    #[tokio::test]
    async fn test_validation_settings() {
        let mut validator = create_test_validator();
        
        let mut settings = ValidationSettings::default();
        settings.allow_low_quality_substitution = false;
        settings.cache_ttl_seconds = 600;

        validator.update_settings(settings);
        
        assert!(!validator.settings.allow_low_quality_substitution);
        assert_eq!(validator.settings.cache_ttl_seconds, 600);
    }
}