mod types;
use types::*;

impl GuildSystem {
    pub fn new() -> Self {
        println!("📝 GuildPlugin: Initializing comprehensive chat management system...");

        Self {
            clans: Some(HashMap::new()),
            chat: Some(ChatManager {
                public_channels: Vec::new(),
                private_channels: Vec::new(),
                direct_messages: HashMap::new(),
                active_conversations: HashMap::new(),
            }),
            roles: Some(HashMap::new()),
            channels: Some(HashMap::new()),
            users: Some(HashMap::new()),
        }
    }

    // Chat system methods
    pub async fn send_message(
        &mut self,
        user_id: Uuid,
        channel_id: Uuid,
        content: String,
        events: Arc<EventSystem>,
    ) -> Result<(), String> {
        // Check permissions first
        let permission_request = PermissionRequest {
            user_id,
            permission: Permission::SendMessagesInChannel(channel_id),
            channel_id: Some(channel_id),
            clan_id: None,
        };

        let permission_result = self.check_permission_via_plugin(permission_request, events.clone()).await?;
        if !permission_result.granted {
            return Err(permission_result.reason.unwrap_or("Permission denied".to_string()));
        }

        // Get user and channel
        let user = self.users.as_ref()
            .and_then(|users| users.get(&user_id))
            .ok_or("User not found")?;

        let channels = self.channels.as_mut().ok_or("Channels not initialized")?;
        let channel = channels.get_mut(&channel_id).ok_or("Channel not found")?;

        // Check if user is muted
        if let Some(muted_until) = user.muted_until {
            if muted_until > Utc::now() {
                return Err("User is muted".to_string());
            }
        }

        // Create message
        let message = ChatMessage {
            id: Uuid::new_v4(),
            sender_id: user_id,
            sender_name: user.username.clone(),
            content,
            timestamp: Utc::now(),
            message_type: MessageType::Text,
            reply_to: None,
            edited_at: None,
            reactions: HashMap::new(),
        };

        // Add to channel history
        channel.message_history.push(message.clone());
        
        // Trim history if needed
        if channel.message_history.len() > channel.max_messages {
            channel.message_history.remove(0);
        }

        // Update last activity
        if let Some(chat) = self.chat.as_mut() {
            chat.active_conversations.insert(channel_id, Utc::now());
        }

        // Emit message event
        let channel_event = ChannelEvent {
            event_type: ChannelEventType::MessageSent,
            channel_id,
            user_id,
            data: Some(serde_json::to_value(&message).unwrap()),
        };

        events.emit_plugin("GuildComms", "MessageSent", &serde_json::to_value(&channel_event).unwrap()).await;

        Ok(())
    }

    pub async fn create_channel(
        &mut self,
        creator_id: Uuid,
        name: String,
        channel_type: ChannelType,
        description: Option<String>,
        clan_id: Option<Uuid>,
        events: Arc<EventSystem>,
    ) -> Result<Uuid, String> {
        // Check permissions
        let permission_request = PermissionRequest {
            user_id: creator_id,
            permission: Permission::CreateChannels,
            channel_id: None,
            clan_id,
        };

        let permission_result = self.check_permission_via_plugin(permission_request, events.clone()).await?;
        if !permission_result.granted {
            return Err(permission_result.reason.unwrap_or("Permission denied".to_string()));
        }

        let channel_id = Uuid::new_v4();
        let channel = Channel {
            id: channel_id,
            name: name.clone(),
            channel_type: channel_type.clone(),
            description,
            created_at: Utc::now(),
            created_by: creator_id,
            members: vec![creator_id],
            permissions: ChannelPermissions {
                can_read: vec![Permission::ViewChannel(channel_id)],
                can_write: vec![Permission::SendMessagesInChannel(channel_id)],
                can_invite: vec![Permission::InviteToChannel],
                can_kick: vec![Permission::KickFromChannel],
                can_manage: vec![Permission::ManageChannels],
                required_roles: Vec::new(),
                banned_users: Vec::new(),
            },
            message_history: Vec::new(),
            max_messages: 1000,
            is_archived: false,
            clan_id,
        };

        // Add to channels
        if let Some(channels) = self.channels.as_mut() {
            channels.insert(channel_id, channel);
        }

        // Update chat manager
        if let Some(chat) = self.chat.as_mut() {
            match channel_type {
                ChannelType::Public => chat.public_channels.push(channel_id),
                _ => chat.private_channels.push(channel_id),
            }
        }

        // If clan channel, add to clan
        if let Some(clan_id) = clan_id {
            if let Some(clans) = self.clans.as_mut() {
                if let Some(clan) = clans.get_mut(&clan_id) {
                    clan.channels.push(channel_id);
                }
            }
        }

        // Emit channel created event
        let channel_event = ChannelEvent {
            event_type: ChannelEventType::Created,
            channel_id,
            user_id: creator_id,
            data: Some(serde_json::json!({
                "name": name,
                "channel_type": channel_type,
                "clan_id": clan_id
            })),
        };

        events.emit_plugin("GuildComms", "ChannelCreated", &serde_json::to_value(&channel_event).unwrap()).await;

        Ok(channel_id)
    }

