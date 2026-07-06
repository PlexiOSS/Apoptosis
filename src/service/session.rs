use chrono::{Utc, DateTime, Duration};
use rand::distr::{Alphanumeric, SampleString};

use crate::{entity::{AnyEntityManager, Entity, EntityFlags}, service::sharedlayer::SharedLayerDb, types::auth::Session};

/// 1 hour expiry time
const LOGIN_EXPIRY_TIME: Duration = Duration::seconds(3600);

/// The response from creating a new session
pub struct CreatedWebSession {
    pub session_id: uuid::Uuid,
    pub token: String,
    pub expires_at: DateTime<Utc>
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
            "SELECT a.id as id, a.name as name, a.created_at as created_at, a.type AS session_type, a.target_type as target_type, a.target_id as target_id, 
             a.expiry as expiry, ke.keid as keid
             FROM api_sessions a 
             INNER JOIN known_entities ke ON ke.target_type = a.target_type AND ke.target_id = a.target_id 
             WHERE a.token = $1 AND a.expiry >= NOW()
            ",
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
            "SELECT a.id as id, a.name as name, a.created_at as created_at, a.type AS session_type, a.target_type as target_type, a.target_id as target_id, 
            a.expiry as expiry, ke.keid as keid
            FROM api_sessions a
            INNER JOIN known_entities ke ON ke.target_type = a.target_type AND ke.target_id = a.target_id
            WHERE a.target_type = $1 AND a.target_id = $2 AND a.expiry >= NOW()
            ",
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
