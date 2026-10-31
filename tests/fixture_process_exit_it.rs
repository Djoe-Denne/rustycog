//! Real Windows boundary proof, separate from the hermetic fixture-runtime target.
//!
//! Run only in the native IT phase under the parent's Docker lease and creation
//! registry watcher (RUSTYCOG_TEST_RUNNER_MODE/RUN_ID/LEDGER_DIR). On Windows,
//! std::process::exit bypasses CRT atexit: Engine must still report running.
//! The parent ALWAYS performs its own exact-ID graceful stop before asserting.
//! This is the same compensating ownership boundary as the campaign F1 finally;
//! this test never adopts inventory/name matches and never removes containers.
#![cfg(all(feature = "testing", windows))]

use rustycog_framework::testing::{
    common::start_service_testcontainer,
    testcontainers::{self, runners::AsyncRunner, GenericImage, ImageExt},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    io::Write,
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use testcontainers::bollard::{container::StopContainerOptions, errors::Error, Docker};

const CHILD_FLAG: &str = "RUSTYCOG_REAL_EXIT_CHILD";

#[test]
fn child_creates_retained_container_then_exits() {
    if std::env::var(CHILD_FLAG).is_err() {
        return;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        // No service/config under test: this port is exclusively the disposable
        // lifecycle probe's host binding, never published to application env.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let name = format!("exit-it-{}", std::process::id());
        let image = GenericImage::new("redis", "7-alpine")
            .with_wait_for(testcontainers::core::WaitFor::message_on_stdout(
                "Ready to accept connections",
            ))
            .with_container_name(&name)
            .with_mapped_port(port, testcontainers::core::ContainerPort::Tcp(6379));
        let container = start_service_testcontainer(image.start(), "exit-it", &name, port)
            .await
            .unwrap();
        container.ready().unwrap();
        // Model a process singleton: no normal handle Drop before process exit.
        // The parent owns cleanup using this creation's durable exact-ID ledger.
        std::mem::forget(container);
    });
    drop(runtime);
    std::process::exit(7);
}

fn snapshot_child_ledgers(dir: &Path) -> Result<BTreeSet<OsString>, String> {
    std::fs::read_dir(dir)
        .map_err(|_| "cannot snapshot child ledger directory")?
        .map(|entry| {
            entry
                .map(|entry| entry.file_name())
                .map_err(|_| "cannot snapshot child ledger entry".into())
        })
        .collect()
}

fn find_new_child_ledger(
    dir: &Path,
    run: &str,
    pid: u32,
    before: &BTreeSet<OsString>,
) -> Result<PathBuf, String> {
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    let prefix = format!("fixture-{run}-{pid}-");
    let mut candidate = None;
    for entry in std::fs::read_dir(dir).map_err(|_| "cannot discover child ledger directory")? {
        let entry = entry.map_err(|_| "cannot discover child ledger entry")?;
        let name = entry.file_name();
        if before.contains(&name) {
            continue;
        }
        let Some(incarnation) = name
            .to_str()
            .and_then(|name| name.strip_prefix(prefix.as_str()))
            .and_then(|name| name.strip_suffix(".jsonl"))
        else {
            continue;
        };
        if !uuid::Uuid::parse_str(incarnation)
            .is_ok_and(|id| id.simple().to_string() == incarnation)
        {
            continue;
        }
        let path = entry.path();
        let metadata =
            std::fs::symlink_metadata(&path).map_err(|_| "cannot inspect child ledger file")?;
        if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err("child ledger must be a regular non-reparse file".into());
        }
        if candidate.replace(path).is_some() {
            return Err("multiple new child ledgers; none adopted".into());
        }
    }
    candidate.ok_or_else(|| "new child ledger missing; no older ledger adopted".into())
}

fn read_creations(path: &Path, run: &str, pid: u32) -> (Vec<serde_json::Value>, Vec<String>) {
    let mut errors = Vec::new();
    let mut creations = BTreeMap::new();
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(_) => return (Vec::new(), vec!["child ledger missing/unreadable".into()]),
    };
    for line in text.lines() {
        let Ok(record) = serde_json::from_str::<serde_json::Value>(line) else {
            errors.push("child ledger has an invalid record".into());
            continue;
        };
        if record["run_id"] != run || record["process_id"].as_u64() != Some(u64::from(pid)) {
            errors.push("child ledger identity mismatch; record not adopted".into());
            continue;
        }
        if record["event"] == "process_exit_stop_requested" {
            errors.push("Windows explicit exit unexpectedly ran the CRT hook".into());
        }
        if record["event"] != "created" {
            continue;
        }
        let Some(id) = record["container_id"].as_str() else {
            errors.push("child creation ID unknown".into());
            continue;
        };
        if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            errors.push("child creation ID not full hex; no fallback".into());
            continue;
        }
        creations.insert(id.to_string(), record);
    }
    (creations.into_values().collect(), errors)
}

