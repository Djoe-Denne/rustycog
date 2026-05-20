use std::{
    fmt::Debug,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use chrono::Utc;
use rustycog_core::error::ServiceError;
use rustycog_db::DbConnectionPool;
use rustycog_events::{DomainEvent, EventPublisher};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    Set,
};
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, warn};

use crate::{
    entity::{
        self, OutboxEvents, STATUS_FAILED, STATUS_PENDING, STATUS_PUBLISHED, STATUS_PUBLISHING,
    },
    stored_event::StoredOutboxEvent,
};

#[derive(Clone, Debug)]
pub struct OutboxConfig {
    pub worker_id: String,
    pub poll_interval: Duration,
    pub batch_size: u64,
    pub lock_timeout: Duration,
    pub max_attempts: i32,
    pub retry_base_delay: Duration,
}

impl Default for OutboxConfig {
    fn default() -> Self {
        Self {
            worker_id: format!("outbox-{}", uuid::Uuid::new_v4()),
            poll_interval: Duration::from_secs(2),
            batch_size: 25,
            lock_timeout: Duration::from_secs(30),
            max_attempts: 20,
            retry_base_delay: Duration::from_secs(5),
        }
    }
}

pub struct OutboxDispatcher<TError> {
    db: DbConnectionPool,
    publisher: Arc<dyn EventPublisher<TError>>,
    config: OutboxConfig,
    stop_requested: Arc<AtomicBool>,
}

impl<TError> Clone for OutboxDispatcher<TError> {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            publisher: Arc::clone(&self.publisher),
            config: self.config.clone(),
            stop_requested: Arc::clone(&self.stop_requested),
        }
    }
}

