//! Private clock/state seams: no caller-supplied Instant and no 60-second sleeps.
use super::super::jwt_handler::UserIdExtractor;
use super::*;
use crate::rustycog_config::{AuthConfig, JwtAuthConfig};
use crate::testing::http::jwt::{
    test_rs256_jwks_json, TEST_JWT_AUDIENCE, TEST_PLATFORM_ISSUER, TEST_RS256_KID,
};
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

const AUTHORITY: &str = "http://publisher.internal/iam/.well-known/jwks.json";

fn auth(url: &str) -> AuthConfig {
    AuthConfig {
        jwt: JwtAuthConfig {
            allowed_algorithms: vec!["RS256".into()],
            jwks_url: Some(url.into()),
            issuer: Some(TEST_PLATFORM_ISSUER.into()),
            audience: Some(TEST_JWT_AUDIENCE.into()),
            ..JwtAuthConfig::default()
        },
        ..AuthConfig::default()
    }
}

async fn seed(url: &str, document: String) -> LocalJwksSeed {
    LocalJwksSeed::capture(url, || async move { Ok(document) })
        .await
        .expect("trusted local fixture snapshot")
}

fn seeded_cache(url: &str, seed: LocalJwksSeed) -> Arc<JwksCache> {
    let cache = JwksCache::from_url(url.into(), 60, 30).unwrap();
    cache
        .install_local_seed(seed, TEST_PLATFORM_ISSUER)
        .unwrap();
    cache
}

#[tokio::test]
async fn seed_factory_dates_before_async_reader_and_install_does_not_reage() {
    let (started, observed) = tokio::sync::oneshot::channel();
    let (release, wait) = tokio::sync::oneshot::channel();
    let before = Instant::now();
    let capture = tokio::spawn(LocalJwksSeed::capture(AUTHORITY, || async move {
        started.send(Instant::now()).unwrap();
        wait.await.unwrap();
        Ok(test_rs256_jwks_json())
    }));
    let reader_started = observed.await.unwrap();
    assert!(!capture.is_finished(), "reader must actually be suspended");
    tokio::task::yield_now().await;
    release.send(()).unwrap();
    let captured = capture.await.unwrap().unwrap();
    let acquired_at = captured.snapshot.acquired_at;
    assert!(before <= acquired_at && acquired_at <= reader_started);
    assert!(
        acquired_at < Instant::now(),
        "read and parse time count toward age"
    );
    tokio::task::yield_now().await; // consumption is later, not capture time
    let cache = seeded_cache(AUTHORITY, captured);
    assert_eq!(
        cache.get_cached(TEST_RS256_KID).unwrap().acquired_at,
        acquired_at
    );
    assert_eq!(
        cache.snapshot.read().unwrap().as_ref().unwrap().acquired_at,
        acquired_at
    );
    assert!(!cache.refresh_started.load(Ordering::SeqCst));
    assert!(cache.last_attempt.read().unwrap().is_none());
}

#[tokio::test]
async fn seed_constructor_rejects_sixty_second_boundary_and_keeps_construction_delay() {
    for age in [60, 61] {
        let mut captured = seed(AUTHORITY, test_rs256_jwks_json()).await;
        captured.snapshot.acquired_at = Instant::now() - Duration::from_secs(age);
        assert!(UserIdExtractor::from_config_with_seeded_jwks(auth(AUTHORITY), captured).is_err());
    }
    let mut captured = seed(AUTHORITY, test_rs256_jwks_json()).await;
    let original = Instant::now() - Duration::from_secs(30);
    captured.snapshot.acquired_at = original;
    let cache = seeded_cache(AUTHORITY, captured);
    let key = cache.get_cached(TEST_RS256_KID).unwrap();
    assert_eq!(key.acquired_at, original);
    assert!(cache
        .get_cached_at(TEST_RS256_KID, original + Duration::from_millis(59_999))
        .is_some());
    assert!(cache
        .get_cached_at(TEST_RS256_KID, original + Duration::from_secs(60))
        .is_none());
    // Repeated known/negative hits do not mutate acquisition time.
    cache.insert_negative("unknown");
    assert!(cache.is_negatively_cached("unknown"));
    assert!(cache.get_cached(TEST_RS256_KID).is_some());
    assert_eq!(
        cache.snapshot.read().unwrap().as_ref().unwrap().acquired_at,
        original
    );
    cache
        .snapshot
        .write()
        .unwrap()
        .as_mut()
        .unwrap()
        .acquired_at = Instant::now() - MAX_SNAPSHOT_AGE;
    assert!(!cache.still_authorizes(TEST_RS256_KID, original));
}