    pub async fn join_channel(
        &mut self,
        user_id: Uuid,
        channel_id: Uuid,
        events: Arc<EventSystem>,
    ) -> Result<(), String> {
        // Check permissions
        let permission_request = PermissionRequest {
            user_id,
            permission: Permission::ViewChannel(channel_id),
            channel_id: Some(channel_id),
            clan_id: None,
        };

        let permission_result = self.check_permission_via_plugin(permission_request, events.clone()).await?;
        if !permission_result.granted {
            return Err(permission_result.reason.unwrap_or("Permission denied".to_string()));
        }

        let channels = self.channels.as_mut().ok_or("Channels not initialized")?;
        let channel = channels.get_mut(&channel_id).ok_or("Channel not found")?;

        // Check if user is banned
        if channel.permissions.banned_users.contains(&user_id) {
            return Err("User is banned from this channel".to_string());
        }

        // Add user to channel if not already member
        if !channel.members.contains(&user_id) {
            channel.members.push(user_id);

            // Add system message
            let user = self.users.as_ref()
                .and_then(|users| users.get(&user_id))
                .ok_or("User not found")?;

            let join_message = ChatMessage {
                id: Uuid::new_v4(),
                sender_id: user_id,
                sender_name: user.username.clone(),
                content: format!("{} joined the channel", user.username),
                timestamp: Utc::now(),
                message_type: MessageType::Join,
                reply_to: None,
                edited_at: None,
                reactions: HashMap::new(),
            };

            channel.message_history.push(join_message);

            // Emit join event
            let channel_event = ChannelEvent {
                event_type: ChannelEventType::UserJoined,
                channel_id,
                user_id,
                data: None,
            };

            events.emit_plugin("GuildComms", "UserJoinedChannel", &serde_json::to_value(&channel_event).unwrap()).await;
        }

        Ok(())
    }

    pub async fn create_clan_channels(&mut self, clan_id: Uuid, events: Arc<EventSystem>) -> Result<(), String> {
        let clan = self.clans.as_ref()
            .and_then(|clans| clans.get(&clan_id))
            .ok_or("Clan not found")?
            .clone();

        // Create general clan channel
        let general_id = self.create_channel(
            clan.leader_id,
            format!("{}-general", clan.tag),
            ChannelType::ClanGeneral,
            Some("General clan discussion".to_string()),
            Some(clan_id),
            events.clone(),
        ).await?;

        // Create officers channel
        let officers_id = self.create_channel(
            clan.leader_id,
            format!("{}-officers", clan.tag),
            ChannelType::ClanOfficers,
            Some("Officers only discussion".to_string()),
            Some(clan_id),
            events.clone(),
        ).await?;

        // Set special permissions for officers channel
        if let Some(channels) = self.channels.as_mut() {
            if let Some(officers_channel) = channels.get_mut(&officers_id) {
                officers_channel.members = clan.officers.clone();
                officers_channel.members.push(clan.leader_id);
            }
        }

        Ok(())
    }

    async fn check_permission_via_plugin(
        &self,
        request: PermissionRequest,
        events: Arc<EventSystem>,
    ) -> Result<PermissionResponse, String> {
        // Emit permission check request to external permissions plugin
        events.emit_plugin("Permissions", "CheckPermission", &serde_json::to_value(&request).unwrap()).await;
        
        // In a real implementation, you'd wait for a response
        // For now, we'll do a basic check
        Ok(PermissionResponse {
            granted: true, // Default allow for demo
            reason: None,
        })
    }

