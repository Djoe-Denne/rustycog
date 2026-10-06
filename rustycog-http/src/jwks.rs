//! JWKS document cache with singleflight refresh and negative caching.

use crate::rustycog_command::CommandError;
use jsonwebtoken::{jwk::Jwk, DecodingKey};
use std::{
    collections::HashMap,
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, PoisonError, RwLock,
    },
    time::{Duration, Instant},
};
use tracing::{debug, warn};

const MAX_SNAPSHOT_AGE: Duration = Duration::from_secs(60);
const MIN_REFRESH_INTERVAL: Duration = Duration::from_millis(250);
const MAX_NEGATIVE_KIDS: usize = 1024;
const MAX_KID_BYTES: usize = 128;
const MAX_DOCUMENT_BYTES: usize = 1024 * 1024;

struct Snapshot {
    keys: HashMap<String, CachedJwk>,
    acquired_at: Instant,
}

/// Move-only snapshot captured from this instance's trusted local JWKS publisher.
///
/// Valid JSON does **not** establish provenance. The trusted composition root
/// must read the authoritative local publisher/primary registry, never a bearer
/// body/header, persisted external configuration, stale copy or bootstrap PEM.
/// The transport endpoint may differ from the public token issuer.
///
/// Capture time precedes the local read and parsing; consuming this seed never
/// renews its strict 60-second authorization lifetime. There is no retained
/// bootstrap fallback after authoritative refresh replaces the snapshot.
pub struct LocalJwksSeed {
    snapshot: Snapshot,
    authority_url: reqwest::Url,
}

impl LocalJwksSeed {
    /// Capture and validate the canonical snapshot of the local publisher.
    ///
    /// This factory performs no HTTP itself. The reader must only read the
    /// trusted local authority and serialize its complete publication snapshot,
    /// using the same filtering/serializer as the configured JWKS endpoint.
    /// Acquisition is dated before invoking/awaiting the reader, not at return
    /// or subsequent extractor construction. Reader errors propagate unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] for an invalid HTTP(S) authority URL (including
    /// userinfo or fragment), reader failure, a document exceeding 1 MiB, or
    /// invalid JSON/RSA keys/canonical trust metadata. Empty sets are valid;
    /// Pending/Revoked keys remain untrusted. Freshness and configured platform
    /// issuer are checked when the seed is consumed by the extractor.
    pub async fn capture<F, Fut>(
        authority_url: impl AsRef<str> + Send,
        read_local_snapshot: F,
    ) -> Result<Self, CommandError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = Result<String, CommandError>> + Send,
    {
        let acquired_at = Instant::now();
        let authority_url = normalize_authority_url(authority_url.as_ref())?;
        let document = read_local_snapshot().await?;
        if document.len() > MAX_DOCUMENT_BYTES {
            return Err(CommandError::authentication(
                "invalid_jwks",
                "JWKS document too large",
            ));
        }
        let keys = parse_jwks_document(&document)?;
        Ok(Self {
            snapshot: Snapshot { keys, acquired_at },
            authority_url,
        })
    }
}

pub(crate) fn normalize_authority_url(raw: &str) -> Result<reqwest::Url, CommandError> {
    let invalid =
        || CommandError::authentication("invalid_jwks_config", "Invalid seeded JWKS authority URL");
    let raw = raw.trim();
    let url = reqwest::Url::parse(raw).map_err(|_| invalid())?;
    // The URL parser can discard empty userinfo. Reject its raw delimiter too,
    // but only in the authority (an '@' in path/query is not userinfo).
    let has_userinfo = raw.split_once(':').is_some_and(|(_, rest)| {
        // HTTP(S) URL parsing also accepts extra slashes/backslashes after the
        // scheme. Do not let those conceal empty userinfo ('@host').
        rest.trim_start_matches(['/', '\\'])
            .split(['/', '\\', '?', '#'])
            .next()
            .is_some_and(|authority| authority.contains('@'))
    });
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || has_userinfo
        || raw.chars().any(|c| matches!(c, '\t' | '\r' | '\n'))
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    Ok(url)
}

/// RSA verification material keyed by opaque `kid`, plus the JWK's custom `iss`.
#[derive(Clone)]
pub(crate) struct CachedJwk {
    pub decoding_key: DecodingKey,
    pub iss: String,
    pub organization_id: Option<uuid::Uuid>,
    pub trusted: bool,
    pub acquired_at: Instant,
}

