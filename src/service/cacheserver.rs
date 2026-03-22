use khronos_runtime::rt::mlua_scheduler::LuaSchedulerAsyncUserData;
use khronos_runtime::rt::mluau::prelude::*;
use sqlx::Row;

/// CacheServerManager provides methods to manage the cache servers for bots
#[derive(Clone)]
pub struct CacheServerManager {
    pool: sqlx::PgPool,
}

pub struct CacheServerInfo {
    pub bots_role: String,
    pub system_bots_role: String,
    pub logs_channel: String,
    pub staff_role: String,
    pub web_moderator_role: String,
    pub name: String,
    pub invite_code: String,
    pub welcome_channel: String,
}

impl IntoLua for CacheServerInfo {
    fn into_lua(self, lua: &Lua) -> Result<LuaValue, LuaError> {
        let table = lua.create_table()?;
        table.set("bots_role", self.bots_role)?;
        table.set("system_bots_role", self.system_bots_role)?;
        table.set("logs_channel", self.logs_channel)?;
        table.set("staff_role", self.staff_role)?;
        table.set("web_moderator_role", self.web_moderator_role)?;
        table.set("name", self.name)?;
        table.set("invite_code", self.invite_code)?;
        table.set("welcome_channel", self.welcome_channel)?;
        table.set_readonly(true);
        Ok(LuaValue::Table(table))
    }
}

impl CacheServerManager {
    /// Creates a new CacheServerManager
    #[allow(dead_code)]
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Returns information about a cache server
    ///
    /// Returns None if the cache server is not found
    pub async fn get(&self, guildid: String) -> Result<Option<CacheServerInfo>, sqlx::Error> {
        let row = sqlx::query("SELECT bots_role, system_bots_role, logs_channel, staff_role, web_moderator_role, name, invite_code, welcome_channel from cache_servers WHERE guild_id = $1")
            .bind(guildid)
            .fetch_optional(&self.pool)
            .await?;

        if let Some(row) = row {
            let info = CacheServerInfo {
                bots_role: row.try_get("bots_role")?,
                system_bots_role: row.try_get("system_bots_role")?,
                logs_channel: row.try_get("logs_channel")?,
                staff_role: row.try_get("staff_role")?,
                web_moderator_role: row.try_get("web_moderator_role")?,
                name: row.try_get("name")?,
                invite_code: row.try_get("invite_code")?,
                welcome_channel: row.try_get("welcome_channel")?,
            };
            Ok(Some(info))
        } else {
            Ok(None)
        }
    }

    /// Returns the cache server id for a bot given its bot id
    ///
    /// Returns None if the bot is not found
    pub async fn lookup_bot(&self, botid: String) -> Result<Option<String>, sqlx::Error> {
        let row = sqlx::query("SELECT guild_id FROM cache_server_bots WHERE bot_id = $1")
            .bind(botid)
            .fetch_optional(&self.pool)
            .await?;

        if let Some(row) = row {
            let guild_id: String = row.try_get("guild_id")?;
            Ok(Some(guild_id))
        } else {
            Ok(None)
        }
    }

    /// Removes a bot from the cache server by its user ID
    pub async fn remove_bot(&self, botid: String) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM cache_server_bots WHERE bot_id = $1")
            .bind(botid)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Deletes a cache server
    pub async fn delete(&self, guildid: String) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM cache_servers WHERE guild_id = $1")
            .bind(guildid)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

impl LuaUserData for CacheServerManager {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_scheduler_async_method("Get", |_lua, this, guildid: String| async move {
            this.get(guildid).await.map_err(LuaError::external)
        });

        methods.add_scheduler_async_method("LookupBot", |_lua, this, botid: String| async move {
            this.lookup_bot(botid).await.map_err(LuaError::external)
        });

        methods.add_scheduler_async_method("RemoveBot", |_lua, this, botid: String| async move {
            this.remove_bot(botid).await.map_err(LuaError::external)
        });

        methods.add_scheduler_async_method("Delete", |_lua, this, guildid: String| async move {
            this.delete(guildid).await.map_err(LuaError::external)
        });
    }
}
