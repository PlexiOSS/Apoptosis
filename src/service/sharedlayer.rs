use crate::entity::EntityType;
use crate::entity::manager::EntityManager;
use crate::service::session::SessionManager;

use sqlx::Row;
use sqlx::types::Uuid;

/// Internally needed so other parts of SharedLayer can access the database
/// and entity manager creation related methods
#[derive(Clone)]
pub(super) struct SharedLayerDb {
    pool: sqlx::PgPool,
}

#[allow(dead_code)]
impl SharedLayerDb {
    fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Returns the underlying sqlx Postgres pool
    pub(super) fn pool(&self) -> &sqlx::PgPool {
        &self.pool
    }

    /// Returns the state of a bot by its user ID on Omni/IBL
    ///
    /// Returns None if the bot is not found
    pub async fn get_bot_state(&self, botid: String) -> Result<Option<String>, sqlx::Error> {
        let row = sqlx::query("SELECT type FROM bots WHERE bot_id = $1")
            .bind(botid)
            .fetch_optional(&self.pool)
            .await?;

        if let Some(row) = row {
            let state: String = row.try_get("type")?;
            Ok(Some(state))
        } else {
            Ok(None)
        }
    }

    /// Returns the user's staff permissions on Omni/IBL
    pub async fn get_user_staff_perms(
        &self,
        userid: String,
    ) -> Result<kittycat::perms::StaffPermissions, sqlx::Error> {
        let row =
            sqlx::query("SELECT positions, perm_overrides FROM staff_members WHERE user_id = $1")
                .bind(userid)
                .fetch_optional(&self.pool)
                .await?;

        let Some(row) = row else {
            return Ok(kittycat::perms::StaffPermissions {
                user_positions: vec![],
                perm_overrides: vec![],
            });
        };

        let positions: Vec<Uuid> = row.try_get("positions")?;
        let perm_overrides: Vec<String> = row.try_get("perm_overrides")?;

        let position_data =
            sqlx::query("SELECT id::text, index, perms FROM staff_positions WHERE id = ANY($1)")
                .bind(&positions)
                .fetch_all(&self.pool)
                .await?;

        let mut positions = Vec::with_capacity(position_data.len());

        for r in position_data {
            positions.push(kittycat::perms::PartialStaffPosition {
                id: r.try_get("id")?,
                index: r.try_get("index")?,
                perms: r
                    .try_get::<Vec<String>, _>("perms")?
                    .into_iter()
                    .map(|x| x.into())
                    .collect(),
            });
        }

        let sp = kittycat::perms::StaffPermissions {
            user_positions: positions,
            perm_overrides: perm_overrides.into_iter().map(|x| x.into()).collect(),
        };

        Ok(sp)
    }

    /// Creates a new EntityManager for the given entity type
    pub fn entity_manager_for(&self, target_type: &str) -> Option<crate::entity::AnyEntityManager> {
        let Some(manager) = EntityType::from_name(target_type, self.pool.clone()) else {
            return None;
        };
        Some(EntityManager::new(manager))
    }
}

/// SharedLayer provides common methods across IBL's entire backend
/// to both
///
/// Ideally, every IBL apoptosis layer will have its own SharedLayer
#[derive(Clone)]
pub struct SharedLayer {
    db: SharedLayerDb,
    session_manager: SessionManager,
}

#[allow(dead_code)]
impl SharedLayer {
    /// Creates a new SharedLayer
    ///
    /// Should be called once per layer
    #[allow(dead_code)]
    pub fn new(pool: sqlx::PgPool) -> Self {
        let db = SharedLayerDb::new(pool.clone());
        Self {
            session_manager: SessionManager::new(db.clone()),
            db,
        }
    }

    /// Returns the underlying session manager
    pub fn session_manager(&self) -> &SessionManager {
        &self.session_manager
    }

    /// Returns the state of a bot by its user ID on Omni/IBL
    ///
    /// Returns None if the bot is not found
    pub async fn get_bot_state(&self, botid: String) -> Result<Option<String>, sqlx::Error> {
        self.db.get_bot_state(botid).await
    }

    /// Returns the user's staff permissions on Omni/IBL
    pub async fn get_user_staff_perms(
        &self,
        userid: String,
    ) -> Result<kittycat::perms::StaffPermissions, sqlx::Error> {
        self.db.get_user_staff_perms(userid).await
    }

    /// Creates a new EntityManager for the given entity type
    pub fn entity_manager_for(&self, target_type: &str) -> Option<crate::entity::AnyEntityManager> {
        self.db.entity_manager_for(target_type)
    }
}
