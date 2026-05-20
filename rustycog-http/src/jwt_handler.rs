use async_trait::async_trait;
use jsonwebtoken::{decode, errors::ErrorKind, Algorithm, DecodingKey, Validation};
use rustycog_command::{Command, CommandError, CommandHandler, ValidateTokenCommand};
use rustycog_config::AuthConfig;
use std::{collections::HashSet, sync::Arc};
use tracing::debug;
use uuid::Uuid;

/// User ID extractor backed by the current shared HS256 bearer-token verifier.
///
/// This path intentionally verifies only HS256 tokens via `auth.jwt.hs256_secret`.
/// It is not a generic JWKS / RS256 verifier.
#[derive(Clone)]
pub struct UserIdExtractor {
    hs256_secret: Arc<String>,
    /// Default user ID to use (for testing/development)
    default_user_id: Option<Uuid>,
}

impl UserIdExtractor {
    /// Create a new user ID extractor
    pub fn new(auth_config: AuthConfig) -> Result<Self, CommandError> {
        Self::from_secret(auth_config.jwt.hs256_secret, None)
    }

    /// Create a new user ID extractor with a pre-resolved secret
    pub fn from_resolved_secret(secret: impl Into<String>) -> Result<Self, CommandError> {
        Self::from_secret(Some(secret.into()), None)
    }

    /// Create a new user ID extractor with a default user ID
    pub fn with_default_user_id(
        auth_config: AuthConfig,
        user_id: Uuid,
    ) -> Result<Self, CommandError> {
        Self::from_secret(auth_config.jwt.hs256_secret, Some(user_id))
    }

    fn from_secret(
        hs256_secret: Option<String>,
        default_user_id: Option<Uuid>,
    ) -> Result<Self, CommandError> {
        let secret = hs256_secret
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                CommandError::authentication(
                    "missing_jwt_secret",
                    "HS256 JWT secret not configured for bearer token verification",
                )
            })?;

        Ok(Self {
            hs256_secret: Arc::new(secret),
            default_user_id,
        })
    }

    fn validation() -> Validation {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.required_spec_claims = HashSet::from([String::from("exp")]);
        validation.validate_nbf = false;
        validation
    }

    fn map_jwt_error(error: jsonwebtoken::errors::Error) -> CommandError {
        match error.kind() {
            ErrorKind::ExpiredSignature => {
                CommandError::authentication("token_expired", "Token has expired")
            }
            _ => CommandError::authentication("invalid_token", "Invalid token"),
        }
    }

    /// Extract user ID from token with signature verification and claim validation
    pub fn extract_user_id(&self, token: &str) -> Result<Uuid, CommandError> {
        if token.trim().is_empty() {
            if let Some(default_user_id) = self.default_user_id {
                return Ok(default_user_id);
            }

            return Err(CommandError::authentication(
                "invalid_token",
                "Token is empty",
            ));
        }

        debug!("Extracting user ID from verified JWT");

        let token_data = decode::<serde_json::Value>(
            token,
            &DecodingKey::from_secret(self.hs256_secret.as_bytes()),
            &Self::validation(),
        )
        .map_err(Self::map_jwt_error)?;

        let claims = token_data.claims;

        let sub = claims["sub"].as_str().ok_or_else(|| {
            CommandError::authentication("invalid_token", "Missing user ID in token")
        })?;

        let exp = claims["exp"].as_i64().ok_or_else(|| {
            CommandError::authentication("invalid_token", "Missing expiration in token")
        })?;

        let _iat = claims["iat"].as_i64().ok_or_else(|| {
            CommandError::authentication("invalid_token", "Missing issued at time in token")
        })?;

        let jti = claims["jti"].as_str().ok_or_else(|| {
            CommandError::authentication("invalid_token", "Missing JWT ID in token")
        })?;

        if jti.trim().is_empty() {
            return Err(CommandError::authentication(
                "invalid_token",
                "Missing JWT ID in token",
            ));
        }

        let now = chrono::Utc::now().timestamp();
        if exp <= now {
            debug!("Token expired: exp={}, now={}", exp, now);
            return Err(CommandError::authentication(
                "token_expired",
                "Token has expired",
            ));
        }

        Uuid::parse_str(sub)
            .map_err(|_| CommandError::authentication("invalid_token", "Invalid user ID format"))
    }
}

/// Command handler for simple user ID extraction
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

        // Validate the command first
        command.validate()?;

        // Extract user ID from a verified bearer token
        self.extractor.extract_user_id(&command.token)
    }
}
