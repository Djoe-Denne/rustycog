//! Internal endpoint and creation ownership context; no name-based teardown.
use std::{
    collections::HashMap,
    future::Future,
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
};
use testcontainers::{ContainerAsync, GenericImage};
use tokio::task::JoinHandle;

type StartResult = Result<OwnedContainer, String>;
type Task = Arc<Slot>;
static CONTEXT: OnceLock<Result<Arc<Context>, String>> = OnceLock::new();
static ATTEMPT: AtomicU64 = AtomicU64::new(0);
static TASKS: OnceLock<Mutex<Vec<Task>>> = OnceLock::new();

#[derive(Clone)]
pub struct Endpoint {
    pub host: String,
}
impl Endpoint {
    fn parse(mode: &str, host: Option<&str>) -> Result<Self, String> {
        if !matches!(mode, "local" | "bridge") {
            return Err("explicit runner mode local|bridge required".into());
        }
        let host = host
            .or_else(|| (mode == "local").then_some("127.0.0.1"))
            .ok_or("bridge runner host required")?;
        if host.is_empty()
            || host.len() > 253
            || !host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
            || host.starts_with('-')
            || host.ends_with('-')
        {
            return Err("invalid runner hostname (DNS/IPv4 only)".into());
        }
        let host = host.to_ascii_lowercase();
        if host.split('.').any(|label| {
            label.is_empty() || label.len() > 63 || label.starts_with('-') || label.ends_with('-')
        }) {
            return Err("invalid DNS hostname".into());
        }
        if host.bytes().all(|b| b.is_ascii_digit() || b == b'.')
            && host.parse::<std::net::Ipv4Addr>().is_err()
        {
            return Err("invalid IPv4 endpoint".into());
        }
        if host == "0.0.0.0"
            || (mode == "bridge" && (host == "localhost" || host.starts_with("127.")))
        {
            return Err("bridge runner cannot assume loopback".into());
        }
        Ok(Self { host })
    }
    pub fn authority(&self, port: u16) -> Result<String, String> {
        if port == 0 {
            return Err("mapped port must be nonzero".into());
        }
        Ok(format!("{}:{port}", self.host))
    }
    pub fn http(&self, port: u16) -> Result<String, String> {
        Ok(format!("http://{}", self.authority(port)?))
    }
    pub fn rewrite_url(&self, raw: &str, port: u16) -> Result<String, String> {
        let mut url = url::Url::parse(raw).map_err(|_| "invalid fixture URL")?;
        if url.scheme() != "http"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.host_str().is_none()
        {
            return Err("invalid fixture HTTP URL".into());
        }
        self.authority(port)?;
        url.set_host(Some(&self.host))
            .map_err(|_| "invalid fixture hostname")?;
        url.set_port(Some(port))
            .map_err(|()| "invalid fixture port")?;
        Ok(url.to_string())
    }
}

