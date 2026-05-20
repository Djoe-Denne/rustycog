use super::{Command, CommandContext, CommandError, CommandHandler, CommandMetrics};
use async_trait::async_trait;
use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tracing::{error, info, warn};

/// Retry policy configuration
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub backoff_multiplier: f64,
    pub use_jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            use_jitter: true,
        }
    }
}

impl RetryPolicy {
    #[must_use]
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        let base_delay_ms = self.base_delay.as_millis() as f64;
        let exponential_delay = base_delay_ms * self.backoff_multiplier.powi(attempt as i32);

        let mut delay = Duration::from_millis(exponential_delay as u64);
        if delay > self.max_delay {
            delay = self.max_delay;
        }

        if self.use_jitter {
            // Simple jitter using system time - not cryptographically secure but sufficient for retry jitter
            let time_nanos = f64::from(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_nanos(),
            );
            let jitter = (time_nanos / 1_000_000_000.0 - 0.5) * 0.1; // ±5% jitter
            let jitter_factor = 1.0 + jitter;
            delay = Duration::from_millis((delay.as_millis() as f64 * jitter_factor) as u64);
        }

        delay
    }

    #[must_use]
    pub const fn is_retryable(&self, error: &CommandError) -> bool {
        matches!(
            error,
            CommandError::Infrastructure { .. } | CommandError::Timeout { .. }
        )
    }
}

// Conversion from rustycog_config::CommandRetryConfig to RetryPolicy
impl From<&rustycog_config::CommandRetryConfig> for RetryPolicy {
    fn from(config: &rustycog_config::CommandRetryConfig) -> Self {
        Self {
            max_attempts: config.max_attempts,
            base_delay: Duration::from_millis(config.base_delay_ms),
            max_delay: Duration::from_millis(config.max_delay_ms),
            backoff_multiplier: config.backoff_multiplier,
            use_jitter: config.use_jitter,
        }
    }
}

/// Registry configuration
#[derive(Debug, Clone)]
pub struct RegistryConfig {
    pub default_timeout: Duration,
    pub retry_policy: RetryPolicy,
    pub enable_metrics: bool,
    pub enable_tracing: bool,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            default_timeout: Duration::from_secs(30),
            retry_policy: RetryPolicy::default(),
            enable_metrics: true,
            enable_tracing: true,
        }
    }
}

impl RegistryConfig {
    /// Create a new `RegistryConfig` from a `CommandRetryConfig`
    #[must_use]
    pub fn from_retry_config(retry_config: &rustycog_config::CommandRetryConfig) -> Self {
        Self {
            default_timeout: Duration::from_secs(30),
            retry_policy: RetryPolicy::from(retry_config),
            enable_metrics: true,
            enable_tracing: true,
        }
    }
}

/// Trait for collecting metrics
#[async_trait]
pub trait MetricsCollector: Send + Sync {
    async fn record_metrics(&self, metrics: CommandMetrics);
}

/// Simple logging metrics collector
pub struct LoggingMetricsCollector;

#[async_trait]
impl MetricsCollector for LoggingMetricsCollector {
    async fn record_metrics(&self, metrics: CommandMetrics) {
        info!(
            command_type = %metrics.command_type,
            duration_ms = metrics.duration_ms,
            success = metrics.success,
            retry_attempts = metrics.retry_attempts,
            error_type = ?metrics.error_type,
            "Command metrics recorded"
        );
    }
}

/// Trait for error mapping that command handlers can implement
pub trait CommandErrorMapper: Send + Sync {
    /// Map a domain error to `CommandError`
    fn map_error(&self, error: Box<dyn std::error::Error + Send + Sync>) -> CommandError;
}

/// Type-erased command handler that can be stored in the registry
#[async_trait]
pub trait DynCommandHandler: Send + Sync {
    /// Execute the command with type-erased parameters
    async fn execute_dyn(
        &self,
        command: Box<dyn Any + Send>,
        _context: CommandContext,
    ) -> Result<Box<dyn Any + Send>, CommandError>;

    /// Get the command type this handler supports
    fn command_type(&self) -> &'static str;

    /// Get the error mapper for this handler
    fn error_mapper(&self) -> Arc<dyn CommandErrorMapper>;
}

