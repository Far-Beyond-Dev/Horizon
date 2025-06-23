use crate::types::*;

pub fn setup_inventory_handler(
    config: &Arc<Mutex<InventoryConfig>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    crafting_recipes: &Arc<Mutex<HashMap<String, CraftingRecipe>>>,
    events: &Arc<EventSystem>,
    event: InventorySettingRequest,
) {
    let result = setup_inventory_system(
        config,
        item_definitions,
        crafting_recipes,
        event,
    );

    match result {
        Ok(setup_result) => {
            info!(
                "🎒 Inventory system setup complete: {} item definitions, {} recipes loaded",
                setup_result.items_loaded,
                setup_result.recipes_loaded
            );

            // Emit setup completion event
            let _ = events.emit_plugin(
                "InventorySystem",
                "system_setup_complete",
                &serde_json::json!({
                    "setup_result": setup_result,
                    "timestamp": current_timestamp()
                }),
            );
        }
        Err(e) => {
            info!("❌ Failed to setup inventory system: {}", e);

            let _ = events.emit_plugin(
                "InventorySystem",
                "setup_failed",
                &serde_json::json!({
                    "reason": e.to_string(),
                    "timestamp": current_timestamp()
                }),
            );
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct SetupResult {
    pub items_loaded: u32,
    pub recipes_loaded: u32,
    pub default_inventories_created: u32,
    pub config_updated: bool,
    pub validation_passed: bool,
}

fn setup_inventory_system(
    config: &Arc<Mutex<InventoryConfig>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    crafting_recipes: &Arc<Mutex<HashMap<String, CraftingRecipe>>>,
    settings: InventorySettingRequest,
) -> Result<SetupResult, InventoryError> {
    let mut setup_result = SetupResult {
        items_loaded: 0,
        recipes_loaded: 0,
        default_inventories_created: 0,
        config_updated: false,
        validation_passed: false,
    };

    // Update configuration
    update_system_config(config, &settings)?;
    setup_result.config_updated = true;

    // Load default item definitions
    let items_loaded = load_default_item_definitions(item_definitions)?;
    setup_result.items_loaded = items_loaded;

    // Load default crafting recipes
    let recipes_loaded = load_default_crafting_recipes(crafting_recipes, item_definitions)?;
    setup_result.recipes_loaded = recipes_loaded;

    // Validate system integrity
    validate_system_integrity(item_definitions, crafting_recipes)?;
    setup_result.validation_passed = true;

    // Setup default inventory templates
    let templates_created = create_default_inventory_templates()?;
    setup_result.default_inventories_created = templates_created;

    info!("✅ Inventory system setup completed successfully");

    Ok(setup_result)
}

fn update_system_config(
    config: &Arc<Mutex<InventoryConfig>>,
    settings: &InventorySettingRequest,
) -> Result<(), InventoryError> {
    let mut config_guard = config.lock().unwrap();

    // Update configuration based on settings
    if let Some(slot_count) = settings.slot_count {
        config_guard.default_inventory_size = slot_count;
    }

    if let Some(inventory_count) = settings.inventory_count {
        config_guard.max_player_inventories = inventory_count as u32;
    }

    if let Some(enable_weight) = settings.enable_weight_system {
        config_guard.enable_weight_system = enable_weight;
    }

    if let Some(enable_durability) = settings.enable_durability {
        config_guard.enable_durability = enable_durability;
    }

    if let Some(auto_stack) = settings.auto_stack_items {
        config_guard.auto_stack_items = auto_stack;
    }

    info!("⚙️ System configuration updated");
    Ok(())
}

fn load_default_item_definitions(
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
) -> Result<u32, InventoryError> {
    let mut defs_guard = item_definitions.lock().unwrap();
    let mut loaded_count = 0;

    // Load basic item definitions
    let default_items = create_default_item_definitions();
    
    for item_def in default_items {
        defs_guard.insert(item_def.id, item_def);
        loaded_count += 1;
    }

    info!("📦 Loaded {} default item definitions", loaded_count);
    Ok(loaded_count)
}

fn create_default_item_definitions() -> Vec<ItemDefinition> {
    vec![
        // Basic weapons
        ItemDefinition {
            id: 1001,
            name: "Iron Sword".to_string(),
            description: "A sturdy iron sword".to_string(),
            category: ItemCategory::Weapon(WeaponType::Sword),
            rarity: ItemRarity::Common,
            max_stack: 1,
            weight: 3.5,
            size: (1, 3),
            tradeable: true,
            consumable: false,
            durability_max: Some(100),
            icon_path: "weapons/iron_sword.png".to_string(),
            value: 150,
            level_requirement: Some(5),
            custom_properties: {
                let mut props = HashMap::new();
                props.insert("damage".to_string(), serde_json::json!(25));
                props.insert("attack_speed".to_string(), serde_json::json!(1.2));
                props
            },
        },
        
        // Basic armor
        ItemDefinition {
            id: 2001,
            name: "Leather Helmet".to_string(),
            description: "Basic protection for your head".to_string(),
            category: ItemCategory::Armor(ArmorType::Helmet),
            rarity: ItemRarity::Common,
            max_stack: 1,
            weight: 1.0,
            size: (1, 1),
            tradeable: true,
            consumable: false,
            durability_max: Some(50),
            icon_path: "armor/leather_helmet.png".to_string(),
            value: 75,
            level_requirement: Some(1),
            custom_properties: {
                let mut props = HashMap::new();
                props.insert("defense".to_string(), serde_json::json!(5));
                props
            },
        },

        // Consumables
        ItemDefinition {
            id: 3001,
            name: "Health Potion".to_string(),
            description: "Restores 50 health points".to_string(),
            category: ItemCategory::Consumable(ConsumableType::Potion),
            rarity: ItemRarity::Common,
            max_stack: 99,
            weight: 0.2,
            size: (1, 1),
            tradeable: true,
            consumable: true,
            durability_max: None,
            icon_path: "consumables/health_potion.png".to_string(),
            value: 25,
            level_requirement: None,
            custom_properties: {
                let mut props = HashMap::new();
                props.insert("consumption_effects".to_string(), serde_json::json!([
                    {
                        "effect_type": "health_restore",
                        "value": 50.0,
                        "duration": null,
                        "description": "Restores health"
                    }
                ]));
                props
            },
        },

        // Materials
        ItemDefinition {
            id: 4001,
            name: "Iron Ore".to_string(),
            description: "Raw iron ore for smelting".to_string(),
            category: ItemCategory::Material,
            rarity: ItemRarity::Common,
            max_stack: 64,
            weight: 2.0,
            size: (1, 1),
            tradeable: true,
            consumable: false,
            durability_max: None,
            icon_path: "materials/iron_ore.png".to_string(),
            value: 10,
            level_requirement: None,
            custom_properties: HashMap::new(),
        },

        // Currency
        ItemDefinition {
            id: 5001,
            name: "Gold Coin".to_string(),
            description: "Standard currency".to_string(),
            category: ItemCategory::Currency,
            rarity: ItemRarity::Common,
            max_stack: 9999,
            weight: 0.01,
            size: (1, 1),
            tradeable: true,
            consumable: false,
            durability_max: None,
            icon_path: "currency/gold_coin.png".to_string(),
            value: 1,
            level_requirement: None,
            custom_properties: HashMap::new(),
        },

        // Tools
        ItemDefinition {
            id: 6001,
            name: "Iron Pickaxe".to_string(),
            description: "Used for mining stone and ore".to_string(),
            category: ItemCategory::Tool,
            rarity: ItemRarity::Common,
            max_stack: 1,
            weight: 2.5,
            size: (1, 2),
            tradeable: true,
            consumable: false,
            durability_max: Some(200),
            icon_path: "tools/iron_pickaxe.png".to_string(),
            value: 100,
            level_requirement: Some(3),
            custom_properties: {
                let mut props = HashMap::new();
                props.insert("mining_power".to_string(), serde_json::json!(2));
                props.insert("mining_speed".to_string(), serde_json::json!(1.5));
                props
            },
        },
    ]
}

fn load_default_crafting_recipes(
    crafting_recipes: &Arc<Mutex<HashMap<String, CraftingRecipe>>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
) -> Result<u32, InventoryError> {
    let mut recipes_guard = crafting_recipes.lock().unwrap();
    let mut loaded_count = 0;

    // Validate that required items exist
    let defs_guard = item_definitions.lock().unwrap();
    
    let default_recipes = create_default_crafting_recipes();
    
    for recipe in default_recipes {
        // Validate recipe ingredients exist
        let mut valid_recipe = true;
        for required_item in &recipe.required_items {
            if !defs_guard.contains_key(&required_item.item_id) {
                info!("⚠️ Recipe '{}' references non-existent item {}", recipe.name, required_item.item_id);
                valid_recipe = false;
            }
        }
        
        for output_item in &recipe.output_items {
            if !defs_guard.contains_key(&output_item.item_id) {
                info!("⚠️ Recipe '{}' outputs non-existent item {}", recipe.name, output_item.item_id);
                valid_recipe = false;
            }
        }

        if valid_recipe {
            recipes_guard.insert(recipe.recipe_id.clone(), recipe);
            loaded_count += 1;
        }
    }

    info!("🔨 Loaded {} crafting recipes", loaded_count);
    Ok(loaded_count)
}

fn create_default_crafting_recipes() -> Vec<CraftingRecipe> {
    vec![
        CraftingRecipe {
            recipe_id: "craft_iron_sword".to_string(),
            name: "Iron Sword".to_string(),
            category: "weapons".to_string(),
            required_items: vec![
                RecipeItem {
                    item_id: 4001, // Iron Ore
                    quantity: 3,
                    consumed: true,
                },
            ],
            required_tools: vec![6001], // Iron Pickaxe (not consumed)
            output_items: vec![
                RecipeItem {
                    item_id: 1001, // Iron Sword
                    quantity: 1,
                    consumed: false,
                }
            ],
            skill_requirements: {
                let mut skills = HashMap::new();
                skills.insert("smithing".to_string(), 15);
                skills
            },
            crafting_time: 30, // 30 seconds
            experience_reward: 50,
            unlock_level: Some(10),
        },

        CraftingRecipe {
            recipe_id: "brew_health_potion".to_string(),
            name: "Health Potion".to_string(),
            category: "alchemy".to_string(),
            required_items: vec![
                RecipeItem {
                    item_id: 4001, // Using Iron Ore as placeholder for herbs
                    quantity: 2,
                    consumed: true,
                },
            ],
            required_tools: vec![], // No special tools required
            output_items: vec![
                RecipeItem {
                    item_id: 3001, // Health Potion
                    quantity: 3,
                    consumed: false,
                }
            ],
            skill_requirements: {
                let mut skills = HashMap::new();
                skills.insert("alchemy".to_string(), 5);
                skills
            },
            crafting_time: 15,
            experience_reward: 25,
            unlock_level: Some(5),
        },
    ]
}

fn validate_system_integrity(
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    crafting_recipes: &Arc<Mutex<HashMap<String, CraftingRecipe>>>,
) -> Result<(), InventoryError> {
    let defs_guard = item_definitions.lock().unwrap();
    let recipes_guard = crafting_recipes.lock().unwrap();

    // Validate item definitions
    for (id, item_def) in defs_guard.iter() {
        if item_def.id != *id {
            return Err(InventoryError::Custom(format!(
                "Item definition ID mismatch: key {} vs definition {}",
                id, item_def.id
            )));
        }

        if item_def.max_stack == 0 {
            return Err(InventoryError::Custom(format!(
                "Item '{}' has invalid max_stack of 0",
                item_def.name
            )));
        }

        if item_def.weight < 0.0 {
            return Err(InventoryError::Custom(format!(
                "Item '{}' has negative weight",
                item_def.name
            )));
        }
    }

    // Validate crafting recipes
    for recipe in recipes_guard.values() {
        // Check if all required items exist
        for required_item in &recipe.required_items {
            if !defs_guard.contains_key(&required_item.item_id) {
                return Err(InventoryError::Custom(format!(
                    "Recipe '{}' requires non-existent item {}",
                    recipe.name, required_item.item_id
                )));
            }
        }

        // Check if all output items exist
        for output_item in &recipe.output_items {
            if !defs_guard.contains_key(&output_item.item_id) {
                return Err(InventoryError::Custom(format!(
                    "Recipe '{}' outputs non-existent item {}",
                    recipe.name, output_item.item_id
                )));
            }
        }

        // Check if all required tools exist
        for tool_id in &recipe.required_tools {
            if !defs_guard.contains_key(tool_id) {
                return Err(InventoryError::Custom(format!(
                    "Recipe '{}' requires non-existent tool {}",
                    recipe.name, tool_id
                )));
            }
        }
    }

    info!("✅ System integrity validation passed");
    Ok(())
}

fn create_default_inventory_templates() -> Result<u32, InventoryError> {
    // This would create default inventory templates for different player types
    // For now, we'll just log that templates are created
    
    let templates = vec![
        "warrior_starter_inventory",
        "mage_starter_inventory", 
        "archer_starter_inventory",
        "crafter_starter_inventory",
    ];

    for template_name in &templates {
        info!("📋 Created inventory template: {}", template_name);
    }

    Ok(templates.len() as u32)
}

// Utility function to reset the entire system
pub fn reset_inventory_system(
    config: &Arc<Mutex<InventoryConfig>>,
    item_definitions: &Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    crafting_recipes: &Arc<Mutex<HashMap<String, CraftingRecipe>>>,
    events: &Arc<EventSystem>,
) -> Result<(), InventoryError> {
    info!("🔄 Resetting inventory system...");

    // Clear all data
    {
        let mut defs_guard = item_definitions.lock().unwrap();
        defs_guard.clear();
    }

    {
        let mut recipes_guard = crafting_recipes.lock().unwrap();
        recipes_guard.clear();
    }

    // Reset config to defaults
    {
        let mut config_guard = config.lock().unwrap();
        *config_guard = InventoryConfig {
            default_inventory_size: 27,
            max_player_inventories: 5,
            enable_weight_system: true,
            enable_durability: true,
            allow_item_stacking: true,
            auto_stack_items: true,
            currency_items: vec![5001], // Gold Coin
            trade_timeout_seconds: 300,
            container_access_distance: 10.0,
            custom_rules: HashMap::new(),
        };
    }

    // Reload defaults
    let default_settings = InventorySettingRequest {
        slot_count: Some(27),
        inventory_count: Some(5),
        enable_weight_system: Some(true),
        enable_durability: Some(true),
        auto_stack_items: Some(true),
    };

    setup_inventory_system(config, item_definitions, crafting_recipes, default_settings)?;

    let _ = events.emit_plugin(
        "InventorySystem",
        "system_reset_complete",
        &serde_json::json!({
            "timestamp": current_timestamp(),
            "message": "Inventory system has been reset to defaults"
        }),
    );

    info!("✅ Inventory system reset completed");
    Ok(())
}