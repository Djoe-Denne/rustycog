//! `PermissionChecker` implementations.
//!
//! - [`OpenFgaPermissionChecker`] is the production implementation. It calls
//!   the `OpenFGA` `Check` HTTP endpoint.
//! - [`InMemoryPermissionChecker`] is a deterministic implementation intended
//!   for unit and integration tests.
//! - [`CachedPermissionChecker`] wraps any inner checker with a short-TTL LRU
//!   cache from the `moka` crate.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use moka::future::Cache;
use rustycog_config::OpenFgaClientConfig;
use rustycog_core::error::DomainError;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::{Permission, PermissionChecker, ResourceRef, Subject};

// =============================================================================
// OpenFGA
// =============================================================================

/// Production permission checker that calls `OpenFGA`'s `Check` endpoint.
pub struct OpenFgaPermissionChecker {
    config: OpenFgaClientConfig,
    http: reqwest::Client,
}

impl OpenFgaPermissionChecker {
    pub fn new(config: OpenFgaClientConfig) -> Result<Self, DomainError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to build OpenFGA HTTP client: {e}"),
            })?;
        Ok(Self { config, http })
    }

    fn check_url(&self) -> String {
        format!(
            "{}/stores/{}/check",
            self.config.api_url().trim_end_matches('/'),
            self.config.store_id
        )
    }
}

#[derive(Serialize)]
struct CheckRequestBody<'a> {
    tuple_key: CheckTupleKey<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    authorization_model_id: Option<&'a str>,
}

#[derive(Serialize)]
struct CheckTupleKey<'a> {
    user: String,
    relation: &'a str,
    object: String,
}

#[derive(Deserialize)]
struct CheckResponseBody {
    #[serde(default)]
    allowed: bool,
}

#[async_trait]
impl PermissionChecker for OpenFgaPermissionChecker {
    async fn check(
        &self,
        subject: Subject,
        action: Permission,
        resource: ResourceRef,
    ) -> Result<bool, DomainError> {
        let body = CheckRequestBody {
            tuple_key: CheckTupleKey {
                user: subject.to_string(),
                relation: action.relation(),
                object: resource.as_object_string(),
            },
            authorization_model_id: self.config.authorization_model_id.as_deref(),
        };

        let mut req = self.http.post(self.check_url()).json(&body);
        if let Some(token) = &self.config.api_token {
            req = req.bearer_auth(token);
        }

        let response = req.send().await.map_err(|e| DomainError::Internal {
            message: format!("OpenFGA Check request failed: {e}"),
        })?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            warn!(
                status = %status,
                body = %text,
                "OpenFGA Check returned non-success status"
            );
            return Err(DomainError::Internal {
                message: format!("OpenFGA Check returned {status}: {text}"),
            });
        }

        let decoded: CheckResponseBody =
            response.json().await.map_err(|e| DomainError::Internal {
                message: format!("Failed to decode OpenFGA Check response: {e}"),
            })?;

        debug!(
            subject = %subject,
            action = %action,
            resource = %resource,
            allowed = decoded.allowed,
            "OpenFGA Check decision"
        );
        Ok(decoded.allowed)
    }
}

// =============================================================================
// In-memory (test)
// =============================================================================

/// In-memory permission checker used by unit tests and local fixtures.
#[derive(Default)]
pub struct InMemoryPermissionChecker {
    tuples: std::sync::RwLock<std::collections::HashSet<(Subject, Permission, ResourceRef)>>,
}

impl InMemoryPermissionChecker {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Grant `action` on `resource` to `subject`.
    pub fn allow(&self, subject: Subject, action: Permission, resource: ResourceRef) {
        let mut guard = self.tuples.write().unwrap();
        guard.insert((subject, action, resource));
    }

    /// Revoke `action` on `resource` from `subject`.
    pub fn deny(&self, subject: Subject, action: Permission, resource: ResourceRef) {
        let mut guard = self.tuples.write().unwrap();
        guard.remove(&(subject, action, resource));
    }
}

#[async_trait]
impl PermissionChecker for InMemoryPermissionChecker {
    async fn check(
        &self,
        subject: Subject,
        action: Permission,
        resource: ResourceRef,
    ) -> Result<bool, DomainError> {
        let guard = self.tuples.read().unwrap();
        Ok(guard.contains(&(subject, action, resource)))
    }
}

// =============================================================================
// Cache decorator
// =============================================================================

#[derive(Clone, Hash, PartialEq, Eq)]
struct CacheKey {
    user_id: uuid::Uuid,
    permission: Permission,
    object_type: &'static str,
    object_id: uuid::Uuid,
}

/// Short-TTL LRU cache around any `PermissionChecker`.
///
/// Decisions are cached by `(subject, action, resource)`. The cache uses
/// time-based invalidation only — a revoke in `OpenFGA` is visible once the
/// cached decision expires. Choose a TTL that balances latency against the
/// blast radius of a stale allow.
pub struct CachedPermissionChecker {
    inner: Arc<dyn PermissionChecker>,
    cache: Cache<CacheKey, bool>,
}

impl CachedPermissionChecker {
    pub fn new(inner: Arc<dyn PermissionChecker>, ttl: Duration, max_capacity: u64) -> Self {
        let cache = Cache::builder()
            .time_to_live(ttl)
            .max_capacity(max_capacity)
            .build();
        Self { inner, cache }
    }
}

