use crate::event::{DomainEvent, EventPublisher};
use crate::{EventConsumer, EventHandler};
use async_trait::async_trait;
use rustycog_core::error::ServiceError;
use tracing;

/// No-op event publisher for testing and development
///
/// This publisher doesn't actually publish events anywhere,
/// but provides a valid implementation for development environments
/// where event publishing is not needed.
pub struct NoOpEventPublisher;

impl NoOpEventPublisher {
    /// Create a new `NoOpEventPublisher`
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for NoOpEventPublisher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventPublisher<ServiceError> for NoOpEventPublisher {
    async fn publish(&self, event: &dyn DomainEvent) -> Result<(), ServiceError> {
        // Log the event but don't actually publish it
        tracing::debug!(
            event_id = %event.event_id(),
            event_type = %event.event_type(),
            aggregate_id = %event.aggregate_id(),
            "Event would be published (no-op mode)"
        );
        Ok(())
    }

    async fn publish_batch(&self, events: &[Box<dyn DomainEvent>]) -> Result<(), ServiceError> {
        for event in events {
            self.publish(event.as_ref()).await?;
        }
        Ok(())
    }

    async fn health_check(&self) -> Result<(), ServiceError> {
        // Always healthy since no-op
        Ok(())
    }
}

/// No-op event consumer for testing and development
///
/// This consumer doesn't actually consume events from anywhere,
/// but provides a valid implementation for development environments
/// where event consumption is not needed.
pub struct NoOpEventConsumer;

impl NoOpEventConsumer {
    /// Create a new `NoOpEventConsumer`
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for NoOpEventConsumer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventConsumer for NoOpEventConsumer {
    async fn start<H>(&self, _handler: H) -> Result<(), ServiceError>
    where
        H: EventHandler + Send + Sync + 'static,
    {
        tracing::info!("No-op event consumer started (no events will be consumed)");
        Ok(())
    }

    async fn stop(&self) -> Result<(), ServiceError> {
        tracing::info!("No-op event consumer stopped");
        Ok(())
    }

    async fn health_check(&self) -> Result<(), ServiceError> {
        // Always healthy since no-op
        Ok(())
    }
}
