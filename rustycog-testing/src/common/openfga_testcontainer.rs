//! Real `OpenFGA` test container utilities.
//!
//! Provides a singleton `openfga/openfga` container reachable over HTTP for
//! integration tests that exercise the production [`rustycog_permission`]
//! checker against the real Check API.
//!
//! The container is configured with `--datastore-engine=memory` so it has no
//! Postgres dependency and starts in roughly two seconds. On first use the
//! fixture loads the `[openfga]` section of the service's typed config (so
//! `port = 0` in `config/test.toml` gets resolved to a free random host
//! port via [`OpenFgaClientConfig::actual_port`]), creates a fresh store,
//! uploads the model JSON provided by the test descriptor, and publishes the
//! resolved scheme / host /
//! port plus the new store and authorization-model ids into every
//! consumer's env-var prefix (`MANIFESTO_OPENFGA__*`, `HIVE_OPENFGA__*`,
//! `TELEGRAPH_OPENFGA__*`, `SENTINEL_SYNC_OPENFGA__*`) so each service's
//! typed `OpenFgaClientConfig` picks them up at boot.
//!
//! Splitting `host` and `port` instead of carrying a single `api_url` string
//! mirrors the convention used by [`DatabaseConfig`] / [`SqsConfig`] and is
//! what lets two test binaries run in parallel against their own `OpenFGA`
//! containers — each one binds a different random host port instead of
//! fighting over a fixed `8090`.
//!
//! Tests that touch this fixture must remain `#[serial]` because the
//! singleton is process-global; concurrent tests would fight for tuple
//! state.
//!
//! [`DatabaseConfig`]: rustycog_config::DatabaseConfig
//! [`SqsConfig`]: rustycog_config::SqsConfig

use rustycog_config::{load_config_part, OpenFgaClientConfig};
use rustycog_permission::{Permission, ResourceRef, Subject};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use testcontainers::{
    core::ContainerPort, runners::AsyncRunner, ContainerAsync, GenericImage, ImageExt,
};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Container name used for the singleton fixture. Must be unique per fixture
/// so `cleanup_existing_openfga_container` only tears down its own container.
const CONTAINER_NAME: &str = "openfga_test-fga";

/// `OpenFGA` Docker image tag pinned for reproducibility. Bump deliberately.
const OPENFGA_IMAGE_TAG: &str = "v1.5.0";

/// Global test `OpenFGA` container instance.
static TEST_OPENFGA_CONTAINER: OnceLock<Arc<Mutex<Option<Arc<TestOpenFgaContainer>>>>> =
    OnceLock::new();

/// One-time guard for the cleanup-handler registration.
static OPENFGA_CLEANUP_REGISTERED: AtomicBool = AtomicBool::new(false);

/// Test `OpenFGA` container wrapper.
///
/// Owns the underlying `ContainerAsync<GenericImage>` and exposes
/// [`Self::cleanup`] for explicit teardown. The container is otherwise
/// dropped along with the singleton when the test process exits.
pub struct TestOpenFgaContainer {
    container: ContainerAsync<GenericImage>,
    pub base_url: String,
    pub port: u16,
}

impl TestOpenFgaContainer {
    /// Stop and remove the underlying Docker container.
    pub async fn cleanup(self) {
        info!("Stopping and removing test OpenFGA container");
        if let Err(e) = self.container.stop().await {
            warn!("Failed to stop OpenFGA container: {}", e);
        } else {
            info!("OpenFGA container stopped successfully");
        }
        if let Err(e) = self.container.rm().await {
            warn!("Failed to remove OpenFGA container: {}", e);
        } else {
            info!("OpenFGA container removed successfully");
        }
        info!("Test OpenFGA container cleanup completed");
    }
}

