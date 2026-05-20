//! # `RustyCog` Database
//!
//! Database management utilities including connection pooling and migrations.
use rustycog_config::DatabaseConfig;
use sea_orm::{
    ConnectOptions, Database, DatabaseConnection, DatabaseTransaction, DbErr, TransactionTrait,
};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

/// Database connection pool with read-write split capabilities
pub struct DbConnectionPool {
    /// Primary/write connection for database modifications
    write_connection: Arc<DatabaseConnection>,
    /// Read connections for database queries
    read_connections: Vec<Arc<DatabaseConnection>>,
    /// Current read connection index for round-robin load balancing
    current_read_index: std::sync::atomic::AtomicUsize,
}

impl Clone for DbConnectionPool {
    fn clone(&self) -> Self {
        Self {
            write_connection: self.write_connection.clone(),
            read_connections: self.read_connections.clone(),
            current_read_index: std::sync::atomic::AtomicUsize::new(
                self.current_read_index
                    .load(std::sync::atomic::Ordering::SeqCst),
            ),
        }
    }
}

impl DbConnectionPool {
    /// Create a new connection pool with the given database configuration
    pub async fn new(db_config: &DatabaseConfig) -> Result<Self, DbErr> {
        let db_url = db_config.url();

        // Create the write connection
        let mut opt = ConnectOptions::new(db_url.clone());
        opt.max_connections(32)
            .min_connections(5)
            .connect_timeout(Duration::from_secs(8))
            .acquire_timeout(Duration::from_secs(8))
            .idle_timeout(Duration::from_secs(8))
            .max_lifetime(Duration::from_secs(8))
            .sqlx_logging(true);

        let write_conn = Database::connect(opt).await?;

        // Create read connections (replicas if provided, otherwise use the main connection)
        let mut read_connections = Vec::new();

        if db_config.read_replicas.is_empty() {
            // Use the write connection for reads if no replicas are provided
            info!("No read replicas specified, using primary database for reads");
            read_connections.push(Arc::new(write_conn.clone()));
        } else {
            // Connect to each read replica
            for (i, replica_url) in db_config.read_replicas.iter().enumerate() {
                info!("Connecting to read replica {}", i + 1);
                let mut opt = ConnectOptions::new(replica_url.to_owned());
                opt.max_connections(32)
                    .min_connections(5)
                    .connect_timeout(Duration::from_secs(8))
                    .acquire_timeout(Duration::from_secs(8))
                    .idle_timeout(Duration::from_secs(8))
                    .max_lifetime(Duration::from_secs(8))
                    .sqlx_logging(true);

                match Database::connect(opt).await {
                    Ok(conn) => {
                        read_connections.push(Arc::new(conn));
                    }
                    Err(e) => {
                        warn!("Failed to connect to read replica {}: {}", i + 1, e);
                    }
                }
            }

            // If all replicas failed, fall back to the write connection
            if read_connections.is_empty() {
                warn!("All read replicas failed to connect, falling back to primary database for reads");
                read_connections.push(Arc::new(write_conn.clone()));
            }
        }

        Ok(Self {
            write_connection: Arc::new(write_conn),
            read_connections,
            current_read_index: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    /// Create a new connection pool with the given database URL (for backward compatibility)
    pub async fn new_from_url(db_url: &str, read_replicas: Vec<String>) -> Result<Self, DbErr> {
        // Parse the URL to create a DatabaseConfig
        let db_config = DatabaseConfig::from_url(db_url)
            .map_err(|e| DbErr::Custom(format!("Failed to parse database URL: {e}")))?;

        let mut config = db_config;
        config.read_replicas = read_replicas;

        Self::new(&config).await
    }

    /// Get a connection for write operations
    pub fn get_write_connection(&self) -> Arc<DatabaseConnection> {
        self.write_connection.clone()
    }

    /// Begin a transaction on the primary/write connection.
    ///
    /// Transactional workflows must use the primary connection so reads and
    /// writes participate in the same committed unit instead of crossing read
    /// replicas that may lag behind the write.
    pub async fn begin_write_transaction(&self) -> Result<DatabaseTransaction, DbErr> {
        self.write_connection.begin().await
    }

    /// Get a connection for read operations (round-robin if multiple replicas)
    pub fn get_read_connection(&self) -> Arc<DatabaseConnection> {
        let len = self.read_connections.len();
        if len == 0 {
            return self.write_connection.clone();
        }

        if len == 1 {
            return self.read_connections[0].clone();
        }

        // Round-robin selection of read connections
        let index = self
            .current_read_index
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            % len;
        self.read_connections[index].clone()
    }
}
