use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(sqlx::FromRow, Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// The ID of the session
    pub id: uuid::Uuid,
    /// The name of the session. Login sessions do not have any names by default
    pub name: Option<String>,
    /// The time the session was created
    pub created_at: DateTime<Utc>,
    /// The type of session token
    #[sqlx(rename = "type")]
    pub session_type: String,
    /// The target (entities) type
    pub target_type: String,
    /// The target (entities) ID
    pub target_id: String,
    /// The time the session expires
    pub expiry: DateTime<Utc>,
    /// Known entity ID
    pub keid: uuid::Uuid,
    /// Permission limits
    pub perm_limits: Vec<String>
}