/// Real-OpenFGA fixture handed to tests through `TestFixture::openfga()`.
///
/// Each fixture instance shares the singleton container but holds the
/// store id and authorization-model id that were created at construction
/// time. `allow` / `deny` write or delete relationship tuples through the
/// real Check pipeline; there is no mock layer.
///
/// `Clone` is intentional: every field is cheap to clone (strings or a
/// `reqwest::Client` Arc handle) and the underlying HTTP connection pool
/// is shared. Tests typically receive a clone from the harness so they
/// can issue `openfga.allow(...)` without holding a borrow on the
/// owning `TestFixture` for the request lifetime.
#[derive(Clone)]
pub struct TestOpenFga {
    pub client: reqwest::Client,
    pub base_url: String,
    pub port: u16,
    pub store_id: String,
    pub authorization_model_id: String,
    model_json: &'static str,
}

impl TestOpenFga {
    /// Get or create the global test `OpenFGA` fixture.
    ///
    /// The first call starts the singleton container, creates a store, and
    /// uploads the authorization model. Subsequent calls reuse the same
    /// container but always create a **fresh** store + model so back-to-back
    /// tests never see each other's tuples.
    pub async fn new(model_json: &'static str) -> Result<Self, Box<dyn std::error::Error>> {
        let (_container, base_url, port) = get_or_create_test_openfga_container().await?;

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()?;

        let (store_id, authorization_model_id) =
            provision_store_and_model(&client, &base_url, model_json).await?;

        publish_env(port, &store_id, &authorization_model_id);

        Ok(Self {
            client,
            base_url,
            port,
            store_id,
            authorization_model_id,
            model_json,
        })
    }

    /// Base URL the production checker should use as `OpenFgaClientConfig::api_url`.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    #[must_use]
    pub fn store_id(&self) -> &str {
        &self.store_id
    }

    #[must_use]
    pub fn authorization_model_id(&self) -> &str {
        &self.authorization_model_id
    }

    /// Ready-made [`OpenFgaClientConfig`] pointing at this fixture, with
    /// `cache_ttl_seconds = 0` so grant/revoke assertions inside a single
    /// test process observe the new decision instead of a cached one.
    ///
    /// `scheme` / `host` mirror the testcontainer's host binding and `port`
    /// is the materialized random port — there is no `api_url` field on
    /// `OpenFgaClientConfig`; the URL is built on the fly via
    /// `OpenFgaClientConfig::api_url()`.
    #[must_use]
    pub fn client_config(&self) -> OpenFgaClientConfig {
        OpenFgaClientConfig {
            scheme: "http".to_string(),
            host: "127.0.0.1".to_string(),
            port: self.port,
            store_id: self.store_id.clone(),
            authorization_model_id: Some(self.authorization_model_id.clone()),
            api_token: None,
            cache_ttl_seconds: Some(0),
        }
    }

    // ---------------------------------------------------------------------
    // Tuple writes
    // ---------------------------------------------------------------------

    /// Grant `(subject, action, resource)` by writing the underlying
    /// **writable** relation tuple.
    ///
    /// `Permission` maps to the *derived* `OpenFGA` relation that the
    /// production checker queries (`administer`, `own`, `read`, `write`).
    /// Tuples may only be written on relations the model declares with
    /// `[user]` direct restrictions, so this helper translates each
    /// permission to its underlying source relation
    /// ([`writable_relation_for`]) and writes that.
    pub async fn allow(
        &self,
        subject: Subject,
        action: Permission,
        resource: ResourceRef,
    ) -> Result<&Self, Box<dyn std::error::Error>> {
        let relation = writable_relation_for(resource.object_type, action);
        self.write_tuple(&subject.to_string(), relation, &resource.as_object_string())
            .await?;
        Ok(self)
    }

    /// Revoke `(subject, action, resource)` by deleting the matching
    /// underlying-relation tuple. Tolerates `cannot_delete_unknown_tuple`
    /// so tests can call `deny` defensively without first having written
    /// the tuple.
    pub async fn deny(
        &self,
        subject: Subject,
        action: Permission,
        resource: ResourceRef,
    ) -> Result<&Self, Box<dyn std::error::Error>> {
        let relation = writable_relation_for(resource.object_type, action);
        self.delete_tuple(&subject.to_string(), relation, &resource.as_object_string())
            .await?;
        Ok(self)
    }

