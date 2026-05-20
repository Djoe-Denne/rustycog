//! Core configuration crate for `RustyCog` services
//!
//! This crate provides core configuration structures and utilities that can be shared
//! across multiple services, including server, database, command retry, Kafka, and logging configuration.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use tracing::debug;

// Re-export config and dotenvy for service use
pub use config::{Config, ConfigError, Environment, File, FileFormat};
pub use dotenvy::dotenv;

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server host
    pub host: String,
    /// Server port
    pub port: u16,
    /// Whether TLS/HTTPS is enabled
    #[serde(default)]
    pub tls_enabled: bool,
    /// Path to TLS certificate file
    #[serde(default = "default_cert_path")]
    pub tls_cert_path: String,
    /// Path to TLS private key file
    #[serde(default = "default_key_path")]
    pub tls_key_path: String,
    /// Port to use when TLS is enabled
    #[serde(default = "default_tls_port")]
    pub tls_port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 8080,
            tls_enabled: false,
            tls_cert_path: default_cert_path(),
            tls_key_path: default_key_path(),
            tls_port: default_tls_port(),
        }
    }
}

impl ServerConfig {
    /// Resolve the configured HTTP port. When `port == 0`, pick one free host
    /// port and cache it so the server and test client agree.
    pub fn actual_port(&self) -> u16 {
        if self.port == 0 {
            let cache_key = format!("server:{}", self.host);
            let cache = PORT_CACHE.get_or_init(|| Arc::new(Mutex::new(HashMap::new())));
            let mut port_cache = cache.lock().unwrap();

            if let Some(&cached_port) = port_cache.get(&cache_key) {
                return cached_port;
            }

            let random_port = Self::get_random_port();
            port_cache.insert(cache_key, random_port);
            debug!("Generated random server port: {}", random_port);
            random_port
        } else {
            self.port
        }
    }

    fn get_random_port() -> u16 {
        use std::net::{SocketAddr, TcpListener};

        match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => match listener.local_addr() {
                Ok(SocketAddr::V4(addr)) => addr.port(),
                Ok(SocketAddr::V6(addr)) => addr.port(),
                Err(_) => 8080,
            },
            Err(_) => 8080,
        }
    }
}

fn default_cert_path() -> String {
    "./certs/cert.pem".to_string()
}

fn default_key_path() -> String {
    "./certs/key.pem".to_string()
}

const fn default_tls_port() -> u16 {
    8443
}

/// Database credentials configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseCredentials {
    /// Database username
    pub username: String,
    /// Database password
    pub password: String,
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Database credentials
    pub creds: DatabaseCredentials,
    /// Database host
    pub host: String,
    /// Database port (5432 default, 0 for random port)
    #[serde(default = "default_db_port")]
    pub port: u16,
    /// Database name
    pub db: String,
    /// Read replica database URLs (still using full URLs for flexibility)
    #[serde(default)]
    pub read_replicas: Vec<String>,
}

const fn default_db_port() -> u16 {
    5432
}

/// Global cache for resolved random ports to ensure consistency
static PORT_CACHE: OnceLock<Arc<Mutex<HashMap<String, u16>>>> = OnceLock::new();

impl DatabaseConfig {
    /// Construct the primary database URL from components
    #[must_use]
    pub fn url(&self) -> String {
        let port = self.actual_port();

        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.creds.username, self.creds.password, self.host, port, self.db
        )
    }

    /// Get a random available port
    fn get_random_port() -> u16 {
        use std::net::{SocketAddr, TcpListener};

        // Try to bind to a random port
        match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => {
                match listener.local_addr() {
                    Ok(SocketAddr::V4(addr)) => addr.port(),
                    Ok(SocketAddr::V6(addr)) => addr.port(),
                    Err(_) => 5432, // fallback to default
                }
            }
            Err(_) => 5432, // fallback to default
        }
    }

    /// Get the actual port being used (resolves random port if needed)
    /// This method caches the resolved port to ensure consistency across calls
    pub fn actual_port(&self) -> u16 {
        if self.port == 0 {
            // Create a unique cache key for this database configuration
            let cache_key = format!("{}:{}:{}", self.host, self.db, self.creds.username);

            let cache = PORT_CACHE.get_or_init(|| Arc::new(Mutex::new(HashMap::new())));
            let mut port_cache = cache.lock().unwrap();

            // Return cached port if available
            if let Some(&cached_port) = port_cache.get(&cache_key) {
                return cached_port;
            }

            // Generate new random port and cache it
            let random_port = Self::get_random_port();
            port_cache.insert(cache_key, random_port);
            random_port
        } else {
            self.port
        }
    }

    /// Create a new `DatabaseConfig` with the specified components
    #[must_use]
    pub const fn new(
        username: String,
        password: String,
        host: String,
        port: u16,
        db: String,
    ) -> Self {
        Self {
            creds: DatabaseCredentials { username, password },
            host,
            port,
            db,
            read_replicas: vec![],
        }
    }

    /// Create a `DatabaseConfig` from a URL (for backward compatibility)
    pub fn from_url(url: &str) -> Result<Self, String> {
        use url::Url;

        let parsed = Url::parse(url).map_err(|e| format!("Invalid URL: {e}"))?;

        if parsed.scheme() != "postgres" && parsed.scheme() != "postgresql" {
            return Err("URL must use postgres:// or postgresql:// scheme".to_string());
        }

        let username = parsed.username().to_string();
        let password = parsed.password().unwrap_or("").to_string();
        let host = parsed.host_str().unwrap_or("localhost").to_string();
        let port = parsed.port().unwrap_or(5432);
        let db = parsed.path().trim_start_matches('/').to_string();

        if db.is_empty() {
            return Err("Database name is required in URL path".to_string());
        }

        Ok(Self::new(username, password, host, port, db))
    }

    /// Clear the port cache (useful for testing)
    pub fn clear_port_cache() {
        if let Some(cache) = PORT_CACHE.get() {
            let mut port_cache = cache.lock().unwrap();

            // Create a unique cache key for this database configuration
            port_cache.remove(&"db".to_string());
            debug!("DB port cleared from cache");
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            creds: DatabaseCredentials {
                username: "postgres".to_string(),
                password: "postgres".to_string(),
            },
            host: "localhost".to_string(),
            port: 0, // Use random port by default
            db: "app_database".to_string(),
            read_replicas: vec![],
        }
    }
}

