// types.rs - Type definitions for the guild communication system
pub use chrono::{DateTime, Utc};
pub use serde::{Deserialize, Serialize};
pub use std::collections::HashMap;
pub use uuid::Uuid;
pub use async_trait::async_trait;
pub use std::sync::Arc;
pub use event_system::{SimplePlugin, EventSystem, PluginError, create_simple_plugin, ServerContext};

// Core chat system types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildSystem {
    pub clans: Option<HashMap<Uuid, ClanSystem>>,
    pub chat: Option<ChatManager>,
    pub roles: Option<HashMap<Uuid, Roles>>,
    pub channels: Option<HashMap<Uuid, Channel>>,
    pub users: Option<HashMap<Uuid, GuildUser>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildUser {
    pub id: Uuid,
    pub username: String,
    pub clan_id: Option<Uuid>,
    pub roles: Vec<Uuid>,
    pub permissions: Vec<Permission>,
    pub last_seen: DateTime<Utc>,
    pub muted_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatManager {
    pub public_channels: Vec<Uuid>,
    pub private_channels: Vec<Uuid>,
    pub direct_messages: HashMap<(Uuid, Uuid), Uuid>, // (user1, user2) -> channel_id
    pub active_conversations: HashMap<Uuid, DateTime<Utc>>, // channel_id -> last_activity
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub id: Uuid,
    pub name: String,
    pub channel_type: ChannelType,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub created_by: Uuid,
    pub members: Vec<Uuid>,
    pub permissions: ChannelPermissions,
    pub message_history: Vec<ChatMessage>,
    pub max_messages: usize,
    pub is_archived: bool,
    pub clan_id: Option<Uuid>, // For clan-specific channels
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChannelType {
    Public,           // Everyone can see and join
    Private,          // Invite only
    ClanGeneral,      // Clan members only
    ClanOfficers,     // Clan officers only
    DirectMessage,    // 1-on-1 chat
    Announcement,     // Read-only for most users
    System,           // System notifications
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelPermissions {
    pub can_read: Vec<Permission>,
    pub can_write: Vec<Permission>,
    pub can_invite: Vec<Permission>,
    pub can_kick: Vec<Permission>,
    pub can_manage: Vec<Permission>,
    pub required_roles: Vec<Uuid>,
    pub banned_users: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: Uuid,
    pub sender_id: Uuid,
    pub sender_name: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub message_type: MessageType,
    pub reply_to: Option<Uuid>,
    pub edited_at: Option<DateTime<Utc>>,
    pub reactions: HashMap<String, Vec<Uuid>>, // emoji -> user_ids
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    Text,
    System,
    Join,
    Leave,
    Kick,
    Promotion,
    Announcement,
    Command,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClanSystem {
    pub id: Uuid,
    pub name: String,
    pub tag: String,
    pub description: String,
    pub leader_id: Uuid,
    pub officers: Vec<Uuid>,
    pub members: Vec<Uuid>,
    pub channels: Vec<Uuid>, // Clan-specific channels
    pub permissions: ClanPermissions,
    pub created_at: DateTime<Utc>,
    pub level: u32,
    pub is_recruiting: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClanPermissions {
    pub can_invite: Vec<Uuid>,
    pub can_kick: Vec<Uuid>,
    pub can_promote: Vec<Uuid>,
    pub can_manage_channels: Vec<Uuid>,
    pub can_edit_info: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Roles {
    pub id: Uuid,
    pub name: String,
    pub color: String,
    pub permissions: Vec<Permission>,
    pub priority: u32, // Higher = more important
    pub is_mentionable: bool,
    pub is_hoisted: bool, // Shows separately in member list
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Permission {
    // Chat permissions
    SendMessages,
    SendMessagesInChannel(Uuid),
    DeleteOwnMessages,
    DeleteAnyMessages,
    EditMessages,
    UseEmojis,
    UseReactions,
    MentionEveryone,
    
    // Channel permissions
    ViewChannel(Uuid),
    ViewAllChannels,
    CreateChannels,
    ManageChannels,
    DeleteChannels,
    InviteToChannel,
    KickFromChannel,
    
    // Clan permissions
    ViewClan,
    InviteToClan,
    KickFromClan,
    PromoteMembers,
    ManageClan,
    
    // Moderation permissions
    MuteUsers,
    BanUsers,
    ViewAuditLog,
    ManageRoles,
    
    // System permissions
    Administrator,
}

// Event types for plugin communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSystemRequest {
    pub action: MessageAction,
    pub user_id: Uuid,
    pub channel_id: Option<Uuid>,
    pub content: Option<String>,
    pub target_user: Option<Uuid>,
    pub message_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageAction {
    SendMessage,
    EditMessage,
    DeleteMessage,
    JoinChannel,
    LeaveChannel,
    CreateChannel,
    InviteUser,
    KickUser,
    MuteUser,
    ReactToMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub user_id: Uuid,
    pub permission: Permission,
    pub channel_id: Option<Uuid>,
    pub clan_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionResponse {
    pub granted: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelEvent {
    pub event_type: ChannelEventType,
    pub channel_id: Uuid,
    pub user_id: Uuid,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChannelEventType {
    Created,
    Deleted,
    UserJoined,
    UserLeft,
    UserKicked,
    PermissionsChanged,
    MessageSent,
    MessageEdited,
    MessageDeleted,
}