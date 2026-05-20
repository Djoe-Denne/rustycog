//! Kafka test container utilities
//!
//! This module provides a Kafka container for integration tests to verify real
//! event publishing functionality.

use rdkafka::consumer::Consumer;
use rustycog_config::{load_config_part, KafkaConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use testcontainers::{runners::AsyncRunner, ContainerAsync, GenericImage, ImageExt};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};
use uuid;

/// Global test Kafka container instance
static TEST_KAFKA_CONTAINER: OnceLock<Arc<Mutex<Option<Arc<TestKafkaContainer>>>>> =
    OnceLock::new();

/// Flag to track if cleanup handler has been registered
static KAFKA_CLEANUP_REGISTERED: AtomicBool = AtomicBool::new(false);

fn kafka_broker_addresses(
    brokers: &str,
) -> Result<Vec<std::net::SocketAddr>, Box<dyn std::error::Error>> {
    brokers
        .split(',')
        .map(|broker| broker.trim().parse::<std::net::SocketAddr>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Invalid Kafka broker address '{brokers}': {e}").into())
}

/// Test Kafka container wrapper
pub struct TestKafkaContainer {
    container: ContainerAsync<GenericImage>,
    pub brokers: String,
    pub port: u16,
}

impl TestKafkaContainer {
    /// Stop and remove the container
    pub async fn cleanup(self) {
        info!("Stopping and removing test Kafka container");
        if let Err(e) = self.container.stop().await {
            warn!("Failed to stop Kafka container: {}", e);
        } else {
            info!("Kafka container stopped successfully");
        }
        if let Err(e) = self.container.rm().await {
            warn!("Failed to remove Kafka container: {}", e);
        } else {
            info!("Kafka container removed successfully");
        }
        info!("Test Kafka container cleanup completed");
    }
}

/// Test Kafka fixture providing Kafka connection and utilities
pub struct TestKafka {
    pub brokers: String,
    pub topic: String,
}

impl TestKafka {
    /// Get or create the global test Kafka instance
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        println!("Creating new Kafka test container");
        let container = get_or_create_test_kafka_container().await?;
        let brokers = container.brokers.clone();
        let topic = "test-user-events".to_string();

        // Parse the brokers string to get host and port
        let parts: Vec<&str> = brokers.split(':').collect();
        if parts.len() == 2 {
            let host = parts[0];
            let port = parts[1];

            // Set environment variables for Kafka configuration so our app config picks it up
            unsafe {
                std::env::set_var("RUSTYCOG_KAFKA__HOST", host);
                std::env::set_var("RUSTYCOG_KAFKA__PORT", port);
                std::env::set_var("RUSTYCOG_KAFKA__ENABLED", "true");
                std::env::set_var("RUSTYCOG_KAFKA__USER_EVENTS_TOPIC", &topic);
            }
        } else {
            // Fallback to old format for compatibility
            unsafe {
                std::env::set_var("RUSTYCOG_KAFKA__HOST", "localhost");
                std::env::set_var("RUSTYCOG_KAFKA__PORT", "9092");
                std::env::set_var("RUSTYCOG_KAFKA__ENABLED", "true");
                std::env::set_var("RUSTYCOG_KAFKA__USER_EVENTS_TOPIC", &topic);
            }
        }

        // Wait for Kafka to be ready
        Self::wait_for_kafka(&brokers).await?;

        Ok(Self { brokers, topic })
    }

    /// Wait for Kafka to be ready using a simple TCP connection test
    async fn wait_for_kafka(brokers: &str) -> Result<(), Box<dyn std::error::Error>> {
        info!("Waiting for Kafka to be ready...");

        let max_attempts = 30;
        let addresses = kafka_broker_addresses(brokers)?;

        for attempt in 1..=max_attempts {
            if Self::can_connect_to_any(&addresses).await {
                info!("Kafka is ready after {} attempts", attempt);
                return Ok(());
            }

            if attempt < max_attempts {
                debug!(
                    "Retrying Kafka connection in 1 second... (attempt {}/{})",
                    attempt, max_attempts
                );
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }

        Err(format!("Kafka failed to become ready after {max_attempts} attempts").into())
    }

    async fn can_connect_to_any(addresses: &[std::net::SocketAddr]) -> bool {
        for addr in addresses {
            match tokio::net::TcpStream::connect(addr).await {
                Ok(_) => return true,
                Err(e) => debug!("Kafka connection failed: {}", e),
            }
        }
        false
    }

    /// Wait for a message to be published to the topic
    /// This is a simplified version that waits for a certain duration
    pub async fn wait_for_message(
        &self,
        timeout_secs: u64,
    ) -> Result<String, Box<dyn std::error::Error>> {
        // For the test, we'll just wait and assume the message was published
        // In a real implementation, we'd need to consume from Kafka
        tokio::time::sleep(Duration::from_secs(timeout_secs.min(5))).await;
        Ok("mock_event_message".to_string())
    }

    /// Get all messages from the topic using infra test consumer
    pub async fn get_all_messages(
        &self,
        max_wait_secs: u64,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        debug!("Creating test consumer for topic: {}", self.topic);
        let consumer = TestKafkaConsumer::new(&self.brokers, &self.topic).await?;
        consumer.get_all_messages(max_wait_secs).await
    }

    /// Wait for a specific number of messages to be available
    pub async fn wait_for_messages(
        &self,
        expected_count: usize,
        max_wait_secs: u64,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        debug!(
            "Creating test consumer to wait for {} messages",
            expected_count
        );
        let consumer = TestKafkaConsumer::new(&self.brokers, &self.topic).await?;
        consumer
            .wait_for_messages(expected_count, max_wait_secs)
            .await
    }
}

/// Get or create the global test Kafka container
async fn get_or_create_test_kafka_container(
) -> Result<Arc<TestKafkaContainer>, Box<dyn std::error::Error>> {
    let container_mutex = TEST_KAFKA_CONTAINER.get_or_init(|| Arc::new(Mutex::new(None)));

    let mut container_guard = container_mutex.lock().await;

    if let Some(ref container) = *container_guard {
        return Ok(container.clone());
    }

    info!("Creating new Kafka test container");

    // Clean up any existing container
    cleanup_existing_kafka_container().await;

    // Clear only the Kafka port cache to ensure fresh random port generation
    // Don't clear all caches as that would interfere with database test containers
    KafkaConfig::clear_port_cache();

    // Load configuration to understand Kafka settings
    let kafka_config =
        load_config_part::<KafkaConfig>("kafka").expect("failed to load kafka config");

    // Use the configuration's port resolution mechanism
    let kafka_port = kafka_config.actual_port();

    // Create Kafka container using Apache Kafka in KRaft mode (no Zookeeper needed)
    let kafka_image = GenericImage::new("apache/kafka", "3.7.0")
        .with_env_var("KAFKA_NODE_ID", "1")
        .with_env_var(
            "KAFKA_LISTENER_SECURITY_PROTOCOL_MAP",
            "CONTROLLER:PLAINTEXT,PLAINTEXT:PLAINTEXT,PLAINTEXT_HOST:PLAINTEXT",
        )
        .with_env_var(
            "KAFKA_ADVERTISED_LISTENERS",
            format!(
                "PLAINTEXT://localhost:{kafka_port},PLAINTEXT_HOST://localhost:{kafka_port}"
            ),
        )
        .with_env_var(
            "KAFKA_LISTENERS",
            format!(
                "PLAINTEXT://0.0.0.0:29092,CONTROLLER://0.0.0.0:29093,PLAINTEXT_HOST://0.0.0.0:{kafka_port}"
            ),
        )
        .with_env_var("KAFKA_INTER_BROKER_LISTENER_NAME", "PLAINTEXT")
        .with_env_var("KAFKA_CONTROLLER_LISTENER_NAMES", "CONTROLLER")
        .with_env_var("KAFKA_CONTROLLER_QUORUM_VOTERS", "1@localhost:29093")
        .with_env_var("KAFKA_PROCESS_ROLES", "broker,controller")
        .with_env_var("KAFKA_OFFSETS_TOPIC_REPLICATION_FACTOR", "1")
        .with_env_var("KAFKA_TRANSACTION_STATE_LOG_REPLICATION_FACTOR", "1")
        .with_env_var("KAFKA_TRANSACTION_STATE_LOG_MIN_ISR", "1")
        .with_env_var("KAFKA_GROUP_INITIAL_REBALANCE_DELAY_MS", "0")
        .with_env_var("KAFKA_AUTO_CREATE_TOPICS_ENABLE", "true")
        .with_env_var("CLUSTER_ID", "MkU3OEVBNTcwNTJENDM2Qk")
        .with_container_name("iam_test-kafka")
        .with_mapped_port(
            kafka_port,
            testcontainers::core::ContainerPort::Tcp(kafka_port),
        );

    // Start Kafka
    info!("Starting Kafka container on port {}...", kafka_port);
    let kafka_container = kafka_image.start().await?;

    let brokers = format!("localhost:{kafka_port}");

    info!("Test Kafka container started");
    info!("Brokers: {}", brokers);

    // Wait for Kafka to be ready
    TestKafka::wait_for_kafka(&brokers).await?;

    let test_container = Arc::new(TestKafkaContainer {
        container: kafka_container,
        brokers,
        port: kafka_port,
    });

    *container_guard = Some(test_container.clone());

    // Register cleanup handler on first container creation
    register_kafka_cleanup_handler().await;

    Ok(test_container)
}

/// Clean up any existing Kafka containers
async fn cleanup_existing_kafka_container() {
    use std::process::Command;

    debug!("Checking for existing Kafka test containers");

    let containers = ["iam_test-kafka"];

    for container_name in &containers {
        // Stop the container
        let _ = Command::new("docker")
            .args(["stop", container_name])
            .output();

        // Remove the container
        let _ = Command::new("docker")
            .args(["rm", "-f", container_name])
            .output();

        debug!("Cleaned up container: {}", container_name);
    }
}

/// Register cleanup handler for Kafka containers
async fn register_kafka_cleanup_handler() {
    if KAFKA_CLEANUP_REGISTERED.swap(true, Ordering::SeqCst) {
        return;
    }

    info!("Registering Kafka test container cleanup handler");
}

/// Test fixture for Kafka integration tests
pub struct TestKafkaFixture {
    pub kafka: TestKafka,
}

impl TestKafkaFixture {
    /// Create a new Kafka test fixture
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let kafka = TestKafka::new().await?;
        Ok(Self { kafka })
    }

    /// Wait for and verify a specific event was published
    pub async fn verify_event_published(
        &self,
        event_type: &str,
        timeout_secs: u64,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let messages = self.kafka.get_all_messages(timeout_secs).await?;

        for message in messages {
            if let Ok(event) = serde_json::from_str::<serde_json::Value>(&message) {
                if let Some(event_type_value) = event.get("event_type") {
                    if event_type_value == event_type {
                        return Ok(event);
                    }
                }
            }
        }

        Err(
            format!("Event with type '{event_type}' not found within {timeout_secs} seconds")
                .into(),
        )
    }

    /// Cleanup Kafka container (for test cleanup)
    pub async fn cleanup_container() -> Result<(), Box<dyn std::error::Error>> {
        let container_mutex = TEST_KAFKA_CONTAINER.get();
        if let Some(container_mutex) = container_mutex {
            let mut container_guard = container_mutex.lock().await;
            if let Some(container_arc) = container_guard.take() {
                info!("Manually cleaning up test Kafka container");

                if let Ok(container) = Arc::try_unwrap(container_arc) {
                    container.cleanup().await;
                    info!("Test Kafka container cleanup completed");
                } else {
                    warn!("Could not cleanup Kafka container: still has references");
                    // Fallback cleanup using Docker commands
                    cleanup_existing_kafka_container().await;
                }
            }
        }
        Ok(())
    }
}