    /// Wildcard-allow: grants `(user:*, action, resource)`. Only meaningful
    /// when the `OpenFGA` model declares the underlying relation with
    /// `[user, user:*]` (today only `project.viewer`).
    pub async fn allow_wildcard(
        &self,
        action: Permission,
        resource: ResourceRef,
    ) -> Result<&Self, Box<dyn std::error::Error>> {
        let relation = writable_relation_for(resource.object_type, action);
        self.write_tuple("user:*", relation, &resource.as_object_string())
            .await?;
        Ok(self)
    }

    /// Wildcard-deny: removes the `(user:*, action, resource)` tuple.
    pub async fn deny_wildcard(
        &self,
        action: Permission,
        resource: ResourceRef,
    ) -> Result<&Self, Box<dyn std::error::Error>> {
        let relation = writable_relation_for(resource.object_type, action);
        self.delete_tuple("user:*", relation, &resource.as_object_string())
            .await?;
        Ok(self)
    }

    /// Convenience: grant every standard permission (Read/Write/Admin/Owner)
    /// to `subject` on `resource`. Use when a happy-path test does not care
    /// about authorization fidelity and only needs to clear every route
    /// guard in its scope.
    pub async fn allow_all(
        &self,
        subject: Subject,
        resource: ResourceRef,
    ) -> Result<&Self, Box<dyn std::error::Error>> {
        for action in Permission::all() {
            self.allow(subject, action, resource).await?;
        }
        Ok(self)
    }

    /// Raw-tuple escape hatch for relations not enumerated by [`Permission`]
    /// (e.g. structural `member` / `viewer` / `organization` tuples that
    /// `sentinel-sync` writes but the checker never asks about directly).
    pub async fn write_tuple(
        &self,
        user: &str,
        relation: &str,
        object: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let body = json!({
            "writes": {
                "tuple_keys": [{
                    "user": user,
                    "relation": relation,
                    "object": object,
                }]
            },
            "authorization_model_id": self.authorization_model_id,
        });

        let url = format!("{}/stores/{}/write", self.base_url, self.store_id);
        let response = self.client.post(&url).json(&body).send().await?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            // Idempotent writes: OpenFGA rejects duplicates with
            // `cannot_write_tuple_which_already_exists`. Treat as success
            // so per-test arrange code can be re-run safely.
            if text.contains("cannot_write_tuple_which_already_exists")
                || text.contains("write_failed_due_to_invalid_input")
                    && text.contains("already exists")
            {
                debug!(
                    user,
                    relation, object, "OpenFGA write tolerated duplicate tuple"
                );
                return Ok(());
            }
            return Err(format!(
                "OpenFGA write returned {status} for ({user}, {relation}, {object}): {text}"
            )
            .into());
        }
        Ok(())
    }

    /// Raw-tuple escape hatch mirror of [`Self::write_tuple`].
    pub async fn delete_tuple(
        &self,
        user: &str,
        relation: &str,
        object: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let body = json!({
            "deletes": {
                "tuple_keys": [{
                    "user": user,
                    "relation": relation,
                    "object": object,
                }]
            },
            "authorization_model_id": self.authorization_model_id,
        });

        let url = format!("{}/stores/{}/write", self.base_url, self.store_id);
        let response = self.client.post(&url).json(&body).send().await?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            if text.contains("cannot_delete_unknown_tuple")
                || text.contains("write_failed_due_to_invalid_input")
            {
                debug!(
                    user,
                    relation, object, "OpenFGA delete tolerated unknown tuple"
                );
                return Ok(());
            }
            return Err(format!(
                "OpenFGA delete returned {status} for ({user}, {relation}, {object}): {text}"
            )
            .into());
        }
        Ok(())
    }

    /// Read tuples back from the store. Pass `None` for any field to leave
    /// it unconstrained. Useful for tests that want to assert what
    /// `sentinel-sync` (or test arrange code) wrote.
    pub async fn read_tuples(
        &self,
        user: Option<&str>,
        relation: Option<&str>,
        object: Option<&str>,
    ) -> Result<Vec<TupleKey>, Box<dyn std::error::Error>> {
        let mut tuple_key = serde_json::Map::new();
        if let Some(u) = user {
            tuple_key.insert("user".into(), Value::String(u.to_string()));
        }
        if let Some(r) = relation {
            tuple_key.insert("relation".into(), Value::String(r.to_string()));
        }
        if let Some(o) = object {
            tuple_key.insert("object".into(), Value::String(o.to_string()));
        }

        let body = json!({ "tuple_key": Value::Object(tuple_key) });
        let url = format!("{}/stores/{}/read", self.base_url, self.store_id);
        let response = self.client.post(&url).json(&body).send().await?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(format!("OpenFGA read returned {status}: {text}").into());
        }

        #[derive(Deserialize)]
        struct ReadResponse {
            #[serde(default)]
            tuples: Vec<TupleEntry>,
        }
        #[derive(Deserialize)]
        struct TupleEntry {
            key: TupleKey,
        }

        let decoded: ReadResponse = response.json().await?;
        Ok(decoded.tuples.into_iter().map(|t| t.key).collect())
    }

    /// Wipe every tuple in the current store by recreating it from scratch
    /// (delete-store, create-store, re-upload model). Refreshes
    /// [`Self::store_id`] / [`Self::authorization_model_id`] in place and
    /// re-publishes the env vars so a service that re-reads its config
    /// after `reset()` picks up the new ids.
    ///
    /// Note: services that already booted (e.g. through `setup_test_server`)
    /// captured the *previous* store id when they constructed
    /// `OpenFgaPermissionChecker`. They will keep talking to a now-deleted
    /// store. Reset before booting the app, not after — or rebuild the app.
    pub async fn reset(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let delete_url = format!("{}/stores/{}", self.base_url, self.store_id);
        let _ = self.client.delete(&delete_url).send().await; // best-effort

        let (store_id, model_id) =
            provision_store_and_model(&self.client, &self.base_url, self.model_json).await?;
        self.store_id = store_id;
        self.authorization_model_id = model_id;
        publish_env(self.port, &self.store_id, &self.authorization_model_id);
        Ok(())
    }
}

