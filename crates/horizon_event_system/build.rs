//! Build script for horizon_event_system crate.
//!
//! This build script extracts the Rust compiler version and makes it available
//! as an environment variable during compilation for ABI version generation.

use std::process::Command;

fn main() {
    // Get the Rust compiler version
    let rust_version = get_rust_version();
    
    // Set the environment variable for use in the crate
    println!("cargo:rustc-env=HORIZON_RUSTC_VERSION={}", rust_version);
    
    // Tell cargo to re-run this build script if the Rust toolchain changes
    println!("cargo:rerun-if-env-changed=RUSTC_VERSION");
    println!("cargo:rerun-if-changed=build.rs");
}

fn get_rust_version() -> String {
    // First, try to get from RUSTC_VERSION environment variable (set by some CI systems)
    if let Ok(version) = std::env::var("RUSTC_VERSION") {
        return version;
    }
    
    // Try to get from rustc --version command
    if let Ok(output) = Command::new("rustc").arg("--version").output() {
        if output.status.success() {
            let version_output = String::from_utf8_lossy(&output.stdout);
            // Parse "rustc 1.75.0 (82e1608df 2023-12-21)" to extract "1.75.0"
            if let Some(version_line) = version_output.lines().next() {
                let parts: Vec<&str> = version_line.split_whitespace().collect();
                if parts.len() >= 2 && parts[0] == "rustc" {
                    return parts[1].to_string();
                }
            }
        }
    }
    
    // Fallback to unknown if we can't determine the version
    "unknown".to_string()
}
