//! Macros for plugin development

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