/// Shared authentication configuration for service-side bearer token verification
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthConfig {
    /// JWT verification settings
    #[serde(default)]
    pub jwt: JwtAuthConfig,
}

/// JWT verification configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JwtAuthConfig {
    /// HS256 secret used to verify bearer tokens
    #[serde(default)]
    pub hs256_secret: Option<String>,
}

/// Scaleway configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalewayConfig {
    /// Scaleway project ID
    pub project_id: String,
    /// Scaleway organization ID
    pub organization_id: String,
    /// region
    pub region: String,
    /// access key
    pub access_key: String,
    /// secret key
    pub secret_key: String,
}

const fn default_scaleway_project_id() -> String {
    String::new()
}

const fn default_scaleway_organization_id() -> String {
    String::new()
}

const fn default_scaleway_region() -> String {
    String::new()
}

const fn default_scaleway_access_key() -> String {
    String::new()
}

const fn default_scaleway_secret_key() -> String {
    String::new()
}

impl Default for ScalewayConfig {
    fn default() -> Self {
        Self {
            project_id: default_scaleway_project_id(),
            organization_id: default_scaleway_organization_id(),
            region: default_scaleway_region(),
            access_key: default_scaleway_access_key(),
            secret_key: default_scaleway_secret_key(),
        }
    }
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleLoggingOutput {
    /// Whether console logging output is enabled.
    #[serde(default = "default_console_logging_enabled")]
    pub enabled: bool,
}

impl Default for ConsoleLoggingOutput {
    fn default() -> Self {
        Self {
            enabled: default_console_logging_enabled(),
        }
    }
}

const fn default_console_logging_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileLoggingOutput {
    /// file path
    pub path: String,
}

impl Default for FileLoggingOutput {
    fn default() -> Self {
        Self {
            path: "logs/rustycog.log".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScalewayLokiLoggingOutput {
    /// scaleway datasource uuid
    pub datasource_uuid: String,
    /// scaleway cockpit token
    pub cockpit_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub level: String,
    /// Optional target directives (tracing `EnvFilter` syntax).
    /// Example: "`warn,my_service=debug,tokio=warn`"
    #[serde(default)]
    pub filter: Option<String>,
    /// Logging output
    #[serde(default = "default_console_logging_output")]
    pub console: Option<ConsoleLoggingOutput>,
    /// Logging output
    #[serde(default = "default_file_logging_output")]
    pub file: Option<FileLoggingOutput>,
    /// Logging output
    #[serde(default = "default_scaleway_loki_logging_output")]
    pub scaleway_loki: Option<ScalewayLokiLoggingOutput>,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_console_logging_output() -> Option<ConsoleLoggingOutput> {
    Some(ConsoleLoggingOutput::default())
}

fn default_file_logging_output() -> Option<FileLoggingOutput> {
    Some(FileLoggingOutput::default())
}

fn default_scaleway_loki_logging_output() -> Option<ScalewayLokiLoggingOutput> {
    Some(ScalewayLokiLoggingOutput::default())
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            filter: None,
            console: default_console_logging_output(),
            file: default_file_logging_output(),
            scaleway_loki: default_scaleway_loki_logging_output(),
        }
    }
}

/// Command retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRetryConfig {
    /// Maximum number of retry attempts
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
    /// Base delay between retries in milliseconds
    #[serde(default = "default_base_delay_ms")]
    pub base_delay_ms: u64,
    /// Maximum delay between retries in milliseconds
    #[serde(default = "default_max_delay_ms")]
    pub max_delay_ms: u64,
    /// Backoff multiplier
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,
    /// Whether to use jitter
    #[serde(default = "default_use_jitter")]
    pub use_jitter: bool,
}

const fn default_max_attempts() -> u32 {
    3
}

const fn default_base_delay_ms() -> u64 {
    100
}

const fn default_max_delay_ms() -> u64 {
    30000
}

const fn default_backoff_multiplier() -> f64 {
    2.0
}

const fn default_use_jitter() -> bool {
    true
}

impl Default for CommandRetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: default_max_attempts(),
            base_delay_ms: default_base_delay_ms(),
            max_delay_ms: default_max_delay_ms(),
            backoff_multiplier: default_backoff_multiplier(),
            use_jitter: default_use_jitter(),
        }
    }
}

