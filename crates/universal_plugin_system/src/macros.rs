//! Macros for plugin development

/// Macro to generate domain-specific event handler registration methods
/// 
/// This allows host applications to easily create familiar APIs on top of the universal system.
/// For example, horizon_event_system can use this to generate `on_core_async`, `on_client`, etc.
#[macro_export]
macro_rules! generate_event_handlers {
    // Generate a simple domain handler (e.g., on_core)
    (impl $method_name:ident, $domain:expr) => {
        /// Generated domain-specific event handler registration
        pub async fn $method_name<T, F>(&self, event_name: &str, handler: F) -> Result<(), $crate::EventError>
        where
            T: $crate::Event + for<'de> serde::Deserialize<'de>,
            F: Fn(T) -> Result<(), $crate::EventError> + Send + Sync + Clone + 'static,
        {
            self.event_bus.on($domain, event_name, handler).await
        }
    };
    
    // Generate a categorized domain handler (e.g., on_client with namespace)
    (impl $method_name:ident, $domain:expr, categorized) => {
        /// Generated categorized domain-specific event handler registration
        pub async fn $method_name<T, F>(
            &self,
            category: &str,
            event_name: &str,
            handler: F,
        ) -> Result<(), $crate::EventError>
        where
            T: $crate::Event + for<'de> serde::Deserialize<'de>,
            F: Fn(T) -> Result<(), $crate::EventError> + Send + Sync + Clone + 'static,
        {
            self.event_bus.on_categorized($domain, category, event_name, handler).await
        }
    };
    
    // Generate an async handler (for methods like on_core_async)
    (impl $method_name:ident, $domain:expr, async_handler) => {
        /// Generated async domain-specific event handler registration
        pub async fn $method_name<T, F>(&self, event_name: &str, handler: F) -> Result<(), $crate::EventError>
        where
            T: $crate::Event + for<'de> serde::Deserialize<'de>,
            F: Fn(T) -> Result<(), $crate::EventError> + Send + Sync + Clone + 'static,
        {
            // Wrap in async context - handler remains sync for DLL boundary safety
            let async_wrapper = move |event: T| -> Result<(), $crate::EventError> {
                handler(event)
            };
            self.event_bus.on($domain, event_name, async_wrapper).await
        }
    };
}

/// Macro to generate domain-specific event emission methods
/// 
/// This allows host applications to easily create familiar APIs like `emit_core`, `emit_client`, etc.
#[macro_export]
macro_rules! generate_event_emitters {
    // Generate a simple domain emitter (e.g., emit_core)
    (impl $method_name:ident, $domain:expr) => {
        /// Generated domain-specific event emission
        #[inline]
        pub async fn $method_name<T>(&self, event_name: &str, event: &T) -> Result<(), $crate::EventError>
        where
            T: $crate::Event + serde::Serialize,
        {
            self.event_bus.emit($domain, event_name, event).await
        }
    };
    
    // Generate a categorized domain emitter (e.g., emit_client with namespace)
    (impl $method_name:ident, $domain:expr, categorized) => {
        /// Generated categorized domain-specific event emission
        #[inline]
        pub async fn $method_name<T>(
            &self,
            category: &str,
            event_name: &str,
            event: &T,
        ) -> Result<(), $crate::EventError>
        where
            T: $crate::Event + serde::Serialize,
        {
            self.event_bus.emit_categorized($domain, category, event_name, event).await
        }
    };
}

/// Macro to create a complete domain-specific event system wrapper
/// 
/// This generates an entire wrapper struct with all the common event system methods
/// for a specific domain like Horizon, making it trivial to create familiar APIs.
#[macro_export]
macro_rules! create_domain_event_system {
    ($wrapper_name:ident, $key_type:ty, $propagator_type:ty) => {
        /// Domain-specific event system wrapper
        pub struct $wrapper_name {
            event_bus: std::sync::Arc<$crate::EventBus<$key_type, $propagator_type>>,
        }

        impl $wrapper_name {
            /// Create a new domain event system
            pub fn new(event_bus: std::sync::Arc<$crate::EventBus<$key_type, $propagator_type>>) -> Self {
                Self { event_bus }
            }

            /// Get the underlying event bus for advanced usage
            pub fn event_bus(&self) -> &std::sync::Arc<$crate::EventBus<$key_type, $propagator_type>> {
                &self.event_bus
            }
        }
    };
}

