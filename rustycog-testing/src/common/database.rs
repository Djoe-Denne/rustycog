//! Test database utilities with testcontainers
//!
//! This module provides a single `PostgreSQL` container for all tests with table truncation
//! between tests to ensure test isolation while maintaining performance.

use crate::testing::common::openfga_testcontainer::TestOpenFga;
use crate::testing::common::sqs_testcontainer::TestSqs;
use crate::testing::common::ServiceTestDescriptor;
use rustycog::config::DatabaseConfig;
use rustycog::db::DbConnectionPool;
use sea_orm::{Database, DatabaseConnection, DbErr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::OnceLock;
use testcontainers::{runners::AsyncRunner, GenericImage, ImageExt};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Global test database container instance
static TEST_CONTAINER: OnceLock<Arc<Mutex<Option<Arc<TestDatabaseContainer>>>>> = OnceLock::new();

/// Flag to track if cleanup handler has been registered
static CLEANUP_REGISTERED: AtomicBool = AtomicBool::new(false);

/// Test database container wrapper
pub struct TestDatabaseContainer {
    container: super::fixture_runtime::OwnedContainer,
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
    _container: Arc<TestDatabaseContainer>,
    pub pool: DbConnectionPool,
    pub connection: Arc<DatabaseConnection>,
    pub database_url: String,
}

impl TestDatabase {
    /// Get or create the global test database instance
    ///
    /// # Errors
    ///
    /// Returns an error if the PostgreSQL test container cannot be started, the
    /// connection pool cannot be created, or migrations fail.
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
            _container: container,
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
///
/// The mutex guard is held for the whole initialization so two callers cannot
/// start two PostgreSQL containers at once.
#[allow(clippy::significant_drop_tightening)]
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

    // Explicit runner endpoint; never evict a discovered container.
    let endpoint = super::fixture_runtime::endpoint().map_err(DbErr::Custom)?;

    // Load test configuration to get database settings
    let db_config = create_base_test_config()?;

    // Determine the port to use
    let host_port = if db_config.port == 0 {
        // Use a random available port
        db_config.actual_port()
    } else {
        db_config.port
    };

    // Create PostgreSQL container using GenericImage with configuration-based settings
    let attempt = super::fixture_runtime::Attempt::prepare("postgres", host_port)
        .await
        .map_err(DbErr::Custom)?;
    let postgres_image = GenericImage::new("postgres", "15-alpine")
        .with_env_var("POSTGRES_DB", &db_config.db)
        .with_env_var("POSTGRES_USER", &db_config.creds.username)
        .with_env_var("POSTGRES_PASSWORD", &db_config.creds.password)
        .with_container_name(attempt.name())
        .with_mapped_port(host_port, testcontainers::core::ContainerPort::Tcp(5432)); // Map host port to container port 5432

    let container = attempt
        .start(postgres_image.start())
        .await
        .map_err(|e| DbErr::Custom(format!("Failed to start container: {e}")))?;

    let host_port = container
        .mapped_port(testcontainers::core::ContainerPort::Tcp(5432))
        .await
        .map_err(DbErr::Custom)?;
    let database_url = format!(
        "postgres://{}:{}@{}:{}/{}",
        db_config.creds.username, db_config.creds.password, endpoint.host, host_port, db_config.db
    );

    info!("Test database container started on port {}", host_port);
    // Do not log the connection URL, which contains credentials.

    // Wait for database to be ready
    wait_for_database(&database_url).await?;
    container.ready().map_err(DbErr::Custom)?;

    let test_container = Arc::new(TestDatabaseContainer {
        container,
        database_url,
        port: host_port,
    });

    *container_guard = Some(test_container.clone());

    // Register cleanup handler on first container creation
    register_cleanup_handler();

    Ok(test_container)
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
fn register_cleanup_handler() {
    extern "C" fn cleanup_on_exit() {
        debug!("Process exiting, attempting to cleanup test database container...");
        // Logging is not cleanup proof. The parent must join creations before
        // runtime shutdown and verify its exact-ID ledger against Docker.
    }

    // Only register once
    if CLEANUP_REGISTERED.swap(true, Ordering::SeqCst) {
        return;
    }

    info!("Registering test database container cleanup handler");

    // Signals must not infer ownership by a static name or bypass joined cleanup.

    unsafe {
        libc::atexit(cleanup_on_exit);
    }
}

/// Create a base test configuration
fn create_base_test_config() -> Result<DatabaseConfig, DbErr> {
    // Load configuration from test.toml
    // The RUN_ENV=test environment variable should be set by the justfile
    rustycog::config::load_config_part::<DatabaseConfig>("database").map_err(|e| {
        DbErr::Custom(format!(
            "Failed to load test configuration: {e}. Make sure RUN_ENV=test is set and config/test.toml exists."
        ))
    })
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
    ///
    /// # Errors
    ///
    /// Returns an error if `OpenFGA` is requested but no authorization model is
    /// provided, or if the `OpenFGA`, database, or `SQS` fixtures cannot be created.
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
                .ok_or_else(|| {
                    DbErr::Custom(
                        "ServiceTestDescriptor::has_openfga() returned true but \
                     openfga_authorization_model_json() returned None"
                            .to_owned(),
                    )
                })?;
            Some(
                TestOpenFga::new(model_json)
                    .await
                    .map_err(|e| DbErr::Custom(format!("Failed to create test OpenFGA: {e}")))?,
            )
        } else {
            None
        };

        let database = if descriptor.has_db() {
            Some(TestDatabase::new(descriptor.clone()).await?)
        } else {
            None
        };

        let sqs = if descriptor.has_sqs() {
            Some(
                TestSqs::new()
                    .await
                    .map_err(|e| DbErr::Custom(format!("Failed to create test SQS: {e}")))?,
            )
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
    ///
    /// # Panics
    ///
    /// Panics if the fixture was built without a database.
    #[allow(clippy::expect_used)]
    pub fn db(&self) -> Arc<DatabaseConnection> {
        self.database
            .as_ref()
            .expect("Database fixture was not requested by the test descriptor")
            .get_connection()
    }

    /// Get the SQS client
    ///
    /// # Panics
    ///
    /// Panics if the fixture was built without SQS.
    #[allow(clippy::expect_used)]
    pub const fn sqs(&self) -> &TestSqs {
        self.sqs
            .as_ref()
            .expect("SQS fixture was not requested by the test descriptor")
    }

    /// Get the `OpenFGA` fixture.
    ///
    /// # Panics
    ///
    /// Panics when `descriptor.has_openfga()` returned `false` at construction time.
    #[allow(clippy::expect_used)]
    pub const fn openfga(&self) -> &TestOpenFga {
        self.openfga
            .as_ref()
            .expect("OpenFGA fixture was not requested by the test descriptor")
    }

    /// Mutable handle to the `OpenFGA` fixture (for `reset()` etc.).
    ///
    /// # Panics
    ///
    /// Panics when `descriptor.has_openfga()` returned `false` at construction time.
    #[allow(clippy::expect_used)]
    pub const fn openfga_mut(&mut self) -> &mut TestOpenFga {
        self.openfga
            .as_mut()
            .expect("OpenFGA fixture was not requested by the test descriptor")
    }

    /// Cleanup the global test container (stops and removes it)
    ///
    /// # Errors
    ///
    /// Returns an error for active consumers, unresolved creations or unverified
    /// teardown. No container is also success when the creation barrier is clean.
    pub async fn cleanup_container() -> Result<(), DbErr> {
        // Join cancelled creations, but an unrelated UNKNOWN must not prevent
        // releasing this proven-owned handle. Recheck/report after teardown.
        let _ = super::fixture_runtime::join_fixture_creations().await;
        let Some(container_mutex) = TEST_CONTAINER.get() else {
            debug!("Test container mutex not initialized");
            return super::fixture_runtime::join_fixture_creations()
                .await
                .map_err(DbErr::Custom);
        };

        let mut container_guard = container_mutex.lock().await;
        info!("Manually cleaning up test database container");
        match super::fixture_runtime::take_unshared(&mut container_guard) {
            Ok(None) => super::fixture_runtime::join_fixture_creations()
                .await
                .map_err(DbErr::Custom),
            Ok(Some(container)) => {
                container.cleanup().await;
                super::fixture_runtime::join_fixture_creations()
                    .await
                    .map_err(DbErr::Custom)
            }
            Err(arc) => {
                arc.container.deferred();
                Err(DbErr::Custom(
                    "database cleanup deferred: active consumers".into(),
                ))
            }
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
