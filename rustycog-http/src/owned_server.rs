//! Caller-owned HTTP execution. No descendant uses Tokio's detached executor.

use std::future::{poll_fn, Future};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard};

use axum::{extract::ConnectInfo, Router};
use axum_server::accept::Accept;
use axum_server::tls_rustls::RustlsConfig;
use futures::{stream::FuturesUnordered, StreamExt};
use hyper::rt::Executor;
use hyper_util::{rt::TokioIo, server::conn::auto::Builder, service::TowerToHyperService};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::{timeout_at, Instant};

use crate::rustycog_config::ServerConfig;

/// Absolute deadlines shared with the caller's other owned groups.
#[derive(Clone, Copy, Debug)]
pub struct HttpShutdownBudget {
    grace_at: Instant,
    deadline: Instant,
}

impl HttpShutdownBudget {
    /// # Errors
    /// Rejects a grace deadline later than the final deadline.
    pub fn new(grace_at: Instant, deadline: Instant) -> anyhow::Result<Self> {
        anyhow::ensure!(grace_at <= deadline, "HTTP grace exceeds final deadline");
        Ok(Self { grace_at, deadline })
    }
}

/// A complete join, not merely a graceful-shutdown notification.
#[derive(Clone, Copy, Debug)]
pub struct HttpShutdownReport {
    pub forced: bool,
}

#[derive(Default)]
struct Tasks {
    closed: bool,
    forced: bool,
    tasks: JoinSet<()>,
}

struct Group {
    tasks: Mutex<Tasks>,
    stop: watch::Sender<bool>,
    errors: Mutex<Vec<String>>,
    changed: tokio::sync::Notify,
}

// Poison is not a reason to lose the cancellation/observation capability.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Group {
    fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        let mut tasks = lock(&self.tasks);
        // Fence and registration are serialized. Rejected futures are destroyed
        // without being polled; accepted futures are recorded before executing.
        if !tasks.closed {
            tasks.tasks.spawn(future);
            self.changed.notify_one();
        }
    }

    fn close(&self) {
        lock(&self.tasks).closed = true;
        self.stop.send_replace(true);
    }

    fn abort(&self) {
        let mut tasks = lock(&self.tasks);
        tasks.closed = true;
        tasks.forced = true;
        tasks.tasks.abort_all();
        self.stop.send_replace(true);
    }

    async fn reap(&self) -> Option<Result<(), tokio::task::JoinError>> {
        poll_fn(|cx| lock(&self.tasks).tasks.poll_join_next(cx)).await
    }

    fn record(&self, result: Result<(), tokio::task::JoinError>, forced: bool) {
        if let Err(error) = result {
            if !((forced || lock(&self.tasks).forced) && error.is_cancelled()) {
                lock(&self.errors).push(format!("HTTP descendant join failed: {error}"));
            }
        }
    }
}

#[derive(Clone)]
struct OwnedExecutor(Arc<Group>);

impl<F> Executor<F> for OwnedExecutor
where
    F: Future + Send + 'static,
    F::Output: Send,
{
    fn execute(&self, future: F) {
        self.0.spawn(async move { drop(future.await) });
    }
}

/// Non-cloneable owner. Drop requests cancellation but cannot attest an async join.
pub struct OwnedHttpServer {
    group: Arc<Group>,
    coordinator: Option<JoinHandle<anyhow::Result<()>>>,
    server_result: Option<Result<(), String>>,
    terminal: Option<Result<HttpShutdownReport, String>>,
    budget: Option<HttpShutdownBudget>,
    forced: bool,
    addrs: Vec<SocketAddr>,
}

impl OwnedHttpServer {
    #[must_use]
    pub fn local_addrs(&self) -> Vec<SocketAddr> {
        self.addrs.clone()
    }