/// Command configuration with retry settings and command-specific overrides
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommandConfig {
    /// Default retry configuration for all commands
    #[serde(default)]
    pub retry: CommandRetryConfig,
    /// Command-specific retry configurations
    #[serde(default)]
    pub overrides: HashMap<String, CommandRetryConfig>,
}

impl CommandConfig {
    /// Get retry configuration for a specific command
    /// Returns command-specific config if available, otherwise returns default
    #[must_use]
    pub fn get_retry_config(&self, command_type: &str) -> &CommandRetryConfig {
        self.overrides.get(command_type).unwrap_or(&self.retry)
    }
}

/// SQS configuration for event publishing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqsConfig {
    /// AWS region
    #[serde(default = "default_sqs_region")]
    pub region: String,
    /// AWS account ID (required for building queue URLs)
    #[serde(default = "default_sqs_account_id")]
    pub account_id: String,
    /// Destination queue names for different event types.
    #[serde(default = "default_sqs_queues")]
    pub queues: HashMap<String, Vec<String>>,
    /// Fallback destination queues when no specific queue is configured for an event type.
    #[serde(default = "default_sqs_default_queues")]
    pub default_queues: Vec<String>,
    /// AWS access key ID (optional, can use IAM roles or environment variables)
    #[serde(default)]
    pub access_key_id: Option<String>,
    /// AWS secret access key (optional, can use IAM roles or environment variables)
    #[serde(default)]
    pub secret_access_key: Option<String>,
    /// AWS session token (optional, for temporary credentials)
    #[serde(default)]
    pub session_token: Option<String>,
    /// Custom endpoint host (for `LocalStack` or custom SQS implementations)
    #[serde(default = "default_sqs_host")]
    pub host: String,
    /// Custom endpoint port (for `LocalStack` or custom SQS implementations, 0 for random port)
    #[serde(default = "default_sqs_port")]
    pub port: u16,
    /// Custom endpoint URL (for `LocalStack` or custom SQS implementations) - deprecated, use host/port instead
    #[serde(default)]
    pub endpoint_url: Option<String>,
    /// Whether to enable SQS (for testing/development flexibility)
    #[serde(default = "default_sqs_enabled")]
    pub enabled: bool,
    /// Maximum number of retries for failed messages
    #[serde(default = "default_sqs_max_retries")]
    pub max_retries: u32,
    /// Message timeout in seconds
    #[serde(default = "default_sqs_timeout_seconds")]
    pub timeout_seconds: u64,
}

impl SqsConfig {
    /// Check if a queue is a FIFO queue based on queue name
    #[must_use]
    pub fn is_fifo_queue(&self, queue_name: &str) -> bool {
        std::path::Path::new(queue_name)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("fifo"))
    }

    /// Get destination queue names for a specific event type, falling back to default queues.
    #[must_use]
    pub fn get_queue_names(&self, event_type: &str) -> Vec<&str> {
        let queue_names = self
            .queues
            .get(event_type)
            .filter(|queues| !queues.is_empty())
            .unwrap_or(&self.default_queues);

        let mut seen = HashSet::new();
        let mut result = Vec::new();
        for queue_name in queue_names {
            if seen.insert(queue_name.as_str()) {
                result.push(queue_name.as_str());
            }
        }
        result
    }

    /// Build the full queue URL for a given queue name.
    #[must_use]
    pub fn queue_url(&self, queue_name: &str) -> String {
        if self.host == "localhost" || self.host == "localstack" {
            // For LocalStack or custom endpoint
            format!(
                "http://{}:{}/000000000000/{}",
                self.host,
                self.actual_port(),
                queue_name
            )
        } else {
            // For AWS
            format!(
                "https://sqs.{}.scaleway.com/{}/{}",
                self.region, self.account_id, queue_name
            )
        }
    }

    /// Get all destination queue URLs for a specific event type.
    #[must_use]
    pub fn get_queue_urls(&self, event_type: &str) -> Vec<String> {
        self.get_queue_names(event_type)
            .into_iter()
            .map(|queue_name| self.queue_url(queue_name))
            .collect()
    }

    /// Get every configured physical queue name.
    #[must_use]
    pub fn all_queue_names(&self) -> HashSet<String> {
        let mut all_queues = HashSet::new();

        for queue_name in &self.default_queues {
            all_queues.insert(queue_name.clone());
        }

        for queue_names in self.queues.values() {
            for queue_name in queue_names {
                all_queues.insert(queue_name.clone());
            }
        }

        all_queues
    }

    /// Get every configured physical queue URL.
    #[must_use]
    pub fn all_queue_urls(&self) -> Vec<String> {
        self.all_queue_names()
            .into_iter()
            .map(|queue_name| self.queue_url(&queue_name))
            .collect()
    }

    /// Get a random available port
    fn get_random_port() -> u16 {
        use std::net::{SocketAddr, TcpListener};

        // Try to bind to a random port
        match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => {
                match listener.local_addr() {
                    Ok(SocketAddr::V4(addr)) => addr.port(),
                    Ok(SocketAddr::V6(addr)) => addr.port(),
                    Err(_) => 4566, // fallback to LocalStack default
                }
            }
            Err(_) => 4566, // fallback to LocalStack default
        }
    }

    /// Get the actual port being used (resolves random port if needed)
    /// This method caches the resolved port to ensure consistency across calls
    pub fn actual_port(&self) -> u16 {
        if self.port == 0 {
            // Create a unique cache key for this SQS configuration
            let cache_key = format!("sqs:{}:{}", self.host, self.region);

            let cache = PORT_CACHE.get_or_init(|| Arc::new(Mutex::new(HashMap::new())));
            let mut port_cache = cache.lock().unwrap();

            // Return cached port if available
            if let Some(&cached_port) = port_cache.get(&cache_key) {
                debug!("Using cached SQS port: {}", cached_port);
                return cached_port;
            }

            // Generate new random port and cache it
            let random_port = Self::get_random_port();
            port_cache.insert(cache_key, random_port);

            debug!("Generated random SQS port: {}", random_port);
            random_port
        } else {
            debug!("Using SQS port from config: {}", self.port);
            self.port
        }
    }

    /// Get the endpoint URL for SQS (constructs from host/port or uses legacy `endpoint_url`)
    #[must_use]
    pub fn endpoint_url(&self) -> Option<String> {
        // If legacy endpoint_url is provided, use it
        if let Some(ref url) = self.endpoint_url {
            return Some(url.clone());
        }

        // If host is localhost or localstack, construct URL from host/port
        if self.host == "localhost" || self.host == "localstack" {
            let port = self.actual_port();
            Some(format!("http://{}:{}", self.host, port))
        } else {
            // For non-localhost/localstack hosts, assume it's AWS (no custom endpoint needed)
            None
        }
    }

    /// Create a new `SqsConfig` with the specified components
    #[must_use]
    pub fn new(
        region: String,
        account_id: String,
        queues: HashMap<String, Vec<String>>,
        default_queues: Vec<String>,
    ) -> Self {
        Self {
            region,
            account_id,
            queues,
            default_queues,
            access_key_id: None,
            secret_access_key: None,
            session_token: None,
            host: default_sqs_host(),
            port: default_sqs_port(),
            endpoint_url: None,
            enabled: default_sqs_enabled(),
            max_retries: default_sqs_max_retries(),
            timeout_seconds: default_sqs_timeout_seconds(),
        }
    }

    /// Clear the port cache for SQS
    pub fn clear_port_cache() {
        if let Some(cache) = PORT_CACHE.get() {
            let mut port_cache = cache.lock().unwrap();
            // Create a unique cache key for this SQS configuration
            port_cache.remove(&"sqs".to_string());
            debug!("SQS port cleared from cache");
        }
    }
}

