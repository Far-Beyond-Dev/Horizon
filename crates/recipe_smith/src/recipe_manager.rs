//! Recipe management system for RecipeSmith
//! 
//! Handles:
//! - Recipe storage and retrieval
//! - Recipe unlocking and discovery
//! - Recipe validation and requirements checking
//! - Recipe categorization and filtering
//! - Dynamic recipe modification

use crate::types::*;
use dashmap::DashMap;
use event_system::types::PlayerId;
use serde_json;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use tokio::fs;
use tracing::{debug, error, info, warn};

// ============================================================================
// Recipe Manager Implementation
// ============================================================================

/// Manages all recipes in the crafting system
pub struct RecipeManager {
    /// All recipes stored by ID
    recipes: DashMap<RecipeId, Arc<Recipe>>,
    /// Recipes organized by category
    categories: DashMap<String, Vec<RecipeId>>,
    /// Recipe unlock dependencies
    unlock_graph: DashMap<RecipeId, Vec<RecipeId>>, // recipe -> unlocks
    /// Reverse dependency mapping
    dependency_graph: DashMap<RecipeId, Vec<RecipeId>>, // recipe -> depends on
    /// Skill-based recipe unlocks
    skill_unlocks: DashMap<String, Vec<(u32, RecipeId)>>, // skill -> (level, recipe)
    /// Recipe discovery patterns
    discovery_patterns: DashMap<String, Vec<RecipeId>>, // ingredient_combination -> recipes
    /// Recipe statistics
    stats: DashMap<RecipeId, RecipeStats>,
}

impl RecipeManager {
    /// Create a new recipe manager
    pub fn new() -> Self {
        Self {
            recipes: DashMap::new(),
            categories: DashMap::new(),
            unlock_graph: DashMap::new(),
            dependency_graph: DashMap::new(),
            skill_unlocks: DashMap::new(),
            discovery_patterns: DashMap::new(),
            stats: DashMap::new(),
        }
    }

    /// Add a recipe to the manager
    pub async fn add_recipe(&self, recipe: Recipe) -> Result<(), CraftingError> {
        let recipe_id = recipe.id;
        let category = recipe.category.clone();
        let recipe_arc = Arc::new(recipe);

        info!("📋 Adding recipe: {} ({})", recipe_arc.name, recipe_id);

        // Store the recipe
        self.recipes.insert(recipe_id, recipe_arc.clone());

        // Update category index
        self.categories
            .entry(category)
            .or_insert_with(Vec::new)
            .push(recipe_id);

        // Build unlock dependencies
        self.build_unlock_dependencies(&recipe_arc).await?;

        // Build skill unlock mappings
        self.build_skill_unlock_mappings(&recipe_arc).await;

        // Build discovery patterns
        self.build_discovery_patterns(&recipe_arc).await;

        // Initialize statistics
        self.stats.insert(recipe_id, RecipeStats::default());

        debug!("✅ Recipe {} added successfully", recipe_id);
        Ok(())
    }

    /// Get a recipe by ID
    pub async fn get_recipe(&self, recipe_id: &RecipeId) -> Option<Arc<Recipe>> {
        self.recipes.get(recipe_id).map(|entry| entry.clone())
    }

