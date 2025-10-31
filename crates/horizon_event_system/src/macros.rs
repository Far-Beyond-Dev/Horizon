//! # Plugin Development Macros
//!
//! This module contains macros that simplify plugin development and event
//! handler registration. These macros provide clean, declarative syntax for
//! common plugin operations while handling all the underlying complexity.
//!
//! ## Core Macros
//!
//! - [`create_simple_plugin!`] - Generates FFI-safe plugin wrapper with panic handling
//! - [`register_handlers!`] - Bulk registration of multiple event handlers
//! - [`on_event!`] - Simple registration of individual event handlers
//!
//! ## Safety Features
//!
//! All macros include comprehensive safety features:
//! - **Panic Isolation**: Plugin panics don't crash the host server
//! - **FFI Safety**: Proper null pointer handling at boundaries
//! - **Memory Safety**: Correct allocation/deallocation patterns
//! - **Error Conversion**: Structured error handling across boundaries

#[allow(unused_imports)] // These imports are used only in macro expansions
use {
    crate::plugin::{Plugin, PluginError, SimplePlugin},
    crate::events::EventError,
    std::sync::Arc,
    async_trait::async_trait,
    futures,
};


// ============================================================================
// Plugin Development Macros and Utilities
// ============================================================================

/// Macro to create a plugin with minimal boilerplate and comprehensive panic handling.
/// 
/// This macro generates all the necessary FFI wrapper code to bridge between
/// the `SimplePlugin` trait and the lower-level `Plugin` trait. It includes
/// comprehensive panic handling to ensure that plugin panics don't crash
/// the host server process.
/// 
/// # Safety Features
/// 
/// - **Panic Isolation**: All plugin methods are wrapped in `catch_unwind`
/// - **FFI Safety**: Proper null pointer handling at FFI boundaries
/// - **Memory Safety**: Correct Box allocation/deallocation for plugin instances
/// - **Error Conversion**: Panics are converted to `PluginError::Runtime`
/// 
/// # Usage
/// 
/// Simply call this macro with your plugin type after implementing `SimplePlugin`:
/// 
/// ```rust
/// use horizon_event_system::*;
/// 
/// struct MyPlugin {
///     // Plugin fields
/// }
/// 
/// impl MyPlugin {
///     fn new() -> Self {
///         Self { /* initialization */ }
///     }
/// }
/// 
/// #[async_trait]
/// impl SimplePlugin for MyPlugin {
///     fn name(&self) -> &str { "my_plugin" }
///     fn version(&self) -> &str { "1.0.0" }
///     
///     async fn register_handlers(&mut self, _events: Arc<EventSystem>, _context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
///         Ok(())
///     }
/// }
/// 
/// create_simple_plugin!(MyPlugin);
/// ```
/// 
/// This generates:
/// - `create_plugin()` - C-compatible plugin creation function
/// - `destroy_plugin()` - C-compatible plugin destruction function
/// - `PluginWrapper` - Internal wrapper with panic handling
/// - Proper FFI exports for dynamic loading
#[macro_export]
macro_rules! create_simple_plugin {
    ($plugin_type:ty) => {
        use $crate::Plugin;

        use std::panic::{catch_unwind, AssertUnwindSafe};

        /// Wrapper to bridge SimplePlugin and Plugin traits with panic protection.
        /// 
        /// This internal struct handles the conversion between the high-level
        /// `SimplePlugin` interface and the low-level `Plugin` trait required
        /// for FFI compatibility.
        struct PluginWrapper {
            inner: $plugin_type,
        }

        impl PluginWrapper {
            /// Helper to convert panics to PluginError.
            /// 
            /// This method extracts meaningful error messages from panic payloads
            /// and converts them to structured `PluginError` instances.
            fn panic_to_error(panic_info: Box<dyn std::any::Any + Send>) -> PluginError {
                let message = if let Some(s) = panic_info.downcast_ref::<&str>() {
                    format!("Plugin panicked: {}", s)
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    format!("Plugin panicked: {}", s)
                } else {
                    "Plugin panicked with unknown error".to_string()
                };
                
                PluginError::Runtime(message)
            }
        }

        #[async_trait]
        impl Plugin for PluginWrapper {
            fn name(&self) -> &str {
                // For synchronous methods, we can use catch_unwind directly
                match catch_unwind(AssertUnwindSafe(|| self.inner.name())) {
                    Ok(name) => name,
                    Err(_) => "unknown-plugin-name", // Fallback name if panic occurs
                }
            }

            fn version(&self) -> &str {
                match catch_unwind(AssertUnwindSafe(|| self.inner.version())) {
                    Ok(version) => version,
                    Err(_) => "unknown-version", // Fallback version if panic occurs
                }
            }

            async fn pre_init(
                &mut self,
                context: Arc<dyn ServerContext>,
            ) -> Result<(), PluginError> {
                // Run directly on the current thread using the current runtime handle
                catch_unwind(AssertUnwindSafe(|| {
                    futures::executor::block_on(self.inner.register_handlers(context.events(), context.clone()))
                }))
                .map_err(Self::panic_to_error)?
            }

            async fn init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
                catch_unwind(AssertUnwindSafe(|| {
                    futures::executor::block_on(self.inner.on_init(context))
                }))
                .map_err(Self::panic_to_error)?
            }

            async fn shutdown(
                &mut self,
                context: Arc<dyn ServerContext>,
            ) -> Result<(), PluginError> {
                catch_unwind(AssertUnwindSafe(|| {
                    futures::executor::block_on(self.inner.on_shutdown(context))
                }))
                .map_err(Self::panic_to_error)?
            }
        }

        /// Plugin version function - required export for ABI compatibility.
        /// 
        /// This function returns the ABI version that this plugin was compiled against.
        /// It is used by the plugin loader to validate ABI compatibility before
        /// attempting to create the plugin instance.
        /// 
        /// # Returns
        /// 
        /// Returns the plugin ABI version string derived from the horizon_event_system crate version.
        /// Format: "crate_version:rust_version" (e.g., "0.10.0:1.75.0")
        #[no_mangle]
        pub unsafe extern "C" fn get_plugin_version() -> *const std::os::raw::c_char {
            // Use the ABI version from the horizon_event_system crate
            // This ensures plugins compiled against different versions report different ABI versions
            let version_cstring = std::ffi::CString::new($crate::ABI_VERSION).unwrap_or_else(|_| {
                std::ffi::CString::new("invalid_version").unwrap()
            });
            
            // Leak the CString to ensure it remains valid for the caller
            // This is safe because plugin loading is a one-time operation per plugin
            version_cstring.into_raw()
        }

        /// Plugin creation function with panic protection - required export.
        /// 
        /// This function is called by the plugin loader to create a new instance
        /// of the plugin. It must be exported with C linkage for dynamic loading.
        /// 
        /// # Safety
        /// 
        /// This function is marked unsafe because it crosses FFI boundaries,
        /// but all operations are carefully protected against panics and
        /// memory safety violations.
        /// 
        /// # Returns
        /// 
        /// Returns a raw pointer to the plugin instance, or null if creation failed.
        #[no_mangle]
        pub unsafe extern "C" fn create_plugin() -> *mut dyn Plugin {
            // Critical: catch panics at FFI boundary to prevent UB
            match catch_unwind(AssertUnwindSafe(|| {
                let plugin = Box::new(PluginWrapper {
                    inner: <$plugin_type>::new(),
                });
                Box::into_raw(plugin) as *mut dyn Plugin
            })) {
                Ok(plugin_ptr) => plugin_ptr,
                Err(panic_info) => {
                    // Log the panic if possible (you might want to use your logging system here)
                    eprintln!("Plugin creation panicked: {:?}", panic_info);
                    std::ptr::null_mut::<PluginWrapper>() as *mut dyn Plugin // Return null on panic
                }
            }
        }

        /// Plugin destruction function with panic protection - required export.
        /// 
        /// This function is called by the plugin loader to clean up a plugin
        /// instance. It properly deallocates the Box and handles panics.
        /// 
        /// # Safety
        /// 
        /// This function is marked unsafe because it operates on raw pointers
        /// from FFI, but all operations are protected against panics.
        /// 
        /// # Arguments
        /// 
        /// * `plugin` - Raw pointer to the plugin instance to destroy
        #[no_mangle]
        pub unsafe extern "C" fn destroy_plugin(plugin: *mut dyn Plugin) {
            if plugin.is_null() {
                return;
            }

            // Critical: catch panics at FFI boundary to prevent UB
            let _ = catch_unwind(AssertUnwindSafe(|| {
                let _ = Box::from_raw(plugin);
            }));
            // If destruction panics, we just ignore it - the memory might leak
            // but it's better than crashing the host process
        }

        /// Optional: Initialize panic hook for better panic handling.
        /// 
        /// Call this once during plugin loading if you want custom panic handling.
        /// This sets up a global panic hook that will capture and log panic
        /// information in a structured way.
        #[allow(dead_code)]
        fn init_plugin_panic_hook() {
            std::panic::set_hook(Box::new(|panic_info| {
                eprintln!("Plugin panic occurred: {}", panic_info);
                // You could also send this to your logging system
            }));
        }
    };
}

