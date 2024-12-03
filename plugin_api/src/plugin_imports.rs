// This file is automatically generated by build.rs
// Do not edit this file manually!

use horizon_plugin_api::LoadedPlugin;
use std::collections::HashMap;

// Invoke the macro with all discovered plugins
pub fn load_plugins() -> HashMap<&'static str, LoadedPlugin> {
    let plugins = crate::load_plugins!(
        chronos_plugin,
        player_lib,
        stars_beyond_plugin,
        test_plugin
    );
    plugins
}
