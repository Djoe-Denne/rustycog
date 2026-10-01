use std::net::TcpListener;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::routing::get;
use axum::{middleware, Extension, Router};
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa,
    KeyPair, KeyUsagePurpose,
};
use reqwest::StatusCode;
use rustycog_framework::config::{AuthConfig, JwtAuthConfig, MeshAuthConfig, ServerConfig};
use rustycog_framework::http::{
    auth_middleware, optional_auth_middleware, serve_router, JwtPrincipal, OptionalAuthUser,
    UserIdExtractor,
};
use uuid::Uuid;

const GATEWAY_SAN: &str = "envoy-mesh";
const ISSUER: &str = "https://idp.example/iam";

struct Pki {
    _dir: tempfile::TempDir,
    server_cert_path: String,
    server_key_path: String,
    client_ca_path: String,
    gateway_identity: Vec<u8>,
    other_identity: Vec<u8>,
}

fn install_crypto() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

fn new_ca() -> (Certificate, KeyPair) {
    let mut params = CertificateParams::new(Vec::new()).unwrap();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params
        .distinguished_name
        .push(DnType::CommonName, "rustycog-mesh-test-ca");
    params.key_usages.push(KeyUsagePurpose::KeyCertSign);
    params.key_usages.push(KeyUsagePurpose::DigitalSignature);
    let key = KeyPair::generate().unwrap();
    (params.self_signed(&key).unwrap(), key)
}

fn new_leaf(
    san: &str,
    eku: ExtendedKeyUsagePurpose,
    ca: &Certificate,
    ca_key: &KeyPair,
) -> (Certificate, KeyPair) {
    let mut params = CertificateParams::new(vec![san.to_string()]).unwrap();
    params.distinguished_name.push(DnType::CommonName, san);
    params.key_usages.push(KeyUsagePurpose::DigitalSignature);
    params.extended_key_usages.push(eku);
    let key = KeyPair::generate().unwrap();
    (params.signed_by(&key, ca, ca_key).unwrap(), key)
}

fn write_pem(path: &Path, contents: &str) -> String {
    std::fs::write(path, contents).unwrap();
    path.to_string_lossy().into_owned()
}

fn generate_pki() -> Pki {
    let dir = tempfile::tempdir().unwrap();
    let (ca, ca_key) = new_ca();
    let (server, server_key) = new_leaf("localhost", ExtendedKeyUsagePurpose::ServerAuth, &ca, &ca_key);
    let (gateway, gateway_key) =
        new_leaf(GATEWAY_SAN, ExtendedKeyUsagePurpose::ClientAuth, &ca, &ca_key);
    let (other, other_key) =
        new_leaf("mesh-client", ExtendedKeyUsagePurpose::ClientAuth, &ca, &ca_key);
    Pki {
        server_cert_path: write_pem(&dir.path().join("server.crt"), &server.pem()),
        server_key_path: write_pem(&dir.path().join("server.key"), &server_key.serialize_pem()),
        client_ca_path: write_pem(&dir.path().join("ca.crt"), &ca.pem()),
        gateway_identity: format!("{}{}", gateway.pem(), gateway_key.serialize_pem()).into_bytes(),
        other_identity: format!("{}{}", other.pem(), other_key.serialize_pem()).into_bytes(),
        _dir: dir,
    }
}

fn ephemeral_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn extractor(gateway_san: &str) -> Arc<UserIdExtractor> {
    let auth = AuthConfig {
        jwt: JwtAuthConfig {
            audience: Some("aiforall".into()),
            jwks_url: Some("http://127.0.0.1:9/iam/.well-known/jwks.json".into()),
            ..JwtAuthConfig::default()
        },
        mesh: MeshAuthConfig {
            trusted_gateway_san: gateway_san.into(),
        },
    };
    Arc::new(UserIdExtractor::new(auth).unwrap())
}

async fn me(Extension(principal): Extension<JwtPrincipal>) -> String {
    format!("{}|{}", principal.iss, principal.sub)
}

async fn maybe(user: OptionalAuthUser) -> String {
    user.user_id()
        .map_or_else(|| "anonymous".to_string(), |id| id.to_string())
}

