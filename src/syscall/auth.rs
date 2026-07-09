use serde::{Deserialize, Serialize};
use serenity::all::UserId;
use crate::{config::CONFIG, service::session::SessionManager, syscall::{AuthError, SyscallContext, SyscallError, SyscallHandler}, types::{auth::Session, dovewing::PartialUser}};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum AuthSyscall {
    /// Creates a login session using oauth2
    CreateLoginSession {
        code: String,
        redirect_uri: String,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum AuthSyscallRet {
    /// A created session returned by a syscall
    CreatedSession {
        /// Session metadata
        session: Session,
        /// Session token
        token: String,
        /// The user who created the session (only sent on OAuth2 login)
        user: Option<PartialUser>,
    },
    UserSessions {
        sessions: Vec<Session>
    },
    Ack
}


impl AuthSyscall {
    pub(super) async fn exec(self, handler: &SyscallHandler, ctx: SyscallContext) -> Result<AuthSyscallRet, SyscallError> {
        match self {
            Self::CreateLoginSession { code, redirect_uri } => {
                handler.limit(&ctx, "CreateLoginSession")?;

                if !CONFIG.allowed_redirect_urls.contains(&redirect_uri) {
                    return Err(SyscallError::AuthError { reason: AuthError::InvalidRedirectUri });
                }

                if code.len() < 3 {
                    return Err(SyscallError::AuthError { reason: AuthError::CodeTooShort });
                }

                if handler.oauth2_code_cache.contains_key(&code) {
                    return Err(SyscallError::AuthError { reason: AuthError::CodeReuseDetected });
                }

                handler.oauth2_code_cache.insert(code.clone(), ()).await;

                #[derive(serde::Serialize)]
                pub struct Response<'a> {
                    client_id: UserId,
                    client_secret: &'a str,
                    grant_type: &'static str,
                    code: String,
                    redirect_uri: String,
                }

                let resp = handler.reqwest.post(format!("{}/api/v10/oauth2/token", CONFIG.proxy_url))
                    .form(&Response {
                        client_id: handler.current_user.id,
                        client_secret: &CONFIG.client_secret,
                        grant_type: "authorization_code",
                        code,
                        redirect_uri,
                    })
                    .send()
                    .await
                    .map_err(|e| format!("Failed to get access token: {e:?}"))?;

                if resp.status() != reqwest::StatusCode::OK {
                    let error_text = resp.text().await?;
                    return Err(format!("Failed to get access token: {}", error_text).into());
                }

                #[derive(serde::Deserialize)]
                struct OauthTokenResponse {
                    pub access_token: String,
                    pub refresh_token: String,
                    pub expires_in: i32,
                    pub scope: String,
                }

                let token_response: OauthTokenResponse = resp.json().await?;

                let scopes = token_response.scope.replace("%20", " ")
                    .split(' ')
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>();

                if !scopes.contains(&"identify".to_string()) || !scopes.contains(&"guilds".to_string()) {
                    return Err(SyscallError::AuthError { reason: AuthError::NeededScopesNotFound });
                }    

                // Fetch user info
                let user_resp = handler.reqwest.get(format!("{}/api/v10/users/@me", CONFIG.proxy_url))
                    .header("Authorization", format!("Bearer {}", &token_response.access_token))
                    .send()
                    .await
                    .map_err(|e| format!("Failed to get user info from discord: {e:?}"))?;

                if user_resp.status() != reqwest::StatusCode::OK {
                    let error_text = user_resp.text().await?;
                    return Err(format!("Failed to get user info: {}", error_text).into());
                }

                let user_info: PartialUser = user_resp.json().await?;

                // Create a session for the user and save the oauth2
                let mut tx = handler.pool.begin().await?;
                
                // Ensure we have a web user for this user
                SessionManager::create_web_user_from_oauth2(
                    &mut *tx,
                    &user_info.id,
                ).await
                .map_err(|e| format!("Failed to create user: {e:?}"))?;

                // Make the session
                let cws = SessionManager::create_login_session(
                    &mut *tx,
                    "user",
                    &user_info.id,
                )
                .await
                .map_err(|e| format!("Failed to create session: {e:?}"))?;

                // Fetch the created session (TODO: improve this)
                let session = SessionManager::get_session_by_known_token_with(&mut *tx, &cws.token)
                .await
                .map_err(|e| format!("Failed to fetch created session: {e:?}"))?;

                // Commit atomically once the above steps have succeeded
                tx.commit().await?;

                Ok(
                    AuthSyscallRet::CreatedSession { 
                        session,
                        token: cws.token,
                        user: Some(user_info)
                    }
                ) 
            }
        }
    }
}