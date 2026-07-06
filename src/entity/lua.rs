use crate::entity::{EntityFlags, manager::EntityManager};

use super::Entity;
use bitflags::Flags;
use khronos_runtime::rt::mluau::prelude::*;
use khronos_runtime::rt::mlua_scheduler::LuaSchedulerAsyncUserData;

/// Wrapper struct to expose EntityManager to Lua
pub struct LuaEntityManager<T: Entity>(EntityManager<T>);

impl<T: Entity> LuaEntityManager<T> {
    pub fn new(manager: EntityManager<T>) -> Self {
        Self(manager)
    }
}

impl<T: Entity> LuaUserData for LuaEntityManager<T> {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Entity", |_, this, _: ()| {
            let entity = this.0.entity().clone();
            Ok(LuaEntity::new(entity))
        });

        /*
fetch_votes(
		&self,
		user_id: &str,
		id: &str,
		only_valid: bool, // whether or not to only fetch non-void votes
		limit_offset: Option<(u32, u32)>, // (limit, offset)
	) -> Result<Vec<EntityVote>, crate::Error> {
         */
        methods.add_scheduler_async_method("FetchVotes", async |lua, this, (user_id, id, only_valid, limit_offset): (String, String, bool, Option<LuaVector>)| {
            let lo = match limit_offset {
                Some(vec) => {
                    let x = vec.x();
                    let y = vec.y();
                    let z= vec.z();
                    if z != 0.0 {
                        return Err(LuaError::external("Limit offset vector must be 2D"));
                    }
                    if !x.is_normal() || !y.is_normal() {
                        return Err(LuaError::external("Limit offset vector components must be normal numbers"));
                    }
                    let limit = x as i64;
                    let offset = y as i64;
                    Some((limit, offset))
                },
                None => None,
            };
            let res = this.0.fetch_votes(&user_id, &id, only_valid, lo).await;
            match res {
                Ok(votes) => lua.to_value(&votes),
                Err(e) => Err(LuaError::external(e.to_string())),
            }
        });

        /*
        	/// Helper method to get full vote info for an entity, wrapping the underlying entity's get_vote_info method and adding flag info.
	    pub async fn get_full_vote_info(&self, id: &str, user_id: Option<&str>) -> Result<VoteInfo, crate::Error> { */
        methods.add_scheduler_async_method("GetFullVoteInfo", async |lua, this, (id, user_id): (String, Option<String>)| {
            let res = this.0.get_full_vote_info(&id, user_id.as_deref()).await;
            match res {
                Ok(vote_info) => lua.to_value(&vote_info),
                Err(e) => Err(LuaError::external(e.to_string())),
            }
        });

        /*
            /// Checks whether or not a user has voted for an entity
        pub async fn vote_check(&self, id: &str, user_id: &str) -> Result<UserVote, crate::Error> { */
        methods.add_scheduler_async_method("VoteCheck", async |lua, this, (id, user_id): (String, String)| {
            let res = this.0.vote_check(&id, &user_id).await;
            match res {
                Ok(user_vote) => lua.to_value(&user_vote),
                Err(e) => Err(LuaError::external(e.to_string())),
            }
        });

        /*
	/// Returns the exact (non-cached/approximate) vote count for an entity
	pub async fn exact_vote_count(&self, id: &str, user_id: &str) -> Result<i64, crate::Error> {
         */
        methods.add_scheduler_async_method("ExactVoteCount", async |_, this, (id, user_id): (String, String)| {
            let res = this.0.exact_vote_count(&id, &user_id).await;
            match res {
                Ok(count) => Ok(count),
                Err(e) => Err(LuaError::external(e.to_string())),
            }
        });

        /*
        	/// Helper function to give votes to an entity 
	pub async fn give_votes(&self, id: &str, user_id: &str, upvote: bool) -> Result<(), crate::Error> { */
        methods.add_scheduler_async_method("GiveVotes", async |_, this, (id, user_id, upvote): (String, String, bool)| {
            let res = this.0.give_votes(&id, &user_id, upvote).await;
            match res {
                Ok(()) => Ok(()),
                Err(e) => Err(LuaError::external(e.to_string())),
            }
        });
    }
}

/// Wrapper struct to expose Entity implementations to Lua.
pub struct LuaEntity<T: Entity>(T);

impl<T: Entity> LuaEntity<T> {
    pub fn new(entity: T) -> Self {
        Self(entity)
    }
}

impl<T: Entity> LuaUserData for LuaEntity<T> {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Name", |_, this, ()| {
            Ok(this.0.name().to_string())
        });

        methods.add_method("TargetType", |_, this, ()| {
            Ok(this.0.target_type().to_string())
        });

        methods.add_method("CdnFolder", |_, this, ()| {
            Ok(this.0.cdn_folder().to_string())
        });

        methods.add_scheduler_async_method("Flags", async |_, this, id: String| {
            let mut flags = Vec::new();
            let fset = this.0.flags(&id).await.map_err(|e| LuaError::external(e.to_string()))?;
            for flag in EntityFlags::FLAGS {
                if fset.contains(*flag.value()) {
                    flags.push(flag.name().to_string());
                }
            }
            Ok(flags)
        });

        methods.add_scheduler_async_method("GetInfo", async |lua, this, id: String| {
            let res = this.0.get_info(&id).await;
            match res {
                Ok(opt_info) => lua.to_value(&opt_info),
                Err(e) => Err(LuaError::external(e.to_string())),
            }
        });

        methods.add_scheduler_async_method("GetVoteInfo", async |lua, this, (id, user_id): (String, Option<String>)| {
            let res = this.0.get_vote_info(&id, user_id.as_deref()).await;
            match res {
                Ok(vote_info) => lua.to_value(&vote_info),
                Err(e) => Err(LuaError::external(e.to_string())),
            }
        });

        methods.add_scheduler_async_method("GetFull", async |lua, this, id: String| {
            let res = this.0.get_full(&id).await;
            match res {
                Ok(full_obj) => lua.to_value(&full_obj),
                Err(e) => Err(LuaError::external(e.to_string())),
            }
        });

        methods.add_scheduler_async_method("GetPublic", async |lua, this, id: String| {
            let res = this.0.get_public(&id).await;
            match res {
                Ok(public_obj) => lua.to_value(&public_obj),
                Err(e) => Err(LuaError::external(e.to_string())),
            }
        });

        methods.add_scheduler_async_method("GetSummary", async |lua, this, id: String| {
            let res = this.0.get_summary(&id).await;
            match res {
                Ok(summary_obj) => lua.to_value(&summary_obj),
                Err(e) => Err(LuaError::external(e.to_string())),
            }
        });
    }
}
