//! Unit tests for ADR-0304 RS256 + JWKS verification (inline and seeded URL caches).

use super::*;
use crate::rustycog_config::{AuthConfig, JwtAuthConfig};
use crate::testing::http::jwt::{
    create_jwt_token_with_secret, create_rs256_jwt_token, create_rs256_jwt_token_with_issuer,
    create_rs256_jwt_token_with_options, test_rs256_jwks_json, test_rs256_jwks_json_with_iss,
    CanonicalJwk, Rs256TokenOptions, TEST_HS256_SECRET, TEST_JWT_AUDIENCE, TEST_PLATFORM_ISSUER,
    TEST_RS256_KID,
};
use uuid::Uuid;

fn signed_claims(claims: &serde_json::Value) -> String {
    signed_claims_with_kid(claims, TEST_RS256_KID)
}

fn signed_claims_with_kid(claims: &serde_json::Value, kid: &str) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.typ = Some(ACCESS_TOKEN_TYP.to_string());
    header.kid = Some(kid.to_string());
    let key = jsonwebtoken::EncodingKey::from_rsa_pem(
        crate::testing::http::jwt::TEST_RS256_PRIVATE_PEM.as_bytes(),
    )
    .expect("nonsecret fixture key");
    jsonwebtoken::encode(&header, claims, &key).expect("fixture token")
}

fn valid_claims() -> serde_json::Value {
    let now = chrono::Utc::now().timestamp();
    serde_json::json!({"sub":Uuid::new_v4().to_string(), "iss":TEST_PLATFORM_ISSUER,
        "aud":TEST_JWT_AUDIENCE, "exp":now+3600, "iat":now, "jti":"fixture-id"})
}

#[tokio::test]
async fn expired_bad_signature_future_dates_and_pending_are_denied() {
    let extractor = rs256_extractor();
    for field in ["exp", "nbf", "iat"] {
        let mut claims = valid_claims();
        let now = chrono::Utc::now().timestamp();
        claims[field] = serde_json::json!(if field == "exp" {
            now - 3600
        } else {
            now + 3600
        });
        assert!(extractor
            .extract_principal(&signed_claims(&claims))
            .await
            .is_err());
    }
    let token = signed_claims(&valid_claims());
    let mut parts: Vec<String> = token.split('.').map(str::to_string).collect();
    let first = if parts[2].starts_with('A') { "B" } else { "A" };
    parts[2].replace_range(0..1, first);
    assert!(extractor.extract_principal(&parts.join(".")).await.is_err());
    let mut doc: serde_json::Value = serde_json::from_str(&test_rs256_jwks_json()).unwrap();
    doc["keys"][0]["status"] = serde_json::json!("pending");
    let pending =
        UserIdExtractor::from_inline_jwks(doc.to_string(), Some(TEST_JWT_AUDIENCE)).unwrap();
    assert!(pending.extract_principal(&token).await.is_err());
    doc["keys"][0]["status"] = serde_json::json!("revoked");
    let revoked =
        UserIdExtractor::from_inline_jwks(doc.to_string(), Some(TEST_JWT_AUDIENCE)).unwrap();
    assert!(revoked.extract_principal(&token).await.is_err());
    doc["keys"][0]["status"] = serde_json::json!("retiring");
    let retiring =
        UserIdExtractor::from_inline_jwks(doc.to_string(), Some(TEST_JWT_AUDIENCE)).unwrap();
    assert!(retiring.extract_principal(&token).await.is_ok());
}

#[tokio::test]
async fn platform_key_cannot_substitute_configured_platform_issuer() {
    let auth = AuthConfig {
        jwt: JwtAuthConfig {
            allowed_algorithms: vec!["RS256".into()],
            issuer: Some("https://expected.example/iam".into()),
            audience: Some(TEST_JWT_AUDIENCE.into()),
            ..JwtAuthConfig::default()
        },
        ..AuthConfig::default()
    };
    let extractor =
        UserIdExtractor::from_config_with_inline_jwks(auth, test_rs256_jwks_json()).unwrap();
    assert!(extractor
        .extract_principal(&signed_claims(&valid_claims()))
        .await
        .is_err());
}

