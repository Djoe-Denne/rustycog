use axum::{
    extract::rejection::JsonRejection,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;
use validator::ValidationErrors as ValidatorValidationErrors;

/// Uniform error response structure for all API errors
#[derive(Debug, Serialize)]
pub struct UniformErrorResponse {
    pub error: ErrorDetails,
}

#[derive(Debug, Serialize)]
pub struct ErrorDetails {
    pub error_code: String,
    pub message: String,
    pub status: u16,
}

/// Custom validation error for uniform error format
#[derive(Debug)]
pub struct ValidationError {
    pub error_code: String,
    pub message: String,
    pub status: StatusCode,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.error_code, self.message)
    }
}

impl std::error::Error for ValidationError {}

impl ValidationError {
    pub fn new(error_code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error_code: error_code.into(),
            message: message.into(),
            status: StatusCode::UNPROCESSABLE_ENTITY,
        }
    }

    #[must_use]
    pub const fn with_status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }
}

impl IntoResponse for ValidationError {
    fn into_response(self) -> Response {
        let body = Json(UniformErrorResponse {
            error: ErrorDetails {
                error_code: self.error_code,
                message: self.message,
                status: self.status.as_u16(),
            },
        });
        (self.status, body).into_response()
    }
}

/// Convert validator `ValidationErrors` to our uniform format
impl From<ValidatorValidationErrors> for ValidationError {
    fn from(errors: ValidatorValidationErrors) -> Self {
        // Extract the first validation error for a clean message
        let (field, error_info) =
            errors
                .errors()
                .iter()
                .next()
                .map_or(("unknown", None), |(field, error_kind)| {
                    let first_error = match error_kind {
                        validator::ValidationErrorsKind::Field(errors) => errors
                            .first()
                            .map(|e| (e.code.as_ref(), e.message.as_deref())),
                        _ => None,
                    };
                    (field.as_ref(), first_error)
                });

        let (error_code, message) =
            if let Some((code, msg)) = error_info {
                let formatted_message =
                    msg.map_or_else(
                        || {
                            match code {
                "empty_string" | "empty_password" | "empty_email" | "empty_username" => {
                    format!("{} is required", field.replace('_', " "))
                }
                "invalid_email_format" => "Invalid email format".to_string(),
                "password_too_short" => "Password must be at least 8 characters long".to_string(),
                "password_needs_letter" => "Password must contain at least one letter".to_string(),
                "password_needs_digit" => "Password must contain at least one number".to_string(),
                "password_too_common" => {
                    "Password is too common, please choose a stronger password".to_string()
                }
                "invalid_username_format" => {
                    "Username can only contain letters, numbers, underscores, and hyphens"
                        .to_string()
                }
                "email_too_long" => "Email address is too long".to_string(),
                "password_too_long" => "Password is too long".to_string(),
                _ => format!("Invalid {}", field.replace('_', " ")),
            }
                        },
                        std::string::ToString::to_string,
                    );
                (format!("validation_{code}"), formatted_message)
            } else {
                (
                    "validation_failed".to_string(),
                    "Validation failed".to_string(),
                )
            };

        Self::new(error_code, message)
    }
}

/// Convert JSON parsing errors to our uniform format
impl From<JsonRejection> for ValidationError {
    fn from(_rejection: JsonRejection) -> Self {
        Self::new("invalid_json", "Invalid JSON format in request body")
            .with_status(StatusCode::BAD_REQUEST)
    }
}

/// Generic HTTP errors for the framework
#[derive(Debug, Error)]
pub enum GenericHttpError {
    /// Authentication required
    #[error("Authentication required")]
    AuthenticationRequired,

    /// Invalid request
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Internal server error
    #[error("Internal server error: {0}")]
    InternalServerError(String),

    /// Validation error
    #[error(transparent)]
    Validation(#[from] ValidationError),
}

impl IntoResponse for GenericHttpError {
    fn into_response(self) -> Response {
        let (status, error_code, message) = match self {
            Self::AuthenticationRequired => (
                StatusCode::UNAUTHORIZED,
                "authentication_required".to_string(),
                "Authentication required".to_string(),
            ),
            Self::InvalidRequest(msg) => {
                (StatusCode::BAD_REQUEST, "invalid_request".to_string(), msg)
            }
            Self::InternalServerError(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_server_error".to_string(),
                msg,
            ),
            Self::Validation(validation_error) => {
                return validation_error.into_response();
            }
        };

        let body = Json(UniformErrorResponse {
            error: ErrorDetails {
                error_code,
                message,
                status: status.as_u16(),
            },
        });

        (status, body).into_response()
    }
}
