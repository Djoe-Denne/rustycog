//! Transactional outbox support for `RustyCog` services.
//!
//! This crate intentionally bridges `rustycog-db` and `rustycog-events` so the
//! event transport crate can remain database-free.

pub mod dispatcher;
pub mod entity;
pub mod migration;
pub mod recorder;
pub mod stored_event;

pub use dispatcher::{OutboxConfig, OutboxDispatcher};
pub use migration::outbox_migration;
pub use recorder::OutboxRecorder;
pub use stored_event::StoredOutboxEvent;
