use axum::{
    body::Body,
    extract::{FromRequestParts, State},
    http::{request::Parts, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use tracing::debug;
use uuid::Uuid;

use super::jwt_handler::{JwtPrincipal, UserIdExtractor};
use super::mesh_principal::{gateway_principal, require_gateway_peer};
use super::tls::PeerClientCertificate;
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
    auth_header.strip_prefix("Bearer ")
}

fn request_gateway_principal(
    req: &Request<Body>,
    gateway_san: &str,
) -> Result<JwtPrincipal, &'static str> {
    gateway_principal(
        req.extensions().get::<PeerClientCertificate>(),
        req.headers(),
        gateway_san,
    )
}

fn with_principal(mut req: Request<Body>, principal: JwtPrincipal) -> Request<Body> {
    // Keep inserting Uuid so AuthUser / OptionalAuthUser / permission middleware work.
    req.extensions_mut().insert(principal.sub);
    debug!("User ID added to request extensions: {:?}", principal.sub);
    req.extensions_mut().insert(principal);
    req
}

/// Authentication middleware using JWT user ID / principal extractor
///
/// In mesh mode the bearer JWT is ignored: the principal comes from the
/// gateway headers on an mTLS connection from the gateway.
///
/// # Errors
///
/// Returns [`StatusCode::UNAUTHORIZED`] if the Authorization header is missing,
/// is not a Bearer token, or user ID extraction fails. In mesh mode, returns it
/// when the peer is not the gateway or the principal headers are invalid.
pub async fn auth_middleware(
    State(user_id_extractor): State<Arc<UserIdExtractor>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if let Some(gateway_san) = user_id_extractor.gateway_san() {
        let principal = request_gateway_principal(&req, gateway_san).map_err(|reason| {
            debug!(reason, "Gateway principal rejected");
            StatusCode::UNAUTHORIZED
        })?;
        return Ok(next.run(with_principal(req, principal)).await);
    }

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

    let principal = user_id_extractor
        .extract_principal(token)
        .await
        .map_err(|e| {
            debug!("User ID extraction failed: {}", e);
            StatusCode::UNAUTHORIZED
        })?;

    Ok(next.run(with_principal(req, principal)).await)
}

/// Optional authentication middleware.
///
/// When `trusted_gateway_san` is set, the mTLS peer must be the gateway.
/// Missing client certificate or a SAN that is not the gateway is 401.
/// Gateway SAN plus valid principal headers become the user; missing or
/// invalid headers continue anonymous (no [`Uuid`] in extensions).
///
/// When mesh mode is off, a missing or invalid bearer continues anonymous.
///
/// # Errors
///
/// Returns [`StatusCode::UNAUTHORIZED`] in mesh mode when the peer is not the
/// gateway. Otherwise the request continues (`Ok`).
pub async fn optional_auth_middleware(
    State(user_id_extractor): State<Arc<UserIdExtractor>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if let Some(gateway_san) = user_id_extractor.gateway_san() {
        require_gateway_peer(req.extensions().get::<PeerClientCertificate>(), gateway_san)
            .map_err(|reason| {
                debug!(reason, "Gateway principal rejected");
                StatusCode::UNAUTHORIZED
            })?;
        return Ok(match request_gateway_principal(&req, gateway_san) {
            Ok(principal) => next.run(with_principal(req, principal)).await,
            Err(reason) => {
                debug!(reason, "No gateway principal, continuing without auth");
                next.run(req).await
            }
        });
    }

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

            if let Ok(principal) = user_id_extractor.extract_principal(token).await {
                return Ok(next.run(with_principal(req, principal)).await);
            }
            debug!("Optional user ID extraction failed, continuing without auth");
        }
    }

    // Continue without authentication
    Ok(next.run(req).await)
}
