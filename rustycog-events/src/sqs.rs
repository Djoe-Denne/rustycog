use crate::event::{DomainEvent, EventPublisher};
use crate::{EventConsumer, EventHandler};
use async_trait::async_trait;
use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_sqs::{types::Message, Client, Config};
use rustycog_config::SqsConfig;
use rustycog_core::error::ServiceError;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, warn};

type SqsBatchEntry = aws_sdk_sqs::types::SendMessageBatchRequestEntry;
type SqsBatchEntriesByQueue = HashMap<String, Vec<SqsBatchEntry>>;

/// SQS event publisher implementation
pub struct SqsEventPublisher {
    client: Client,
    config: SqsConfig,
}

impl SqsEventPublisher {
    /// Create a new SQS event publisher from configuration
    pub async fn new(config: SqsConfig) -> Result<Self, ServiceError> {
        let client = Self::create_client(&config).await?;

        Ok(Self { client, config })
    }

    /// Create an SQS client from configuration
    async fn create_client(config: &SqsConfig) -> Result<Client, ServiceError> {
        let mut aws_config_builder = aws_config::defaults(BehaviorVersion::latest());

        // Set region
        aws_config_builder = aws_config_builder.region(Region::new(config.region.clone()));

        // Set endpoint if using localstack or custom endpoint (now using host/port configuration)
        if let Some(endpoint_url) = config.endpoint_url() {
            aws_config_builder = aws_config_builder.endpoint_url(endpoint_url);
        }

        // Set credentials if provided
        if let (Some(ref access_key), Some(ref secret_key)) =
            (&config.access_key_id, &config.secret_access_key)
        {
            let credentials = Credentials::new(
                access_key,
                secret_key,
                config.session_token.clone(),
                None,
                "rustycog-events",
            );
            aws_config_builder = aws_config_builder.credentials_provider(credentials);
        }

        let aws_config = aws_config_builder.load().await;
        let sqs_config = Config::from(&aws_config);
        let client = Client::from_conf(sqs_config);

        Ok(client)
    }

    /// Serialize domain event to SQS message body
    fn serialize_event(&self, event: &dyn DomainEvent) -> Result<String, ServiceError> {
        // Get the event JSON and parse it back to a Value so it's properly structured in the data field
        let event_json_str = event.to_json()?;
        let event_data: serde_json::Value = serde_json::from_str(&event_json_str).map_err(|e| {
            ServiceError::infrastructure(format!("Failed to parse event JSON: {e}"))
        })?;

        let message_body = json!({
            "event_id": event.event_id(),
            "event_type": event.event_type(),
            "aggregate_id": event.aggregate_id(),
            "occurred_at": event.occurred_at(),
            "version": event.version(),
            "data": event_data,
            "metadata": event.metadata()
        });

        serde_json::to_string(&message_body).map_err(|e| {
            ServiceError::infrastructure(format!("Failed to serialize event for SQS: {e}"))
        })
    }

    /// Get destination queue names for an event.
    fn get_queue_names_for_event(
        &self,
        event: &dyn DomainEvent,
    ) -> Result<Vec<String>, ServiceError> {
        let queue_names = self.config.get_queue_names(event.event_type());
        if queue_names.is_empty() {
            return Err(ServiceError::infrastructure(format!(
                "No SQS destination queues configured for event type '{}'",
                event.event_type()
            )));
        }

        Ok(queue_names.into_iter().map(str::to_string).collect())
    }

    /// Create message attributes for the event
    fn create_message_attributes(
        &self,
        event: &dyn DomainEvent,
    ) -> std::collections::HashMap<String, aws_sdk_sqs::types::MessageAttributeValue> {
        let mut attributes = std::collections::HashMap::new();

        attributes.insert(
            "event_id".to_string(),
            aws_sdk_sqs::types::MessageAttributeValue::builder()
                .data_type("String")
                .string_value(event.event_id().to_string())
                .build()
                .unwrap(),
        );

        attributes.insert(
            "event_type".to_string(),
            aws_sdk_sqs::types::MessageAttributeValue::builder()
                .data_type("String")
                .string_value(event.event_type())
                .build()
                .unwrap(),
        );

        attributes.insert(
            "aggregate_id".to_string(),
            aws_sdk_sqs::types::MessageAttributeValue::builder()
                .data_type("String")
                .string_value(event.aggregate_id().to_string())
                .build()
                .unwrap(),
        );

        attributes.insert(
            "source".to_string(),
            aws_sdk_sqs::types::MessageAttributeValue::builder()
                .data_type("String")
                .string_value("rustycog-events")
                .build()
                .unwrap(),
        );

        attributes
    }

