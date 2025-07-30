use horizon_event_system::{ABI_VERSION, horizon_build_info};

fn main() {
    println!("=== Horizon ABI Version Detection ===");
    println!();
    println!("🔧 ABI Version: {}", ABI_VERSION);
    println!("📋 Build Info: {}", horizon_build_info());
    println!();
    
    // Parse and display the components
    if let Some((crate_version, rust_version)) = ABI_VERSION.split_once(':') {
        println!("📦 Crate Version: {}", crate_version);
        println!("🦀 Rust Version: {}", rust_version);
        println!();
        
        if rust_version != "unknown" {
            println!("✅ Successfully detected Rust compiler version!");
            println!("   This ensures proper ABI compatibility between plugins and server.");
        } else {
            println!("⚠️  Could not detect Rust compiler version.");
            println!("   Falling back to 'unknown' - plugins may not be fully validated.");
        }
    } else {
        println!("❌ Invalid ABI version format!");
    }
    
    println!();
    println!("💡 The ABI version is used to ensure plugins are compatible with the server.");
    println!("   Plugins compiled with different Rust versions or crate versions may");
    println!("   have ABI incompatibilities that could cause crashes.");
}
