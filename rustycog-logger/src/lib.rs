use rustycog_config::HasLoggingConfig;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[cfg(feature = "scaleway-loki")]
use anyhow::Context;
#[cfg(feature = "scaleway-loki")]
use rustycog_config::{HasScalewayConfig, ScalewayLokiLoggingOutput};
#[cfg(feature = "scaleway-loki")]
use std::env;

#[cfg(feature = "scaleway-loki")]
fn build_scaleway_loki_stack<C: ServiceLoggerConfig>(
    config: &C,
    scaleway_loki: ScalewayLokiLoggingOutput,
) -> anyhow::Result<(tracing_loki::Layer, tracing_loki::BackgroundTask)> {
    let loki_endpoint = format!(
        "https://{}.logs.cockpit.{}.scw.cloud",
        scaleway_loki.datasource_uuid,
        config.scaleway_config().region
    );

    let url = tracing_loki::url::Url::parse(&loki_endpoint)
        .context("parse Scaleway Loki endpoint URL")?;

    tracing_loki::builder()
        .label(
            "job",
            env::var("JOB").unwrap_or_else(|_| "unknown".to_string()),
        )
        .map_err(anyhow::Error::from)?
        .label(
            "service",
            env::var("SERVICE").unwrap_or_else(|_| "unknown".to_string()),
        )
        .map_err(anyhow::Error::from)?
        .http_header(
            "Authorization",
            format!("Bearer {}", scaleway_loki.cockpit_token),
        )
        .map_err(anyhow::Error::from)?
        .build_url(url)
        .map_err(anyhow::Error::from)
}

#[cfg(feature = "scaleway-loki")]
pub trait ServiceLoggerConfig: HasLoggingConfig + HasScalewayConfig {}
#[cfg(feature = "scaleway-loki")]
impl<T: HasLoggingConfig + HasScalewayConfig> ServiceLoggerConfig for T {}

#[cfg(not(feature = "scaleway-loki"))]
pub trait ServiceLoggerConfig: HasLoggingConfig {}
#[cfg(not(feature = "scaleway-loki"))]
impl<T: HasLoggingConfig> ServiceLoggerConfig for T {}

/// Setup logging based on configuration
pub fn setup_logging<C: ServiceLoggerConfig>(config: &C) {
    let level_directive = match config.logging_config().level.to_lowercase().as_str() {
        "trace" => "trace",
        "debug" => "debug",
        "info" => "info",
        "warn" => "warn",
        "error" => "error",
        _ => "info",
    };
    let level_fallback = level_directive.to_string();
    let env_filter = config
        .logging_config()
        .filter
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(EnvFilter::new)
        .or_else(|| EnvFilter::try_from_default_env().ok())
        .unwrap_or_else(|| EnvFilter::new(level_fallback));

    let console_layer = tracing_subscriber::fmt::layer()
        .with_line_number(true)
        .with_target(true)
        .with_thread_names(true);

    #[cfg(feature = "scaleway-loki")]
    let (loki_layer, loki_task) =
        if let Some(scaleway_loki) = config.logging_config().scaleway_loki.clone() {
            match build_scaleway_loki_stack(config, scaleway_loki) {
                Ok((layer, task)) => (Some(layer), Some(task)),
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        "failed to initialize Scaleway Loki exporter; continuing without Loki"
                    );
                    (None, None)
                }
            }
        } else {
            (None, None)
        };

    #[cfg(not(feature = "scaleway-loki"))]
    let loki_layer: Option<tracing_subscriber::fmt::Layer<_>> = None;

    // Use try_init() to avoid panicking if subscriber is already initialized
    // This is especially important during testing where setup_logging might be called multiple times
    let _ = tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer)
        .with(loki_layer)
        .try_init();

    #[cfg(feature = "scaleway-loki")]
    {
        if let Some(loki_task) = loki_task {
            // Spawn the Loki background task
            tokio::spawn(loki_task);
        }
    }
}
