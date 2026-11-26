//! Authoritative platform-session liveness decisions for direct HTTP admission.

use crate::rustycog_config::SessionLivenessConfig;
use async_trait::async_trait;
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use uuid::Uuid;

/// Verified token data required for an authoritative session-liveness read.
///
/// The raw access token is deliberately excluded. Callers MUST validate the JWT
/// and classify the exact issuer before constructing this query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionLivenessQuery {
    /// Exact verified issuer (`iss`).
    pub issuer: String,
    /// Verified subject (`sub`).
    pub subject: Uuid,
    /// Canonical epoch claim, or `None` for a legacy token with no claim.
    pub auth_epoch: Option<u64>,
}

/// Authoritative decision for a verified platform-session query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionLivenessDecision {
    /// The token epoch is current.
    Live,
    /// The issuer is outside the local platform epoch authority.
    NotApplicable,
    /// The token is stale or its platform account no longer exists.
    Revoked,
}

/// Authoritative decision plus the current account epoch when applicable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionLivenessOutcome {
    /// Admission decision.
    pub decision: SessionLivenessDecision,
    /// Current platform-account epoch; absent for non-platform or missing accounts.
    pub current_epoch: Option<u64>,
}

impl SessionLivenessOutcome {
    /// Construct an outcome without an account epoch.
    #[must_use]
    pub const fn new(decision: SessionLivenessDecision) -> Self {
        Self {
            decision,
            current_epoch: None,
        }
    }

    /// Construct an outcome carrying the authoritative account epoch.
    #[must_use]
    pub const fn with_current_epoch(decision: SessionLivenessDecision, current_epoch: u64) -> Self {
        Self {
            decision,
            current_epoch: Some(current_epoch),
        }
    }
}

/// Failure to obtain or apply an authoritative liveness decision.
#[derive(Debug, Error)]
pub enum SessionLivenessError {
    /// The verified credential is no longer live.
    #[error("platform session is revoked")]
    Revoked,
    /// The authoritative store is unavailable, timed out or inconsistent.
    #[error("platform session liveness is unavailable: {message}")]
    Unavailable {
        /// Non-secret diagnostic detail.
        message: String,
    },
}

impl SessionLivenessError {
    /// Construct an infrastructure failure without exposing token material.
    #[must_use]
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::Unavailable {
            message: message.into(),
        }
    }
}

/// Composition-root supplied authoritative session-liveness reader.
///
/// Implementations MUST read the authoritative primary or an equivalent
/// strongly consistent store. They MUST NOT serve positive decisions from a
/// stale replica or cache, and they MUST NOT convert an infrastructure failure
/// into an allow decision.
#[async_trait]
pub trait SessionLivenessChecker: Send + Sync {
    /// Decide whether the verified platform credential is still live.
    ///
    /// # Errors
    ///
    /// Returns [`SessionLivenessError::Unavailable`] when the authoritative
    /// decision cannot be obtained. Revocation is represented by
    /// [`SessionLivenessDecision::Revoked`], not as an infrastructure error.
    async fn check(
        &self,
        query: &SessionLivenessQuery,
    ) -> Result<SessionLivenessOutcome, SessionLivenessError>;
}

pub(crate) fn parse_canonical_epoch(value: &str) -> Result<u64, &'static str> {
    let canonical = !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && (value.len() == 1 || !value.starts_with('0'));
    if !canonical {
        return Err("auth_epoch must be a canonical unsigned decimal string");
    }
    value
        .parse::<u64>()
        .map_err(|_| "auth_epoch exceeds the unsigned 64-bit range")
}

/// Bounded HTTP client for an authoritative session-liveness endpoint.
///
/// The caller supplies the exact workload token header and value. The client
/// sends only verified issuer/subject/epoch data, never the raw access token.
/// It performs one attempt with a total deadline and treats every transport,
/// status, decoding or consistency failure as unavailable.
pub struct HttpSessionLivenessChecker {
    client: reqwest::Client,
    endpoint: Url,
    token_header: HeaderName,
    token: HeaderValue,
    timeout: Duration,
}

