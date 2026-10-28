//! Process-lifetime stop guard. Statics are not dropped at process exit.
//!
//! The dedicated thread owns its Docker client AND Tokio reactor from before
//! creation until exit; it never reuses a test's already-shut-down reactor.
//! Only IDs returned by creation are enrolled. No discovery, adoption or rm.
//!
//! This is a CRT `atexit` guard, not an out-of-process supervisor. On Windows,
//! Rust's `std::process::exit` bypasses CRT callbacks (including libtest's
//! failure exit). Abort/forced termination cannot run this guard either. The
//! campaign runner MUST reconcile creation ledgers against Engine inventory and
//! stop remaining owned IDs in its `finally`, on success AND failure. A timeout
//! or INCONCLUSIVE event likewise requires that compensating cleanup.
use super::{Attempt, Context};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex, OnceLock,
    },
    time::Duration,
};
use testcontainers::bollard::{container::StopContainerOptions, errors::Error, Docker};

const STOP_TIMEOUT: Duration = Duration::from_secs(15);
const EXIT_TIMEOUT: Duration = Duration::from_secs(20);
static GUARD: OnceLock<Result<Guard, String>> = OnceLock::new();

pub(super) struct Record {
    context: Arc<Context>,
    number: u64,
    role: String,
    name: String,
    port: u16,
    id: String,
    removed: AtomicBool,
}

impl Record {
    fn new(attempt: &Attempt, id: &str) -> Result<Self, String> {
        if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("process exit guard requires a full creation-returned container ID".into());
        }
        Ok(Self {
            context: attempt.context.clone(),
            number: attempt.number,
            role: attempt.role.clone(),
            name: attempt.name.clone(),
            port: attempt.port,
            id: id.into(),
            removed: AtomicBool::new(false),
        })
    }

    pub(super) fn removed(&self) {
        self.removed.store(true, Ordering::SeqCst);
    }

    fn event(&self, event: &str) -> Result<(), String> {
        self.context.event(
            self.number,
            &self.role,
            &self.name,
            Some(&self.id),
            event,
            self.port,
        )
    }
}

struct Guard {
    records: Mutex<Vec<Arc<Record>>>,
    signal: Mutex<Option<tokio::sync::oneshot::Sender<Vec<Arc<Record>>>>>,
    completion: Mutex<mpsc::Receiver<Result<(), String>>>,
}

impl Guard {
    fn new() -> Result<Self, String> {
        let guard = Self::with_worker(|signal_rx, ready_tx| async move {
            let docker = tokio::time::timeout(
                Duration::from_secs(5),
                testcontainers::core::client::docker_client_instance(),
            )
            .await
            .map_err(|_| "process exit Docker client timed out")?
            .map_err(|_| "process exit Docker client unavailable")?;
            ready_tx
                .send(Ok(()))
                .map_err(|_| "process exit guard caller gone")?;
            let records = signal_rx
                .await
                .map_err(|_| "process exit guard signal lost")?;
            stop_records(records, |record| stop_exact(&docker, record)).await
        })?;
        // SAFETY: the callback has C ABI, catches unwind, and uses process-lived
        // state only. Register before any enrolled container can be created.
        if unsafe { libc::atexit(on_exit) } != 0 {
            return Err("cannot register process exit guard".into());
        }
        Ok(guard)
    }