/// `OpenFGA` tuple key returned by the `Read` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TupleKey {
    pub user: String,
    pub relation: String,
    pub object: String,
}

/// Map a `(object_type, Permission)` pair to the `OpenFGA` relation a tuple
/// must be written on so that `Check(subject, Permission::relation(),
/// object)` succeeds against the checked-in
/// [`openfga/model.json`](../../../../openfga/model.json) authorization
/// model.
///
/// `Permission::relation()` returns the *derived* relation
/// (`administer`, `own`, `read`, `write`); those relations are computed
/// from the underlying direct relations (`admin`, `owner`, `viewer`,
/// `member`). `OpenFGA` only allows tuple writes against direct relations,
/// so this helper inverts the mapping per object type.
///
/// Special cases:
/// - `notification` collapses every action onto its single direct relation
///   `recipient` (the model derives all four derived relations from it).
/// - Default object types (`organization`, `project`) use the canonical
///   `viewer / member / admin / owner` mapping.
/// - `component` deliberately panics for `Admin` / `Owner` because those
///   relations are *only* derivable from a parent project tuple — write
///   `(subject, Admin, project:<id>)` instead of trying to grant admin
///   directly on a component.
#[must_use]
pub fn writable_relation_for(object_type: &str, action: Permission) -> &'static str {
    match (object_type, action) {
        ("notification", _) => "recipient",
        ("component", Permission::Read) => "viewer",
        ("component", Permission::Write) => "editor",
        ("component", Permission::Admin | Permission::Owner) => panic!(
            "component admin/owner is not directly writable; grant the parent project tuple instead"
        ),
        (_, Permission::Read) => "viewer",
        (_, Permission::Write) => "member",
        (_, Permission::Admin) => "admin",
        (_, Permission::Owner) => "owner",
    }
}

