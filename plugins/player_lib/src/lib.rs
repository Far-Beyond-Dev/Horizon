use horizon_data_types::Player;
use socketioxide::extract::SocketRef;
pub use horizon_plugin_api::{Plugin, Pluginstate, LoadedPlugin};
use parking_lot::RwLock;
use std::sync::Arc;
use std::collections::HashMap;

pub mod world;
// use socketioxide::packet::Str;
// use PebbleVault;

pub trait PluginAPI {    
    fn player_joined(&self, socket: SocketRef, player: Arc<RwLock<horizon_data_types::Player>>);   
}

pub trait PluginConstruct {
    fn get_structs(&self) -> Vec<&str>;
    // If you want default implementations, mark them with 'default'
    fn new(plugins: HashMap<String, (Pluginstate, Plugin)>) -> Plugin;
    
}

impl PluginConstruct for Plugin {
    fn new(_plugins: HashMap<String, (Pluginstate, Plugin)>) -> Plugin {        
        Plugin {}
    }

    fn get_structs(&self) -> Vec<&str> {
        vec!["MyPlayer"]
    }
}

impl PluginAPI for Plugin {
    fn player_joined(&self, socket: SocketRef, player: Arc<RwLock<horizon_data_types::Player>>) {
        println!("player_lib");
        setup_listeners(socket, player);
    }
}

//-----------------------------------------------------------------------------
// Plugin Implementation
//-----------------------------------------------------------------------------

pub type HorizonPlayer = (Character,Player);

/// The player character struct
///  - name: The player character's human-readable name
///  - health: The player character's health
///  - position: The player character's world position
///  - rotation: The player character's world rotation
///  - scale: The player character's world scale
///  - accelleration: The player character's world accelleration
///  - animations: The player character's animations
///  - model: The player character's model
///  - texture: The player character's texture
///  - keybones: The player character's keybones

pub struct Character {
    pub name:          String,           // The player character's human-readable name
    pub health:        i64,              // The player character's health
    pub position:      (f64, f64, f64),  // The player character's world position
    pub rotation:      (f64, f64, f64),  // The player character's world rotation
    pub scale:         (f64, f64, f64),  // The player character's world scale
    pub accelleration: (f64, f64, f64),  // The player character's world accelleration
    pub animations:    Vec<String>,      // The player character's animations
    pub model:         String,           // The player character's model
    pub texture:       String,           // The player character's texture
    pub keybones:      Vec<[f64; 9]>     // The player character's keybones
}

impl Character {
    /// Create a new player character
    pub fn new(
        name:          String,
        health:        i64,
        position:      (f64, f64, f64),
        rotation:      (f64, f64, f64),
        scale:         (f64, f64, f64),
        accelleration: (f64, f64, f64),
        animations:    Vec<String>,
        model:         String,
        texture:       String,
        keybones:      Vec<[f64; 9]>,
    ) -> Self {
        Self {
            name,
            health,
            position,
            rotation,
            scale,
            accelleration,
            animations,
            model,
            texture,
            keybones,
        }
    }

    /// Update the player character's name
    pub fn update_position(&mut self, new_position: (f64, f64, f64)) {
        self.position = new_position;
    }

    /// Update the player character's health
    pub fn update_health(&mut self, new_health: i64) {
        self.health = new_health;
    }

    /// Add an animation to the player character
    pub fn add_animation(&mut self, animation: String) {
        self.animations.push(animation);
    }

    /// Remove an animation from the player character
    pub fn update_rotation(&mut self, new_rotation: (f64, f64, f64)) {
        self.rotation = new_rotation;
    }

    /// Update the player character's scale
    pub fn update_scale(&mut self, new_scale: (f64, f64, f64)) {
        self.scale = new_scale;
    }

    /// Update the player character's accelleration
    pub fn update_acceleration(&mut self, new_acceleration: (f64, f64, f64)) {
        self.accelleration = new_acceleration;
    }

    /// Update the player character's model
    pub fn update_model(&mut self, new_model: String) {
        self.model = new_model;
    }

    /// Update the player character's texture
    pub fn update_texture(&mut self, new_texture: String) {
        self.texture = new_texture;
    }

    /// Update the player character's keybones
    pub fn update_keybones(&mut self, new_keybones: Vec<[f64; 9]>) {
        self.keybones = new_keybones;
    }
}

fn setup_listeners(_socket: SocketRef, _player: Arc<RwLock<Player>>) {
}