impl HttpSessionLivenessChecker {
    /// Construct a checker from shared service configuration.
    ///
    /// # Errors
    ///
    /// Returns [`SessionLivenessError::Unavailable`] when settings are partial,
    /// the workload token environment variable is unavailable, the timeout is
    /// out of bounds, or the endpoint/client is invalid. An entirely absent
    /// configuration returns `Ok(None)`.
    pub fn from_config(
        config: &SessionLivenessConfig,
    ) -> Result<Option<Self>, SessionLivenessError> {
        let Some(endpoint) = config.endpoint.as_deref() else {
            if config.token_env.is_some()
                || config.token_header.is_some()
                || config.timeout_ms.is_some()
            {
                return Err(SessionLivenessError::unavailable(
                    "session liveness endpoint is required when settings are present",
                ));
            }
            return Ok(None);
        };
        if endpoint.trim().is_empty() {
            return Err(SessionLivenessError::unavailable(
                "session liveness endpoint must not be empty",
            ));
        }
        let token_env = config.token_env.as_deref().ok_or_else(|| {
            SessionLivenessError::unavailable("session liveness token_env is required")
        })?;
        let token = std::env::var(token_env)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                SessionLivenessError::unavailable(
                    "session liveness workload token environment variable is unavailable",
                )
            })?;
        let timeout_ms = config.timeout_ms.unwrap_or(2000);
        if !(1..=30_000).contains(&timeout_ms) {
            return Err(SessionLivenessError::unavailable(
                "session liveness timeout_ms must be between 1 and 30000",
            ));
        }
        Self::new(
            endpoint,
            config
                .token_header
                .as_deref()
                .unwrap_or("x-iam-internal-token"),
            &token,
            Duration::from_millis(timeout_ms),
        )
        .map(Some)
    }

    /// Construct a bounded liveness client.
    ///
    /// # Errors
    ///
    /// Returns [`SessionLivenessError::Unavailable`] for an invalid endpoint,
    /// header/token, zero timeout, or HTTP client construction failure.
    pub fn new(
        endpoint: &str,
        token_header: &str,
        token: &str,
        timeout: Duration,
    ) -> Result<Self, SessionLivenessError> {
        let endpoint = Url::parse(endpoint)
            .map_err(|_| SessionLivenessError::unavailable("invalid liveness endpoint"))?;
        if !matches!(endpoint.scheme(), "http" | "https")
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(SessionLivenessError::unavailable(
                "liveness endpoint must be HTTP(S) without userinfo or fragment",
            ));
        }
        if timeout.is_zero() {
            return Err(SessionLivenessError::unavailable(
                "liveness timeout must be positive",
            ));
        }
        let token_header = HeaderName::from_bytes(token_header.as_bytes())
            .map_err(|_| SessionLivenessError::unavailable("invalid workload token header"))?;
        let token = HeaderValue::from_str(token)
            .map_err(|_| SessionLivenessError::unavailable("invalid workload token"))?;
        let client = reqwest::Client::builder()
            .build()
            .map_err(|_| SessionLivenessError::unavailable("liveness HTTP client failed"))?;
        Ok(Self {
            client,
            endpoint,
            token_header,
            token,
            timeout,
        })
    }
}

#[async_trait]
impl SessionLivenessChecker for HttpSessionLivenessChecker {
    async fn check(
        &self,
        query: &SessionLivenessQuery,
    ) -> Result<SessionLivenessOutcome, SessionLivenessError> {
        let request = LivenessRequest {
            issuer: &query.issuer,
            subject: query.subject,
            auth_epoch: query.auth_epoch.map(|epoch| epoch.to_string()),
        };
        let response = self
            .client
            .post(self.endpoint.clone())
            .timeout(self.timeout)
            .header(self.token_header.clone(), self.token.clone())
            .json(&request)
            .send()
            .await
            .map_err(|_| SessionLivenessError::unavailable("liveness request failed"))?;
        if !response.status().is_success() {
            return Err(SessionLivenessError::unavailable(format!(
                "liveness endpoint returned HTTP {}",
                response.status().as_u16()
            )));
        }
        let response: LivenessResponse = response
            .json()
            .await
            .map_err(|_| SessionLivenessError::unavailable("invalid liveness response"))?;
        if response.decision == WireDecision::NotApplicable {
            return if response.current_epoch.is_none() {
                Ok(SessionLivenessOutcome::new(
                    SessionLivenessDecision::NotApplicable,
                ))
            } else {
                Err(SessionLivenessError::unavailable(
                    "not_applicable response must not carry an epoch",
                ))
            };
        }
        let current_epoch = response
            .current_epoch
            .as_deref()
            .ok_or_else(|| SessionLivenessError::unavailable("missing current epoch"))?;
        let current_epoch =
            parse_canonical_epoch(current_epoch).map_err(SessionLivenessError::unavailable)?;
        let expected = if query.auth_epoch == Some(current_epoch)
            || (query.auth_epoch.is_none() && current_epoch == 0)
        {
            SessionLivenessDecision::Live
        } else {
            SessionLivenessDecision::Revoked
        };
        let decision = match response.decision {
            WireDecision::Live => SessionLivenessDecision::Live,
            WireDecision::Revoked => SessionLivenessDecision::Revoked,
            WireDecision::NotApplicable => {
                return Ok(SessionLivenessOutcome::new(
                    SessionLivenessDecision::NotApplicable,
                ))
            }
        };
        if decision != expected {
            return Err(SessionLivenessError::unavailable(
                "liveness decision is inconsistent with the authoritative epoch",
            ));
        }
        Ok(SessionLivenessOutcome::with_current_epoch(
            decision,
            current_epoch,
        ))
    }
}