/// Get or create the global `OpenFGA` test container. Returns the wrapper
/// plus the resolved base URL and host port.
///
/// Port resolution flow:
/// - Loads the `[openfga]` section of the service's `config/test.toml` via
///   [`load_config_part`]. The expected shape is `host = "localhost"` plus
///   `port = 0`.
/// - Calls [`OpenFgaClientConfig::actual_port`] which materializes the
///   `0` into a free random port and caches the result process-wide so the
///   testcontainer fixture and the application boot path resolve to the
///   same value.
/// - Wires that port into `with_mapped_port(port, 8080)` so the container
///   listens on it on the host side.
async fn get_or_create_test_openfga_container(
) -> Result<(Arc<TestOpenFgaContainer>, String, u16), Box<dyn std::error::Error>> {
    let container_mutex = TEST_OPENFGA_CONTAINER.get_or_init(|| Arc::new(Mutex::new(None)));
    let mut container_guard = container_mutex.lock().await;

    if let Some(ref container) = *container_guard {
        return Ok((
            container.clone(),
            container.base_url.clone(),
            container.port,
        ));
    }

    info!("Creating new OpenFGA test container");

    cleanup_existing_openfga_container().await;

    // Clear only the OpenFGA port cache so a fresh container gets a fresh
    // random port instead of pointing at a previously-resolved port whose
    // container has been torn down (e.g. across `cargo test` runs that
    // share `OnceLock`-backed singletons).
    OpenFgaClientConfig::clear_port_cache();

    // Resolve the port through the typed config so `port = 0` in
    // `config/test.toml` gets materialized to a random free port via the
    // shared cache. Falls back to a default `OpenFgaClientConfig` (which
    // also has `port = 0` ⇒ random) if the section is missing.
    let openfga_config: OpenFgaClientConfig = load_config_part::<OpenFgaClientConfig>("openfga")
        .unwrap_or_else(|err| {
            warn!(
                error = %err,
                "Failed to load [openfga] config; falling back to default (port = 0)"
            );
            let mut cfg = OpenFgaClientConfig::default();
            cfg.port = 0;
            cfg
        });
    let port = openfga_config.actual_port();

    let image = GenericImage::new("openfga/openfga", OPENFGA_IMAGE_TAG)
        .with_cmd(vec![
            "run".to_string(),
            "--datastore-engine=memory".to_string(),
        ])
        .with_container_name(CONTAINER_NAME)
        .with_mapped_port(port, ContainerPort::Tcp(8080));

    info!("Starting OpenFGA container on port {}...", port);
    let container = image.start().await?;

    let base_url = format!("http://127.0.0.1:{port}");

    info!("OpenFGA container started; waiting for /healthz");
    wait_for_openfga(&base_url).await?;

    let test_container = Arc::new(TestOpenFgaContainer {
        container,
        base_url: base_url.clone(),
        port,
    });
    *container_guard = Some(test_container.clone());

    register_openfga_cleanup_handler().await;

    Ok((test_container, base_url, port))
}

/// Defensive shellout to remove any leaked container from a prior run.
async fn cleanup_existing_openfga_container() {
    use std::process::Command;
    debug!("Checking for existing OpenFGA test container '{CONTAINER_NAME}'");
    let _ = Command::new("docker")
        .args(["stop", CONTAINER_NAME])
        .output();
    let _ = Command::new("docker")
        .args(["rm", "-f", CONTAINER_NAME])
        .output();
    debug!("Cleaned up container: {CONTAINER_NAME}");
}

async fn register_openfga_cleanup_handler() {
    if OPENFGA_CLEANUP_REGISTERED.swap(true, Ordering::SeqCst) {
        return;
    }
    info!("Registering OpenFGA test container cleanup handler");
}

