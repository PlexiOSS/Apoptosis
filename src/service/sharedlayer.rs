use crate::Db;
use crate::entity::EntityType;
use crate::entity::manager::EntityManager;
use crate::service::session::SessionManager;

use super::cacheserver::CacheServerManager;
use super::kittycat as srv_kittycat;
use super::optional_value::OptionalValue;
use khronos_runtime::rt::mlua_scheduler::LuaSchedulerAsyncUserData;
use khronos_runtime::rt::mluau::prelude::*;
use sqlx::Row;
use sqlx::types::Uuid;
use std::rc::Rc;

/// Internally needed so other parts of SharedLayer can access the database
/// and entity manager creation related methods
#[derive(Clone)]
pub(super) struct SharedLayerDb {
    pool: sqlx::PgPool,
    diesel: Db,
}

#[allow(dead_code)]
impl SharedLayerDb {
    fn new(pool: sqlx::PgPool, diesel: Db) -> Self {
        Self { pool, diesel }
    }

    /// Returns the underlying sqlx Postgres pool
    pub(super) fn pool(&self) -> &sqlx::PgPool {
        &self.pool
    }

    /// Returns the underlying Diesel database connection
    pub(super) fn diesel(&self) -> &Db {
        &self.diesel
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
        let Some(manager) = EntityType::from_name(target_type, self.pool.clone(), self.diesel.clone()) else {
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
    cache_server_manager: CacheServerManager,
    session_manager: SessionManager,
}

#[allow(dead_code)]
impl SharedLayer {
    /// Creates a new SharedLayer
    ///
    /// Should be called once per layer
    #[allow(dead_code)]
    pub fn new(pool: sqlx::PgPool, diesel: Db) -> Self {
        let db = SharedLayerDb::new(pool.clone(), diesel.clone());
        Self {
            cache_server_manager: CacheServerManager::new(pool.clone()),
            session_manager: SessionManager::new(db.clone()),
            db,
        }
    }

    /// Returns the underlying cache server manager
    pub fn cache_server_manager(&self) -> &CacheServerManager {
        &self.cache_server_manager
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

impl IntoLua for SharedLayer {
    fn into_lua(self, lua: &Lua) -> LuaResult<LuaValue> {
        let lua_shared_layer = LuaSharedLayer::new(self);
        let ud = lua.create_userdata(lua_shared_layer)?;
        Ok(LuaValue::UserData(ud))
    }
}

/// Non-thread safe LuaUserData wrapper around SharedLayer
#[derive(Clone)]
pub struct LuaSharedLayer {
    shared: SharedLayer,

    // Cache any computed fields here
    cache_server_manager_cache: Rc<OptionalValue<LuaAnyUserData>>,
    session_manager_cache: Rc<OptionalValue<LuaAnyUserData>>,
    shared_layer_ud: Rc<OptionalValue<LuaAnyUserData>>,
}

impl LuaSharedLayer {
    pub fn new(shared: SharedLayer) -> Self {
        Self {
            shared,
            cache_server_manager_cache: Rc::new(OptionalValue::new()),
            session_manager_cache: Rc::new(OptionalValue::new()),
            shared_layer_ud: Rc::new(OptionalValue::new()),
        }
    }
}

impl std::ops::Deref for LuaSharedLayer {
    type Target = SharedLayer;

    fn deref(&self) -> &Self::Target {
        &self.shared
    }
}

impl LuaUserData for LuaSharedLayer {
    fn add_fields<F: LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("CacheServerManager", |lua, this| {
            this.cache_server_manager_cache
                .get_failable(|| lua.create_any_userdata(this.cache_server_manager.clone()))
        });

        fields.add_field_method_get("SessionManager", |lua, this| {
            this.session_manager_cache
                .get_failable(|| lua.create_any_userdata(this.session_manager.clone()))
        });
    }

    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_scheduler_async_method(
            "GetUserStaffPerms",
            |_lua, this, userid: String| async move {
                let perms = this
                    .get_user_staff_perms(userid)
                    .await
                    .map_err(LuaError::external)?;
                let lua_perms = srv_kittycat::StaffPermissions::from(perms);
                Ok(lua_perms)
            },
        );

        methods.add_scheduler_async_method("GetBotState", |_lua, this, botid: String| async move {
            let state = this
                .get_bot_state(botid)
                .await
                .map_err(LuaError::external)?;
            Ok(state)
        });
    }
}