impl Default for SqsConfig {
    fn default() -> Self {
        Self {
            region: default_sqs_region(),
            account_id: default_sqs_account_id(),
            queues: default_sqs_queues(),
            default_queues: default_sqs_default_queues(),
            access_key_id: None,
            secret_access_key: None,
            session_token: None,
            host: default_sqs_host(),
            port: default_sqs_port(), // Use random port for testing by default
            endpoint_url: None,
            enabled: default_sqs_enabled(),
            max_retries: default_sqs_max_retries(),
            timeout_seconds: default_sqs_timeout_seconds(),
        }
    }
}

// SQS configuration defaults
fn default_sqs_region() -> String {
    "us-east-1".to_string()
}

fn default_sqs_account_id() -> String {
    "123456789012".to_string()
}

fn default_sqs_queues() -> HashMap<String, Vec<String>> {
    HashMap::new()
}

fn default_sqs_default_queues() -> Vec<String> {
    vec!["user-events".to_string()]
}

fn default_sqs_host() -> String {
    "localhost".to_string()
}

const fn default_sqs_port() -> u16 {
    4566 // LocalStack SQS default port
}

const fn default_sqs_enabled() -> bool {
    true
}

const fn default_sqs_max_retries() -> u32 {
    3
}

const fn default_sqs_timeout_seconds() -> u64 {
    30
}

/// `OpenFGA` HTTP client configuration shared by services that use
/// `rustycog-permission` and by writers such as `sentinel-sync`.
///
/// `host` / `port` are split deliberately, matching [`DatabaseConfig`] and
/// [`SqsConfig`]. In tests, `port = 0` asks [`Self::actual_port`] to resolve a
/// free random host port and cache it so the `OpenFGA` testcontainer fixture and
/// the app boot path agree on the same port.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenFgaClientConfig {
    /// URL scheme for the `OpenFGA` HTTP API.
    #[serde(default = "default_openfga_scheme")]
    pub scheme: String,
    /// Hostname or IP for the `OpenFGA` HTTP API.
    #[serde(default = "default_openfga_host")]
    pub host: String,
    /// Port for the `OpenFGA` HTTP API. Use `0` in test configs for a random
    /// free host port resolved through [`Self::actual_port`].
    #[serde(default = "default_openfga_port")]
    pub port: u16,
    #[serde(default)]
    pub store_id: String,
    #[serde(default)]
    pub authorization_model_id: Option<String>,
    #[serde(default)]
    pub api_token: Option<String>,
    /// Cache TTL (seconds) applied by permission-checker cache decorators.
    /// `None` keeps the production default; `Some(0)` disables the cache in
    /// integration tests that exercise grant / revoke / deny flows.
    #[serde(default)]
    pub cache_ttl_seconds: Option<u64>,
}

impl OpenFgaClientConfig {
    /// Build the API base URL from `scheme`, `host`, and the resolved port.
    #[must_use]
    pub fn api_url(&self) -> String {
        format!(
            "{}://{}:{}",
            self.scheme.trim_end_matches("://"),
            self.host,
            self.actual_port()
        )
    }