/// Poll `GET {base_url}/healthz` until `OpenFGA` reports SERVING. Bounded at
/// 30 attempts × 1s — far more than the typical 1–2s warm-up.
async fn wait_for_openfga(base_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{base_url}/healthz");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;

    for attempt in 1..=30 {
        match client.get(&url).send().await {
            Ok(response) if response.status().is_success() => {
                info!("OpenFGA is ready after {} attempt(s)", attempt);
                return Ok(());
            }
            Ok(response) => {
                debug!(
                    "OpenFGA /healthz returned {} on attempt {}",
                    response.status(),
                    attempt
                );
            }
            Err(e) => {
                debug!("OpenFGA /healthz call failed on attempt {}: {}", attempt, e);
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    Err("OpenFGA failed to become ready within 30 seconds".into())
}

/// Create a fresh store and upload the provided authorization model.
async fn provision_store_and_model(
    client: &reqwest::Client,
    base_url: &str,
    model_json: &str,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    #[derive(Deserialize)]
    struct CreateStoreResponse {
        id: String,
    }
    #[derive(Deserialize)]
    struct WriteModelResponse {
        authorization_model_id: String,
    }

    let store_url = format!("{base_url}/stores");
    let store_response = client
        .post(&store_url)
        .json(&json!({ "name": "rustycog-test-store" }))
        .send()
        .await?;
    let store_status = store_response.status();
    if !store_status.is_success() {
        let text = store_response.text().await.unwrap_or_default();
        return Err(format!("OpenFGA create-store returned {store_status}: {text}").into());
    }
    let store: CreateStoreResponse = store_response.json().await?;

    let model_url = format!("{base_url}/stores/{}/authorization-models", store.id);
    // The model JSON is expected to be the body wrapper:
    // `{schema_version, type_definitions}`. OpenFGA's
    // `WriteAuthorizationModel` accepts that shape directly.
    let model_value: Value = serde_json::from_str(model_json)?;
    let model_response = client.post(&model_url).json(&model_value).send().await?;
    let model_status = model_response.status();
    if !model_status.is_success() {
        let text = model_response.text().await.unwrap_or_default();
        return Err(format!("OpenFGA write-model returned {model_status}: {text}").into());
    }
    let model: WriteModelResponse = model_response.json().await?;

    info!(
        store_id = %store.id,
        authorization_model_id = %model.authorization_model_id,
        "Provisioned OpenFGA store + model"
    );
    Ok((store.id, model.authorization_model_id))
}

/// Publish the resolved fixture coordinates into every `OpenFGA` env-var
/// prefix that exists in this repo today. Confined to one `unsafe` block
/// per the testcontainer-fixture skill so env mutation has a single source.
///
/// Publishes `SCHEME` / `HOST` / `PORT` as three separate fields (not a
/// single `API_URL`) so the consumer's typed `OpenFgaClientConfig` keeps
/// its `host` / `port` shape — same convention as the SQS / DB testcontainer
/// fixtures. The `port` here is the host port we already bound the
/// container to via `with_mapped_port(...)`, so the consumer never needs to
/// hit `actual_port()`'s random-port fallback.
fn publish_env(port: u16, store_id: &str, authorization_model_id: &str) {
    const PREFIXES: &[&str] = &["MANIFESTO", "HIVE", "TELEGRAPH", "SENTINEL_SYNC"];
    let port_str = port.to_string();
    unsafe {
        for prefix in PREFIXES {
            // `rustycog-config` uses `<PREFIX>_` as the service-prefix
            // separator and `__` for nested fields, e.g.
            // `HIVE_OPENFGA__STORE_ID` -> `[openfga].store_id`.
            let canonical = format!("{prefix}_OPENFGA");
            publish_env_for_prefix(&canonical, &port_str, store_id, authorization_model_id);

            // Keep publishing the historical double-underscore shape while
            // in-flight docs/tests converge. It is ignored by the current
            // config loader but harmless for callers that might read env vars
            // directly.
            let legacy = format!("{prefix}__OPENFGA");
            publish_env_for_prefix(&legacy, &port_str, store_id, authorization_model_id);
        }
    }
}

unsafe fn publish_env_for_prefix(
    env_prefix: &str,
    port: &str,
    store_id: &str,
    authorization_model_id: &str,
) {
    std::env::set_var(format!("{env_prefix}__SCHEME"), "http");
    std::env::set_var(format!("{env_prefix}__HOST"), "127.0.0.1");
    std::env::set_var(format!("{env_prefix}__PORT"), port);
    std::env::set_var(format!("{env_prefix}__STORE_ID"), store_id);
    std::env::set_var(
        format!("{env_prefix}__AUTHORIZATION_MODEL_ID"),
        authorization_model_id,
    );
    std::env::set_var(format!("{env_prefix}__CACHE_TTL_SECONDS"), "0");
}
