//! # `RustyCog` Events
//!
//! Event publishing and subscription utilities.

use std::sync::Once;

static CRYPTO_PROVIDER_INIT: Once = Once::new();

/// Initialize the crypto provider for rustls
/// This is called automatically when the library is first used
fn init_crypto_provider() {
    CRYPTO_PROVIDER_INIT.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

pub mod adapter;
pub mod event;
pub mod kafka;
pub mod no_op;
pub mod sqs;

use async_trait::async_trait;
use rustycog_config::{KafkaConfig, QueueConfig, SqsConfig};
use rustycog_core::error::ServiceError;
use std::collections::HashSet;
use std::sync::Arc;

pub use adapter::*;
pub use event::*;
pub use kafka::{KafkaEventConsumer, KafkaEventPublisher};
pub use no_op::*;
pub use sqs::{SqsEventConsumer, SqsEventPublisher};

/// Concrete event publisher that can be Kafka, SQS, or `NoOp`
pub enum ConcreteEventPublisher {
    Kafka(KafkaEventPublisher),
    Sqs(SqsEventPublisher),
    NoOp(Arc<dyn EventPublisher<ServiceError>>),
}

impl ConcreteEventPublisher {
    pub async fn new(config: &QueueConfig) -> Result<Self, ServiceError> {
        match config {
            QueueConfig::Kafka(kafka_config) => Ok(Self::Kafka(
                KafkaEventPublisher::new(kafka_config.clone()).await?,
            )),
            QueueConfig::Sqs(sqs_config) => {
                Ok(Self::Sqs(SqsEventPublisher::new(sqs_config.clone()).await?))
            }
            QueueConfig::Disabled => Ok(Self::NoOp(Arc::new(NoOpEventPublisher::new()))),
        }
    }
}

#[async_trait]
impl EventPublisher<ServiceError> for ConcreteEventPublisher {
    async fn publish(&self, event: &dyn DomainEvent) -> Result<(), ServiceError> {
        match self {
            Self::Kafka(kafka) => kafka.publish(event).await,
            Self::Sqs(sqs) => sqs.publish(event).await,
            Self::NoOp(no_op) => no_op.publish(event).await,
        }
    }

    async fn publish_batch(&self, events: &[Box<dyn DomainEvent>]) -> Result<(), ServiceError> {
        match self {
            Self::Kafka(kafka) => kafka.publish_batch(events).await,
            Self::Sqs(sqs) => sqs.publish_batch(events).await,
            Self::NoOp(no_op) => no_op.publish_batch(events).await,
        }
    }

    async fn health_check(&self) -> Result<(), ServiceError> {
        match self {
            Self::Kafka(kafka) => kafka.health_check().await,
            Self::Sqs(sqs) => sqs.health_check().await,
            Self::NoOp(no_op) => no_op.health_check().await,
        }
    }
}

/// Check if a Kafka test container is currently running
#[cfg(any(test, feature = "test-utils"))]
fn is_test_kafka_container_running() -> bool {
    // The kafka_testcontainer.rs sets these environment variables when a container is started
    // We check for these specific test environment variables to detect if a test container is active
    std::env::var("RUSTYCOG_KAFKA__HOST").is_ok()
        && std::env::var("RUSTYCOG_KAFKA__PORT").is_ok()
        && std::env::var("RUSTYCOG_KAFKA__ENABLED").is_ok_and(|v| v == "true")
}

/// Check if we're running in test mode
const fn is_test_mode() -> bool {
    cfg!(test) || cfg!(feature = "test-utils")
}

/// Factory function to create an event publisher based on queue configuration
pub async fn create_event_publisher_from_queue_config(
    config: &QueueConfig,
) -> Result<Arc<ConcreteEventPublisher>, ServiceError> {
    match config {
        QueueConfig::Kafka(kafka_config) => create_kafka_event_publisher(kafka_config).await,
        QueueConfig::Sqs(_sqs_config) => create_sqs_event_publisher(_sqs_config).await,
        QueueConfig::Disabled => {
            tracing::info!("Queue disabled, using no-op event publisher");
            Ok(Arc::new(ConcreteEventPublisher::NoOp(Arc::new(
                NoOpEventPublisher::new(),
            ))))
        }
    }
}

/// Factory function to create a Kafka event publisher based on configuration (legacy support)
pub async fn create_event_publisher(
    config: &KafkaConfig,
) -> Result<Arc<ConcreteEventPublisher>, ServiceError> {
    create_kafka_event_publisher(config).await
}

/// Factory function to create a Kafka event publisher
pub async fn create_kafka_event_publisher(
    config: &KafkaConfig,
) -> Result<Arc<ConcreteEventPublisher>, ServiceError> {
    // In test mode, only use Kafka if explicitly enabled AND a test container is running
    if is_test_mode() {
        #[cfg(any(test, feature = "test-utils"))]
        {
            if config.enabled && is_test_kafka_container_running() {
                tracing::info!(
                    "Test mode: Test Kafka container detected, using Kafka event publisher"
                );
                match KafkaEventPublisher::new(config.clone()).await {
                    Ok(publisher) => {
                        return Ok(Arc::new(ConcreteEventPublisher::Kafka(publisher)));
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to create Kafka event publisher in test mode, falling back to no-op: {}",
                            e
                        );
                        return Ok(Arc::new(ConcreteEventPublisher::NoOp(Arc::new(
                            NoOpEventPublisher::new(),
                        ))));
                    }
                }
            }
            tracing::info!(
                "Test mode: No Kafka test container detected or Kafka disabled, using no-op event publisher"
            );
            return Ok(Arc::new(ConcreteEventPublisher::NoOp(Arc::new(
                NoOpEventPublisher::new(),
            ))));
        }

        #[cfg(not(any(test, feature = "test-utils")))]
        {
            // This branch should never be reached due to is_test_mode() check above,
            // but included for completeness
            tracing::info!(
                "Test mode detected but test-utils feature not available, using no-op event publisher"
            );
            return Ok(Arc::new(ConcreteEventPublisher::NoOp(Arc::new(
                NoOpEventPublisher::new(),
            ))));
        }
    }

    // Production mode: use the original logic
    if config.enabled {
        match KafkaEventPublisher::new(config.clone()).await {
            Ok(publisher) => {
                tracing::info!("Created Kafka event publisher");
                Ok(Arc::new(ConcreteEventPublisher::Kafka(publisher)))
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to create Kafka event publisher, falling back to no-op: {}",
                    e
                );
                // Fall back to no-op publisher if Kafka creation fails
                Ok(Arc::new(ConcreteEventPublisher::NoOp(Arc::new(
                    NoOpEventPublisher::new(),
                ))))
            }
        }
    } else {
        tracing::info!("Kafka disabled, using no-op event publisher");
        Ok(Arc::new(ConcreteEventPublisher::NoOp(Arc::new(
            NoOpEventPublisher::new(),
        ))))
    }
}