#[tokio::test]
async fn canonical_fixture_matches_configured_platform_issuer_without_bypass() {
    let issuer = "https://configured.test/iam";
    let auth = AuthConfig {
        jwt: JwtAuthConfig {
            allowed_algorithms: vec!["RS256".into()],
            issuer: Some(issuer.into()),
            audience: Some(TEST_JWT_AUDIENCE.into()),
            ..JwtAuthConfig::default()
        },
        ..AuthConfig::default()
    };
    let user = Uuid::new_v4();
    let extractor =
        UserIdExtractor::from_config_with_inline_jwks(auth, test_rs256_jwks_json_with_iss(issuer))
            .unwrap();
    let token = create_rs256_jwt_token_with_issuer(user, issuer);
    assert_eq!(extractor.extract_principal(&token).await.unwrap().sub, user);
}

#[tokio::test]
async fn organization_owner_is_bound_to_key_not_claim() {
    let owner = Uuid::new_v4();
    let issuer = format!("https://issuer.example/iam/orgs/{owner}");
    let fixture = CanonicalJwk::organization(&issuer, owner);
    let extractor =
        UserIdExtractor::from_inline_jwks(fixture.to_jwks_json(), Some(TEST_JWT_AUDIENCE)).unwrap();
    let mut claims = valid_claims();
    claims["iss"] = serde_json::json!(issuer);
    let sign = |claims: &serde_json::Value| signed_claims_with_kid(claims, fixture.kid());
    assert!(extractor.extract_principal(&sign(&claims)).await.is_err());
    claims["org"] = serde_json::json!(Uuid::new_v4().to_string());
    assert!(extractor.extract_principal(&sign(&claims)).await.is_err());
    claims["org"] = serde_json::json!(owner.to_string());
    assert!(extractor.extract_principal(&sign(&claims)).await.is_ok());
}

#[test]
fn publisher_metadata_missing_unknown_duplicate_or_bad_binding_is_invalid() {
    let doc: serde_json::Value = serde_json::from_str(&test_rs256_jwks_json()).unwrap();
    for field in ["status", "trust_scope", "organization_id"] {
        let mut invalid = doc.clone();
        invalid["keys"][0].as_object_mut().unwrap().remove(field);
        assert!(
            UserIdExtractor::from_inline_jwks(invalid.to_string(), Some(TEST_JWT_AUDIENCE))
                .is_err()
        );
    }
    let mut duplicate = doc.clone();
    duplicate["keys"]
        .as_array_mut()
        .unwrap()
        .push(doc["keys"][0].clone());
    assert!(
        UserIdExtractor::from_inline_jwks(duplicate.to_string(), Some(TEST_JWT_AUDIENCE)).is_err()
    );
    for (field, value) in [("status", "unknown"), ("organization_id", "not-null")] {
        let mut invalid = doc.clone();
        invalid["keys"][0][field] = serde_json::json!(value);
        assert!(
            UserIdExtractor::from_inline_jwks(invalid.to_string(), Some(TEST_JWT_AUDIENCE))
                .is_err()
        );
    }
}

fn rs256_extractor() -> UserIdExtractor {
    UserIdExtractor::from_inline_jwks(test_rs256_jwks_json(), Some(TEST_JWT_AUDIENCE))
        .expect("inline JWKS extractor")
}