    /// Resolve the configured port. When `port == 0`, picks a random free port
    /// once for this host and caches it for the rest of the process.
    pub fn actual_port(&self) -> u16 {
        if self.port == 0 {
            let cache_key = format!("openfga:{}", self.host);
            let cache = PORT_CACHE.get_or_init(|| Arc::new(Mutex::new(HashMap::new())));
            let mut port_cache = cache.lock().unwrap();

            if let Some(&cached_port) = port_cache.get(&cache_key) {
                debug!("Using cached OpenFGA port: {}", cached_port);
                return cached_port;
            }

            let random_port = Self::get_random_port();
            port_cache.insert(cache_key, random_port);
            debug!("Generated random OpenFGA port: {}", random_port);
            random_port
        } else {
            self.port
        }
    }

    fn get_random_port() -> u16 {
        use std::net::{SocketAddr, TcpListener};

        match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => match listener.local_addr() {
                Ok(SocketAddr::V4(addr)) => addr.port(),
                Ok(SocketAddr::V6(addr)) => addr.port(),
                Err(_) => default_openfga_port(),
            },
            Err(_) => default_openfga_port(),
        }
    }

    /// Clear cached random `OpenFGA` ports so the next container start gets a
    /// fresh host port.
    pub fn clear_port_cache() {
        if let Some(cache) = PORT_CACHE.get() {
            let mut port_cache = cache.lock().unwrap();
            port_cache.retain(|key, _| !key.starts_with("openfga:"));
            debug!("OpenFGA port cleared from cache");
        }
    }
}

impl Default for OpenFgaClientConfig {
    fn default() -> Self {
        Self {
            scheme: default_openfga_scheme(),
            host: default_openfga_host(),
            port: default_openfga_port(),
            store_id: String::new(),
            authorization_model_id: None,
            api_token: None,
            cache_ttl_seconds: None,
        }
    }
}

fn default_openfga_scheme() -> String {
    "http".to_string()
}

fn default_openfga_host() -> String {
    "localhost".to_string()
}

const fn default_openfga_port() -> u16 {
    8090
}

/// Event queue configuration - can be either Kafka or SQS
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum QueueConfig {
    #[serde(rename = "kafka")]
    Kafka(KafkaConfig),
    #[serde(rename = "sqs")]
    Sqs(SqsConfig),
    #[serde(rename = "disabled")]
    Disabled,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self::Kafka(KafkaConfig::default())
    }
}

impl QueueConfig {
    /// Check if queue is enabled
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        match self {
            Self::Kafka(config) => config.enabled,
            Self::Sqs(config) => config.enabled,
            Self::Disabled => false,
        }
    }
}

/// Kafka configuration for event publishing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KafkaConfig {
    /// Kafka broker host
    #[serde(default = "default_kafka_host")]
    pub host: String,
    /// Kafka broker port (9092 default, 0 for random port)
    #[serde(default = "default_kafka_port")]
    pub port: u16,
    /// Topic for user events
    #[serde(default = "default_user_events_topic")]
    pub user_events_topic: String,
    /// Producer client ID
    #[serde(default = "default_kafka_client_id")]
    pub client_id: String,
    /// Message timeout in milliseconds
    #[serde(default = "default_kafka_timeout_ms")]
    pub timeout_ms: u64,
    /// Maximum number of retries for failed messages
    #[serde(default = "default_kafka_max_retries")]
    pub max_retries: u32,
    /// Whether to enable Kafka (for testing/development flexibility)
    #[serde(default = "default_kafka_enabled")]
    pub enabled: bool,
    /// Compression type for messages (none, gzip, snappy, lz4, zstd)
    #[serde(default = "default_kafka_compression")]
    pub compression: String,
    /// Security protocol (plaintext, ssl, `sasl_plaintext`, `sasl_ssl`)
    #[serde(default = "default_kafka_security_protocol")]
    pub security_protocol: String,
    /// SASL mechanism (PLAIN, SCRAM-SHA-256, SCRAM-SHA-512, GSSAPI, OAUTHBEARER)
    #[serde(default)]
    pub sasl_mechanism: Option<String>,
    /// SASL username
    #[serde(default)]
    pub sasl_username: Option<String>,
    /// SASL password
    #[serde(default)]
    pub sasl_password: Option<String>,
    /// SSL CA certificate location (use "probe" for system CA certificates)
    #[serde(default)]
    pub ssl_ca_location: Option<String>,
    /// SSL certificate location for client authentication
    #[serde(default)]
    pub ssl_certificate_location: Option<String>,
    /// SSL private key location for client authentication
    #[serde(default)]
    pub ssl_key_location: Option<String>,
    /// SSL private key password
    #[serde(default)]
    pub ssl_key_password: Option<String>,
    /// Additional broker hosts (for multi-broker setups) - backward compatibility
    #[serde(default)]
    pub additional_brokers: Vec<String>,
}

impl KafkaConfig {
    /// Get the brokers string for Kafka client configuration
    #[must_use]
    pub fn brokers(&self) -> String {
        let port = self.actual_port();
        let primary_broker = format!("{}:{}", self.host, port);

        if self.additional_brokers.is_empty() {
            primary_broker
        } else {
            let mut all_brokers = vec![primary_broker];
            all_brokers.extend(self.additional_brokers.clone());
            all_brokers.join(",")
        }
    }

