use crate::event::{DomainEvent, EventPublisher};
use crate::{EventConsumer, EventHandler};
use async_trait::async_trait;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::message::{BorrowedMessage, Headers, Message};
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::util::Timeout;
use rustycog_config::KafkaConfig;
use rustycog_core::error::ServiceError;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Real Kafka event publisher implementation
pub struct KafkaEventPublisher {
    producer: FutureProducer,
    config: KafkaConfig,
}

impl KafkaEventPublisher {
    /// Create a new Kafka event publisher from configuration
    pub async fn new(config: KafkaConfig) -> Result<Self, ServiceError> {
        let producer = Self::create_producer(&config).await?;

        Ok(Self { producer, config })
    }

    /// Create a Kafka producer from configuration
    async fn create_producer(config: &KafkaConfig) -> Result<FutureProducer, ServiceError> {
        let mut client_config = ClientConfig::new();

        // Basic configuration
        client_config
            .set("bootstrap.servers", config.brokers())
            .set("client.id", &config.client_id)
            .set("message.timeout.ms", config.timeout_ms.to_string())
            .set("retries", config.max_retries.to_string())
            .set("compression.type", &config.compression);

        // Security configuration
        client_config.set("security.protocol", &config.security_protocol);

        // SASL configuration if provided (for plaintext SASL)
        if let Some(ref mechanism) = config.sasl_mechanism {
            client_config.set("sasl.mechanism", mechanism);
        }

        if let Some(ref username) = config.sasl_username {
            client_config.set("sasl.username", username);
        }

        if let Some(ref password) = config.sasl_password {
            client_config.set("sasl.password", password);
        }

        // SSL configuration for secure connections
        if config.security_protocol == "ssl" || config.security_protocol == "sasl_ssl" {
            // Set CA certificate location (default to system certificates if not specified)
            let ca_location = config.ssl_ca_location.as_deref().unwrap_or("probe");
            client_config.set("ssl.ca.location", ca_location);

            // Enable SSL certificate verification
            client_config.set("ssl.certificate.verification", "true");

            // Set SSL endpoint identification algorithm
            client_config.set("ssl.endpoint.identification.algorithm", "https");

            // Set client certificate and key if provided (for mutual TLS)
            if let Some(ref cert_location) = config.ssl_certificate_location {
                client_config.set("ssl.certificate.location", cert_location);
            }

            if let Some(ref key_location) = config.ssl_key_location {
                client_config.set("ssl.key.location", key_location);
            }

            if let Some(ref key_password) = config.ssl_key_password {
                client_config.set("ssl.key.password", key_password);
            }
        }

        // Producer-specific configuration
        client_config
            .set("acks", "all") // Wait for all replicas to acknowledge
            .set("enable.idempotence", "true") // Enable idempotent producer
            .set("max.in.flight.requests.per.connection", "5")
            .set("batch.size", "16384")
            .set("linger.ms", "5");

        client_config.create().map_err(|e| {
            ServiceError::infrastructure(format!("Failed to create Kafka producer: {e}"))
        })
    }

    /// Serialize domain event to JSON
    fn serialize_event(&self, event: &dyn DomainEvent) -> Result<String, ServiceError> {
        event
            .to_json()
            .map_err(|e| ServiceError::infrastructure(format!("Failed to serialize event: {e}")))
    }

    /// Get topic for event
    fn get_topic_for_event(&self, _event: &dyn DomainEvent) -> &str {
        // For now, all events go to the user events topic
        // In the future, we could route different event types to different topics
        &self.config.user_events_topic
    }
}

#[async_trait]
impl EventPublisher<ServiceError> for KafkaEventPublisher {
    async fn publish(&self, event: &dyn DomainEvent) -> Result<(), ServiceError> {
        if !self.config.enabled {
            debug!(
                event_id = %event.event_id(),
                event_type = %event.event_type(),
                "Kafka publishing disabled, skipping event"
            );
            return Ok(());
        }

        let topic = self.get_topic_for_event(event);
        let payload = self.serialize_event(event)?;
        let event_id = event.event_id().to_string();
        let aggregate_id = event.aggregate_id().to_string();

        debug!(
            event_id = %event.event_id(),
            event_type = %event.event_type(),
            aggregate_id = %event.aggregate_id(),
            topic = topic,
            "Publishing event to Kafka"
        );

        let record = FutureRecord::to(topic)
            .key(&aggregate_id) // Use aggregate_id as partition key for ordering
            .payload(&payload)
            .headers(
                rdkafka::message::OwnedHeaders::new()
                    .insert(rdkafka::message::Header {
                        key: "event_id",
                        value: Some(&event_id),
                    })
                    .insert(rdkafka::message::Header {
                        key: "event_type",
                        value: Some(event.event_type()),
                    })
                    .insert(rdkafka::message::Header {
                        key: "aggregate_id",
                        value: Some(&aggregate_id),
                    }),
            );

        let timeout = Timeout::After(Duration::from_millis(self.config.timeout_ms));

        match self.producer.send(record, timeout).await {
            Ok((partition, offset)) => {
                info!(
                    event_id = %event.event_id(),
                    event_type = %event.event_type(),
                    aggregate_id = %event.aggregate_id(),
                    partition = partition,
                    offset = offset,
                    topic = topic,
                    "✅ Event successfully published to Kafka"
                );
                Ok(())
            }
            Err((kafka_error, _)) => {
                error!(
                    event_id = %event.event_id(),
                    event_type = %event.event_type(),
                    aggregate_id = %event.aggregate_id(),
                    topic = topic,
                    error = %kafka_error,
                    "❌ Failed to publish event to Kafka"
                );
                Err(ServiceError::infrastructure(format!(
                    "Failed to publish event to Kafka: {kafka_error}"
                )))
            }
        }
    }

