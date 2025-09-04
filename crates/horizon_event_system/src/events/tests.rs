#[cfg(test)]
mod tests {
    use crate::events::{ClientEventWrapper, Event};
    use crate::types::PlayerId;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestChatEvent {
        message: String,
        channel: String,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] 
    struct TestMovementEvent {
        x: f32,
        y: f32,
        z: f32,
    }

    #[test]
    fn test_client_event_wrapper_creation() {
        let player_id = PlayerId::new();
        let chat_event = TestChatEvent {
            message: "Hello world!".to_string(),
            channel: "global".to_string(),
        };

        let wrapper = ClientEventWrapper::new(player_id, chat_event.clone());
        
        assert_eq!(wrapper.player_id, player_id);
        assert_eq!(wrapper.data, chat_event);
    }

    #[test]
    fn test_client_event_wrapper_methods() {
        let player_id = PlayerId::new();
        let movement_event = TestMovementEvent { x: 100.0, y: 0.0, z: 200.0 };
        
        let mut wrapper = ClientEventWrapper::new(player_id, movement_event.clone());
        
        // Test data() method
        assert_eq!(wrapper.data(), &movement_event);
        
        // Test data_mut() method
        wrapper.data_mut().x = 150.0;
        assert_eq!(wrapper.data().x, 150.0);
        
        // Test into_data() method
        let extracted = wrapper.into_data();
        assert_eq!(extracted.x, 150.0);
        assert_eq!(extracted.y, 0.0);
        assert_eq!(extracted.z, 200.0);
    }

    #[test]
    fn test_client_event_wrapper_serialization() {
        let player_id = PlayerId::new();
        let chat_event = TestChatEvent {
            message: "Test message".to_string(),
            channel: "test".to_string(),
        };
        
        let wrapper = ClientEventWrapper::new(player_id, chat_event);
        
        // Test serialization to JSON
        let json_result = serde_json::to_value(&wrapper);
        assert!(json_result.is_ok());
        
        let json_value = json_result.unwrap();
        assert!(json_value.get("player_id").is_some());
        assert!(json_value.get("data").is_some());
        
        // Verify data structure
        let data = json_value.get("data").unwrap();
        assert_eq!(data.get("message").unwrap(), "Test message");
        assert_eq!(data.get("channel").unwrap(), "test");
    }

    #[test]
    fn test_client_event_wrapper_deserialization() {
        let player_id = PlayerId::new();
        
        // Create JSON that matches the wrapper structure
        let json_data = serde_json::json!({
            "player_id": player_id,
            "data": {
                "message": "Deserialized message",
                "channel": "deserial"
            }
        });
        
        // Test deserialization from JSON
        let wrapper_result: Result<ClientEventWrapper<TestChatEvent>, _> = 
            serde_json::from_value(json_data);
            
        assert!(wrapper_result.is_ok());
        
        let wrapper = wrapper_result.unwrap();
        assert_eq!(wrapper.player_id, player_id);
        assert_eq!(wrapper.data.message, "Deserialized message");
        assert_eq!(wrapper.data.channel, "deserial");
    }

    #[test]
    fn test_client_event_wrapper_roundtrip() {
        let player_id = PlayerId::new();
        let original_event = TestMovementEvent { x: 42.0, y: 100.5, z: -25.3 };
        let original_wrapper = ClientEventWrapper::new(player_id, original_event.clone());
        
        // Serialize to JSON bytes (what the event system does)
        let json_bytes = serde_json::to_vec(&original_wrapper).unwrap();
        
        // Deserialize back (what the typed handler receives)
        let deserialized_wrapper: ClientEventWrapper<TestMovementEvent> = 
            serde_json::from_slice(&json_bytes).unwrap();
        
        // Verify everything matches
        assert_eq!(deserialized_wrapper.player_id, player_id);
        assert_eq!(deserialized_wrapper.data, original_event);
    }

    #[test]
    fn test_client_event_wrapper_implements_event_trait() {
        let player_id = PlayerId::new();
        let chat_event = TestChatEvent {
            message: "Event trait test".to_string(),
            channel: "trait_test".to_string(),
        };
        let wrapper = ClientEventWrapper::new(player_id, chat_event);
        
        // Test Event trait methods
        let type_name = ClientEventWrapper::<TestChatEvent>::type_name();
        assert!(type_name.contains("ClientEventWrapper"));
        
        // Test serialization via Event trait
        let serialized = Event::serialize(&wrapper);
        assert!(serialized.is_ok());
        
        // Test deserialization via Event trait
        let bytes = serialized.unwrap();
        let deserialized = <ClientEventWrapper<TestChatEvent> as Event>::deserialize(&bytes);
        assert!(deserialized.is_ok());
        
        let recovered_wrapper = deserialized.unwrap();
        assert_eq!(recovered_wrapper.player_id, player_id);
        assert_eq!(recovered_wrapper.data.message, "Event trait test");
    }

    #[test]
    fn test_emit_client_with_context_format_compatibility() {
        // This tests that our wrapper matches the format created by emit_client_with_context
        let player_id = PlayerId::new();
        let chat_event = TestChatEvent {
            message: "Context test".to_string(),
            channel: "context".to_string(),
        };
        
        // Simulate what emit_client_with_context does
        let context_json = serde_json::json!({
            "player_id": player_id,
            "data": chat_event
        });
        
        // Convert to bytes (what gets passed to handlers)
        let context_bytes = serde_json::to_vec(&context_json).unwrap();
        
        // Try to deserialize as our wrapper type
        let wrapper_result: Result<ClientEventWrapper<TestChatEvent>, _> = 
            serde_json::from_slice(&context_bytes);
            
        assert!(wrapper_result.is_ok(), "Wrapper should deserialize from emit_client_with_context format");
        
        let wrapper = wrapper_result.unwrap();
        assert_eq!(wrapper.player_id, player_id);
        assert_eq!(wrapper.data.message, "Context test");
        assert_eq!(wrapper.data.channel, "context");
    }
}