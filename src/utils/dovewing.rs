use std::sync::Arc;

use serenity::all::UserId;
use sqlx::PgPool;
use serenity::model::user::OnlineStatus;
use chrono::{DateTime, Utc};
use stratum_client::GetResourceRequest;
use super::stratum::Stratum;
use crate::types::dovewing::PlatformUser;

#[derive(Clone)]
pub enum DovewingSource {
    Discord(Stratum),
}

impl DovewingSource {
    /// Returns the expiry time of a user
    pub fn user_expiry_time(&self) -> i64 {
        match self {
            // 8 hours
            DovewingSource::Discord(_) => 8 * 60 * 60,
        }
    }

    /// Fetch member if possible
    pub async fn member(&self, user_id: &str) -> Result<Option<serde_json::Value>, crate::Error> {
        match self {
            DovewingSource::Discord(s) => {
                let Some(guild_ids) = s.get_parsed_resource_from_cache::<Vec<String>>(GetResourceRequest::GuildIds).await? else {
                    return Err("internal error: stratum returned no guild ids".into())
                };
                let user_id = user_id.parse()?;
                for guild_id in guild_ids {
                    let guild_id = guild_id.parse()?;
                    if let Some(cached_user) = s.get_resource_from_cache(GetResourceRequest::GuildMember { guild_id, user_id }).await? {
                        return Ok(Some(cached_user))
                    }
                }

                return Ok(None)
            }
        }
    }
}

#[derive(Clone)]
pub struct Dovewing {
    pool: PgPool,
    src: DovewingSource,
}

impl Dovewing {
    pub async fn get_platform_user(
        &self,
        _user_id: &str,
    ) -> Result<PlatformUser, crate::Error> {
        todo!("Implement this");
    }
}
