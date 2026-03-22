use chrono::{Utc, DateTime, Duration};
use khronos_runtime::rt::mlua_scheduler::LuaSchedulerAsyncUserData;
use khronos_runtime::rt::mluau::prelude::*;
use khronos_runtime::core::datetime::DateTime as LuaDateTime;
use rand::distr::{Alphanumeric, SampleString};

use crate::{entity::{AnyEntityManager, Entity, EntityFlags, lua::LuaEntityManager}, service::sharedlayer::SharedLayerDb, types::auth::Session};

/// 1 hour expiry time
const LOGIN_EXPIRY_TIME: Duration = Duration::seconds(3600);

/// The response from creating a new session
pub struct CreatedWebSession {
    pub session_id: uuid::Uuid,
    pub token: String,
    pub expires_at: DateTime<Utc>
}

// Implement conversion to Lua value explicitly to allow using DateTime and other
// userdata and ensuring the raw created web session isn't serialized in API etc.
impl IntoLua for CreatedWebSession {
    fn into_lua(self, lua: &Lua) -> Result<LuaValue, LuaError> {
        let table = lua.create_table()?;
        table.set("session_id", self.session_id.to_string())?;
        table.set("token", self.token)?;
        table.set("expires_at", LuaDateTime { dt: self.expires_at.with_timezone(&chrono_tz::UTC) })?;
        table.set_readonly(true);
        Ok(LuaValue::Table(table))
    }
}

/// The response from checking web auth, can be used to control API access
pub enum SessionPermit {
    Success {
        session: Session,
        flags: EntityFlags,
        manager: AnyEntityManager,
    },
    ApiBanned {
        session: Session,
    },
    InvalidToken,
    EntityNotSupported,
}

impl IntoLua for SessionPermit {
    fn into_lua(self, lua: &Lua) -> Result<LuaValue, LuaError> {
        match self {
            SessionPermit::Success { session, flags, manager } => {
                let table = lua.create_table()?;
                table.set("status", "Success")?;
                table.set("session", lua.to_value(&session)?)?;
                table.set("flags", lua.to_value(&flags.bits())?)?;
                table.set("manager", lua.create_userdata(LuaEntityManager::new(manager))?)?;
                Ok(LuaValue::Table(table))
            }
            SessionPermit::ApiBanned { session } => {
                let table = lua.create_table()?;
                table.set("status", "ApiBanned")?;
                table.set("session", lua.to_value(&session)?)?;
                Ok(LuaValue::Table(table))
            }
            SessionPermit::InvalidToken => {
                let table = lua.create_table()?;
                table.set("status", "InvalidToken")?;
                Ok(LuaValue::Table(table))
            }
            SessionPermit::EntityNotSupported => {
                let table = lua.create_table()?;
                table.set("status", "EntityNotSupported")?;
                Ok(LuaValue::Table(table))
            }
        }
    }
}

/// SessionManager provides methods to manage sessions for entities
#[derive(Clone)]
pub struct SessionManager {
    shared_db: SharedLayerDb,
}

#[allow(dead_code)]
impl SessionManager {
    /// Creates a new SessionManager
    #[allow(dead_code)]
    pub(super) fn new(shared_db: SharedLayerDb) -> Self {
        Self { shared_db }
    }

    /// Fetches the session of a entity given token
    pub async fn get_session_by_token(
        &self,
        token: &str,
    ) -> Result<Option<Session>, sqlx::Error> {
        // Delete old/expiring auths first
        sqlx::query("DELETE FROM api_sessions WHERE expiry < NOW()")
            .execute(self.shared_db.pool())
            .await?;

        let session: Option<Session> = sqlx::query_as(
            "SELECT id, name, created_at, type AS session_type, target_type, target_id, expiry 
             FROM api_sessions WHERE token = $1 AND expiry >= NOW()",
        )
        .bind(token)
        .fetch_optional(self.shared_db.pool())
        .await?;

        Ok(session)
    }

    /// Returns the permit for a session given token
    pub async fn get_permit_for(
        &self,
        token: &str,
    ) -> Result<SessionPermit, crate::Error> {
        let sess = self.get_session_by_token(token)
            .await?;

        let Some(auth) = sess else {
            return Ok(SessionPermit::InvalidToken);
        };

        let Some(manager) = self.shared_db.entity_manager_for(&auth.target_type) else {
            return Ok(SessionPermit::EntityNotSupported);
        };

        let flags = manager.entity().flags(&auth.target_id).await?;

        if flags.contains(EntityFlags::BANNED) {
            return Ok(SessionPermit::ApiBanned {
                session: auth,
            });
        }

        // If everything is fine, return the success response
        Ok(SessionPermit::Success {
            session: auth,
            flags,
            manager
        })
    }