    fn with_worker<F, Fut>(worker: F) -> Result<Self, String>
    where
        F: FnOnce(
                tokio::sync::oneshot::Receiver<Vec<Arc<Record>>>,
                mpsc::SyncSender<Result<(), String>>,
            ) -> Fut
            + Send
            + 'static,
        Fut: std::future::Future<Output = Result<(), String>>,
    {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (signal_tx, signal_rx) = tokio::sync::oneshot::channel();
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("fixture-process-exit".into())
            .spawn(move || {
                let result = (|| {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|_| "process exit guard runtime unavailable".to_string())?;
                    let result = runtime.block_on(worker(signal_rx, ready_tx.clone()));
                    runtime.shutdown_timeout(Duration::from_secs(1));
                    result
                })();
                // Also unblocks initialization on any error before ready.
                if let Err(ref error) = result {
                    let _ = ready_tx.send(Err(error.clone()));
                }
                let _ = done_tx.send(result);
            })
            .map_err(|_| "cannot start process exit guard thread")?;
        ready_rx
            .recv_timeout(Duration::from_secs(7))
            .map_err(|_| "process exit guard initialization timed out")??;
        Ok(Self {
            records: Mutex::new(Vec::new()),
            signal: Mutex::new(Some(signal_tx)),
            completion: Mutex::new(done_rx),
        })
    }

    fn finish(&self) -> Result<(), String> {
        let records = self
            .records
            .try_lock()
            .map_err(|_| "process exit registry busy or poisoned")?
            .clone();
        let sender = self
            .signal
            .try_lock()
            .map_err(|_| "process exit signal busy or poisoned")?
            .take()
            .ok_or("process exit guard already consumed")?;
        sender
            .send(records)
            .map_err(|_| "process exit guard thread unavailable")?;
        self.completion
            .try_lock()
            .map_err(|_| "process exit completion busy or poisoned")?
            .recv_timeout(EXIT_TIMEOUT)
            .map_err(|_| "process exit stop deadline exceeded")?
    }
}

extern "C" fn on_exit() {
    // Never unwind across C, and do not depend on tracing's shutdown order.
    let result = std::panic::catch_unwind(|| -> Result<(), String> {
        let unresolved = super::process_exit_creations_unresolved();
        GUARD
            .get()
            .ok_or("process exit guard missing".to_string())?
            .as_ref()
            .map_err(Clone::clone)?
            .finish()?;
        if unresolved {
            Err("process exit creations unresolved".into())
        } else {
            Ok(())
        }
    });
    if !matches!(result, Ok(Ok(()))) {
        eprintln!(
            "fixture process exit cleanup INCONCLUSIVE; reconcile exact-ID ledgers with Docker"
        );
    }
}

pub(super) fn arm() -> Result<(), String> {
    GUARD
        .get_or_init(Guard::new)
        .as_ref()
        .map(|_| ())
        .map_err(Clone::clone)
}

pub(super) fn track(attempt: &Attempt, id: &str) -> Result<Arc<Record>, String> {
    let record = Arc::new(Record::new(attempt, id)?);
    let guard = GUARD
        .get_or_init(Guard::new)
        .as_ref()
        .map_err(Clone::clone)?;
    let mut records = guard
        .records
        .lock()
        .map_err(|_| "process exit registry poisoned")?;
    if records.iter().any(|existing| existing.id == id) {
        return Err("fixture process exit ID already enrolled".into());
    }
    records.push(record.clone());
    Ok(record)
}

async fn stop_records<F, Fut>(records: Vec<Arc<Record>>, stop: F) -> Result<(), String>
where
    F: Fn(Arc<Record>) -> Fut,
    Fut: std::future::Future<Output = Result<&'static str, String>>,
{
    stop_records_with_timeout(records, stop, STOP_TIMEOUT).await
}

async fn stop_records_with_timeout<F, Fut>(
    records: Vec<Arc<Record>>,
    stop: F,
    timeout: Duration,
) -> Result<(), String>
where
    F: Fn(Arc<Record>) -> Fut,
    Fut: std::future::Future<Output = Result<&'static str, String>>,
{
    let results: Vec<Result<(), String>> =
        futures::future::join_all(records.into_iter().map(|record| {
            let stop = &stop;
            async move {
                if record.removed.load(Ordering::SeqCst) {
                    return Ok(());
                }
                // A broken ledger is INCONCLUSIVE, not a reason to leave a known,
                // creation-returned ID running. Attempt its safe stop regardless.
                let requested = record.event("process_exit_stop_requested");
                let outcome = tokio::time::timeout(timeout, stop(record.clone())).await;
                let event = match outcome {
                    Ok(Ok(event)) => event,
                    _ => {
                        record.context.unknown.store(true, Ordering::SeqCst);
                        "process_exit_stop_failed"
                    }
                };
                let recorded = record.event(event);
                if requested.is_err() || recorded.is_err() {
                    record.context.unknown.store(true, Ordering::SeqCst);
                }
                if record.context.unknown.load(Ordering::SeqCst) {
                    Err("process exit cleanup INCONCLUSIVE".into())
                } else {
                    Ok(())
                }
            }
        }))
        .await;
    if results.into_iter().any(|result| result.is_err()) {
        Err("process exit cleanup INCONCLUSIVE".into())
    } else {
        Ok(())
    }
}

