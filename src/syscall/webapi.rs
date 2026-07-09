use std::time::Duration;

use axum::response::Response;
use axum::routing::{get, post};
use axum::{extract::{State, FromRequestParts, Json}, Router, response::IntoResponse};
use reqwest::{StatusCode, header};
use reqwest::header::AUTHORIZATION;
use tower_http::cors::MaxAge;
use crate::service::session::SessionPermit;
use crate::syscall::AuthData;

use super::{SyscallArgs, SyscallContext, SyscallRet};
use super::{SyscallError, SyscallHandler};

impl IntoResponse for SyscallRet {
    fn into_response(self) -> Response {
        (StatusCode::OK, Json(self)).into_response()
    }
}

impl IntoResponse for SyscallError {
    fn into_response(self) -> Response {
        match self {
            SyscallError::Ratelimited { retry_after, .. } => {
                (
                    StatusCode::TOO_MANY_REQUESTS, 
                    [
                        ("Retry-After", retry_after.to_string()),
                    ],
                    Json(self)
                ).into_response()
            },
            _ => (StatusCode::BAD_REQUEST, Json(self)).into_response()
        }
    }
}

/// This extractor checks entity auth
struct AuthorizedUser {
    pub data: AuthData
}

struct OptionalAuthorizedUser(Option<AuthorizedUser>);

impl FromRequestParts<SyscallHandler> for OptionalAuthorizedUser {
    type Rejection = SyscallError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &SyscallHandler,
    ) -> Result<Self, Self::Rejection> {
        if parts.headers.contains_key(AUTHORIZATION) {
            Ok(Self(Some(AuthorizedUser::from_request_parts(parts, state).await?)))
        } else {
            Ok(Self(None))
        }
    }
}

impl FromRequestParts<SyscallHandler> for AuthorizedUser {
    type Rejection = SyscallError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &SyscallHandler,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .ok_or_else(|| SyscallError::Unauthorized { reason: "No Authorization header found" })?;

        let auth_response = state.session_manager.get_permit_for(token).await?;

        match auth_response {
            SessionPermit::Success { session, flags, manager } => Ok(AuthorizedUser { 
                data: AuthData { session, flags, manager } 
            }),
            SessionPermit::ApiBanned { .. } => {
                return Err(SyscallError::Unauthorized { reason: "You have banned from using this service" })
            }
            SessionPermit::InvalidToken => return Err(SyscallError::Unauthorized { reason: "The token provided is invalid. Check that it hasn't expired and try again?" }),
            SessionPermit::EntityNotSupported => return Err(SyscallError::Unauthorized { reason: "Unsupported entity type" }),
        }
    }
}

pub fn create(handler: SyscallHandler) -> axum::routing::IntoMakeService<Router> {
    async fn logger(
        request: axum::extract::Request,
        next: axum::middleware::Next,
    ) -> axum::response::Response {
        log::info!(
            "Received request: method = {}, path={}",
            request.method(),
            request.uri().path()
        );

        let response = next.run(request).await;
        response
    }

    pub(super) async fn msyscall(
        user: OptionalAuthorizedUser,
        State(handler): State<SyscallHandler>,
        Json(args): Json<SyscallArgs>,
    ) -> Result<SyscallRet, SyscallError> {
        let ctx = if let Some(user) = user.0 { 
            SyscallContext::Api(user.data)
        } else { SyscallContext::ApiAnon };
        let resp = handler.handle_syscall(args, ctx).await?;
        Ok(resp)
    }

    let mut router = Router::new();

    router = router
        .route("/healthcheck", post(|| async { Json(()) }))
        .route("/syscall", post(msyscall))
        .fallback(get(|| async {
            (
                StatusCode::NOT_FOUND,
                "Use /syscall for syscall (insecure) and /syscall/secure for syscall (secure, staff-only)"
            )
        }))
        .layer(
            tower_http::cors::CorsLayer::very_permissive()
            .expose_headers([header::RETRY_AFTER])
            .max_age(MaxAge::exact(Duration::from_secs(86400)))
        )
        .layer(axum::middleware::from_fn(logger));

    let router: Router<()> = router.with_state(handler);
    router.into_make_service()
}