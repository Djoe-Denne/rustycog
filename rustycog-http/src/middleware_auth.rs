use crate::rustycog_permission::{PrincipalPolicy, PrincipalRef};
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

/// Authority-preserving identity for principal-aware handlers.
#[derive(Debug, Clone)]
pub struct AuthPrincipal {
    pub principal: PrincipalRef,
    policy: Arc<PrincipalPolicy>,
}

impl AuthPrincipal {
    pub fn subject(&self) -> Result<crate::rustycog_permission::Subject, StatusCode> {
        crate::rustycog_permission::Subject::from_principal(self.principal.clone(), &self.policy)
            .map_err(|_| StatusCode::FORBIDDEN)
    }

    pub fn platform_user_id(&self) -> Result<Uuid, StatusCode> {
        self.policy
            .platform_user_id(&self.principal)
            .map_err(|error| {
                tracing::debug!(code = %error, "Principal rejected by UUID-only interface");
                StatusCode::FORBIDDEN
            })
    }
}

#[derive(Debug, Clone)]
pub struct OptionalAuthPrincipal {
    pub principal: Option<AuthPrincipal>,
}

pub(crate) fn request_principal(
    extensions: &axum::http::Extensions,
) -> Result<Option<AuthPrincipal>, StatusCode> {
    let Some(principal) = extensions.get::<JwtPrincipal>() else {
        return Ok(None);
    };
    let policy = extensions
        .get::<Arc<PrincipalPolicy>>()
        .cloned()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let principal = PrincipalRef::new(principal.iss.clone(), principal.sub)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Some(AuthPrincipal { principal, policy }))
}

impl<S: Send + Sync> FromRequestParts<S> for AuthPrincipal {
    type Rejection = StatusCode;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        request_principal(&parts.extensions)?.ok_or(StatusCode::UNAUTHORIZED)
    }
}

impl<S: Send + Sync> FromRequestParts<S> for OptionalAuthPrincipal {
    type Rejection = StatusCode;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self {
            principal: request_principal(&parts.extensions)?,
        })
    }
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let principal = request_principal(&parts.extensions)?.ok_or(StatusCode::UNAUTHORIZED)?;
        let user_id = principal.platform_user_id()?;
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
        let user = request_principal(&parts.extensions)?
            .map(|principal| {
                principal
                    .platform_user_id()
                    .map(|user_id| AuthUser { user_id })
            })
            .transpose()?;
        Ok(Self { user })
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

fn clear_identity(req: &mut Request<Body>) {
    req.extensions_mut().remove::<Uuid>();
    req.extensions_mut().remove::<JwtPrincipal>();
    req.extensions_mut().remove::<Arc<PrincipalPolicy>>();
    req.extensions_mut().remove::<AuthPrincipal>();
    req.extensions_mut().remove::<OptionalAuthPrincipal>();
    req.extensions_mut().remove::<AuthUser>();
    req.extensions_mut().remove::<OptionalAuthUser>();
}

fn with_principal(
    mut req: Request<Body>,
    principal: JwtPrincipal,
    extractor: &UserIdExtractor,
) -> Result<Request<Body>, StatusCode> {
    let policy = extractor
        .principal_policy()
        .cloned()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    PrincipalRef::new(principal.iss.clone(), principal.sub).map_err(|_| StatusCode::FORBIDDEN)?;
    clear_identity(&mut req);
    req.extensions_mut().insert(policy);
    req.extensions_mut().insert(principal);
    Ok(req)
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
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    clear_identity(&mut req);
    if let Some(gateway_san) = user_id_extractor.gateway_san() {
        let principal = request_gateway_principal(&req, gateway_san).map_err(|reason| {
            debug!(reason, "Gateway principal rejected");
            StatusCode::UNAUTHORIZED
        })?;
        return Ok(next
            .run(with_principal(req, principal, &user_id_extractor)?)
            .await);
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

    Ok(next
        .run(with_principal(req, principal, &user_id_extractor)?)
        .await)
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
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    clear_identity(&mut req);
    if let Some(gateway_san) = user_id_extractor.gateway_san() {
        require_gateway_peer(req.extensions().get::<PeerClientCertificate>(), gateway_san)
            .map_err(|reason| {
                debug!(reason, "Gateway principal rejected");
                StatusCode::UNAUTHORIZED
            })?;
        return Ok(match request_gateway_principal(&req, gateway_san) {
            Ok(principal) => {
                next.run(with_principal(req, principal, &user_id_extractor)?)
                    .await
            }
            Err(reason) => {
                if req.headers().contains_key("x-principal-iss")
                    || req.headers().contains_key("x-principal-sub")
                {
                    return Err(StatusCode::UNAUTHORIZED);
                }
                debug!(reason, "No gateway principal, continuing without auth");
                next.run(req).await
            }
        });
    }

    if let Some(auth_header) = req.headers().get("Authorization") {
        let token = auth_header
            .to_str()
            .ok()
            .and_then(extract_token)
            .ok_or(StatusCode::UNAUTHORIZED)?;
        let principal = user_id_extractor
            .extract_principal(token)
            .await
            .map_err(|_| StatusCode::UNAUTHORIZED)?;
        return Ok(next
            .run(with_principal(req, principal, &user_id_extractor)?)
            .await);
    }

    // Continue without authentication
    Ok(next.run(req).await)
}

#[cfg(test)]
mod principal_tests {
    use super::*;

    #[tokio::test]
    async fn uuid_only_consumer_rejects_foreign_authority_before_lookup() {
        let id = Uuid::new_v4();
        let policy = Arc::new(PrincipalPolicy::new("https://platform.example/iam", false).unwrap());
        for issuer in [
            "https://platform.example/iam",
            "https://organization.example/iam",
        ] {
            let (mut parts, _) = Request::new(Body::empty()).into_parts();
            parts.extensions.insert(JwtPrincipal {
                iss: issuer.into(),
                sub: id,
                org: None,
            });
            parts.extensions.insert(policy.clone());
            let full = AuthPrincipal::from_request_parts(&mut parts, &())
                .await
                .unwrap();
            assert_eq!(full.principal.issuer, issuer);
            let legacy = AuthUser::from_request_parts(&mut parts, &()).await;
            if issuer == policy.platform_issuer {
                assert_eq!(legacy.unwrap().user_id, id);
            } else {
                assert_eq!(legacy.unwrap_err(), StatusCode::FORBIDDEN);
                assert_eq!(
                    OptionalAuthUser::from_request_parts(&mut parts, &())
                        .await
                        .unwrap_err(),
                    StatusCode::FORBIDDEN
                );
            }
        }
        let (mut parts, _) = Request::new(Body::empty()).into_parts();
        parts.extensions.insert(id);
        assert_eq!(
            AuthUser::from_request_parts(&mut parts, &())
                .await
                .unwrap_err(),
            StatusCode::UNAUTHORIZED
        );
    }
}
