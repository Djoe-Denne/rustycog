use super::*;
use wiremock::{
    matchers::{header, method, path},
    Mock, MockServer, ResponseTemplate,
};

const TOKEN: &str = "synthetic-bootstrap-token";

async fn fixture() -> (MockServer, tempfile::NamedTempFile, OpenBaoConfig) {
    let server = MockServer::start().await;
    let token = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(token.path(), format!("{TOKEN}\r\n")).unwrap();
    let config = OpenBaoConfig {
        endpoint: server.uri(),
        auth: OpenBaoAuthConfig::TokenFile {
            path: token.path().to_owned(),
        },
        allow_loopback_http: true,
        ca_cert_file: None,
    };
    (server, token, config)
}

async fn lookup(server: &MockServer, policies: serde_json::Value, ttl: u64) {
    Mock::given(method("GET"))
        .and(path("/v1/auth/token/lookup-self"))
        .and(header("X-Vault-Token", TOKEN))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"data":{"policies":policies,"ttl":ttl}})),
        )
        .mount(server)
        .await;
}

fn reference() -> SecretReference {
    SecretReference::new(
        "secret".into(),
        "connector/oauth".into(),
        "client_secret".into(),
    )
    .unwrap()
}

#[tokio::test]
async fn kv_v2_success_exact_bytes_and_redacted_debug() {
    let (server, _token, config) = fixture().await;
    lookup(&server, serde_json::json!(["connector-read"]), 600).await;
    Mock::given(method("GET"))
        .and(path("/v1/secret/data/connector/oauth"))
        .and(header("X-Vault-Token", TOKEN))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"data":{"data":{"client_secret":" synthetic-sensitive-value "}}}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let resolver = OpenBaoKv2Resolver::connect(&config).await.unwrap();
    let value = resolver.resolve(&reference()).await.unwrap();
    assert!(value.expose_secret() == " synthetic-sensitive-value ");
    let debug = format!("{config:?} {resolver:?} {value:?} {:?}", reference());
    assert!(!debug.contains(TOKEN));
    assert!(!debug.contains("synthetic-sensitive-value"));
    assert!(!debug.contains(&server.uri()));
    assert!(value.into_inner() == " synthetic-sensitive-value ");
}

#[tokio::test]
async fn kv_failures_are_closed_and_sanitized() {
    let (server, _token, config) = fixture().await;
    for (response, expected) in [
        (
            ResponseTemplate::new(403).set_body_string(TOKEN),
            SecretError::AuthenticationFailed,
        ),
        (ResponseTemplate::new(404), SecretError::NotFound),
        (ResponseTemplate::new(503), SecretError::TransportFailed),
        (
            ResponseTemplate::new(200).set_body_string(TOKEN),
            SecretError::InvalidResponse,
        ),
        (
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"data":{"data":{}}})),
            SecretError::NotFound,
        ),
        (
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"data":{"data":{"client_secret":null}}})),
            SecretError::InvalidResponse,
        ),
        (
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"data":{"data":{"client_secret":"  "}}})),
            SecretError::InvalidValue,
        ),
        (
            ResponseTemplate::new(200).set_body_string("x".repeat(RESPONSE_LIMIT + 1)),
            SecretError::LimitExceeded,
        ),
    ] {
        server.reset().await;
        lookup(&server, serde_json::json!(["connector-read"]), 600).await;
        Mock::given(method("GET"))
            .and(path("/v1/secret/data/connector/oauth"))
            .respond_with(response)
            .mount(&server)
            .await;
        let resolver = OpenBaoKv2Resolver::connect(&config).await.unwrap();
        let error = resolver.resolve(&reference()).await.unwrap_err();
        assert!(error == expected);
        assert!(!format!("{error:?} {error}").contains(TOKEN));
        assert!(std::error::Error::source(&error).is_none());
    }
}

#[tokio::test]
async fn bootstrap_requires_bounded_nonroot_identity() {
    let (server, token, config) = fixture().await;
    for (policies, ttl) in [
        (serde_json::json!(["root"]), 600),
        (serde_json::json!(["read"]), 0),
        (serde_json::json!(["read"]), 3601),
        (serde_json::json!([42]), 600),
    ] {
        server.reset().await;
        lookup(&server, policies, ttl).await;
        assert!(matches!(
            OpenBaoKv2Resolver::connect(&config).await,
            Err(SecretError::AuthenticationFailed)
        ));
    }
    for content in ["", "bad token", "a\nb", "a\n\n"] {
        std::fs::write(token.path(), content).unwrap();
        assert!(OpenBaoKv2Resolver::connect(&config).await.is_err());
    }
    std::fs::write(token.path(), "x".repeat(VALUE_LIMIT + 1)).unwrap();
    assert!(matches!(
        OpenBaoKv2Resolver::connect(&config).await,
        Err(SecretError::LimitExceeded)
    ));
    let mut missing = config;
    missing.auth = OpenBaoAuthConfig::TokenFile {
        path: token.path().join("missing"),
    };
    assert!(OpenBaoKv2Resolver::connect(&missing).await.is_err());
}

#[tokio::test]
async fn no_redirect_or_ambient_transport_fallback() {
    let (server, _token, mut config) = fixture().await;
    let destination = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&destination)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(307).insert_header("Location", destination.uri()))
        .mount(&server)
        .await;
    assert!(OpenBaoKv2Resolver::connect(&config).await.is_err());
    destination.verify().await;
    config.allow_loopback_http = false;
    assert!(matches!(
        OpenBaoKv2Resolver::connect(&config).await,
        Err(SecretError::InvalidConfig)
    ));
    config.allow_loopback_http = true;
    for url in [
        "http://localhost:8200",
        "http://10.0.0.1",
        "https://user:credential@example.invalid",
        "https://bao.invalid/path",
        "https://bao.invalid/?token=credential",
        "https://bao.invalid/#credential",
    ] {
        config.endpoint = url.into();
        assert!(matches!(
            OpenBaoKv2Resolver::connect(&config).await,
            Err(SecretError::InvalidConfig)
        ));
    }
}

#[test]
fn reference_and_value_validation() {
    for value in [
        "",
        "../other",
        "a//b",
        "/a",
        "a/",
        "a%2fb",
        "a?token=x",
        "a\\b",
    ] {
        assert!(SecretReference::new("secret".into(), value.into(), "value".into()).is_err());
    }
    assert!(serde_json::from_str::<SecretReference>(
        r#"{"mount":"secret","path":"valid","field":"value","unknown":true}"#
    )
    .is_err());
    assert!(SecretValue::new(" \n ".into()).is_err());
    assert!(SecretValue::new("x".repeat(VALUE_LIMIT + 1)).is_err());
}