async fn inspect_owned(docker: &Docker, id: &str) -> Result<Option<bool>, String> {
    let inspect =
        match tokio::time::timeout(Duration::from_secs(5), docker.inspect_container(id, None))
            .await
            .map_err(|_| "Engine inspect timed out")?
        {
            Ok(inspect) => inspect,
            Err(Error::DockerResponseServerError {
                status_code: 404, ..
            }) => return Ok(None),
            Err(_) => return Err("Engine inspect failed".into()),
        };
    if inspect.id.as_deref() != Some(id) {
        return Err("Engine exact-ID mismatch; no fallback".into());
    }
    inspect
        .state
        .and_then(|state| state.running)
        .map(Some)
        .ok_or_else(|| "Engine running state missing".into())
}

async fn stop_owned_child(docker: &Docker, id: &str) -> Result<(), String> {
    match inspect_owned(docker, id).await? {
        None | Some(false) => return Ok(()),
        Some(true) => {}
    }
    tokio::time::timeout(
        Duration::from_secs(15),
        docker.stop_container(id, Some(StopContainerOptions { t: 10 })),
    )
    .await
    .map_err(|_| "owned child stop timed out")?
    .map_err(|_| "owned child stop failed")?;
    match inspect_owned(docker, id).await? {
        None | Some(false) => Ok(()),
        Some(true) => Err("owned child remains running after stop".into()),
    }
}