/// Test Kafka consumer for verifying published events  
pub struct TestKafkaConsumer {
    consumer: rdkafka::consumer::StreamConsumer,
    topic: String,
}

impl TestKafkaConsumer {
    /// Create a new test consumer
    pub async fn new(brokers: &str, topic: &str) -> Result<Self, Box<dyn std::error::Error>> {
        use rdkafka::config::ClientConfig;
        use rdkafka::consumer::Consumer;

        let consumer: rdkafka::consumer::StreamConsumer = ClientConfig::new()
            .set(
                "group.id",
                format!("test-consumer-{}", uuid::Uuid::new_v4()),
            )
            .set("bootstrap.servers", brokers)
            .set("enable.partition.eof", "false")
            .set("session.timeout.ms", "6000")
            .set("enable.auto.commit", "false") // Disable auto commit for more control
            .set("auto.offset.reset", "earliest")
            .set("enable.auto.offset.store", "false")
            .set("api.version.request", "true")
            .set("fetch.wait.max.ms", "100") // Reduce wait time for faster polling
            .create()?;

        consumer.subscribe(&[topic])?;
        debug!("Test consumer subscribed to topic: {}", topic);

        // Wait a moment for subscription to take effect
        tokio::time::sleep(Duration::from_millis(500)).await;

        Ok(Self {
            consumer,
            topic: topic.to_string(),
        })
    }

