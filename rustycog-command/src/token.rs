use crate::{Command, CommandError};
use async_trait::async_trait;
use uuid::Uuid;

/// Command to validate a JWT token and return the user ID
#[derive(Debug, Clone)]
pub struct ValidateTokenCommand {
    /// The JWT token to validate
    pub token: String,
    /// Command instance ID
    pub command_id: Uuid,
}

impl ValidateTokenCommand {
    /// Create a new `ValidateTokenCommand`
    #[must_use]
    pub fn new(token: String) -> Self {
        Self {
            token,
            command_id: Uuid::new_v4(),
        }
    }
}

#[async_trait]
impl Command for ValidateTokenCommand {
    type Result = Uuid;

    fn command_type(&self) -> &'static str {
        "validate_token"
    }

    fn command_id(&self) -> Uuid {
        self.command_id
    }

    fn validate(&self) -> Result<(), CommandError> {
        if self.token.trim().is_empty() {
            return Err(CommandError::validation(
                "empty_token",
                "Token cannot be empty",
            ));
        }

        // Basic JWT format validation (three parts separated by dots)
        let parts: Vec<&str> = self.token.split('.').collect();
        if parts.len() != 3 {
            return Err(CommandError::validation(
                "invalid_token_format",
                "Invalid JWT token format",
            ));
        }

        Ok(())
    }
}
