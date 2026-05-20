use chrono::Utc;
use rustycog_core::error::ServiceError;
use rustycog_events::DomainEvent;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Set};
use serde_json::Value;
use uuid::Uuid;

use crate::entity::{self, STATUS_PENDING};

#[derive(Clone, Debug, Default)]
pub struct OutboxRecorder;

impl OutboxRecorder {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    pub async fn record<C>(
        &self,
        connection: &C,
        event: &dyn DomainEvent,
    ) -> Result<(), ServiceError>
    where
        C: ConnectionTrait,
    {
        let payload_json: Value = serde_json::from_str(&event.to_json()?).map_err(|e| {
            ServiceError::infrastructure(format!("Failed to parse event payload JSON: {e}"))
        })?;
        let metadata_json = serde_json::to_value(event.metadata()).map_err(|e| {
            ServiceError::infrastructure(format!("Failed to serialize event metadata: {e}"))
        })?;
        let now = Utc::now();

        entity::ActiveModel {
            id: Set(Uuid::new_v4()),
            event_id: Set(event.event_id()),
            event_type: Set(event.event_type().to_string()),
            aggregate_id: Set(event.aggregate_id()),
            version: Set(i32::try_from(event.version()).map_err(|_| {
                ServiceError::infrastructure("Event version does not fit in i32".to_string())
            })?),
            occurred_at: Set(event.occurred_at()),
            payload_json: Set(payload_json),
            metadata_json: Set(metadata_json),
            status: Set(STATUS_PENDING.to_string()),
            attempts: Set(0),
            next_attempt_at: Set(now),
            locked_by: Set(None),
            locked_until: Set(None),
            last_error: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(connection)
        .await
        .map_err(|e| ServiceError::infrastructure(format!("Failed to record outbox event: {e}")))?;

        Ok(())
    }
}