#[async_trait]
impl PermissionChecker for CachedPermissionChecker {
    async fn check(
        &self,
        subject: Subject,
        action: Permission,
        resource: ResourceRef,
    ) -> Result<bool, DomainError> {
        // Wildcard subjects (anonymous-public-read via `viewer@user:*`) are
        // intentionally **not** cached. The cache key is keyed on
        // `user_id: Uuid` and the wildcard reuses `Uuid::nil()`, which would
        // collide across every anonymous request and let one project's
        // public-read decision answer for another. Skipping the cache also
        // means a freshly-flipped public->private (when sentinel-sync
        // removes the wildcard tuple) is observed on the very next request
        // instead of after the TTL window, which is the right safety
        // posture for a relation that grants everyone read access.
        if subject.is_wildcard() {
            return self.inner.check(subject, action, resource).await;
        }

        let key = CacheKey {
            user_id: subject.user_id,
            permission: action,
            object_type: resource.object_type,
            object_id: resource.object_id,
        };

        if let Some(hit) = self.cache.get(&key).await {
            return Ok(hit);
        }

        let decision = self.inner.check(subject, action, resource).await?;
        self.cache.insert(key, decision).await;
        Ok(decision)
    }
}

// =============================================================================
// Metrics decorator
// =============================================================================

/// Instrumented `PermissionChecker` that emits a `tracing` span and a
/// structured event per decision.
///
/// Point a `tracing` subscriber / OpenTelemetry bridge at these events to
/// capture p50/p95/p99 latency, allow vs deny rate, and error counts. When
/// no subscriber is wired the overhead is a single `Instant::now()` pair.
pub struct MetricsPermissionChecker {
    inner: Arc<dyn PermissionChecker>,
}

impl MetricsPermissionChecker {
    pub fn new(inner: Arc<dyn PermissionChecker>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl PermissionChecker for MetricsPermissionChecker {
    async fn check(
        &self,
        subject: Subject,
        action: Permission,
        resource: ResourceRef,
    ) -> Result<bool, DomainError> {
        let started = std::time::Instant::now();
        let result = self.inner.check(subject, action, resource).await;
        let elapsed_us = started.elapsed().as_micros();

        match &result {
            Ok(allowed) => tracing::info!(
                target: "permission_checker",
                subject = %subject,
                action = %action,
                resource = %resource,
                decision = if *allowed { "allow" } else { "deny" },
                elapsed_us = %elapsed_us,
                "permission decision"
            ),
            Err(error) => tracing::warn!(
                target: "permission_checker",
                subject = %subject,
                action = %action,
                resource = %resource,
                error = %error,
                elapsed_us = %elapsed_us,
                "permission decision error"
            ),
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn in_memory_checker_allows_after_grant() {
        let checker = InMemoryPermissionChecker::new();
        let subject = Subject::new(uuid::Uuid::new_v4());
        let resource = ResourceRef::new("organization", uuid::Uuid::new_v4());

        assert!(!checker
            .check(subject, Permission::Read, resource)
            .await
            .unwrap());

        checker.allow(subject, Permission::Read, resource);
        assert!(checker
            .check(subject, Permission::Read, resource)
            .await
            .unwrap());

        checker.deny(subject, Permission::Read, resource);
        assert!(!checker
            .check(subject, Permission::Read, resource)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn cached_checker_serves_second_call_from_cache() {
        let inner = Arc::new(InMemoryPermissionChecker::new());
        let subject = Subject::new(uuid::Uuid::new_v4());
        let resource = ResourceRef::new("project", uuid::Uuid::new_v4());
        inner.allow(subject, Permission::Write, resource);

        let cached = CachedPermissionChecker::new(
            inner.clone() as Arc<dyn PermissionChecker>,
            Duration::from_secs(30),
            128,
        );

        assert!(cached
            .check(subject, Permission::Write, resource)
            .await
            .unwrap());
        inner.deny(subject, Permission::Write, resource);
        assert!(cached
            .check(subject, Permission::Write, resource)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn cached_checker_bypasses_cache_for_wildcard_subject() {
        let inner = Arc::new(InMemoryPermissionChecker::new());
        let wildcard = Subject::wildcard();
        let resource = ResourceRef::new("project", uuid::Uuid::new_v4());
        inner.allow(wildcard, Permission::Read, resource);

        let cached = CachedPermissionChecker::new(
            inner.clone() as Arc<dyn PermissionChecker>,
            Duration::from_secs(30),
            128,
        );

        // First wildcard check -> allow (inner has the tuple).
        assert!(cached
            .check(wildcard, Permission::Read, resource)
            .await
            .unwrap());

        // Flip the inner decision. With the cache bypassed, the next
        // wildcard call must observe the new state immediately. If the
        // wildcard were cached, this assertion would fail because the
        // cached `true` would be returned instead.
        inner.deny(wildcard, Permission::Read, resource);
        assert!(!cached
            .check(wildcard, Permission::Read, resource)
            .await
            .unwrap());

        // Sanity: a concrete user-id subject with the same key shape is
        // still cached (regression guard for the rest of the cache logic).
        let user_subject = Subject::new(uuid::Uuid::new_v4());
        inner.allow(user_subject, Permission::Read, resource);
        assert!(cached
            .check(user_subject, Permission::Read, resource)
            .await
            .unwrap());
        inner.deny(user_subject, Permission::Read, resource);
        assert!(cached
            .check(user_subject, Permission::Read, resource)
            .await
            .unwrap()); // cached
    }
}
