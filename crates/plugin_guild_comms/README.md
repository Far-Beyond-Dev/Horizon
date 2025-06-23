# Guild Communications System

A game-ready chat, channel, and clan management plugin for game servers with advanced permissions integration and real-time event-driven communication.

## 🚀 Features

- **Multi-tier Chat System**: Public channels, private channels, clan communications, and direct messages
- **Advanced Permissions**: Role-based access control with external permissions plugin integration
- **Clan Management**: Automatic channel creation for clans with officer-only areas
- **Real-time Events**: Comprehensive event system for cross-plugin communication
- **Message Management**: History, reactions, editing, muting, and moderation tools
- **Scalable Architecture**: Event-driven design with complete plugin isolation

## 📦 Installation

The Guild Communications system runs as an independent plugin with no direct dependencies on other plugins.

## 🔌 Integration with Other Plugins

**Important**: Plugins communicate exclusively through the event system using JSON payloads. No direct type access between plugins is allowed.

### Sending Events TO Guild Communications

Other plugins can interact with the guild system by emitting JSON events:

#### Send a Chat Message
```rust
// From any plugin - send via JSON only
let message_data = serde_json::json!({
    "action": "SendMessage",
    "user_id": "550e8400-e29b-41d4-a716-446655440000",
    "channel_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
    "content": "Hello from my plugin!",
    "target_user": null,
    "message_id": null
});

events.emit_plugin("GuildComms", "Chat", &message_data).await;
```

#### Create a New Channel
```rust
let create_channel_data = serde_json::json!({
    "action": "CreateChannel",
    "user_id": "550e8400-e29b-41d4-a716-446655440000",
    "channel_id": null,
    "content": {
        "name": "new-game-lobby",
        "channel_type": "Public",
        "description": "Game lobby chat",
        "clan_id": null
    },
    "target_user": null,
    "message_id": null
});

events.emit_plugin("GuildComms", "Chat", &create_channel_data).await;
```

#### Join a Channel
```rust
let join_data = serde_json::json!({
    "action": "JoinChannel",
    "user_id": "550e8400-e29b-41d4-a716-446655440000",
    "channel_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
    "content": null,
    "target_user": null,
    "message_id": null
});

events.emit_plugin("GuildComms", "Chat", &join_data).await;
```

#### Register a New User
```rust
let new_user_data = serde_json::json!({
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "username": "PlayerName",
    "clan_id": null,
    "roles": [],
    "permissions": ["SendMessages"],
    "last_seen": "2025-06-23T10:30:00Z",
    "muted_until": null
});

events.emit_plugin("GuildComms", "User", &new_user_data).await;
```

#### Create a Clan
```rust
let new_clan_data = serde_json::json!({
    "id": "550e8400-e29b-41d4-a716-446655440001",
    "name": "Elite Warriors",
    "tag": "EW",
    "description": "The best of the best",
    "leader_id": "550e8400-e29b-41d4-a716-446655440000",
    "officers": [],
    "members": ["550e8400-e29b-41d4-a716-446655440000"],
    "channels": [],
    "permissions": {
        "can_invite": ["550e8400-e29b-41d4-a716-446655440000"],
        "can_kick": ["550e8400-e29b-41d4-a716-446655440000"],
        "can_promote": ["550e8400-e29b-41d4-a716-446655440000"],
        "can_manage_channels": ["550e8400-e29b-41d4-a716-446655440000"],
        "can_edit_info": ["550e8400-e29b-41d4-a716-446655440000"]
    },
    "created_at": "2025-06-23T10:30:00Z",
    "level": 1,
    "is_recruiting": true
});

events.emit_plugin("GuildComms", "Clan", &new_clan_data).await;
```

#### Create a Role
```rust
let new_role_data = serde_json::json!({
    "id": "550e8400-e29b-41d4-a716-446655440002",
    "name": "Moderator",
    "color": "#FF5733",
    "permissions": ["SendMessages", "DeleteAnyMessages", "MuteUsers"],
    "priority": 10,
    "is_mentionable": true,
    "is_hoisted": true
});

events.emit_plugin("GuildComms", "Role", &new_role_data).await;
```

### Listening for Events FROM Guild Communications

Register handlers to respond to guild system events using JSON:

#### Listen for New Messages
```rust
events.on_plugin(
    "GuildComms",
    "MessageSent",
    |json_event: serde_json::Value| {
        // Parse the channel event
        let event_type = json_event["event_type"].as_str().unwrap();
        let channel_id = json_event["channel_id"].as_str().unwrap();
        let user_id = json_event["user_id"].as_str().unwrap();
        
        if let Some(message_data) = json_event["data"].as_object() {
            let content = message_data["content"].as_str().unwrap_or("");
            let sender_name = message_data["sender_name"].as_str().unwrap_or("Unknown");
            
            println!("📨 New message from {}: {}", sender_name, content);
            
            // Your plugin logic here
            // Example: Check for spam, log messages, trigger notifications, etc.
        }
        
        Ok(())
    },
).await?;
```

#### Listen for Channel Events
```rust
events.on_plugin(
    "GuildComms",
    "ChannelCreated",
    |json_event: serde_json::Value| {
        let channel_id = json_event["channel_id"].as_str().unwrap();
        let creator_id = json_event["user_id"].as_str().unwrap();
        
        if let Some(data) = json_event["data"].as_object() {
            let channel_name = data["name"].as_str().unwrap_or("Unknown");
            let channel_type = data["channel_type"].as_str().unwrap_or("Unknown");
            
            println!("🆕 New {} channel created: {} by {}", 
                     channel_type, channel_name, creator_id);
        }
        
        Ok(())
    },
).await?;

events.on_plugin(
    "GuildComms",
    "UserJoinedChannel",
    |json_event: serde_json::Value| {
        let channel_id = json_event["channel_id"].as_str().unwrap();
        let user_id = json_event["user_id"].as_str().unwrap();
        
        println!("👋 User {} joined channel {}", user_id, channel_id);
        
        // Example: Send welcome message, update user stats, etc.
        
        Ok(())
    },
).await?;
```

