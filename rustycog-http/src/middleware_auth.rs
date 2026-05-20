use axum::{
    body::Body,
    extract::{FromRequestParts, State},
    http::{request::Parts, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use tracing::debug;
use uuid::Uuid;

use crate::jwt_handler::UserIdExtractor;
use std::sync::Arc;

/// Authenticated user information extracted from middleware
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
}
/// Optional authenticated user information extracted from middleware
#[derive(Debug, Clone)]
pub struct OptionalAuthUser {
    pub user: Option<AuthUser>,
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let user_id = parts
            .extensions
            .get::<Uuid>()
            .copied()
            .ok_or(StatusCode::UNAUTHORIZED)?;

        Ok(Self { user_id })
    }
}

impl OptionalAuthUser {
    /// Get the user ID if authenticated, None otherwise
    #[must_use]
    pub fn user_id(&self) -> Option<Uuid> {
        self.user.as_ref().map(|u| u.user_id)
    }

    /// Check if the user is authenticated
    #[must_use]
    pub const fn is_authenticated(&self) -> bool {
        self.user.is_some()
    }
}

impl<S> FromRequestParts<S> for OptionalAuthUser
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let user_id = parts.extensions.get::<Uuid>().copied();

        Ok(Self {
            user: user_id.map(|user_id| AuthUser { user_id }),
        })
    }
}

/// Extract JWT token from the Authorization header
fn extract_token(auth_header: &str) -> Option<&str> {
    if auth_header.starts_with("Bearer ") {
        Some(&auth_header[7..])
    } else {
        None
    }
}

/// Authentication middleware using simple user ID extractor
pub async fn auth_middleware(
    State(user_id_extractor): State<Arc<UserIdExtractor>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Get the Authorization header
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|header| header.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Extract the token
    let token = extract_token(auth_header).ok_or(StatusCode::UNAUTHORIZED)?;

    // Get request ID for logging
    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|h| h.to_str().ok())
        .map(std::string::ToString::to_string);

    debug!(
        "Try to validate token for query {}",
        request_id.unwrap_or_default()
    );

    // Extract user ID from token (no verification)
    let user_id = user_id_extractor.extract_user_id(token).map_err(|e| {
        debug!("User ID extraction failed: {}", e);
        StatusCode::UNAUTHORIZED
    })?;

    // Add the user ID to the request extensions
    let mut req = req;
    req.extensions_mut().insert(user_id);

    debug!("User ID added to request extensions: {:?}", user_id);
    // Continue with the request
    Ok(next.run(req).await)
}

/// Optional authentication middleware that doesn't fail if no auth is provided
pub async fn optional_auth_middleware(
    State(user_id_extractor): State<Arc<UserIdExtractor>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Try to get the Authorization header, but don't fail if it's missing
    if let Some(auth_header) = req
        .headers()
        .get("Authorization")
        .and_then(|header| header.to_str().ok())
    {
        // Try to extract the token
        if let Some(token) = extract_token(auth_header) {
            // Get request ID for logging
            let request_id = req
                .headers()
                .get("x-request-id")
                .and_then(|h| h.to_str().ok())
                .map(std::string::ToString::to_string);

            debug!(
                "Try to validate optional token for query {}",
                request_id.unwrap_or_default()
            );

            // Try to extract user ID from token
            if let Ok(user_id) = user_id_extractor.extract_user_id(token) {
                // Add the user ID to the request extensions if successful
                let mut req = req;
                req.extensions_mut().insert(user_id);
                debug!("User ID added to request extensions: {:?}", user_id);
                return Ok(next.run(req).await);
            }
            debug!("Optional user ID extraction failed, continuing without auth");
        }
    }

    // Continue without authentication
    Ok(next.run(req).await)
}