#[tokio::test]
async fn windows_explicit_exit_bypasses_guard_but_parent_finally_stops_exact_child_id() {
    assert_eq!(std::env::var("RUSTYCOG_TEST_RUNNER_MODE").unwrap(), "local");
    let run = std::env::var("RUSTYCOG_TEST_RUN_ID").unwrap();
    let dir = std::path::PathBuf::from(std::env::var("RUSTYCOG_TEST_LEDGER_DIR").unwrap());
    assert!(
        dir.is_absolute() && dir.is_dir(),
        "parent-approved ledger directory required"
    );
    let before = snapshot_child_ledgers(&dir).expect("pre-spawn ledger snapshot required");
    let docker = testcontainers::core::client::docker_client_instance()
        .await
        .unwrap();
    let mut child = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "child_creates_retained_container_then_exits",
            "--nocapture",
        ])
        .env(CHILD_FLAG, "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap();
    let mut errors = Vec::new();
    match tokio::time::timeout(Duration::from_secs(180), child.wait()).await {
        Ok(Ok(status)) if status.code() == Some(7) => {}
        Ok(_) => errors.push("child did not reach explicit exit(7)".into()),
        Err(_) => {
            errors.push("child creation/exit deadline exceeded".into());
            if child.kill().await.is_err() {
                errors.push("child termination unverified".into());
            }
        }
    }
    // No assertions/panics between discovering creation IDs and the finally
    // cleanup below. Errors are accumulated so a failing oracle cannot leak.
    let (path, creations) = match find_new_child_ledger(&dir, &run, pid, &before) {
        Ok(path) => {
            let (creations, ledger_errors) = read_creations(&path, &run, pid);
            errors.extend(ledger_errors);
            (Some(path), creations)
        }
        Err(error) => {
            errors.push(error);
            (None, Vec::new())
        }
    };
    if creations.len() != 1 {
        errors.push("expected one exact-ID child creation".into());
    }
    for record in &creations {
        let id = record["container_id"].as_str().unwrap(); // validated by read_creations
        match inspect_owned(&docker, id).await {
            Ok(Some(true)) => {}
            _ => errors.push("Engine did not witness retained running child container".into()),
        }
    }
    // finally: own ONLY IDs returned by this child creation, even on failure.
    for mut record in creations {
        let id = record["container_id"].as_str().unwrap().to_string();
        match stop_owned_child(&docker, &id).await {
            Ok(()) => {
                record["event"] = "process_exit_it_parent_stopped".into();
                record["parent_process_id"] = std::process::id().into();
                let persisted = (|| -> std::io::Result<()> {
                    let path = path.as_ref().ok_or_else(|| {
                        std::io::Error::other("validated child ledger path unavailable")
                    })?;
                    let mut file = std::fs::OpenOptions::new().append(true).open(path)?;
                    serde_json::to_writer(&mut file, &record)?;
                    file.write_all(b"\n")?;
                    file.sync_data()
                })();
                if persisted.is_err() {
                    errors.push("parent stop ledger append failed".into());
                }
            }
            Err(error) => errors.push(format!("cleanup INCONCLUSIVE id={id}: {error}")),
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("; "));
}

#[cfg(test)]
mod ledger_tests {
    use super::*;

    fn ledger_path(dir: &Path, run: &str, pid: u32, incarnation: u128) -> PathBuf {
        dir.join(format!(
            "fixture-{run}-{pid}-{}.jsonl",
            uuid::Uuid::from_u128(incarnation).simple()
        ))
    }

    fn creation_record(run: &str, pid: u32, id: &str) -> serde_json::Value {
        serde_json::json!({
            "schema_version": 1,
            "run_id": run,
            "process_id": pid,
            "event": "created",
            "container_id": id,
        })
    }

    #[test]
    fn child_ledger_discovery_excludes_pre_spawn_reused_pid_ledgers() {
        let dir = tempfile::tempdir().unwrap();
        let run = "reused-pid";
        let pid = 42;
        let old_record = creation_record(run, pid, &"a".repeat(64)).to_string();
        let legacy = dir.path().join(format!("fixture-{run}-{pid}.jsonl"));
        let old = ledger_path(dir.path(), run, pid, 1);
        std::fs::write(&legacy, &old_record).unwrap();
        std::fs::write(&old, &old_record).unwrap();
        let before = snapshot_child_ledgers(dir.path()).unwrap();
        assert!(find_new_child_ledger(dir.path(), run, pid, &before).is_err());
        let fresh = ledger_path(dir.path(), run, pid, 2);
        let new_id = "b".repeat(64);
        std::fs::write(&fresh, creation_record(run, pid, &new_id).to_string()).unwrap();
        let selected = find_new_child_ledger(dir.path(), run, pid, &before).unwrap();
        assert_eq!(selected, fresh);
        let (creations, errors) = read_creations(&selected, run, pid);
        assert!(errors.is_empty());
        assert_eq!(creations.len(), 1);
        assert_eq!(creations[0]["container_id"], new_id);
        assert_eq!(std::fs::read_to_string(legacy).unwrap(), old_record);
        assert_eq!(std::fs::read_to_string(old).unwrap(), old_record);
    }

    #[test]
    fn child_ledger_discovery_rejects_missing_or_multiple_new_ledgers() {
        let dir = tempfile::tempdir().unwrap();
        let before = snapshot_child_ledgers(dir.path()).unwrap();
        assert!(find_new_child_ledger(dir.path(), "scope", 42, &before).is_err());
        for incarnation in [1, 2] {
            std::fs::write(ledger_path(dir.path(), "scope", 42, incarnation), "").unwrap();
        }
        assert!(find_new_child_ledger(dir.path(), "scope", 42, &before).is_err());
        let non_file = tempfile::tempdir().unwrap();
        let before = snapshot_child_ledgers(non_file.path()).unwrap();
        std::fs::create_dir(ledger_path(non_file.path(), "scope", 42, 1)).unwrap();
        assert!(find_new_child_ledger(non_file.path(), "scope", 42, &before).is_err());
    }

    #[test]
    fn child_ledger_discovery_matches_exact_run_and_pid() {
        let dir = tempfile::tempdir().unwrap();
        let before = snapshot_child_ledgers(dir.path()).unwrap();
        for (run, pid) in [("scope-extra", 42), ("scope", 420)] {
            std::fs::write(ledger_path(dir.path(), run, pid, 1), "").unwrap();
        }
        for name in [
            "fixture-scope-42-not-a-uuid.jsonl",
            "fixture-scope-42-00000000-0000-0000-0000-000000000001.jsonl",
            "fixture-scope-42-00000000000000000000000000000001.jsonl.extra",
        ] {
            std::fs::write(dir.path().join(name), "").unwrap();
        }
        assert!(find_new_child_ledger(dir.path(), "scope", 42, &before).is_err());
        let exact = ledger_path(dir.path(), "scope", 42, 2);
        std::fs::write(&exact, "").unwrap();
        assert_eq!(
            find_new_child_ledger(dir.path(), "scope", 42, &before).unwrap(),
            exact
        );
    }

    #[test]
    fn child_ledger_records_reject_wrong_identity_and_invalid_created_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = ledger_path(dir.path(), "scope", 42, 1);
        let valid_id = "a".repeat(64);
        let valid = creation_record("scope", 42, &valid_id);
        std::fs::write(&path, valid.to_string()).unwrap();
        let (creations, errors) = read_creations(&path, "scope", 42);
        assert_eq!(creations.len(), 1);
        assert!(errors.is_empty());
        let mut invalid = vec![
            creation_record("other-scope", 42, &valid_id).to_string(),
            creation_record("scope", 420, &valid_id).to_string(),
            creation_record("scope", 42, "abc").to_string(),
            creation_record("scope", 42, &"z".repeat(64)).to_string(),
            "not-json".into(),
        ];
        let mut no_id = valid;
        no_id["container_id"] = serde_json::Value::Null;
        invalid.push(no_id.to_string());
        for text in invalid {
            std::fs::write(&path, text).unwrap();
            let (creations, errors) = read_creations(&path, "scope", 42);
            assert!(creations.is_empty());
            assert!(!errors.is_empty());
        }
    }
}
