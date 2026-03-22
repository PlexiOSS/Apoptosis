use std::sync::Arc;

use serenity::all::UserId;
use sqlx::PgPool;
use serenity::model::user::OnlineStatus;
use chrono::{DateTime, Utc};
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

    /// Fetch user and store banner if needed
    pub async fn user(&self, _user_id: &str) -> Result<serde_json::Value, crate::Error> {
        match self {
            DovewingSource::Discord(_c) => {
                todo!("Support get guild ids in stratum");
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