async fn stop_exact(docker: &Docker, record: Arc<Record>) -> Result<&'static str, String> {
    let inspect = match docker.inspect_container(&record.id, None).await {
        Ok(inspect) => inspect,
        Err(Error::DockerResponseServerError {
            status_code: 404, ..
        }) => return Ok("process_exit_absent"),
        Err(_) => return Err("process exit inspect failed".into()),
    };
    if inspect.id.as_deref() != Some(&record.id) {
        return Err("process exit identity mismatch; no fallback".into());
    }
    if !confirmed_stopped(inspect.state.as_ref()) {
        docker
            .stop_container(&record.id, Some(StopContainerOptions { t: 10 }))
            .await
            .map_err(|_| "process exit stop failed")?;
    }
    let after = docker
        .inspect_container(&record.id, None)
        .await
        .map_err(|_| "process exit stop verification failed")?;
    if after.id.as_deref() != Some(&record.id) || !confirmed_stopped(after.state.as_ref()) {
        return Err("process exit stop unverified".into());
    }
    Ok("process_exit_stopped")
}

fn confirmed_stopped(state: Option<&testcontainers::bollard::models::ContainerState>) -> bool {
    use testcontainers::bollard::models::ContainerStateStatusEnum;
    state.is_some_and(|state| {
        state.status == Some(ContainerStateStatusEnum::EXITED)
            && state.running == Some(false)
            && state.paused == Some(false)
            && state.restarting == Some(false)
            && state.dead == Some(false)
    })
}

#[cfg(test)]
mod tests {
    use super::super::Endpoint;
    use super::*;
    use std::{collections::HashMap, path::Path};

    fn records(path: &Path, roles: &[&str]) -> Vec<Arc<Record>> {
        let context = Arc::new(Context {
            run: "exit-unit".into(),
            endpoint: Endpoint {
                host: "127.0.0.1".into(),
            },
            ledger: Mutex::new(
                std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(path)
                    .unwrap(),
            ),
            ids: Mutex::new(HashMap::new()),
            unknown: AtomicBool::new(false),
        });
        roles
            .iter()
            .enumerate()
            .map(|(number, role)| {
                let attempt = Attempt {
                    context: context.clone(),
                    number: number as u64,
                    role: (*role).into(),
                    name: format!("diagnostic-{role}"),
                    port: 1234,
                };
                let record =
                    Arc::new(Record::new(&attempt, &format!("{:064x}", number + 1)).unwrap());
                record.event("created").unwrap();
                record
            })
            .collect()
    }

    fn events(path: &Path) -> Vec<serde_json::Value> {
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[tokio::test]
    async fn all_container_roles_stop_by_exact_id_and_removed_is_not_repeated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger");
        let records = records(
            &path,
            &[
                "postgres", "sqs", "kafka", "openfga", "redis", "openbao", "smtp",
            ],
        );
        let calls = Mutex::new(Vec::new());
        records[0].removed();
        stop_records(records.clone(), |record| {
            calls.lock().unwrap().push(record.id.clone());
            std::future::ready(Ok("process_exit_stopped"))
        })
        .await
        .unwrap();
        assert_eq!(
            *calls.lock().unwrap(),
            records[1..]
                .iter()
                .map(|r| r.id.clone())
                .collect::<Vec<_>>()
        );
        assert!(calls.lock().unwrap().iter().all(|id| id.len() == 64));
        let ledger = events(&path);
        assert_eq!(
            ledger
                .iter()
                .filter(|e| e["event"] == "process_exit_stopped")
                .count(),
            6
        );
        assert!(!ledger.iter().any(|e| e["event"] == "removed"));
        assert!(ledger
            .iter()
            .all(|e| e["container_id"].as_str().unwrap().len() == 64));
    }

