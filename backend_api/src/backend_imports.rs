// This file is automatically generated by build.rs
// Do not edit this file manually!

use std::collections::HashMap;

pub use stars_beyond;
pub use stars_beyond::*;
pub use stars_beyond::Plugin as stars_beyond_backend;


// Invoke the macro with all discovered backends
pub fn load_backends() -> HashMap<String, (Pluginstate, Plugin)> {
    let backends = crate::load_backends!(
        stars_beyond
    );
    backends
}
