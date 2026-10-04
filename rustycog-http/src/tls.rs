//! Optional TLS client authentication for the HTTP listener.

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use anyhow::Context as _;
use axum::http::Request;
use axum_server::accept::Accept;
use axum_server::tls_rustls::{RustlsAcceptor, RustlsConfig};
use rustls::pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::RootCertStore;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::server::TlsStream;
use tower::Service;

use crate::rustycog_config::ServerConfig;

/// Leaf TLS client certificate presented during the handshake, as raw DER.
///
/// Inserted into [`axum::http::Request`] extensions only when a client
/// certificate was accepted. Absent when the client did not present a
/// certificate (optional client auth).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerClientCertificate {
    /// DER-encoded end-entity (leaf) certificate bytes.
    pub der: Vec<u8>,
}

pub(crate) fn install_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

/// Build a rustls server config that verifies client certs against
/// `tls_client_ca_path`.
///
/// `tls_require_client_cert` false still accepts connections without a
/// certificate. True requires one. Callers must refuse to start when the
/// flag is true and the CA path is empty.
///
/// # Errors
///
/// Returns an error if the crypto provider cannot be used, the server
/// certificate/key or client CA PEM cannot be loaded, or rustls rejects the
/// resulting configuration.
pub(crate) fn rustls_config_with_client_auth(
    config: &ServerConfig,
) -> anyhow::Result<RustlsConfig> {
    install_crypto_provider();

    let certs = load_certs(&config.tls_cert_path)?;
    let key = load_private_key(&config.tls_key_path)?;
    let roots = load_client_ca_roots(&config.tls_client_ca_path)?;

    let mut verifier_builder = WebPkiClientVerifier::builder(Arc::new(roots));
    if !config.tls_require_client_cert {
        verifier_builder = verifier_builder.allow_unauthenticated();
    }
    let verifier = verifier_builder
        .build()
        .context("failed to build TLS client certificate verifier")?;

    let mut server_config = rustls::ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(certs, key)
        .context("invalid TLS server certificate or private key")?;
    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(RustlsConfig::from_config(Arc::new(server_config)))
}

fn load_certs(path: &str) -> anyhow::Result<Vec<CertificateDer<'static>>> {
    let certs = CertificateDer::pem_file_iter(path)
        .with_context(|| format!("failed to read TLS certificate {path}"))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("invalid TLS certificate PEM in {path}"))?;
    if certs.is_empty() {
        anyhow::bail!("no certificates in {path}");
    }
    Ok(certs)
}

fn load_private_key(path: &str) -> anyhow::Result<PrivateKeyDer<'static>> {
    PrivateKeyDer::from_pem_file(path)
        .with_context(|| format!("failed to read TLS private key {path}"))
}

fn load_client_ca_roots(path: &str) -> anyhow::Result<RootCertStore> {
    let mut roots = RootCertStore::empty();
    let certs = CertificateDer::pem_file_iter(path)
        .with_context(|| format!("failed to read TLS client CA {path}"))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("invalid TLS client CA PEM in {path}"))?;
    if certs.is_empty() {
        anyhow::bail!("no certificates in TLS client CA {path}");
    }
    for cert in certs {
        roots
            .add(cert)
            .with_context(|| format!("invalid TLS client CA certificate in {path}"))?;
    }
    Ok(roots)
}

/// Wraps [`RustlsAcceptor`] so accepted connections inject the peer leaf cert
/// into request extensions.
#[derive(Clone)]
pub(crate) struct PeerClientCertAcceptor {
    inner: RustlsAcceptor,
}

impl PeerClientCertAcceptor {
    pub(crate) fn new(config: RustlsConfig) -> Self {
        Self {
            inner: RustlsAcceptor::new(config),
        }
    }
}

impl<I, S> Accept<I, S> for PeerClientCertAcceptor
where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    S: Send + 'static,
{
    type Stream = TlsStream<I>;
    type Service = InjectPeerClientCertService<S>;
    type Future = Pin<Box<dyn Future<Output = io::Result<(Self::Stream, Self::Service)>> + Send>>;

    fn accept(&self, stream: I, service: S) -> Self::Future {
        let inner = self.inner.clone();
        Box::pin(async move {
            let (tls_stream, service) = inner.accept(stream, service).await?;
            let peer = peer_from_tls_stream(&tls_stream);
            Ok((
                tls_stream,
                InjectPeerClientCertService {
                    inner: service,
                    peer,
                },
            ))
        })
    }
}

fn peer_from_tls_stream<I>(tls_stream: &TlsStream<I>) -> Option<PeerClientCertificate> {
    tls_stream
        .get_ref()
        .1
        .peer_certificates()
        .and_then(|certs| certs.first())
        .map(|der| PeerClientCertificate {
            der: der.as_ref().to_vec(),
        })
}

/// Tower service that copies the connection's peer cert into each request.
#[derive(Clone)]
pub(crate) struct InjectPeerClientCertService<S> {
    inner: S,
    peer: Option<PeerClientCertificate>,
}

impl<S, B> Service<Request<B>> for InjectPeerClientCertService<S>
where
    S: Service<Request<B>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<B>) -> Self::Future {
        // Only the accepted handshake supplies this extension. Preserve the
        // SocketAddr make-service's ConnectInfo layer; never derive a peer here.
        req.extensions_mut().remove::<PeerClientCertificate>();
        if let Some(peer) = self.peer.clone() {
            req.extensions_mut().insert(peer);
        }
        self.inner.call(req)
    }
}

#[cfg(test)]
mod metadata_tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::extract::ConnectInfo;
    use axum::routing::get;
    use axum::{Extension, Router};
    use std::net::SocketAddr;

    async fn observe(
        ConnectInfo(addr): ConnectInfo<SocketAddr>,
        peer: Option<Extension<PeerClientCertificate>>,
    ) -> String {
        let cert = peer.map(|Extension(peer)| peer.der);
        format!("{addr}:{}", cert == Some(vec![1, 2, 3]))
    }

    async fn metadata_response(peer: Option<PeerClientCertificate>) -> String {
        // In-memory make-service target, not a listener or TLS handshake.
        let addr: SocketAddr = "192.0.2.10:4321".parse().unwrap();
        let app = Router::new().route("/", get(observe));
        let mut make = app.into_make_service_with_connect_info::<SocketAddr>();
        let service = Service::<SocketAddr>::call(&mut make, addr).await.unwrap();
        let mut service = InjectPeerClientCertService {
            inner: service,
            peer,
        };
        let mut req = Request::builder()
            .uri("/")
            .header("x-forwarded-for", "203.0.113.99")
            .header("x-forwarded-client-cert", "unverified-client-certificate")
            .header("x-principal-iss", "https://unverified.example")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(PeerClientCertificate { der: vec![9, 9, 9] });
        req.extensions_mut().insert(ConnectInfo(
            "203.0.113.99:9999".parse::<SocketAddr>().unwrap(),
        ));
        let response = service.call(req).await.unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        String::from_utf8(body.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn socket_connect_info_survives_certificate_injection() {
        assert_eq!(
            metadata_response(Some(PeerClientCertificate { der: vec![1, 2, 3] })).await,
            "192.0.2.10:4321:true"
        );
    }

    #[tokio::test]
    async fn absent_handshake_certificate_never_trusts_request_headers_or_extension() {
        assert_eq!(metadata_response(None).await, "192.0.2.10:4321:false");
    }
}