    fn build_batch_entries(
        &self,
        events: &[Box<dyn DomainEvent>],
    ) -> (SqsBatchEntriesByQueue, Option<ServiceError>) {
        let mut entries_by_queue = HashMap::new();
        let mut first_error = None;

        for (idx, event) in events.iter().enumerate() {
            if let Err(error) =
                self.add_event_to_batch_entries(idx, event.as_ref(), &mut entries_by_queue)
            {
                first_error.get_or_insert(error);
            }
        }

        (entries_by_queue, first_error)
    }

    fn add_event_to_batch_entries(
        &self,
        idx: usize,
        event: &dyn DomainEvent,
        entries_by_queue: &mut SqsBatchEntriesByQueue,
    ) -> Result<(), ServiceError> {
        let queue_names = self.get_queue_names_for_event(event)?;
        let message_body = self.serialize_event(event)?;
        let message_attributes = self.create_message_attributes(event);

        for (queue_idx, queue_name) in queue_names.into_iter().enumerate() {
            let entry = self.build_batch_entry(
                event,
                format!("entry_{idx}_{queue_idx}"),
                message_body.clone(),
                message_attributes.clone(),
                &queue_name,
            )?;
            entries_by_queue.entry(queue_name).or_default().push(entry);
        }

        Ok(())
    }

    fn build_batch_entry(
        &self,
        event: &dyn DomainEvent,
        entry_id: String,
        message_body: String,
        message_attributes: HashMap<String, aws_sdk_sqs::types::MessageAttributeValue>,
        queue_name: &str,
    ) -> Result<SqsBatchEntry, ServiceError> {
        let mut entry = SqsBatchEntry::builder()
            .id(entry_id)
            .message_body(message_body)
            .set_message_attributes(Some(message_attributes));

        if self.config.is_fifo_queue(queue_name) {
            entry = entry
                .message_group_id(event.aggregate_id().to_string())
                .message_deduplication_id(event.event_id().to_string());
        }

        entry.build().map_err(|e| {
            ServiceError::infrastructure(format!("Failed to build SQS batch entry: {e}"))
        })
    }

    async fn send_batch_entries(
        &self,
        entries_by_queue: SqsBatchEntriesByQueue,
    ) -> Option<ServiceError> {
        let mut first_error = None;

        for (queue_name, entries) in entries_by_queue {
            let error = self.send_queue_batches(&queue_name, entries).await;
            first_error = first_error.or(error);
        }

        first_error
    }

    async fn send_queue_batches(
        &self,
        queue_name: &str,
        mut entries: Vec<SqsBatchEntry>,
    ) -> Option<ServiceError> {
        let queue_url = self.config.queue_url(queue_name);
        let mut first_error = None;

        while !entries.is_empty() {
            let batch_entries: Vec<_> = entries.drain(..entries.len().min(10)).collect();
            let error = self
                .send_single_batch(queue_name, &queue_url, batch_entries)
                .await;
            first_error = first_error.or(error);
        }

        first_error
    }

    async fn send_single_batch(
        &self,
        queue_name: &str,
        queue_url: &str,
        batch_entries: Vec<SqsBatchEntry>,
    ) -> Option<ServiceError> {
        match self
            .client
            .send_message_batch()
            .queue_url(queue_url)
            .set_entries(Some(batch_entries))
            .send()
            .await
        {
            Ok(response) => self.log_batch_response(queue_name, queue_url, response),
            Err(aws_error) => {
                error!(
                    queue_name = %queue_name,
                    queue_url = %queue_url,
                    error = %aws_error,
                    "Failed to send message batch to SQS"
                );
                Some(ServiceError::infrastructure(format!(
                    "Failed to send batch to SQS: {aws_error}"
                )))
            }
        }
    }

