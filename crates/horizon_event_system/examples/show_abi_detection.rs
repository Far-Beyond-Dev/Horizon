use horizon_event_system::{ABI_VERSION, horizon_build_info};
use tracing::info;

fn main() {
    info!("=== Horizon ABI Version Detection ===");
    info!("🔧 ABI Version: {}", ABI_VERSION);
    info!("📋 Build Info: {}", horizon_build_info());
    
    // Parse and display the components
    if let Some((crate_version, rust_version)) = ABI_VERSION.split_once(':') {
        info!("📦 Crate Version: {}", crate_version);
        info!("🦀 Rust Version: {}", rust_version);
            
        if rust_version != "unknown" {
            info!("✅ Successfully detected Rust compiler version!");
            info!("   This ensures proper ABI compatibility between plugins and server.");
        } else {
            info!("⚠️  Could not detect Rust compiler version.");
            info!("   Falling back to 'unknown' - plugins may not be fully validated.");
        }
    } else {
        info!("❌ Invalid ABI version format!");
    }
    
    info!("💡 The ABI version is used to ensure plugins are compatible with the server.");
    info!("   Plugins compiled with different Rust versions or crate versions may");
    info!("   have ABI incompatibilities that could cause crashes.");
}