/// In-memory JWKS cache. Known kids verify locally; unknown kids trigger a
/// coalesced refresh when a URL is configured.
pub(crate) struct JwksCache {
    url: Option<String>,
    snapshot: RwLock<Option<Snapshot>>,
    last_attempt: RwLock<Option<Instant>>,
    negative: RwLock<HashMap<String, Instant>>,
    negative_ttl: Duration,
    refresh_interval: Duration,
    refresh_lock: tokio::sync::Mutex<()>,
    http: Option<reqwest::Client>,
    refresh_started: AtomicBool,
}

impl JwksCache {
    /// Install only during construction of a fresh URL-backed extractor.
    /// The opaque seed is consumed, with no second fallback copy or re-aging.
    pub(crate) fn install_local_seed(
        &self,
        seed: LocalJwksSeed,
        platform_issuer: &str,
    ) -> Result<(), CommandError> {
        let url = self.url.as_deref().ok_or_else(|| {
            CommandError::authentication("invalid_jwks_config", "Seed requires a JWKS URL")
        })?;
        if normalize_authority_url(url)? != seed.authority_url {
            return Err(CommandError::authentication(
                "invalid_jwks_config",
                "Seed and configured JWKS endpoints differ",
            ));
        }
        if seed
            .snapshot
            .keys
            .values()
            .any(|key| key.organization_id.is_none() && key.iss != platform_issuer)
        {
            return Err(CommandError::authentication(
                "invalid_jwks",
                "Seed platform issuer does not match configuration",
            ));
        }
        let mut snapshot = self
            .snapshot
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        if seed.snapshot.acquired_at.elapsed() >= MAX_SNAPSHOT_AGE {
            return Err(CommandError::authentication(
                "invalid_jwks",
                "Local JWKS seed has expired",
            ));
        }
        *snapshot = Some(seed.snapshot);
        Ok(())
    }

