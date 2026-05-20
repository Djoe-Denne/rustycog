use crate::common::ServiceTestDescriptor;
use rustycog_config::{load_config_fresh, HasLoggingConfig, HasServerConfig, ServerConfig};
use rustycog_logger::setup_logging;
use std::sync::Arc;
use tracing::debug;

pub async fn build_test_app<D, T>(descriptor: Arc<D>) -> anyhow::Result<()>
where
    D: ServiceTestDescriptor<T>,
    T: Send + Sync + 'static,
{
    let config = load_config_fresh::<D::Config>().expect("failed to load config");
    debug!("🔄 Building test app with configuration:");
    descriptor.build_app(config, ServerConfig::default()).await
}

pub async fn spawn_test_server<D, T>(descriptor: Arc<D>) -> anyhow::Result<()>
where
    D: ServiceTestDescriptor<T>,
    T: Send + Sync + 'static,
{
    // Use your real config loading logic
    let config = load_config_fresh::<D::Config>().expect("failed to load config");

    // Initialize logging for the test server
    if !config.logging_config().level.is_empty() {
        setup_logging(&config);
    }

    debug!("🚀 Starting test server with configuration:");
    debug!("   Server host: {}", config.server_config().host);
    debug!("   Server port: {}", config.server_config().port);
    debug!("   TLS enabled: {}", config.server_config().tls_enabled);

    // Create server configuration
    let server_config = ServerConfig {
        host: config.server_config().host.clone(),
        port: config.server_config().port,
        tls_enabled: config.server_config().tls_enabled,
        tls_cert_path: if config.server_config().tls_enabled {
            config.server_config().tls_cert_path.clone()
        } else {
            String::new()
        },
        tls_key_path: if config.server_config().tls_enabled {
            config.server_config().tls_key_path.clone()
        } else {
            String::new()
        },
        tls_port: if config.server_config().tls_enabled {
            config.server_config().tls_port
        } else {
            0
        },
    };

    debug!(
        "🌐 Test server will listen on: http://{}:{}",
        config.server_config().host,
        config.server_config().port
    );

    // Build and run the application - this should run indefinitely
    descriptor.run_app(config, server_config).await
}
