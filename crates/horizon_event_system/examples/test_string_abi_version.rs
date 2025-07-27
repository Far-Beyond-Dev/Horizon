use horizon_event_system::ABI_VERSION;

fn main() {
    println!("Current ABI version: {}", ABI_VERSION);
    
    // Parse and validate the format
    let parts: Vec<&str> = ABI_VERSION.split(':').collect();
    
    if parts.len() == 2 {
        let crate_version = parts[0];
        let rust_version = parts[1];
        
        println!("✅ ABI version format is valid:");
        println!("   Crate version: {}", crate_version);
        println!("   Rust version: {}", rust_version);
        
        // Verify it's the expected crate version
        assert_eq!(crate_version, env!("CARGO_PKG_VERSION"), "Should match the current Cargo.toml version");
        
        println!("✅ All checks passed!");
    } else {
        panic!("❌ Invalid ABI version format: {}", ABI_VERSION);
    }
}
