//! Generic adapter system for integrating different domain types with rustycog-events
//!
//! This module provides a flexible adapter pattern that allows any project to integrate
//! their own domain events and error types with rustycog-events, while maintaining
//! type safety and allowing custom mappings.

use crate::ConcreteEventPublisher;
use async_trait::async_trait;
use rustycog_core::error::ServiceError;
use std::collections::HashSet;
use std::sync::Arc;

use crate::event::{DomainEvent, EventPublisher};

/// Trait for bidirectional mapping between custom error types and `ServiceError`
pub trait ErrorMapper<E>: Send + Sync {
    /// Map a custom error type to `ServiceError`
    fn to_service_error(&self, error: E) -> ServiceError;

    /// Map a `ServiceError` back to custom error type
    fn from_service_error(&self, error: ServiceError) -> E;
}

/// Generic event publisher adapter that can work with any domain event and error type
pub struct GenericEventPublisherAdapter<TError> {
    inner: Arc<ConcreteEventPublisher>,
    error_mapper: Arc<dyn ErrorMapper<TError>>,
}

impl<TError> GenericEventPublisherAdapter<TError> {
    /// Create a new generic event publisher adapter
    pub fn new(
        inner: Arc<ConcreteEventPublisher>,
        error_mapper: Arc<dyn ErrorMapper<TError>>,
    ) -> Self {
        Self {
            inner,
            error_mapper,
        }
    }

    /// Publish a single event
    pub async fn publish(&self, event: &dyn DomainEvent) -> Result<(), TError> {
        tracing::info!("Publishing event: {:?}", event);

        self.inner
            .publish(event)
            .await
            .map_err(|service_error| self.error_mapper.from_service_error(service_error))
    }

    /// Publish multiple events in a batch
    pub async fn publish_batch(&self, events: &[Box<dyn DomainEvent>]) -> Result<(), TError> {
        self.inner
            .publish_batch(events)
            .await
            .map_err(|service_error| self.error_mapper.from_service_error(service_error))
    }

    /// Health check
    pub async fn health_check(&self) -> Result<(), TError> {
        self.inner
            .health_check()
            .await
            .map_err(|service_error| self.error_mapper.from_service_error(service_error))
    }
}

/// Multi-queue event publisher that can publish to multiple queues
pub struct MultiQueueEventPublisher<TError> {
    publishers: Vec<GenericEventPublisherAdapter<TError>>,
    queue_names: HashSet<String>,
}

impl<TError> MultiQueueEventPublisher<TError> {
    /// Create a new multi-queue event publisher
    #[must_use]
    pub const fn new(
        publishers: Vec<GenericEventPublisherAdapter<TError>>,
        queue_names: HashSet<String>,
    ) -> Self {
        Self {
            publishers,
            queue_names,
        }
    }

    /// Check if this publisher handles the given queue name
    #[must_use]
    pub fn handles_queue(&self, queue_name: &str) -> bool {
        self.queue_names.is_empty() || self.queue_names.contains(queue_name)
    }

    /// Get the queue names this publisher handles
    #[must_use]
    pub const fn queue_names(&self) -> &HashSet<String> {
        &self.queue_names
    }
}

#[async_trait]
impl<TError> EventPublisher<TError> for MultiQueueEventPublisher<TError> {
    async fn publish(&self, event: &dyn DomainEvent) -> Result<(), TError> {
        // Publish to all configured publishers
        for publisher in &self.publishers {
            publisher.publish(event).await?;
        }
        Ok(())
    }

    async fn publish_batch(&self, events: &[Box<dyn DomainEvent>]) -> Result<(), TError> {
        for publisher in &self.publishers {
            publisher.publish_batch(events).await?;
        }
        Ok(())
    }

    async fn health_check(&self) -> Result<(), TError> {
        // Check health of all publishers
        for publisher in &self.publishers {
            publisher.health_check().await?;
        }
        Ok(())
    }
}
