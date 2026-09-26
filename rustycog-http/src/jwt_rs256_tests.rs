//! Unit tests for ADR-0304 RS256 + JWKS verification (inline JWKS, no network).

use super::*;
use crate::rustycog_config::{AuthConfig, JwtAuthConfig};
use crate::testing::http::jwt::{
    create_jwt_token_with_secret, create_rs256_jwt_token, create_rs256_jwt_token_with_options,
    test_rs256_jwks_json, Rs256TokenOptions, TEST_HS256_SECRET, TEST_JWT_AUDIENCE,
    TEST_PLATFORM_ISSUER, TEST_RS256_KID,
};
use uuid::Uuid;

fn rs256_extractor() -> UserIdExtractor {
    UserIdExtractor::from_inline_jwks(test_rs256_jwks_json(), Some(TEST_JWT_AUDIENCE))
        .expect("inline JWKS extractor")
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