/// Convenience macro for registering multiple handlers with clean syntax.
/// 
/// This macro provides a declarative way to register multiple event handlers
/// at once, organized by event category. It supports all three event types:
/// core, client, and plugin events.
/// 
/// # Syntax
/// 
/// ```rust
/// register_handlers!(events;
///     core {
///         "event_name" => |event: EventType| { /* handler */ Ok(()) },
///         // ... more core handlers
///     }
///     client {
///         "namespace", "event_name" => |event: EventType| { /* handler */ Ok(()) },
///         // ... more client handlers  
///     }
///     plugin {
///         "target_plugin", "event_name" => |event: EventType| { /* handler */ Ok(()) },
///         // ... more plugin handlers
///     }
/// );
/// ```
/// 
/// # Examples
/// 
/// ```rust
/// register_handlers!(events;
///     core {
///         "server_started" => |event: ServerStartedEvent| {
///             println!("Server is online!");
///             Ok(())
///         },
///         "player_connected" => |event: PlayerConnectedEvent| {
///             println!("Player {} joined", event.player_id);
///             Ok(())
///         }
///     }
///     client {
///         "movement", "jump" => |event: RawClientMessageEvent| {
///             handle_player_jump(event.player_id, &event.data)?;
///             Ok(())
///         },
///         "chat", "message" => |event: RawClientMessageEvent| {
///             process_chat_message(event.player_id, &event.data)?;
///             Ok(())
///         }
///     }
///     plugin {
///         "combat", "damage_dealt" => |event: DamageEvent| {
///             update_statistics(event.attacker, event.damage)?;
///             Ok(())
///         }
///     }
/// );
/// ```
#[macro_export]
macro_rules! register_handlers {
    // Handle client events section
    ($events:expr; client { $($namespace:expr, $event_name:expr => $handler:expr),* $(,)? }) => {{
        $(
            $events.on_client($namespace, $event_name, $handler).await.map_err(|e| PluginError::ExecutionError(e.to_string()))?;;
        )*
        Ok(())
    }};

    // Handle plugin events section
    ($events:expr; plugin { $($target_plugin:expr, $event_name:expr => $handler:expr),* $(,)? }) => {{
        $(
            $events.on_plugin($target_plugin, $event_name, $handler).await.map_err(|e| PluginError::ExecutionError(e.to_string()))?;
        )*
        Ok(())
    }};

    // Handle core events section
    ($events:expr; core { $($event_name:expr => $handler:expr),* $(,)? }) => {{
        $(
            $events.on_core($event_name, $handler).await.map_err(|e| PluginError::ExecutionError(e.to_string()))?;
        )*
        Ok(())
    }};

    // Handle mixed events with semicolon separators
    ($events:expr;
     $(client { $($c_namespace:expr, $c_event_name:expr => $c_handler:expr),* $(,)? })?
     $(plugin { $($p_target_plugin:expr, $p_event_name:expr => $p_handler:expr),* $(,)? })?
     $(core { $($core_event_name:expr => $core_handler:expr),* $(,)? })?
    ) => {{
        $($(
            $events.on_client($c_namespace, $c_event_name, $c_handler).await?;
        )*)?
        $($(
            $events.on_plugin($p_target_plugin, $p_event_name, $p_handler).await?;
        )*)?
        $($(
            $events.on_core($core_event_name, $core_handler).await?;
        )*)?
    }};
}

