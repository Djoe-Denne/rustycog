//! RustyCog feature-gated framework crate.
//!
//! This crate consolidates the historical `rustycog-*` crates under a single
//! package while preserving module boundaries via Cargo features.

#[cfg(feature = "core")]
#[path = "../rustycog-core/src/lib.rs"]
pub mod core;

#[cfg(feature = "config")]
#[path = "../rustycog-config/src/lib.rs"]
pub mod config;

#[cfg(feature = "command")]
#[path = "../rustycog-command/src/lib.rs"]
pub mod command;

#[cfg(feature = "db")]
#[path = "../rustycog-db/src/lib.rs"]
pub mod db;

#[cfg(feature = "events")]
#[path = "../rustycog-events/src/lib.rs"]
pub mod events;

#[cfg(feature = "http")]
#[path = "../rustycog-http/src/lib.rs"]
pub mod http;

#[cfg(feature = "logger")]
#[path = "../rustycog-logger/src/lib.rs"]
pub mod logger;

#[cfg(feature = "outbox")]
#[path = "../rustycog-outbox/src/lib.rs"]
pub mod outbox;

#[cfg(feature = "permission")]
#[path = "../rustycog-permission/src/lib.rs"]
pub mod permission;

#[cfg(feature = "server")]
#[path = "../rustycog-server/src/lib.rs"]
pub mod server;

// Internal compatibility namespaces so existing module source can keep
// referencing historical crate paths while compiled as one package.
#[cfg(feature = "core")]
pub mod rustycog_core {
    pub use crate::core::*;
}

#[cfg(feature = "config")]
pub mod rustycog_config {
    pub use crate::config::*;
}

#[cfg(feature = "command")]
pub mod rustycog_command {
    pub use crate::command::*;
}

#[cfg(feature = "db")]
pub mod rustycog_db {
    pub use crate::db::*;
}

#[cfg(feature = "events")]
pub mod rustycog_events {
    pub use crate::events::*;
}

#[cfg(feature = "http")]
pub mod rustycog_http {
    pub use crate::http::*;
}

#[cfg(feature = "logger")]
pub mod rustycog_logger {
    pub use crate::logger::*;
}

#[cfg(feature = "outbox")]
pub mod rustycog_outbox {
    pub use crate::outbox::*;
}

#[cfg(feature = "permission")]
pub mod rustycog_permission {
    pub use crate::permission::*;
}

#[cfg(feature = "server")]
pub mod rustycog_server {
    pub use crate::server::*;
}
