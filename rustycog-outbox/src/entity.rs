use chrono::{DateTime, Utc};
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const STATUS_PENDING: &str = "pending";
pub const STATUS_PUBLISHING: &str = "publishing";
pub const STATUS_PUBLISHED: &str = "published";
pub const STATUS_FAILED: &str = "failed";

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "rustycog_outbox_events")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub event_id: Uuid,
    pub event_type: String,
    pub aggregate_id: Uuid,
    pub version: i32,
    pub occurred_at: DateTime<Utc>,
    pub payload_json: Value,
    pub metadata_json: Value,
    pub status: String,
    pub attempts: i32,
    pub next_attempt_at: DateTime<Utc>,
    pub locked_by: Option<String>,
    pub locked_until: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

pub type OutboxEvents = Entity;
