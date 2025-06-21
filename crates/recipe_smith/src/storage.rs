//! Production-grade recipe storage implementation

use super::*;
use async_trait::async_trait;
use serde_json;
use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};
use tokio::{
    fs as tokio_fs,
    io::{AsyncReadExt, AsyncWriteExt},
};
use tracing::{debug, error, info, instrument};

/// JSON-based recipe storage
#[derive(Debug)]
pub struct JsonRecipeStorage {
    recipe_dir: PathBuf,
}

impl JsonRecipeStorage {
    /// Create a new JSON recipe storage
    pub fn new(recipe_dir: PathBuf) -> Self {
        // Create directory if it doesn't exist
        if !recipe_dir.exists() {
            if let Err(e) = fs::create_dir_all(&recipe_dir) {
                error!("Failed to create recipe directory: {}", e);
            }
        }

        Self { recipe_dir }
    }

    /// Get path for a recipe file
    fn recipe_path(&self, recipe_id: &RecipeId) -> PathBuf {
        self.recipe_dir.join(format!("{}.recipe.json", recipe_id))
    }

    /// Load a single recipe file
    #[instrument(skip(self))]
    async fn load_recipe_file(&self, path: &Path) -> Result<Recipe, StorageError> {
        let mut file = tokio_fs::File::open(path)
            .await
            .map_err(|e| StorageError::FileRead(path.to_path_buf(), e))?;

        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .await
            .map_err(|e| StorageError::FileRead(path.to_path_buf(), e))?;

        serde_json::from_str(&contents)
            .map_err(|e| StorageError::Deserialization(path.to_path_buf(), e))
    }
}

#[async_trait]
impl RecipeStorage for JsonRecipeStorage {
    #[instrument(skip(self))]
    async fn load_all(&self) -> Result<Vec<Recipe>, StorageError> {
        let mut recipes = Vec::new();
        let mut entries = tokio_fs::read_dir(&self.recipe_dir)
            .await
            .map_err(|e| StorageError::DirectoryRead(self.recipe_dir.clone(), e))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| StorageError::DirectoryRead(self.recipe_dir.clone(), e))?
        {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("recipe.json") {
                match self.load_recipe_file(&path).await {
                    Ok(recipe) => {
                        debug!("Loaded recipe: {}", recipe.id);
                        recipes.push(recipe);
                    }
                    Err(e) => {
                        error!("Failed to load recipe from {}: {}", path.display(), e);
                        continue;
                    }
                }
            }
        }

        Ok(recipes)
    }

    #[instrument(skip(self, recipe))]
    async fn save(&self, recipe: &Recipe) -> Result<(), StorageError> {
        let path = self.recipe_path(&recipe.id);
        let temp_path = path.with_extension("tmp");

        let json = serde_json::to_string_pretty(recipe)
            .map_err(|e| StorageError::Serialization(recipe.id, e))?;

        let mut file = tokio_fs::File::create(&temp_path)
            .await
            .map_err(|e| StorageError::FileCreate(temp_path.clone(), e))?;

        file.write_all(json.as_bytes())
            .await
            .map_err(|e| StorageError::FileWrite(temp_path.clone(), e))?;

        file.sync_all()
            .await
            .map_err(|e| StorageError::FileSync(temp_path.clone(), e))?;

        // Atomic rename
        tokio_fs::rename(&temp_path, &path)
            .await
            .map_err(|e| StorageError::FileRename(temp_path, path.clone(), e))?;

        info!("Saved recipe: {}", recipe.id);
        Ok(())
    }

    #[instrument(skip(self))]
    async fn delete(&self, recipe_id: &RecipeId) -> Result<(), StorageError> {
        let path = self.recipe_path(recipe_id);
        if path.exists() {
            tokio_fs::remove_file(&path)
                .await
                .map_err(|e| StorageError::FileDelete(path, e))?;
            info!("Deleted recipe: {}", recipe_id);
        }
        Ok(())
    }

    #[instrument(skip(self))]
    async fn last_modified(&self, recipe_id: &RecipeId) -> Result<SystemTime, StorageError> {
        let path = self.recipe_path(recipe_id);
        let metadata = tokio_fs::metadata(&path)
            .await
            .map_err(|e| StorageError::FileMetadata(path.clone(), Box::new(e)))?;
        metadata
            .modified()
            .map_err(|e| StorageError::FileMetadata(path, Box::new(e)))
    }
}

/// Recipe storage trait
#[async_trait]
pub trait RecipeStorage: Send + Sync + std::fmt::Debug {
    /// Load all recipes
    async fn load_all(&self) -> Result<Vec<Recipe>, StorageError>;

    /// Save a recipe
    async fn save(&self, recipe: &Recipe) -> Result<(), StorageError>;

    /// Delete a recipe
    async fn delete(&self, recipe_id: &RecipeId) -> Result<(), StorageError>;

    /// Get last modified time for a recipe
    async fn last_modified(&self, recipe_id: &RecipeId) -> Result<SystemTime, StorageError>;
}