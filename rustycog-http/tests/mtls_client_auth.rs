use std::net::TcpListener;
use std::path::Path;
use std::time::{Duration, Instant};

use axum::extract::Request;
use axum::routing::get;
use axum::Router;
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa,
    KeyPair, KeyUsagePurpose,
};
use rustycog_framework::config::ServerConfig;
use rustycog_framework::http::{serve_router, PeerClientCertificate};

struct TestPki {
    _dir: tempfile::TempDir,
    server_cert_path: String,
    server_key_path: String,
    client_ca_path: String,
    client_identity_pem: Vec<u8>,
    client_leaf_der: Vec<u8>,
    foreign_identity_pem: Vec<u8>,
}

fn install_crypto() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

async fn peer_handler(req: Request) -> String {
    match req.extensions().get::<PeerClientCertificate>() {
        None => "none".to_string(),
        Some(cert) => format!("der:{}", hex_encode(&cert.der)),
    }
}

fn new_ca(common_name: &str) -> (Certificate, KeyPair) {
    let mut params = CertificateParams::new(Vec::new()).unwrap();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params
        .distinguished_name
        .push(DnType::CommonName, common_name);
    params.key_usages.push(KeyUsagePurpose::DigitalSignature);
    params.key_usages.push(KeyUsagePurpose::KeyCertSign);
    params.key_usages.push(KeyUsagePurpose::CrlSign);
    let key_pair = KeyPair::generate().unwrap();
    (params.self_signed(&key_pair).unwrap(), key_pair)
}

fn new_end_entity(
    sans: Vec<String>,
    common_name: &str,
    eku: ExtendedKeyUsagePurpose,
    issuer: Option<(&Certificate, &KeyPair)>,
) -> (Certificate, KeyPair) {
    let mut params = CertificateParams::new(sans).unwrap();
    params
        .distinguished_name
        .push(DnType::CommonName, common_name);
    params.key_usages.push(KeyUsagePurpose::DigitalSignature);
    params.extended_key_usages.push(eku);
    let key_pair = KeyPair::generate().unwrap();
    let cert = match issuer {
        Some((ca, ca_key)) => params.signed_by(&key_pair, ca, ca_key).unwrap(),
        None => params.self_signed(&key_pair).unwrap(),
    };
    (cert, key_pair)
}

fn identity_pem(cert: &Certificate, key: &KeyPair) -> Vec<u8> {
    format!("{}{}", cert.pem(), key.serialize_pem()).into_bytes()
}

fn write_pem(path: &Path, contents: &str) -> String {
    std::fs::write(path, contents).unwrap();
    path.to_string_lossy().into_owned()
}

