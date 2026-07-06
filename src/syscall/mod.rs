mod webapi;

use std::num::NonZeroU32;
use std::{sync::Arc, time::Duration};
use std::fmt::{Display, Debug};
use governor::DefaultKeyedRateLimiter;
use governor::clock::{Clock, QuantaClock};
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use crate::entity::{AnyEntityManager, EntityFlags};
use crate::service::session::SessionManager;
use crate::types::auth::Session;
use crate::utils::stratum::Stratum;

#[derive(Debug)]
pub struct AuthData {
    session: Session,
    flags: EntityFlags,
    manager: AnyEntityManager,
}

/// The context in which the syscall is executing in
#[derive(Debug)]
#[allow(dead_code)]
pub enum MSyscallContext {
    /// API context (anonymous/logged out)
    ApiAnon,
    /// API context (api token)
    Api(AuthData),
    //// A 'secure' API context (luau etc.)
    ApiSecure(AuthData),
}

#[allow(dead_code)]
impl MSyscallContext {
    /// Returns if the given context is secure (admin/root access only)
    /// 
    /// A context is considered secure iff it originates from a user (with admin permissions)
    /// running under the secure msyscall API endpoint (which verifies that the user has admin)
    /// or if the request comes from the tw shell (which is assumed to have admin permissions)
    #[inline(always)]
    pub const fn is_secure(&self) -> bool {
        matches!(self, Self::ApiSecure(_))
    }

