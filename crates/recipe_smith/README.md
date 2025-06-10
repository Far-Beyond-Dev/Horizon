# 🧪 RecipeSmith Plugin

**Horizon Game Server Extension**
Version: *auto-filled from `CARGO_PKG_VERSION`*

---

## 📘 Description

The **RecipeSmith** plugin provides a structured system for managing crafting recipes and tracking player crafting outcomes in a Horizon-based game. It is designed to work entirely via Horizon's **callback-based event system**, ensuring high performance and low overhead without requiring polling.

Ideal for crafting-heavy games (RPGs, survival, sandbox), it enables features such as:

* Centralized recipe management
* Player-specific crafting history tracking
* Event-driven responses for recipe queries and updates
* Seamless integration with other plugins via shared events

---

## 🔧 Features

| Feature                    | Description                                                                |
| -------------------------- | -------------------------------------------------------------------------- |
| 🔁 Add/Update Recipes      | Dynamically register or change recipes during runtime                      |
| 📦 Track Crafting Outcomes | Log main product and byproducts per player with timestamped entries        |
| 🔍 Query Support           | Retrieve all recipes or specific recipe details on demand                  |
| 🧠 Callback-Based Events   | Fully event-driven, integrates directly with Horizon’s plugin lifecycle    |
| 🛠 Safe FFI Boundary       | Includes `create_plugin` and `destroy_plugin` exports for Rust integration |

---

## 📦 Data Structures

### Recipe

```rust
struct Recipe {
    id: RecipeId,
    name: String,
    ingredients: Vec<RecipeIngredient>,
    main_product: RecipeProduct,
    byproducts: Vec<RecipeProduct>,
}
```

### CraftingOutcome

```rust
struct CraftingOutcome {
    recipe_id: RecipeId,
    main_product: RecipeProduct,
    byproducts: Vec<RecipeProduct>,
    timestamp: u64,
}
```

### Event Types

The plugin emits and handles the following events:

* `plugin_initialized`
* `plugin_shutdown`
* `recipe_updated`
* `crafting_completed`
* `all_recipes_info`
* `recipe_info`
* `player_crafting_history`

---

## 🧬 Lifecycle & Events

### Initialization

On startup (`initialize`):

* Preloads sample recipes
* Emits a `PluginInitialized` event
* Logs plugin status to the Horizon server log

### Shutdown

When `shutdown()` is called:

* Clears all recipe and player data
* Emits a `PluginShutdown` event
* Resets internal state for safe future reuse

### Event Subscriptions

RecipeSmith subscribes to the following events:

| Namespace       | Event Name                    |
| --------------- | ----------------------------- |
| `core`          | `player_joined`               |
| `core`          | `player_left`                 |
| `core`          | `custom_message`              |
| `plugin.<name>` | All custom RecipeSmith events |

---

## 💬 Messages

External systems (clients, other plugins) can interact with RecipeSmith via the `custom_message` system. The plugin supports:

```rust
enum RecipeSmithMessage {
    AddOrUpdateRecipe { recipe: Recipe },
    RecordCraftingOutcome { player_id: PlayerId, outcome: CraftingOutcome },
    GetAllRecipes { player_id: PlayerId },
    GetRecipeInfo { player_id: PlayerId, recipe_id: RecipeId },
    GetPlayerCraftingHistory { player_id: PlayerId, target_player_id: PlayerId },
}
```

These messages are sent as serialized JSON to the `core:custom_message` event.

---

## 🧩 Integration

To use the plugin:

1. Add it to your Horizon server's plugin directory.
2. Ensure it is registered in your server configuration.
3. On runtime, the plugin will self-initialize and begin handling events.

### FFI Exports

These are required for the Horizon plugin system to load/unload the plugin safely.

```rust
#[no_mangle]
pub unsafe extern "C" fn create_plugin() -> *mut dyn Plugin
```

```rust
#[no_mangle]
pub unsafe extern "C" fn destroy_plugin(plugin: *mut dyn Plugin)
```

---

## 🔍 Logging

RecipeSmith uses Horizon's built-in logging interface:

* `Debug`: Internal state changes and event handling
* `Info`: Major lifecycle updates and user-facing actions
* `Error`: Emitted when critical operations fail (e.g., event dispatch errors)

---

## 📁 Example Output (Logs)

```
[INFO] Initializing RecipeSmith plugin v1.0.0
[INFO] Loaded 2 initial recipes.
[DEBUG] Player 123 requested info for recipe ID: 1
[INFO] Crafting outcome recorded for player 456.
```

---

## 🛠 Status & Extensibility

RecipeSmith is production-ready and can be extended with:

* Recipe categories or tags
* Crafting success/failure probabilities
* Usage statistics or analytics
* UI display integration via additional event types

---

## 📚 License

Licensed under MIT