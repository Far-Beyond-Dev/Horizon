use std::collections::HashMap;
pub use horizon_plugin_api::{Plugin, Pluginstate, Version, get_plugin};

pub mod plugin_macro;
pub mod plugin_imports;

// Define the current plugin version
const PLUGIN_API_VERSION: Version = Version {
    major: 0,
    minor: 1,
    hotfix: 0
};

pub struct PluginManager {
    plugins: HashMap<String,(Pluginstate,Plugin)>
}

#[macro_export]
macro_rules! load_plugins {
    ($($plugin:ident),* $(,)?) => {
        {
            let mut plugins = HashMap::new();
            $(
                plugins.insert(
                    stringify!($plugin),
                    LoadedPlugin {
                        instance: <$plugin::Plugin as $plugin::PluginConstruct>::new(plugins.clone()),
                    }
                );
            )*
            plugins
        }
    };
}

impl PluginManager {
    /// Allow instantiation of the ``PluginManager`` struct
    pub fn new() -> PluginManager {
        let new_manager = PluginManager {
            plugins: HashMap::new(), 
        };

        new_manager 
    }

    pub fn load_plugin(mut self,name: String, plugin: Plugin) {
        self.plugins.insert(name, (Pluginstate::ACTIVE, plugin));
    }

    pub fn unload_plugin(mut self,name: String) {
        self.plugins.remove(&name);
    }

    pub fn get_plugins(self) -> HashMap<String,(Pluginstate,Plugin)> {
        self.plugins
    }

    pub fn load_all(&self) {
        let plugins = plugin_imports::load_plugins();
    
        let my_test_plugin = get_plugin!(test_plugin, plugins);
        let result = my_test_plugin.thing();
    }
}
