// This file is automatically generated by build.rs
// Do not edit this file manually!

use std::collections::HashMap;

pub use chronos_plugin;
pub use chronos_plugin::*;
pub use chronos_plugin::Plugin as chronos_plugin_plugin;
pub use player_lib;
pub use player_lib::*;
pub use player_lib::Plugin as player_lib_plugin;


// Invoke the macro with all discovered plugins
pub fn load_plugins() -> HashMap<String, (Pluginstate, Plugin)> {
    let plugins = crate::load_plugins!(
        chronos_plugin,
        player_lib
    );
    plugins
}
