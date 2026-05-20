//! Health check utilities

/// Health check status
#[derive(Debug, Clone)]
pub enum HealthStatus {
    Healthy,
    Unhealthy(String),
}

/// Health checker trait
pub trait HealthChecker: Send + Sync {
    /// Check the health of a component
    async fn check(&self) -> HealthStatus;
}

/// Basic health checker implementation
pub struct BasicHealthChecker;

impl HealthChecker for BasicHealthChecker {
    async fn check(&self) -> HealthStatus {
        HealthStatus::Healthy
    }
}