struct Context {
    run: String,
    endpoint: Endpoint,
    ledger: Mutex<std::fs::File>,
    ids: Mutex<HashMap<u64, String>>,
    unknown: AtomicBool,
}
impl Context {
    fn event(
        &self,
        attempt: u64,
        role: &str,
        name: &str,
        id: Option<&str>,
        event: &str,
        port: u16,
    ) -> Result<(), String> {
        let id = {
            let mut ids = self
                .ids
                .lock()
                .map_err(|_| "fixture identity lock poisoned")?;
            if let Some(id) = id {
                if ids.get(&attempt).is_some_and(|previous| previous != id) {
                    self.unknown.store(true, Ordering::SeqCst);
                    return Err("fixture creation identity cannot change".into());
                }
                ids.insert(attempt, id.to_string());
            }
            ids.get(&attempt).cloned()
        };
        let record = serde_json::json!({"schema_version":1,"run_id":self.run,"process_id":std::process::id(),"attempt_id":attempt,"fixture_role":role,"container_name":name,"container_id":id,"event":event,"published_port":port});
        self.write_record(&record, attempt)
    }
    fn write_record(&self, record: &serde_json::Value, attempt: u64) -> Result<(), String> {
        let mut file = self
            .ledger
            .lock()
            .map_err(|_| "fixture ledger lock poisoned")?;
        let mut bytes =
            serde_json::to_vec(&record).map_err(|_| "fixture ledger serialization failed")?;
        bytes.push(b'\n');
        file.write_all(&bytes)
            .and_then(|()| file.flush())
            .and_then(|()| file.sync_data())
            .map_err(|_| {
                self.unknown.store(true, Ordering::SeqCst);
                tracing::error!(
                    attempt,
                    container_id = record["container_id"].as_str(),
                    "fixture ledger write failed; cleanup INCONCLUSIVE"
                );
                "fixture ledger write failed".into()
            })
    }
    fn unknown(&self, attempt: u64, role: &str, name: &str, event: &str, port: u16) {
        self.unknown.store(true, Ordering::SeqCst);
        let _ = self.event(attempt, role, name, None, event, port);
        tracing::error!(
            attempt,
            role,
            "fixture creation identity UNKNOWN; cleanup INCONCLUSIVE"
        );
    }
}
fn context() -> Result<Arc<Context>, String> {
    CONTEXT
        .get_or_init(|| {
            let mode = std::env::var("RUSTYCOG_TEST_RUNNER_MODE")
                .map_err(|_| "explicit runner mode required")?;
            let host = std::env::var("RUSTYCOG_TEST_RUNNER_HOST").ok();
            let endpoint = Endpoint::parse(&mode, host.as_deref())?;
            let run = std::env::var("RUSTYCOG_TEST_RUN_ID")
                .or_else(|_| {
                    if mode == "local" {
                        Ok(uuid::Uuid::new_v4().simple().to_string())
                    } else {
                        Err(std::env::VarError::NotPresent)
                    }
                })
                .map_err(|_| "parent run ID required")?;
            if run.is_empty()
                || run.len() > 40
                || !run.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            {
                return Err("invalid bounded run ID".into());
            }
            let dir = PathBuf::from(
                std::env::var("RUSTYCOG_TEST_LEDGER_DIR")
                    .map_err(|_| "parent-approved ledger directory required")?,
            );
            if !dir.is_absolute() || !dir.is_dir() {
                return Err("ledger directory must already exist and be absolute".into());
            }
            let path = dir.join(format!("fixture-{run}-{}.jsonl", std::process::id()));
            let ledger = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|_| "cannot create exclusive fixture ledger")?;
            Ok(Arc::new(Context {
                run,
                endpoint,
                ledger: Mutex::new(ledger),
                ids: Mutex::new(HashMap::new()),
                unknown: AtomicBool::new(false),
            }))
        })
        .clone()
}
pub fn endpoint() -> Result<Endpoint, String> {
    Ok(context()?.endpoint.clone())
}
pub fn take_unshared<T>(singleton: &mut Option<Arc<T>>) -> Result<Option<T>, Arc<T>> {
    let Some(arc) = singleton.take() else {
        return Ok(None);
    };
    match Arc::try_unwrap(arc) {
        Ok(owned) => Ok(Some(owned)),
        Err(arc) => {
            *singleton = Some(arc.clone());
            Err(arc)
        }
    }
}

