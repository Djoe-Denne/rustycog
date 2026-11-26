use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{Extension, State};
use axum::routing::get;
use axum::{middleware, Router};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::StatusCode;
use rustycog_framework::config::ServerConfig;
use rustycog_framework::http::{
    auth_middleware, optional_auth_middleware, serve_router, JwtPrincipal, OptionalAuthUser,
    SessionLivenessChecker, SessionLivenessDecision, SessionLivenessError, SessionLivenessOutcome,
    SessionLivenessQuery, UserIdExtractor,
};
use rustycog_framework::permission::PrincipalPolicy;
use serde_json::{json, Value};
use uuid::Uuid;

const SECRET: &str = "rustycog-session-liveness-test-secret";
const PLATFORM_ISSUER: &str = "https://platform.example/iam";
const ORGANIZATION_ISSUER: &str = "https://organization.example/iam";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Behavior {
    Live,
    Revoked,
    Unavailable,
    NotApplicable,
}

#[derive(Debug, Clone)]
struct TestChecker {
    behavior: Behavior,
    queries: Arc<Mutex<Vec<SessionLivenessQuery>>>,
}

impl TestChecker {
    fn new(behavior: Behavior) -> Self {
        Self {
            behavior,
            queries: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait::async_trait]
impl SessionLivenessChecker for TestChecker {
    async fn check(
        &self,
        query: &SessionLivenessQuery,
    ) -> Result<SessionLivenessOutcome, SessionLivenessError> {
        self.queries.lock().unwrap().push(query.clone());
        match self.behavior {
            Behavior::Live => Ok(SessionLivenessOutcome::new(SessionLivenessDecision::Live)),
            Behavior::Revoked => Ok(SessionLivenessOutcome::new(
                SessionLivenessDecision::Revoked,
            )),
            Behavior::Unavailable => Err(SessionLivenessError::unavailable("fixture outage")),
            Behavior::NotApplicable => Ok(SessionLivenessOutcome::new(
                SessionLivenessDecision::NotApplicable,
            )),
        }
    }
}

struct Server {
    base: String,
    handle: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

async fn required(
    State(counter): State<Arc<AtomicUsize>>,
    Extension(principal): Extension<JwtPrincipal>,
) -> String {
    counter.fetch_add(1, Ordering::SeqCst);
    format!("{}|{}", principal.iss, principal.sub)
}

async fn optional(State(counter): State<Arc<AtomicUsize>>, user: OptionalAuthUser) -> String {
    counter.fetch_add(1, Ordering::SeqCst);
    if user.is_authenticated() {
        "authenticated".into()
    } else {
        "anonymous".into()
    }
}

fn extractor(checker: &TestChecker) -> Arc<UserIdExtractor> {
    Arc::new(
        UserIdExtractor::from_resolved_secret(SECRET)
            .unwrap()
            .with_principal_policy(PrincipalPolicy::new(PLATFORM_ISSUER, false).unwrap())
            .unwrap()
            .with_session_liveness_checker(Arc::new(checker.clone())),
    )
}

async fn spawn(checker: &TestChecker) -> (Server, Arc<AtomicUsize>) {
    let counter = Arc::new(AtomicUsize::new(0));
    let extractor = extractor(checker);
    let app = Router::new()
        .route(
            "/required",
            get(required).route_layer(middleware::from_fn_with_state(
                extractor.clone(),
                auth_middleware,
            )),
        )
        .route(
            "/optional",
            get(optional).route_layer(middleware::from_fn_with_state(
                extractor,
                optional_auth_middleware,
            )),
        )
        .with_state(counter.clone());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let config = ServerConfig {
        host: "127.0.0.1".into(),
        port,
        tls_enabled: false,
        tls_port: 0,
        tls_cert_path: String::new(),
        tls_key_path: String::new(),
        tls_client_ca_path: String::new(),
        tls_require_client_cert: false,
    };
    let handle = tokio::task::spawn(async move { serve_router(app, config).await });
    let server = Server {
        base: format!("http://127.0.0.1:{port}"),
        handle,
    };
    let client = reqwest::Client::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(!server.handle.is_finished(), "server exited before ready");
        if client
            .get(format!("{}/optional", server.base))
            .send()
            .await
            .is_ok()
        {
            break;
        }
        assert!(Instant::now() < deadline, "server did not become ready");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    counter.store(0, Ordering::SeqCst);
    (server, counter)
}

fn token(issuer: &str, auth_epoch: Option<Value>) -> String {
    let now = chrono::Utc::now().timestamp();
    let mut claims = json!({
        "iss": issuer,
        "sub": Uuid::new_v4().to_string(),
        "exp": now + 3600,
        "iat": now,
        "jti": Uuid::new_v4().to_string(),
    });
    if let Some(epoch) = auth_epoch {
        claims["auth_epoch"] = epoch;
    }
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap()
}

async fn request(server: &Server, path: &str, token: Option<&str>) -> reqwest::Response {
    let client = reqwest::Client::new();
    let mut request = client.get(format!("{}{path}", server.base));
    if let Some(token) = token {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    request.send().await.unwrap()
}

#[tokio::test]
async fn required_auth_maps_live_revoked_and_unavailable_without_handler_effects() {
    for (behavior, expected) in [
        (Behavior::Live, StatusCode::OK),
        (Behavior::Revoked, StatusCode::UNAUTHORIZED),
        (Behavior::Unavailable, StatusCode::SERVICE_UNAVAILABLE),
        (Behavior::NotApplicable, StatusCode::SERVICE_UNAVAILABLE),
    ] {
        let checker = TestChecker::new(behavior);
        let (server, counter) = spawn(&checker).await;
        let response = request(
            &server,
            "/required",
            Some(&token(PLATFORM_ISSUER, Some(json!("3")))),
        )
        .await;
        assert_eq!(response.status(), expected);
        assert_eq!(
            counter.load(Ordering::SeqCst),
            usize::from(behavior == Behavior::Live)
        );
        let queries = checker.queries.lock().unwrap();
        assert_eq!(queries.len(), 1);
        assert_eq!(queries[0].issuer, PLATFORM_ISSUER);
        assert_eq!(queries[0].auth_epoch, Some(3));
    }
}

#[tokio::test]
async fn optional_presented_token_is_never_downgraded_on_denial() {
    for (behavior, expected) in [
        (Behavior::Live, StatusCode::OK),
        (Behavior::Revoked, StatusCode::UNAUTHORIZED),
        (Behavior::Unavailable, StatusCode::SERVICE_UNAVAILABLE),
        (Behavior::NotApplicable, StatusCode::SERVICE_UNAVAILABLE),
    ] {
        let checker = TestChecker::new(behavior);
        let (server, counter) = spawn(&checker).await;
        let response = request(
            &server,
            "/optional",
            Some(&token(PLATFORM_ISSUER, Some(json!("1")))),
        )
        .await;
        assert_eq!(response.status(), expected);
        assert_eq!(
            counter.load(Ordering::SeqCst),
            usize::from(behavior == Behavior::Live)
        );
    }

    let checker = TestChecker::new(Behavior::Revoked);
    let (server, counter) = spawn(&checker).await;
    let response = request(&server, "/optional", None).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.text().await.unwrap(), "anonymous");
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    assert!(checker.queries.lock().unwrap().is_empty());
}

#[tokio::test]
async fn non_platform_issuer_and_invalid_token_do_not_call_liveness() {
    let checker = TestChecker::new(Behavior::Revoked);
    let (server, _) = spawn(&checker).await;
    let response = request(
        &server,
        "/required",
        Some(&token(ORGANIZATION_ISSUER, Some(json!("9")))),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(checker.queries.lock().unwrap().is_empty());

    let mut invalid = token(PLATFORM_ISSUER, Some(json!("1")));
    let replacement = if invalid.ends_with('a') { 'b' } else { 'a' };
    invalid.pop();
    invalid.push(replacement);
    let response = request(&server, "/required", Some(&invalid)).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(checker.queries.lock().unwrap().is_empty());
}

#[tokio::test]
async fn missing_epoch_is_forwarded_as_legacy_none() {
    let checker = TestChecker::new(Behavior::Live);
    let (server, _) = spawn(&checker).await;
    let response = request(&server, "/required", Some(&token(PLATFORM_ISSUER, None))).await;
    assert_eq!(response.status(), StatusCode::OK);
    let queries = checker.queries.lock().unwrap();
    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0].auth_epoch, None);
}

#[test]
fn enforcement_composition_requires_a_checker() {
    let extractor = UserIdExtractor::from_resolved_secret(SECRET).unwrap();
    let error = extractor
        .require_session_liveness_checker()
        .err()
        .expect("missing checker must fail");
    assert_eq!(error.code(), "SESSION_LIVENESS_UNAVAILABLE");
}
