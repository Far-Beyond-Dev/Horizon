// crates/recipe_smith/src/events.rs

use event_system::{EventSystemImpl, EventId, EventError, types::*};
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use crate::{RecipeSmith, CraftEvent, StartCraftRequest, CancelCraftRequest, ClientCraftingRequest, CraftId, RecipeId, ItemStack, PlayerId, PlayerLeftEvent, StartCraftResponse, CraftEventNotification};

#[derive(Serialize, Deserialize, Debug)]
pub struct PluginCraftEvent {
    pub player_id: PlayerId,
    pub event: CraftEvent,
}

impl RecipeSmith {
    async fn emit_craft_event(&self, player_id: PlayerId, event: CraftEvent) -> Result<(), EventError> {
        if let Some(events) = self.event_system.read().await.as_ref() {
            events.emit_plugin("recipe_smith", "craft_event", &PluginCraftEvent {
                player_id,
                event,
            }).await
        } else {
            Err(EventError::HandlerExecution("Event system not initialized".into()))
        }
    }
    
    // async fn handle_player_left(&mut self, event: PlayerLeftEvent) -> Result<(), EventError> {
    //     if let Some(crafts) = self.active_crafts.write().await.remove(&event.player_id) {
    //         for craft in crafts.crafts {
    //             self.emit_craft_event(event.player_id, CraftEvent::Cancelled {
    //                 recipe_id: craft.0,
    //                 craft_id: craft.1.craft_id,
    //                 reason: "Player disconnected".to_string(),
    //             }).await?;
    //         }
    //     }
    //     Ok(())
    // }
    
    // async fn handle_start_craft(&mut self, request: StartCraftRequest) -> Result<(), EventError> {
    //     match self.start_craft(request.player_id, request.recipe_id, request.ingredients).await {
    //         Ok(started) => {
    //             self.emit_craft_event(request.player_id, CraftEvent::Started {
    //                 craft_id: started.craft_id,
    //                 recipe_id: started.recipe_id,
    //                 duration: started.duration,
    //             }).await?;
    //         }
    //         Err(e) => {
    //             self.emit_craft_event(request.player_id, CraftEvent::Failed {
    //                 craft_id: CraftId::nil(), // Temporary ID for failed starts
    //                 recipe_id: request.recipe_id,
    //                 reason: e.to_string(),
    //             }).await?;
    //         }
    //     }
    //     Ok(())
    // }
    
    // pub async fn handle_cancel_craft(&mut self, request: CancelCraftRequest) -> Result<(), EventError> {
    //     if let Some(crafts) = self.active_crafts.write().await.get_mut(&request.player_id) {
    //         if let Some(craft) = crafts.crafts.remove(&request.craft_id) {
    //             self.emit_craft_event(request.player_id, CraftEvent::Cancelled {
    //                 craft_id: craft.craft_id,
    //                 recipe_id: craft.recipe_id,
    //                 reason: "Player cancelled".to_string(),
    //             }).await?;
    //             return Ok(());
    //         }
    //     }
        
    //     self.emit_craft_event(request.player_id, CraftEvent::Failed {
    //         craft_id: request.craft_id,
    //         recipe_id: RecipeId::nil(), // Unknown recipe
    //         reason: "Craft not found".to_string(),
    //     }).await
    // }
    
    async fn handle_client_request(&mut self, request: ClientCraftingRequest) -> Result<(), EventError> {
        // Convert client inventory slots to ingredient format
        let ingredients = request.ingredient_sources.into_iter()
            .map(|slot| ItemStack {
                item_id: slot.item_id,
                quantity: slot.quantity,
                metadata: None,
            })
            .collect();
            
        self.handle_start_craft(StartCraftRequest {
            player_id: request.player_id,
            recipe_id: request.recipe_id,
            response_event: None, // No response event for client requests
            ingredients,
        }).await.map_err(|e| EventError::HandlerExecution(e.to_string().into()))
    }
    
    pub async fn complete_craft(&mut self, craft_id: CraftId) -> Result<(), EventError> {
        // Find the craft by ID
        let craft = {
            let mut found = None;
            for crafts in self.active_crafts.write().await.values_mut() {
                if let Some(index) = crafts.crafts.iter().position(|c| c.1.craft_id == craft_id) {
                    found = Some(crafts.crafts.remove(&crafts.crafts.keys().nth(index).unwrap().clone()).unwrap());
                    break;
                }
            }
            found.ok_or(EventError::HandlerExecution("Craft not found".into()))?
        };
        
        // Get the recipe products
        let products = {
            let recipes = self.recipes.read().await;
            let recipe = recipes.get(&craft.recipe_id)
                .ok_or(EventError::HandlerExecution("Recipe not found".into()))?;
            recipe.products.clone()
        };
            
        // Emit completion event
        self.emit_craft_event(craft.player_id, CraftEvent::Completed {
            craft_id: craft.craft_id,
            recipe_id: craft.recipe_id,
            products,
        }).await
    }
}