struct Server {
    handle: tokio::task::JoinHandle<anyhow::Result<()>>,
    http: String,
    https: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

async fn spawn(pki: &Pki, gateway_san: &str) -> Server {
    let extractor = extractor(gateway_san);
    let app = Router::new()
        .route(
            "/me",
            get(me).route_layer(middleware::from_fn_with_state(
                extractor.clone(),
                auth_middleware,
            )),
        )
        .route(
            "/maybe",
            get(maybe).route_layer(middleware::from_fn_with_state(
                extractor,
                optional_auth_middleware,
            )),
        );
    let http_port = ephemeral_port();
    let tls_port = ephemeral_port();
    let config = ServerConfig {
        host: "127.0.0.1".into(),
        port: http_port,
        tls_enabled: true,
        tls_cert_path: pki.server_cert_path.clone(),
        tls_key_path: pki.server_key_path.clone(),
        tls_client_ca_path: pki.client_ca_path.clone(),
        tls_require_client_cert: false,
        tls_port,
    };
    let handle = tokio::spawn(async move { serve_router(app, config).await });
    let server = Server {
        handle,
        http: format!("http://127.0.0.1:{http_port}"),
        https: format!("https://127.0.0.1:{tls_port}"),
    };
    wait_ready(&server, &client(None), &format!("{}/maybe", server.https)).await;
    wait_ready(&server, &client(None), &format!("{}/maybe", server.http)).await;
    server
}

async fn wait_ready(server: &Server, client: &reqwest::Client, url: &str) {
    let start = Instant::now();
    loop {
        assert!(!server.handle.is_finished(), "server exited before ready");
        if client.get(url).send().await.is_ok() {
            return;
        }
        assert!(start.elapsed() < Duration::from_secs(5), "server not ready: {url}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn client(identity: Option<&[u8]>) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .no_proxy()
        .timeout(Duration::from_secs(5));
    if let Some(pem) = identity {
        builder = builder.identity(reqwest::Identity::from_pem(pem).unwrap());
    }
    builder.build().unwrap()
}

async fn get_with_principal(client: &reqwest::Client, url: &str, sub: Uuid) -> reqwest::Response {
    client
        .get(url)
        .header("x-principal-iss", ISSUER)
        .header("x-principal-sub", sub.to_string())
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn gateway_peer_principal_is_trusted() {
    install_crypto();
    let pki = generate_pki();
    let server = spawn(&pki, GATEWAY_SAN).await;
    let gateway = client(Some(&pki.gateway_identity));
    let sub = Uuid::new_v4();

    let response = get_with_principal(&gateway, &format!("{}/me", server.https), sub).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.text().await.unwrap(), format!("{ISSUER}|{sub}"));

    let response = get_with_principal(&gateway, &format!("{}/maybe", server.https), sub).await;
    assert_eq!(response.text().await.unwrap(), sub.to_string());
}

#[tokio::test]
async fn bearer_jwt_is_not_verified_in_mesh_mode() {
    install_crypto();
    let pki = generate_pki();
    let server = spawn(&pki, GATEWAY_SAN).await;
    let gateway = client(Some(&pki.gateway_identity));

    let response = gateway
        .get(format!("{}/me", server.https))
        .bearer_auth("header.payload.signature")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn other_peers_cannot_inject_a_principal() {
    install_crypto();
    let pki = generate_pki();
    let server = spawn(&pki, GATEWAY_SAN).await;
    let sub = Uuid::new_v4();
    let me = format!("{}/me", server.https);

    let other = client(Some(&pki.other_identity));
    assert_eq!(
        get_with_principal(&other, &me, sub).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let anonymous = get_with_principal(&other, &format!("{}/maybe", server.https), sub).await;
    assert_eq!(anonymous.text().await.unwrap(), "anonymous");

    let no_cert = client(None);
    assert_eq!(
        get_with_principal(&no_cert, &me, sub).await.status(),
        StatusCode::UNAUTHORIZED
    );

    let plain = client(None);
    assert_eq!(
        get_with_principal(&plain, &format!("{}/me", server.http), sub)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn principal_headers_are_ignored_when_mesh_mode_is_off() {
    install_crypto();
    let pki = generate_pki();
    let server = spawn(&pki, "").await;
    let gateway = client(Some(&pki.gateway_identity));

    let response =
        get_with_principal(&gateway, &format!("{}/me", server.https), Uuid::new_v4()).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