    fn log_batch_response(
        &self,
        queue_name: &str,
        queue_url: &str,
        response: aws_sdk_sqs::operation::send_message_batch::SendMessageBatchOutput,
    ) -> Option<ServiceError> {
        let failed = response.failed();
        if !failed.is_empty() {
            error!(
                failed_count = failed.len(),
                queue_name = %queue_name,
                queue_url = %queue_url,
                "Some messages failed to send in batch"
            );
            let first_failed = &failed[0];
            return Some(ServiceError::infrastructure(format!(
                "SQS batch send failed: {} - {}",
                first_failed.code(),
                first_failed.message().unwrap_or("no message")
            )));
        }

        let successful = response.successful();
        info!(
            successful_count = successful.len(),
            queue_name = %queue_name,
            queue_url = %queue_url,
            "Messages successfully sent in batch"
        );
        None
    }
}

#[async_trait]
impl EventPublisher<ServiceError> for SqsEventPublisher {
    async fn publish(&self, event: &dyn DomainEvent) -> Result<(), ServiceError> {
        if !self.config.enabled {
            debug!(
                event_id = %event.event_id(),
                event_type = %event.event_type(),
                "SQS publishing disabled, skipping event"
            );
            return Ok(());
        }

        let queue_names = self.get_queue_names_for_event(event)?;
        let message_body = self.serialize_event(event)?;
        let message_attributes = self.create_message_attributes(event);

        let mut first_error = None;
        for queue_name in queue_names {
            let queue_url = self.config.queue_url(&queue_name);

            debug!(
                event_id = %event.event_id(),
                event_type = %event.event_type(),
                aggregate_id = %event.aggregate_id(),
                queue_name = %queue_name,
                queue_url = %queue_url,
                "Publishing event to SQS"
            );

            let mut send_request = self
                .client
                .send_message()
                .queue_url(&queue_url)
                .message_body(message_body.clone())
                .set_message_attributes(Some(message_attributes.clone()));

            // Use aggregate_id as message group ID for FIFO queues
            if self.config.is_fifo_queue(&queue_name) {
                send_request = send_request
                    .message_group_id(event.aggregate_id().to_string())
                    .message_deduplication_id(event.event_id().to_string());
            }

            match send_request.send().await {
                Ok(response) => {
                    info!(
                        event_id = %event.event_id(),
                        event_type = %event.event_type(),
                        aggregate_id = %event.aggregate_id(),
                        message_id = response.message_id().unwrap_or("unknown"),
                        queue_name = %queue_name,
                        queue_url = %queue_url,
                        "Event successfully published to SQS"
                    );
                }
                Err(aws_error) => {
                    error!(
                        event_id = %event.event_id(),
                        event_type = %event.event_type(),
                        aggregate_id = %event.aggregate_id(),
                        queue_name = %queue_name,
                        queue_url = %queue_url,
                        error = %aws_error,
                        "Failed to publish event to SQS"
                    );
                    if first_error.is_none() {
                        first_error = Some(ServiceError::infrastructure(format!(
                            "Failed to publish event to SQS queue '{queue_name}': {aws_error}"
                        )));
                    }
                }
            }
        }

        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    async fn publish_batch(&self, events: &[Box<dyn DomainEvent>]) -> Result<(), ServiceError> {
        if !self.config.enabled {
            debug!(
                event_count = events.len(),
                "SQS publishing disabled, skipping batch"
            );
            return Ok(());
        }

        if events.is_empty() {
            return Ok(());
        }

        debug!(
            event_count = events.len(),
            "Publishing batch of events to SQS"
        );

        let (entries_by_queue, build_error) = self.build_batch_entries(events);
        let send_error = self.send_batch_entries(entries_by_queue).await;
        let first_error = build_error.or(send_error);

        if let Some(error) = first_error {
            Err(error)
        } else {
            info!("Successfully published all events in batch");
            Ok(())
        }
    }

    async fn health_check(&self) -> Result<(), ServiceError> {
        if !self.config.enabled {
            return Ok(());
        }

        debug!("Performing SQS health check");

        for queue_name in self.config.all_queue_names() {
            let queue_url = self.config.queue_url(&queue_name);
            match self
                .client
                .get_queue_attributes()
                .queue_url(&queue_url)
                .attribute_names(
                    aws_sdk_sqs::types::QueueAttributeName::ApproximateNumberOfMessages,
                )
                .send()
                .await
            {
                Ok(_) => {
                    debug!(queue_name = %queue_name, queue_url = %queue_url, "SQS health check passed");
                }
                Err(aws_error) => {
                    error!(
                        queue_name = %queue_name,
                        queue_url = %queue_url,
                        error = %aws_error,
                        "SQS health check failed"
                    );
                    return Err(ServiceError::infrastructure(format!(
                        "SQS health check failed for queue '{queue_name}': {aws_error}"
                    )));
                }
            }
        }

        Ok(())
    }
}

/// SQS event consumer implementation
pub struct SqsEventConsumer {
    client: Client,
    config: SqsConfig,
    should_stop: Arc<std::sync::atomic::AtomicBool>,
}

impl SqsEventConsumer {
    /// Create a new SQS event consumer from configuration
    pub async fn new(config: SqsConfig) -> Result<Self, ServiceError> {
        let client = SqsEventPublisher::create_client(&config).await?;

        Ok(Self {
            client,
            config,
            should_stop: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    fn configured_queue_urls(&self) -> Vec<String> {
        self.config.all_queue_urls()
    }

    /// Parse message body into a domain event
    fn parse_message_body(body: &str) -> Result<Box<dyn DomainEvent>, ServiceError> {
        let message: Value = serde_json::from_str(body).map_err(|e| {
            ServiceError::infrastructure(format!("Failed to parse SQS message: {e}"))
        })?;

        // Extract basic event information
        let event_id = message
            .get("event_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ServiceError::infrastructure("Missing event_id in SQS message".to_string())
            })?;

        let event_type = message
            .get("event_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ServiceError::infrastructure("Missing event_type in SQS message".to_string())
            })?;

        let aggregate_id = message
            .get("aggregate_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ServiceError::infrastructure("Missing aggregate_id in SQS message".to_string())
            })?;

        let occurred_at = message
            .get("occurred_at")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ServiceError::infrastructure("Missing occurred_at in SQS message".to_string())
            })?;

