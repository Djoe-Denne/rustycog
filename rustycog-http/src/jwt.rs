use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Claims for JWT tokens
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenClaims {
    /// Subject (user id)
    pub sub: String,

    /// Username
    pub username: String,

    /// JWT expiration timestamp
    pub exp: i64,

    /// JWT issued at timestamp
    pub iat: i64,

    /// JWT ID
    pub jti: String,
}

impl TokenClaims {
    /// Creates new token claims for a user
    #[must_use]
    pub fn new(user_id: &str, username: &str, expires_in: Duration) -> Self {
        let now = Utc::now();
        Self {
            sub: user_id.to_string(),
            username: username.to_string(),
            exp: (now + expires_in).timestamp(),
            iat: now.timestamp(),
            jti: Uuid::new_v4().to_string(),
        }
    }
}
