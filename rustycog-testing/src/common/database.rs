//! Test database utilities with testcontainers
//!
//! This module provides a single `PostgreSQL` container for all tests with table truncation
//! between tests to ensure test isolation while maintaining performance.

use crate::common::openfga_testcontainer::TestOpenFga;
use crate::common::sqs_testcontainer::TestSqs;
use crate::common::ServiceTestDescriptor;
use rustycog_config::DatabaseConfig;
use rustycog_db::DbConnectionPool;
use sea_orm::{Database, DatabaseConnection, DbErr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::OnceLock;
use testcontainers::{runners::AsyncRunner, ContainerAsync, GenericImage, ImageExt};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Global test database container instance
static TEST_CONTAINER: OnceLock<Arc<Mutex<Option<Arc<TestDatabaseContainer>>>>> = OnceLock::new();

/// Flag to track if cleanup handler has been registered
static CLEANUP_REGISTERED: AtomicBool = AtomicBool::new(false);

/// Test database container wrapper
pub struct TestDatabaseContainer {
    container: ContainerAsync<GenericImage>,
    pub database_url: String,
    pub port: u16,
}

impl TestDatabaseContainer {
    /// Stop and remove the container
    pub async fn cleanup(self) {
        info!("Stopping and removing test database container");
        if let Err(e) = self.container.stop().await {
            warn!("Failed to stop container: {}", e);
        } else {
            info!("Container stopped successfully");
        }
        if let Err(e) = self.container.rm().await {
            warn!("Failed to remove container: {}", e);
        } else {
            info!("Container removed successfully");
        }
        info!("Test database container cleanup completed");
    }
}

/// Test database fixture providing database connection and cleanup utilities
pub struct TestDatabase {
    pub pool: DbConnectionPool,
    pub connection: Arc<DatabaseConnection>,
    pub database_url: String,
}

impl TestDatabase {
    /// Get or create the global test database instance
    pub async fn new<D, T>(descriptor: Arc<D>) -> Result<Self, DbErr>
    where
        D: ServiceTestDescriptor<T>,
        T: Send + Sync + 'static,
    {
        let container = get_or_create_test_container().await?;
        let database_url = container.database_url.clone();

        // Create connection pool
        let pool = DbConnectionPool::new_from_url(&database_url, vec![]).await?;
        let connection = pool.get_write_connection();

        // Run migrations
        Self::run_migrations(descriptor, &connection).await?;

        Ok(Self {
            pool,
            connection,
            database_url,
        })
    }

    /// Run database migrations
    async fn run_migrations<D, T>(
        descriptor: Arc<D>,
        connection: &DatabaseConnection,
    ) -> Result<(), DbErr>
    where
        D: ServiceTestDescriptor<T>,
        T: Send + Sync + 'static,
    {
        info!("Try migration down first, to ensure we start with a clean slate");
        if let Err(e) = descriptor.run_migrations_down(connection).await {
            warn!("Failed to run migrations down: {}", e);
        }

        info!("Migration down completed successfully");

        info!("Running database migrations for test database");
        descriptor
            .run_migrations_up(connection)
            .await
            .map_err(|e| DbErr::Custom(e.to_string()))?;
        info!("Database migrations completed successfully");
        Ok(())
    }

    /// Get the database connection for direct use
    pub fn get_connection(&self) -> Arc<DatabaseConnection> {
        self.connection.clone()
    }

    /// Get the connection pool
    pub const fn get_pool(&self) -> &DbConnectionPool {
        &self.pool
    }
}

/// Get or create the global test container
async fn get_or_create_test_container() -> Result<Arc<TestDatabaseContainer>, DbErr> {
    let container_mutex = TEST_CONTAINER.get_or_init(|| Arc::new(Mutex::new(None)));

    let mut container_guard = container_mutex.lock().await;

    if let Some(ref container) = *container_guard {
        return Ok(container.clone());
    }

    info!("Creating new PostgreSQL test container");

    // Clear only the database port cache to ensure fresh random port generation
    // Don't clear all caches as that would interfere with Kafka test containers
    DatabaseConfig::clear_port_cache();

    // First, try to clean up any existing container with the same name
    cleanup_existing_container().await;

    // Load test configuration to get database settings
    let db_config = create_base_test_config();

    // Determine the port to use
    let host_port = if db_config.port == 0 {
        // Use a random available port
        db_config.actual_port()
    } else {
        db_config.port
    };

    // Create PostgreSQL container using GenericImage with configuration-based settings
    let postgres_image = GenericImage::new("postgres", "15-alpine")
        .with_env_var("POSTGRES_DB", &db_config.db)
        .with_env_var("POSTGRES_USER", &db_config.creds.username)
        .with_env_var("POSTGRES_PASSWORD", &db_config.creds.password)
        .with_container_name("test-db") // Static name for easy cleanup
        .with_mapped_port(host_port, testcontainers::core::ContainerPort::Tcp(5432)); // Map host port to container port 5432

    let container = postgres_image
        .start()
        .await
        .map_err(|e| DbErr::Custom(format!("Failed to start container: {e}")))?;

    let database_url = format!(
        "postgres://{}:{}@{}:{}/{}",
        db_config.creds.username, db_config.creds.password, db_config.host, host_port, db_config.db
    );

    info!("Test database container started on port {}", host_port);
    info!("Database URL: {}", database_url);

    // Wait for database to be ready
    wait_for_database(&database_url).await?;

    let test_container = Arc::new(TestDatabaseContainer {
        container,
        database_url,
        port: host_port,
    });

    *container_guard = Some(test_container.clone());

    // Register cleanup handler on first container creation
    register_cleanup_handler().await;

    Ok(test_container)
}

/// Clean up any existing container with the test name
async fn cleanup_existing_container() {
    use std::process::Command;

    debug!("Checking for existing test container 'test-db'");

    // Try to stop the container if it's running
    let stop_result = Command::new("docker").args(["stop", "test-db"]).output();

    match stop_result {
        Ok(output) if output.status.success() => {
            debug!("Stopped existing container 'test-db'");
        }
        Ok(_) => {
            debug!("Container 'test-db' was not running or doesn't exist");
        }
        Err(e) => {
            debug!("Failed to stop container: {}", e);
        }
    }

    // Try to remove the container
    let rm_result = Command::new("docker")
        .args(["rm", "-f", "test-db"])
        .output();

    match rm_result {
        Ok(output) if output.status.success() => {
            debug!("Removed existing container 'test-db'");
        }
        Ok(_) => {
            debug!("Container 'test-db' was already removed or doesn't exist");
        }
        Err(e) => {
            debug!("Failed to remove container: {}", e);
        }
    }
}

/// Wait for the database to be ready for connections
async fn wait_for_database(database_url: &str) -> Result<(), DbErr> {
    use tokio::time::{sleep, timeout, Duration};

    info!("Waiting for database to be ready...");

    let max_attempts = 30;
    let mut attempts = 0;

    while attempts < max_attempts {
        match timeout(Duration::from_secs(2), Database::connect(database_url)).await {
            Ok(Ok(conn)) => {
                // Test the connection with a simple query
                match conn.ping().await {
                    Ok(()) => {
                        info!("Database is ready after {} attempts", attempts + 1);
                        return Ok(());
                    }
                    Err(e) => {
                        debug!("Database ping failed: {}", e);
                    }
                }
            }
            Ok(Err(e)) => {
                debug!("Database connection failed: {}", e);
            }
            Err(_) => {
                debug!("Database connection timed out");
            }
        }

        attempts += 1;
        if attempts < max_attempts {
            debug!(
                "Retrying database connection in 1 second... (attempt {}/{})",
                attempts, max_attempts
            );
            sleep(Duration::from_secs(1)).await;
        }
    }

    Err(DbErr::Custom(format!(
        "Database failed to become ready after {max_attempts} attempts"
    )))
}

/// Register cleanup handler to stop container when process exits
async fn register_cleanup_handler() {
    // Only register once
    if CLEANUP_REGISTERED.swap(true, Ordering::SeqCst) {
        return;
    }

    info!("Registering test database container cleanup handler");

    // Register cleanup for Ctrl+C and other signals
    let _ = ctrlc::set_handler(move || {
        info!("Received termination signal, cleaning up test database container");

        // Use direct docker command to cleanup the specific container
        use std::process::Command;
        let _ = Command::new("docker").args(["stop", "test-db"]).output();
        let _ = Command::new("docker").args(["rm", "test-db"]).output();

        std::process::exit(0);
    });

    // Register cleanup for normal process termination
    extern "C" fn cleanup_on_exit() {
        debug!("Process exiting, attempting to cleanup test database container...");
        // Note: We can't do async cleanup here, but the container will be cleaned up
        // by Docker eventually. This is just for logging.
    }

    unsafe {
        libc::atexit(cleanup_on_exit);
    }
}

/// Create a base test configuration
fn create_base_test_config() -> DatabaseConfig {
    // Load configuration from test.toml
    // The RUN_ENV=test environment variable should be set by the justfile
    rustycog_config::load_config_part::<DatabaseConfig>("database").expect(
        "Failed to load test configuration. Make sure RUN_ENV=test is set and config/test.toml exists."
    )
}

/// Test fixture that automatically cleans up after each test
pub struct TestFixture {
    pub database: Option<TestDatabase>,
    pub sqs: Option<TestSqs>,
    pub openfga: Option<TestOpenFga>,
    /// Flag to track if this fixture should cleanup the container on drop
    cleanup_container_on_drop: bool,
}

impl TestFixture {
    /// Create a new test fixture with database cleanup
    pub async fn new<D, T>(descriptor: Arc<D>) -> Result<Self, DbErr>
    where
        D: ServiceTestDescriptor<T>,
        T: Send + Sync + 'static,
    {
        // OpenFGA is provisioned **before** the database on purpose: the
        // testcontainer's constructor publishes `*_OPENFGA__*` env vars
        // that the service-under-test will read when its typed config is
        // loaded. Booting the app before publishing those vars would
        // produce an `OpenFgaPermissionChecker` pointing at the
        // `test.toml` placeholders.
        let openfga = if descriptor.has_openfga() {
            let model_json = descriptor
                .openfga_authorization_model_json()
                .expect(
                    "ServiceTestDescriptor::has_openfga() returned true but \
                     openfga_authorization_model_json() returned None",
                );
            Some(
                TestOpenFga::new(model_json)
                    .await
                    .expect("Failed to create test OpenFGA"),
            )
        } else {
            None
        };

        let database = if descriptor.has_db() {
            Some(
                TestDatabase::new(descriptor.clone())
                    .await
                    .expect("Failed to create test database"),
            )
        } else {
            None
        };

        let sqs = if descriptor.has_sqs() {
            Some(TestSqs::new().await.expect("Failed to create test SQS"))
        } else {
            None
        };

        Ok(Self {
            database,
            sqs,
            openfga,
            cleanup_container_on_drop: false,
        })
    }

    /// Get the database connection
    pub fn db(&self) -> Arc<DatabaseConnection> {
        self.database.as_ref().unwrap().get_connection()
    }

    /// Get the SQS client
    pub const fn sqs(&self) -> &TestSqs {
        self.sqs.as_ref().unwrap()
    }

    /// Get the `OpenFGA` fixture. Panics when `descriptor.has_openfga()`
    /// returned `false` at construction time.
    pub const fn openfga(&self) -> &TestOpenFga {
        self.openfga
            .as_ref()
            .expect("OpenFGA fixture was not requested by the test descriptor")
    }

    /// Mutable handle to the `OpenFGA` fixture (for `reset()` etc.).
    pub const fn openfga_mut(&mut self) -> &mut TestOpenFga {
        self.openfga
            .as_mut()
            .expect("OpenFGA fixture was not requested by the test descriptor")
    }

    /// Cleanup the global test container (stops and removes it)
    pub async fn cleanup_container() -> Result<(), DbErr> {
        let Some(container_mutex) = TEST_CONTAINER.get() else {
            debug!("Test container mutex not initialized");
            return Ok(());
        };

        let mut container_guard = container_mutex.lock().await;
        let Some(container_arc) = container_guard.take() else {
            debug!("No test container to cleanup");
            return Ok(());
        };

        info!("Manually cleaning up test database container");
        cleanup_test_db_container(container_arc).await;
        Ok(())
    }
}

async fn cleanup_test_db_container(container_arc: Arc<TestDatabaseContainer>) {
    match Arc::try_unwrap(container_arc) {
        Ok(container) => {
            container.cleanup().await;
            info!("Test database container cleanup completed");
        }
        Err(arc) => {
            warn!(
                "Could not cleanup container: still has {} references",
                Arc::strong_count(&arc)
            );
            info!("Attempting fallback cleanup using Docker commands");
            cleanup_test_db_container_with_docker();
        }
    }
}

fn cleanup_test_db_container_with_docker() {
    const CONTAINER_NAME: &str = "test-db";
    run_docker_cleanup_command("stop", &[CONTAINER_NAME], "stopped");
    run_docker_cleanup_command("rm", &["-f", CONTAINER_NAME], "removed");
}

fn run_docker_cleanup_command(command: &str, args: &[&str], success_action: &str) {
    match std::process::Command::new("docker")
        .arg(command)
        .args(args)
        .output()
    {
        Ok(output) if output.status.success() => {
            info!("Successfully {} container {}", success_action, "test-db");
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Failed to {} container test-db: {}", command, stderr);
        }
        Err(e) => {
            warn!("Failed to execute docker {}: {}", command, e);
        }
    }
}

impl Drop for TestFixture {
    fn drop(&mut self) {
        // Schedule cleanup in a blocking context
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            let cleanup_container = self.cleanup_container_on_drop;

            rt.spawn(async move {
                // Optionally cleanup container
                if cleanup_container {
                    info!("Cleaning up test container on TestFixture drop");
                    if let Err(e) = Self::cleanup_container().await {
                        warn!("Failed to cleanup container on drop: {}", e);
                    }
                }
            });
        }
    }
}
