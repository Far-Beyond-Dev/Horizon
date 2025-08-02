//! Utility functions and helpers

use std::time::{SystemTime, UNIX_EPOCH};

/// Get current timestamp in milliseconds
pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Generate a unique ID
pub fn generate_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Serialize an object to JSON bytes
pub fn serialize_to_json<T: serde::Serialize>(obj: &T) -> Result<Vec<u8>, crate::error::PluginSystemError> {
    serde_json::to_vec(obj)
        .map_err(|e| crate::error::PluginSystemError::SerializationError(e.to_string()))
}

/// Deserialize from JSON bytes
pub fn deserialize_from_json<T: for<'de> serde::Deserialize<'de>>(data: &[u8]) -> Result<T, crate::error::PluginSystemError> {
    serde_json::from_slice(data)
        .map_err(|e| crate::error::PluginSystemError::SerializationError(e.to_string()))
}

/// Format a version string
pub fn format_version(major: u32, minor: u32, patch: u32) -> String {
    format!("{}.{}.{}", major, minor, patch)
}

/// Parse a version string into components
pub fn parse_version(version: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() >= 3 {
        if let (Ok(major), Ok(minor), Ok(patch)) = (
            parts[0].parse::<u32>(),
            parts[1].parse::<u32>(),
            parts[2].parse::<u32>(),
        ) {
            return Some((major, minor, patch));
        }
    }
    None
}

/// Check if a version is compatible with another version (major.minor matching)
pub fn is_version_compatible(version1: &str, version2: &str) -> bool {
    if let (Some((major1, minor1, _)), Some((major2, minor2, _))) = 
        (parse_version(version1), parse_version(version2)) {
        major1 == major2 && minor1 == minor2
    } else {
        version1 == version2
    }
}

/// Safe string conversion from C string pointer
pub unsafe fn c_str_to_string(ptr: *const std::os::raw::c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    
    std::ffi::CStr::from_ptr(ptr).to_str().ok().map(|s| s.to_string())
}

/// Convert Rust string to C string
pub fn string_to_c_str(s: &str) -> std::ffi::CString {
    std::ffi::CString::new(s).unwrap_or_else(|_| std::ffi::CString::new("invalid_string").unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parsing() {
        assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("0.1.0"), Some((0, 1, 0)));
        assert_eq!(parse_version("invalid"), None);
        assert_eq!(parse_version("1.2"), None);
    }

    #[test]
    fn test_version_compatibility() {
        assert!(is_version_compatible("1.2.3", "1.2.4"));
        assert!(is_version_compatible("1.2.0", "1.2.999"));
        assert!(!is_version_compatible("1.2.0", "1.3.0"));
        assert!(!is_version_compatible("1.2.0", "2.2.0"));
        assert!(is_version_compatible("invalid", "invalid"));
        assert!(!is_version_compatible("invalid", "1.2.3"));
    }

    #[test]
    fn test_format_version() {
        assert_eq!(format_version(1, 2, 3), "1.2.3");
        assert_eq!(format_version(0, 1, 0), "0.1.0");
    }

    #[test]
    fn test_serialization() {
        use serde::{Deserialize, Serialize};

        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct TestStruct {
            name: String,
            value: i32,
        }

        let original = TestStruct {
            name: "test".to_string(),
            value: 42,
        };

        let serialized = serialize_to_json(&original).unwrap();
        let deserialized: TestStruct = deserialize_from_json(&serialized).unwrap();

        assert_eq!(original.name, deserialized.name);
        assert_eq!(original.value, deserialized.value);
    }

    #[test]
    fn test_id_generation() {
        let id1 = generate_id();
        let id2 = generate_id();
        
        assert_ne!(id1, id2);
        assert!(!id1.is_empty());
        assert!(!id2.is_empty());
    }
}