use horizon_event_system::ABI_VERSION;

fn main() {
    println!("Current ABI version: {}", ABI_VERSION);
    println!("For version 0.10.0, this should be: 0.10.0:rust_version");
    
    // Verify it's not the old hardcoded value
    assert_ne!(ABI_VERSION, "1", "ABI version should not be the old hardcoded value");
    
    // Verify it starts with the correct crate version
    assert!(ABI_VERSION.starts_with("0.10.0:"), "ABI version should start with '0.10.0:' for version 0.10.0");
    
    // Verify it contains the colon separator
    assert!(ABI_VERSION.contains(':'), "ABI version should contain ':' separator");
    
    println!("✅ ABI version is correctly set to: {}", ABI_VERSION);
}
