//! Input validation and sanitization utilities.

use super::SecurityError;
use crate::config::SecurityConfig;
use serde_json::Value;

/// Validates a JSON message for security concerns using the provided config
pub fn validate_json_message(message: &[u8], config: &SecurityConfig) -> Result<(), SecurityError> {
    // Check message size
    if message.len() > config.max_message_size {
        return Err(SecurityError::MessageTooLarge(message.len()));
    }

    // Parse JSON
    let json: Value = serde_json::from_slice(message)
        .map_err(|e| SecurityError::InvalidMessageFormat(e.to_string()))?;

    // Validate JSON structure
    validate_json_value(&json, 0, config)?;

    // Additional security checks
    check_for_malicious_patterns(&json)?;

    Ok(())
}

/// Legacy function for backward compatibility (uses default config)
pub fn validate_json_message_default(message: &[u8]) -> Result<(), SecurityError> {
    let default_config = SecurityConfig::default();
    validate_json_message(message, &default_config)
}

/// Recursively validates a JSON value
fn validate_json_value(value: &Value, depth: usize, config: &SecurityConfig) -> Result<(), SecurityError> {
    if depth > config.max_json_depth {
        return Err(SecurityError::InvalidMessageFormat(
            "JSON nesting too deep".to_string()
        ));
    }

    match value {
        Value::String(s) => {
            if s.len() > config.max_string_length {
                return Err(SecurityError::InvalidMessageFormat(
                    format!("String too long: {} characters", s.len())
                ));
            }
            validate_string_content(s)?;
        }
        Value::Array(arr) => {
            if arr.len() > config.max_collection_size {
                return Err(SecurityError::InvalidMessageFormat(
                    format!("Array too large: {} elements", arr.len())
                ));
            }
            for item in arr {
                validate_json_value(item, depth + 1, config)?;
            }
        }
        Value::Object(obj) => {
            if obj.len() > config.max_collection_size {
                return Err(SecurityError::InvalidMessageFormat(
                    format!("Object too large: {} keys", obj.len())
                ));
            }
            for (key, val) in obj {
                if key.len() > config.max_string_length {
                    return Err(SecurityError::InvalidMessageFormat(
                        format!("Object key too long: {} characters", key.len())
                    ));
                }
                validate_string_content(key)?;
                validate_json_value(val, depth + 1, config)?;
            }
        }
        Value::Number(n) => {
            // Check for extremely large numbers that could cause issues
            if let Some(f) = n.as_f64() {
                if !f.is_finite() {
                    return Err(SecurityError::InvalidMessageFormat(
                        "Invalid number: not finite".to_string()
                    ));
                }
            }
        }
        Value::Bool(_) | Value::Null => {
            // These are always safe
        }
    }

    Ok(())
}

/// Validates string content for malicious patterns
fn validate_string_content(s: &str) -> Result<(), SecurityError> {
    // Check for null bytes
    if s.contains('\0') {
        return Err(SecurityError::MaliciousContent);
    }

    // Check for excessive control characters
    let control_char_count = s.chars().filter(|c| c.is_control() && *c != '\n' && *c != '\r' && *c != '\t').count();
    if control_char_count > 5 {
        return Err(SecurityError::MaliciousContent);
    }

    // Check for potential script injection patterns
    let lower = s.to_lowercase();
    let dangerous_patterns = [
        "<script", "javascript:", "data:text/html", "vbscript:",
        "onload=", "onerror=", "onclick=", "eval(", "setTimeout(",
        "setInterval(", "document.cookie", "window.location"
    ];

    for pattern in &dangerous_patterns {
        if lower.contains(pattern) {
            return Err(SecurityError::MaliciousContent);
        }
    }

    Ok(())
}

/// Checks for malicious patterns in the entire JSON structure
fn check_for_malicious_patterns(json: &Value) -> Result<(), SecurityError> {
    let json_str = json.to_string();
    
    // Check for excessively long JSON representation
    if json_str.len() > 1024 * 1024 { // 1MB
        return Err(SecurityError::InvalidMessageFormat(
            "JSON representation too large".to_string()
        ));
    }

    // Check for potential ReDoS patterns
    if contains_redos_pattern(&json_str) {
        return Err(SecurityError::MaliciousContent);
    }

    Ok(())
}

