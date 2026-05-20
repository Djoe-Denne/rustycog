use chrono::{DateTime, Utc};
use rustycog_core::error::ServiceError;
use rustycog_events::DomainEvent;
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

use crate::entity;

#[derive(Clone, Debug)]
pub struct StoredOutboxEvent {
    pub id: Uuid,
    pub event_id: Uuid,
    pub event_type: String,
    pub aggregate_id: Uuid,
    pub version: i32,
    pub occurred_at: DateTime<Utc>,
    pub payload_json: Value,
    pub metadata: HashMap<String, String>,
}

impl StoredOutboxEvent {
    #[must_use]
    pub fn from_model(model: entity::Model) -> Self {
        let metadata = serde_json::from_value(model.metadata_json).unwrap_or_default();

        Self {
            id: model.id,
            event_id: model.event_id,
            event_type: model.event_type,
            aggregate_id: model.aggregate_id,
            version: model.version,
            occurred_at: model.occurred_at,
            payload_json: model.payload_json,
            metadata,
        }
    }
}

impl DomainEvent for StoredOutboxEvent {
    fn event_type(&self) -> &str {
        self.event_type.as_str()
    }

    fn event_id(&self) -> Uuid {
        self.event_id
    }

    fn aggregate_id(&self) -> Uuid {
        self.aggregate_id
    }

    fn occurred_at(&self) -> DateTime<Utc> {
        self.occurred_at
    }

    fn version(&self) -> u32 {
        self.version as u32
    }

    fn to_json(&self) -> Result<String, ServiceError> {
        serde_json::to_string(&self.payload_json).map_err(|e| {
            ServiceError::infrastructure(format!("Failed to serialize outbox payload: {e}"))
        })
    }

    fn metadata(&self) -> HashMap<String, String> {
        self.metadata.clone()
    }
}
