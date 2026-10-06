//! JWT bearer-token verifier (RS256 + JWKS, with optional HS256 migration window).

use super::jwks::{normalize_authority_url, JwksCache, LocalJwksSeed};
use crate::rustycog_command::{Command, CommandError, CommandHandler, ValidateTokenCommand};
use crate::rustycog_config::{AuthConfig, JwtAuthConfig, MeshAuthConfig};
use async_trait::async_trait;
use jsonwebtoken::{
    decode, decode_header, errors::ErrorKind, Algorithm, DecodingKey, Header, Validation,
};
use std::{collections::HashSet, sync::Arc};
use tracing::debug;
use uuid::Uuid;

/// Access-token type header required for RS256 platform tokens (ADR-0304).
pub const ACCESS_TOKEN_TYP: &str = "aiforall-access+jwt";

/// Authenticated JWT principal: canonical identity is `(iss, sub)`.
///
/// Inserted into Axum request extensions alongside the bare `Uuid` `sub` so
/// existing [`crate::AuthUser`] extractors keep working.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JwtPrincipal {
    /// Token issuer (`iss` claim).
    pub iss: String,
    /// Subject user id (`sub` claim) — always a UUID.
    pub sub: Uuid,
    /// Optional organization trust-context claim (`org`).
    pub org: Option<String>,
}

/// User ID / principal extractor backed by RS256 JWKS and optional HS256.
#[derive(Clone)]
pub struct UserIdExtractor {
    hs256_secret: Option<Arc<String>>,
    allowed_algorithms: Arc<HashSet<Algorithm>>,
    /// Default user ID to use (for testing/development) when the token is empty.
    default_user_id: Option<Uuid>,
    /// Optional expected issuer — applied only on the HS256 path.
    hs256_issuer: Option<String>,
    audience: Option<String>,
    jwks: Option<Arc<JwksCache>>,
    /// Mesh mode: SAN of the gateway whose `x-principal-*` headers are trusted.
    gateway_san: Option<Arc<str>>,
}

impl UserIdExtractor {
    /// Create a new user ID extractor from auth configuration.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] if allowed algorithms cannot be resolved, if
    /// HS256 is allowed without a secret, or if RS256 is allowed without a
    /// JWKS URL (and no inline JWKS was provided via another constructor).
    pub fn new(auth_config: AuthConfig) -> Result<Self, CommandError> {
        Self::from_parts(auth_config.jwt, None, None)
            .map(|extractor| extractor.with_mesh(&auth_config.mesh))
    }

    /// Build a URL-backed verifier seeded by this instance's local publisher.
    ///
    /// The trusted composition root must capture the complete authoritative
    /// publication via [`LocalJwksSeed::capture`], not bootstrap PEM or client
    /// data. This constructor performs no HTTP; polling remains lazy. Fresh
    /// seeded keys validate the first concurrent burst without acquisition.
    /// Successful refresh replaces the seed, including empty/revoked snapshots;
    /// lookup and final authorization retain the original strict 60-second age.
    /// Explicit HS256 migration configuration and mesh settings are preserved,
    /// without enabling HS256 or deriving its secret from RSA material.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] for existing configuration errors, RS256 not
    /// allowed, missing/empty issuer or audience, missing/invalid HTTP(S) URL
    /// (userinfo/fragment rejected), a different normalized seed endpoint,
    /// inconsistent platform issuer, or seed age >=60 seconds. No inline or
    /// bootstrap fallback is installed on failure.
    pub fn from_config_with_seeded_jwks(
        mut auth_config: AuthConfig,
        seed: LocalJwksSeed,
    ) -> Result<Self, CommandError> {
        let issuer = trim_opt(auth_config.jwt.issuer.clone()).ok_or_else(|| {
            CommandError::authentication("invalid_jwt_config", "Seed requires a platform issuer")
        })?;
        if trim_opt(auth_config.jwt.audience.clone()).is_none() {
            return Err(CommandError::authentication(
                "invalid_jwt_config",
                "Seed requires an audience",
            ));
        }
        let raw_url = auth_config.jwt.jwks_url.as_deref().ok_or_else(|| {
            CommandError::authentication("invalid_jwks_config", "Seed requires a JWKS URL")
        })?;
        auth_config.jwt.jwks_url = Some(normalize_authority_url(raw_url)?.to_string());
        // Reuse the existing algorithm/secret/client construction unchanged.
        // This cache has not escaped and polling has not started.
        let extractor = Self::from_parts(auth_config.jwt, None, None)?;
        let cache = extractor.jwks.as_ref().ok_or_else(|| {
            CommandError::authentication("invalid_jwt_config", "Seed requires RS256 verification")
        })?;
        cache.install_local_seed(seed, &issuer)?;
        Ok(extractor.with_mesh(&auth_config.mesh))
    }