    /// Observe the coordinator without consuming the caller's ownership.
    /// # Errors
    /// Reports coordinator failure/panic or an owned descendant panic.
    pub async fn wait_for_server_exit(&mut self) -> anyhow::Result<()> {
        if self.server_result.is_none() {
            let result = self
                .coordinator
                .as_mut()
                .expect("coordinator retained until observed")
                .await;
            self.server_result = Some(match result {
                Ok(Ok(())) => Ok(()),
                Ok(Err(error)) => Err(format!("HTTP coordinator failed: {error:#}")),
                Err(error) if self.forced && error.is_cancelled() => Ok(()),
                Err(error) => Err(format!("HTTP coordinator join failed: {error}")),
            });
            self.coordinator = None;
        }
        self.server_result
            .clone()
            .expect("result recorded")
            .map_err(anyhow::Error::msg)
    }

    pub fn request_shutdown(&mut self, budget: HttpShutdownBudget) {
        self.budget = Some(match self.budget {
            Some(old) => HttpShutdownBudget {
                grace_at: old.grace_at.min(budget.grace_at),
                deadline: old.deadline.min(budget.deadline),
            },
            None => budget,
        });
        self.group.close();
    }

    /// # Errors
    /// Requires a shutdown request; reports panics, failures and incomplete joins.
    /// Cancellation of this borrowed future leaves all handles in the owner.
    pub async fn join(&mut self) -> anyhow::Result<HttpShutdownReport> {
        if let Some(result) = &self.terminal {
            return result.clone().map_err(anyhow::Error::msg);
        }
        let budget = self
            .budget
            .ok_or_else(|| anyhow::anyhow!("HTTP join requires shutdown budget"))?;
        if self.server_result.is_none() {
            let until = if self.forced {
                budget.deadline
            } else {
                budget.grace_at
            };
            if timeout_at(until, self.wait_for_server_exit())
                .await
                .is_err()
            {
                self.force();
                if timeout_at(budget.deadline, self.wait_for_server_exit())
                    .await
                    .is_err()
                {
                    anyhow::bail!("HTTP coordinator join unobserved at final deadline");
                }
            }
        }
        loop {
            let until = if self.forced {
                budget.deadline
            } else {
                budget.grace_at
            };
            match timeout_at(until, self.group.reap()).await {
                Ok(Some(result)) => self.group.record(result, self.forced),
                Ok(None) => break,
                Err(_) if !self.forced => self.force(),
                Err(_) => anyhow::bail!("HTTP descendant joins unobserved at final deadline"),
            }
        }
        let mut errors = lock(&self.group.errors).clone();
        if let Some(Err(error)) = &self.server_result {
            errors.push(error.clone());
        }
        let result = if errors.is_empty() {
            Ok(HttpShutdownReport {
                forced: self.forced,
            })
        } else {
            Err(errors.join("; "))
        };
        self.terminal = Some(result.clone());
        result.map_err(anyhow::Error::msg)
    }

    fn force(&mut self) {
        self.forced = true;
        self.group.abort();
        if let Some(task) = &self.coordinator {
            task.abort();
        }
    }
}

impl Drop for OwnedHttpServer {
    fn drop(&mut self) {
        self.force();
    }
}