    /// Returns the underlying known entity ID
    pub const fn keid(&self) -> Option<uuid::Uuid> {
        match self {
            Self::Api(ad) | Self::ApiSecure(ad) => Some(ad.session.keid),
            _ => None
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum MSyscallArgs {
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum MSyscallRet {
}

#[derive(Serialize, Debug)]
#[serde(tag = "op")]
pub enum AuthError {
    /// Invalid redirect URI not allowed by server
    InvalidRedirectUri,
    /// Code too short (invalid)
    CodeTooShort,
    /// Code has been reused in the past couple minutes, most likely invalid, reauth needed
    CodeReuseDetected,
    /// Oauth requires 'identify' and 'guilds' scope but a needed scope was not found
    NeededScopesNotFound,
    /// Expiry time out of range (for creating api sessions etc)
    ExpiryTimeOutOfRange
}

#[derive(Debug, Serialize)]
#[serde(tag = "op")]
pub enum MSyscallError {
    /// Generic error response
    Generic { message: String },
    /// Context is too insecure to perform this operation
    ContextInsecure,
    /// Context requires a user to actually perform this operation on/with
    ContextRequiresUser,
    /// An authentication error has occurred
    AuthError { reason: AuthError },
    /// Unauthorized
    Unauthorized { reason: &'static str },
    /// Entity not found
    EntityNotFound { reason: &'static str },
    /// Ratelimited
    Ratelimited {
        retry_after: f32,
        bucket: &'static str,
        req_bucket: &'static str
    }
}

impl<T: Debug + Display + 'static> From<T> for MSyscallError {
    fn from(value: T) -> Self {
        Self::Generic { message: value.to_string() }
    }
}

#[derive(Clone)]
pub struct MSyscallHandler {
    pub(super) current_user: Arc<serenity::all::CurrentUser>,
    pub(super) reqwest: reqwest::Client,
    pub(super) stratum: Stratum,
    pub(super) pool: sqlx::PgPool,
    pub(super) oauth2_code_cache: Cache<String, ()>,
    pub(super) user_rl: Arc<Ratelimiter>,
    pub(super) session_manager: SessionManager
}

impl MSyscallHandler {
    /// Creates a new MSyscallHandler
    pub fn new(
        current_user: Arc<serenity::all::CurrentUser>, 
        stratum: Stratum,
        reqwest: reqwest::Client,
        pool: sqlx::PgPool,
        session_manager: SessionManager
    ) -> Self {
        Self { 
            pool: pool.clone(), 
            current_user,
            reqwest,
            stratum,
            oauth2_code_cache: Cache::builder().time_to_live(Duration::from_secs(60 * 10)).build(),
            user_rl: Self::user_limits().expect("Failed to build user limits").into(),
            session_manager
        }
    }

    /// Helper method to return msyscall ratelimits
    fn user_limits() -> Result<Ratelimiter, crate::Error> {
        fn new(limit_per: u32, limit_time: Duration) -> DefaultKeyedRateLimiter<uuid::Uuid> {
            let quota =
                Ratelimiter::create_quota(NonZeroU32::new(limit_per).unwrap(), limit_time).expect("Failed to create quota");
            let lim = DefaultKeyedRateLimiter::keyed(quota);
            lim
        }

        // Create the global limit
        let global1 = new(10, Duration::from_secs(1));

        // GetUserGuilds (with refresh)
        let gug_refresh1 = new(3, Duration::from_secs(5));

        // GetGuildInfo
        let ggi1 = new(2, Duration::from_secs(5));

        // SearchGuildMembers
        let sgm1 = new(1, Duration::from_secs(4));
        let sgm2 = new(5, Duration::from_mins(1));

        // Create the clock
        let clock = QuantaClock::default();

        Ok(Ratelimiter {
            global: vec![global1],
            per_bucket: indexmap::indexmap!(
                "GetUserGuilds__Refresh" => vec![gug_refresh1],
                "GetGuildInfo" => vec![ggi1],
                "SearchGuildMembers" => vec![sgm1, sgm2]
            ),
            clock,
        })
    }

    pub(super) fn limit(&self, ctx: &MSyscallContext, op: &'static str) -> Result<(), MSyscallError> {
        if let Some(keid) = ctx.keid() {
            self.user_rl.check(op, keid).map_err(|e| MSyscallError::Ratelimited {
                retry_after: e.dur.as_secs_f32(),
                bucket: e.bucket,
                req_bucket: e.req_bucket
            })
        } else {
            Ok(())
        }
    }

    pub(super) fn sub_limit(&self, ctx: &MSyscallContext, op: &'static str) -> Result<(), MSyscallError> {
        if let Some(keid) = ctx.keid() {
            self.user_rl.sub_check(op, keid).map_err(|e| MSyscallError::Ratelimited {
                retry_after: e.dur.as_secs_f32(),
                bucket: e.bucket,
                req_bucket: e.req_bucket
            })
        } else {
            Ok(())
        }
    }

    /// Handles a syscall
    pub async fn handle_syscall(&self, args: MSyscallArgs, ctx: MSyscallContext) -> Result<MSyscallRet, MSyscallError> {
        match args {
            /*MSyscallArgs::Bot { req } => {
                Ok(MSyscallRet::Bot { data: req.exec(self, ctx).await? })
            }
            MSyscallArgs::Discord { req } => {
                Ok(MSyscallRet::Discord { data: req.exec(self, ctx).await? })
            }
            MSyscallArgs::Auth { req } => {
                Ok(MSyscallRet::Auth { data: req.exec(self, ctx).await? })
            }
            MSyscallArgs::Gkv { req } => {
                Ok(MSyscallRet::Gkv { data: req.exec(self, ctx).await? })
            }*/
        }
    }
}


#[allow(dead_code)]
pub struct Ratelimiter {
    pub clock: QuantaClock,
    pub global: Vec<DefaultKeyedRateLimiter<uuid::Uuid>>,
    pub per_bucket: indexmap::IndexMap<&'static str, Vec<DefaultKeyedRateLimiter<uuid::Uuid>>>,
}

struct RlExceeded {
    dur: Duration,
    bucket: &'static str,
    req_bucket: &'static str
}

impl Ratelimiter {
    fn create_quota(
        limit_per: NonZeroU32,
        limit_time: Duration,
    ) -> Result<governor::Quota, crate::Error> {
        let quota = governor::Quota::with_period(limit_time)
            .ok_or("Failed to create quota")?
            .allow_burst(limit_per);

        Ok(quota)
    }

    fn check(&self, bucket: &'static str, id: uuid::Uuid) -> Result<(), RlExceeded> {
        for global_lim in self.global.iter() {
            match global_lim.check_key(&id) {
                Ok(()) => continue,
                Err(wait) => {
                    return Err(RlExceeded { dur: wait.wait_time_from(self.clock.now()), bucket: "global", req_bucket: bucket });
                }
            };
        }

        // Check per bucket ratelimits
        if let Some(per_bucket) = self.per_bucket.get(bucket) {
            for lim in per_bucket.iter() {
                match lim.check_key(&id) {
                    Ok(()) => continue,
                    Err(wait) => {
                        return Err(RlExceeded { dur: wait.wait_time_from(self.clock.now()), bucket, req_bucket: bucket });
                    }
                };
            }
        }

        Ok(())
    }

    /// Same as check, but only checks bucket
    fn sub_check(&self, bucket: &'static str, id: uuid::Uuid) -> Result<(), RlExceeded> {
        // Check per bucket ratelimits
        if let Some(per_bucket) = self.per_bucket.get(bucket) {
            for lim in per_bucket.iter() {
                match lim.check_key(&id) {
                    Ok(()) => continue,
                    Err(wait) => {
                        return Err(RlExceeded { dur: wait.wait_time_from(self.clock.now()), bucket, req_bucket: bucket });
                    }
                };
            }
        }

        Ok(())
    }
}
