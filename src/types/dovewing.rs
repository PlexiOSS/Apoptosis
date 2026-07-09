use serde::{Deserialize, Serialize};

/// The PartialUser of a user, which contains only the necessary fields for the API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialUser {
    /// The ID of the user
    pub id: String,
    /// The username of the user
    pub username: String,
    /// The global name of the user
    pub global_name: Option<String>,
    /// The avatar hash of the user
    pub avatar: Option<String>,
}