    /// Returns the list of all sessions for a user
    pub async fn get_sessions(&self, target_type: &str, target_id: &str) -> Result<Vec<Session>, crate::Error> {
        let sessions: Vec<Session> = sqlx::query_as(
            "SELECT id, name, created_at, type AS session_type, target_type, target_id, expiry 
             FROM api_sessions WHERE target_type = $1 AND target_id = $2",
        )
        .bind(target_type)
        .bind(target_id)
        .fetch_all(self.shared_db.pool())
        .await?;

        Ok(sessions)
    }

    /// Create a new login session
    pub async fn create_login_session(
        &self,
        target_type: &str,
        target_id: &str,
    ) -> Result<CreatedWebSession, crate::Error> {
        self.create_session(target_type, target_id, None, "login", Utc::now() + LOGIN_EXPIRY_TIME).await
    }

    /// Create a new API session
    pub async fn create_api_session(
        &self,
        target_type: &str,
        target_id: &str,
        name: Option<String>,
        expires_at: DateTime<Utc>,
    ) -> Result<CreatedWebSession, crate::Error> {
        self.create_session(target_type, target_id, name, "api", expires_at).await
    }

    /// Internal method to create a session
    #[inline(always)]
    async fn create_session(
        &self,
        target_type: &str,
        target_id: &str,
        name: Option<String>,
        session_type: &str,
        expiry: DateTime<Utc>,
    ) -> Result<CreatedWebSession, crate::Error> {
        // Generate a new session ID
        #[derive(sqlx::FromRow)]
        struct NewSession {
            #[sqlx(rename = "id")]
            session_id: uuid::Uuid,
        }

        let token = Alphanumeric.sample_string(&mut rand::rng(), 128);

        let new_session: NewSession = sqlx::query_as(
            "INSERT INTO api_sessions (target_type, target_id, type, token, expiry, name) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        )
        .bind(target_type)
        .bind(target_id)
        .bind(session_type)
        .bind(&token)
        .bind(expiry)
        .bind(name)
        .fetch_one(self.shared_db.pool())
        .await?;

        Ok(CreatedWebSession { 
            session_id: new_session.session_id,
            token,
            expires_at: expiry,
        })
    }

    /// Deletes a session by ID for the given entity
    pub async fn delete_session(&self, target_type: &str, target_id: &str, session_id: uuid::Uuid) -> Result<(), crate::Error> {        
        let res = sqlx::query("DELETE FROM api_sessions WHERE target_type = $1 AND target_id = $2 AND id = $3")
            .bind(target_type)
            .bind(target_id)
            .bind(session_id)
            .execute(self.shared_db.pool())
            .await?;

        if res.rows_affected() == 0 {
            return Err("No session found to delete".into());
        }

        Ok(())
    }

    pub async fn delete_all_sessions(
        &self,
        target_type: &str,
        target_id: &str,
    ) -> Result<(), crate::Error> {
        sqlx::query("DELETE FROM api_sessions WHERE target_type = $1 AND target_id = $2")
            .bind(target_type)
            .bind(target_id)
            .execute(self.shared_db.pool())
            .await?;

        Ok(())
    }
}

impl LuaUserData for SessionManager {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_scheduler_async_method(
            "GetSessionByToken",
            |lua, this, token: String| async move {
                let session = this
                    .get_session_by_token(&token)
                    .await
                    .map_err(LuaError::external)?;
                lua.to_value(&session)
            },
        );

        methods.add_scheduler_async_method("GetPermitFor", async |_lua, this, token: String| {
            let permit = this
                .get_permit_for(&token)
                .await
                .map_err(|e| LuaError::external(e.to_string()))?;
            Ok(permit)
        });

        methods.add_scheduler_async_method("CreateLoginSession", async |_lua, this, (target_type, target_id): (String, String)| {
            let res = this.create_login_session(&target_type, &target_id).await
            .map_err(|e| LuaError::external(e.to_string()))?;

            Ok(res)
        });

        methods.add_scheduler_async_method("CreateApiSession", async |_lua, this, (target_type, target_id, name, expires_at): (String, String, Option<String>, LuaUserDataRef<LuaDateTime<chrono_tz::Tz>>)| {
            let res = this.create_api_session(&target_type, &target_id, name, expires_at.dt.with_timezone(&Utc)).await
            .map_err(|e| LuaError::external(e.to_string()))?;

            Ok(res)
        });

        methods.add_scheduler_async_method("DeleteSession", async |_lua, this, (target_type, target_id, session_id): (String, String, String)| {
            let session_uuid = match uuid::Uuid::parse_str(&session_id) {
                Ok(uuid) => uuid,
                Err(_) => return Err(LuaError::external("Invalid session ID format")),
            };

            let res = this.delete_session(&target_type, &target_id, session_uuid).await;
            match res {
                Ok(()) => Ok(()),
                Err(e) => Err(LuaError::external(e.to_string())),
            }
        });

        methods.add_scheduler_async_method("DeleteAllSessions", async |_lua, this, (target_type, target_id): (String, String)| {
            let res = this.delete_all_sessions(&target_type, &target_id).await;
            match res {
                Ok(()) => Ok(()),
                Err(e) => Err(LuaError::external(e.to_string())),
            }
        });
    }
}