/// Factory function to create an SQS event publisher
pub async fn create_sqs_event_publisher(
    config: &SqsConfig,
) -> Result<Arc<ConcreteEventPublisher>, ServiceError> {
    // Initialize crypto provider before any AWS SDK usage
    init_crypto_provider();

    // In test mode, only use SQS if explicitly enabled
    if is_test_mode() {
        #[cfg(any(test, feature = "test-utils"))]
        {
            if config.enabled {
                tracing::info!("Test mode: SQS enabled, using SQS event publisher");
                match SqsEventPublisher::new(config.clone()).await {
                    Ok(publisher) => {
                        return Ok(Arc::new(ConcreteEventPublisher::Sqs(publisher)));
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to create SQS event publisher in test mode, falling back to no-op: {}",
                            e
                        );
                        return Ok(Arc::new(ConcreteEventPublisher::NoOp(Arc::new(
                            NoOpEventPublisher::new(),
                        ))));
                    }
                }
            }
            tracing::info!("Test mode: SQS disabled, using no-op event publisher");
            return Ok(Arc::new(ConcreteEventPublisher::NoOp(Arc::new(
                NoOpEventPublisher::new(),
            ))));
        }

        #[cfg(not(any(test, feature = "test-utils")))]
        {
            tracing::info!(
                "Test mode detected but test-utils feature not available, using no-op event publisher"
            );
            return Ok(Arc::new(ConcreteEventPublisher::NoOp(Arc::new(
                NoOpEventPublisher::new(),
            ))));
        }
    }

    // Production mode: use the original logic
    if config.enabled {
        match SqsEventPublisher::new(config.clone()).await {
            Ok(publisher) => {
                tracing::info!("Created SQS event publisher");
                Ok(Arc::new(ConcreteEventPublisher::Sqs(publisher)))
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to create SQS event publisher, falling back to no-op: {}",
                    e
                );
                // Fall back to no-op publisher if SQS creation fails
                Ok(Arc::new(ConcreteEventPublisher::NoOp(Arc::new(
                    NoOpEventPublisher::new(),
                ))))
            }
        }
    } else {
        tracing::info!("SQS disabled, using no-op event publisher");
        Ok(Arc::new(ConcreteEventPublisher::NoOp(Arc::new(
            NoOpEventPublisher::new(),
        ))))
    }
}

