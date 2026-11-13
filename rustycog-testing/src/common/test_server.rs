use super::http_test::spawn_test_server_with_config;
use crate::testing::common::{build_test_app, ServiceTestDescriptor};
use reqwest::Client;
use rustycog::config::{load_config_part, ServerConfig};
use std::any::TypeId;
use std::io;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, warn};

/// Global test server instance that starts only once
static TEST_SERVER: OnceLock<Arc<Mutex<Option<TestServer>>>> = OnceLock::new();
static TEST_SERVER_DESCRIPTOR_TYPE: OnceLock<Arc<Mutex<Option<TypeId>>>> = OnceLock::new();

struct TestServer {
    handle: JoinHandle<()>,
    base_url: String,
}

/// Resolve a new server's port after its dependency fixtures have started.
/// A port cached by an earlier runtime may now belong to wiremock or another listener.
fn resolve_new_server_config(mut config: ServerConfig) -> io::Result<ServerConfig> {
    if config.port == 0 {
        let listener = std::net::TcpListener::bind((config.host.as_str(), 0))?;
        config.port = listener.local_addr()?.port();
    }
    Ok(config)
}

/// Get or create the global test server instance
///
/// # Errors
///
/// Returns an error if the server config cannot be loaded, the test app cannot
/// be built, or the server does not become ready.
#[allow(clippy::significant_drop_tightening)]
pub async fn get_test_server<D, T>(descriptor: Arc<D>) -> Result<String, Box<dyn std::error::Error>>
where
    D: ServiceTestDescriptor<T>,
    T: Send + Sync + 'static,
{
    let server_mutex = TEST_SERVER.get_or_init(|| Arc::new(Mutex::new(None)));
    let descriptor_type_mutex =
        TEST_SERVER_DESCRIPTOR_TYPE.get_or_init(|| Arc::new(Mutex::new(None)));

    let mut server_guard = server_mutex.lock().await;
    let mut descriptor_type_guard = descriptor_type_mutex.lock().await;
    let descriptor_type = TypeId::of::<D>();
    let descriptor_changed =
        descriptor_type_guard.is_some_and(|existing| existing != descriptor_type);

    if descriptor_changed {
        if let Some(server) = server_guard.take() {
            debug!("🔄 Descriptor changed, restarting test server...");
            server.handle.abort();
            let _ = server.handle.await;
        }
        *descriptor_type_guard = None;
    }

    // Check if we need to start a new server
    let needs_new_server = server_guard
        .as_ref()
        .is_none_or(|server| server.handle.is_finished());

    if needs_new_server {
        // If the old handle is finished, clear it
        if server_guard.is_some() {
            debug!("🔄 Previous server has stopped, starting a new one...");
            *server_guard = None;
            *descriptor_type_guard = None;
        }

        build_test_app::<D, T>(descriptor.clone()).await?;

        let server_config = resolve_new_server_config(load_config_part::<ServerConfig>("server")?)?;
        let server_port = server_config.port;
        let base_url = format!("http://{}:{}", server_config.host, server_port);

        // Start the server using the existing spawn_test_server function
        let server_handle = tokio::spawn(async move {
            if let Err(e) =
                spawn_test_server_with_config::<D, T>(descriptor.clone(), Some(server_port)).await
            {
                debug!("Server failed to start: {}", e);
            }
        });

        *server_guard = Some(TestServer {
            handle: server_handle,
            base_url,
        });
        *descriptor_type_guard = Some(descriptor_type);

        wait_for_server_ready(
            &server_config.host,
            server_port,
            server_guard.as_ref().map(|server| &server.handle),
        )
        .await?;
    } else {
        debug!("♻️  Reusing existing server instance");
    }

    // Return the base URL based on test config
    let base_url = server_guard
        .as_ref()
        .ok_or_else(|| io::Error::other("test server missing after startup"))?
        .base_url
        .clone();
    debug!("🔗 Test client will connect to: {}", base_url);
    Ok(base_url)
}

/// Start or reuse the global test server and return its base URL plus an HTTP client.
///
/// # Errors
///
/// Returns an error if the test server cannot be started or is not ready.
pub async fn setup_test_server<D, T>(
    descriptor: Arc<D>,
) -> Result<(String, Client), Box<dyn std::error::Error>>
where
    D: ServiceTestDescriptor<T>,
    T: Send + Sync + 'static,
{
    let base_url = get_test_server::<D, T>(descriptor.clone()).await?;
    let client = create_test_client();
    Ok((base_url, client))
}

/// Build an HTTP client that does not follow redirects.
///
/// # Panics
///
/// Panics if the reqwest client cannot be built.
#[must_use]
#[allow(clippy::expect_used)]
pub fn create_test_client() -> Client {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("Failed to create HTTP client")
}

async fn wait_for_server_ready(
    host: &str,
    port: u16,
    handle: Option<&JoinHandle<()>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let address = format!("{host}:{port}");
    let max_attempts = 50;

    for attempt in 1..=max_attempts {
        if let Some(handle) = handle {
            if handle.is_finished() {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionRefused,
                    format!("test server task exited before {address} became ready"),
                )
                .into());
            }
        }

        match TcpStream::connect(&address).await {
            Ok(_) => {
                debug!("✅ Test server is ready at {}", address);
                return Ok(());
            }
            Err(error) => {
                debug!(
                    "Waiting for test server at {} ({}/{}): {}",
                    address, attempt, max_attempts, error
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }

    warn!("Test server did not become ready at {}", address);
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        format!("test server at {address} did not become ready"),
    )
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_server_does_not_reuse_a_cached_port_now_owned_by_a_fixture() {
        let config = ServerConfig {
            host: "127.0.0.1".into(),
            port: 0,
            ..ServerConfig::default()
        };
        let cached = config.actual_port();
        // Reproduce the CI condition: the application stopped and wiremock
        // acquired its former port. A TCP-only probe would accept this listener.
        let fixture = std::net::TcpListener::bind(("127.0.0.1", cached)).unwrap();
        let resolved = resolve_new_server_config(config.clone()).unwrap();
        assert_ne!(resolved.port, cached);
        assert_ne!(resolved.port, 0);
        assert_eq!(
            config.actual_port(),
            cached,
            "production cache remains unchanged"
        );
        let application =
            std::net::TcpListener::bind((resolved.host.as_str(), resolved.port)).unwrap();
        assert_eq!(application.local_addr().unwrap().port(), resolved.port);
        drop(fixture);
    }

    #[test]
    fn explicit_server_port_is_not_silently_changed_when_occupied() {
        let fixture = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = fixture.local_addr().unwrap().port();
        let config = ServerConfig {
            host: "127.0.0.1".into(),
            port,
            ..ServerConfig::default()
        };
        assert_eq!(resolve_new_server_config(config).unwrap().port, port);
    }
}