#[tokio::test]
async fn seed_configuration_is_strict_and_normalizes_only_the_authority_endpoint() {
    let captured = seed(
        "HTTP://EXAMPLE.COM:80/a/../jwks?tenant=1",
        test_rs256_jwks_json(),
    )
    .await;
    let mut config = auth("http://example.com/jwks?tenant=1");
    config.mesh.trusted_gateway_san = "spiffe://mesh/gateway".into();
    let extractor = UserIdExtractor::from_config_with_seeded_jwks(config, captured).unwrap();
    assert_eq!(extractor.gateway_san(), Some("spiffe://mesh/gateway"));
    // Public token issuer intentionally differs from the transport authority.
    for url in [
        "https://publisher.internal/iam/.well-known/jwks.json",
        "http://other.internal/iam/.well-known/jwks.json",
        "http://publisher.internal:8080/iam/.well-known/jwks.json",
        "http://publisher.internal/other",
        "http://publisher.internal/iam/.well-known/jwks.json?tenant=1",
    ] {
        let captured = seed(AUTHORITY, test_rs256_jwks_json()).await;
        assert!(UserIdExtractor::from_config_with_seeded_jwks(auth(url), captured).is_err());
    }
    for missing in [
        "issuer",
        "blank_issuer",
        "audience",
        "blank_audience",
        "url",
        "blank_url",
        "hs_only",
        "unsupported",
        "dual_without_secret",
    ] {
        let mut config = auth(AUTHORITY);
        match missing {
            "issuer" => config.jwt.issuer = None,
            "blank_issuer" => config.jwt.issuer = Some("  ".into()),
            "audience" => config.jwt.audience = None,
            "blank_audience" => config.jwt.audience = Some("  ".into()),
            "url" => config.jwt.jwks_url = None,
            "blank_url" => config.jwt.jwks_url = Some("  ".into()),
            "hs_only" => {
                config.jwt.allowed_algorithms = vec!["HS256".into()];
                config.jwt.hs256_secret = Some("independent-hmac-fixture-secret".into());
            }
            "unsupported" => config.jwt.allowed_algorithms = vec!["ES256".into()],
            _ => {
                config.jwt.allowed_algorithms = vec!["RS256".into(), "HS256".into()];
                config.jwt.hs256_secret = None;
            }
        }
        let captured = seed(AUTHORITY, test_rs256_jwks_json()).await;
        assert!(
            UserIdExtractor::from_config_with_seeded_jwks(config, captured).is_err(),
            "{missing}"
        );
    }
    let mut config = auth(AUTHORITY);
    config.jwt.allowed_algorithms.clear(); // existing URL default resolves RS256 only
    assert!(UserIdExtractor::from_config_with_seeded_jwks(
        config,
        seed(AUTHORITY, test_rs256_jwks_json()).await
    )
    .is_ok());
    let mut document: serde_json::Value = serde_json::from_str(&test_rs256_jwks_json()).unwrap();
    document["keys"][0]["iss"] = serde_json::json!("https://other-platform.example/iam");
    assert!(UserIdExtractor::from_config_with_seeded_jwks(
        auth(AUTHORITY),
        seed(AUTHORITY, document.to_string()).await
    )
    .is_err());
}

#[tokio::test]
async fn seed_factory_and_constructor_refuse_userinfo_fragments_and_non_http_urls() {
    for url in [
        "",
        "not a URL",
        "ftp://host/jwks",
        "file:///jwks",
        "http://user@host/jwks",
        "http://user:pass@host/jwks",
        "http://@host/jwks",
        "http:////@host/jwks",
        r"http:\\@host/jwks",
        "http://\t@host/jwks",
        "http://host/jwks#",
        "http://host/jwks#fragment",
    ] {
        let read = Arc::new(AtomicBool::new(false));
        let witness = read.clone();
        assert!(
            LocalJwksSeed::capture(url, || async move {
                witness.store(true, Ordering::SeqCst);
                Ok(test_rs256_jwks_json())
            })
            .await
            .is_err(),
            "{url}"
        );
        assert!(
            !read.load(Ordering::SeqCst),
            "invalid endpoint must fail before reader"
        );
        let captured = seed(AUTHORITY, test_rs256_jwks_json()).await;
        assert!(UserIdExtractor::from_config_with_seeded_jwks(auth(url), captured).is_err());
    }
    assert!(normalize_authority_url("https://host/jwks/@local?reader=@local").is_ok());
    let error = LocalJwksSeed::capture(AUTHORITY, || async {
        Err::<String, _>(CommandError::authentication(
            "local_read_failed",
            "Local publisher failed",
        ))
    })
    .await;
    match error {
        Err(CommandError::Authentication { code, message }) => {
            assert_eq!(code, "local_read_failed");
            assert_eq!(message, "Local publisher failed");
        }
        _ => {
            panic!("reader failure must propagate unchanged, not become an empty/fallback snapshot")
        }
    }
}