    fn with_mesh(mut self, mesh: &MeshAuthConfig) -> Self {
        let san = mesh.trusted_gateway_san.trim();
        self.gateway_san = (!san.is_empty()).then(|| Arc::from(san));
        self
    }

    /// Gateway SAN when mesh mode is on: authenticated routes then read the
    /// gateway principal instead of verifying the bearer JWT.
    #[must_use]
    pub fn gateway_san(&self) -> Option<&str> {
        self.gateway_san.as_deref()
    }

    /// Create a new user ID extractor with a pre-resolved HS256 secret.
    ///
    /// Legacy helper for existing tests — HS256 only, no issuer/audience.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] if the provided secret is empty after trimming.
    pub fn from_resolved_secret(secret: impl Into<String>) -> Result<Self, CommandError> {
        let secret = secret.into();
        let trimmed = secret.trim();
        if trimmed.is_empty() {
            return Err(CommandError::authentication(
                "missing_jwt_secret",
                "HS256 JWT secret not configured for bearer token verification",
            ));
        }
        Ok(Self {
            hs256_secret: Some(Arc::new(trimmed.to_string())),
            allowed_algorithms: Arc::new(HashSet::from([Algorithm::HS256])),
            default_user_id: None,
            hs256_issuer: None,
            audience: None,
            jwks: None,
            gateway_san: None,
        })
    }

    /// Create a new user ID extractor with a default user ID.
    ///
    /// Allows the RS256 path without an HS256 secret when JWKS is configured.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] on the same configuration failures as [`Self::new`].
    pub fn with_default_user_id(
        auth_config: AuthConfig,
        user_id: Uuid,
    ) -> Result<Self, CommandError> {
        Self::from_parts(auth_config.jwt, Some(user_id), None)
            .map(|extractor| extractor.with_mesh(&auth_config.mesh))
    }

    /// Create an RS256 extractor from an inline JWKS JSON document (no network).
    ///
    /// Intended for unit tests. Allowed algorithms default to `[RS256]`.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] if the JWKS document is invalid.
    pub fn from_inline_jwks(
        jwks_json: impl AsRef<str>,
        audience: Option<&str>,
    ) -> Result<Self, CommandError> {
        let mut jwt = JwtAuthConfig {
            allowed_algorithms: vec!["RS256".to_string()],
            audience: audience.map(str::to_string),
            ..JwtAuthConfig::default()
        };
        // Ensure empty secret / no url — inline only.
        jwt.hs256_secret = None;
        jwt.jwks_url = None;
        Self::from_parts(jwt, None, Some(jwks_json.as_ref()))
    }

    /// Create an extractor from config plus an inline JWKS document (no network).
    ///
    /// Useful for dual-verify window tests (`allowed_algorithms` includes both
    /// RS256 and HS256) without hitting a live JWKS server.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] on invalid config or JWKS JSON.
    pub fn from_config_with_inline_jwks(
        auth_config: AuthConfig,
        jwks_json: impl AsRef<str>,
    ) -> Result<Self, CommandError> {
        Self::from_parts(auth_config.jwt, None, Some(jwks_json.as_ref()))
            .map(|extractor| extractor.with_mesh(&auth_config.mesh))
    }

    fn from_parts(
        jwt: JwtAuthConfig,
        default_user_id: Option<Uuid>,
        inline_jwks: Option<&str>,
    ) -> Result<Self, CommandError> {
        let has_inline = inline_jwks.is_some();
        let has_url = jwt.jwks_url.as_ref().is_some_and(|u| !u.trim().is_empty());
        let has_secret = jwt
            .hs256_secret
            .as_ref()
            .is_some_and(|s| !s.trim().is_empty());

        let allowed = resolve_allowed_algorithms(&jwt, has_url || has_inline, has_secret)?;
        let allows_hs256 = allowed.contains(&Algorithm::HS256);
        let allows_rs256 = allowed.contains(&Algorithm::RS256);

        let hs256_secret = if allows_hs256 {
            let secret = jwt
                .hs256_secret
                .as_ref()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .ok_or_else(|| {
                    CommandError::authentication(
                        "missing_jwt_secret",
                        "HS256 JWT secret not configured for bearer token verification",
                    )
                })?;
            Some(Arc::new(secret))
        } else {
            None
        };

        let jwks = if allows_rs256 {
            if let Some(json) = inline_jwks {
                Some(JwksCache::from_inline_json(json)?)
            } else if has_url {
                let url = jwt.jwks_url.ok_or_else(|| {
                    CommandError::authentication(
                        "missing_jwks",
                        "RS256 is allowed but no jwks_url or inline JWKS was configured",
                    )
                })?;
                Some(JwksCache::from_url(
                    url,
                    jwt.jwks_refresh_interval_secs,
                    jwt.jwks_negative_cache_ttl_secs,
                )?)
            } else {
                return Err(CommandError::authentication(
                    "missing_jwks",
                    "RS256 is allowed but no jwks_url or inline JWKS was configured",
                ));
            }
        } else {
            None
        };

        Ok(Self {
            hs256_secret,
            allowed_algorithms: Arc::new(allowed),
            default_user_id,
            hs256_issuer: trim_opt(jwt.issuer),
            audience: trim_opt(jwt.audience),
            jwks,
            gateway_san: None,
        })
    }