/// Macro to add domain-specific methods to an existing struct
/// 
/// This allows adding methods like `on_core_async` to any struct that has an `event_bus` field.
#[macro_export]
macro_rules! impl_domain_methods {
    // Core domain methods
    ($target:ty, core) => {
        impl $target {
            $crate::generate_event_handlers!(impl on_core, "core");
            $crate::generate_event_handlers!(impl on_core_async, "core", async_handler);
            $crate::generate_event_emitters!(impl emit_core, "core");
        }
    };
    
    // Client domain methods  
    ($target:ty, client) => {
        impl $target {
            $crate::generate_event_handlers!(impl on_client, "client", categorized);
            $crate::generate_event_handlers!(impl on_client_async, "client", categorized);
            $crate::generate_event_emitters!(impl emit_client, "client", categorized);
        }
    };
    
    // Plugin domain methods
    ($target:ty, plugin) => {
        impl $target {
            $crate::generate_event_handlers!(impl on_plugin, "plugin", categorized);
            $crate::generate_event_emitters!(impl emit_plugin, "plugin", categorized);
        }
    };
    
    // GORC domain methods
    ($target:ty, gorc) => {
        impl $target {
            /// Register handler for GORC object events on a specific channel
            pub async fn on_gorc<T, F>(
                &self,
                object_type: &str,
                channel: u8,
                event_name: &str,
                handler: F,
            ) -> Result<(), $crate::EventError>
            where
                T: $crate::Event + for<'de> serde::Deserialize<'de>,
                F: Fn(T) -> Result<(), $crate::EventError> + Send + Sync + Clone + 'static,
            {
                self.event_bus.on_gorc_class("gorc", object_type, channel, event_name, true, handler).await
            }
            
            /// Emit GORC event using object type string
            #[inline]
            pub async fn emit_gorc<T>(
                &self,
                object_type: &str,
                channel: u8,
                event_name: &str,
                event: &T,
            ) -> Result<(), $crate::EventError>
            where
                T: $crate::Event + serde::Serialize,
            {
                self.event_bus.emit_gorc_class("gorc", object_type, channel, event_name, true, event).await
            }
        }
    };
    
    // All common domains at once
    ($target:ty, all) => {
        $crate::impl_domain_methods!($target, core);
        $crate::impl_domain_methods!($target, client);
        $crate::impl_domain_methods!($target, plugin);
        $crate::impl_domain_methods!($target, gorc);
    };
}

/// Macro to create a plugin with minimal boilerplate and comprehensive panic handling
/// 
/// This macro generates all the necessary FFI wrapper code to bridge between
/// the `SimplePlugin` trait and the lower-level `Plugin` trait.
#[macro_export]
macro_rules! create_plugin {
    ($plugin_type:ty, $propagator:ty) => {
        use $crate::{Plugin, PluginWrapper};
        use std::panic::{catch_unwind, AssertUnwindSafe};

        /// Plugin version function - required export for ABI compatibility
        #[no_mangle]
        pub unsafe extern "C" fn get_plugin_version() -> *const std::os::raw::c_char {
            let version_cstring = std::ffi::CString::new($crate::UNIVERSAL_PLUGIN_SYSTEM_VERSION)
                .unwrap_or_else(|_| std::ffi::CString::new("invalid_version").unwrap());
            
            // Leak the CString to ensure it remains valid for the caller
            version_cstring.into_raw()
        }

        /// Plugin creation function with panic protection - required export
        #[no_mangle]
        pub unsafe extern "C" fn create_plugin() -> *mut dyn Plugin<$propagator> {
            // Critical: catch panics at FFI boundary to prevent UB
            match catch_unwind(AssertUnwindSafe(|| {
                let plugin = <$plugin_type>::new();
                let wrapper = PluginWrapper::new(plugin);
                Box::into_raw(Box::new(wrapper)) as *mut dyn Plugin<$propagator>
            })) {
                Ok(plugin_ptr) => plugin_ptr,
                Err(panic_info) => {
                    eprintln!("Plugin creation panicked: {:?}", panic_info);
                    std::ptr::null_mut()
                }
            }
        }

        /// Plugin destruction function with panic protection - required export
        #[no_mangle]
        pub unsafe extern "C" fn destroy_plugin(plugin: *mut dyn Plugin<$propagator>) {
            if plugin.is_null() {
                return;
            }

            let _ = catch_unwind(AssertUnwindSafe(|| {
                let _ = Box::from_raw(plugin);
            }));
        }
    };
}

/// Convenience macro for registering multiple handlers with clean syntax
#[macro_export]
macro_rules! register_handlers {
    ($event_bus:expr; $($namespace:expr, $event_name:expr => $handler:expr),* $(,)?) => {{
        $(
            $event_bus.on($namespace, $event_name, $handler).await?;
        )*
        Ok::<(), $crate::PluginSystemError>(())
    }};
    
    ($event_bus:expr; $($namespace:expr, $category:expr, $event_name:expr => $handler:expr),* $(,)?) => {{
        $(
            $event_bus.on_categorized($namespace, $category, $event_name, $handler).await?;
        )*
        Ok::<(), $crate::PluginSystemError>(())
    }};
}

/// Simple macro for single handler registration
#[macro_export]
macro_rules! on_event {
    ($event_bus:expr, $namespace:expr, $event_name:expr => $handler:expr) => {
        $event_bus.on($namespace, $event_name, $handler).await?;
    };
    ($event_bus:expr, $namespace:expr, $category:expr, $event_name:expr => $handler:expr) => {
        $event_bus.on_categorized($namespace, $category, $event_name, $handler).await?;
    };
}

/// Macro to define an event type with automatic trait implementations
#[macro_export]
macro_rules! define_event {
    ($name:ident { $($field:ident: $type:ty),* $(,)? }) => {
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct $name {
            $(pub $field: $type,)*
        }

        impl $crate::Event for $name {
            fn event_type() -> &'static str {
                stringify!($name)
            }
        }
    };
}

/// Macro to create a plugin factory
#[macro_export]
macro_rules! create_plugin_factory {
    ($plugin_type:ty, $propagator:ty, $name:expr, $version:expr) => {
        $crate::SimplePluginFactory::<$plugin_type, $propagator>::new(
            $name.to_string(),
            $version.to_string(),
            || <$plugin_type>::new(),
        )
    };
}

/// Macro to help with context provider registration
#[macro_export]
macro_rules! add_providers {
    ($context:expr; $($provider:expr),* $(,)?) => {{
        $(
            $context.add_provider($provider);
        )*
    }};
}