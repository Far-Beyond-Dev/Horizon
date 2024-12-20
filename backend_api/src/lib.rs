use std::collections::HashMap;
pub use horizon_plugin_api::{Plugin, Pluginstate, Version, get_plugin, LoadedPlugin};

pub mod plugin_macro;
pub mod backend_imports;

// Define the current plugin version
const PLUGIN_API_VERSION: Version = Version {
    major: 0,
    minor: 1,
    hotfix: 0
};

#[derive(Clone)]
pub struct PluginManager {
    plugins: HashMap<String,(Pluginstate,Plugin)>
}

#[macro_export]
macro_rules! load_backends {
    ($($plugin:ident),* $(,)?) => {
        {
            let mut plugins = HashMap::new();
            $(
                plugins.insert(
                    stringify!($plugin).to_string(),
                    (Pluginstate::ACTIVE, <$plugin::Plugin as $plugin::PluginConstruct>::new(plugins.clone())),
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

    pub fn load_all(&mut self) ->  HashMap<String, LoadedPlugin> {
        self.plugins = backend_imports::load_backends();
    
        //let my_test_plugin = get_plugin!(test_plugin, plugins);
        //let result = my_test_plugin.thing();

        let mut loaded_plugins = HashMap::new();
        for (name, (state, plugin)) in &self.plugins {
            if *state == Pluginstate::ACTIVE {
            loaded_plugins.insert(name.clone(), LoadedPlugin {
                instance: plugin.clone(),
            });
            }
        }
        loaded_plugins
    }
}
