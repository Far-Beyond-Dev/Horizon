//! Bootinator is a plugin used for banning unwanted people

use async_trait::async_trait;
use std::{net::Ipv4Addr, sync::Arc};
use serde::{Deserialize, Serialize};

use sqlx::SqlitePool;
use sqlx::Row;
use std::time::{SystemTime, UNIX_EPOCH};

use horizon_event_system::{
    create_simple_plugin, EventSystem, PluginError, ServerContext, SimplePlugin
};

pub struct BootinatorPlugin {
    name: String,
    db_pool: Option<SqlitePool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    ip: Ipv4Addr,
    reason: String,
    is_banned: bool,
}

struct BanDb;

impl BanDb {
    pub async fn ban_player(pool: &SqlitePool, player_id: &str, ip: Ipv4Addr, reason: &str) -> Result<(), sqlx::Error> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
        sqlx::query(r#"
            INSERT INTO banned_players (player_id, ip, reason, banned_at, is_banned)
            VALUES (?, ?, ?, ?, 1)
        "#)
        .bind(player_id)
        .bind(ip.to_string())
        .bind(reason)
        .bind(now)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn unban_player(pool: &SqlitePool, player_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(r#"
            UPDATE banned_players SET is_banned = 0 WHERE player_id = ?
        "#)
        .bind(player_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn is_banned(pool: &SqlitePool, player_id: &str) -> Result<bool, sqlx::Error> {
        if let Some(row) = sqlx::query("SELECT is_banned FROM banned_players WHERE player_id = ? ORDER BY banned_at DESC LIMIT 1")
            .bind(player_id)
            .fetch_optional(pool)
            .await?
        {
            let is_banned: i64 = row.get(0);
            Ok(is_banned != 0)
        } else {
            Ok(false)
        }
    }
}

impl BootinatorPlugin {
    pub fn new() -> Self {
        Self {
            name: "bootinator".to_string(),
            db_pool: None,
        }
    }
}

impl Default for BootinatorPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SimplePlugin for BootinatorPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    async fn register_handlers(
        &mut self,
        events: Arc<EventSystem>,
        _context: Arc<dyn ServerContext>,
    ) -> Result<(), PluginError> {
        println!("📝 BootinatorPlugin: Registering event handlers...");
        let _ = events;
        // Clone pool for handler use
        let pool = self.db_pool.clone();

        events.on_plugin("plugin_bootinator", "ban", move |event: serde_json::Value| {
            println!("🚫 BootinatorPlugin: Ban event received: {:?}", event);
            // spawn a task to handle DB work if pool is present
            let pool = pool.clone();
            tokio::spawn(async move {
                if let Some(pool) = pool {
                    if let Some(obj) = event.as_object() {
                        let player_id = obj.get("player_id").and_then(|v| v.as_str()).unwrap_or("");
                        let ip = obj.get("ip").and_then(|v| v.as_str()).unwrap_or("0.0.0.0");
                        let reason = obj.get("reason").and_then(|v| v.as_str()).unwrap_or("");
                        // Best-effort parse
                        if let Ok(parsed_ip) = ip.parse::<Ipv4Addr>() {
                            let _ = BanDb::ban_player(&pool, player_id, parsed_ip, reason).await;
                        }
                    }
                }
            });
            Ok(())
        })
        .await
        .expect("Failed to register ban event handler");
        // Unban handler
        let pool = self.db_pool.clone();
        events.on_plugin("plugin_bootinator", "unban", move |event: serde_json::Value| {
            println!("🔓 BootinatorPlugin: Unban event received: {:?}", event);
            let pool = pool.clone();
            tokio::spawn(async move {
                if let Some(pool) = pool {
                    if let Some(obj) = event.as_object() {
                        if let Some(player_id) = obj.get("player_id").and_then(|v| v.as_str()) {
                            let _ = BanDb::unban_player(&pool, player_id).await;
                            println!("Bootinator: unbanned {}", player_id);
                        }
                    }
                }
            });
            Ok(())
        })
        .await
        .expect("Failed to register unban event handler");

        // is_banned handler
        let pool = self.db_pool.clone();
        events.on_plugin("plugin_bootinator", "is_banned", move |event: serde_json::Value| {
            println!("? BootinatorPlugin: is_banned event received: {:?}", event);
            let pool = pool.clone();
            tokio::spawn(async move {
                if let Some(pool) = pool {
                    if let Some(obj) = event.as_object() {
                        if let Some(player_id) = obj.get("player_id").and_then(|v| v.as_str()) {
                            match BanDb::is_banned(&pool, player_id).await {
                                Ok(true) => println!("Bootinator: {} is banned", player_id),
                                Ok(false) => println!("Bootinator: {} is not banned", player_id),
                                Err(e) => eprintln!("Bootinator: failed to query ban status: {}", e),
                            }
                        }
                    }
                }
            });
            Ok(())
        })
        .await
        .expect("Failed to register is_banned event handler");
        Ok(())
    }

    async fn on_init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        println!("⚙️ BootinatorPlugin: Initializing with server context");
        let _context_clone = context.clone();

        // Initialize or open sqlite DB for bans
        // Use a file next to the current working directory; in a fuller system this would come from config
        let database_url = "sqlite://bootinator.db";
        match SqlitePool::connect(database_url).await {
            Ok(pool) => {
                // Create table if not exists
                if let Err(e) = sqlx::query(
                    r#"
                    CREATE TABLE IF NOT EXISTS banned_players (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        player_id TEXT NOT NULL,
                        ip TEXT NOT NULL,
                        reason TEXT,
                        banned_at INTEGER NOT NULL,
                        is_banned INTEGER NOT NULL
                    )
                    "#,
                )
                .execute(&pool)
                .await
                {
                    eprintln!("Bootinator: failed to create banned_players table: {}", e);
                }

                self.db_pool = Some(pool);
            }
            Err(e) => {
                eprintln!("Bootinator: failed to open database: {}", e);
            }
        }
        Ok(())
    }

    async fn on_shutdown(&mut self, _context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        println!("🛑 BootinatorPlugin: Shutting down");
        let _ = _context;
        Ok(())
    }
}

create_simple_plugin!(BootinatorPlugin);