impl<TError> OutboxDispatcher<TError>
where
    TError: Debug + Send + Sync + 'static,
{
    pub fn new(
        db: DbConnectionPool,
        publisher: Arc<dyn EventPublisher<TError>>,
        config: OutboxConfig,
    ) -> Self {
        Self {
            db,
            publisher,
            config,
            stop_requested: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn start(&self) -> Result<(), ServiceError> {
        self.stop_requested.store(false, Ordering::SeqCst);
        info!(worker_id = %self.config.worker_id, "RustyCog outbox dispatcher started");

        while !self.stop_requested.load(Ordering::SeqCst) {
            if let Err(error) = self.dispatch_once().await {
                error!(error = %error, "RustyCog outbox dispatch cycle failed");
            }
            sleep(self.config.poll_interval).await;
        }

        info!(worker_id = %self.config.worker_id, "RustyCog outbox dispatcher stopped");
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), ServiceError> {
        self.stop_requested.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub async fn dispatch_once(&self) -> Result<usize, ServiceError> {
        let claimed = self.claim_batch().await?;
        let count = claimed.len();

        for row in claimed {
            self.publish_claimed(row).await?;
        }

        Ok(count)
    }

    async fn claim_batch(&self) -> Result<Vec<entity::Model>, ServiceError> {
        let db = self.db.get_write_connection();
        let now = Utc::now();
        let lock_until = now
            + chrono::Duration::from_std(self.config.lock_timeout).map_err(|e| {
                ServiceError::infrastructure(format!("Invalid outbox lock timeout: {e}"))
            })?;

        let candidates = OutboxEvents::find()
            .filter(
                Condition::any()
                    .add(
                        Condition::all()
                            .add(entity::Column::Status.eq(STATUS_PENDING))
                            .add(entity::Column::NextAttemptAt.lte(now)),
                    )
                    .add(
                        Condition::all()
                            .add(entity::Column::Status.eq(STATUS_FAILED))
                            .add(entity::Column::NextAttemptAt.lte(now)),
                    )
                    .add(
                        Condition::all()
                            .add(entity::Column::Status.eq(STATUS_PUBLISHING))
                            .add(entity::Column::LockedUntil.lte(now)),
                    ),
            )
            .filter(entity::Column::Attempts.lt(self.config.max_attempts))
            .order_by_asc(entity::Column::CreatedAt)
            .limit(self.config.batch_size)
            .all(db.as_ref())
            .await
            .map_err(|e| ServiceError::infrastructure(format!("Failed to query outbox: {e}")))?;

        let mut claimed = Vec::new();

        for candidate in candidates {
            let updated = entity::ActiveModel {
                id: Set(candidate.id),
                status: Set(STATUS_PUBLISHING.to_string()),
                attempts: Set(candidate.attempts + 1),
                locked_by: Set(Some(self.config.worker_id.clone())),
                locked_until: Set(Some(lock_until)),
                updated_at: Set(now),
                ..Default::default()
            }
            .update(db.as_ref())
            .await
            .map_err(|e| {
                ServiceError::infrastructure(format!("Failed to claim outbox row: {e}"))
            })?;

            claimed.push(updated);
        }

        Ok(claimed)
    }

    async fn publish_claimed(&self, row: entity::Model) -> Result<(), ServiceError> {
        let event = StoredOutboxEvent::from_model(row);
        let event_id = event.event_id;
        let event_type = event.event_type.clone();
        let outbox_id = event.id;
        let boxed_event: Box<dyn DomainEvent> = Box::new(event);

        match self.publisher.publish(boxed_event.as_ref()).await {
            Ok(()) => {
                self.mark_published(outbox_id).await?;
                debug!(%event_id, %event_type, "Outbox event published");
                Ok(())
            }
            Err(error) => {
                warn!(%event_id, %event_type, error = ?error, "Outbox publish failed");
                self.mark_failed(outbox_id, format!("{error:?}")).await
            }
        }
    }

    async fn mark_published(&self, id: uuid::Uuid) -> Result<(), ServiceError> {
        let db = self.db.get_write_connection();
        entity::ActiveModel {
            id: Set(id),
            status: Set(STATUS_PUBLISHED.to_string()),
            locked_by: Set(None),
            locked_until: Set(None),
            last_error: Set(None),
            updated_at: Set(Utc::now()),
            ..Default::default()
        }
        .update(db.as_ref())
        .await
        .map_err(|e| {
            ServiceError::infrastructure(format!("Failed to mark outbox published: {e}"))
        })?;

        Ok(())
    }

    async fn mark_failed(&self, id: uuid::Uuid, error: String) -> Result<(), ServiceError> {
        let db = self.db.get_write_connection();
        let row = OutboxEvents::find_by_id(id)
            .one(db.as_ref())
            .await
            .map_err(|e| ServiceError::infrastructure(format!("Failed to reload outbox row: {e}")))?
            .ok_or_else(|| ServiceError::infrastructure("Outbox row disappeared during retry"))?;
        let retry_delay = self.retry_delay(row.attempts);
        let next_attempt_at = Utc::now()
            + chrono::Duration::from_std(retry_delay).map_err(|e| {
                ServiceError::infrastructure(format!("Invalid outbox retry delay: {e}"))
            })?;

        entity::ActiveModel {
            id: Set(id),
            status: Set(STATUS_FAILED.to_string()),
            next_attempt_at: Set(next_attempt_at),
            locked_by: Set(None),
            locked_until: Set(None),
            last_error: Set(Some(error)),
            updated_at: Set(Utc::now()),
            ..Default::default()
        }
        .update(db.as_ref())
        .await
        .map_err(|e| ServiceError::infrastructure(format!("Failed to mark outbox failed: {e}")))?;

        Ok(())
    }

    fn retry_delay(&self, attempts: i32) -> Duration {
        retry_delay_for(self.config.retry_base_delay, attempts)
    }
}

fn retry_delay_for(base_delay: Duration, attempts: i32) -> Duration {
    let capped = attempts.saturating_sub(1).clamp(0, 6);
    let exponent = u32::try_from(capped).unwrap_or(0);
    base_delay * 2_u32.pow(exponent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_delay_uses_capped_exponential_backoff() {
        assert_eq!(
            retry_delay_for(Duration::from_secs(3), 1),
            Duration::from_secs(3)
        );
        assert_eq!(
            retry_delay_for(Duration::from_secs(3), 2),
            Duration::from_secs(6)
        );
        assert_eq!(
            retry_delay_for(Duration::from_secs(3), 100),
            Duration::from_secs(192)
        );
    }
}