    async fn publish_batch(&self, events: &[Box<dyn DomainEvent>]) -> Result<(), ServiceError> {
        if !self.config.enabled {
            debug!(
                event_count = events.len(),
                "Kafka publishing disabled, skipping batch"
            );
            return Ok(());
        }

        debug!(
            event_count = events.len(),
            "Publishing batch of events to Kafka"
        );

        // Publish all events concurrently
        let futures: Vec<_> = events
            .iter()
            .map(|event| self.publish(event.as_ref()))
            .collect();

        // Wait for all to complete
        let results: Vec<Result<(), ServiceError>> = futures::future::join_all(futures).await;

        // Check for any failures
        let failures: Vec<_> = results
            .into_iter()
            .enumerate()
            .filter_map(|(i, result)| match result {
                Err(e) => Some((i, e)),
                Ok(()) => None,
            })
            .collect();

        if !failures.is_empty() {
            let failure_count = failures.len();
            let first_error = &failures[0].1;
            error!(
                failure_count = failure_count,
                first_error = %first_error,
                "Failed to publish some events in batch"
            );

            // Return the first error for simplicity
            // In production, you might want to return all errors or a summary
            return Err(ServiceError::infrastructure(format!(
                "Failed to publish {failure_count} events in batch. First error: {first_error}"
            )));
        }

        info!("✅ Successfully published all events in batch");
        Ok(())
    }

    async fn health_check(&self) -> Result<(), ServiceError> {
        if !self.config.enabled {
            debug!("Kafka publishing disabled, health check passed");
            return Ok(());
        }

        // Create a simple metadata request to check connectivity
        let _timeout = Duration::from_millis(self.config.timeout_ms);

        // This is a simple check - in production you might want to:
        // 1. Check cluster metadata
        // 2. Verify topic existence
        // 3. Test produce to a health check topic

        // For now, we'll just verify the producer is still healthy
        // by checking if we can create a new one with the same config
        match Self::create_producer(&self.config).await {
            Ok(_) => {
                debug!("Kafka health check passed");
                Ok(())
            }
            Err(e) => {
                error!(error = %e, "Kafka health check failed");
                Err(ServiceError::infrastructure(format!(
                    "Kafka health check failed: {e}"
                )))
            }
        }
    }
}

/// Kafka event consumer implementation
pub struct KafkaEventConsumer {
    consumer: StreamConsumer,
    config: KafkaConfig,
    should_stop: Arc<std::sync::atomic::AtomicBool>,
}

