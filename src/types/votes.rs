use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
/// Represents a vote on an entity.
pub struct EntityVote {
    /// The internal ID of the entity.
    pub itag: Uuid,
    /// The type of the entity that was voted on
    pub target_type: String,
    /// The ID of the entity that was voted on
    pub target_id: String,
    /// The ID of the user who voted
    pub author: String,
    /// Whether or not the vote was an upvote
    pub upvote: bool,
    /// Whether or not the vote was voided
    pub void: bool,
    /// The reason the vote was voided
    pub void_reason: Option<String>,
    /// The time the vote was voided, if it was voided
    pub voided_at: Option<DateTime<Utc>>,
    /// The time the vote was created
    pub created_at: DateTime<Utc>,
    /// The number of the vote (second vote of double vote will have vote_num as 2 etc.)
    pub vote_num: i32,
    /// Whether or not the vote is immutable
    pub immutable: bool,
}

#[derive(Debug, Serialize, Deserialize)]
/// Core vote info about an entity.
pub struct VoteInfo {
    /// The amount of votes a single vote creates on this entity
    pub per_user: u8,
    /// The amount of time in hours until a user can vote again
    pub vote_time: u16,
    /// Whether or not the entity supports vote credits
    pub vote_credits: bool,
    /// Whether or not the entity supports multiple votes per time interval
    pub multiple_votes: bool,
    /// Whether or not the entity supports upvotes
    pub supports_upvotes: bool,
    /// Whether or not the entity supports downvotes
    pub supports_downvotes: bool,
}

#[derive(Debug, Serialize, Deserialize)]
/// Stores the hours, minutes and seconds until the user can vote again
pub struct VoteWait {
    /// Hours until the user can vote again
    pub hours: i32,
    /// Minutes until the user can vote again
    pub minutes: i32,
    /// Seconds until the user can vote again
    pub seconds: i32,
}

#[derive(Debug, Serialize, Deserialize)]
/// A user vote is a struct containing basic info on a users vote
pub struct UserVote {
    /// Whether or not the user has voted for the entity. If an entity supports multiple votes, this will be true if the user has voted in the last vote time, otherwise, it will be true if the user has voted at all
    pub has_voted: bool,
    /// A list of all non-voided votes the user has made on the entity
    pub valid_votes: Vec<EntityVote>,
    /// Some information about the vote
    pub vote_info: VoteInfo,
    /// The time until the user can vote again
    pub wait: Option<VoteWait>,
}
