use chrono::{Duration, Utc};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::Serialize;
use uuid::Uuid;

pub const TEST_HS256_SECRET: &str = "rustycog-test-hs256-secret";

#[derive(Debug, Serialize)]
struct TestClaims {
    sub: String,
    exp: usize,
    iat: usize,
    jti: String,
}

/// Create a JWT token for the given user ID with a shared HS256 test secret
#[must_use]
pub fn create_jwt_token(user_id: Uuid) -> String {
    create_jwt_token_with_secret(user_id, TEST_HS256_SECRET)
}

/// Create a JWT token with a caller-provided HS256 secret
#[must_use]
pub fn create_jwt_token_with_secret(user_id: Uuid, secret: &str) -> String {
    let now = Utc::now();
    let claims = TestClaims {
        sub: user_id.to_string(),
        exp: (now + Duration::hours(1)).timestamp() as usize,
        iat: now.timestamp() as usize,
        jti: Uuid::new_v4().to_string(),
    };

    let header = Header::new(Algorithm::HS256);
    encode(
        &header,
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("failed to encode test JWT")
}