/// Wrapper that implements `DynCommandHandler` for concrete command handlers
pub struct CommandHandlerWrapper<C, H>
where
    C: Command + 'static,
    H: CommandHandler<C>,
{
    handler: Arc<H>,
    error_mapper: Arc<dyn CommandErrorMapper>,
    _phantom: std::marker::PhantomData<C>,
}

impl<C, H> CommandHandlerWrapper<C, H>
where
    C: Command + 'static,
    H: CommandHandler<C>,
{
    pub fn new(handler: Arc<H>, error_mapper: Arc<dyn CommandErrorMapper>) -> Self {
        Self {
            handler,
            error_mapper,
            _phantom: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<C, H> DynCommandHandler for CommandHandlerWrapper<C, H>
where
    C: Command + 'static,
    H: CommandHandler<C>,
{
    async fn execute_dyn(
        &self,
        command: Box<dyn Any + Send>,
        _context: CommandContext,
    ) -> Result<Box<dyn Any + Send>, CommandError> {
        // Downcast the command to the expected type
        let command = command.downcast::<C>().map_err(|_| {
            CommandError::infrastructure("invalid_command_type", "Invalid command type")
        })?;

        // Execute the command and handle errors
        match self.handler.handle(*command).await {
            Ok(result) => Ok(Box::new(result)),
            Err(command_error) => {
                // If it's already a CommandError, just pass it through
                // Otherwise, this shouldn't happen since handlers should return CommandError
                Err(command_error)
            }
        }
    }

    fn command_type(&self) -> &'static str {
        // We need to get this from a sample command, but we can't create one here
        // This will be set when registering
        std::any::type_name::<C>()
    }

    fn error_mapper(&self) -> Arc<dyn CommandErrorMapper> {
        self.error_mapper.clone()
    }
}

/// Command registry that stores all command handlers
pub struct CommandRegistry {
    handlers: HashMap<String, Arc<dyn DynCommandHandler>>,
    config: RegistryConfig,
    metrics_collector: Arc<dyn MetricsCollector>,
}

impl CommandRegistry {
    /// Create a new command registry with default configuration
    #[must_use]
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            config: RegistryConfig::default(),
            metrics_collector: Arc::new(LoggingMetricsCollector),
        }
    }

    /// Create a new command registry with custom configuration
    #[must_use]
    pub fn with_config(config: RegistryConfig) -> Self {
        Self {
            handlers: HashMap::new(),
            config,
            metrics_collector: Arc::new(LoggingMetricsCollector),
        }
    }

    /// Create a new command registry with custom configuration and metrics collector
    pub fn with_config_and_metrics(
        config: RegistryConfig,
        metrics_collector: Arc<dyn MetricsCollector>,
    ) -> Self {
        Self {
            handlers: HashMap::new(),
            config,
            metrics_collector,
        }
    }

    /// Register a command handler with its error mapper
    pub fn register<C, H>(
        &mut self,
        command_type: String,
        handler: Arc<H>,
        error_mapper: Arc<dyn CommandErrorMapper>,
    ) where
        C: Command + 'static,
        H: CommandHandler<C> + 'static,
    {
        let wrapper = Arc::new(CommandHandlerWrapper::new(handler, error_mapper));
        self.handlers.insert(command_type, wrapper);
    }

    /// Get a handler for a command type
    #[must_use]
    pub fn get_handler(&self, command_type: &str) -> Option<Arc<dyn DynCommandHandler>> {
        self.handlers.get(command_type).cloned()
    }

    /// Execute a command through the registry with full cross-cutting concerns
    pub async fn execute_command<C: Command + Clone + 'static>(
        &self,
        command: C,
        context: CommandContext,
    ) -> Result<C::Result, CommandError> {
        let command_type = command.command_type();
        let start_time = Instant::now();

        self.trace_command_start(&command, &context);
        self.validate_command(&command, command_type)?;

        let handler = self.get_handler(command_type).ok_or_else(|| {
            CommandError::infrastructure(
                "handler_not_found",
                format!("No handler registered for command type: {command_type}"),
            )
        })?;

        self.execute_with_retry(command, context, handler, command_type, start_time)
            .await
    }

    async fn execute_with_retry<C: Command + Clone + 'static>(
        &self,
        command: C,
        context: CommandContext,
        handler: Arc<dyn DynCommandHandler>,
        command_type: &str,
        start_time: Instant,
    ) -> Result<C::Result, CommandError> {
        let mut retry_attempts = 0;
        loop {
            let execution_future =
                self.execute_once(handler.clone(), command.clone(), context.clone());
            let execution_result = timeout(self.config.default_timeout, execution_future).await;

            match execution_result {
                Ok(Ok(result)) => {
                    self.record_success(&command, command_type, start_time, retry_attempts)
                        .await;
                    return Ok(result);
                }
                Ok(Err(e)) => {
                    retry_attempts += 1;
                    let delay = self
                        .handle_command_failure(
                            &command,
                            command_type,
                            start_time,
                            retry_attempts,
                            e,
                        )
                        .await?;
                    tokio::time::sleep(delay).await;
                }
                Err(_) => {
                    retry_attempts += 1;
                    let delay = self
                        .handle_command_timeout(&command, command_type, start_time, retry_attempts)
                        .await?;
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    fn trace_command_start<C: Command>(&self, command: &C, context: &CommandContext) {
        if self.config.enable_tracing {
            info!(
                command_type = %command.command_type(),
                command_id = %command.command_id(),
                execution_id = %context.execution_id,
                "Starting command execution"
            );
        }
    }

    fn validate_command<C: Command>(
        &self,
        command: &C,
        command_type: &str,
    ) -> Result<(), CommandError> {
        command.validate().map_err(|validation_error| {
            if self.config.enable_tracing {
                error!(
                    command_type = %command_type,
                    error = %validation_error,
                    "Command validation failed"
                );
            }
            validation_error
        })
    }

    async fn record_success<C: Command>(
        &self,
        command: &C,
        command_type: &str,
        start_time: Instant,
        retry_attempts: u32,
    ) {
        let duration = start_time.elapsed();
        if self.config.enable_tracing {
            info!(
                command_type = %command_type,
                duration_ms = duration.as_millis() as u64,
                retry_attempts = retry_attempts,
                "Command executed successfully"
            );
        }

        if self.config.enable_metrics {
            self.record_success_metrics(command, duration, retry_attempts)
                .await;
        }
    }

    async fn handle_command_failure<C: Command>(
        &self,
        command: &C,
        command_type: &str,
        start_time: Instant,
        retry_attempts: u32,
        error: CommandError,
    ) -> Result<Duration, CommandError> {
        let retry_policy = &self.config.retry_policy;

        if !retry_policy.is_retryable(&error) {
            self.record_failure(
                command,
                command_type,
                start_time,
                retry_attempts,
                &error,
                "Command execution failed - error not retryable",
            )
            .await;
            return Err(error);
        }

        if retry_policy.max_attempts == 0 || retry_attempts >= retry_policy.max_attempts {
            self.record_failure(
                command,
                command_type,
                start_time,
                retry_attempts,
                &error,
                "Command execution failed after maximum retries",
            )
            .await;
            return Err(CommandError::retry_exhausted(
                "max_retries_exceeded",
                error.to_string(),
            ));
        }

        let delay = retry_policy.calculate_delay(retry_attempts - 1);
        self.trace_retry(command_type, Some(&error), retry_attempts, delay);
        Ok(delay)
    }

    async fn handle_command_timeout<C: Command>(
        &self,
        command: &C,
        command_type: &str,
        start_time: Instant,
        retry_attempts: u32,
    ) -> Result<Duration, CommandError> {
        let retry_policy = &self.config.retry_policy;
        let timeout_error = CommandError::timeout("command_timeout", "Command execution timed out");

        if retry_policy.max_attempts == 0 || retry_attempts >= retry_policy.max_attempts {
            self.record_failure(
                command,
                command_type,
                start_time,
                retry_attempts,
                &timeout_error,
                "Command execution timed out after retries",
            )
            .await;
            return Err(CommandError::retry_exhausted(
                "timeout_retries_exceeded",
                "Timeout after retries",
            ));
        }

        let delay = retry_policy.calculate_delay(retry_attempts - 1);
        self.trace_retry(command_type, None, retry_attempts, delay);
        Ok(delay)
    }

    async fn record_failure<C: Command>(
        &self,
        command: &C,
        command_type: &str,
        start_time: Instant,
        retry_attempts: u32,
        error: &CommandError,
        message: &str,
    ) {
        let duration = start_time.elapsed();
        if self.config.enable_tracing {
            error!(
                command_type = %command_type,
                error = %error,
                duration_ms = duration.as_millis() as u64,
                retry_attempts = retry_attempts,
                "{message}"
            );
        }

        if self.config.enable_metrics {
            self.record_failure_metrics(command, start_time, retry_attempts, error)
                .await;
        }
    }

    fn trace_retry(
        &self,
        command_type: &str,
        error: Option<&CommandError>,
        retry_attempts: u32,
        delay: Duration,
    ) {
        if !self.config.enable_tracing {
            return;
        }

        if let Some(error) = error {
            warn!(
                command_type = %command_type,
                error = %error,
                retry_attempt = retry_attempts,
                delay_ms = delay.as_millis() as u64,
                "Command failed, retrying"
            );
        } else {
            warn!(
                command_type = %command_type,
                retry_attempt = retry_attempts,
                delay_ms = delay.as_millis() as u64,
                "Command timed out, retrying"
            );
        }
    }

    /// Execute command once without retry logic
    async fn execute_once<C: Command + 'static>(
        &self,
        handler: Arc<dyn DynCommandHandler>,
        command: C,
        context: CommandContext,
    ) -> Result<C::Result, CommandError> {
        let result = handler.execute_dyn(Box::new(command), context).await?;

        // Downcast result back to expected type
        result
            .downcast::<C::Result>()
            .map(|boxed| *boxed)
            .map_err(|_| CommandError::infrastructure("invalid_result_type", "Invalid result type"))
    }

    async fn record_success_metrics<C: Command>(
        &self,
        command: &C,
        duration: Duration,
        retry_attempts: u32,
    ) {
        let metrics = CommandMetrics {
            command_type: command.command_type().to_string(),
            duration_ms: duration.as_millis() as u64,
            success: true,
            retry_attempts,
            error_type: None,
        };

        self.metrics_collector.record_metrics(metrics).await;
    }

    async fn record_failure_metrics<C: Command>(
        &self,
        command: &C,
        start_time: Instant,
        retry_attempts: u32,
        error: &CommandError,
    ) {
        let duration = start_time.elapsed();
        let metrics = CommandMetrics {
            command_type: command.command_type().to_string(),
            duration_ms: duration.as_millis() as u64,
            success: false,
            retry_attempts,
            error_type: Some(format!("{error:?}")),
        };

        self.metrics_collector.record_metrics(metrics).await;
    }

    /// List all registered command types
    #[must_use]
    pub fn list_command_types(&self) -> Vec<String> {
        self.handlers.keys().cloned().collect()
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for constructing a command registry
pub struct CommandRegistryBuilder {
    registry: CommandRegistry,
}

impl CommandRegistryBuilder {
    /// Create a new registry builder with default configuration
    #[must_use]
    pub fn new() -> Self {
        Self {
            registry: CommandRegistry::new(),
        }
    }

    /// Create a new registry builder with custom configuration
    #[must_use]
    pub fn with_config(config: RegistryConfig) -> Self {
        Self {
            registry: CommandRegistry::with_config(config),
        }
    }

    /// Create a new registry builder with custom configuration and metrics collector
    pub fn with_config_and_metrics(
        config: RegistryConfig,
        metrics_collector: Arc<dyn MetricsCollector>,
    ) -> Self {
        Self {
            registry: CommandRegistry::with_config_and_metrics(config, metrics_collector),
        }
    }

    /// Register a command handler with error mapper
    pub fn register<C, H>(
        mut self,
        command_type: String,
        handler: Arc<H>,
        error_mapper: Arc<dyn CommandErrorMapper>,
    ) -> Self
    where
        C: Command + 'static,
        H: CommandHandler<C> + 'static,
    {
        self.registry
            .register::<C, H>(command_type, handler, error_mapper);
        self
    }

    /// Build the registry
    #[must_use]
    pub fn build(self) -> CommandRegistry {
        self.registry
    }
}

impl Default for CommandRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}