    pub fn get_user_channels(&self, user_id: Uuid) -> Vec<&Channel> {
        self.channels
            .as_ref()
            .map(|channels| {
                channels
                    .values()
                    .filter(|channel| channel.members.contains(&user_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn get_clan_channels(&self, clan_id: Uuid) -> Vec<&Channel> {
        self.channels
            .as_ref()
            .map(|channels| {
                channels
                    .values()
                    .filter(|channel| channel.clan_id == Some(clan_id))
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[async_trait]
impl SimplePlugin for GuildSystem {
    fn name(&self) -> &str {
        "guild_comms"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    async fn register_handlers(&mut self, events: Arc<EventSystem>) -> Result<(), PluginError> {
        // Chat message handler
        {
            let events_clone = events.clone();
            events
                .on_plugin(
                    "GuildComms",
                    "Chat",
                    move |json_event: serde_json::Value| {
                        let event: MessageSystemRequest =
                            serde_json::from_value(json_event).expect("Failed to read json");

                        println!("📝 Chat message received: {:?}", event);

                        // Handle different message actions
                        match event.action {
                            MessageAction::SendMessage => {
                                if let (Some(channel_id), Some(content)) = (event.channel_id, event.content) {
                                    println!("💬 User {} sending message to channel {}: {}", 
                                        event.user_id, channel_id, content);
                                }
                            }
                            MessageAction::JoinChannel => {
                                if let Some(channel_id) = event.channel_id {
                                    println!("👋 User {} joining channel {}", event.user_id, channel_id);
                                }
                            }
                            MessageAction::CreateChannel => {
                                println!("🆕 User {} creating new channel", event.user_id);
                            }
                            _ => {}
                        }

                        Ok(())
                    },
                )
                .await
                .unwrap();
        }

        // Clan system handler
        {
            events
                .on_plugin(
                    "GuildComms",
                    "Clan",
                    move |json_event: serde_json::Value| {
                        let event: ClanSystem =
                            serde_json::from_value(json_event).expect("Failed to read json");

                        println!("🏰 New clan created: {} [{}]", event.name, event.tag);

                        Ok(())
                    },
                )
                .await
                .unwrap();
        }

        // Role system handler
        {
            events
                .on_plugin(
                    "GuildComms",
                    "Role",
                    move |json_event: serde_json::Value| {
                        let event: Roles =
                            serde_json::from_value(json_event).expect("Failed to read json");

                        println!("👑 New role created: {} with {} permissions", 
                            event.name, event.permissions.len());

                        Ok(())
                    },
                )
                .await
                .unwrap();
        }

        // Channel events handler
        {
            events
                .on_plugin(
                    "GuildComms",
                    "Channel",
                    move |json_event: serde_json::Value| {
                        let event: ChannelEvent =
                            serde_json::from_value(json_event).expect("Failed to read json");

                        match event.event_type {
                            ChannelEventType::Created => {
                                println!("📺 New channel created: {}", event.channel_id);
                            }
                            ChannelEventType::UserJoined => {
                                println!("👋 User {} joined channel {}", event.user_id, event.channel_id);
                            }
                            ChannelEventType::MessageSent => {
                                println!("💬 Message sent in channel {}", event.channel_id);
                            }
                            _ => {}
                        }

                        Ok(())
                    },
                )
                .await
                .unwrap();
        }

        // Permission response handler (from external permissions plugin)
        // TODO - @tristanpoland This should be replaced with an improved version for the actual permission plugin system that prevents conflicts
        //        to do this we will need the central server to pass each plugin a UUID for itself so we can verify each plugin has a unique ID
        //        with which to sign its private events
        {
            events
                .on_plugin(
                    "Permissions",
                    "PermissionResponse",
                    move |json_event: serde_json::Value| {
                        let response: PermissionResponse =
                            serde_json::from_value(json_event).expect("Failed to read json");

                        println!("🔐 Permission check result: granted={}", response.granted);

                        Ok(())
                    },
                )
                .await
                .unwrap();
        }

        // User events handler
        {
            events
                .on_plugin(
                    "GuildComms",
                    "User",
                    move |json_event: serde_json::Value| {
                        let event: GuildUser =
                            serde_json::from_value(json_event).expect("Failed to read json");

                        println!("👤 User event: {} ({})", event.username, event.id);

                        Ok(())
                    },
                )
                .await
                .unwrap();
        }

        println!("✅ GuildComms plugin handlers registered successfully!");
        Ok(())
    }
}

create_simple_plugin!(GuildSystem);