#[derive(Default)]
struct Inventory {
    names: Vec<String>,
    ports: Vec<u16>,
}
impl Inventory {
    fn include_container(
        &mut self,
        names: &[String],
        published_ports: &[u16],
        listed_state: Option<&str>,
        inspected_state: Option<&testcontainers::bollard::models::ContainerState>,
    ) {
        // Names remain reserved in ALL states. No inventory evidence grants
        // ownership, adoption, or permission to remove the existing container.
        self.names.extend_from_slice(names);
        // HostConfig retains port bindings after stop. Ignore only a positively
        // confirmed exited state; contradictory, missing, paused/restarting or
        // transitional evidence remains a fail-closed port reservation.
        let confirmed_exited = listed_state == Some("exited")
            && inspected_state.is_some_and(|state| {
                state.status
                    == Some(testcontainers::bollard::models::ContainerStateStatusEnum::EXITED)
                    && state.running == Some(false)
                    && state.paused == Some(false)
                    && state.restarting == Some(false)
                    && state.dead == Some(false)
            });
        if !confirmed_exited {
            self.ports.extend_from_slice(published_ports);
        }
    }
}
fn check_collision(inventory: &Inventory, name: &str, port: u16) -> Result<(), String> {
    if inventory
        .names
        .iter()
        .any(|n| n.trim_start_matches('/') == name)
        || inventory.ports.contains(&port)
    {
        return Err("foreign/unknown fixture name or published-port collision; STOP".into());
    }
    Ok(())
}
pub struct Attempt {
    context: Arc<Context>,
    number: u64,
    role: String,
    name: String,
    port: u16,
}
impl Attempt {
    pub async fn prepare(role: &str, port: u16) -> Result<Self, String> {
        let context = context()?;
        context.endpoint.authority(port)?;
        if role.is_empty()
            || role.len() > 24
            || !role.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
        {
            return Err("invalid fixture role".into());
        }
        let number = ATTEMPT.fetch_add(1, Ordering::SeqCst);
        if number >= 128 {
            return Err("fixture attempt limit reached".into());
        }
        let name = format!("{role}-{}-{}-{number}", context.run, std::process::id());
        let docker = testcontainers::core::client::docker_client_instance()
            .await
            .map_err(|_| "Docker inventory client unavailable")?;
        let containers = docker
            .list_containers(Some(
                testcontainers::bollard::container::ListContainersOptions::<String> {
                    all: true,
                    ..Default::default()
                },
            ))
            .await
            .map_err(|_| "Docker inventory failed; STOP")?;
        let mut inventory = Inventory::default();
        let mut before = Vec::new();
        for container in containers {
            let id = container
                .id
                .as_deref()
                .filter(|id| !id.is_empty())
                .ok_or("Docker inventory has unknown ID; STOP")?;
            let mut published: Vec<u16> = container
                .ports
                .as_deref()
                .unwrap_or_default()
                .iter()
                .filter_map(|port| port.public_port)
                .collect();
            // Stopped containers may omit Ports in list output. Read only their
            // identity/binding evidence; never adopt them or emit inspect data.
            let inspect = docker
                .inspect_container(
                    id,
                    None::<testcontainers::bollard::container::InspectContainerOptions>,
                )
                .await
                .map_err(|_| "cannot inspect fixture collision inventory; STOP")?;
            let bindings = inspect
                .host_config
                .and_then(|config| config.port_bindings)
                .unwrap_or_default();
            for bindings in bindings.into_values().flatten() {
                for binding in bindings {
                    if let Some(raw) = binding.host_port.filter(|raw| !raw.is_empty()) {
                        let port = raw
                            .parse::<u16>()
                            .map_err(|_| "unknown published-port inventory; STOP")?;
                        if port != 0 {
                            published.push(port);
                        }
                    }
                }
            }
            before.push(serde_json::json!({"id":id,"names":container.names,"published_ports":published,"state":container.state}));
            inventory.include_container(
                container.names.as_deref().unwrap_or_default(),
                &published,
                container.state.as_deref(),
                inspect.state.as_ref(),
            );
        }
        context.write_record(&serde_json::json!({"schema_version":1,"run_id":context.run,"process_id":std::process::id(),"attempt_id":number,"fixture_role":role,"container_name":name,"container_id":null,"event":"inventory","published_port":port,"before_inventory":before}), number)?;
        if let Err(error) = check_collision(&inventory, &name, port) {
            context.event(number, role, &name, None, "collision_refused", port)?;
            return Err(error);
        }
        context.event(number, role, &name, None, "attempt", port)?;
        Ok(Self {
            context,
            number,
            role: role.into(),
            name,
            port,
        })
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub async fn start<F>(self, create: F) -> StartResult
    where
        F: Future<
                Output = Result<
                    ContainerAsync<GenericImage>,
                    testcontainers::core::error::TestcontainersError,
                >,
            > + Send
            + 'static,
    {
        let context = self.context.clone();
        let slot = Arc::new(Slot {
            active: AtomicBool::new(true),
            task: tokio::sync::Mutex::new(None),
            teardown: tokio::sync::Mutex::new(None),
            context: context.clone(),
            number: self.number,
            role: self.role.clone(),
            name: self.name.clone(),
            port: self.port,
        });
        {
            let mut tasks = TASKS
                .get_or_init(|| Mutex::new(Vec::new()))
                .lock()
                .map_err(|_| "fixture supervisor lock poisoned")?;
            let task = tokio::spawn(async move {
                if let Ok(container) = create.await {
                    let id = container.id().to_string();
                    if id.is_empty() {
                        self.context.unknown(
                            self.number,
                            &self.role,
                            &self.name,
                            "unknown_empty_id",
                            self.port,
                        );
                        return Err("fixture returned no ID; cleanup INCONCLUSIVE".into());
                    }
                    let owned = OwnedContainer {
                        container: Some(container),
                        attempt: self,
                        id,
                        removed_confirmed: false,
                    };
                    owned.attempt.context.event(
                        owned.attempt.number,
                        &owned.attempt.role,
                        &owned.attempt.name,
                        Some(&owned.id),
                        "created",
                        owned.attempt.port,
                    )?;
                    Ok(owned)
                } else {
                    self.context.unknown(
                        self.number,
                        &self.role,
                        &self.name,
                        "unknown_opaque_start_failure",
                        self.port,
                    );
                    Err("opaque fixture start failure; identity UNKNOWN".into())
                }
            });
            *slot
                .task
                .try_lock()
                .map_err(|_| "new supervisor slot unexpectedly locked")? = Some(task);
            tasks.push(slot.clone());
        }
        let mut guard = CallerLease {
            slot: slot.clone(),
            joined: false,
        };
        let result = join_slot(&slot).await;
        guard.joined = true;
        drop(guard);
        result
    }
}
struct Slot {
    active: AtomicBool,
    task: tokio::sync::Mutex<Option<JoinHandle<StartResult>>>,
    teardown: tokio::sync::Mutex<Option<JoinHandle<Result<(), String>>>>,
    context: Arc<Context>,
    number: u64,
    role: String,
    name: String,
    port: u16,
}
struct CallerLease {
    slot: Task,
    joined: bool,
}
impl Drop for CallerLease {
    fn drop(&mut self) {
        self.slot.active.store(false, Ordering::SeqCst);
        let _ = self.slot.context.event(
            self.slot.number,
            &self.slot.role,
            &self.slot.name,
            None,
            if self.joined {
                "creation_joined"
            } else {
                "caller_cancelled_creation_retained_active_lease"
            },
            self.slot.port,
        );
    }
}
async fn await_retained<T>(
    cell: &tokio::sync::Mutex<Option<JoinHandle<T>>>,
) -> Result<Option<T>, tokio::task::JoinError> {
    let mut guard = cell.lock().await;
    let Some(task) = guard.as_mut() else {
        return Ok(None);
    };
    let result = task.await;
    guard.take();
    drop(guard);
    result.map(Some)
}
async fn join_slot(slot: &Slot) -> StartResult {
    match await_retained(&slot.task).await {
        Ok(Some(result)) => result,
        Ok(None) => Err("fixture creation already joined".into()),
        Err(_) => {
            slot.context.unknown(
                slot.number,
                &slot.role,
                &slot.name,
                "unknown_start_task_failure",
                slot.port,
            );
            Err("fixture task failed; identity UNKNOWN".into())
        }
    }
}
enum Reconciliation<T> {
    Idle,
    Pending,
    Busy,
    Joined(Result<T, tokio::task::JoinError>),
}
async fn join_finished<T>(cell: &mut Option<JoinHandle<T>>) -> Reconciliation<T> {
    let Some(task) = cell.as_mut() else {
        return Reconciliation::Idle;
    };
    if !task.is_finished() {
        return Reconciliation::Pending;
    }
    // A finished task has a join result available. Borrow until that result is
    // obtained: cancelling this future must never discard the retained handle.
    let result = task.await;
    cell.take();
    Reconciliation::Joined(result)
}
async fn reconcile_retained<T>(
    cell: &tokio::sync::Mutex<Option<JoinHandle<T>>>,
) -> Reconciliation<T> {
    let Ok(mut guard) = cell.try_lock() else {
        return Reconciliation::Busy;
    };
    join_finished(&mut guard).await
}
#[derive(Debug, PartialEq, Eq)]
enum OrphanReconciliation {
    Idle,
    Pending,
    CreationFailed,
    CreationTaskFailed,
    TeardownFailed,
    TeardownTaskFailed,
}
async fn reconcile_orphan<T, R, F>(
    creation: &tokio::sync::Mutex<Option<JoinHandle<Result<T, String>>>>,
    removal: &tokio::sync::Mutex<Option<JoinHandle<Result<(), String>>>>,
    release: R,
) -> OrphanReconciliation
where
    T: Send + 'static,
    R: FnOnce(T) -> F + Send,
    F: Future<Output = Result<(), String>> + Send + 'static,
{
    // Reserve the destination before joining a creation: its owned result must
    // always be supervised without a second, potentially busy lock.
    let Ok(mut teardown) = removal.try_lock() else {
        return OrphanReconciliation::Pending;
    };
    match join_finished(&mut teardown).await {
        Reconciliation::Pending | Reconciliation::Busy => return OrphanReconciliation::Pending,
        Reconciliation::Joined(Err(_)) => return OrphanReconciliation::TeardownTaskFailed,
        Reconciliation::Joined(Ok(Err(_))) => return OrphanReconciliation::TeardownFailed,
        Reconciliation::Idle | Reconciliation::Joined(Ok(Ok(()))) => {}
    }
    match reconcile_retained(creation).await {
        Reconciliation::Joined(Ok(Ok(container))) => {
            // Owned rm can itself wait on Docker. Retain its task rather than
            // blocking cleanup of an unrelated eligible singleton.
            *teardown = Some(tokio::spawn(release(container)));
            OrphanReconciliation::Pending
        }
        Reconciliation::Joined(Err(_)) => OrphanReconciliation::CreationTaskFailed,
        Reconciliation::Joined(Ok(Err(_))) => OrphanReconciliation::CreationFailed,
        Reconciliation::Pending | Reconciliation::Busy => OrphanReconciliation::Pending,
        Reconciliation::Idle => OrphanReconciliation::Idle,
    }
}
/// Reconcile completed cancelled creations without waiting for pending/busy tasks.
///
/// Completed orphan creations use normal owned removal in a retained task cell;
/// a later pass joins its completion. Pending creation/removal keeps an active
/// runtime lease. Retry within the originating runtime; do not shut it down.
/// This pass does not certify that all fixture consumers have been released.
///
/// # Errors
/// Returns INCONCLUSIVE for pending/busy tasks, unknown IDs or unverified removal.
pub async fn join_fixture_creations() -> Result<(), String> {
    let slots = {
        let tasks = TASKS
            .get_or_init(|| Mutex::new(Vec::new()))
            .try_lock()
            .map_err(|_| "fixture cleanup INCONCLUSIVE: supervisor busy or poisoned; retain originating runtime")?;
        tasks.clone()
    };
    let mut inconclusive = false;
    for slot in slots {
        inconclusive |= slot.context.unknown.load(Ordering::SeqCst);
        if slot.active.load(Ordering::SeqCst) {
            inconclusive = true;
            continue;
        }
        match reconcile_orphan(&slot.task, &slot.teardown, |container| async move {
            container.rm().await
        })
        .await
        {
            OrphanReconciliation::CreationTaskFailed => {
                slot.context.unknown(
                    slot.number,
                    &slot.role,
                    &slot.name,
                    "unknown_start_task_failure",
                    slot.port,
                );
                inconclusive = true;
            }
            OrphanReconciliation::TeardownTaskFailed => {
                slot.context.unknown(
                    slot.number,
                    &slot.role,
                    &slot.name,
                    "unknown_teardown_task_failure",
                    slot.port,
                );
                inconclusive = true;
            }
            OrphanReconciliation::Pending
            | OrphanReconciliation::CreationFailed
            | OrphanReconciliation::TeardownFailed => inconclusive = true,
            OrphanReconciliation::Idle => {}
        }
        inconclusive |= slot.context.unknown.load(Ordering::SeqCst);
    }
    if inconclusive {
        Err("fixture cleanup INCONCLUSIVE: active/pending/busy creation or teardown, UNKNOWN identity, or unverified teardown; retain originating runtime for retained tasks".into())
    } else {
        Ok(())
    }
}

pub struct OwnedContainer {
    container: Option<ContainerAsync<GenericImage>>,
    attempt: Attempt,
    id: String,
    removed_confirmed: bool,
}
impl OwnedContainer {
    pub async fn mapped_port(
        &self,
        internal: testcontainers::core::ContainerPort,
    ) -> Result<u16, String> {
        let port = self
            .container
            .as_ref()
            .ok_or("owned container exists until consuming teardown")?
            .get_host_port_ipv4(internal)
            .await
            .map_err(|_| "cannot obtain daemon-mapped fixture port".to_string())?;
        self.attempt.context.endpoint.authority(port)?;
        self.attempt.context.event(
            self.attempt.number,
            &self.attempt.role,
            &self.attempt.name,
            Some(&self.id),
            "mapped_port_observed",
            port,
        )?;
        Ok(port)
    }
    pub fn deferred(&self) {
        let _ = self.attempt.context.event(
            self.attempt.number,
            &self.attempt.role,
            &self.attempt.name,
            Some(&self.id),
            "deferred_active_consumers",
            self.attempt.port,
        );
        tracing::warn!(
            container_id = self.id,
            "fixture has active consumers; no fallback"
        );
    }
    pub fn ready(&self) -> Result<(), String> {
        self.attempt.context.event(
            self.attempt.number,
            &self.attempt.role,
            &self.attempt.name,
            Some(&self.id),
            "ready",
            self.attempt.port,
        )
    }
    pub async fn stop(&self) -> Result<(), String> {
        self.container
            .as_ref()
            .ok_or("owned container exists until consuming teardown")?
            .stop()
            .await
            .map_err(|_| "owned container stop failed".to_string())
    }
    pub async fn rm(mut self) -> Result<(), String> {
        ensure_exact_owned_id(
            &self.id,
            self.container.as_ref().map(ContainerAsync::id),
            false,
        )?;
        self.attempt.context.event(
            self.attempt.number,
            &self.attempt.role,
            &self.attempt.name,
            Some(&self.id),
            "owned_teardown_requested",
            self.attempt.port,
        )?;
        let container = self
            .container
            .take()
            .ok_or("owned handle already consumed")?;
        let result = container
            .rm()
            .await
            .map_err(|_| "owned container teardown failed".to_string());
        if result.is_err() {
            self.attempt.context.unknown.store(true, Ordering::SeqCst);
        } else {
            self.removed_confirmed = true;
        }
        self.attempt.context.event(
            self.attempt.number,
            &self.attempt.role,
            &self.attempt.name,
            Some(&self.id),
            if result.is_ok() {
                "removed"
            } else {
                "teardown_failed"
            },
            self.attempt.port,
        )?;
        result
    }
}
fn ensure_exact_owned_id(
    recorded: &str,
    actual: Option<&str>,
    active_lease: bool,
) -> Result<(), String> {
    if active_lease || recorded.is_empty() || actual != Some(recorded) {
        return Err("missing/mismatched owned ID or active lease: no fallback".into());
    }
    Ok(())
}
impl Drop for OwnedContainer {
    fn drop(&mut self) {
        if !self.removed_confirmed {
            // Includes cancellation while consuming rm(): taking the handle
            // is not evidence that the daemon completed its removal.
            self.attempt.context.unknown.store(true, Ordering::SeqCst);
            let _ = self.attempt.context.event(
                self.attempt.number,
                &self.attempt.role,
                &self.attempt.name,
                Some(&self.id),
                "owned_handle_dropped_teardown_unverified",
                self.attempt.port,
            );
        }
    }
}

#[cfg(test)]
mod safety_tests {
    use super::*;
    use testcontainers::bollard::models::{ContainerState, ContainerStateStatusEnum};