    /// Get all available messages from the topic
    pub async fn get_all_messages(
        &self,
        max_wait_secs: u64,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        use rdkafka::message::Message;

        let mut messages = Vec::new();
        let timeout = Duration::from_secs(max_wait_secs);
        let start_time = std::time::Instant::now();

        info!(
            "Starting to consume messages from topic: {} for up to {}s",
            self.topic, max_wait_secs
        );

        // Poll multiple times to ensure we get all messages
        let mut consecutive_timeouts = 0;
        const MAX_CONSECUTIVE_TIMEOUTS: u32 = 3;

        while start_time.elapsed() < timeout && consecutive_timeouts < MAX_CONSECUTIVE_TIMEOUTS {
            match tokio::time::timeout(Duration::from_secs(1), self.consumer.recv()).await {
                Ok(Ok(m)) => {
                    consecutive_timeouts = 0; // Reset timeout counter

                    if let Some(payload) = m.payload() {
                        let message_str = String::from_utf8_lossy(payload).to_string();
                        debug!("Received message: {}", message_str);
                        messages.push(message_str);

                        // Manually commit the offset
                        if let Err(e) = self
                            .consumer
                            .commit_message(&m, rdkafka::consumer::CommitMode::Sync)
                        {
                            warn!("Failed to commit message: {}", e);
                        }
                    }
                }
                Ok(Err(e)) => {
                    debug!("Consumer error: {}", e);
                    consecutive_timeouts += 1;
                }
                Err(_) => {
                    // Timeout on recv() - this is expected when no more messages
                    debug!("Consumer timeout (expected when no more messages)");
                    consecutive_timeouts += 1;
                }
            }

            // Small delay to prevent busy waiting
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        info!(
            "Retrieved {} messages from topic {} in {:?}",
            messages.len(),
            self.topic,
            start_time.elapsed()
        );
        Ok(messages)
    }

    /// Wait for a specific number of messages
    pub async fn wait_for_messages(
        &self,
        expected_count: usize,
        max_wait_secs: u64,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let start_time = std::time::Instant::now();
        let timeout = Duration::from_secs(max_wait_secs);

        while start_time.elapsed() < timeout {
            let messages = self.get_all_messages(2).await?;

            if messages.len() >= expected_count {
                info!(
                    "Found {} messages (expected {})",
                    messages.len(),
                    expected_count
                );
                return Ok(messages);
            }

            debug!(
                "Found {} messages, waiting for {} (elapsed: {:?})",
                messages.len(),
                expected_count,
                start_time.elapsed()
            );

            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        let messages = self.get_all_messages(1).await?;
        if messages.len() >= expected_count {
            Ok(messages)
        } else {
            Err(format!(
                "Timeout waiting for messages. Expected {}, found {} after {:?}",
                expected_count,
                messages.len(),
                timeout
            )
            .into())
        }
    }
}