/// Simple macro for single handler registration (alternative to the bulk macro).
/// 
/// This macro provides a concise way to register individual event handlers
/// without the overhead of the bulk registration syntax. It's useful for
/// conditional registration or when you only need to register one handler.
/// 
/// # Syntax
/// 
/// - `on_event!(events, core "event_name" => handler)`
/// - `on_event!(events, client "namespace", "event_name" => handler)`  
/// - `on_event!(events, plugin "target_plugin", "event_name" => handler)`
/// 
/// # Examples
/// 
/// ```rust
/// // Register a core event handler
/// on_event!(events, core "server_started" => |event: ServerStartedEvent| {
///     println!("Server started at {}", event.timestamp);
///     Ok(())
/// });
/// 
/// // Register a client event handler
/// on_event!(events, client "movement", "jump" => |event: RawClientMessageEvent| {
///     handle_jump(event.player_id)?;
///     Ok(())
/// });
/// 
/// // Register a plugin event handler
/// on_event!(events, plugin "inventory", "item_used" => |event: ItemUsedEvent| {
///     apply_item_effects(event.player_id, event.item_id)?;
///     Ok(())
/// });
/// ```
#[macro_export]
macro_rules! on_event {
    ($events:expr, client $namespace:expr, $event_name:expr => $handler:expr) => {
        $events.on_client($namespace, $event_name, $handler).await?;
    };
    ($events:expr, plugin $target_plugin:expr, $event_name:expr => $handler:expr) => {
        $events
            .on_plugin($target_plugin, $event_name, $handler)
            .await?;
    };
    ($events:expr, core $event_name:expr => $handler:expr) => {
        $events.on_core($event_name, $handler).await?;
    };
}