    /// Get a random available port
    fn get_random_port() -> u16 {
        use std::net::TcpListener;

        // Try to bind to a random port
        match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => {
                match listener.local_addr() {
                    Ok(addr) => addr.port(),
                    Err(_) => 9092, // fallback to default
                }
            }
            Err(_) => 9092, // fallback to default
        }
    }

    /// Get the actual port being used (resolves random port if needed)
    /// This method caches the resolved port to ensure consistency across calls
    pub fn actual_port(&self) -> u16 {
        if self.port == 0 {
            // Create a unique cache key for this Kafka configuration
            let cache_key = format!("kafka:{}:{}", self.host, self.client_id);

            let cache = PORT_CACHE.get_or_init(|| Arc::new(Mutex::new(HashMap::new())));
            let mut port_cache = cache.lock().unwrap();

            // Return cached port if available
            if let Some(&cached_port) = port_cache.get(&cache_key) {
                debug!("cached_port: {}", cached_port);
                return cached_port;
            }

            // Generate new random port and cache it
            let random_port = Self::get_random_port();
            port_cache.insert(cache_key, random_port);
            random_port
        } else {
            self.port
        }
    }

    /// Create a new `KafkaConfig` with the specified components
    #[must_use]
    pub fn new(host: String, port: u16, user_events_topic: String, client_id: String) -> Self {
        Self {
            host,
            port,
            user_events_topic,
            client_id,
            timeout_ms: default_kafka_timeout_ms(),
            max_retries: default_kafka_max_retries(),
            enabled: default_kafka_enabled(),
            compression: default_kafka_compression(),
            security_protocol: default_kafka_security_protocol(),
            sasl_mechanism: None,
            sasl_username: None,
            sasl_password: None,
            ssl_ca_location: None,
            ssl_certificate_location: None,
            ssl_key_location: None,
            ssl_key_password: None,
            additional_brokers: vec![],
        }
    }

    /// Create a `KafkaConfig` from a brokers string (for backward compatibility)
    pub fn from_brokers(brokers: &str) -> Result<Self, String> {
        let broker_list: Vec<&str> = brokers.split(',').collect();
        if broker_list.is_empty() {
            return Err("Brokers string cannot be empty".to_string());
        }

        // Parse the first broker as primary
        let primary_broker = broker_list[0].trim();
        let parts: Vec<&str> = primary_broker.split(':').collect();

        if parts.len() != 2 {
            return Err(format!(
                "Invalid broker format '{primary_broker}', expected 'host:port'"
            ));
        }

        let host = parts[0].to_string();
        let port = parts[1]
            .parse::<u16>()
            .map_err(|_| format!("Invalid port in broker '{primary_broker}'"))?;

        // Handle additional brokers
        let additional_brokers = if broker_list.len() > 1 {
            broker_list[1..]
                .iter()
                .map(|b| b.trim().to_string())
                .collect()
        } else {
            vec![]
        };

        Ok(Self {
            host,
            port,
            user_events_topic: default_user_events_topic(),
            client_id: default_kafka_client_id(),
            timeout_ms: default_kafka_timeout_ms(),
            max_retries: default_kafka_max_retries(),
            enabled: default_kafka_enabled(),
            compression: default_kafka_compression(),
            security_protocol: default_kafka_security_protocol(),
            sasl_mechanism: None,
            sasl_username: None,
            sasl_password: None,
            ssl_ca_location: None,
            ssl_certificate_location: None,
            ssl_key_location: None,
            ssl_key_password: None,
            additional_brokers,
        })
    }

    /// Clear the port cache (useful for testing)
    pub fn clear_port_cache() {
        if let Some(cache) = PORT_CACHE.get() {
            let mut port_cache = cache.lock().unwrap();
            // Create a unique cache key for this Kafka configuration
            port_cache.remove(&"kafka".to_string());
            debug!("Kafka port cleared from cache");
        }
    }
}

impl Default for KafkaConfig {
    fn default() -> Self {
        Self {
            host: default_kafka_host(),
            port: default_kafka_port(),
            user_events_topic: default_user_events_topic(),
            client_id: default_kafka_client_id(),
            timeout_ms: default_kafka_timeout_ms(),
            max_retries: default_kafka_max_retries(),
            enabled: default_kafka_enabled(),
            compression: default_kafka_compression(),
            security_protocol: default_kafka_security_protocol(),
            sasl_mechanism: None,
            sasl_username: None,
            sasl_password: None,
            ssl_ca_location: None,
            ssl_certificate_location: None,
            ssl_key_location: None,
            ssl_key_password: None,
            additional_brokers: vec![],
        }
    }
}

// Kafka configuration defaults
fn default_kafka_host() -> String {
    "localhost".to_string()
}

const fn default_kafka_port() -> u16 {
    9092
}

fn default_user_events_topic() -> String {
    "user-events".to_string()
}

fn default_kafka_client_id() -> String {
    "rustycog-service".to_string()
}

const fn default_kafka_timeout_ms() -> u64 {
    5000
}

const fn default_kafka_max_retries() -> u32 {
    3
}

const fn default_kafka_enabled() -> bool {
    true
}

fn default_kafka_compression() -> String {
    "gzip".to_string()
}

fn default_kafka_security_protocol() -> String {
    "plaintext".to_string()
}