    /// Get all recipes in a category
    pub async fn get_recipes_by_category(&self, category: &str) -> Vec<Arc<Recipe>> {
        if let Some(recipe_ids) = self.categories.get(category) {
            recipe_ids
                .iter()
                .filter_map(|id| self.recipes.get(id).map(|entry| entry.clone()))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get all known recipes for a player
    pub async fn get_player_recipes(
        &self,
        known_recipe_ids: &[RecipeId],
    ) -> Vec<Arc<Recipe>> {
        known_recipe_ids
            .iter()
            .filter_map(|id| self.recipes.get(id).map(|entry| entry.clone()))
            .collect()
    }

    /// Check which recipes a player can unlock based on skill level update
    pub async fn check_recipe_unlocks(
        &self,
        player_id: &PlayerId,
        skill_name: &str,
        skill_level: u32,
        known_recipes: &[RecipeId],
    ) -> Vec<RecipeId> {
        let mut newly_unlocked = Vec::new();
        let known_set: HashSet<_> = known_recipes.iter().copied().collect();

        // Check skill-based unlocks
        if let Some(skill_unlocks) = self.skill_unlocks.get(skill_name) {
            for &(required_level, recipe_id) in skill_unlocks.iter() {
                if skill_level >= required_level && !known_set.contains(&recipe_id) {
                    // Check if all other unlock conditions are met
                    if let Some(recipe) = self.recipes.get(&recipe_id) {
                        if self.check_unlock_conditions(&recipe, known_recipes, player_id).await {
                            newly_unlocked.push(recipe_id);
                        }
                    }
                }
            }
        }

        // Check dependency-based unlocks (recipes that depend on other recipes)
        for known_recipe in known_recipes {
            if let Some(unlocks) = self.unlock_graph.get(known_recipe) {
                for &unlocked_recipe in unlocks.iter() {
                    if !known_set.contains(&unlocked_recipe) {
                        if let Some(recipe) = self.recipes.get(&unlocked_recipe) {
                            if self.check_unlock_conditions(&recipe, known_recipes, player_id).await {
                                newly_unlocked.push(unlocked_recipe);
                            }
                        }
                    }
                }
            }
        }

        newly_unlocked.sort();
        newly_unlocked.dedup();

        if !newly_unlocked.is_empty() {
            info!("🔓 Player {} unlocked {} recipes via skill {} level {}", 
                  player_id, newly_unlocked.len(), skill_name, skill_level);
        }

        newly_unlocked
    }

    /// Attempt recipe discovery through experimentation
    pub async fn attempt_recipe_discovery(
        &self,
        ingredients: &[u32], // item IDs
        player_known_recipes: &[RecipeId],
    ) -> Vec<RecipeId> {
        let mut discovered = Vec::new();
        let known_set: HashSet<_> = player_known_recipes.iter().copied().collect();

        // Sort ingredients to create a consistent key
        let mut sorted_ingredients = ingredients.to_vec();
        sorted_ingredients.sort();
        let ingredient_key = format!("{:?}", sorted_ingredients);

        // Check if this combination matches any discovery patterns
        if let Some(discoverable_recipes) = self.discovery_patterns.get(&ingredient_key) {
            for &recipe_id in discoverable_recipes.iter() {
                if !known_set.contains(&recipe_id) {
                    // Check if the player meets other requirements for this recipe
                    if let Some(recipe) = self.recipes.get(&recipe_id) {
                        // For discovery, we might have relaxed requirements
                        if self.check_discovery_requirements(&recipe).await {
                            discovered.push(recipe_id);
                        }
                    }
                }
            }
        }

        if !discovered.is_empty() {
            info!("🧪 Discovered {} recipes through experimentation", discovered.len());
        }

        discovered
    }

    /// Get all recipes (for admin/debug purposes)
    pub async fn get_all_recipes(&self) -> Vec<Arc<Recipe>> {
        self.recipes
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get recipe count
    pub async fn get_recipe_count(&self) -> usize {
        self.recipes.len()
    }

    /// Get recipes that can be crafted with available ingredients
    pub async fn get_craftable_recipes(
        &self,
        known_recipes: &[RecipeId],
        available_ingredients: &HashMap<u32, u32>, // item_id -> quantity
    ) -> Vec<RecipeId> {
        let mut craftable = Vec::new();

        for &recipe_id in known_recipes {
            if let Some(recipe) = self.recipes.get(&recipe_id) {
                if self.can_craft_with_ingredients(&recipe, available_ingredients) {
                    craftable.push(recipe_id);
                }
            }
        }

        craftable
    }

    /// Load recipes from a JSON file
    pub async fn load_recipes_from_file<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<usize, CraftingError> {
        let content = fs::read_to_string(path.as_ref()).await
            .map_err(|e| CraftingError::ConfigurationError(format!("Failed to read recipe file: {}", e)))?;

        let recipes: Vec<Recipe> = serde_json::from_str(&content)
            .map_err(|e| CraftingError::ConfigurationError(format!("Failed to parse recipe file: {}", e)))?;

        let count = recipes.len();
        for recipe in recipes {
            self.add_recipe(recipe).await?;
        }

        info!("📁 Loaded {} recipes from file", count);
        Ok(count)
    }

    /// Export recipes to a JSON file
    pub async fn export_recipes_to_file<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<(), CraftingError> {
        let all_recipes: Vec<Recipe> = self.recipes
            .iter()
            .map(|entry| (**entry.value()).clone())
            .collect();

        let content = serde_json::to_string_pretty(&all_recipes)
            .map_err(|e| CraftingError::SystemError(format!("Failed to serialize recipes: {}", e)))?;

        fs::write(path.as_ref(), content).await
            .map_err(|e| CraftingError::SystemError(format!("Failed to write recipe file: {}", e)))?;

        info!("💾 Exported {} recipes to file", all_recipes.len());
        Ok(())
    }

    /// Update recipe statistics
    pub async fn update_recipe_stats(
        &self,
        recipe_id: &RecipeId,
        success: bool,
        craft_time: std::time::Duration,
        quality: Option<f32>,
    ) {
        if let Some(mut stats) = self.stats.get_mut(recipe_id) {
            stats.times_crafted += 1;
            
            if success {
                stats.total_successes += 1;
                
                if let Some(quality_value) = quality {
                    // Update average quality
                    let total_quality_samples = stats.total_successes as f32;
                    stats.average_quality = (stats.average_quality * (total_quality_samples - 1.0) + quality_value) / total_quality_samples;
                }
            } else {
                stats.total_failures += 1;
            }

            // Update fastest craft time
            if stats.fastest_craft_time.is_none() || craft_time < stats.fastest_craft_time.unwrap() {
                stats.fastest_craft_time = Some(craft_time);
            }
        }
    }

    /// Get recipe statistics
    pub async fn get_recipe_stats(&self, recipe_id: &RecipeId) -> Option<RecipeStats> {
        self.stats.get(recipe_id).map(|entry| entry.clone())
    }

    /// Get most popular recipes
    pub async fn get_popular_recipes(&self, limit: usize) -> Vec<(RecipeId, u64)> {
        let mut recipe_counts: Vec<_> = self.stats
            .iter()
            .map(|entry| (*entry.key(), entry.times_crafted))
            .collect();

        recipe_counts.sort_by(|a, b| b.1.cmp(&a.1));
        recipe_counts.truncate(limit);
        recipe_counts
    }

    /// Remove a recipe (admin function)
    pub async fn remove_recipe(&self, recipe_id: &RecipeId) -> Option<Arc<Recipe>> {
        if let Some((_, recipe)) = self.recipes.remove(recipe_id) {
            // Clean up category index
            let category = &recipe.category;
            if let Some(mut cat_recipes) = self.categories.get_mut(category) {
                cat_recipes.retain(|&id| id != *recipe_id);
            }

            // Clean up unlock graphs
            self.unlock_graph.remove(recipe_id);
            self.dependency_graph.remove(recipe_id);

            // Clean up stats
            self.stats.remove(recipe_id);

            info!("🗑️ Removed recipe: {} ({})", recipe.name, recipe_id);
            Some(recipe)
        } else {
            None
        }
    }

    /// Validate a recipe for correctness
    pub async fn validate_recipe(&self, recipe: &Recipe) -> Result<(), CraftingError> {
        // Check for empty ingredients
        if recipe.ingredients.is_empty() {
            return Err(CraftingError::ValidationFailed("Recipe has no ingredients".to_string()));
        }

        // Check for invalid quantities
        for ingredient in &recipe.ingredients {
            if ingredient.quantity == 0 {
                return Err(CraftingError::ValidationFailed(
                    format!("Ingredient {} has zero quantity", ingredient.item_id)
                ));
            }
        }

        // Check main product
        if recipe.main_product.quantity == 0 {
            return Err(CraftingError::ValidationFailed("Main product has zero quantity".to_string()));
        }

        // Check probability values
        if recipe.failure_chance < 0.0 || recipe.failure_chance > 1.0 {
            return Err(CraftingError::ValidationFailed("Invalid failure chance".to_string()));
        }

        if recipe.critical_success_chance < 0.0 || recipe.critical_success_chance > 1.0 {
            return Err(CraftingError::ValidationFailed("Invalid critical success chance".to_string()));
        }

        // Check for circular dependencies in unlock conditions
        self.check_circular_dependencies(&recipe).await?;

        Ok(())
    }

    // ========================================================================
    // Private Helper Methods
    // ========================================================================

    /// Build unlock dependency graph
    async fn build_unlock_dependencies(&self, recipe: &Recipe) -> Result<(), CraftingError> {
        for unlock_condition in &recipe.unlock_conditions {
            if let UnlockCondition::RecipeKnown(dependency_recipe_id) = unlock_condition {
                // recipe depends on dependency_recipe_id
                self.dependency_graph
                    .entry(recipe.id)
                    .or_insert_with(Vec::new)
                    .push(*dependency_recipe_id);

                // dependency_recipe_id unlocks recipe
                self.unlock_graph
                    .entry(*dependency_recipe_id)
                    .or_insert_with(Vec::new)
                    .push(recipe.id);
            }
        }
        Ok(())
    }

    /// Build skill-based unlock mappings
    async fn build_skill_unlock_mappings(&self, recipe: &Recipe) {
        for unlock_condition in &recipe.unlock_conditions {
            if let UnlockCondition::SkillLevel(skill_name, required_level) = unlock_condition {
                self.skill_unlocks
                    .entry(skill_name.clone())
                    .or_insert_with(Vec::new)
                    .push((*required_level, recipe.id));
            }
        }
    }

    /// Build discovery patterns based on ingredients
    async fn build_discovery_patterns(&self, recipe: &Recipe) {
        // Create a discovery pattern based on main ingredients
        let mut ingredient_ids: Vec<u32> = recipe.ingredients
            .iter()
            .map(|ingredient| ingredient.item_id)
            .collect();
        
        ingredient_ids.sort();
        let pattern_key = format!("{:?}", ingredient_ids);

        self.discovery_patterns
            .entry(pattern_key)
            .or_insert_with(Vec::new)
            .push(recipe.id);
    }

    /// Check if all unlock conditions are met
    async fn check_unlock_conditions(
        &self,
        recipe: &Recipe,
        known_recipes: &[RecipeId],
        _player_id: &PlayerId,
    ) -> bool {
        let known_set: HashSet<_> = known_recipes.iter().copied().collect();

        for condition in &recipe.unlock_conditions {
            match condition {
                UnlockCondition::RecipeKnown(required_recipe) => {
                    if !known_set.contains(required_recipe) {
                        return false;
                    }
                }
                UnlockCondition::SkillLevel(_, _) => {
                    // This should be checked by the calling code
                    // We assume it's already satisfied if we're here
                }
                UnlockCondition::QuestCompleted(_) => {
                    // TODO: Integrate with quest system
                    // For now, assume completed
                }
                UnlockCondition::ItemDiscovered(_) => {
                    // TODO: Integrate with item discovery system
                    // For now, assume discovered
                }
                UnlockCondition::LocationRequirement(_) => {
                    // TODO: Integrate with location system
                    // For now, assume satisfied
                }
                UnlockCondition::CustomCondition(_, _) => {
                    // TODO: Handle custom conditions
                    // For now, assume satisfied
                }
            }
        }

        true
    }

    /// Check if discovery requirements are met (usually more lenient)
    async fn check_discovery_requirements(&self, _recipe: &Recipe) -> bool {
        // For discovery, we might have more relaxed requirements
        // For example, no skill requirements, just the right ingredients
        true
    }

    /// Check if a recipe can be crafted with available ingredients
    fn can_craft_with_ingredients(
        &self,
        recipe: &Recipe,
        available_ingredients: &HashMap<u32, u32>,
    ) -> bool {
        for ingredient in &recipe.ingredients {
            let available = available_ingredients.get(&ingredient.item_id).unwrap_or(&0);
            if *available < ingredient.quantity {
                return false;
            }
        }
        true
    }

    /// Check for circular dependencies in unlock conditions
    async fn check_circular_dependencies(&self, recipe: &Recipe) -> Result<(), CraftingError> {
        let mut visited = HashSet::new();
        let mut in_progress = HashSet::new();

        fn dfs(
            recipe_id: RecipeId,
            dependency_graph: &DashMap<RecipeId, Vec<RecipeId>>,
            visited: &mut HashSet<RecipeId>,
            in_progress: &mut HashSet<RecipeId>,
        ) -> bool {
            if in_progress.contains(&recipe_id) {
                return true; // Circular dependency found
            }

            if visited.contains(&recipe_id) {
                return false; // Already processed
            }

            in_progress.insert(recipe_id);

            if let Some(dependencies) = dependency_graph.get(&recipe_id) {
                for &dep_recipe_id in dependencies.iter() {
                    if dfs(dep_recipe_id, dependency_graph, visited, in_progress) {
                        return true;
                    }
                }
            }

            in_progress.remove(&recipe_id);
            visited.insert(recipe_id);
            false
        }

        if dfs(recipe.id, &self.dependency_graph, &mut visited, &mut in_progress) {
            return Err(CraftingError::ValidationFailed(
                format!("Circular dependency detected for recipe {}", recipe.id)
            ));
        }

        Ok(())
    }
}

// ============================================================================
// Recipe Search and Filter System
// ============================================================================

/// Criteria for searching recipes
#[derive(Debug, Clone)]
pub struct RecipeSearchCriteria {
    pub name_pattern: Option<String>,
    pub category: Option<String>,
    pub max_skill_level: Option<HashMap<String, u32>>,
    pub available_ingredients: Option<HashMap<u32, u32>>,
    pub max_craft_time: Option<std::time::Duration>,
    pub min_success_rate: Option<f32>,
}

impl RecipeManager {
    /// Search recipes based on criteria
    pub async fn search_recipes(
        &self,
        criteria: &RecipeSearchCriteria,
        known_recipes: &[RecipeId],
    ) -> Vec<Arc<Recipe>> {
        let known_set: HashSet<_> = known_recipes.iter().copied().collect();
        let mut results = Vec::new();

        for recipe_entry in self.recipes.iter() {
            let recipe = recipe_entry.value();
            
            // Only include known recipes
            if !known_set.contains(&recipe.id) {
                continue;
            }

            // Apply filters
            if let Some(name_pattern) = &criteria.name_pattern {
                if !recipe.name.to_lowercase().contains(&name_pattern.to_lowercase()) {
                    continue;
                }
            }

            if let Some(category) = &criteria.category {
                if recipe.category != *category {
                    continue;
                }
            }

            if let Some(max_skills) = &criteria.max_skill_level {
                let mut skill_check_passed = true;
                for (skill, &max_level) in max_skills {
                    if let Some(&required_level) = recipe.requirements.skill_requirements.get(skill) {
                        if required_level > max_level {
                            skill_check_passed = false;
                            break;
                        }
                    }
                }
                if !skill_check_passed {
                    continue;
                }
            }

            if let Some(available_ingredients) = &criteria.available_ingredients {
                if !self.can_craft_with_ingredients(recipe, available_ingredients) {
                    continue;
                }
            }

            if let Some(max_time) = &criteria.max_craft_time {
                if recipe.timing.base_duration > *max_time {
                    continue;
                }
            }

            if let Some(min_success_rate) = &criteria.min_success_rate {
                let success_rate = 1.0 - recipe.failure_chance;
                if success_rate < *min_success_rate {
                    continue;
                }
            }

            results.push(recipe.clone());
        }

        // Sort by name
        results.sort_by(|a, b| a.name.cmp(&b.name));
        results
    }

    /// Get recipe suggestions based on available ingredients
    pub async fn get_recipe_suggestions(
        &self,
        available_ingredients: &HashMap<u32, u32>,
        known_recipes: &[RecipeId],
        limit: usize,
    ) -> Vec<(Arc<Recipe>, f32)> { // (recipe, match_score)
        let known_set: HashSet<_> = known_recipes.iter().copied().collect();
        let mut suggestions = Vec::new();

        for recipe_entry in self.recipes.iter() {
            let recipe = recipe_entry.value();
            
            // Only suggest known recipes
            if !known_set.contains(&recipe.id) {
                continue;
            }

            // Calculate match score
            let match_score = self.calculate_ingredient_match_score(recipe, available_ingredients);
            
            if match_score > 0.0 {
                suggestions.push((recipe.clone(), match_score));
            }
        }

        // Sort by match score (descending)
        suggestions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        suggestions.truncate(limit);
        suggestions
    }

    /// Calculate how well available ingredients match a recipe
    fn calculate_ingredient_match_score(
        &self,
        recipe: &Recipe,
        available_ingredients: &HashMap<u32, u32>,
    ) -> f32 {
        let mut total_required = 0u32;
        let mut total_available = 0u32;

        for ingredient in &recipe.ingredients {
            total_required += ingredient.quantity;
            let available = available_ingredients.get(&ingredient.item_id).unwrap_or(&0);
            total_available += (*available).min(ingredient.quantity);
        }

        if total_required == 0 {
            return 0.0;
        }

        total_available as f32 / total_required as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn create_test_recipe() -> Recipe {
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
                skill_requirements: HashMap::new(),
                station_requirement: None,
                tool_requirements: vec![],
                environmental_requirements: vec![],
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

    #[tokio::test]
    async fn test_add_and_get_recipe() {
        let manager = RecipeManager::new();
        let recipe = create_test_recipe();
        let recipe_id = recipe.id;

        manager.add_recipe(recipe).await.unwrap();
        
        let retrieved = manager.get_recipe(&recipe_id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test Recipe");
    }

    #[tokio::test]
    async fn test_recipe_validation() {
        let manager = RecipeManager::new();
        let mut recipe = create_test_recipe();
        
        // Valid recipe should pass
        assert!(manager.validate_recipe(&recipe).await.is_ok());
        
        // Invalid failure chance should fail
        recipe.failure_chance = 1.5;
        assert!(manager.validate_recipe(&recipe).await.is_err());
    }

    #[tokio::test]
    async fn test_ingredient_matching() {
        let manager = RecipeManager::new();
        let recipe = create_test_recipe();
        
        let mut available = HashMap::new();
        available.insert(1, 2); // Exact amount
        
        assert!(manager.can_craft_with_ingredients(&recipe, &available));
        
        available.insert(1, 1); // Insufficient
        assert!(!manager.can_craft_with_ingredients(&recipe, &available));
        
        available.insert(1, 5); // More than enough
        assert!(manager.can_craft_with_ingredients(&recipe, &available));
    }

    #[tokio::test]
    async fn test_recipe_search() {
        let manager = RecipeManager::new();
        let recipe = create_test_recipe();
        let recipe_id = recipe.id;
        
        manager.add_recipe(recipe).await.unwrap();
        
        let criteria = RecipeSearchCriteria {
            name_pattern: Some("Test".to_string()),
            category: None,
            max_skill_level: None,
            available_ingredients: None,
            max_craft_time: None,
            min_success_rate: None,
        };
        
        let results = manager.search_recipes(&criteria, &[recipe_id]).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Test Recipe");
    }
}