    fn exited_state() -> ContainerState {
        ContainerState {
            status: Some(ContainerStateStatusEnum::EXITED),
            running: Some(false),
            paused: Some(false),
            restarting: Some(false),
            dead: Some(false),
            ..ContainerState::default()
        }
    }

    #[test]
    fn prepare_projection_frees_only_exited_ports_and_keeps_exited_names() {
        let mut inventory = Inventory::default();
        inventory.include_container(
            &["/postgres-old-run".into()],
            &[45283],
            Some("exited"),
            Some(&exited_state()),
        );
        assert!(check_collision(&inventory, "postgres-fresh-run", 45283).is_ok());
        assert!(check_collision(&inventory, "postgres-old-run", 1234).is_err());
        assert_eq!(inventory.names, vec!["/postgres-old-run"]);
        assert!(inventory.ports.is_empty());
    }

    #[test]
    fn prepare_projection_protects_active_transitional_and_unknown_ports() {
        for (listed, status) in [
            (Some("running"), Some(ContainerStateStatusEnum::RUNNING)),
            (Some("paused"), Some(ContainerStateStatusEnum::PAUSED)),
            (
                Some("restarting"),
                Some(ContainerStateStatusEnum::RESTARTING),
            ),
            (Some("created"), Some(ContainerStateStatusEnum::CREATED)),
            (Some("removing"), Some(ContainerStateStatusEnum::REMOVING)),
            (Some("dead"), Some(ContainerStateStatusEnum::DEAD)),
            (Some("unknown"), None),
            (None, None),
            (Some("exited"), Some(ContainerStateStatusEnum::RUNNING)),
            (Some("running"), Some(ContainerStateStatusEnum::EXITED)),
        ] {
            let mut state = exited_state();
            state.status = status;
            state.running = Some(matches!(
                status,
                Some(
                    ContainerStateStatusEnum::RUNNING
                        | ContainerStateStatusEnum::PAUSED
                        | ContainerStateStatusEnum::RESTARTING
                )
            ));
            state.paused = Some(status == Some(ContainerStateStatusEnum::PAUSED));
            state.restarting = Some(status == Some(ContainerStateStatusEnum::RESTARTING));
            state.dead = Some(status == Some(ContainerStateStatusEnum::DEAD));
            let mut inventory = Inventory::default();
            inventory.include_container(&["/old".into()], &[45283], listed, Some(&state));
            assert!(
                check_collision(&inventory, "fresh", 45283).is_err(),
                "{listed:?}"
            );
            assert!(check_collision(&inventory, "old", 1234).is_err());
        }
    }

