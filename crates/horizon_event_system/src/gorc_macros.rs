/// Macro for easily defining simple GORC objects
/// 
/// This macro reduces boilerplate by automatically implementing the SimpleGorcObject trait
/// and providing sensible defaults for most object types.
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::define_simple_gorc_object;
/// define_simple_gorc_object! {
///     struct MyAsteroid {
///         position: Vec3,
///         velocity: Vec3,
///         health: f32,
///         mineral_type: MineralType,
///     }
///     
///     type_name: "MyAsteroid",
///     
///     channels: {
///         0 => ["position", "health"],      // Critical
///         1 => ["velocity"],                // Detailed  
///         3 => ["mineral_type"],            // Metadata
///     }
/// }
/// ```
#[macro_export]
macro_rules! define_simple_gorc_object {
    (
        struct $name:ident {
            position: Vec3,
            $($field:ident: $field_type:ty),*
        }
        
        type_name: $type_name:expr,
        
        channels: {
            $($channel:expr => [$($prop:expr),*]),*
        }
        
        $(config: {
            $($config_field:ident: $config_value:expr),*
        })?
    ) => {
        #[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
        pub struct $name {
            pub position: $crate::Vec3,
            $(pub $field: $field_type),*
        }
        
        impl $crate::SimpleGorcObject for $name {
            fn position(&self) -> $crate::Vec3 {
                self.position
            }
            
            fn set_position(&mut self, position: $crate::Vec3) {
                self.position = position;
            }
            
            fn object_type() -> &'static str {
                $type_name
            }
            
            fn channel_properties(channel: u8) -> Vec<String> {
                match channel {
                    $(
                        $channel => vec![$($prop.to_string()),*],
                    )*
                    _ => vec![]
                }
            }
            
            $(
                fn replication_config() -> $crate::SimpleReplicationConfig {
                    let mut config = $crate::SimpleReplicationConfig::default();
                    $(
                        config.$config_field = $config_value;
                    )*
                    config
                }
            )?
        }
    };
}