/// Generic configuration cache and loading functionality
/// This allows any service to implement their own configuration structure
/// while using the same caching and loading logic.

/// Configuration cache trait that services must implement
pub trait ConfigCache<T> {
    /// Get the cached configuration if available
    fn get_cached() -> Option<T>;
    /// Set the cached configuration
    fn set_cached(config: T);
    /// Clear the cached configuration
    fn clear_cached();
}

/// Generic configuration loader
pub trait ConfigLoader<T>: Default + for<'de> Deserialize<'de> + Serialize + Clone {
    /// Create a default configuration instance
    fn create_default() -> T;
    /// Get the configuration prefix for environment variables (e.g., "IAM" for IAM_*)
    fn config_prefix() -> &'static str;
}

pub trait HasDbConfig {
    fn db_config(&self) -> &DatabaseConfig;
    fn set_db_config(&mut self, config: DatabaseConfig);
}

pub trait HasQueueConfig {
    fn queue_config(&self) -> &QueueConfig;
    fn set_queue_config(&mut self, config: QueueConfig);
}

pub trait HasServerConfig {
    fn server_config(&self) -> &ServerConfig;
    fn set_server_config(&mut self, config: ServerConfig);
}

pub trait HasLoggingConfig {
    fn logging_config(&self) -> &LoggingConfig;
    fn set_logging_config(&mut self, config: LoggingConfig);
}

pub trait HasScalewayConfig {
    fn scaleway_config(&self) -> &ScalewayConfig;
    fn set_scaleway_config(&mut self, config: ScalewayConfig);
}

pub trait HasOpenFgaConfig {
    fn openfga_config(&self) -> &OpenFgaClientConfig;
    fn set_openfga_config(&mut self, config: OpenFgaClientConfig);
}

/// Load configuration with caching
pub fn load_config_with_cache<T, C>() -> Result<T, ConfigError>
where
    T: ConfigLoader<T>,
    C: ConfigCache<T>,
{
    // Return cached config if available
    if let Some(config) = C::get_cached() {
        tracing::debug!("Returning cached configuration");
        return Ok(config);
    }

    // Load fresh configuration
    let config = load_config_fresh::<T>()?;

    // Cache the configuration
    C::set_cached(config.clone());
    tracing::debug!("Configuration loaded and cached");

    Ok(config)
}

/// Load fresh configuration without caching
pub fn load_config_fresh<T>() -> Result<T, ConfigError>
where
    T: ConfigLoader<T>,
{
    let config = build_config_with_env_prefix(T::config_prefix())?;

    // Try to deserialize to the target type
    match config.try_deserialize::<T>() {
        Ok(app_config) => {
            tracing::info!("Configuration loaded successfully");
            Ok(app_config)
        }
        Err(e) => {
            tracing::error!("Failed to deserialize configuration: {}", e);
            tracing::info!("Falling back to default configuration");
            Ok(T::create_default())
        }
    }
}

/// Helper function to build configuration with environment prefix
/// Extracts common configuration loading logic
fn build_config_with_env_prefix(env_prefix: &str) -> Result<Config, ConfigError> {
    use std::env;
    use std::path::Path;

    // Load .env file if it exists
    let _ = dotenv().ok();

    // Get environment
    let env = env::var("RUN_ENV").unwrap_or_else(|_| "development".to_string());

    // Determine the configuration file to use
    let config_file = match env.as_str() {
        "test" => "config/test.toml",
        "production" => "config/production.toml",
        _ => "config/development.toml",
    };

    tracing::info!("Loading configuration from environment: {}", env);
    tracing::debug!("Configuration file: {}", config_file);

    let mut builder = Config::builder();

    // Load base configuration file if it exists
    if Path::new(config_file).exists() {
        tracing::debug!("Loading configuration file: {}", config_file);
        builder = builder.add_source(File::with_name(config_file).format(FileFormat::Toml));
    } else {
        tracing::warn!(
            "Configuration file not found: {}, using defaults",
            config_file
        );
    }

    // Load environment-specific configuration if different from base
    if env != "development" {
        let env_config_path = format!("config/{env}.toml");
        if Path::new(&env_config_path).exists() && env_config_path != config_file {
            tracing::debug!("Loading environment configuration from {}", env_config_path);
            builder =
                builder.add_source(File::with_name(&env_config_path).format(FileFormat::Toml));
        }
    }

    // Add environment variable overrides with specified prefix
    tracing::debug!("Loading environment variables with prefix: {}_", env_prefix);
    builder = builder.add_source(
        Environment::with_prefix(env_prefix)
            .prefix_separator("_")
            .separator("__")
            .try_parsing(true),
    );

    // Build and return configuration
    builder.build()
}

/// Load a specific configuration part (server, database, logging, queue, etc.)
/// This is useful when you only need a specific part of the configuration
/// rather than loading the entire application config.
pub fn load_config_part<T>(section_name: &str) -> Result<T, ConfigError>
where
    T: for<'de> Deserialize<'de> + Default + Clone,
{
    // Use uppercase section name as environment prefix
    let env_prefix = section_name.to_uppercase();
    let config = build_config_with_env_prefix(&env_prefix)?;

    tracing::info!("Loading {} configuration", section_name);

    // Try to deserialize the specific section
    match config.get::<T>(section_name) {
        Ok(parsed_config) => {
            tracing::info!("{} configuration loaded successfully", section_name);
            Ok(parsed_config)
        }
        Err(_) => {
            // Section not found or failed to parse, try to deserialize the entire config as the target type
            // This handles cases where the config part is at the root level
            match config.try_deserialize::<T>() {
                Ok(parsed_config) => {
                    tracing::info!(
                        "{} configuration loaded successfully from root",
                        section_name
                    );
                    Ok(parsed_config)
                }
                Err(e) => {
                    tracing::warn!("Failed to load {} configuration: {}", section_name, e);
                    tracing::info!("Using default {} configuration", section_name);
                    Ok(T::default())
                }
            }
        }
    }
}