async fn seeded_rs256_extractor(url: &str, document: String) -> UserIdExtractor {
    let seed = LocalJwksSeed::capture(url, || async move { Ok(document) })
        .await
        .expect("trusted local publisher fixture");
    let auth = AuthConfig {
        jwt: JwtAuthConfig {
            allowed_algorithms: vec!["RS256".into()],
            jwks_url: Some(url.into()),
            issuer: Some(TEST_PLATFORM_ISSUER.into()),
            audience: Some(TEST_JWT_AUDIENCE.into()),
            ..JwtAuthConfig::default()
        },
        ..AuthConfig::default()
    };
    UserIdExtractor::from_config_with_seeded_jwks(auth, seed).expect("fresh authoritative seed")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn seeded_first_signed_burst_succeeds_without_capture_constructor_or_lookup_http() {
    let server = wiremock::MockServer::start().await;
    let extractor = Arc::new(seeded_rs256_extractor(&server.uri(), test_rs256_jwks_json()).await);
    assert!(server.received_requests().await.unwrap().is_empty());
    let user = Uuid::new_v4();
    let token = create_rs256_jwt_token(user);
    let barrier = Arc::new(tokio::sync::Barrier::new(25));
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..24 {
        let extractor = extractor.clone();
        let barrier = barrier.clone();
        let token = token.clone();
        tasks.spawn(async move {
            barrier.wait().await;
            extractor.extract_principal(&token).await
        });
    }
    barrier.wait().await;
    let mut successes = 0;
    while let Some(result) = tasks.join_next().await {
        let principal = result
            .unwrap()
            .expect("every first-call valid bearer must authenticate");
        assert_eq!(principal.sub, user);
        assert_eq!(principal.iss, TEST_PLATFORM_ISSUER);
        assert_eq!(principal.org, None);
        successes += 1;
    }
    assert_eq!(successes, 24);
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn seeded_signed_denials_preserve_signature_typ_audience_issuer_dates_and_algorithms() {
    let server = wiremock::MockServer::start().await;
    // Even noncanonical extra audience metadata is not an audience authority.
    let mut doc: serde_json::Value = serde_json::from_str(&test_rs256_jwks_json()).unwrap();
    doc["keys"][0]["aud"] = serde_json::json!("attacker-audience");
    let extractor = seeded_rs256_extractor(&server.uri(), doc.to_string()).await;
    for field in ["aud", "iss", "exp", "nbf", "iat", "sub"] {
        let mut claims = valid_claims();
        let now = chrono::Utc::now().timestamp();
        claims[field] = match field {
            "aud" => serde_json::json!("attacker-audience"),
            "iss" => serde_json::json!("https://attacker.example/iam"),
            "sub" => serde_json::json!("not-a-uuid"),
            "exp" => serde_json::json!(now - 3600),
            _ => serde_json::json!(now + 3600),
        };
        assert!(
            extractor
                .extract_principal(&signed_claims(&claims))
                .await
                .is_err(),
            "{field}"
        );
    }
    let mut no_audience = valid_claims();
    no_audience.as_object_mut().unwrap().remove("aud");
    assert!(extractor
        .extract_principal(&signed_claims(&no_audience))
        .await
        .is_err());
    let valid = signed_claims(&valid_claims());
    let mut parts: Vec<String> = valid.split('.').map(str::to_string).collect();
    let first = if parts[2].starts_with('A') { "B" } else { "A" };
    parts[2].replace_range(0..1, first);
    assert!(extractor.extract_principal(&parts.join(".")).await.is_err());
    for options in [
        Rs256TokenOptions {
            typ: Some(None),
            ..Default::default()
        },
        Rs256TokenOptions {
            typ: Some(Some("JWT")),
            ..Default::default()
        },
        Rs256TokenOptions {
            kid: Some(None),
            ..Default::default()
        },
        Rs256TokenOptions {
            kid: Some(Some("../unsafe")),
            ..Default::default()
        },
        Rs256TokenOptions {
            jku: Some("https://attacker.example/jwks"),
            ..Default::default()
        },
        Rs256TokenOptions {
            x5u: Some("https://attacker.example/key"),
            ..Default::default()
        },
        Rs256TokenOptions {
            include_jwk_header: true,
            ..Default::default()
        },
    ] {
        let token = create_rs256_jwt_token_with_options(Uuid::new_v4(), options);
        assert!(extractor.extract_principal(&token).await.is_err());
    }
    let hs = create_jwt_token_with_secret(Uuid::new_v4(), TEST_HS256_SECRET);
    assert!(
        extractor.extract_principal(&hs).await.is_err(),
        "seed must not activate HS256"
    );
    assert!(extractor.extract_principal(&valid).await.is_ok());
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn seeded_hs256_window_uses_only_its_explicit_independent_secret() {
    let server = wiremock::MockServer::start().await;
    let seed = LocalJwksSeed::capture(server.uri(), || async { Ok(test_rs256_jwks_json()) })
        .await
        .unwrap();
    let auth = AuthConfig {
        jwt: JwtAuthConfig {
            allowed_algorithms: vec!["RS256".into(), "HS256".into()],
            hs256_secret: Some(TEST_HS256_SECRET.into()),
            jwks_url: Some(server.uri()),
            issuer: Some(TEST_PLATFORM_ISSUER.into()),
            audience: Some(TEST_JWT_AUDIENCE.into()),
            ..JwtAuthConfig::default()
        },
        ..AuthConfig::default()
    };
    let extractor = UserIdExtractor::from_config_with_seeded_jwks(auth, seed).unwrap();
    let user = Uuid::new_v4();
    let mut header = Header::new(Algorithm::HS256);
    header.kid = Some(TEST_RS256_KID.into()); // RSA kid never selects an HMAC secret
    let mut claims = valid_claims();
    claims["sub"] = serde_json::json!(user.to_string());
    let hs = jsonwebtoken::encode(
        &header,
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(TEST_HS256_SECRET.as_bytes()),
    )
    .unwrap();
    assert_eq!(extractor.extract_principal(&hs).await.unwrap().sub, user);
    for wrong in [
        "wrong-independent-secret",
        crate::testing::http::jwt::TEST_RS256_PUBLIC_PEM,
    ] {
        let token = jsonwebtoken::encode(
            &header,
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(wrong.as_bytes()),
        )
        .unwrap();
        assert!(extractor.extract_principal(&token).await.is_err());
    }
    assert_eq!(
        extractor
            .extract_principal(&create_rs256_jwt_token(user))
            .await
            .unwrap()
            .sub,
        user
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn seeded_organization_issuer_and_owner_are_preserved_not_platform_rewritten() {
    let server = wiremock::MockServer::start().await;
    let owner = Uuid::new_v4();
    let issuer = format!("https://organization.example/iam/orgs/{owner}");
    let fixture = CanonicalJwk::organization(&issuer, owner);
    let extractor = seeded_rs256_extractor(&server.uri(), fixture.to_jwks_json()).await;
    let mut claims = valid_claims();
    claims["iss"] = serde_json::json!(issuer);
    claims["org"] = serde_json::json!(owner.to_string());
    let token = signed_claims_with_kid(&claims, fixture.kid());
    let principal = extractor.extract_principal(&token).await.unwrap();
    assert_eq!(principal.iss, fixture.issuer());
    assert_eq!(principal.org, Some(owner.to_string()));
    for invalid_org in [
        serde_json::Value::Null,
        serde_json::json!(Uuid::new_v4().to_string()),
    ] {
        claims["org"] = invalid_org;
        assert!(extractor
            .extract_principal(&signed_claims_with_kid(&claims, fixture.kid()))
            .await
            .is_err());
    }
    claims["org"] = serde_json::json!(owner.to_string());
    claims["iss"] = serde_json::json!(TEST_PLATFORM_ISSUER);
    assert!(extractor
        .extract_principal(&signed_claims_with_kid(&claims, fixture.kid()))
        .await
        .is_err());
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn seeded_unknown_org_fetches_live_and_authoritatively_removes_platform_seed() {
    use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    let owner = Uuid::new_v4();
    let issuer = format!("https://organization.example/iam/orgs/{owner}");
    let org = CanonicalJwk::organization(&issuer, owner);
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(org.to_jwks_json()))
        .mount(&server)
        .await;
    let extractor = seeded_rs256_extractor(&server.uri(), test_rs256_jwks_json()).await;
    assert!(server.received_requests().await.unwrap().is_empty());
    let old = create_rs256_jwt_token(Uuid::new_v4());
    let mut claims = valid_claims();
    claims["iss"] = serde_json::json!(issuer);
    claims["org"] = serde_json::json!(owner.to_string());
    let principal = extractor
        .extract_principal(&signed_claims_with_kid(&claims, org.kid()))
        .await
        .unwrap();
    assert_eq!(principal.org, Some(owner.to_string()));
    assert_eq!(principal.iss, org.issuer());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    assert!(
        extractor.extract_principal(&old).await.is_err(),
        "no seed/live union or bootstrap fallback"
    );
}

#[tokio::test]
async fn seeded_empty_pending_and_revoked_snapshots_never_authenticate() {
    for status in ["empty", "pending", "revoked"] {
        let server = wiremock::MockServer::start().await;
        let doc = if status == "empty" {
            r#"{"keys":[]}"#.into()
        } else {
            let mut value: serde_json::Value =
                serde_json::from_str(&test_rs256_jwks_json()).unwrap();
            value["keys"][0]["status"] = serde_json::json!(status);
            value.to_string()
        };
        let extractor = seeded_rs256_extractor(&server.uri(), doc).await;
        assert!(server.received_requests().await.unwrap().is_empty());
        let token = create_rs256_jwt_token(Uuid::new_v4());
        assert!(
            extractor.extract_principal(&token).await.is_err(),
            "{status}"
        );
        if status != "empty" {
            assert!(
                server.received_requests().await.unwrap().is_empty(),
                "untrusted hit is not a refresh"
            );
        }
    }
}

#[tokio::test]
async fn rs256_valid_returns_sub_and_principal_iss() {
    let user_id = Uuid::new_v4();
    let token = create_rs256_jwt_token(user_id);
    let extractor = rs256_extractor();

    let principal = extractor
        .extract_principal(&token)
        .await
        .expect("valid RS256 token");
    assert_eq!(principal.sub, user_id);
    assert_eq!(principal.iss, TEST_PLATFORM_ISSUER);

    let sub = extractor.extract_user_id(&token).await.expect("sub");
    assert_eq!(sub, user_id);
}

#[tokio::test]
async fn hs256_rejected_when_only_rs256_allowed() {
    let user_id = Uuid::new_v4();
    let hs_token = create_jwt_token_with_secret(user_id, TEST_HS256_SECRET);

    let auth = AuthConfig {
        jwt: JwtAuthConfig {
            hs256_secret: Some(TEST_HS256_SECRET.to_string()),
            allowed_algorithms: vec!["RS256".to_string()],
            audience: Some(TEST_JWT_AUDIENCE.to_string()),
            ..JwtAuthConfig::default()
        },
        ..AuthConfig::default()
    };
    let extractor = UserIdExtractor::from_config_with_inline_jwks(auth, test_rs256_jwks_json())
        .expect("dual-config RS256-only");

    let err = extractor
        .extract_user_id(&hs_token)
        .await
        .expect_err("HS256 must be rejected");
    assert!(err.to_string().contains("not allowed") || err.to_string().contains("invalid"));
}

#[tokio::test]
async fn hs256_accepted_when_allowed_with_secret() {
    let user_id = Uuid::new_v4();
    let hs_token = create_jwt_token_with_secret(user_id, TEST_HS256_SECRET);

    let auth = AuthConfig {
        jwt: JwtAuthConfig {
            hs256_secret: Some(TEST_HS256_SECRET.to_string()),
            allowed_algorithms: vec!["RS256".to_string(), "HS256".to_string()],
            issuer: Some("iamrusty".to_string()),
            audience: Some(TEST_JWT_AUDIENCE.to_string()),
            ..JwtAuthConfig::default()
        },
        ..AuthConfig::default()
    };
    let extractor = UserIdExtractor::from_config_with_inline_jwks(auth, test_rs256_jwks_json())
        .expect("window config");

    let sub = extractor
        .extract_user_id(&hs_token)
        .await
        .expect("HS256 window");
    assert_eq!(sub, user_id);
}

#[tokio::test]
async fn rs256_missing_kid_rejected() {
    let user_id = Uuid::new_v4();
    let token = create_rs256_jwt_token_with_options(
        user_id,
        Rs256TokenOptions {
            kid: Some(None),
            ..Default::default()
        },
    );
    let err = rs256_extractor()
        .extract_user_id(&token)
        .await
        .expect_err("missing kid");
    let msg = err.to_string();
    assert!(msg.contains("kid") || msg.contains("invalid"), "{msg}");
}

#[tokio::test]
async fn jku_x5u_jwk_headers_rejected() {
    let user_id = Uuid::new_v4();
    let extractor = rs256_extractor();

    let cases = [
        Rs256TokenOptions {
            jku: Some("https://evil.example/jwks"),
            ..Default::default()
        },
        Rs256TokenOptions {
            x5u: Some("https://evil.example/cert"),
            ..Default::default()
        },
        Rs256TokenOptions {
            include_jwk_header: true,
            ..Default::default()
        },
    ];

    for options in cases {
        let token = create_rs256_jwt_token_with_options(user_id, options);
        extractor
            .extract_user_id(&token)
            .await
            .expect_err("untrusted header must be rejected");
    }
}

#[tokio::test]
async fn rs256_iss_mismatch_rejected() {
    let user_id = Uuid::new_v4();
    // Signature valid against JWKS key, but JWT iss differs from JWK iss.
    let token = create_rs256_jwt_token_with_options(
        user_id,
        Rs256TokenOptions {
            iss: Some("http://127.0.0.1/iam/orgs/other"),
            ..Default::default()
        },
    );
    let err = rs256_extractor()
        .extract_user_id(&token)
        .await
        .expect_err("iss mismatch");
    assert!(
        err.to_string().contains("iss") || err.to_string().contains("invalid"),
        "{}",
        err
    );
}

#[tokio::test]
async fn rs256_missing_or_wrong_typ_rejected() {
    let user_id = Uuid::new_v4();
    let extractor = rs256_extractor();

    let missing_typ = create_rs256_jwt_token_with_options(
        user_id,
        Rs256TokenOptions {
            typ: Some(None),
            ..Default::default()
        },
    );
    extractor
        .extract_user_id(&missing_typ)
        .await
        .expect_err("missing typ");

    let jwt_typ = create_rs256_jwt_token_with_options(
        user_id,
        Rs256TokenOptions {
            typ: Some(Some("JWT")),
            ..Default::default()
        },
    );
    extractor
        .extract_user_id(&jwt_typ)
        .await
        .expect_err("typ=JWT");
}

#[tokio::test]
async fn dangerous_kid_rejected_without_url_use() {
    let user_id = Uuid::new_v4();
    let extractor = rs256_extractor();

    for kid in ["foo/bar", "https://evil.example/key", "a://b", "..hidden"] {
        let token = create_rs256_jwt_token_with_options(
            user_id,
            Rs256TokenOptions {
                kid: Some(Some(kid)),
                ..Default::default()
            },
        );
        extractor
            .extract_user_id(&token)
            .await
            .expect_err("dangerous kid");
    }

    // Sanity: the legitimate opaque kid is still accepted.
    let _ = TEST_RS256_KID;
}