    /// Extract the authenticated principal from a bearer token.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] if the token is empty (and no default user is
    /// configured), the header/algorithm is rejected, signature verification
    /// fails, required claims are missing, the token is expired, issuer trust
    /// checks fail, or `sub` is not a valid UUID.
    pub async fn extract_principal(&self, token: &str) -> Result<JwtPrincipal, CommandError> {
        if token.trim().is_empty() {
            if let Some(default_user_id) = self.default_user_id {
                return Ok(JwtPrincipal {
                    iss: String::new(),
                    sub: default_user_id,
                    org: None,
                });
            }
            return Err(CommandError::authentication(
                "invalid_token",
                "Token is empty",
            ));
        }

        debug!("Extracting principal from verified JWT");

        let header = decode_header(token).map_err(|error| Self::map_jwt_error(&error))?;
        refuse_untrusted_header_params(&header)?;

        let alg = header.alg;
        if !self.allowed_algorithms.contains(&alg) {
            return Err(CommandError::authentication(
                "invalid_token",
                format!("JWT algorithm {alg:?} is not allowed"),
            ));
        }

        match alg {
            Algorithm::HS256 => self.verify_hs256(token, &header),
            Algorithm::RS256 => self.verify_rs256(token, &header).await,
            other => Err(CommandError::authentication(
                "invalid_token",
                format!("JWT algorithm {other:?} is not supported"),
            )),
        }
    }

    /// Extract user ID (`sub`) from a bearer token.
    ///
    /// # Errors
    ///
    /// Same failure modes as [`Self::extract_principal`].
    pub async fn extract_user_id(&self, token: &str) -> Result<Uuid, CommandError> {
        Ok(self.extract_principal(token).await?.sub)
    }

    fn verify_hs256(&self, token: &str, _header: &Header) -> Result<JwtPrincipal, CommandError> {
        let secret = self.hs256_secret.as_ref().ok_or_else(|| {
            CommandError::authentication(
                "missing_jwt_secret",
                "HS256 JWT secret not configured for bearer token verification",
            )
        })?;

        // HS256 migration window may omit typ — do not require ACCESS_TOKEN_TYP.
        let mut validation = Validation::new(Algorithm::HS256);
        let mut required = HashSet::from([String::from("exp")]);
        if let Some(iss) = &self.hs256_issuer {
            validation.set_issuer(&[iss]);
            required.insert(String::from("iss"));
        }
        if let Some(aud) = &self.audience {
            validation.set_audience(&[aud]);
            required.insert(String::from("aud"));
        } else {
            validation.validate_aud = false;
        }
        validation.required_spec_claims = required;
        validation.validate_nbf = true;

        let token_data = decode::<serde_json::Value>(
            token,
            &DecodingKey::from_secret(secret.as_bytes()),
            &validation,
        )
        .map_err(|error| Self::map_jwt_error(&error))?;

        let claims = token_data.claims;
        let principal = claims_to_principal(&claims, self.hs256_issuer.clone())?;
        Ok(principal)
    }