/// Clear all configuration caches
/// This is useful for testing to ensure fresh configuration loading
pub fn clear_all_caches() {
    DatabaseConfig::clear_port_cache();
    KafkaConfig::clear_port_cache();
    SqsConfig::clear_port_cache();
    OpenFgaClientConfig::clear_port_cache();
    println!("All configuration caches cleared");
}

/// Convenience functions for loading specific configuration parts

/// Load server configuration
pub fn load_server_config() -> Result<ServerConfig, ConfigError> {
    load_config_part::<ServerConfig>("server")
}

/// Load database configuration  
pub fn load_database_config() -> Result<DatabaseConfig, ConfigError> {
    load_config_part::<DatabaseConfig>("database")
}

/// Load logging configuration
pub fn load_logging_config() -> Result<LoggingConfig, ConfigError> {
    load_config_part::<LoggingConfig>("logging")
}

/// Load command configuration
pub fn load_command_config() -> Result<CommandConfig, ConfigError> {
    load_config_part::<CommandConfig>("command")
}

/// Load queue configuration
pub fn load_queue_config() -> Result<QueueConfig, ConfigError> {
    load_config_part::<QueueConfig>("queue")
}

/// Load Kafka configuration
pub fn load_kafka_config() -> Result<KafkaConfig, ConfigError> {
    load_config_part::<KafkaConfig>("kafka")
}

/// Load SQS configuration
pub fn load_sqs_config() -> Result<SqsConfig, ConfigError> {
    load_config_part::<SqsConfig>("sqs")
}

/// Load `OpenFGA` configuration
pub fn load_openfga_config() -> Result<OpenFgaClientConfig, ConfigError> {
    load_config_part::<OpenFgaClientConfig>("openfga")
}

/// Generate a default configuration file in TOML format
pub fn generate_default_config_toml<T>() -> Result<String, ConfigError>
where
    T: ConfigLoader<T>,
{
    let default_config = T::create_default();
    toml::to_string_pretty(&default_config)
        .map_err(|e| ConfigError::Message(format!("Failed to serialize default config: {e}")))
}

#[cfg(test)]
mod tests {
    use super::SqsConfig;
    use std::collections::HashMap;

    #[test]
    fn sqs_config_returns_event_specific_destination_queues() {
        let mut config = SqsConfig::default();
        config.queues.insert(
            "user_signed_up".to_string(),
            vec!["telegraph-events".to_string()],
        );

        assert_eq!(
            config.get_queue_names("user_signed_up"),
            vec!["telegraph-events"]
        );
    }

    #[test]
    fn sqs_config_falls_back_to_default_destination_queues() {
        let config = SqsConfig {
            default_queues: vec![
                "telegraph-events".to_string(),
                "sentinel-sync-events".to_string(),
            ],
            ..SqsConfig::default()
        };

        assert_eq!(
            config.get_queue_names("unknown_event"),
            vec!["telegraph-events", "sentinel-sync-events"]
        );
    }

    #[test]
    fn sqs_config_deduplicates_event_destinations_preserving_order() {
        let mut queues = HashMap::new();
        queues.insert(
            "user_signed_up".to_string(),
            vec![
                "telegraph-events".to_string(),
                "telegraph-events".to_string(),
                "sentinel-sync-events".to_string(),
            ],
        );

        let config = SqsConfig {
            queues,
            ..SqsConfig::default()
        };

        assert_eq!(
            config.get_queue_names("user_signed_up"),
            vec!["telegraph-events", "sentinel-sync-events"]
        );
    }

    #[test]
    fn sqs_config_all_queue_names_includes_defaults_and_event_destinations() {
        let mut queues = HashMap::new();
        queues.insert(
            "user_signed_up".to_string(),
            vec![
                "telegraph-events".to_string(),
                "sentinel-sync-events".to_string(),
            ],
        );
        queues.insert(
            "password_reset_requested".to_string(),
            vec!["telegraph-events".to_string()],
        );

        let config = SqsConfig {
            queues,
            default_queues: vec!["fallback-events".to_string()],
            ..SqsConfig::default()
        };

        let all_queue_names = config.all_queue_names();
        assert!(all_queue_names.contains("fallback-events"));
        assert!(all_queue_names.contains("telegraph-events"));
        assert!(all_queue_names.contains("sentinel-sync-events"));
        assert_eq!(all_queue_names.len(), 3);
    }

    #[test]
    fn sqs_config_builds_queue_urls_from_queue_names() {
        let config = SqsConfig {
            host: "localhost".to_string(),
            port: 4566,
            ..SqsConfig::default()
        };

        assert_eq!(
            config.queue_url("telegraph-events"),
            "http://localhost:4566/000000000000/telegraph-events"
        );
    }
}