#### Listen for Clan Events
```rust
events.on_plugin(
    "GuildComms",
    "Clan",
    |json_event: serde_json::Value| {
        let clan_name = json_event["name"].as_str().unwrap_or("Unknown");
        let clan_tag = json_event["tag"].as_str().unwrap_or("");
        let leader_id = json_event["leader_id"].as_str().unwrap();
        
        println!("🏰 New clan created: {} [{}] led by {}", 
                 clan_name, clan_tag, leader_id);
        
        // Your plugin logic here
        // Example: Update leaderboards, send notifications, etc.
        
        Ok(())
    },
).await?;
```

### Permissions Plugin Integration

If you're building a permissions plugin, listen for permission requests:

#### Handle Permission Checks
```rust
events.on_plugin(
    "Permissions",
    "CheckPermission",
    |json_event: serde_json::Value| {
        let user_id = json_event["user_id"].as_str().unwrap();
        let permission = json_event["permission"].as_str().unwrap();
        let channel_id = json_event["channel_id"].as_str();
        
        // Your permission logic here
        let granted = check_user_permission(user_id, permission, channel_id);
        
        // Send response back
        let response = serde_json::json!({
            "granted": granted,
            "reason": if granted { null } else { "Insufficient permissions" }
        });
        
        events.emit_plugin("Permissions", "PermissionResponse", &response).await;
        
        Ok(())
    },
).await?;
```

## 📋 Event Schema Reference

### Incoming Events (TO Guild Communications)

#### Chat Events
```json
{
  "action": "SendMessage" | "EditMessage" | "DeleteMessage" | "JoinChannel" | "LeaveChannel" | "CreateChannel" | "InviteUser" | "KickUser" | "MuteUser" | "ReactToMessage",
  "user_id": "uuid",
  "channel_id": "uuid | null",
  "content": "string | null",
  "target_user": "uuid | null",
  "message_id": "uuid | null"
}
```

#### User Registration
```json
{
  "id": "uuid",
  "username": "string",
  "clan_id": "uuid | null",
  "roles": ["uuid"],
  "permissions": ["string"],
  "last_seen": "ISO 8601 datetime",
  "muted_until": "ISO 8601 datetime | null"
}
```

#### Clan Creation
```json
{
  "id": "uuid",
  "name": "string",
  "tag": "string",
  "description": "string",
  "leader_id": "uuid",
  "officers": ["uuid"],
  "members": ["uuid"],
  "channels": ["uuid"],
  "permissions": {
    "can_invite": ["uuid"],
    "can_kick": ["uuid"],
    "can_promote": ["uuid"],
    "can_manage_channels": ["uuid"],
    "can_edit_info": ["uuid"]
  },
  "created_at": "ISO 8601 datetime",
  "level": "number",
  "is_recruiting": "boolean"
}
```

### Outgoing Events (FROM Guild Communications)

#### Message Events
```json
{
  "event_type": "MessageSent" | "UserJoined" | "UserLeft" | "Created",
  "channel_id": "uuid",
  "user_id": "uuid",
  "data": {
    "id": "uuid",
    "sender_id": "uuid",
    "sender_name": "string",
    "content": "string",
    "timestamp": "ISO 8601 datetime",
    "message_type": "Text" | "System" | "Join" | "Leave",
    "reply_to": "uuid | null",
    "edited_at": "ISO 8601 datetime | null",
    "reactions": {}
  }
}
```

#### Permission Requests
```json
{
  "user_id": "uuid",
  "permission": "string",
  "channel_id": "uuid | null",
  "clan_id": "uuid | null"
}
```

## 🔧 Available Message Actions

- `SendMessage`: Send a text message to a channel
- `EditMessage`: Edit an existing message
- `DeleteMessage`: Delete a message
- `JoinChannel`: Join a public or accessible channel
- `LeaveChannel`: Leave a channel
- `CreateChannel`: Create a new channel
- `InviteUser`: Invite a user to a channel
- `KickUser`: Remove a user from a channel
- `MuteUser`: Temporarily mute a user
- `ReactToMessage`: Add emoji reaction to a message

## 🔑 Available Permissions

- `SendMessages`: Basic message sending
- `SendMessagesInChannel(uuid)`: Channel-specific messaging
- `DeleteOwnMessages`: Delete own messages
- `DeleteAnyMessages`: Delete any messages (moderator)
- `ViewChannel(uuid)`: Access specific channel
- `ViewAllChannels`: Access all channels
- `CreateChannels`: Create new channels
- `ManageChannels`: Manage channel settings
- `InviteToChannel`: Invite users to channels
- `KickFromChannel`: Remove users from channels
- `MuteUsers`: Mute users temporarily
- `Administrator`: Full access

## 🧪 Testing Integration

Example test for your plugin integration:

```rust
#[tokio::test]
async fn test_send_message_to_guild() {
    let events = Arc::new(EventSystem::new());
    
    // Send a message
    let message_data = serde_json::json!({
        "action": "SendMessage",
        "user_id": "test-user-id",
        "channel_id": "test-channel-id",
        "content": "Test message"
    });
    
    events.emit_plugin("GuildComms", "Chat", &message_data).await;
    
    // Verify response (implement your verification logic)
}
```