fn generate_pki() -> TestPki {
    let dir = tempfile::tempdir().unwrap();
    let (ca_cert, ca_key) = new_ca("rustycog-test-client-ca");
    let (foreign_ca, foreign_ca_key) = new_ca("rustycog-foreign-client-ca");
    let (server_cert, server_key) = new_end_entity(
        vec!["127.0.0.1".into(), "localhost".into()],
        "rustycog-test-server",
        ExtendedKeyUsagePurpose::ServerAuth,
        None,
    );
    let (client_cert, client_key) = new_end_entity(
        vec!["mtls-client.test".into()],
        "mtls-client",
        ExtendedKeyUsagePurpose::ClientAuth,
        Some((&ca_cert, &ca_key)),
    );
    let (foreign_cert, foreign_key) = new_end_entity(
        vec!["foreign-client.test".into()],
        "foreign-client",
        ExtendedKeyUsagePurpose::ClientAuth,
        Some((&foreign_ca, &foreign_ca_key)),
    );

    let server_cert_path = write_pem(&dir.path().join("server.crt"), &server_cert.pem());
    let server_key_path = write_pem(&dir.path().join("server.key"), &server_key.serialize_pem());
    let client_ca_path = write_pem(&dir.path().join("client-ca.crt"), &ca_cert.pem());

    TestPki {
        server_cert_path,
        server_key_path,
        client_ca_path,
        client_identity_pem: identity_pem(&client_cert, &client_key),
        client_leaf_der: client_cert.der().as_ref().to_vec(),
        foreign_identity_pem: identity_pem(&foreign_cert, &foreign_key),
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

fn server_config(pki: &TestPki, tls_client_ca_path: String, tls_port: u16) -> ServerConfig {
    dual_bind_config(pki, 0, tls_client_ca_path, tls_port)
}

fn dual_bind_config(
    pki: &TestPki,
    http_port: u16,
    tls_client_ca_path: String,
    tls_port: u16,
) -> ServerConfig {
    ServerConfig {
        host: "127.0.0.1".into(),
        port: http_port,
        tls_enabled: true,
        tls_cert_path: pki.server_cert_path.clone(),
        tls_key_path: pki.server_key_path.clone(),
        tls_client_ca_path,
        tls_require_client_cert: false,
        tls_port,
    }
}

fn https_client(identity_pem: Option<&[u8]>) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .no_proxy()
        .timeout(Duration::from_secs(5));
    if let Some(pem) = identity_pem {
        builder = builder.identity(reqwest::Identity::from_pem(pem).unwrap());
    }
    builder.build().unwrap()
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

async fn spawn_server(config: ServerConfig) -> tokio::task::JoinHandle<anyhow::Result<()>> {
    let app = Router::new().route("/peer", get(peer_handler));
    tokio::spawn(async move { serve_router(app, config).await })
}

async fn wait_until_ready(
    handle: &tokio::task::JoinHandle<anyhow::Result<()>>,
    client: &reqwest::Client,
    url: &str,
) {
    let start = Instant::now();
    loop {
        if handle.is_finished() {
            panic!("TLS server exited before becoming ready");
        }
        match client.get(url).send().await {
            Ok(_) => return,
            Err(err) => {
                if start.elapsed() > Duration::from_secs(5) {
                    panic!("TLS server not ready after 5s: {err}");
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

async fn get_body(client: &reqwest::Client, url: &str) -> String {
    let response = client.get(url).send().await.unwrap();
    assert!(
        response.status().is_success(),
        "expected 2xx, got {}",
        response.status()
    );
    response.text().await.unwrap()
}

#[tokio::test]
async fn one_way_tls_without_client_ca_has_no_peer_extension() {
    install_crypto();
    let pki = generate_pki();
    let port = ephemeral_port();
    let url = format!("https://127.0.0.1:{port}/peer");
    let handle = spawn_server(server_config(&pki, String::new(), port)).await;
    let client = https_client(None);
    wait_until_ready(&handle, &client, &url).await;

    let body = get_body(&client, &url).await;
    assert_eq!(body, "none");

    handle.abort();
}

#[tokio::test]
async fn optional_client_auth_without_cert_has_no_peer_extension() {
    install_crypto();
    let pki = generate_pki();
    let port = ephemeral_port();
    let url = format!("https://127.0.0.1:{port}/peer");
    let handle = spawn_server(server_config(&pki, pki.client_ca_path.clone(), port)).await;
    let client = https_client(None);
    wait_until_ready(&handle, &client, &url).await;

    let body = get_body(&client, &url).await;
    assert_eq!(body, "none");

    handle.abort();
}

#[tokio::test]
async fn trusted_client_cert_inserts_leaf_der_extension() {
    install_crypto();
    let pki = generate_pki();
    let port = ephemeral_port();
    let url = format!("https://127.0.0.1:{port}/peer");
    let handle = spawn_server(server_config(&pki, pki.client_ca_path.clone(), port)).await;
    let probe = https_client(None);
    wait_until_ready(&handle, &probe, &url).await;

    let client = https_client(Some(&pki.client_identity_pem));
    let body = get_body(&client, &url).await;
    assert_eq!(body, format!("der:{}", hex_encode(&pki.client_leaf_der)));

    handle.abort();
}

#[tokio::test]
async fn foreign_client_cert_fails_handshake() {
    install_crypto();
    let pki = generate_pki();
    let port = ephemeral_port();
    let url = format!("https://127.0.0.1:{port}/peer");
    let handle = spawn_server(server_config(&pki, pki.client_ca_path.clone(), port)).await;
    let probe = https_client(None);
    wait_until_ready(&handle, &probe, &url).await;

    let client = https_client(Some(&pki.foreign_identity_pem));
    match client.get(&url).send().await {
        Err(_) => {}
        Ok(response) => assert!(
            !response.status().is_success(),
            "foreign client cert must not get 2xx, got {}",
            response.status()
        ),
    }

    handle.abort();
}

fn required_server_config(pki: &TestPki, tls_port: u16) -> ServerConfig {
    let mut config = server_config(pki, pki.client_ca_path.clone(), tls_port);
    config.tls_require_client_cert = true;
    config
}

#[tokio::test]
async fn required_client_cert_rejects_missing_and_foreign_ca() {
    install_crypto();
    let pki = generate_pki();
    let port = ephemeral_port();
    let url = format!("https://127.0.0.1:{port}/peer");
    let handle = spawn_server(required_server_config(&pki, port)).await;

    let mesh = https_client(Some(&pki.client_identity_pem));
    wait_until_ready(&handle, &mesh, &url).await;
    assert_eq!(
        get_body(&mesh, &url).await,
        format!("der:{}", hex_encode(&pki.client_leaf_der))
    );

    let bare = https_client(None);
    assert!(
        bare.get(&url).send().await.is_err(),
        "missing client certificate must fail the handshake"
    );

    let foreign = https_client(Some(&pki.foreign_identity_pem));
    match foreign.get(&url).send().await {
        Err(_) => {}
        Ok(response) => assert!(
            !response.status().is_success(),
            "foreign client cert must not get 2xx, got {}",
            response.status()
        ),
    }

    handle.abort();
}

#[tokio::test]
async fn required_client_cert_without_ca_does_not_start() {
    install_crypto();
    let pki = generate_pki();
    let port = ephemeral_port();
    let mut config = server_config(&pki, String::new(), port);
    config.tls_require_client_cert = true;
    let handle = spawn_server(config).await;
    let joined = tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .expect("server should exit when client CA is missing");
    let started = joined.expect("server task");
    assert!(
        started.is_err(),
        "tls_require_client_cert without a CA must fail closed"
    );
}

#[tokio::test]
async fn dual_bind_http_and_optional_mtls() {
    install_crypto();
    let pki = generate_pki();
    let cleartext_port = ephemeral_port();
    let tls_listen = ephemeral_port();
    let cleartext_url = format!("http://127.0.0.1:{cleartext_port}/peer");
    let tls_url = format!("https://127.0.0.1:{tls_listen}/peer");
    let handle = spawn_server(dual_bind_config(
        &pki,
        cleartext_port,
        pki.client_ca_path.clone(),
        tls_listen,
    ))
    .await;

    let plain = http_client();
    wait_until_ready(&handle, &plain, &cleartext_url).await;
    let tls_probe = https_client(None);
    wait_until_ready(&handle, &tls_probe, &tls_url).await;

    assert_eq!(get_body(&plain, &cleartext_url).await, "none");
    assert_eq!(get_body(&tls_probe, &tls_url).await, "none");

    let mesh = https_client(Some(&pki.client_identity_pem));
    assert_eq!(
        get_body(&mesh, &tls_url).await,
        format!("der:{}", hex_encode(&pki.client_leaf_der))
    );

    let foreign = https_client(Some(&pki.foreign_identity_pem));
    match foreign.get(&tls_url).send().await {
        Err(_) => {}
        Ok(response) => assert!(
            !response.status().is_success(),
            "foreign client cert must not get 2xx, got {}",
            response.status()
        ),
    }

    handle.abort();
}

#[tokio::test]
async fn matching_http_and_tls_port_binds_tls_only() {
    install_crypto();
    let pki = generate_pki();
    let shared_port = ephemeral_port();
    let tls_url = format!("https://127.0.0.1:{shared_port}/peer");
    let cleartext_url = format!("http://127.0.0.1:{shared_port}/peer");
    let handle = spawn_server(dual_bind_config(
        &pki,
        shared_port,
        pki.client_ca_path.clone(),
        shared_port,
    ))
    .await;

    let tls_probe = https_client(None);
    wait_until_ready(&handle, &tls_probe, &tls_url).await;
    assert_eq!(get_body(&tls_probe, &tls_url).await, "none");

    let plain = http_client();
    assert!(
        plain.get(&cleartext_url).send().await.is_err(),
        "HTTP client must not succeed on a TLS-only listener"
    );

    handle.abort();
}

/// Keep the failing address occupied until serve_router has returned. The other
/// address is chosen with an owned reservation, never a fixed/global test port.
/// A TCP probe gives runnable sibling tasks a transport polling opportunity;
/// successful exclusive rebind, not a TLS/HTTP request error, proves release.
async fn assert_dual_bind_conflict_releases_sibling(tls_fails: bool) {
    install_crypto();
    let pki = generate_pki();
    let occupied = TcpListener::bind("127.0.0.1:0").expect("owned conflict listener");
    let occupied_address = occupied.local_addr().expect("conflict address");
    let sibling_reservation = TcpListener::bind("127.0.0.1:0").expect("owned sibling reservation");
    let sibling_address = sibling_reservation.local_addr().expect("sibling address");
    assert_ne!(occupied_address, sibling_address);
    let (http_port, tls_port) = if tls_fails {
        (sibling_address.port(), occupied_address.port())
    } else {
        (occupied_address.port(), sibling_address.port())
    };
    // Nonempty, valid CA/cert/key exercises the real dual-bind branch without
    // async certificate-file loading delaying a detached sibling's bind.
    let config = dual_bind_config(&pki, http_port, pki.client_ca_path.clone(), tls_port);
    drop(sibling_reservation);
    let mut serving = spawn_server(config).await;
    let completed = tokio::time::timeout(Duration::from_secs(5), &mut serving).await;
    let (returned_error, deliberate_bind_error) = match completed {
        Ok(Ok(Err(error))) => {
            let bind_error = error.chain().any(|cause| {
                cause
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::AddrInUse)
            });
            (true, bind_error)
        }
        Ok(_) => (false, false),
        Err(_) => {
            // Always join our exact task before any assertion/PKI release.
            serving.abort();
            let _ = serving.await;
            (false, false)
        }
    };
    let sibling_connect = tokio::time::timeout(
        Duration::from_secs(1),
        tokio::net::TcpStream::connect(sibling_address),
    )
    .await;
    let sibling_accepted = matches!(&sibling_connect, Ok(Ok(_)));
    // Close any probe stream before taking the bind snapshot and asserting.
    drop(sibling_connect);
    let sibling_rebound = TcpListener::bind(sibling_address);
    let sibling_released = sibling_rebound.is_ok();
    // Drop all test-owned sockets before assertions, including mutant failures.
    drop(sibling_rebound);
    drop(occupied);
    assert!(
        returned_error,
        "dual-bind conflict must return an error within the bound, not hang or silently succeed"
    );
    assert!(deliberate_bind_error, "failure must be AddrInUse from the deliberate reservation, not certificate/configuration setup");
    assert!(
        !sibling_accepted,
        "a detached sibling must not keep accepting after serve_router failed"
    );
    assert!(sibling_released, "the sibling address must be exclusively bindable after the failure; a connection error alone is not release proof");
}

#[tokio::test]
async fn dual_bind_tls_bind_conflict_releases_http_listener() {
    assert_dual_bind_conflict_releases_sibling(true).await;
}

#[tokio::test]
async fn dual_bind_http_bind_conflict_does_not_leave_tls_listener() {
    assert_dual_bind_conflict_releases_sibling(false).await;
}