        let version = message
            .get("version")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(1) as i32;

        let data = message.get("data").ok_or_else(|| {
            ServiceError::infrastructure("Missing data in SQS message".to_string())
        })?;

        let metadata = message
            .get("metadata")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        // Create a generic domain event from the parsed data
        let event = GenericDomainEvent {
            event_id: event_id.to_string(),
            event_type: event_type.to_string(),
            aggregate_id: aggregate_id.to_string(),
            occurred_at: occurred_at.to_string(),
            version,
            data: data.clone(),
            metadata,
        };

        Ok(Box::new(event))
    }

    /// Poll one queue for messages and handle them.
    async fn poll_queue_and_handle_messages<H>(
        client: &Client,
        queue_url: &str,
        handler: &H,
    ) -> Result<(), ServiceError>
    where
        H: EventHandler + Send + Sync,
    {
        let response = match client
            .receive_message()
            .queue_url(queue_url)
            .max_number_of_messages(10) // SQS max
            .wait_time_seconds(2) // Long polling
            .send()
            .await
        {
            Ok(response) => response,
            Err(e) => {
                error!(
                    error = %e,
                    queue_url = %queue_url,
                    "Failed to receive messages from SQS"
                );
                return Err(ServiceError::infrastructure(format!(
                    "SQS receive error: {e}"
                )));
            }
        };

        let Some(messages) = response.messages else {
            info!("No messages received from SQS");
            return Ok(());
        };

        for message in messages {
            Self::handle_sqs_message(client, queue_url, handler, message).await;
        }

        Ok(())
    }

    async fn handle_sqs_message<H>(client: &Client, queue_url: &str, handler: &H, message: Message)
    where
        H: EventHandler + Send + Sync,
    {
        let Some(body) = message.body() else {
            return;
        };

        let event = match Self::parse_message_body(body) {
            Ok(event) => event,
            Err(e) => {
                warn!(
                    error = %e,
                    message_id = message.message_id().unwrap_or("unknown"),
                    "Failed to parse message body"
                );
                return;
            }
        };

        let event_type = event.event_type().to_string();
        if !handler.supports_event_type(&event_type) {
            debug!(
                event_type = %event_type,
                "Handler doesn't support event type, skipping"
            );
            return;
        }

        if let Err(e) = handler.handle_event(event).await {
            warn!(
                error = %e,
                message_id = message.message_id().unwrap_or("unknown"),
                "Failed to handle message"
            );
            return;
        }

        Self::delete_processed_message(client, queue_url, &message).await;
    }

    async fn delete_processed_message(client: &Client, queue_url: &str, message: &Message) {
        let Some(receipt_handle) = message.receipt_handle() else {
            return;
        };

        if let Err(e) = client
            .delete_message()
            .queue_url(queue_url)
            .receipt_handle(receipt_handle)
            .send()
            .await
        {
            warn!(
                error = %e,
                message_id = message.message_id().unwrap_or("unknown"),
                "Failed to delete processed message"
            );
        }
    }
}