/// Prepare every bind and TLS configuration before admitting connections.
/// # Errors
/// Reports invalid TLS/mTLS configuration or a failed bind. Partial listeners
/// are local RAII values, including when this startup future is cancelled.
pub async fn start_router_owned(
    app: Router,
    config: ServerConfig,
) -> anyhow::Result<OwnedHttpServer> {
    let tls = if config.tls_enabled {
        super::tls::install_crypto_provider();
        anyhow::ensure!(
            !config.tls_require_client_cert || !config.tls_client_ca_path.is_empty(),
            "tls_require_client_cert requires tls_client_ca_path (fail-closed)"
        );
        Some(if config.tls_client_ca_path.is_empty() {
            RustlsConfig::from_pem_file(&config.tls_cert_path, &config.tls_key_path).await?
        } else {
            super::tls::rustls_config_with_client_auth(&config)?
        })
    } else {
        None
    };
    let mut listeners = Vec::new();
    if !config.tls_enabled || (config.port != 0 && config.port != config.tls_port) {
        let port = config.port;
        listeners.push((
            TcpListener::bind(format!("{}:{port}", config.host)).await?,
            None,
        ));
    }
    if let Some(tls) = tls {
        listeners.push((
            TcpListener::bind(format!("{}:{}", config.host, config.tls_port)).await?,
            Some(tls),
        ));
    }
    let addrs = listeners
        .iter()
        .map(|(listener, _)| listener.local_addr())
        .collect::<Result<_, _>>()?;
    let group = Arc::new(Group {
        tasks: Mutex::new(Tasks::default()),
        stop: watch::channel(false).0,
        errors: Mutex::new(Vec::new()),
        changed: tokio::sync::Notify::new(),
    });
    let admission = group.clone();
    let app = app.layer(axum::middleware::from_fn(
        move |request: axum::extract::Request, next: axum::middleware::Next| {
            let admission = admission.clone();
            async move {
                let closed = lock(&admission.tasks).closed;
                if closed {
                    use axum::response::IntoResponse;
                    axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response()
                } else {
                    next.run(request).await
                }
            }
        },
    ));
    let mut owner = OwnedHttpServer {
        group: group.clone(),
        coordinator: None,
        server_result: None,
        terminal: None,
        budget: None,
        forced: false,
        addrs,
    };
    owner.coordinator = Some(tokio::spawn(async move {
        // Listener futures remain inside the coordinator, not detached tasks.
        let mut loops = FuturesUnordered::new();
        // Accept loops are polled inline; their destruction is observed by the
        // coordinator join even on panic, without another detached task.
        for (listener, tls) in listeners {
            let group = group.clone();
            let app = app.clone();
            loops.push(accept_loop(listener, tls, app, group));
        }
        loop {
            tokio::select! {
                result = loops.next() => match result {
                    Some(Ok(())) if loops.is_empty() => return Ok(()),
                    Some(Ok(())) => {},
                    Some(Err(error)) => return Err(error),
                    None => return Ok(()),
                },
                () = group.changed.notified() => {},
                result = group.reap(), if !lock(&group.tasks).tasks.is_empty() => {
                    if let Some(result) = result { group.record(result, false); }
                    if !lock(&group.errors).is_empty() { anyhow::bail!("HTTP descendant failed"); }
                }
            }
        }
    }));
    Ok(owner)
}

async fn stopped(stop: &mut watch::Receiver<bool>) {
    if !*stop.borrow_and_update() {
        let _ = stop.wait_for(|stopped| *stopped).await;
    }
}

async fn accept_loop(
    listener: TcpListener,
    tls: Option<RustlsConfig>,
    app: Router,
    group: Arc<Group>,
) -> anyhow::Result<()> {
    let mut stop = group.stop.subscribe();
    loop {
        let (stream, addr) = tokio::select! {
            biased;
            () = stopped(&mut stop) => return Ok(()),
            result = listener.accept() => result?,
        };
        let app = app.clone().layer(axum::Extension(ConnectInfo(addr)));
        let tls = tls.clone();
        let child_group = group.clone();
        group.spawn(async move {
            if let Some(tls) = tls {
                let acceptor = super::tls::PeerClientCertAcceptor::new(tls);
                // The TLS handshake itself is part of this registered task.
                match acceptor.accept(stream, app).await {
                    Ok((stream, app)) => connection(stream, app, child_group).await,
                    Err(error) => tracing::debug!(%error, "HTTP TLS handshake failed"),
                }
            } else {
                connection(stream, app, child_group).await;
            }
        });
    }
}