    async fn verify_rs256(
        &self,
        token: &str,
        header: &Header,
    ) -> Result<JwtPrincipal, CommandError> {
        let typ = header.typ.as_deref().unwrap_or("");
        if typ != ACCESS_TOKEN_TYP {
            return Err(CommandError::authentication(
                "invalid_token",
                "RS256 token requires typ=aiforall-access+jwt",
            ));
        }

        let kid = header.kid.as_deref().ok_or_else(|| {
            CommandError::authentication("invalid_token", "RS256 token requires kid header")
        })?;
        refuse_dangerous_kid(kid)?;

        let jwks = self.jwks.as_ref().ok_or_else(|| {
            CommandError::authentication(
                "missing_jwks",
                "RS256 is allowed but JWKS is not configured",
            )
        })?;

        let cached = jwks.resolve_key(kid).await?;
        if !cached.trusted {
            return Err(CommandError::authentication(
                "invalid_token",
                "JWK is pending or revoked",
            ));
        }

        let mut validation = Validation::new(Algorithm::RS256);
        let mut required = HashSet::from([String::from("exp"), String::from("iss")]);
        // Do NOT set Validation issuer from JwtAuthConfig — org tokens differ.
        // JWK.iss is checked after a valid signature below.
        if let Some(aud) = &self.audience {
            validation.set_audience(&[aud]);
            required.insert(String::from("aud"));
        } else {
            validation.validate_aud = false;
        }
        validation.required_spec_claims = required;
        validation.validate_nbf = true;

        let token_data = decode::<serde_json::Value>(token, &cached.decoding_key, &validation)
            .map_err(|error| Self::map_jwt_error(&error))?;

        let claims = token_data.claims;
        let jwt_iss = claims["iss"].as_str().ok_or_else(|| {
            CommandError::authentication("invalid_token", "Missing issuer in token")
        })?;
        if jwt_iss != cached.iss {
            return Err(CommandError::authentication(
                "invalid_token",
                "JWT iss does not match JWK iss",
            ));
        }
        if cached.organization_id.is_none()
            && self
                .hs256_issuer
                .as_deref()
                .is_some_and(|expected| expected != jwt_iss)
        {
            return Err(CommandError::authentication(
                "invalid_token",
                "JWT platform issuer does not match configuration",
            ));
        }

        if let Some(owner) = cached.organization_id {
            let org = claims["org"]
                .as_str()
                .and_then(|raw| Uuid::parse_str(raw).ok());
            if org != Some(owner) {
                return Err(CommandError::authentication(
                    "invalid_token",
                    "JWT org does not match JWK owner",
                ));
            }
        }

        let principal = claims_to_principal(&claims, Some(jwt_iss.to_string()))?;
        if !jwks.still_authorizes(kid, cached.acquired_at) {
            return Err(CommandError::authentication(
                "invalid_token",
                "JWKS authorization snapshot expired or replaced",
            ));
        }
        Ok(principal)
    }

    fn map_jwt_error(error: &jsonwebtoken::errors::Error) -> CommandError {
        match error.kind() {
            ErrorKind::ExpiredSignature => {
                CommandError::authentication("token_expired", "Token has expired")
            }
            _ => CommandError::authentication("invalid_token", "Invalid token"),
        }
    }
}

fn resolve_allowed_algorithms(
    jwt: &JwtAuthConfig,
    has_jwks: bool,
    has_secret: bool,
) -> Result<HashSet<Algorithm>, CommandError> {
    if jwt.allowed_algorithms.is_empty() {
        return if has_jwks {
            Ok(HashSet::from([Algorithm::RS256]))
        } else if has_secret {
            Ok(HashSet::from([Algorithm::HS256]))
        } else {
            Err(CommandError::authentication(
                "invalid_jwt_config",
                "No JWT algorithms configured: set allowed_algorithms, jwks_url, or hs256_secret",
            ))
        };
    }

    let mut out = HashSet::new();
    for name in &jwt.allowed_algorithms {
        let alg = match name.trim().to_ascii_uppercase().as_str() {
            "RS256" => Algorithm::RS256,
            "HS256" => Algorithm::HS256,
            other => {
                return Err(CommandError::authentication(
                    "invalid_jwt_config",
                    format!("unsupported JWT algorithm in allowed_algorithms: {other}"),
                ));
            }
        };
        out.insert(alg);
    }
    Ok(out)
}

fn trim_opt(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let t = v.trim().to_string();
        (!t.is_empty()).then_some(t)
    })
}

fn refuse_untrusted_header_params(header: &Header) -> Result<(), CommandError> {
    if header.jku.as_ref().is_some_and(|s| !s.is_empty()) {
        return Err(CommandError::authentication(
            "invalid_token",
            "JWT header jku is not allowed",
        ));
    }
    if header.x5u.as_ref().is_some_and(|s| !s.is_empty()) {
        return Err(CommandError::authentication(
            "invalid_token",
            "JWT header x5u is not allowed",
        ));
    }
    if header.jwk.is_some() {
        return Err(CommandError::authentication(
            "invalid_token",
            "JWT header jwk is not allowed",
        ));
    }
    Ok(())
}