    #[tokio::test]
    async fn failure_or_timeout_is_inconclusive_without_skipping_unrelated_stop() {
        for timeout in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("ledger");
            let records = records(&path, &["postgres", "openfga"]);
            let result = stop_records_with_timeout(
                records.clone(),
                |record| async move {
                    if record.role == "postgres" {
                        if timeout {
                            std::future::pending::<()>().await;
                        }
                        Err("injected stop failure".into())
                    } else {
                        Ok("process_exit_stopped")
                    }
                },
                Duration::from_millis(10),
            )
            .await;
            assert!(result.unwrap_err().contains("INCONCLUSIVE"));
            assert!(records[0].context.unknown.load(Ordering::SeqCst));
            let ledger = events(&path);
            assert!(ledger.iter().any(
                |e| e["fixture_role"] == "postgres" && e["event"] == "process_exit_stop_failed"
            ));
            assert!(ledger
                .iter()
                .any(|e| e["fixture_role"] == "openfga" && e["event"] == "process_exit_stopped"));
        }
    }

    #[tokio::test]
    async fn stopping_known_id_does_not_clear_prior_unknown_creation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger");
        let records = records(&path, &["postgres"]);
        records[0].context.unknown.store(true, Ordering::SeqCst);
        assert!(
            stop_records(records, |_| std::future::ready(Ok("process_exit_stopped")))
                .await
                .is_err()
        );
        assert_eq!(
            events(&path).last().unwrap()["event"],
            "process_exit_stopped"
        );
    }

    #[tokio::test]
    async fn broken_ledger_is_loud_but_does_not_suppress_known_id_stop() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger");
        let records = records(&path, &["postgres"]);
        let context = records[0].context.clone();
        let poison = context.clone();
        assert!(std::thread::spawn(move || {
            let _ledger = poison.ledger.lock().unwrap();
            panic!("inject ledger poison");
        })
        .join()
        .is_err());
        let called = AtomicBool::new(false);
        let result = stop_records(records, |_| {
            called.store(true, Ordering::SeqCst);
            std::future::ready(Ok("process_exit_stopped"))
        })
        .await;
        assert!(called.load(Ordering::SeqCst));
        assert!(result.unwrap_err().contains("INCONCLUSIVE"));
        assert!(context.unknown.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn docker_stop_requires_exact_identity_and_verified_exit() {
        use wiremock::{
            matchers::{method, path_regex, query_param},
            Mock, MockServer, ResponseTemplate,
        };
        for mismatch in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("ledger");
            let record = records(&path, &["smtp"]).pop().unwrap();
            let server = MockServer::start().await;
            let stopped = Arc::new(AtomicBool::new(false));
            let state = stopped.clone();
            let id = if mismatch {
                "f".repeat(64)
            } else {
                record.id.clone()
            };
            Mock::given(method("GET"))
                .and(path_regex(format!(
                    "^(/v[0-9.]+)?/containers/{}/json$",
                    record.id
                )))
                .respond_with(move |_: &wiremock::Request| {
                    let stopped = state.load(Ordering::SeqCst);
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "Id":id,"State":{"Status":if stopped {"exited"} else {"running"},
                        "Running":!stopped,"Paused":false,"Restarting":false,"Dead":false}
                    }))
                })
                .expect(if mismatch { 1 } else { 2 })
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .and(path_regex(format!(
                    "^(/v[0-9.]+)?/containers/{}/stop$",
                    record.id
                )))
                .and(query_param("t", "10"))
                .respond_with(move |_: &wiremock::Request| {
                    stopped.store(true, Ordering::SeqCst);
                    ResponseTemplate::new(204)
                })
                .expect(if mismatch { 0 } else { 1 })
                .mount(&server)
                .await;
            let docker = Docker::connect_with_http(
                &server.uri(),
                2,
                testcontainers::bollard::API_DEFAULT_VERSION,
            )
            .unwrap();
            let result = stop_exact(&docker, record).await;
            if mismatch {
                assert!(result.unwrap_err().contains("identity mismatch"));
            } else {
                assert_eq!(
                    result.unwrap(),
                    "process_exit_stopped",
                    "requests: {:?}",
                    server
                        .received_requests()
                        .await
                        .unwrap()
                        .iter()
                        .map(|r| r.url.path())
                        .collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    fn child_exit_fixture() {
        let Ok(path) = std::env::var("RUSTYCOG_EXIT_UNIT_LEDGER") else {
            return;
        };
        let fail = std::env::var("RUSTYCOG_EXIT_UNIT_FAIL").is_ok();
        let records = records(Path::new(&path), &["postgres", "smtp"]);
        let guard = Guard::with_worker(move |signal, ready| async move {
            ready.send(Ok(())).unwrap();
            stop_records(signal.await.unwrap(), move |_| {
                std::future::ready(if fail {
                    Err("injected exit failure".into())
                } else {
                    Ok("process_exit_stopped")
                })
            })
            .await
        })
        .unwrap();
        *guard.records.lock().unwrap() = records;
        assert!(GUARD.set(Ok(guard)).is_ok());
        assert_eq!(unsafe { libc::atexit(on_exit) }, 0);
        if std::env::var("RUSTYCOG_EXIT_UNIT_NONZERO").is_ok() {
            std::process::exit(7);
        }
        // Normal libtest process exit happens AFTER this test's thread ends.
    }

    #[test]
    fn process_exit_contract_distinguishes_crt_exit_from_windows_explicit_exit() {
        // Hermetic lifecycle test: IDs/backend are synthetic, NOT Docker
        // evidence. The real campaign's mandatory F1 finally reconciles the
        // fixture ledger with Engine as its compensating control for ExitProcess.
        for (nonzero, fail) in [(false, false), (false, true), (true, false), (true, true)] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("ledger");
            let mut child = std::process::Command::new(std::env::current_exe().unwrap());
            child
                .args([
                    "--exact",
                    "fixture_runtime::process_exit::tests::child_exit_fixture",
                    "--nocapture",
                ])
                .env("RUSTYCOG_EXIT_UNIT_LEDGER", &path);
            if nonzero {
                child.env("RUSTYCOG_EXIT_UNIT_NONZERO", "1");
            }
            if fail {
                child.env("RUSTYCOG_EXIT_UNIT_FAIL", "1");
            }
            let output = child.output().unwrap();
            assert_eq!(output.status.code(), Some(if nonzero { 7 } else { 0 }));
            let ledger = events(&path);
            if cfg!(windows) && nonzero {
                // Executable Windows boundary: std::process::exit bypasses the
                // CRT hook, so the stop backend has not even been invoked.
                assert_eq!(ledger.len(), 2);
                assert!(ledger.iter().all(|e| e["event"] == "created"));
                assert!(!String::from_utf8_lossy(&output.stderr).contains("INCONCLUSIVE"));
                continue;
            }
            let event = if fail {
                "process_exit_stop_failed"
            } else {
                "process_exit_stopped"
            };
            assert_eq!(ledger.iter().filter(|e| e["event"] == event).count(), 2);
            assert_eq!(
                String::from_utf8_lossy(&output.stderr).contains("INCONCLUSIVE"),
                fail
            );
        }
    }

    #[test]
    fn unresolved_pending_creation_reports_inconclusive_composition() {
        // Composition guard×join (G-10, review T-2): a slot stuck in the
        // creation phase (active lease, nothing joined) at exit time must
        // report INCONCLUSIVE through process_exit_creations_unresolved —
        // never a clean stop. Synthetic IDs/ledger only; no Docker evidence.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger");
        let context = Arc::new(Context {
            run: "exit-unit-unresolved".into(),
            endpoint: Endpoint {
                host: "127.0.0.1".into(),
            },
            ledger: Mutex::new(
                std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&path)
                    .unwrap(),
            ),
            ids: Mutex::new(HashMap::new()),
            unknown: AtomicBool::new(false),
        });
        let slot = Arc::new(super::super::Slot {
            active: AtomicBool::new(true),
            task: tokio::sync::Mutex::new(None),
            teardown: tokio::sync::Mutex::new(None),
            context: context.clone(),
            number: 0,
            role: "postgres".into(),
            name: "unresolved-pending-creation".into(),
            port: 1234,
        });
        super::super::TASKS
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
            .unwrap()
            .push(slot);
        assert!(super::super::process_exit_creations_unresolved());
        assert!(context.unknown.load(std::sync::atomic::Ordering::SeqCst));
        let ledger = events(&path);
        assert!(ledger
            .iter()
            .any(|e| e["event"] == "process_exit_creation_unresolved"));
    }
}
