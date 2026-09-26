//! JWKS document cache with singleflight refresh and negative caching.

use crate::rustycog_command::CommandError;
use jsonwebtoken::{jwk::Jwk, DecodingKey};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, PoisonError, RwLock,
    },
    time::{Duration, Instant},
};
use tracing::{debug, warn};

/// RSA verification material keyed by opaque `kid`, plus the JWK's custom `iss`.
#[derive(Clone)]
pub(crate) struct CachedJwk {
    pub decoding_key: DecodingKey,
    pub iss: String,
}

/// In-memory JWKS cache. Known kids verify locally; unknown kids trigger a
/// coalesced refresh when a URL is configured.
pub(crate) struct JwksCache {
    url: Option<String>,
    keys: RwLock<HashMap<String, CachedJwk>>,
    negative: RwLock<HashMap<String, Instant>>,
    negative_ttl: Duration,
    refresh_interval: Duration,
    refresh_lock: tokio::sync::Mutex<()>,
    http: Option<reqwest::Client>,
    refresh_started: AtomicBool,
}

impl JwksCache {
    /// Build a cache from an inline JWKS JSON document (no network).
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] if the document cannot be parsed or contains
    /// no usable RSA keys with both `kid` and `iss`.
    pub(crate) fn from_inline_json(jwks_json: &str) -> Result<Arc<Self>, CommandError> {
        let keys = parse_jwks_document(jwks_json)?;
        Ok(Arc::new(Self {
            url: None,
            keys: RwLock::new(keys),
            negative: RwLock::new(HashMap::new()),
            negative_ttl: Duration::from_secs(30),
            refresh_interval: Duration::from_secs(300),
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
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| {
                CommandError::authentication(
                    "invalid_jwks_config",
                    format!("failed to build JWKS HTTP client: {e}"),
                )
            })?;

        Ok(Arc::new(Self {
            url: Some(url),
            keys: RwLock::new(HashMap::new()),
            negative: RwLock::new(HashMap::new()),
            negative_ttl: Duration::from_secs(negative_cache_ttl_secs.max(1)),
            refresh_interval: Duration::from_secs(refresh_interval_secs.max(1)),
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
        let _guard = self.refresh_lock.lock().await;

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
        let url = self.url.as_ref().ok_or_else(|| {
            CommandError::authentication("invalid_jwks_config", "JWKS URL not configured")
        })?;
        let client = self.http.as_ref().ok_or_else(|| {
            CommandError::authentication("invalid_jwks_config", "JWKS HTTP client not configured")
        })?;

        debug!("Refreshing JWKS from {url}");
        let response = client.get(url.as_str()).send().await.map_err(|e| {
            CommandError::authentication("jwks_fetch_failed", format!("JWKS fetch failed: {e}"))
        })?;

        if !response.status().is_success() {
            return Err(CommandError::authentication(
                "jwks_fetch_failed",
                format!("JWKS fetch returned HTTP {}", response.status()),
            ));
        }

        let body = response.text().await.map_err(|e| {
            CommandError::authentication(
                "jwks_fetch_failed",
                format!("JWKS response body read failed: {e}"),
            )
        })?;

        let parsed = parse_jwks_document(&body)?;
        let mut keys = self.keys.write().unwrap_or_else(PoisonError::into_inner);
        *keys = parsed;
        // Successful refresh clears negative entries so kids can be retried.
        let mut negative = self
            .negative
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        negative.clear();
        Ok(())
    }

    fn get_cached(&self, kid: &str) -> Option<CachedJwk> {
        self.keys
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(kid)
            .cloned()
    }

    fn is_negatively_cached(&self, kid: &str) -> bool {
        let mut negative = self
            .negative
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        match negative.get(kid) {
            Some(expires_at) if Instant::now() < *expires_at => true,
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
        negative.insert(kid.to_string(), Instant::now() + self.negative_ttl);
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
                warn!("Periodic JWKS refresh failed (keeping last-known-good): {e}");
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
/// Returns [`CommandError`] on invalid JSON or when no usable keys are found.
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
        let Some(kid) = key_value.get("kid").and_then(|v| v.as_str()) else {
            debug!("Skipping JWK without kid");
            continue;
        };
        let Some(iss) = key_value.get("iss").and_then(|v| v.as_str()) else {
            debug!(kid = kid, "Skipping JWK without custom iss field");
            continue;
        };
        if iss.trim().is_empty() {
            continue;
        }
        if kid.contains('/') || kid.contains('\\') || kid.contains("://") || kid.contains("..") {
            debug!(kid = kid, "Skipping JWK with dangerous kid");
            continue;
        }
        let kty = key_value.get("kty").and_then(|v| v.as_str()).unwrap_or("");
        if !kty.eq_ignore_ascii_case("RSA") {
            debug!(kid = kid, kty = kty, "Skipping non-RSA JWK (HMAC never trusted)");
            continue;
        }
        if let Some(alg) = key_value.get("alg").and_then(|v| v.as_str()) {
            if !alg.eq_ignore_ascii_case("RS256") {
                debug!(kid = kid, alg = alg, "Skipping JWK with non-RS256 alg");
                continue;
            }
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
            },
        );
    }

    if out.is_empty() {
        return Err(CommandError::authentication(
            "invalid_jwks",
            "JWKS document contains no usable RSA keys with kid and iss",
        ));
    }

    Ok(out)
}
