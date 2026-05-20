use axum::{middleware, Router};
use rustycog_command::GenericCommandService;
use rustycog_config::ServerConfig;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::propagate_header::PropagateHeaderLayer;

use crate::middleware_permission::{
    optional_permission_middleware, permission_middleware, PermissionGuard,
};
use crate::{
    handle_panic, health_check,
    jwt_handler::UserIdExtractor,
    middleware_auth::{auth_middleware, optional_auth_middleware},
    tracing_middleware::{tracing_middleware, X_CORRELATION_ID},
};
use rustycog_permission::{Permission, PermissionChecker};

/// Application state for HTTP handlers
#[derive(Clone)]
pub struct AppState {
    pub running: bool,
    /// Command service for handling commands with cross-cutting concerns
    pub command_service: Arc<GenericCommandService>,
    /// User ID extractor for authentication
    pub user_id_extractor: Arc<UserIdExtractor>,
    /// Centralized permission checker (OpenFGA-backed in production)
    pub permission_checker: Arc<dyn PermissionChecker>,
}

impl AppState {
    /// Create a new `AppState`
    pub fn new(
        command_service: Arc<GenericCommandService>,
        user_id_extractor: UserIdExtractor,
        permission_checker: Arc<dyn PermissionChecker>,
    ) -> Self {
        Self {
            running: false,
            command_service,
            user_id_extractor: Arc::new(user_id_extractor),
            permission_checker,
        }
    }
}

/// Fluent route builder for creating HTTP routes
pub struct RouteBuilder {
    router: Router<AppState>,
    state: AppState,
    current_path: Option<String>,
    current_layer: Option<axum::routing::MethodRouter<AppState>>,
    // None: no auth, Some(true): require auth, Some(false): optional auth
    pending_auth: Option<bool>,
}

impl RouteBuilder {
    /// Create a new route builder
    #[must_use]
    pub fn new(state: AppState) -> Self {
        Self {
            router: Router::new(),
            state,
            current_path: None,
            current_layer: None,
            pending_auth: None,
        }
    }

    /// Add a route with a method router
    fn push_current(&mut self) {
        if let (Some(path), Some(layer)) = (self.current_path.take(), self.current_layer.take()) {
            let mut layer = layer;
            // Apply pending auth as the outermost layer so it runs first
            if let Some(require_auth) = self.pending_auth.take() {
                layer = if require_auth {
                    layer.route_layer(middleware::from_fn_with_state(
                        self.state.user_id_extractor.clone(),
                        auth_middleware,
                    ))
                } else {
                    layer.route_layer(middleware::from_fn_with_state(
                        self.state.user_id_extractor.clone(),
                        optional_auth_middleware,
                    ))
                };
            }
            let router = std::mem::take(&mut self.router);
            self.router = router.route(&path, layer);
        }
    }

    #[must_use]
    pub fn route(
        mut self,
        path: &str,
        method_router: axum::routing::MethodRouter<AppState>,
    ) -> Self {
        self.push_current();
        self.current_path = Some(path.to_string());
        self.current_layer = Some(method_router);
        self
    }

    /// Add a GET route
    pub fn get<H, T>(mut self, path: &str, handler: H) -> Self
    where
        H: axum::handler::Handler<T, AppState>,
        T: 'static,
    {
        self.push_current();
        self.current_path = Some(path.to_string());
        self.current_layer = Some(axum::routing::get(handler));
        self
    }

    /// Add a POST route
    pub fn post<H, T>(mut self, path: &str, handler: H) -> Self
    where
        H: axum::handler::Handler<T, AppState>,
        T: 'static,
    {
        self.push_current();
        self.current_path = Some(path.to_string());
        self.current_layer = Some(axum::routing::post(handler));
        self
    }

    /// Add a PUT route
    pub fn put<H, T>(mut self, path: &str, handler: H) -> Self
    where
        H: axum::handler::Handler<T, AppState>,
        T: 'static,
    {
        self.push_current();
        self.current_path = Some(path.to_string());
        self.current_layer = Some(axum::routing::put(handler));
        self
    }