#[async_trait]
impl EventConsumer for SqsEventConsumer {
    async fn start<H>(&self, handler: H) -> Result<(), ServiceError>
    where
        H: EventHandler + Send + Sync + 'static,
    {
        if !self.config.enabled {
            info!("SQS consumer disabled, not starting");
            return Ok(());
        }

        let queue_urls = self.configured_queue_urls();
        if queue_urls.is_empty() {
            return Err(ServiceError::infrastructure(
                "No SQS queues configured for consumer".to_string(),
            ));
        }

        info!(
            queue_count = queue_urls.len(),
            "Starting SQS event consumer"
        );
        self.should_stop.store(false, Ordering::SeqCst);

        let handler = Arc::new(handler);
        let mut tasks = Vec::new();

        for queue_url in queue_urls {
            let client = self.client.clone();
            let handler = handler.clone();
            let should_stop = self.should_stop.clone();

            tasks.push(tokio::spawn(async move {
                while !should_stop.load(Ordering::SeqCst) {
                    if let Err(e) =
                        Self::poll_queue_and_handle_messages(&client, &queue_url, handler.as_ref())
                            .await
                    {
                        error!(
                            error = %e,
                            queue_url = %queue_url,
                            "Error polling SQS messages"
                        );
                        // Sleep for a bit before retrying to avoid a tight loop on errors.
                        sleep(Duration::from_millis(500)).await;
                    }
                }

                Ok::<(), ServiceError>(())
            }));
        }

        for task in tasks {
            task.await.map_err(|e| {
                ServiceError::infrastructure(format!("SQS consumer task panicked: {e}"))
            })??;
        }

        info!("SQS event consumer stopped");
        Ok(())
    }

    async fn stop(&self) -> Result<(), ServiceError> {
        info!("Stopping SQS event consumer");
        self.should_stop.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn health_check(&self) -> Result<(), ServiceError> {
        if !self.config.enabled {
            return Ok(());
        }

        debug!("Performing SQS consumer health check");

        for queue_name in self.config.all_queue_names() {
            let queue_url = self.config.queue_url(&queue_name);
            match self
                .client
                .get_queue_attributes()
                .queue_url(&queue_url)
                .attribute_names(
                    aws_sdk_sqs::types::QueueAttributeName::ApproximateNumberOfMessages,
                )
                .send()
                .await
            {
                Ok(_) => {
                    debug!(
                        queue_name = %queue_name,
                        queue_url = %queue_url,
                        "SQS consumer health check passed"
                    );
                }
                Err(aws_error) => {
                    error!(
                        queue_name = %queue_name,
                        queue_url = %queue_url,
                        error = %aws_error,
                        "SQS consumer health check failed"
                    );
                    return Err(ServiceError::infrastructure(format!(
                        "SQS consumer health check failed for queue '{queue_name}': {aws_error}"
                    )));
                }
            }
        }

        Ok(())
    }
}

/// Generic domain event implementation for parsing SQS messages
#[derive(Debug, Clone)]
struct GenericDomainEvent {
    event_id: String,
    event_type: String,
    aggregate_id: String,
    occurred_at: String,
    version: i32,
    data: Value,
    metadata: serde_json::Map<String, Value>,
}

impl DomainEvent for GenericDomainEvent {
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
        self.version as u32
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use std::collections::HashMap;
    use std::sync::atomic::AtomicBool;
    use uuid::Uuid;