pub async fn create_multi_queue_event_publisher<TError>(
    config: &QueueConfig,
    queue_names: Option<HashSet<String>>,
    error_mapper: Arc<dyn ErrorMapper<TError>>,
) -> Result<Arc<MultiQueueEventPublisher<TError>>, TError> {
    let queue_names = queue_names.unwrap_or_else(|| {
        // If no specific queue names provided, use all configured queues
        match config {
            QueueConfig::Disabled => HashSet::new(),
            QueueConfig::Sqs(sqs_config) => sqs_config.all_queue_names(),
            QueueConfig::Kafka(kafka_config) => {
                let mut all_queues = HashSet::new();
                all_queues.insert(kafka_config.user_events_topic.clone());
                all_queues
            }
        }
    });

    let adapted_publisher = create_event_publisher_from_queue_config(config)
        .await
        .map_err(|service_error| error_mapper.from_service_error(service_error))?;
    let publisher = GenericEventPublisherAdapter::<TError>::new(adapted_publisher, error_mapper);

    Ok(Arc::new(MultiQueueEventPublisher::new(
        vec![publisher],
        queue_names,
    )))
}

// =============================================================================
// Event Consumers
// =============================================================================

/// Trait for consuming events from queues
#[async_trait]
pub trait EventConsumer: Send + Sync {
    /// Start consuming events
    async fn start<H>(&self, handler: H) -> Result<(), ServiceError>
    where
        H: EventHandler + Send + Sync + 'static;

    /// Stop consuming events
    async fn stop(&self) -> Result<(), ServiceError>;

    /// Health check for the consumer
    async fn health_check(&self) -> Result<(), ServiceError>;
}

/// Trait for handling consumed events
#[async_trait]
pub trait EventHandler: Send + Sync {
    /// Handle a single event
    async fn handle_event(&self, event: Box<dyn DomainEvent>) -> Result<(), ServiceError>;

    /// Check if this handler supports the given event type
    fn supports_event_type(&self, event_type: &str) -> bool;
}

/// Concrete event consumer that can be Kafka, SQS, or `NoOp`
pub enum ConcreteEventConsumer {
    Kafka(KafkaEventConsumer),
    Sqs(SqsEventConsumer),
    NoOp(NoOpEventConsumer),
}

#[async_trait]
impl EventConsumer for ConcreteEventConsumer {
    async fn start<H>(&self, handler: H) -> Result<(), ServiceError>
    where
        H: EventHandler + Send + Sync + 'static,
    {
        match self {
            Self::Kafka(kafka) => kafka.start(handler).await,
            Self::Sqs(sqs) => sqs.start(handler).await,
            Self::NoOp(no_op) => no_op.start(handler).await,
        }
    }

    async fn stop(&self) -> Result<(), ServiceError> {
        match self {
            Self::Kafka(kafka) => kafka.stop().await,
            Self::Sqs(sqs) => sqs.stop().await,
            Self::NoOp(no_op) => no_op.stop().await,
        }
    }

    async fn health_check(&self) -> Result<(), ServiceError> {
        match self {
            Self::Kafka(kafka) => kafka.health_check().await,
            Self::Sqs(sqs) => sqs.health_check().await,
            Self::NoOp(no_op) => no_op.health_check().await,
        }
    }
}

/// Factory function to create an event consumer based on queue configuration
pub async fn create_event_consumer_from_queue_config(
    config: &QueueConfig,
) -> Result<Arc<ConcreteEventConsumer>, ServiceError> {
    match config {
        QueueConfig::Kafka(kafka_config) => create_kafka_event_consumer(kafka_config).await,
        QueueConfig::Sqs(sqs_config) => create_sqs_event_consumer(sqs_config).await,
        QueueConfig::Disabled => {
            tracing::info!("Queue disabled, using no-op event consumer");
            Ok(Arc::new(ConcreteEventConsumer::NoOp(
                NoOpEventConsumer::new(),
            )))
        }
    }
}

