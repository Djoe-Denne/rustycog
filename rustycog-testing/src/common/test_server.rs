use crate::common::{build_test_app, spawn_test_server, ServiceTestDescriptor};
use reqwest::Client;
use rustycog_config::{load_config_part, ServerConfig};
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
static TEST_SERVER: OnceLock<Arc<Mutex<Option<JoinHandle<()>>>>> = OnceLock::new();
static TEST_SERVER_DESCRIPTOR_TYPE: OnceLock<Arc<Mutex<Option<TypeId>>>> = OnceLock::new();

/// Get or create the global test server instance
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
    let descriptor_changed = descriptor_type_guard
        .map(|existing| existing != descriptor_type)
        .unwrap_or(false);

    if descriptor_changed {
        if let Some(handle) = server_guard.take() {
            debug!("🔄 Descriptor changed, restarting test server...");
            handle.abort();
            let _ = handle.await;
        }
        *descriptor_type_guard = None;
    }

    // Check if we need to start a new server
    let needs_new_server = match server_guard.as_ref() {
        None => true,                         // No server handle exists
        Some(handle) => handle.is_finished(), // Server handle exists but task is finished
    };

    let server_config =
        load_config_part::<ServerConfig>("server").expect("failed to load server config");
    let server_port = server_config.actual_port();
    let base_url = format!("http://{}:{}", server_config.host, server_port);

    if needs_new_server {
        // If the old handle is finished, clear it
        if server_guard.is_some() {
            debug!("🔄 Previous server has stopped, starting a new one...");
            *server_guard = None;
            *descriptor_type_guard = None;
        }

        build_test_app::<D, T>(descriptor.clone()).await?;

        // Start the server using the existing spawn_test_server function
        let server_handle = tokio::spawn(async move {
            if let Err(e) = spawn_test_server::<D, T>(descriptor.clone()).await {
                debug!("Server failed to start: {}", e);
            }
        });

        *server_guard = Some(server_handle);
        *descriptor_type_guard = Some(descriptor_type);

        wait_for_server_ready(&server_config.host, server_port, server_guard.as_ref()).await?;
    } else {
        debug!("♻️  Reusing existing server instance");
    }

    // Return the base URL based on test config
    debug!("🔗 Test client will connect to: {}", base_url);
    Ok(base_url)
}

// method that return a test fixture, base_url and client
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

#[must_use]
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