    #[derive(Debug)]
    struct TestEvent {
        event_type: String,
        event_id: Uuid,
        aggregate_id: Uuid,
        occurred_at: DateTime<Utc>,
    }

    impl TestEvent {
        fn new(event_type: &str) -> Self {
            Self {
                event_type: event_type.to_string(),
                event_id: Uuid::new_v4(),
                aggregate_id: Uuid::new_v4(),
                occurred_at: Utc::now(),
            }
        }
    }

    impl DomainEvent for TestEvent {
        fn event_type(&self) -> &str {
            &self.event_type
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
            1
        }

        fn to_json(&self) -> Result<String, ServiceError> {
            Ok("{}".to_string())
        }

        fn metadata(&self) -> HashMap<String, String> {
            HashMap::new()
        }
    }

    fn test_client() -> Client {
        let credentials = Credentials::new("test", "test", None, None, "rustycog-events-test");
        let config = aws_sdk_sqs::Config::builder()
            .region(Region::new("us-east-1"))
            .endpoint_url("http://localhost:4566")
            .credentials_provider(credentials)
            .build();
        Client::from_conf(config)
    }

    #[test]
    fn publisher_resolves_all_event_destinations() {
        let mut queues = HashMap::new();
        queues.insert(
            "user_signed_up".to_string(),
            vec!["telegraph-events".to_string(), "audit-events".to_string()],
        );
        let config = SqsConfig {
            queues,
            default_queues: vec!["fallback-events".to_string()],
            ..SqsConfig::default()
        };
        let publisher = SqsEventPublisher {
            client: test_client(),
            config,
        };
        let event = TestEvent::new("user_signed_up");

        let queue_names = publisher
            .get_queue_names_for_event(&event)
            .expect("event destinations should resolve");

        assert_eq!(queue_names, vec!["telegraph-events", "audit-events"]);
    }

    #[test]
    fn publisher_falls_back_to_default_destinations() {
        let config = SqsConfig {
            default_queues: vec!["fallback-a".to_string(), "fallback-b".to_string()],
            ..SqsConfig::default()
        };
        let publisher = SqsEventPublisher {
            client: test_client(),
            config,
        };
        let event = TestEvent::new("unknown_event");

        let queue_names = publisher
            .get_queue_names_for_event(&event)
            .expect("fallback destinations should resolve");

        assert_eq!(queue_names, vec!["fallback-a", "fallback-b"]);
    }

    #[test]
    fn consumer_polls_all_configured_queue_urls() {
        let mut queues = HashMap::new();
        queues.insert(
            "user_signed_up".to_string(),
            vec!["telegraph-events".to_string(), "audit-events".to_string()],
        );
        let config = SqsConfig {
            queues,
            default_queues: vec!["fallback-events".to_string()],
            host: "localhost".to_string(),
            port: 4566,
            ..SqsConfig::default()
        };
        let consumer = SqsEventConsumer {
            client: test_client(),
            config,
            should_stop: Arc::new(AtomicBool::new(false)),
        };

        let queue_urls = consumer.configured_queue_urls();

        assert_eq!(queue_urls.len(), 3);
        assert!(queue_urls
            .iter()
            .any(|url| url.ends_with("/telegraph-events")));
        assert!(queue_urls.iter().any(|url| url.ends_with("/audit-events")));
        assert!(queue_urls
            .iter()
            .any(|url| url.ends_with("/fallback-events")));
    }
}