    /// Build a cache from an inline JWKS JSON document (no network).
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] if the document cannot be parsed or contains
    /// invalid RSA keys or missing canonical trust metadata. Empty sets are valid.
    pub(crate) fn from_inline_json(jwks_json: &str) -> Result<Arc<Self>, CommandError> {
        let keys = parse_jwks_document(jwks_json)?;
        Ok(Arc::new(Self {
            url: None,
            snapshot: RwLock::new(Some(Snapshot {
                keys,
                acquired_at: Instant::now(),
            })),
            last_attempt: RwLock::new(None),
            negative: RwLock::new(HashMap::new()),
            negative_ttl: Duration::from_secs(30),
            refresh_interval: Duration::from_secs(60),
            refresh_lock: tokio::sync::Mutex::new(()),
            http: None,
            refresh_started: AtomicBool::new(true), // no periodic refresh for inline
        }))
    }

    /// Build a cache that may fetch from `url`.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] if `url` is empty after trimming.
    pub(crate) fn from_url(
        url: String,
        refresh_interval_secs: u64,
        negative_cache_ttl_secs: u64,
    ) -> Result<Arc<Self>, CommandError> {
        let url = url.trim().to_string();
        if url.is_empty() {
            return Err(CommandError::authentication(
                "invalid_jwks_config",
                "JWKS URL is empty",
            ));
        }

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_millis(500))
            .timeout(Duration::from_millis(500))
            .build()
            .map_err(|e| {
                CommandError::authentication(
                    "invalid_jwks_config",
                    format!("failed to build JWKS HTTP client: {e}"),
                )
            })?;

        Ok(Arc::new(Self {
            url: Some(url),
            snapshot: RwLock::new(None),
            last_attempt: RwLock::new(None),
            negative: RwLock::new(HashMap::new()),
            negative_ttl: Duration::from_secs(negative_cache_ttl_secs.max(1)),
            refresh_interval: Duration::from_secs(refresh_interval_secs.clamp(1, 60)),
            refresh_lock: tokio::sync::Mutex::new(()),
            http: Some(client),
            refresh_started: AtomicBool::new(false),
        }))
    }

    /// Look up a kid; on miss, singleflight-refresh then retry (when URL set).
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] if the kid is unknown / negatively cached, or
    /// if a network refresh fails and the kid was never seen.
    pub(crate) async fn resolve_key(
        self: &Arc<Self>,
        kid: &str,
    ) -> Result<CachedJwk, CommandError> {
        if kid.is_empty() || kid.len() > MAX_KID_BYTES {
            return Err(CommandError::authentication(
                "invalid_token",
                "Invalid kid length",
            ));
        }
        self.ensure_periodic_refresh();

        if let Some(key) = self.get_cached(kid) {
            return Ok(key);
        }

        if self.is_negatively_cached(kid) {
            return Err(CommandError::authentication(
                "unknown_kid",
                "JWT kid not found in JWKS",
            ));
        }

        // Inline-only cache: no URL to refresh.
        if self.url.is_none() {
            return Err(CommandError::authentication(
                "unknown_kid",
                "JWT kid not found in JWKS",
            ));
        }

        self.refresh_and_lookup(kid).await
    }

    async fn refresh_and_lookup(&self, kid: &str) -> Result<CachedJwk, CommandError> {
        let Ok(_guard) = self.refresh_lock.try_lock() else {
            return self.get_cached(kid).ok_or_else(|| {
                CommandError::authentication("jwks_refresh_pending", "JWKS refresh in progress")
            });
        };

        // Another waiter may have populated the cache while we waited.
        if let Some(key) = self.get_cached(kid) {
            return Ok(key);
        }
        if self.is_negatively_cached(kid) {
            return Err(CommandError::authentication(
                "unknown_kid",
                "JWT kid not found in JWKS",
            ));
        }

        match self.fetch_and_apply().await {
            Ok(()) => {}
            Err(e) => {
                // Last-known-good: if we still don't have the kid, surface the
                // refresh error (or unknown_kid if keys exist but not this kid).
                warn!("JWKS refresh failed: {e}");
                if let Some(key) = self.get_cached(kid) {
                    return Ok(key);
                }
                return Err(e);
            }
        }

        if let Some(key) = self.get_cached(kid) {
            return Ok(key);
        }

        self.insert_negative(kid);
        Err(CommandError::authentication(
            "unknown_kid",
            "JWT kid not found in JWKS",
        ))
    }

    async fn fetch_and_apply(&self) -> Result<(), CommandError> {
        let acquired_at = Instant::now();
        {
            let mut attempt = self
                .last_attempt
                .write()
                .unwrap_or_else(PoisonError::into_inner);
            if attempt
                .is_some_and(|at| acquired_at.saturating_duration_since(at) < MIN_REFRESH_INTERVAL)
            {
                return Ok(());
            }
            *attempt = Some(acquired_at);
        }
        let url = self.url.as_ref().ok_or_else(|| {
            CommandError::authentication("invalid_jwks_config", "JWKS URL not configured")
        })?;
        let client = self.http.as_ref().ok_or_else(|| {
            CommandError::authentication("invalid_jwks_config", "JWKS HTTP client not configured")
        })?;

        debug!("Refreshing JWKS from {url}");
        let mut response = client.get(url.as_str()).send().await.map_err(|e| {
            CommandError::authentication("jwks_fetch_failed", format!("JWKS fetch failed: {e}"))
        })?;

        if !response.status().is_success() {
            return Err(CommandError::authentication(
                "jwks_fetch_failed",
                format!("JWKS fetch returned HTTP {}", response.status()),
            ));
        }

        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| {
            CommandError::authentication("jwks_fetch_failed", "JWKS response body read failed")
        })? {
            if bytes.len().saturating_add(chunk.len()) > MAX_DOCUMENT_BYTES {
                return Err(CommandError::authentication(
                    "invalid_jwks",
                    "JWKS document too large",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let body = String::from_utf8(bytes).map_err(|_| {
            CommandError::authentication("invalid_jwks", "JWKS document is not UTF-8")
        })?;

        let parsed = parse_jwks_document(&body)?;
        self.apply_snapshot(parsed, acquired_at);
        // Successful refresh clears negative entries so kids can be retried.
        let mut negative = self
            .negative
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        negative.clear();
        Ok(())
    }

    fn get_cached(&self, kid: &str) -> Option<CachedJwk> {
        self.get_cached_at(kid, Instant::now())
    }

    fn get_cached_at(&self, kid: &str, now: Instant) -> Option<CachedJwk> {
        let guard = self.snapshot.read().unwrap_or_else(PoisonError::into_inner);
        let snapshot = guard.as_ref()?;
        if now.saturating_duration_since(snapshot.acquired_at) >= MAX_SNAPSHOT_AGE {
            return None;
        }
        snapshot.keys.get(kid).cloned().map(|mut key| {
            key.acquired_at = snapshot.acquired_at;
            key
        })
    }

    pub(crate) fn still_authorizes(&self, kid: &str, acquired_at: Instant) -> bool {
        self.get_cached(kid)
            .is_some_and(|key| key.trusted && key.acquired_at == acquired_at)
    }

    fn apply_snapshot(&self, keys: HashMap<String, CachedJwk>, acquired_at: Instant) {
        *self
            .snapshot
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(Snapshot { keys, acquired_at });
    }

    fn is_negatively_cached(&self, kid: &str) -> bool {
        self.is_negatively_cached_at(kid, Instant::now())
    }

    fn is_negatively_cached_at(&self, kid: &str, now: Instant) -> bool {
        let mut negative = self
            .negative
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        match negative.get(kid) {
            Some(expires_at) if now < *expires_at => true,
            Some(_) => {
                negative.remove(kid);
                false
            }
            None => false,
        }
    }

    fn insert_negative(&self, kid: &str) {
        let mut negative = self
            .negative
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        let now = Instant::now();
        negative.retain(|_, expires_at| now < *expires_at);
        if kid.len() <= MAX_KID_BYTES && negative.len() < MAX_NEGATIVE_KIDS {
            negative.insert(kid.to_string(), now + self.negative_ttl);
        }
    }

    fn ensure_periodic_refresh(self: &Arc<Self>) {
        if self.url.is_none() {
            return;
        }
        if self
            .refresh_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        let this = Arc::clone(self);
        tokio::spawn(async move {
            this.run_periodic_refresh().await;
        });
    }

    async fn run_periodic_refresh(self: Arc<Self>) {
        let mut ticker = tokio::time::interval(self.refresh_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // Skip the immediate first tick — resolve_key refreshes on demand.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            let _guard = self.refresh_lock.lock().await;
            if let Err(e) = self.fetch_and_apply().await {
                warn!("Periodic JWKS refresh failed (snapshot age unchanged): {e}");
            }
        }
    }
}

/// Parse a JWKS JSON document into kid → (DecodingKey, iss).
///
/// The custom JWK field `iss` is required for every key we accept.
///
/// # Errors
///
/// Returns [`CommandError`] on invalid JSON, keys or canonical trust metadata.
pub(crate) fn parse_jwks_document(
    jwks_json: &str,
) -> Result<HashMap<String, CachedJwk>, CommandError> {
    let root: serde_json::Value = serde_json::from_str(jwks_json).map_err(|e| {
        CommandError::authentication("invalid_jwks", format!("invalid JWKS JSON: {e}"))
    })?;

    let keys = root.get("keys").and_then(|v| v.as_array()).ok_or_else(|| {
        CommandError::authentication("invalid_jwks", "JWKS document missing keys array")
    })?;

    let mut out = HashMap::new();
    for key_value in keys {
        let invalid =
            || CommandError::authentication("invalid_jwks", "Invalid JWKS trust metadata");
        let status = key_value
            .get("status")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(invalid)?;
        let trusted = match status {
            "active" | "retiring" => true,
            "pending" | "revoked" => false,
            _ => return Err(invalid()),
        };
        let organization_id = match key_value
            .get("trust_scope")
            .and_then(serde_json::Value::as_str)
        {
            Some("platform")
                if key_value
                    .get("organization_id")
                    .is_some_and(serde_json::Value::is_null) =>
            {
                None
            }
            Some("organization") => {
                let raw = key_value
                    .get("organization_id")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(invalid)?;
                let id = uuid::Uuid::parse_str(raw).map_err(|_| invalid())?;
                if id.to_string() != raw {
                    return Err(invalid());
                }
                Some(id)
            }
            _ => return Err(invalid()),
        };
        let Some(kid) = key_value.get("kid").and_then(|v| v.as_str()) else {
            return Err(invalid());
        };
        let Some(iss) = key_value.get("iss").and_then(|v| v.as_str()) else {
            return Err(invalid());
        };
        if iss.trim().is_empty()
            || kid.is_empty()
            || kid.len() > MAX_KID_BYTES
            || out.contains_key(kid)
        {
            return Err(invalid());
        }
        if kid.contains('/') || kid.contains('\\') || kid.contains("://") || kid.contains("..") {
            debug!(kid = kid, "Skipping JWK with dangerous kid");
            return Err(invalid());
        }
        let kty = key_value.get("kty").and_then(|v| v.as_str()).unwrap_or("");
        if kty != "RSA" {
            debug!(
                kid = kid,
                kty = kty,
                "Skipping non-RSA JWK (HMAC never trusted)"
            );
            return Err(invalid());
        }
        if let Some(alg) = key_value.get("alg").and_then(|v| v.as_str()) {
            if alg != "RS256" {
                debug!(kid = kid, alg = alg, "Skipping JWK with non-RS256 alg");
                return Err(invalid());
            }
        }
        if key_value
            .get("use")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|usage| usage != "sig")
        {
            return Err(invalid());
        }

        let jwk: Jwk = serde_json::from_value(key_value.clone()).map_err(|e| {
            CommandError::authentication(
                "invalid_jwks",
                format!("failed to parse JWK kid={kid}: {e}"),
            )
        })?;

        let decoding_key = DecodingKey::from_jwk(&jwk).map_err(|e| {
            CommandError::authentication(
                "invalid_jwks",
                format!("failed to build decoding key for kid={kid}: {e}"),
            )
        })?;

        out.insert(
            kid.to_string(),
            CachedJwk {
                decoding_key,
                iss: iss.to_string(),
                organization_id,
                trusted,
                acquired_at: Instant::now(),
            },
        );
    }

    if out.is_empty() && !keys.is_empty() {
        return Err(CommandError::authentication(
            "invalid_jwks",
            "JWKS document contains no usable RSA keys with kid and iss",
        ));
    }

    Ok(out)
}

#[cfg(all(test, feature = "testing"))]
#[path = "jwks_seed_tests.rs"]
mod seed_tests;

#[cfg(test)]
mod freshness_tests {
    use super::*;

    fn keys() -> HashMap<String, CachedJwk> {
        // State-machine-only fixture; cryptographic/JWKS tests use RSA fixtures.
        HashMap::from([(
            "kid".into(),
            CachedJwk {
                decoding_key: DecodingKey::from_secret(b"state-only-nonsecret"),
                iss: "https://issuer.example/iam".into(),
                organization_id: None,
                trusted: true,
                acquired_at: Instant::now(),
            },
        )])
    }

    #[test]
    fn outage_known_hits_expire_empty_revokes_and_recovery_replaces() {
        let cache = JwksCache::from_inline_json(r#"{"keys":[]}"#).unwrap();
        let start = Instant::now();
        assert!(cache.get_cached_at("kid", start).is_none());
        cache.apply_snapshot(keys(), start);
        for second in [0, 1, 30, 59] {
            assert!(cache
                .get_cached_at("kid", start + Duration::from_secs(second))
                .is_some());
        }
        *cache.last_attempt.write().unwrap() = Some(start + Duration::from_secs(59));
        assert!(cache
            .get_cached_at("kid", start + Duration::from_secs(60))
            .is_none());
        assert!(cache
            .get_cached_at("kid", start + Duration::from_secs(3600))
            .is_none());
        cache.apply_snapshot(HashMap::new(), start + Duration::from_secs(61));
        assert!(cache
            .get_cached_at("kid", start + Duration::from_secs(61))
            .is_none());
        cache.apply_snapshot(keys(), start + Duration::from_secs(62));
        assert!(cache
            .get_cached_at("kid", start + Duration::from_secs(63))
            .is_some());
        assert!(parse_jwks_document(r#"{"keys":null}"#).is_err());
        assert!(parse_jwks_document(r#"{"keys":[]}"#).unwrap().is_empty());
    }

    #[test]
    fn negative_cache_is_bounded_and_expires_at_boundary() {
        let cache = JwksCache::from_inline_json(r#"{"keys":[]}"#).unwrap();
        for index in 0..MAX_NEGATIVE_KIDS * 2 {
            cache.insert_negative(&index.to_string());
        }
        assert_eq!(cache.negative.read().unwrap().len(), MAX_NEGATIVE_KIDS);
        let expiry = *cache.negative.read().unwrap().get("0").unwrap();
        assert!(cache.is_negatively_cached_at("0", expiry - Duration::from_nanos(1)));
        assert!(!cache.is_negatively_cached_at("0", expiry));
        assert!(!cache.negative.read().unwrap().contains_key("0"));
    }
}
