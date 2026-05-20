//! Mock event publisher for testing event publishing behavior
//!
//! This allows tests to verify that events are correctly sent or not sent
//! based on the business logic, without requiring a real message queue.

use async_trait::async_trait;
use rustycog_core::error::ServiceError;
use rustycog_events::event::EventPublisher;
use rustycog_events::DomainEvent;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::time::{sleep, Duration, Instant};
use uuid::Uuid;

/// Captured event information for testing
#[derive(Debug, Clone)]
pub struct CapturedEvent {
    pub event_type: String,
    pub event_id: Uuid,
    pub aggregate_id: Uuid,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    pub version: u32,
    pub json_data: String,
    pub metadata: HashMap<String, String>,
}

impl CapturedEvent {
    /// Parse the JSON data as a `serde_json::Value` for inspection
    pub fn parse_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::from_str(&self.json_data)
    }

    /// Get a field from the parsed JSON data
    #[must_use]
    pub fn get_json_field(&self, field_name: &str) -> Option<serde_json::Value> {
        self.parse_json()
            .ok()
            .and_then(|json| json.get(field_name).cloned())
    }

    /// Get a string field from the parsed JSON data
    #[must_use]
    pub fn get_json_string_field(&self, field_name: &str) -> Option<String> {
        self.get_json_field(field_name)
            .and_then(|v| v.as_str().map(std::string::ToString::to_string))
    }
}

/// Mock event publisher that captures published events for test verification
#[derive(Debug, Clone)]
pub struct MockEventPublisher {
    published_events: Arc<Mutex<Vec<CapturedEvent>>>,
}

impl MockEventPublisher {
    /// Create a new mock event publisher
    #[must_use]
    pub fn new() -> Self {
        Self {
            published_events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Get all published events
    #[must_use]
    pub fn get_published_events(&self) -> Vec<CapturedEvent> {
        let events = self.published_events.lock().unwrap();
        events.clone()
    }

    /// Get the total number of published events
    #[must_use]
    pub fn get_event_count(&self) -> usize {
        let events = self.published_events.lock().unwrap();
        events.len()
    }

    /// Get events by type
    #[must_use]
    pub fn get_events_by_type(&self, event_type: &str) -> Vec<CapturedEvent> {
        let events = self.published_events.lock().unwrap();
        events
            .iter()
            .filter(|event| event.event_type == event_type)
            .cloned()
            .collect()
    }

    /// Check if any events of a specific type were published
    #[must_use]
    pub fn has_event_type(&self, event_type: &str) -> bool {
        let events = self.published_events.lock().unwrap();
        events.iter().any(|event| event.event_type == event_type)
    }

    /// Helper methods for common event types
    #[must_use]
    pub fn has_password_reset_requested_event(&self) -> bool {
        self.has_event_type("password_reset_requested")
    }

    #[must_use]
    pub fn get_password_reset_requested_events(&self) -> Vec<CapturedEvent> {
        self.get_events_by_type("password_reset_requested")
    }

    #[must_use]
    pub fn has_user_signed_up_event(&self) -> bool {
        self.has_event_type("user_signed_up")
    }

    #[must_use]
    pub fn get_user_signed_up_events(&self) -> Vec<CapturedEvent> {
        self.get_events_by_type("user_signed_up")
    }

    #[must_use]
    pub fn has_user_email_verified_event(&self) -> bool {
        self.has_event_type("user_email_verified")
    }

    #[must_use]
    pub fn get_user_email_verified_events(&self) -> Vec<CapturedEvent> {
        self.get_events_by_type("user_email_verified")
    }

    #[must_use]
    pub fn has_user_logged_in_event(&self) -> bool {
        self.has_event_type("user_logged_in")
    }

    #[must_use]
    pub fn get_user_logged_in_events(&self) -> Vec<CapturedEvent> {
        self.get_events_by_type("user_logged_in")
    }

    /// Clear all captured events (useful for test setup)
    pub fn clear_events(&self) {
        let mut events = self.published_events.lock().unwrap();
        events.clear();
    }

    /// Wait until at least `expected_count` events have been captured.
    pub async fn wait_for_event_count(
        &self,
        expected_count: usize,
        timeout: Duration,
    ) -> Vec<CapturedEvent> {
        let deadline = Instant::now() + timeout;

        loop {
            let events = self.get_published_events();
            if events.len() >= expected_count || Instant::now() >= deadline {
                return events;
            }

            sleep(Duration::from_millis(50)).await;
        }
    }

    /// Wait until at least one event of `event_type` has been captured.
    pub async fn wait_for_event_type(
        &self,
        event_type: &str,
        timeout: Duration,
    ) -> Vec<CapturedEvent> {
        let deadline = Instant::now() + timeout;

        loop {
            let events = self.get_events_by_type(event_type);
            if !events.is_empty() || Instant::now() >= deadline {
                return events;
            }

            sleep(Duration::from_millis(50)).await;
        }
    }

    /// Get a shared reference to this publisher (for dependency injection)
    #[must_use]
    pub fn as_arc(self) -> Arc<Self> {
        Arc::new(self)
    }
}

impl Default for MockEventPublisher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventPublisher<ServiceError> for MockEventPublisher {
    async fn publish(&self, event: &dyn DomainEvent) -> Result<(), ServiceError> {
        // Capture event information and serialize for test verification
        let captured_event = CapturedEvent {
            event_type: event.event_type().to_string(),
            event_id: event.event_id(),
            aggregate_id: event.aggregate_id(),
            occurred_at: event.occurred_at(),
            version: event.version(),
            json_data: event.to_json()?,
            metadata: event.metadata(),
        };

        let mut events = self.published_events.lock().unwrap();
        events.push(captured_event);
        Ok(())
    }

    async fn publish_batch(&self, events: &[Box<dyn DomainEvent>]) -> Result<(), ServiceError> {
        // Capture all events for test verification
        let mut stored_events = self.published_events.lock().unwrap();
        for event in events {
            let captured_event = CapturedEvent {
                event_type: event.event_type().to_string(),
                event_id: event.event_id(),
                aggregate_id: event.aggregate_id(),
                occurred_at: event.occurred_at(),
                version: event.version(),
                json_data: event.to_json()?,
                metadata: event.metadata(),
            };
            stored_events.push(captured_event);
        }
        Ok(())
    }

    async fn health_check(&self) -> Result<(), ServiceError> {
        // Mock is always healthy
        Ok(())
    }
}
