use std::sync::Arc;
use tokio::sync::OnceCell;
use tracing::debug;
use wiremock::MockServer;

/// Shared wiremock server instance for all fixtures
static MOCK_SERVER: OnceCell<Arc<MockServer>> = OnceCell::const_new();

/// Get or create the shared mock server instance on `127.0.0.1:3000`.
///
/// Service `config/test.toml` files (IAM OAuth, Hive collaborators) pin that
/// port. Use [`MockServerFixture::isolated`] when a test must not share it.
///
/// # Panics
///
/// Panics if `127.0.0.1:3000` cannot be bound.
#[allow(clippy::expect_used)]
pub async fn get_mock_server() -> Arc<MockServer> {
    MOCK_SERVER
        .get_or_init(|| async {
            let listener = std::net::TcpListener::bind("127.0.0.1:3000")
                .expect("Failed to bind shared wiremock on 127.0.0.1:3000");

            let server = Arc::new(MockServer::builder().listener(listener).start().await);
            debug!("🚀 Started shared wiremock server at: {}", server.uri());

            // Lifecycle note (T-4): wiremock is an in-process listener — the OS
            // reclaims it at process exit, and the process-exit guard owns
            // container teardown. No exit handler here: a Ctrl+C handler that
            // called std::process::exit would bypass the CRT atexit hook and
            // therefore the guard (proven Windows boundary), while a log-only
            // atexit no-op only LOOKED like a cleanup guarantee.

            server
        })
        .await
        .clone()
}

/// Get the base URL for the mock server
pub async fn get_mock_base_url() -> String {
    let server = get_mock_server().await;
    server.uri()
}

/// Reset all mocks on the shared server (for test isolation)
pub async fn reset_all_mocks() {
    if let Some(server) = MOCK_SERVER.get() {
        debug!("🧹 Resetting all wiremock mocks for test isolation");
        server.reset().await;
        debug!("✅ All wiremock mocks reset successfully");
    }
}

/// Test fixture wrapper for automatic mock cleanup
pub struct MockServerFixture {
    server: Arc<MockServer>,
}

impl MockServerFixture {
    /// Start a dedicated mock server for this fixture (no shared listener).
    ///
    /// # Panics
    ///
    /// Panics if an ephemeral localhost port cannot be bound.
    #[allow(clippy::expect_used)]
    pub async fn isolated() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")
            .expect("Failed to bind an ephemeral wiremock port");
        let server = Arc::new(MockServer::builder().listener(listener).start().await);
        Self { server }
    }

    /// Create a new mock server fixture on the shared listener with automatic cleanup.
    pub async fn new() -> Self {
        let server = get_mock_server().await;

        // Reset all existing mocks for test isolation
        reset_all_mocks().await;

        Self { server }
    }

    /// Get the mock server instance
    #[must_use]
    pub fn server(&self) -> Arc<MockServer> {
        self.server.clone()
    }

    /// Get the base URL
    #[must_use]
    pub fn base_url(&self) -> String {
        self.server.uri()
    }

    /// Manual reset (also happens automatically on drop)
    pub async fn reset(&self) {
        debug!("🧹 Manually resetting wiremock mocks");
        self.server.reset().await;
    }
}

impl Drop for MockServerFixture {
    fn drop(&mut self) {
        // Schedule cleanup in a blocking context
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            let server = self.server.clone();
            rt.spawn(async move {
                debug!("🧹 Cleaning up wiremock mocks on fixture drop");
                server.reset().await;
                debug!("✅ Wiremock mocks cleaned up successfully");
            });
        }
    }
}