/// Factory function to create a Kafka event consumer
pub async fn create_kafka_event_consumer(
    config: &KafkaConfig,
) -> Result<Arc<ConcreteEventConsumer>, ServiceError> {
    // In test mode, only use Kafka if explicitly enabled AND a test container is running
    if is_test_mode() {
        #[cfg(any(test, feature = "test-utils"))]
        {
            if config.enabled && is_test_kafka_container_running() {
                tracing::info!(
                    "Test mode: Test Kafka container detected, using Kafka event consumer"
                );
                match KafkaEventConsumer::new(config.clone()).await {
                    Ok(consumer) => {
                        return Ok(Arc::new(ConcreteEventConsumer::Kafka(consumer)));
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to create Kafka event consumer in test mode, falling back to no-op: {}",
                            e
                        );
                        return Ok(Arc::new(ConcreteEventConsumer::NoOp(
                            NoOpEventConsumer::new(),
                        )));
                    }
                }
            }
            tracing::info!(
                "Test mode: No Kafka test container detected or Kafka disabled, using no-op event consumer"
            );
            return Ok(Arc::new(ConcreteEventConsumer::NoOp(
                NoOpEventConsumer::new(),
            )));
        }

        #[cfg(not(any(test, feature = "test-utils")))]
        {
            tracing::info!(
                "Test mode detected but test-utils feature not available, using no-op event consumer"
            );
            return Ok(Arc::new(ConcreteEventConsumer::NoOp(
                NoOpEventConsumer::new(),
            )));
        }
    }

    // Production mode: use the original logic
    if config.enabled {
        match KafkaEventConsumer::new(config.clone()).await {
            Ok(consumer) => {
                tracing::info!("Created Kafka event consumer");
                Ok(Arc::new(ConcreteEventConsumer::Kafka(consumer)))
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to create Kafka event consumer, falling back to no-op: {}",
                    e
                );
                // Fall back to no-op consumer if Kafka creation fails
                Ok(Arc::new(ConcreteEventConsumer::NoOp(
                    NoOpEventConsumer::new(),
                )))
            }
        }
    } else {
        tracing::info!("Kafka disabled, using no-op event consumer");
        Ok(Arc::new(ConcreteEventConsumer::NoOp(
            NoOpEventConsumer::new(),
        )))
    }
}

/// Factory function to create an SQS event consumer
pub async fn create_sqs_event_consumer(
    config: &SqsConfig,
) -> Result<Arc<ConcreteEventConsumer>, ServiceError> {
    // Initialize crypto provider before any AWS SDK usage
    init_crypto_provider();

    // In test mode, only use SQS if explicitly enabled
    if is_test_mode() {
        #[cfg(any(test, feature = "test-utils"))]
        {
            if config.enabled {
                tracing::info!("Test mode: SQS enabled, using SQS event consumer");
                match SqsEventConsumer::new(config.clone()).await {
                    Ok(consumer) => {
                        return Ok(Arc::new(ConcreteEventConsumer::Sqs(consumer)));
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to create SQS event consumer in test mode, falling back to no-op: {}",
                            e
                        );
                        return Ok(Arc::new(ConcreteEventConsumer::NoOp(
                            NoOpEventConsumer::new(),
                        )));
                    }
                }
            }
            tracing::info!("Test mode: SQS disabled, using no-op event consumer");
            return Ok(Arc::new(ConcreteEventConsumer::NoOp(
                NoOpEventConsumer::new(),
            )));
        }

        #[cfg(not(any(test, feature = "test-utils")))]
        {
            tracing::info!(
                "Test mode detected but test-utils feature not available, using no-op event consumer"
            );
            return Ok(Arc::new(ConcreteEventConsumer::NoOp(
                NoOpEventConsumer::new(),
            )));
        }
    }

    // Production mode: use the original logic
    if config.enabled {
        match SqsEventConsumer::new(config.clone()).await {
            Ok(consumer) => {
                tracing::info!("Created SQS event consumer");
                Ok(Arc::new(ConcreteEventConsumer::Sqs(consumer)))
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to create SQS event consumer, falling back to no-op: {}",
                    e
                );
                // Fall back to no-op consumer if SQS creation fails
                Ok(Arc::new(ConcreteEventConsumer::NoOp(
                    NoOpEventConsumer::new(),
                )))
            }
        }
    } else {
        tracing::info!("SQS disabled, using no-op event consumer");
        Ok(Arc::new(ConcreteEventConsumer::NoOp(
            NoOpEventConsumer::new(),
        )))
    }
}
