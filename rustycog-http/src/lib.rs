pub mod builder;
pub mod error;
pub mod extractors;
pub mod jwt;
pub mod jwt_handler;
pub mod middleware_auth;
pub mod middleware_permission;
pub mod tracing_middleware;

pub use builder::{serve_router, AppState, RouteBuilder};
pub use error::{GenericHttpError, ValidationError};
pub use extractors::ValidatedJson;
pub use jwt::TokenClaims;
pub use jwt_handler::{UserIdExtractionHandler, UserIdExtractor};
pub use middleware_auth::{auth_middleware, optional_auth_middleware, AuthUser, OptionalAuthUser};
pub use tracing_middleware::{
    get_correlation_id, get_request_id, tracing_middleware, X_CORRELATION_ID, X_REQUEST_ID,
};

use axum::{
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde_json::json;

/// Health check handler
pub async fn health_check() -> &'static str {
    "OK"
}

/// Handle panic in middleware
pub fn handle_panic(err: Box<dyn std::any::Any + Send + 'static>) -> axum::response::Response {
    let details = if let Some(s) = err.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = err.downcast_ref::<&str>() {
        s.to_string()
    } else {
        "Unknown panic".to_string()
    };

    tracing::error!("Service panicked: {}", details);

    let body = Json(json!({
        "error": {
            "message": "Internal server error",
            "status": 500,
        }
    }));

    (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
}
