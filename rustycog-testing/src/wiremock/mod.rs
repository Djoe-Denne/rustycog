use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::OnceCell;
use tracing::{debug, info};
use wiremock::MockServer;

/// Shared wiremock server instance for all fixtures
static MOCK_SERVER: OnceCell<Arc<MockServer>> = OnceCell::const_new();

/// Flag to track if cleanup handler has been registered
static CLEANUP_REGISTERED: AtomicBool = AtomicBool::new(false);

/// Get or create the shared mock server instance
pub async fn get_mock_server() -> Arc<MockServer> {
    MOCK_SERVER
        .get_or_init(|| async {
            // Create a TCP listener on port 3000
            let listener =
                std::net::TcpListener::bind("127.0.0.1:3000").expect("Failed to bind to port 3000");

            let server = Arc::new(MockServer::builder().listener(listener).start().await);
            debug!("🚀 Started shared wiremock server at: {}", server.uri());

            // Register cleanup handler on first server creation
            register_cleanup_handler().await;

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

/// Register cleanup handler to reset mocks when process exits
async fn register_cleanup_handler() {
    // Only register once
    if CLEANUP_REGISTERED.swap(true, Ordering::SeqCst) {
        return;
    }

    info!("📝 Registering wiremock cleanup handler");

    // Register cleanup for Ctrl+C and other signals
    let _ = ctrlc::set_handler(move || {
        info!("🧹 Received termination signal, cleaning up wiremock server");
        // Note: We can't do async cleanup here, but the server will be cleaned up
        // when the process exits. This is just for logging.
        std::process::exit(0);
    });

    // Register cleanup for normal process termination
    extern "C" fn cleanup_on_exit() {
        debug!("🧹 Process exiting, wiremock server will be cleaned up automatically");
    }

    unsafe {
        libc::atexit(cleanup_on_exit);
    }
}

/// Test fixture wrapper for automatic mock cleanup
pub struct MockServerFixture {
    server: Arc<MockServer>,
}

impl MockServerFixture {
    /// Create a new mock server fixture with automatic cleanup
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