#[derive(Serialize)]
struct LivenessRequest<'a> {
    issuer: &'a str,
    subject: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    auth_epoch: Option<String>,
}

#[derive(Deserialize)]
struct LivenessResponse {
    decision: WireDecision,
    #[serde(default)]
    current_epoch: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WireDecision {
    Live,
    Revoked,
    NotApplicable,
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn query(epoch: Option<u64>) -> SessionLivenessQuery {
        SessionLivenessQuery {
            issuer: "https://platform.example/iam".into(),
            subject: Uuid::new_v4(),
            auth_epoch: epoch,
        }
    }

    fn checker(server: &MockServer) -> HttpSessionLivenessChecker {
        HttpSessionLivenessChecker::new(
            &format!("{}/liveness", server.uri()),
            "x-workload-token",
            "fixture-secret",
            Duration::from_secs(2),
        )
        .expect("valid checker")
    }

    #[tokio::test]
    async fn live_and_revoked_responses_match_the_authoritative_epoch() {
        for (current, token_epoch, decision) in [
            ("4", Some(4), SessionLivenessDecision::Live),
            ("5", Some(4), SessionLivenessDecision::Revoked),
            ("0", None, SessionLivenessDecision::Live),
            ("1", None, SessionLivenessDecision::Revoked),
        ] {
            let server = MockServer::start().await;
            let wire = match decision {
                SessionLivenessDecision::Live => "live",
                SessionLivenessDecision::Revoked => "revoked",
                SessionLivenessDecision::NotApplicable => unreachable!("fixture decision"),
            };
            Mock::given(method("POST"))
                .and(path("/liveness"))
                .and(header("x-workload-token", "fixture-secret"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "decision": wire,
                    "current_epoch": current,
                })))
                .mount(&server)
                .await;
            let outcome = checker(&server).check(&query(token_epoch)).await.unwrap();
            assert_eq!(outcome.decision, decision);
            assert_eq!(outcome.current_epoch, Some(current.parse().unwrap()));
        }
    }

    #[tokio::test]
    async fn request_contains_only_verified_identity_data() {
        let server = MockServer::start().await;
        let query = query(Some(7));
        Mock::given(method("POST"))
            .and(path("/liveness"))
            .and(body_json(serde_json::json!({
                "issuer": query.issuer,
                "subject": query.subject,
                "auth_epoch": "7",
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "decision": "live",
                "current_epoch": "7",
            })))
            .expect(1)
            .mount(&server)
            .await;
        checker(&server).check(&query).await.unwrap();
        server.verify().await;
    }

    #[tokio::test]
    async fn malformed_inconsistent_and_non_success_responses_fail_closed() {
        for body in [
            serde_json::json!({"decision": "live", "current_epoch": "01"}),
            serde_json::json!({"decision": "live", "current_epoch": "8"}),
            serde_json::json!({"decision": "not_applicable", "current_epoch": "7"}),
            serde_json::json!({"decision": "unknown"}),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
                .mount(&server)
                .await;
            assert!(matches!(
                checker(&server).check(&query(Some(7))).await,
                Err(SessionLivenessError::Unavailable { .. })
            ));
        }

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        assert!(matches!(
            checker(&server).check(&query(Some(7))).await,
            Err(SessionLivenessError::Unavailable { .. })
        ));
    }
}
