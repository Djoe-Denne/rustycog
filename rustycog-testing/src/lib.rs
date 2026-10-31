pub mod common;
pub mod db;
pub mod events;
pub mod http;
pub mod permission;
pub mod wiremock;

// Service-local fixtures passed to start_service_testcontainer must use the
// framework's exact testcontainers types, even with multiple versions resolved.
pub use testcontainers;

// Re-export commonly used items
pub use self::common::*;
pub use self::db::*;
pub use self::http::*;
pub use self::wiremock::*;
