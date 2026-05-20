//! # `RustyCog` Command
//!
//! Generic command pattern implementation with registry and execution framework.

use async_trait::async_trait;

use std::fmt::Debug;
use thiserror::Error;
use uuid::Uuid;

pub mod generic_service;
pub mod registry;
pub mod token;

pub use generic_service::*;
pub use registry::*;
pub use token::*;

/// Command execution error
#[derive(Debug, Error)]
pub enum CommandError {
    /// Validation error
    #[error("Validation error [{code}]: {message}")]
    Validation { code: String, message: String },

    /// Authentication error
    #[error("Authentication error [{code}]: {message}")]
    Authentication { code: String, message: String },

    /// Business logic error
    #[error("Business error [{code}]: {message}")]
    Business { code: String, message: String },

    /// Infrastructure error (database, external services, etc.)
    #[error("Infrastructure error [{code}]: {message}")]
    Infrastructure { code: String, message: String },

    /// Timeout error
    #[error("Command execution timeout [{code}]: {message}")]
    Timeout { code: String, message: String },

    /// Retry exhausted error
    #[error("Maximum retries exhausted [{code}]: {message}")]
    RetryExhausted { code: String, message: String },
}

impl CommandError {
    /// Create a validation error with code and message
    pub fn validation(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Validation {
            code: code.into(),
            message: message.into(),
        }
    }

    /// Create an authentication error with code and message
    pub fn authentication(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Authentication {
            code: code.into(),
            message: message.into(),
        }
    }

    /// Create a business error with code and message
    pub fn business(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Business {
            code: code.into(),
            message: message.into(),
        }
    }

    /// Create an infrastructure error with code and message
    pub fn infrastructure(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Infrastructure {
            code: code.into(),
            message: message.into(),
        }
    }

    /// Create a timeout error with code and message
    pub fn timeout(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Timeout {
            code: code.into(),
            message: message.into(),
        }
    }

    /// Create a retry exhausted error with code and message
    pub fn retry_exhausted(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::RetryExhausted {
            code: code.into(),
            message: message.into(),
        }
    }

    /// Get the error code
    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            Self::Validation { code, .. } => code,
            Self::Authentication { code, .. } => code,
            Self::Business { code, .. } => code,
            Self::Infrastructure { code, .. } => code,
            Self::Timeout { code, .. } => code,
            Self::RetryExhausted { code, .. } => code,
        }
    }

    /// Get the error message
    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            Self::Validation { message, .. } => message,
            Self::Authentication { message, .. } => message,
            Self::Business { message, .. } => message,
            Self::Infrastructure { message, .. } => message,
            Self::Timeout { message, .. } => message,
            Self::RetryExhausted { message, .. } => message,
        }
    }
}

/// Command trait that all commands must implement
#[async_trait]
pub trait Command: Debug + Send + Sync {
    /// The result type returned by this command
    type Result: Send + Sync;

    /// Unique identifier for this command type
    fn command_type(&self) -> &'static str;

    /// Unique identifier for this command instance
    fn command_id(&self) -> Uuid;

    /// Validate the command before execution
    fn validate(&self) -> Result<(), CommandError>;
}

/// Command handler trait
#[async_trait]
pub trait CommandHandler<C: Command>: Send + Sync {
    /// Execute the command
    async fn handle(&self, command: C) -> Result<C::Result, CommandError>;
}

/// Command execution context
#[derive(Debug, Clone)]
pub struct CommandContext {
    /// Command execution ID
    pub execution_id: Uuid,
    /// User ID (if applicable)
    pub user_id: Option<Uuid>,
    /// Request ID for tracing
    pub request_id: Option<String>,
    /// Additional metadata
    pub metadata: std::collections::HashMap<String, String>,
}

impl CommandContext {
    #[must_use]
    pub fn new() -> Self {
        Self {
            execution_id: Uuid::new_v4(),
            user_id: None,
            request_id: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    #[must_use]
    pub const fn with_user_id(mut self, user_id: Uuid) -> Self {
        self.user_id = Some(user_id);
        self
    }

    #[must_use]
    pub fn with_request_id(mut self, request_id: String) -> Self {
        self.request_id = Some(request_id);
        self
    }

    #[must_use]
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

impl Default for CommandContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Command execution metrics
#[derive(Debug, Clone)]
pub struct CommandMetrics {
    /// Command type
    pub command_type: String,
    /// Execution duration in milliseconds
    pub duration_ms: u64,
    /// Whether the command succeeded
    pub success: bool,
    /// Number of retry attempts
    pub retry_attempts: u32,
    /// Error type (if failed)
    pub error_type: Option<String>,
}