fn refuse_dangerous_kid(kid: &str) -> Result<(), CommandError> {
    if kid.contains('/') || kid.contains('\\') || kid.contains("://") || kid.contains("..") {
        return Err(CommandError::authentication(
            "invalid_token",
            "JWT kid contains forbidden characters",
        ));
    }
    Ok(())
}

fn claims_to_principal(
    claims: &serde_json::Value,
    fallback_iss: Option<String>,
) -> Result<JwtPrincipal, CommandError> {
    let sub = claims["sub"]
        .as_str()
        .ok_or_else(|| CommandError::authentication("invalid_token", "Missing user ID in token"))?;

    let exp = claims["exp"].as_i64().ok_or_else(|| {
        CommandError::authentication("invalid_token", "Missing expiration in token")
    })?;

    let iat = claims["iat"].as_i64().ok_or_else(|| {
        CommandError::authentication("invalid_token", "Missing issued at time in token")
    })?;

    let jti = claims["jti"]
        .as_str()
        .ok_or_else(|| CommandError::authentication("invalid_token", "Missing JWT ID in token"))?;
    if jti.trim().is_empty() {
        return Err(CommandError::authentication(
            "invalid_token",
            "Missing JWT ID in token",
        ));
    }

    let now = chrono::Utc::now().timestamp();
    validate_claim_times(claims, iat, now, Validation::new(Algorithm::RS256).leeway)?;
    if exp <= now {
        debug!("Token expired: exp={exp}, now={now}");
        return Err(CommandError::authentication(
            "token_expired",
            "Token has expired",
        ));
    }

    let sub = Uuid::parse_str(sub)
        .map_err(|_| CommandError::authentication("invalid_token", "Invalid user ID format"))?;

    let iss = claims["iss"]
        .as_str()
        .map(str::to_string)
        .or(fallback_iss)
        .unwrap_or_default();

    let org = claims["org"].as_str().map(str::to_string);

    Ok(JwtPrincipal { iss, sub, org })
}

fn validate_claim_times(
    claims: &serde_json::Value,
    iat: i64,
    now: i64,
    leeway: u64,
) -> Result<(), CommandError> {
    let latest = now.saturating_add(i64::try_from(leeway).unwrap_or(i64::MAX));
    if iat < 0 || iat > latest {
        return Err(CommandError::authentication(
            "invalid_token",
            "Invalid issued at time",
        ));
    }
    if let Some(nbf) = claims.get("nbf") {
        if !nbf.as_i64().is_some_and(|time| time >= 0 && time <= latest) {
            return Err(CommandError::authentication(
                "invalid_token",
                "Invalid not before time",
            ));
        }
    }
    Ok(())
}

/// Command handler for user ID extraction from bearer tokens.
pub struct UserIdExtractionHandler {
    extractor: Arc<UserIdExtractor>,
}

impl UserIdExtractionHandler {
    /// Create a new user ID extraction handler
    #[must_use]
    pub fn new(extractor: UserIdExtractor) -> Self {
        Self {
            extractor: Arc::new(extractor),
        }
    }
}

#[async_trait]
impl CommandHandler<ValidateTokenCommand> for UserIdExtractionHandler {
    async fn handle(&self, command: ValidateTokenCommand) -> Result<Uuid, CommandError> {
        debug!(
            "Handling ValidateTokenCommand with ID: {}",
            command.command_id()
        );

        command.validate()?;
        self.extractor.extract_user_id(&command.token).await
    }
}

#[cfg(all(test, feature = "testing"))]
#[path = "jwt_rs256_tests.rs"]
mod jwt_rs256_tests;

#[cfg(test)]
mod claim_time_tests {
    use super::*;

    #[test]
    fn optional_nbf_and_iat_use_the_same_bounded_skew() {
        let now = 1_700_000_000;
        let skew = Validation::new(Algorithm::RS256).leeway;
        let bound = now + i64::try_from(skew).unwrap();
        assert!(validate_claim_times(&serde_json::json!({}), bound, now, skew).is_ok());
        assert!(validate_claim_times(&serde_json::json!({}), bound + 1, now, skew).is_err());
        assert!(validate_claim_times(&serde_json::json!({}), -1, now, skew).is_err());
        assert!(validate_claim_times(&serde_json::json!({"nbf":bound}), now, now, skew).is_ok());
        for value in [
            serde_json::json!(bound + 1),
            serde_json::json!("future"),
            serde_json::Value::Null,
            serde_json::json!(-1),
        ] {
            assert!(
                validate_claim_times(&serde_json::json!({"nbf":value}), now, now, skew).is_err()
            );
        }
    }
}