#[tokio::test]
async fn seed_uses_live_canonical_parser_and_exact_one_mib_bound() {
    let valid: serde_json::Value = serde_json::from_str(&test_rs256_jwks_json()).unwrap();
    let mut invalid = vec!["not JSON".into(), "{}".into(), r#"{"keys":null}"#.into()];
    for field in [
        "status",
        "trust_scope",
        "organization_id",
        "kid",
        "iss",
        "kty",
        "n",
        "e",
    ] {
        let mut doc = valid.clone();
        doc["keys"][0].as_object_mut().unwrap().remove(field);
        invalid.push(doc.to_string());
    }
    for (field, value) in [
        ("status", "unknown"),
        ("trust_scope", "unknown"),
        ("organization_id", "not-null"),
        ("kid", ""),
        ("kid", "../unsafe"),
        ("iss", "  "),
        ("kty", "oct"),
        ("alg", "HS256"),
        ("use", "enc"),
        ("n", "!invalid-base64!"),
        ("e", "!invalid-base64!"),
    ] {
        let mut doc = valid.clone();
        doc["keys"][0][field] = serde_json::json!(value);
        invalid.push(doc.to_string());
    }
    let mut duplicate = valid.clone();
    duplicate["keys"]
        .as_array_mut()
        .unwrap()
        .push(valid["keys"][0].clone());
    invalid.push(duplicate.to_string());
    let mut long_kid = valid.clone();
    long_kid["keys"][0]["kid"] = serde_json::json!("k".repeat(MAX_KID_BYTES + 1));
    invalid.push(long_kid.to_string());
    let mut org = valid.clone();
    org["keys"][0]["trust_scope"] = serde_json::json!("organization");
    for owner in [
        serde_json::Value::Null,
        serde_json::json!("bad-owner"),
        serde_json::json!(uuid::Uuid::new_v4().simple().to_string()),
    ] {
        org["keys"][0]["organization_id"] = owner;
        invalid.push(org.to_string());
    }
    for doc in invalid {
        assert!(LocalJwksSeed::capture(AUTHORITY, || async { Ok(doc) })
            .await
            .is_err());
    }
    let base = r#"{"keys":[],"padding":""}"#;
    let padding = "x".repeat(MAX_DOCUMENT_BYTES - base.len());
    let exact = format!(r#"{{"keys":[],"padding":"{padding}"}}"#);
    assert_eq!(exact.len(), MAX_DOCUMENT_BYTES);
    assert!(
        LocalJwksSeed::capture(AUTHORITY, || async { Ok(exact.clone()) })
            .await
            .is_ok()
    );
    assert!(
        LocalJwksSeed::capture(AUTHORITY, || async { Ok(exact + " ") })
            .await
            .is_err()
    );
}

#[tokio::test]
async fn seed_empty_and_untrusted_statuses_do_not_gain_authority() {
    let empty = seed(AUTHORITY, r#"{"keys":[]}"#.into()).await;
    assert!(UserIdExtractor::from_config_with_seeded_jwks(auth(AUTHORITY), empty).is_ok());
    for status in ["pending", "revoked", "retiring"] {
        let mut doc: serde_json::Value = serde_json::from_str(&test_rs256_jwks_json()).unwrap();
        doc["keys"][0]["status"] = serde_json::json!(status);
        let cache = seeded_cache(AUTHORITY, seed(AUTHORITY, doc.to_string()).await);
        let key = cache.get_cached(TEST_RS256_KID).unwrap();
        assert_eq!(key.trusted, status == "retiring");
        assert_eq!(
            cache.still_authorizes(TEST_RS256_KID, key.acquired_at),
            status == "retiring"
        );
    }
}

#[tokio::test]
async fn seed_lookup_skips_acquisition_and_unknown_burst_remains_nonwaiting() {
    let server = MockServer::start().await;
    let url = server.uri();
    let cache = seeded_cache(&url, seed(&url, test_rs256_jwks_json()).await);
    let flight = cache.refresh_lock.lock().await;
    for _ in 0..5 {
        assert!(cache.resolve_key(TEST_RS256_KID).await.unwrap().trusted);
        let unknown =
            tokio::time::timeout(Duration::from_millis(100), cache.resolve_key("unknown")).await;
        assert!(matches!(
            unknown.expect("unknown caller must not queue behind acquisition"),
            Err(CommandError::Authentication { code, .. }) if code == "jwks_refresh_pending"
        ));
    }
    assert!(server.received_requests().await.unwrap().is_empty());
    assert!(cache.last_attempt.read().unwrap().is_none());
    drop(flight);
}

#[tokio::test]
async fn seed_replacement_and_final_recheck_never_union_or_revive_bootstrap() {
    let cache = seeded_cache(AUTHORITY, seed(AUTHORITY, test_rs256_jwks_json()).await);
    let original = cache.get_cached(TEST_RS256_KID).unwrap().acquired_at;
    for status in ["active", "pending", "revoked"] {
        let mut doc: serde_json::Value = serde_json::from_str(&test_rs256_jwks_json()).unwrap();
        doc["keys"][0]["status"] = serde_json::json!(status);
        doc["keys"][0]["iss"] = serde_json::json!("https://replacement.example/iam");
        cache.apply_snapshot(
            parse_jwks_document(&doc.to_string()).unwrap(),
            Instant::now(),
        );
        assert!(
            !cache.still_authorizes(TEST_RS256_KID, original),
            "final check rejects cloned old seed key"
        );
        assert_eq!(
            cache.get_cached(TEST_RS256_KID).unwrap().iss,
            "https://replacement.example/iam"
        );
    }
    cache.apply_snapshot(HashMap::new(), Instant::now());
    assert!(cache.get_cached(TEST_RS256_KID).is_none());
    assert!(!cache.still_authorizes(TEST_RS256_KID, original));
}

#[tokio::test]
async fn seed_successful_live_empty_pending_revoked_and_changed_kid_replace_old_snapshot() {
    for replacement in ["empty", "pending", "revoked", "new_kid"] {
        let server = MockServer::start().await;
        let url = server.uri();
        let cache = seeded_cache(&url, seed(&url, test_rs256_jwks_json()).await);
        let old = cache.get_cached(TEST_RS256_KID).unwrap();
        let document = if replacement == "empty" {
            r#"{"keys":[]}"#.into()
        } else {
            let mut doc: serde_json::Value = serde_json::from_str(&test_rs256_jwks_json()).unwrap();
            if replacement == "new_kid" {
                doc["keys"][0]["kid"] = serde_json::json!("replacement-platform-key");
            } else {
                doc["keys"][0]["status"] = serde_json::json!(replacement);
            }
            doc.to_string()
        };
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(document))
            .mount(&server)
            .await;
        // Cloned old verification material is held across the actual live
        // acquisition, as in a concurrent request's final authorization check.
        cache.fetch_and_apply().await.unwrap();
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
        assert!(
            !cache.still_authorizes(TEST_RS256_KID, old.acquired_at),
            "{replacement}"
        );
        match replacement {
            "empty" => assert!(cache
                .snapshot
                .read()
                .unwrap()
                .as_ref()
                .unwrap()
                .keys
                .is_empty()),
            "new_kid" => {
                assert!(cache.get_cached(TEST_RS256_KID).is_none());
                let new = cache.get_cached("replacement-platform-key").unwrap();
                assert!(new.trusted);
                assert!(cache.still_authorizes("replacement-platform-key", new.acquired_at));
            }
            _ => {
                let new = cache.get_cached(TEST_RS256_KID).unwrap();
                assert!(!new.trusted);
                assert!(!cache.still_authorizes(TEST_RS256_KID, new.acquired_at));
            }
        }
    }
}

#[tokio::test]
async fn seed_negative_hits_and_stale_bursts_remain_bounded_nonwaiting_and_do_not_reage() {
    let server = MockServer::start().await;
    let url = server.uri();
    let cache = seeded_cache(&url, seed(&url, test_rs256_jwks_json()).await);
    let original = cache.get_cached(TEST_RS256_KID).unwrap().acquired_at;
    for n in 0..(MAX_NEGATIVE_KIDS + 10) {
        cache.insert_negative(&format!("unknown-{n}"));
    }
    assert_eq!(cache.negative.read().unwrap().len(), MAX_NEGATIVE_KIDS);
    cache.insert_negative(&"x".repeat(MAX_KID_BYTES + 1));
    assert_eq!(cache.negative.read().unwrap().len(), MAX_NEGATIVE_KIDS);
    let negative_expiry = cache.negative.read().unwrap()["unknown-0"];
    assert!(cache.is_negatively_cached_at("unknown-0", negative_expiry - Duration::from_nanos(1)));
    assert!(cache.resolve_key("unknown-1").await.is_err());
    assert_eq!(
        cache.get_cached(TEST_RS256_KID).unwrap().acquired_at,
        original
    );
    assert!(!cache.is_negatively_cached_at("unknown-0", negative_expiry));
    assert!(server.received_requests().await.unwrap().is_empty());

    let expired = Instant::now() - MAX_SNAPSHOT_AGE;
    cache
        .snapshot
        .write()
        .unwrap()
        .as_mut()
        .unwrap()
        .acquired_at = expired;
    let flight = cache.refresh_lock.lock().await;
    for _ in 0..24 {
        let result = tokio::time::timeout(
            Duration::from_millis(100),
            cache.resolve_key(TEST_RS256_KID),
        )
        .await;
        assert!(matches!(
            result.expect("stale kid must fail fast, not wait for flight"),
            Err(CommandError::Authentication { code, .. }) if code == "jwks_refresh_pending"
        ));
    }
    assert_eq!(
        cache.snapshot.read().unwrap().as_ref().unwrap().acquired_at,
        expired
    );
    assert!(server.received_requests().await.unwrap().is_empty());
    drop(flight);
}

#[tokio::test]
async fn seed_stale_outage_and_refresh_throttle_do_not_renew_acquisition() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let url = server.uri();
    let cache = seeded_cache(&url, seed(&url, test_rs256_jwks_json()).await);
    let expired = Instant::now() - MAX_SNAPSHOT_AGE;
    cache
        .snapshot
        .write()
        .unwrap()
        .as_mut()
        .unwrap()
        .acquired_at = expired;
    assert!(cache.resolve_key(TEST_RS256_KID).await.is_err());
    assert!(cache.get_cached(TEST_RS256_KID).is_none());
    assert!(!cache.still_authorizes(TEST_RS256_KID, expired));
    assert_eq!(
        cache.snapshot.read().unwrap().as_ref().unwrap().acquired_at,
        expired
    );
    let count = server.received_requests().await.unwrap().len();
    assert_eq!(count, 1);
    // A deterministic private throttle timestamp, not an authorization reset.
    *cache.last_attempt.write().unwrap() = Some(Instant::now());
    cache.fetch_and_apply().await.unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), count);
    assert_eq!(
        cache.snapshot.read().unwrap().as_ref().unwrap().acquired_at,
        expired
    );
}

