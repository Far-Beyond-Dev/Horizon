//! Production-grade recipe validation

use super::*;
use async_trait::async_trait;
use std::collections::HashSet;
use tracing::{debug, error, info, instrument, warn};

/// Recipe validator
#[derive(Debug, Clone)]
pub struct RecipeValidator {
    strict_mode: bool,
}

impl RecipeValidator {
    /// Create a new validator
    pub fn new() -> Self {
        Self { strict_mode: true }
    }

    /// Set strict mode
    pub fn with_strict_mode(mut self, strict: bool) -> Self {
        self.strict_mode = strict;
        self
    }

    /// Validate all recipes
    #[instrument(skip(self, recipes))]
    pub async fn validate_all(&self, recipes: Vec<Recipe>) -> Result<Vec<Recipe>, ValidationError> {
        let mut valid_recipes = Vec::with_capacity(recipes.len());
        let mut errors = Vec::new();

        for recipe in recipes {
            match self.validate_recipe(&recipe).await {
                Ok(_) => {
                    debug!("Recipe validated: {}", recipe.id);
                    valid_recipes.push(recipe);
                }
                Err(e) => {
                    error!("Invalid recipe {}: {}", recipe.id, e);
                    if self.strict_mode {
                        errors.push(e);
                    }
                }
            }
        }

        if !errors.is_empty() {
            return Err(ValidationError::BatchValidationFailed(errors));
        }

        Ok(valid_recipes)
    }

    /// Validate a single recipe
    #[instrument(skip(self, recipe))]
    pub async fn validate_recipe(&self, recipe: &Recipe) -> Result<(), ValidationError> {
        // Validate ID
        if recipe.id.is_nil() {
            return Err(ValidationError::InvalidRecipeId);
        }

        // Validate name
        if recipe.name.trim().is_empty() {
            return Err(ValidationError::EmptyName);
        }

        // Validate ingredients
        if recipe.ingredients.is_empty() {
            return Err(ValidationError::NoIngredients);
        }

        // Validate products
        if recipe.products.is_empty() {
            return Err(ValidationError::NoProducts);
        }

        // Check for duplicate items in ingredients
        let mut ingredient_ids = HashSet::new();
        for item in &recipe.ingredients {
            if item.item_id.is_nil() {
                return Err(ValidationError::InvalidIngredientId);
            }
            if item.quantity == 0 {
                return Err(ValidationError::ZeroQuantity);
            }
            if !ingredient_ids.insert(item.item_id) {
                return Err(ValidationError::DuplicateIngredient(item.item_id));
            }
        }

        // Check for duplicate items in products
        let mut product_ids = HashSet::new();
        for item in &recipe.products {
            if item.item_id.is_nil() {
                return Err(ValidationError::InvalidProductId);
            }
            if item.quantity == 0 {
                return Err(ValidationError::ZeroQuantity);
            }
            if !product_ids.insert(item.item_id) {
                return Err(ValidationError::DuplicateProduct(item.item_id));
            }
        }

        // Validate craft time if present
        if let Some(time) = recipe.craft_time {
            if time == 0 {
                return Err(ValidationError::InvalidCraftTime);
            }
        }

        Ok(())
    }

    /// Validate ingredients against a recipe
    #[instrument(skip(self, recipe, ingredients))]
    pub async fn validate_ingredients(
        &self,
        recipe: &Recipe,
        ingredients: &[ItemStack],
    ) -> Result<(), ValidationError> {
        // Convert recipe ingredients to a map for easy lookup
        let required_ingredients: HashMap<ItemId, u32> = recipe
            .ingredients
            .iter()
            .map(|i| (i.item_id, i.quantity))
            .collect();

        // Track provided ingredients
        let mut provided_ingredients: HashMap<ItemId, u32> = HashMap::new();
        for item in ingredients {
            *provided_ingredients.entry(item.item_id).or_default() += item.quantity;
        }

        // Validate all required ingredients are present in sufficient quantities
        for (item_id, required_qty) in required_ingredients {
            match provided_ingredients.get(&item_id) {
                Some(provided_qty) if provided_qty >= &required_qty => continue,
                Some(provided_qty) => {
                    return Err(ValidationError::InsufficientQuantity {
                        item_id,
                        required: required_qty,
                        provided: *provided_qty,
                    });
                }
                None => return Err(ValidationError::MissingIngredient(item_id)),
            }
        }

        Ok(())
    }
}

impl Default for RecipeValidator {
    fn default() -> Self {
        Self::new()
    }
}