impl KafkaEventConsumer {
    /// Create a new Kafka event consumer from configuration
    pub async fn new(config: KafkaConfig) -> Result<Self, ServiceError> {
        let consumer = Self::create_consumer(&config).await?;

        Ok(Self {
            consumer,
            config,
            should_stop: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    /// Create a Kafka consumer from configuration
    async fn create_consumer(config: &KafkaConfig) -> Result<StreamConsumer, ServiceError> {
        let mut client_config = ClientConfig::new();

        // Basic configuration
        client_config
            .set("bootstrap.servers", config.brokers())
            .set("client.id", format!("{}-consumer", config.client_id))
            .set("group.id", format!("{}-group", config.client_id))
            .set("session.timeout.ms", "30000")
            .set("heartbeat.interval.ms", "10000")
            .set("max.poll.interval.ms", "300000")
            .set("auto.offset.reset", "earliest")
            .set("enable.auto.commit", "false"); // Manual commit for better control

        // Security configuration (same as producer)
        client_config.set("security.protocol", &config.security_protocol);

        if let Some(ref mechanism) = config.sasl_mechanism {
            client_config.set("sasl.mechanism", mechanism);
        }

        if let Some(ref username) = config.sasl_username {
            client_config.set("sasl.username", username);
        }

        if let Some(ref password) = config.sasl_password {
            client_config.set("sasl.password", password);
        }

        // SSL configuration
        if config.security_protocol == "ssl" || config.security_protocol == "sasl_ssl" {
            let ca_location = config.ssl_ca_location.as_deref().unwrap_or("probe");
            client_config.set("ssl.ca.location", ca_location);
            client_config.set("ssl.certificate.verification", "true");
            client_config.set("ssl.endpoint.identification.algorithm", "https");

            if let Some(ref cert_location) = config.ssl_certificate_location {
                client_config.set("ssl.certificate.location", cert_location);
            }

            if let Some(ref key_location) = config.ssl_key_location {
                client_config.set("ssl.key.location", key_location);
            }

            if let Some(ref key_password) = config.ssl_key_password {
                client_config.set("ssl.key.password", key_password);
            }
        }

        let consumer: StreamConsumer = client_config.create().map_err(|e| {
            ServiceError::infrastructure(format!("Failed to create Kafka consumer: {e}"))
        })?;

        // Subscribe to the user events topic
        consumer
            .subscribe(&[&config.user_events_topic])
            .map_err(|e| {
                ServiceError::infrastructure(format!("Failed to subscribe to Kafka topic: {e}"))
            })?;

        Ok(consumer)
    }

    /// Parse Kafka message into a domain event
    fn parse_message(
        &self,
        message: &BorrowedMessage,
    ) -> Result<Box<dyn DomainEvent>, ServiceError> {
        let payload = message.payload().ok_or_else(|| {
            ServiceError::infrastructure("Kafka message has no payload".to_string())
        })?;

        let payload_str = std::str::from_utf8(payload).map_err(|e| {
            ServiceError::infrastructure(format!(
                "Failed to parse Kafka message payload as UTF-8: {e}"
            ))
        })?;

        // Parse the JSON payload
        let data: Value = serde_json::from_str(payload_str).map_err(|e| {
            ServiceError::infrastructure(format!("Failed to parse Kafka message as JSON: {e}"))
        })?;

        // Extract metadata from headers if available
        let mut metadata = std::collections::HashMap::new();
        if let Some(headers) = message.headers() {
            for header in headers.iter() {
                if let Some(value) = header.value {
                    if let Ok(value_str) = std::str::from_utf8(value) {
                        metadata.insert(header.key.to_string(), value_str.to_string());
                    }
                }
            }
        }

        // Try to extract event information from headers first, then fallback to payload
        let event_id = metadata
            .get("event_id")
            .map(std::string::String::as_str)
            .or_else(|| data.get("event_id").and_then(|v| v.as_str()))
            .ok_or_else(|| {
                ServiceError::infrastructure("Missing event_id in Kafka message".to_string())
            })?;

        let event_type = metadata
            .get("event_type")
            .map(std::string::String::as_str)
            .or_else(|| data.get("event_type").and_then(|v| v.as_str()))
            .ok_or_else(|| {
                ServiceError::infrastructure("Missing event_type in Kafka message".to_string())
            })?;

        let aggregate_id = metadata
            .get("aggregate_id")
            .map(std::string::String::as_str)
            .or_else(|| data.get("aggregate_id").and_then(|v| v.as_str()))
            .ok_or_else(|| {
                ServiceError::infrastructure("Missing aggregate_id in Kafka message".to_string())
            })?;

        let occurred_at = data
            .get("occurred_at")
            .and_then(|v| v.as_str())
            .unwrap_or("1970-01-01T00:00:00Z");

        let raw_version = data
            .get("version")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(1);
        let version = i32::try_from(raw_version).map_err(|_| {
            ServiceError::infrastructure(format!("Kafka event version out of range: {raw_version}"))
        })?;

        let event_data = data.get("data").unwrap_or(&data).clone();

        let json_metadata: serde_json::Map<String, Value> = metadata
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect();

        // Create a generic domain event from the parsed data
        let event = KafkaGenericDomainEvent {
            event_id: event_id.to_string(),
            event_type: event_type.to_string(),
            aggregate_id: aggregate_id.to_string(),
            occurred_at: occurred_at.to_string(),
            version,
            data: event_data,
            metadata: json_metadata,
        };

        Ok(Box::new(event))
    }

    /// Poll for messages and handle them
    async fn poll_and_handle_messages<H>(&self, handler: &H) -> Result<(), ServiceError>
    where
        H: EventHandler + Send + Sync,
    {
        use rdkafka::consumer::Consumer; // Bring trait into scope for recv()

        // Poll with timeout
        match tokio::time::timeout(Duration::from_secs(30), self.consumer.recv()).await {
            Ok(Ok(message)) => {
                match self.parse_message(&message) {
                    Ok(event) => {
                        if handler.supports_event_type(event.event_type()) {
                            if let Err(e) = handler.handle_event(event).await {
                                warn!(
                                    error = %e,
                                    topic = message.topic(),
                                    partition = message.partition(),
                                    offset = message.offset(),
                                    "Failed to handle message"
                                );
                                return Ok(()); // Don't commit this message
                            }
                        } else {
                            debug!(
                                event_type = event.event_type(),
                                "Handler doesn't support event type, skipping"
                            );
                        }

                        // Commit the message after successful processing
                        if let Err(e) = self
                            .consumer
                            .commit_message(&message, rdkafka::consumer::CommitMode::Sync)
                        {
                            warn!(
                                error = %e,
                                topic = message.topic(),
                                partition = message.partition(),
                                offset = message.offset(),
                                "Failed to commit message"
                            );
                        }
                    }
                    Err(e) => {
                        warn!(
                            error = %e,
                            topic = message.topic(),
                            partition = message.partition(),
                            offset = message.offset(),
                            "Failed to parse message"
                        );
                        // Still commit to avoid reprocessing bad messages
                        let _ = self
                            .consumer
                            .commit_message(&message, rdkafka::consumer::CommitMode::Sync);
                    }
                }
            }
            Ok(Err(e)) => {
                error!(error = %e, "Error receiving Kafka message");
                return Err(ServiceError::infrastructure(format!(
                    "Kafka receive error: {e}"
                )));
            }
            Err(_) => {
                // Timeout - this is normal, just continue polling
                debug!("Kafka polling timeout, continuing");
            }
        }

        Ok(())
    }
}

#[async_trait]
impl EventConsumer for KafkaEventConsumer {
    async fn start<H>(&self, handler: H) -> Result<(), ServiceError>
    where
        H: EventHandler + Send + Sync + 'static,
    {
        if !self.config.enabled {
            info!("Kafka consumer disabled, not starting");
            return Ok(());
        }

        info!("Starting Kafka event consumer");
        self.should_stop
            .store(false, std::sync::atomic::Ordering::SeqCst);

        let handler = Arc::new(handler);

        while !self.should_stop.load(std::sync::atomic::Ordering::SeqCst) {
            if let Err(e) = self.poll_and_handle_messages(handler.as_ref()).await {
                error!(error = %e, "Error polling Kafka messages");
                // Sleep for a bit before retrying to avoid tight loop on errors
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }

        info!("Kafka event consumer stopped");
        Ok(())
    }

    async fn stop(&self) -> Result<(), ServiceError> {
        info!("Stopping Kafka event consumer");
        self.should_stop
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    async fn health_check(&self) -> Result<(), ServiceError> {
        if !self.config.enabled {
            return Ok(());
        }

        debug!("Performing Kafka consumer health check");

        // Try to get metadata as a health check
        match self.consumer.fetch_metadata(
            Some(&self.config.user_events_topic),
            Duration::from_secs(10),
        ) {
            Ok(_) => {
                debug!("✅ Kafka consumer health check passed");
                Ok(())
            }
            Err(e) => {
                error!(
                    error = %e,
                    topic = self.config.user_events_topic,
                    "❌ Kafka consumer health check failed"
                );
                Err(ServiceError::infrastructure(format!(
                    "Kafka consumer health check failed: {e}"
                )))
            }
        }
    }
}

/// Generic domain event implementation for parsing Kafka messages
#[derive(Debug, Clone)]
struct KafkaGenericDomainEvent {
    event_id: String,
    event_type: String,
    aggregate_id: String,
    occurred_at: String,
    version: i32,
    data: Value,
    metadata: serde_json::Map<String, Value>,
}

impl DomainEvent for KafkaGenericDomainEvent {
    fn event_id(&self) -> uuid::Uuid {
        uuid::Uuid::parse_str(&self.event_id).unwrap_or_else(|_| uuid::Uuid::new_v4())
    }

    fn event_type(&self) -> &str {
        &self.event_type
    }

    fn aggregate_id(&self) -> uuid::Uuid {
        uuid::Uuid::parse_str(&self.aggregate_id).unwrap_or_else(|_| uuid::Uuid::new_v4())
    }

    fn occurred_at(&self) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(&self.occurred_at)
            .map_or_else(|_| chrono::Utc::now(), |dt| dt.with_timezone(&chrono::Utc))
    }

    fn version(&self) -> u32 {
        u32::try_from(self.version.max(0)).unwrap_or(0)
    }

    fn to_json(&self) -> Result<String, ServiceError> {
        serde_json::to_string(&self.data).map_err(|e| {
            ServiceError::infrastructure(format!("Failed to serialize event data: {e}"))
        })
    }

    fn metadata(&self) -> std::collections::HashMap<String, String> {
        self.metadata
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect()
    }
}