#[tokio::test]
async fn seed_cancelled_refresh_releases_flight_without_renewing_snapshot() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(2))
                .set_body_string(r#"{"keys":[]}"#),
        )
        .mount(&server)
        .await;
    let url = server.uri();
    let cache = seeded_cache(&url, seed(&url, test_rs256_jwks_json()).await);
    let original = cache.get_cached(TEST_RS256_KID).unwrap().acquired_at;
    let worker_cache = cache.clone();
    let worker = tokio::spawn(async move { worker_cache.resolve_key("unknown").await });
    tokio::time::timeout(Duration::from_secs(1), async {
        while server.received_requests().await.unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("bounded proof that acquisition is in flight");
    assert!(cache.refresh_lock.try_lock().is_err());
    let attempt = *cache.last_attempt.read().unwrap();
    worker.abort();
    match worker.await {
        Err(error) => assert!(error.is_cancelled()),
        Ok(_) => panic!("refresh must still have been in flight when cancelled"),
    }
    assert!(cache.refresh_lock.try_lock().is_ok());
    assert_eq!(*cache.last_attempt.read().unwrap(), attempt);
    assert_eq!(
        cache.get_cached(TEST_RS256_KID).unwrap().acquired_at,
        original
    );
}

#[tokio::test]
async fn seed_live_redirect_is_not_followed_and_failure_keeps_original_age() {
    let target = MockServer::start().await;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", target.uri()))
        .mount(&server)
        .await;
    let url = server.uri();
    let cache = seeded_cache(&url, seed(&url, test_rs256_jwks_json()).await);
    let original = cache.get_cached(TEST_RS256_KID).unwrap().acquired_at;
    assert!(cache.resolve_key("unknown").await.is_err());
    assert!(target.received_requests().await.unwrap().is_empty());
    assert_eq!(
        cache.get_cached(TEST_RS256_KID).unwrap().acquired_at,
        original
    );
}