    #[test]
    fn prepare_projection_missing_or_contradictory_exit_evidence_fails_closed() {
        let mut states = vec![None, Some(ContainerState::default())];
        for field in ["running", "paused", "restarting", "dead"] {
            for value in [None, Some(true)] {
                let mut state = exited_state();
                match field {
                    "running" => state.running = value,
                    "paused" => state.paused = value,
                    "restarting" => state.restarting = value,
                    _ => state.dead = value,
                }
                states.push(Some(state));
            }
        }
        for state in states {
            let mut inventory = Inventory::default();
            inventory.include_container(&["/old".into()], &[45283], Some("exited"), state.as_ref());
            assert!(check_collision(&inventory, "fresh", 45283).is_err());
            assert!(check_collision(&inventory, "old", 1234).is_err());
        }
    }

    #[test]
    fn foreign_collisions_are_stop_not_ownership() {
        assert!(check_collision(
            &Inventory {
                names: vec!["/foreign".into()],
                ports: vec![5432]
            },
            "foreign",
            1234
        )
        .is_err());
        assert!(check_collision(
            &Inventory {
                names: vec![],
                ports: vec![5432]
            },
            "new",
            5432
        )
        .is_err());
        assert!(check_collision(&Inventory::default(), "new", 1234).is_ok());
    }
    #[test]
    fn endpoint_propagates_host_and_port_without_bridge_loopback() {
        let endpoint = Endpoint::parse("bridge", Some("runner-gateway.test")).unwrap();
        assert_eq!(
            endpoint.http(4567).unwrap(),
            "http://runner-gateway.test:4567"
        );
        assert_eq!(
            endpoint.authority(9092).unwrap(),
            "runner-gateway.test:9092"
        );
        assert_eq!(
            endpoint
                .rewrite_url("http://localhost:4566/000000000000/queue", 4567)
                .unwrap(),
            "http://runner-gateway.test:4567/000000000000/queue"
        );
        for host in [
            "",
            "http://host",
            "host:123",
            "localhost",
            "127.0.0.1",
            "host/path",
        ] {
            assert!(Endpoint::parse("bridge", Some(host)).is_err());
        }
        assert!(Endpoint::parse("bridge", None).is_err());
        assert!(endpoint.http(0).is_err());
        assert_eq!(Endpoint::parse("local", None).unwrap().host, "127.0.0.1");
    }
    #[tokio::test]
    async fn cancelled_waiter_retains_creation_and_supervisor_joins_exact_result() {
        let (send, receive) = tokio::sync::oneshot::channel::<String>();
        let cell = Arc::new(tokio::sync::Mutex::new(Some(tokio::spawn(async move {
            receive.await.unwrap()
        }))));
        let waiter_cell = cell.clone();
        let waiter = tokio::spawn(async move { await_retained(&waiter_cell).await });
        tokio::task::yield_now().await;
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());
        assert!(cell.lock().await.is_some());
        send.send("actual-create-id".into()).unwrap();
        assert_eq!(
            await_retained(&cell).await.unwrap(),
            Some("actual-create-id".into())
        );
        assert!(cell.lock().await.is_none());
    }
    #[tokio::test]
    async fn opaque_failure_and_panic_expose_no_fabricated_identity() {
        let cell =
            tokio::sync::Mutex::new(Some(tokio::spawn(async { Err::<String, _>("opaque") })));
        assert_eq!(await_retained(&cell).await.unwrap(), Some(Err("opaque")));
        let cell = tokio::sync::Mutex::new(Some(tokio::spawn(async {
            panic!("pure creation fake");
            #[allow(unreachable_code)]
            String::new()
        })));
        assert!(await_retained(&cell).await.unwrap_err().is_panic());
        assert!(cell.lock().await.is_none());
    }
    #[tokio::test(flavor = "current_thread")]
    async fn pending_creation_releases_unrelated_fixture_then_reconciles_exact_result() {
        // Poll once, not through a timer: a waiting reconciliation is a
        // deterministic failure, not a hung test requiring an external timeout.
        async fn immediate<F: Future>(future: F) -> F::Output {
            tokio::pin!(future);
            std::future::poll_fn(|cx| match future.as_mut().poll(cx) {
                std::task::Poll::Ready(result) => std::task::Poll::Ready(result),
                std::task::Poll::Pending => panic!("reconciliation waited on a pending/busy task"),
            })
            .await
        }
        fn unexpected_release(_: String) -> std::future::Ready<Result<(), String>> {
            panic!("no completed creation may be released in this pass")
        }

        let (release_creation, creation_barrier) = tokio::sync::oneshot::channel::<String>();
        let (creation_finished, finished_creation) = tokio::sync::oneshot::channel();
        let creation = tokio::sync::Mutex::new(Some(tokio::spawn(async move {
            let id = creation_barrier.await.unwrap();
            creation_finished.send(()).unwrap();
            Ok::<_, String>(id)
        })));
        let removal = tokio::sync::Mutex::new(None);
        let mut caller = Box::pin(await_retained(&creation));
        std::future::poll_fn(|cx| {
            assert!(caller.as_mut().poll(cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        assert!(matches!(
            immediate(reconcile_retained(&creation)).await,
            Reconciliation::Busy
        ));
        // Cancellation drops only the awaiting caller, never the creation task.
        drop(caller);
        assert!(creation.try_lock().unwrap().is_some());

        let released = Arc::new(Mutex::new(Vec::<String>::new()));
        let mut unrelated = Some(Arc::new("unrelated-created-id".to_string()));
        let cleanup = async {
            let before = reconcile_orphan(&creation, &removal, unexpected_release).await;
            let eligible = take_unshared(&mut unrelated).unwrap().unwrap();
            released.lock().unwrap().push(eligible);
            let after = reconcile_orphan(&creation, &removal, unexpected_release).await;
            (before, after)
        };
        assert_eq!(
            immediate(cleanup).await,
            (OrphanReconciliation::Pending, OrphanReconciliation::Pending)
        );
        assert!(unrelated.is_none());
        assert_eq!(*released.lock().unwrap(), vec!["unrelated-created-id"]);
        assert!(creation.try_lock().unwrap().is_some());

        release_creation.send("actual-created-id".into()).unwrap();
        // On this current-thread runtime the task returns in the same poll as
        // this signal; no sleeps, scheduler guesses or daemon are involved.
        finished_creation.await.unwrap();
        let (release_removal, removal_barrier) = tokio::sync::oneshot::channel();
        let (removal_finished, finished_removal) = tokio::sync::oneshot::channel();
        let release_log = released.clone();
        let joined = immediate(reconcile_orphan(&creation, &removal, |id| async move {
            removal_barrier.await.unwrap();
            release_log.lock().unwrap().push(id);
            removal_finished.send(()).unwrap();
            Ok(())
        }))
        .await;
        assert_eq!(joined, OrphanReconciliation::Pending);
        assert!(creation.try_lock().unwrap().is_none());
        assert!(removal.try_lock().unwrap().is_some());
        assert_eq!(
            immediate(reconcile_orphan(&creation, &removal, unexpected_release)).await,
            OrphanReconciliation::Pending
        );
        // Another drain holding the cell is also reported, never awaited.
        let busy_removal = removal.try_lock().unwrap();
        assert_eq!(
            immediate(reconcile_orphan(&creation, &removal, unexpected_release)).await,
            OrphanReconciliation::Pending
        );
        drop(busy_removal);
        release_removal.send(()).unwrap();
        finished_removal.await.unwrap();
        assert_eq!(
            immediate(reconcile_orphan(&creation, &removal, unexpected_release)).await,
            OrphanReconciliation::Idle
        );
        assert_eq!(
            immediate(reconcile_orphan(&creation, &removal, unexpected_release)).await,
            OrphanReconciliation::Idle
        );
        assert!(removal.try_lock().unwrap().is_none());
        assert_eq!(
            *released.lock().unwrap(),
            vec!["unrelated-created-id", "actual-created-id"]
        );
    }
    #[test]
    fn active_clones_defer_and_unknown_ledger_ids_stay_null() {
        assert!(ensure_exact_owned_id("", None, false).is_err());
        assert!(ensure_exact_owned_id("created-id", Some("created-id"), false).is_ok());
        assert!(ensure_exact_owned_id("created-id", None, false).is_err());
        assert!(ensure_exact_owned_id("created-id", Some("name-match"), false).is_err());
        assert!(ensure_exact_owned_id("created-id", Some("created-id"), true).is_err());
        let lease = Arc::new("actual-created-id");
        let consumer = lease.clone();
        let mut singleton = Some(lease);
        let retained = take_unshared(&mut singleton).unwrap_err();
        assert!(singleton.is_some());
        assert_eq!(Arc::strong_count(&retained), 3);
        drop(retained);
        drop(consumer);
        assert_eq!(
            take_unshared(&mut singleton).unwrap(),
            Some("actual-created-id")
        );
        assert!(singleton.is_none());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.jsonl");
        let context = Context {
            run: "pure-test".into(),
            endpoint: Endpoint::parse("local", None).unwrap(),
            ledger: Mutex::new(std::fs::File::create(&path).unwrap()),
            ids: Mutex::new(HashMap::new()),
            unknown: AtomicBool::new(false),
        };
        context
            .event(1, "postgres", "diagnostic-only", None, "attempt", 5432)
            .unwrap();
        context.unknown(
            1,
            "postgres",
            "diagnostic-only",
            "unknown_opaque_start_failure",
            5432,
        );
        context
            .event(
                2,
                "postgres",
                "diagnostic-only",
                Some("actual-created-id"),
                "created",
                5432,
            )
            .unwrap();
        context
            .event(
                2,
                "postgres",
                "diagnostic-only",
                None,
                "creation_joined",
                5432,
            )
            .unwrap();
        assert!(context
            .event(
                2,
                "postgres",
                "diagnostic-only",
                Some("different-id"),
                "created",
                5432
            )
            .is_err());
        let events: Vec<serde_json::Value> = std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert!(events[0]["container_id"].is_null());
        assert!(events[1]["container_id"].is_null());
        assert_eq!(events[2]["container_id"], "actual-created-id");
        assert_eq!(events[3]["container_id"], "actual-created-id");
        assert!(context.unknown.load(Ordering::SeqCst));
    }
}