/// Checks for potential Regular Expression Denial of Service patterns
fn contains_redos_pattern(s: &str) -> bool {
    // Look for patterns that could cause catastrophic backtracking
    let redos_indicators = [
        // Nested quantifiers
        r"(\+\+|\*\*|\+\*|\*\+)",
        // Alternation with overlapping
        r"\([^)]*\|[^)]*\)\+",
    ];
    
    // Check for excessive repetition separately
    let excessive_repetition = ".".repeat(1000);
    if s.contains(&excessive_repetition) {
        return true;
    }

    for pattern in &redos_indicators {
        if s.contains(pattern) {
            return true;
        }
    }

    false
}

/// Sanitizes a string by removing or escaping dangerous characters
pub fn sanitize_string(input: &str, config: &SecurityConfig) -> String {
    input
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\r' || *c == '\t')
        .take(config.max_string_length)
        .collect()
}

/// Legacy sanitize function using default config
pub fn sanitize_string_default(input: &str) -> String {
    let default_config = SecurityConfig::default();
    sanitize_string(input, &default_config)
}

/// Validates a namespace string for plugin events
pub fn validate_namespace(namespace: &str) -> Result<(), SecurityError> {
    if namespace.is_empty() || namespace.len() > 64 {
        return Err(SecurityError::InvalidMessageFormat(
            "Invalid namespace length".to_string()
        ));
    }

    // Only allow alphanumeric characters and underscores
    if !namespace.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(SecurityError::InvalidMessageFormat(
            "Invalid namespace characters".to_string()
        ));
    }

    Ok(())
}

/// Validates an event name string
pub fn validate_event_name(event_name: &str) -> Result<(), SecurityError> {
    if event_name.is_empty() || event_name.len() > 64 {
        return Err(SecurityError::InvalidMessageFormat(
            "Invalid event name length".to_string()
        ));
    }

    // Only allow alphanumeric characters, underscores, and hyphens
    if !event_name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return Err(SecurityError::InvalidMessageFormat(
            "Invalid event name characters".to_string()
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_simple_json() {
        let json = br#"{"action": "move", "x": 100, "y": 200}"#;
        assert!(validate_json_message_default(json).is_ok());
    }

    #[test]
    fn test_reject_oversized_json() {
        let large_string = "x".repeat(2000);
        let json = format!(r#"{{"data": "{}"}}"#, large_string);
        assert!(validate_json_message_default(json.as_bytes()).is_err());
    }

    #[test]
    fn test_reject_deep_nesting() {
        let mut json = String::from("{");
        for _ in 0..15 {
            json.push_str(r#""nested": {"#);
        }
        json.push_str(r#""value": true"#);
        for _ in 0..15 {
            json.push('}');
        }
        json.push('}');
        
        assert!(validate_json_message_default(json.as_bytes()).is_err());
    }

    #[test]
    fn test_reject_script_injection() {
        let json = br#"{"message": "<script>alert('xss')</script>"}"#;
        assert!(validate_json_message_default(json).is_err());
    }

    #[test]
    fn test_sanitize_string() {
        let input = "Hello\0World\x01!";
        let sanitized = sanitize_string_default(input);
        assert_eq!(sanitized, "HelloWorld!");
    }

    #[test]
    fn test_custom_config_validation() {
        let config = SecurityConfig {
            max_string_length: 5,
            max_collection_size: 2,
            max_json_depth: 2,
            ..SecurityConfig::default()
        };

        // Should fail with custom limits
        let json = br#"{"key": "toolong"}"#;
        assert!(validate_json_message(json, &config).is_err());

        // Should pass with shorter string
        let json = br#"{"key": "ok"}"#;
        assert!(validate_json_message(json, &config).is_ok());
    }

    #[test]
    fn test_validate_namespace() {
        assert!(validate_namespace("movement").is_ok());
        assert!(validate_namespace("chat_system").is_ok());
        assert!(validate_namespace("").is_err());
        assert!(validate_namespace("invalid-chars!").is_err());
    }
}