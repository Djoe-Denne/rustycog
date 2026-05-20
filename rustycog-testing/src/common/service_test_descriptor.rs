use async_trait::async_trait;

#[async_trait]
pub trait ServiceTestDescriptor<T>: Send + Sync + 'static {
    type Config: rustycog_config::ConfigLoader<Self::Config>
        + rustycog_config::HasServerConfig
        + rustycog_config::HasLoggingConfig
        + rustycog_config::HasDbConfig
        + rustycog_logger::ServiceLoggerConfig
        + Send
        + Sync
        + 'static;

    async fn build_app(
        &self,
        config: Self::Config,
        server_config: rustycog_config::ServerConfig,
    ) -> anyhow::Result<()>;

    async fn run_app(
        &self,
        config: Self::Config,
        server_config: rustycog_config::ServerConfig,
    ) -> anyhow::Result<()>;

    async fn run_migrations_up(
        &self,
        connection: &sea_orm::DatabaseConnection,
    ) -> anyhow::Result<()>;

    async fn run_migrations_down(
        &self,
        connection: &sea_orm::DatabaseConnection,
    ) -> anyhow::Result<()>;

    fn has_db(&self) -> bool;

    fn has_sqs(&self) -> bool;

    /// Whether this service exercises the centralized
    /// [`rustycog_permission::OpenFgaPermissionChecker`] in tests.
    ///
    /// When `true`, [`crate::common::TestFixture::new`] starts the singleton
    /// `openfga/openfga` testcontainer, creates a fresh store, uploads the
    /// model returned by [`Self::openfga_authorization_model_json`], and
    /// publishes the resolved API URL plus store / model ids into
    /// every consumer's env-var prefix so the typed
    /// `OpenFgaClientConfig` of the service-under-test picks them up at
    /// boot.
    ///
    /// No default — every implementor must declare it explicitly so a new
    /// service is forced to think about authorization wiring.
    fn has_openfga(&self) -> bool;

    /// Authorization model payload used by the OpenFGA testcontainer when
    /// [`Self::has_openfga`] returns `true`.
    ///
    /// Should return the full JSON body accepted by OpenFGA's
    /// `WriteAuthorizationModel` endpoint (`{schema_version, type_definitions}`).
    /// Typical implementation uses `Some(include_str!("../../openfga/model.json"))`
    /// in each service test descriptor.
    ///
    /// Defaults to `None` so descriptors that do not use OpenFGA do not need
    /// boilerplate.
    fn openfga_authorization_model_json(&self) -> Option<&'static str> {
        None
    }
}