async fn connection<I, S>(io: I, service: S, group: Arc<Group>)
where
    I: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    S: tower::Service<
            hyper::Request<hyper::body::Incoming>,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    S::Future: Send,
{
    let mut stop = group.stop.subscribe();
    let builder = Builder::new(OwnedExecutor(group));
    let connection =
        builder.serve_connection_with_upgrades(TokioIo::new(io), TowerToHyperService::new(service));
    tokio::pin!(connection);
    tokio::select! {
        result = &mut connection => { if let Err(error) = result { tracing::debug!(%error, "HTTP connection ended"); } },
        () = stopped(&mut stop) => {
            connection.as_mut().graceful_shutdown();
            if let Err(error) = connection.await { tracing::debug!(%error, "HTTP drain ended"); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{start_router_owned, HttpShutdownBudget, OwnedHttpServer};
    use axum::{
        body::Body,
        extract::Request,
        routing::{get, post},
        Router,
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::time::Duration;
    use tokio::{
        io::AsyncWriteExt,
        net::TcpStream,
        sync::Notify,
        time::{timeout, Instant},
    };

    struct Witness {
        destroyed: Arc<AtomicUsize>,
        dropped: Arc<Notify>,
    }

    impl Drop for Witness {
        fn drop(&mut self) {
            self.destroyed.fetch_add(1, Ordering::SeqCst);
            self.dropped.notify_one();
        }
    }

    async fn start(router: Router) -> OwnedHttpServer {
        start_router_owned(
            router,
            crate::rustycog_config::ServerConfig {
                host: "127.0.0.1".into(),
                port: 0,
                tls_enabled: false,
                ..Default::default()
            },
        )
        .await
        .expect("loopback startup")
    }

    fn budget() -> HttpShutdownBudget {
        let now = Instant::now();
        HttpShutdownBudget::new(
            now + Duration::from_millis(50),
            now + Duration::from_secs(1),
        )
        .unwrap()
    }

    fn blocked_route(
        entered: Arc<Notify>,
        release: Arc<Notify>,
        destroyed: Arc<AtomicUsize>,
        dropped: Arc<Notify>,
        mutations: Arc<AtomicUsize>,
    ) -> Router {
        Router::new().route(
            "/owned",
            get(move || {
                let (entered, release, destroyed, dropped, mutations) = (
                    entered.clone(),
                    release.clone(),
                    destroyed.clone(),
                    dropped.clone(),
                    mutations.clone(),
                );
                async move {
                    let _witness = Witness { destroyed, dropped };
                    entered.notify_one();
                    release.notified().await;
                    mutations.fetch_add(1, Ordering::SeqCst);
                    "finished"
                }
            }),
        )
    }

    async fn request(server: &OwnedHttpServer) -> TcpStream {
        let mut socket = TcpStream::connect(server.local_addrs()[0]).await.unwrap();
        socket
            .write_all(b"GET /owned HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        socket
    }

    // G-1/G-4: cancellation of a borrowed join must not lose the pending work.
    #[tokio::test]
    async fn owned_force_resumes_cancelled_join_and_preserves_terminal_report() {
        let (entered, release, dropped) = (
            Arc::new(Notify::new()),
            Arc::new(Notify::new()),
            Arc::new(Notify::new()),
        );
        let destroyed = Arc::new(AtomicUsize::new(0));
        let mutations = Arc::new(AtomicUsize::new(0));
        let mut server = start(blocked_route(
            entered.clone(),
            release.clone(),
            destroyed.clone(),
            dropped,
            mutations.clone(),
        ))
        .await;
        let socket = request(&server).await;
        timeout(Duration::from_secs(1), entered.notified())
            .await
            .unwrap();
        server.request_shutdown(budget());
        let cancelled = timeout(Duration::from_millis(5), server.join()).await;
        let joined = server.join().await;
        let at_return = destroyed.load(Ordering::SeqCst);
        release.notify_one();
        drop(socket);
        let again = server.join().await;
        assert!(
            cancelled.is_err(),
            "pending borrowed join was actually cancelled"
        );
        assert!(joined.unwrap().forced);
        assert!(
            again.unwrap().forced,
            "terminal result must survive repeated join"
        );
        assert_eq!(
            at_return, 1,
            "request destructor must run before complete join"
        );
        assert_eq!(
            mutations.load(Ordering::SeqCst),
            0,
            "released request cannot mutate after join"
        );
    }

    // G-1: work admitted before stop is allowed to finish during grace.
    #[tokio::test]
    async fn owned_graceful_request_finishes_without_force() {
        let (entered, release, dropped) = (
            Arc::new(Notify::new()),
            Arc::new(Notify::new()),
            Arc::new(Notify::new()),
        );
        let destroyed = Arc::new(AtomicUsize::new(0));
        let mutations = Arc::new(AtomicUsize::new(0));
        let mut server = start(blocked_route(
            entered.clone(),
            release.clone(),
            destroyed.clone(),
            dropped,
            mutations.clone(),
        ))
        .await;
        let socket = request(&server).await;
        timeout(Duration::from_secs(1), entered.notified())
            .await
            .unwrap();
        let now = Instant::now();
        server.request_shutdown(
            HttpShutdownBudget::new(now + Duration::from_secs(1), now + Duration::from_secs(2))
                .unwrap(),
        );
        release.notify_one();
        let report = server.join().await;
        drop(socket);
        assert!(!report.unwrap().forced);
        assert_eq!(destroyed.load(Ordering::SeqCst), 1);
        assert_eq!(mutations.load(Ordering::SeqCst), 1);
    }

    // G-2: an admitted incoming body must not survive the complete join.
    #[tokio::test]
    async fn owned_force_destroys_handler_waiting_for_incomplete_request_body() {
        let entered = Arc::new(Notify::new());
        let destroyed = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(Notify::new());
        let router = Router::new().route(
            "/body",
            post({
                let (entered, destroyed, dropped) =
                    (entered.clone(), destroyed.clone(), dropped.clone());
                move |request: Request| {
                    let (entered, destroyed, dropped) =
                        (entered.clone(), destroyed.clone(), dropped.clone());
                    async move {
                        let _witness = Witness { destroyed, dropped };
                        entered.notify_one();
                        axum::body::to_bytes(request.into_body(), 100)
                            .await
                            .unwrap();
                        "body completed"
                    }
                }
            }),
        );
        let mut server = start(router).await;
        let mut socket = TcpStream::connect(server.local_addrs()[0]).await.unwrap();
        socket
            .write_all(b"POST /body HTTP/1.1\r\nHost: localhost\r\nContent-Length: 10\r\n\r\nx")
            .await
            .unwrap();
        timeout(Duration::from_secs(1), entered.notified())
            .await
            .unwrap();
        server.request_shutdown(budget());
        let report = server.join().await;
        let at_return = destroyed.load(Ordering::SeqCst);
        drop(socket);
        assert!(report.unwrap().forced);
        assert_eq!(
            at_return, 1,
            "body future was destroyed before join returned"
        );
    }

    // G-2: headers alone do not prove ownership of the response body stream.
    #[tokio::test]
    async fn owned_force_destroys_response_stream_after_headers() {
        let polled = Arc::new(Notify::new());
        let destroyed = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(Notify::new());
        let router = Router::new().route(
            "/stream",
            get({
                let (polled, destroyed, dropped) =
                    (polled.clone(), destroyed.clone(), dropped.clone());
                move || {
                    let witness = Witness {
                        destroyed: destroyed.clone(),
                        dropped: dropped.clone(),
                    };
                    let polled = polled.clone();
                    async move {
                        Body::from_stream(futures::stream::unfold(
                            (false, witness),
                            move |(sent, witness)| {
                                let polled = polled.clone();
                                async move {
                                    if sent {
                                        polled.notify_one();
                                        std::future::pending::<()>().await;
                                    }
                                    Some((
                                        Ok::<_, std::io::Error>(axum::body::Bytes::from_static(
                                            b"first",
                                        )),
                                        (true, witness),
                                    ))
                                }
                            },
                        ))
                    }
                }
            }),
        );
        let mut server = start(router).await;
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let response = client
            .get(format!("http://{}/stream", server.local_addrs()[0]))
            .send()
            .await
            .unwrap();
        timeout(Duration::from_secs(1), polled.notified())
            .await
            .unwrap();
        server.request_shutdown(budget());
        let report = server.join().await;
        let at_return = destroyed.load(Ordering::SeqCst);
        drop(response);
        drop(client);
        assert!(report.unwrap().forced);
        assert_eq!(
            at_return, 1,
            "response body owner must not outlive complete join"
        );
    }

    // G-4: coordinator errors/panics cannot erase independent descendant ownership.
    #[tokio::test]
    async fn owned_failed_coordinator_retains_active_request_and_primary_diagnostic() {
        for panic in [false, true] {
            let (entered, release, dropped) = (
                Arc::new(Notify::new()),
                Arc::new(Notify::new()),
                Arc::new(Notify::new()),
            );
            let destroyed = Arc::new(AtomicUsize::new(0));
            let mutations = Arc::new(AtomicUsize::new(0));
            let mut server = start(blocked_route(
                entered.clone(),
                release.clone(),
                destroyed.clone(),
                dropped,
                mutations.clone(),
            ))
            .await;
            let socket = request(&server).await;
            timeout(Duration::from_secs(1), entered.notified())
                .await
                .unwrap();
            // Private existing owner fields allow a controlled coordinator fault,
            // while its real admitted HTTP descendant remains untouched.
            let original = server.coordinator.take().unwrap();
            original.abort();
            let _ = original.await;
            server.coordinator = Some(tokio::spawn(async move {
                assert!(!panic, "coordinator panic witness");
                anyhow::bail!("coordinator error witness")
            }));
            let observed = server.wait_for_server_exit().await;
            server.request_shutdown(budget());
            let joined = server.join().await;
            let at_return = destroyed.load(Ordering::SeqCst);
            release.notify_one();
            drop(socket);
            assert!(format!("{:#}", observed.unwrap_err()).contains(if panic {
                "panic witness"
            } else {
                "error witness"
            }));
            assert!(format!("{:#}", joined.unwrap_err()).contains(if panic {
                "panic witness"
            } else {
                "error witness"
            }));
            assert_eq!(at_return, 1);
            assert_eq!(mutations.load(Ordering::SeqCst), 0);
        }
    }

    // G-4: Drop requests cancellation; an external witness, not Drop's return,
    // establishes that the active runtime actually destroyed the request.
    #[tokio::test]
    async fn owned_parent_cancellation_destroys_request_under_active_runtime() {
        let (entered, release, dropped) = (
            Arc::new(Notify::new()),
            Arc::new(Notify::new()),
            Arc::new(Notify::new()),
        );
        let destroyed = Arc::new(AtomicUsize::new(0));
        let mutations = Arc::new(AtomicUsize::new(0));
        let server = start(blocked_route(
            entered.clone(),
            release.clone(),
            destroyed.clone(),
            dropped.clone(),
            mutations.clone(),
        ))
        .await;
        let socket = request(&server).await;
        timeout(Duration::from_secs(1), entered.notified())
            .await
            .unwrap();
        let parent = tokio::spawn(async move {
            let _owner = server;
            std::future::pending::<()>().await;
        });
        parent.abort();
        let parent_result = parent.await;
        let observed = timeout(Duration::from_secs(1), dropped.notified()).await;
        release.notify_one();
        drop(socket);
        assert!(parent_result.unwrap_err().is_cancelled());
        assert!(
            observed.is_ok(),
            "runtime must observe destruction after Drop cancellation"
        );
        assert_eq!(destroyed.load(Ordering::SeqCst), 1);
        assert_eq!(mutations.load(Ordering::SeqCst), 0);
    }

    // G-2/G-3: real peer metadata and keep-alive admission fence after stop.
    #[tokio::test]
    async fn owned_keepalive_stop_fences_new_requests_and_preserves_connect_info() {
        use tokio::io::AsyncReadExt;
        let calls = Arc::new(AtomicUsize::new(0));
        let router = Router::new().route(
            "/peer",
            get({
                let calls = calls.clone();
                move |axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<
                    std::net::SocketAddr,
                >| {
                    let calls = calls.clone();
                    async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        peer.to_string()
                    }
                }
            }),
        );
        let mut server = start(router).await;
        let mut socket = TcpStream::connect(server.local_addrs()[0]).await.unwrap();
        let peer = socket.local_addr().unwrap().to_string();
        socket
            .write_all(b"GET /peer HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        let first = timeout(Duration::from_secs(1), async {
            let mut result = Vec::new();
            while !String::from_utf8_lossy(&result).ends_with(&peer) {
                let mut buffer = [0; 1024];
                let count = socket.read(&mut buffer).await.unwrap();
                assert_ne!(count, 0, "first response must carry the peer address");
                result.extend_from_slice(&buffer[..count]);
            }
            result
        })
        .await
        .unwrap();
        server.request_shutdown(budget());
        let _ = socket
            .write_all(b"GET /peer HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await;
        let report = server.join().await;
        drop(socket);
        assert!(String::from_utf8_lossy(&first).starts_with("HTTP/1.1 200"));
        assert!(!report.unwrap().forced);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "keep-alive request after stop must not enter handler"
        );
    }

    // G-3: a real H2 stream must be owned through Hyper's indirect executor.
    #[tokio::test]
    async fn owned_http2_admitted_stream_is_destroyed_before_forced_join() {
        let (entered, release, dropped) = (
            Arc::new(Notify::new()),
            Arc::new(Notify::new()),
            Arc::new(Notify::new()),
        );
        let destroyed = Arc::new(AtomicUsize::new(0));
        let mutations = Arc::new(AtomicUsize::new(0));
        let router = blocked_route(
            entered.clone(),
            release.clone(),
            destroyed.clone(),
            dropped,
            mutations.clone(),
        )
        .route("/probe", get(|| async { "h2" }));
        let mut server = start(router).await;
        let socket = TcpStream::connect(server.local_addrs()[0]).await.unwrap();
        let (mut sender, connection) = hyper::client::conn::http2::handshake(
            hyper_util::rt::TokioExecutor::new(),
            hyper_util::rt::TokioIo::new(socket),
        )
        .await
        .unwrap();
        let client_driver = tokio::spawn(connection);
        let probe = sender
            .send_request(
                hyper::Request::builder()
                    .uri("http://localhost/probe")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let version = probe.version();
        drop(probe);
        let mut request = tokio::spawn(async move {
            sender
                .send_request(
                    hyper::Request::builder()
                        .uri("http://localhost/owned")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
        });
        timeout(Duration::from_secs(1), entered.notified())
            .await
            .unwrap();
        server.request_shutdown(budget());
        let report = server.join().await;
        let at_return = destroyed.load(Ordering::SeqCst);
        release.notify_one();
        let request_result = timeout(Duration::from_secs(1), &mut request).await;
        if request_result.is_err() {
            request.abort();
            let _ = request.await;
        }
        client_driver.abort();
        let _ = client_driver.await;
        assert_eq!(
            version,
            hyper::Version::HTTP_2,
            "probe must negotiate real H2"
        );
        assert!(report.unwrap().forced);
        assert_eq!(at_return, 1, "indirect H2 request task cannot outlive join");
        assert_eq!(mutations.load(Ordering::SeqCst), 0);
        assert!(
            matches!(request_result, Ok(Ok(Err(_)))),
            "cancelled stream must not become successful response"
        );
    }
}