/// Macro to register an object type with the GORC system.
/// 
/// This macro simplifies the registration of game objects that implement
/// the `GorcObject` trait with the GORC object registry. It handles
/// the type name generation and registration process automatically.
/// 
/// # Usage
/// 
/// ```rust
/// use horizon_event_system::{defObject, GorcObject, ReplicationLayer, Vec3, ReplicationPriority, CompressionType, MineralType};
/// use serde::{Serialize, Deserialize};
/// 
/// #[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// struct Asteroid {
///     pub radius: i32,
///     pub position: Vec3,
///     pub velocity: Vec3,
///     pub health: f32,
///     pub mineral_type: MineralType,
/// }
/// 
/// impl GorcObject for Asteroid {
///     fn type_name(&self) -> &str { "Asteroid" }
///     fn position(&self) -> Vec3 { self.position }
///     fn get_layers(&self) -> Vec<ReplicationLayer> {
///         vec![ReplicationLayer::new(
///             0, 50.0, 30.0, 
///             vec!["position".to_string(), "velocity".to_string()],
///             CompressionType::Delta
///         )]
///     }
///     fn get_priority(&self, _observer_pos: Vec3) -> ReplicationPriority {
///         ReplicationPriority::Normal
///     }
///     // ... other required methods
/// }
/// 
/// defObject!(Asteroid);
/// ```
/// 
/// This generates registration functions and handles the object lifecycle
/// for integration with the GORC system.
#[macro_export]
macro_rules! defObject {
    ($object_type:ty) => {
        /// Register this object type with a GORC object registry
        impl $object_type {
            pub async fn register_with_gorc(
                registry: std::sync::Arc<$crate::gorc::GorcObjectRegistry>
            ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                registry.register_object_type::<$object_type>(stringify!($object_type).to_string()).await;
                Ok(())
            }
            
            /// Get the type name of this object for GORC registration
            pub fn get_object_type_name() -> &'static str {
                stringify!($object_type)
            }
        }
    };
}