    /// Add a DELETE route
    pub fn delete<H, T>(mut self, path: &str, handler: H) -> Self
    where
        H: axum::handler::Handler<T, AppState>,
        T: 'static,
    {
        self.push_current();
        self.current_path = Some(path.to_string());
        self.current_layer = Some(axum::routing::delete(handler));
        self
    }

    /// Add a PATCH route
    pub fn patch<H, T>(mut self, path: &str, handler: H) -> Self
    where
        H: axum::handler::Handler<T, AppState>,
        T: 'static,
    {
        self.push_current();
        self.current_path = Some(path.to_string());
        self.current_layer = Some(axum::routing::patch(handler));
        self
    }

    /// Add a health check endpoint
    pub fn health_check(mut self) -> Self {
        self.push_current();
        self.router = self
            .router
            .route("/health", axum::routing::get(health_check));
        self
    }

    /// Add nested routes with a prefix
    #[must_use]
    pub fn nest(mut self, prefix: &str, router: Router<AppState>) -> Self {
        self.router = self.router.nest(prefix, router);
        self
    }

    /// Build the final router with panic handling
    pub fn into_router(mut self) -> Router
    where
        AppState: Clone + Send + Sync + 'static,
    {
        // Push any pending route being built
        self.push_current();

        self.router
            .layer(CatchPanicLayer::custom(handle_panic))
            .layer(PropagateHeaderLayer::new(X_CORRELATION_ID.parse().unwrap()))
            .layer(middleware::from_fn(tracing_middleware))
            .with_state(self.state)
    }

    /// Build and serve the final router.
    pub async fn build(self, config: ServerConfig) -> anyhow::Result<()>
    where
        AppState: Clone + Send + Sync + 'static,
    {
        serve_router(self.into_router(), config).await
    }
}

/// Serve an already-built Axum router using the configured HTTP/TLS listener.
pub async fn serve_router(app: Router, config: ServerConfig) -> anyhow::Result<()> {
    if config.tls_enabled {
        tracing::info!(
            "Starting HTTPS server on {}:{}",
            config.host,
            config.tls_port
        );

        let tls_config = axum_server::tls_rustls::RustlsConfig::from_pem_file(
            config.tls_cert_path,
            config.tls_key_path,
        )
        .await?;
        let addr: SocketAddr = format!("{}:{}", config.host, config.tls_port).parse()?;

        axum_server::bind_rustls(addr, tls_config)
            .serve(app.into_make_service())
            .await?;
    } else {
        let port = config.actual_port();
        tracing::info!("Starting HTTP server on {}:{}", config.host, port);
        let addr: SocketAddr = format!("{}:{}", config.host, port).parse()?;
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;
    }

    Ok(())
}

impl RouteBuilder {
    /// Mark the current route as requiring authentication
    #[must_use]
    pub const fn authenticated(mut self) -> Self {
        self.pending_auth = Some(true);
        self
    }

    /// Mark the current route as allowing optional authentication
    #[must_use]
    pub const fn might_be_authenticated(mut self) -> Self {
        self.pending_auth = Some(false);
        self
    }

    /// Attach a centralized permission guard to the current route.
    ///
    /// `object_type` must match an `OpenFGA` type defined in `openfga/model.fga`
    /// (e.g. `"organization"`, `"project"`, `"component"`, `"notification"`).
    /// The middleware extracts the deepest UUID path segment and builds a
    /// `ResourceRef` of that type, then calls
    /// `AppState.permission_checker.check(...)`.
    pub fn with_permission_on(mut self, required: Permission, object_type: &'static str) -> Self {
        let guard = Arc::new(PermissionGuard {
            required,
            object_type,
            checker: self.state.permission_checker.clone(),
        });
        if let Some(layer) = self.current_layer.take() {
            self.current_layer = Some(if self.pending_auth == Some(false) {
                layer.route_layer(middleware::from_fn_with_state(
                    guard,
                    optional_permission_middleware,
                ))
            } else {
                layer.route_layer(middleware::from_fn_with_state(guard, permission_middleware))
            });
        }
        self
    }
}
