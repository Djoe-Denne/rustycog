//! PostgreSQL-specific create-response ownership; no name/event adoption.
use super::{
    container_handle::ContainerHandle, process_exit, start_diagnostic, Attempt, OwnedContainer,
    StartResult,
};
use rustycog::config::DatabaseConfig;
use std::{collections::HashMap, sync::Arc, time::Duration};
use testcontainers::{
    bollard::{
        container::{Config, CreateContainerOptions},
        errors::Error,
        models::{HostConfig, PortBinding},
        Docker,
    },
    core::ContainerPort,
    runners::AsyncRunner,
    GenericImage,
};

type Enroll = fn(&Attempt, &str) -> Result<Arc<process_exit::Record>, String>;

pub(super) async fn ensure_image(client: &Docker) -> Result<(), String> {
    match client.inspect_image("postgres:15-alpine").await {
        Ok(_) => Ok(()),
        Err(Error::DockerResponseServerError {
            status_code: 404, ..
        }) => {
            GenericImage::new("postgres", "15-alpine")
                .pull_image()
                .await
                .map_err(|error| start_diagnostic::describe(&error))?;
            Ok(())
        }
        Err(error) => Err(start_diagnostic::docker("inspect_image", &error)),
    }
}

fn request(config: &DatabaseConfig) -> Config<String> {
    Config {
        image: Some("postgres:15-alpine".into()),
        env: Some(vec![
            format!("POSTGRES_DB={}", config.db),
            format!("POSTGRES_USER={}", config.creds.username),
            format!("POSTGRES_PASSWORD={}", config.creds.password),
        ]),
        exposed_ports: Some(HashMap::from([("5432/tcp".into(), HashMap::new())])),
        host_config: Some(HostConfig {
            port_bindings: Some(HashMap::from([(
                "5432/tcp".into(),
                Some(vec![PortBinding {
                    host_ip: None,
                    // Docker interprets zero as daemon-owned dynamic allocation.
                    // Never call config.actual_port() before the daemon binds it.
                    host_port: Some(config.port.to_string()),
                }]),
            )])),
            ..Default::default()
        }),
        ..Default::default()
    }
}

pub(super) async fn start(attempt: Attempt, config: DatabaseConfig, client: Docker) -> StartResult {
    start_with_registration(attempt, config, client, process_exit::track).await
}

async fn start_with_registration(
    attempt: Attempt,
    config: DatabaseConfig,
    client: Docker,
    enroll: Enroll,
) -> StartResult {
    let created = match client
        .create_container(
            Some(CreateContainerOptions {
                name: attempt.name.clone(),
                platform: None,
            }),
            request(&config),
        )
        .await
    {
        Ok(response) => response,
        Err(error) => {
            attempt.context.unknown(
                attempt.number,
                &attempt.role,
                &attempt.name,
                "unknown_create_response_failure",
                attempt.port,
            );
            return Err(format!(
                "PostgreSQL create failed; {}",
                start_diagnostic::docker("create_container", &error)
            ));
        }
    };
    let id = created.id;
    if id.len() != 64 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        attempt.context.unknown(
            attempt.number,
            &attempt.role,
            &attempt.name,
            "unknown_create_response_id",
            attempt.port,
        );
        return Err("PostgreSQL create returned no valid full ID".into());
    }
    let handle = ContainerHandle::Engine {
        client: client.clone(),
        id: id.clone(),
    };
    // No await between the create response and installing exact-ID ownership.
    let exit_record = match enroll(&attempt, &id) {
        Ok(record) => record,
        Err(_) => {
            let _ = attempt.context.event(
                attempt.number,
                &attempt.role,
                &attempt.name,
                Some(&id),
                "created",
                attempt.port,
            );
            let cleanup = handle.rm().await;
            let _ = attempt.context.event(
                attempt.number,
                &attempt.role,
                &attempt.name,
                Some(&id),
                if cleanup.is_ok() {
                    "removed"
                } else {
                    "teardown_failed"
                },
                attempt.port,
            );
            return Err("PostgreSQL exit guard enrollment failed; start refused".into());
        }
    };
    let mut owned = OwnedContainer {
        container: Some(handle),
        attempt,
        id,
        removed_confirmed: false,
        exit_record,
        database_publication: None,
    };
    if owned
        .attempt
        .context
        .event(
            owned.attempt.number,
            &owned.attempt.role,
            &owned.attempt.name,
            Some(&owned.id),
            "created",
            owned.attempt.port,
        )
        .is_err()
    {
        return failed(
            owned,
            "PostgreSQL creation ledger failed; start refused".into(),
            "creation_ledger_failed",
        )
        .await;
    }
    let start = tokio::time::timeout(
        Duration::from_secs(60),
        client.start_container(
            &owned.id,
            None::<testcontainers::bollard::container::StartContainerOptions<String>>,
        ),
    )
    .await;
    let error = match start {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(start_diagnostic::docker("start_container", &error)),
        Err(_) => Some("stage=start_container reason=startup_timeout".into()),
    };
    if let Some(error) = error {
        return failed(
            owned,
            format!("recorded PostgreSQL start failed; {error}"),
            "recorded_start_failed",
        )
        .await;
    }
    let mapped = match owned.mapped_port(ContainerPort::Tcp(5432)).await {
        Ok(port) => port,
        Err(error) => return failed(owned, error, "mapped_port_failed").await,
    };
    owned.attempt.port = mapped;
    if config.port != 0 && config.port != mapped {
        return failed(
            owned,
            "PostgreSQL mapping contradicts configured port".into(),
            "mapped_port_failed",
        )
        .await;
    }
    // A retained creation can outlive a cancelled initializer. It owns the
    // container, not the singleton's configuration publication. The live caller
    // publishes only after joining this result while holding the singleton lock.
    Ok(owned)
}

pub(in crate::testing::common) async fn finish_readiness<F>(
    owned: OwnedContainer,
    readiness: F,
) -> StartResult
where
    F: std::future::Future<Output = Result<(), sea_orm::DbErr>>,
{
    if readiness.await.is_err() {
        return failed(
            owned,
            "stage=readiness reason=sql_unavailable".into(),
            "sql_readiness_failed",
        )
        .await;
    }
    if owned.ready().is_err() {
        return failed(
            owned,
            "PostgreSQL ready ledger failed; application refused".into(),
            "ready_ledger_failed",
        )
        .await;
    }
    Ok(owned)
}

async fn failed(owned: OwnedContainer, error: String, event: &'static str) -> StartResult {
    let _ = owned.attempt.context.event(
        owned.attempt.number,
        &owned.attempt.role,
        &owned.attempt.name,
        Some(&owned.id),
        event,
        owned.attempt.port,
    );
    let cleanup = owned.rm().await;
    Err(format!(
        "{error}; exact-ID cleanup={}",
        if cleanup.is_ok() {
            "complete"
        } else {
            "failed"
        }
    ))
}

#[cfg(test)]
mod tests {
    use super::super::{Context, Endpoint};
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    };
    use wiremock::{
        matchers::{method, path_regex},
        Mock, MockServer, Request, ResponseTemplate,
    };

    struct Fixture {
        attempt: Option<Attempt>,
        context: Arc<Context>,
        path: std::path::PathBuf,
        id: String,
    }
    impl Fixture {
        fn new() -> Self {
            let nonce = uuid::Uuid::new_v4().simple().to_string();
            let path = std::env::temp_dir().join(format!("rustycog-postgres-{nonce}.jsonl"));
            let context = Arc::new(Context {
                run: nonce.clone(),
                endpoint: Endpoint {
                    host: "127.0.0.1".into(),
                },
                ledger: Mutex::new(
                    std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&path)
                        .unwrap(),
                ),
                ids: Mutex::new(HashMap::new()),
                unknown: AtomicBool::new(false),
            });
            Self {
                attempt: Some(Attempt {
                    context: context.clone(),
                    number: 0,
                    role: "postgres".into(),
                    name: format!("postgres-{nonce}"),
                    port: 0,
                }),
                context,
                path,
                id: nonce.repeat(2),
            }
        }
        fn records(&self) -> Vec<serde_json::Value> {
            std::fs::read_to_string(&self.path)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    fn config() -> DatabaseConfig {
        DatabaseConfig {
            db: uuid::Uuid::new_v4().simple().to_string(),
            ..DatabaseConfig::default()
        }
    }
    fn client(server: &MockServer) -> Docker {
        Docker::connect_with_http(
            &server.uri(),
            5,
            testcontainers::bollard::API_DEFAULT_VERSION,
        )
        .unwrap()
    }
    async fn mocks(server: &MockServer, fixture: &Fixture, start_status: u16, mapping: bool) {
        mocks_with_delay(server, fixture, start_status, mapping, "", None).await;
    }
    async fn mocks_with_delay(
        server: &MockServer,
        fixture: &Fixture,
        start_status: u16,
        mapping: bool,
        phase: &str,
        notify: Option<Arc<tokio::sync::Notify>>,
    ) {
        let create_delayed = phase == "create";
        let start_delayed = phase == "start";
        let create_notify = notify.clone();
        let create_id = fixture.id.clone();
        Mock::given(method("POST"))
            .and(path_regex(r"/containers/create$"))
            .respond_with(move |_request: &Request| {
                if create_delayed {
                    create_notify.as_ref().unwrap().notify_one();
                }
                ResponseTemplate::new(201)
                    .set_body_json(serde_json::json!({
                        "Id": create_id, "Warnings": []
                    }))
                    .set_delay(Duration::from_millis(if create_delayed { 150 } else { 0 }))
            })
            .expect(1)
            .mount(server)
            .await;
        let file = fixture.path.clone();
        let id = fixture.id.clone();
        Mock::given(method("POST"))
            .and(path_regex(r"/containers/[^/]+/start$"))
            .respond_with(move |_request: &Request| {
                let text = std::fs::read_to_string(&file).unwrap();
                let records: Vec<serde_json::Value> = text
                    .lines()
                    .map(|line| serde_json::from_str(line).unwrap())
                    .collect();
                assert!(records
                    .iter()
                    .any(|row| row["event"] == "created" && row["container_id"] == id));
                if start_delayed {
                    notify.as_ref().unwrap().notify_one();
                }
                ResponseTemplate::new(start_status).set_body_json(serde_json::json!({
                    "message": "address already in use private-start-sentinel password=never-print"
                })).set_delay(Duration::from_millis(if start_delayed { 150 } else { 0 }))
            })
            .expect(1)
            .mount(server)
            .await;
        let ports = if mapping {
            serde_json::json!({
                "5432/tcp": [{"HostIp":"0.0.0.0","HostPort":"45678"}]
            })
        } else {
            serde_json::json!({})
        };
        Mock::given(method("GET"))
            .and(path_regex(r"/containers/[^/]+/json$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "Id": fixture.id,
                "State": {
                    "Status": if start_status == 204 { "running" } else { "created" },
                    "Running": start_status == 204, "Paused":false,"Restarting":false
                },
                "NetworkSettings":{"Ports":ports}
            })))
            .mount(server)
            .await;
        Mock::given(method("POST"))
            .and(path_regex(r"/containers/[^/]+/stop$"))
            .respond_with(ResponseTemplate::new(204))
            .mount(server)
            .await;
        Mock::given(method("DELETE"))
            .and(path_regex(r"/containers/[^/]+$"))
            .respond_with(ResponseTemplate::new(204))
            .mount(server)
            .await;
    }

    #[test]
    fn dynamic_request_is_daemon_owned_and_fixed_request_is_unchanged() {
        let mut config = config();
        let dynamic = request(&config);
        let bindings = dynamic.host_config.unwrap().port_bindings.unwrap();
        assert_eq!(
            bindings["5432/tcp"].as_ref().unwrap()[0]
                .host_port
                .as_deref(),
            Some("0")
        );
        config.port = 45432;
        let fixed = request(&config).host_config.unwrap().port_bindings.unwrap();
        assert_eq!(
            fixed["5432/tcp"].as_ref().unwrap()[0].host_port.as_deref(),
            Some("45432")
        );
    }

    #[test]
    fn unjoined_postgres_results_cannot_overwrite_a_live_initializers_cache() {
        // Serialize this global-cache oracle against the synchronous config tests.
        // The guard is outside the per-test async runtime, never inside a spawned task.
        let _guard = crate::config::DATABASE_PORT_TEST_LOCK.lock().unwrap();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let server = MockServer::start().await;
                let mut fixture = Fixture::new();
                mocks(&server, &fixture, 204, true).await;
                let config = config();
                config.cache_mapped_port(44123).unwrap();
                let owned = start_with_registration(
                    fixture.attempt.take().unwrap(),
                    config.clone(),
                    client(&server),
                    process_exit::Record::unenrolled,
                )
                .await
                .expect("mock retained creation succeeds");
                let unpublished = owned.database_publication.is_none();
                let observed = config.actual_port();
                owned.rm().await.expect("mock orphan cleanup");
                let after_cleanup = config.actual_port();
                config.clear_mapped_port(44123);
                assert!(
                    unpublished,
                    "unjoined creation must not publish a database endpoint"
                );
                assert_eq!(
                    observed, 44123,
                    "late creation must not replace another initializer"
                );
                assert_eq!(
                    after_cleanup, 44123,
                    "orphan cleanup must not invalidate that initializer"
                );
            });
    }

    #[tokio::test]
    async fn create_id_is_recorded_before_start_and_daemon_mapping_is_observed() {
        let server = MockServer::start().await;
        let mut fixture = Fixture::new();
        mocks(&server, &fixture, 204, true).await;
        let config = config();
        let mut owned = start_with_registration(
            fixture.attempt.take().unwrap(),
            config.clone(),
            client(&server),
            process_exit::Record::unenrolled,
        )
        .await
        .expect("mock lifecycle must succeed");
        assert_eq!(owned.attempt.port, 45678);
        assert!(owned.database_publication.is_none());
        {
            let _guard = crate::config::DATABASE_PORT_TEST_LOCK.lock().unwrap();
            assert_eq!(owned.publish_database_port(&config).unwrap(), 45678);
            assert_eq!(config.actual_port(), 45678);
        }
        assert!(!fixture.context.unknown.load(Ordering::SeqCst));
        owned
            .rm()
            .await
            .expect("mock exact-ID cleanup must succeed");
        assert!(fixture
            .records()
            .iter()
            .any(|row| row["event"] == "removed" && row["container_id"] == fixture.id));
        let requests = server.received_requests().await.unwrap();
        let deletion = requests
            .iter()
            .find(|request| request.method.as_str() == "DELETE")
            .unwrap();
        assert!(deletion
            .url
            .query_pairs()
            .any(|(key, value)| key == "force" && value == "false"));
        assert!(deletion
            .url
            .query_pairs()
            .any(|(key, value)| key == "v" && value == "false"));
    }

    #[tokio::test]
    async fn recorded_start_failure_and_missing_mapping_are_cleaned_without_secret_output() {
        for (status, mapping) in [(500, true), (204, false)] {
            let server = MockServer::start().await;
            let mut fixture = Fixture::new();
            mocks(&server, &fixture, status, mapping).await;
            let result = start_with_registration(
                fixture.attempt.take().unwrap(),
                config(),
                client(&server),
                process_exit::Record::unenrolled,
            )
            .await;
            let error = match result {
                Ok(_) => panic!("mock refusal required"),
                Err(error) => error,
            };
            assert!(error.contains("exact-ID cleanup=complete"));
            assert!(!error.contains("private-start-sentinel") && !error.contains("never-print"));
            let records = fixture.records();
            assert!(records
                .iter()
                .any(|row| row["event"] == "created" && row["container_id"] == fixture.id));
            assert!(records
                .iter()
                .any(|row| row["event"] == "removed" && row["container_id"] == fixture.id));
            assert!(!records
                .iter()
                .any(|row| row["event"].as_str().unwrap().starts_with("unknown")));
        }
    }

    #[tokio::test]
    async fn cancelled_callers_leave_create_and_start_supervised_until_exact_teardown() {
        for phase in ["create", "start"] {
            let server = MockServer::start().await;
            let mut fixture = Fixture::new();
            let notify = Arc::new(tokio::sync::Notify::new());
            mocks_with_delay(&server, &fixture, 204, true, phase, Some(notify.clone())).await;
            let attempt = fixture.attempt.take().unwrap();
            let client = client(&server);
            let caller = tokio::spawn(attempt.supervise(move |attempt| {
                start_with_registration(attempt, config(), client, process_exit::Record::unenrolled)
            }));
            tokio::time::timeout(Duration::from_secs(3), notify.notified())
                .await
                .unwrap();
            caller.abort();
            let _ = caller.await;
            let slot = super::super::TASKS
                .get()
                .unwrap()
                .lock()
                .unwrap()
                .iter()
                .find(|slot| Arc::ptr_eq(&slot.context, &fixture.context))
                .unwrap()
                .clone();
            assert!(!slot.active.load(Ordering::SeqCst));
            let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
            loop {
                let state = super::super::reconcile_orphan(
                    &slot.task,
                    &slot.teardown,
                    |owned| async move { owned.rm().await },
                )
                .await;
                if matches!(state, super::super::OrphanReconciliation::Idle) {
                    break;
                }
                assert!(matches!(state, super::super::OrphanReconciliation::Pending));
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "mock reconciliation deadline"
                );
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let records = fixture.records();
            assert_eq!(
                records
                    .iter()
                    .filter(|row| row["event"] == "created")
                    .count(),
                1
            );
            assert_eq!(
                records
                    .iter()
                    .filter(|row| row["event"] == "removed" && row["container_id"] == fixture.id)
                    .count(),
                1
            );
            assert!(!records
                .iter()
                .any(|row| row["event"].as_str().unwrap().starts_with("unknown")));
        }
    }

    #[tokio::test]
    async fn failed_creation_ledger_refuses_start_but_retains_exact_id_for_cleanup() {
        let server = MockServer::start().await;
        let mut fixture = Fixture::new();
        *fixture.context.ledger.lock().unwrap() = std::fs::File::open(&fixture.path).unwrap();
        Mock::given(method("POST"))
            .and(path_regex(r"/containers/create$"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "Id":fixture.id, "Warnings":[]
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path_regex(r"/containers/[^/]+/start$"))
            .respond_with(ResponseTemplate::new(204))
            .expect(0)
            .mount(&server)
            .await;
        Mock::given(method("GET")).and(path_regex(r"/containers/[^/]+/json$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "Id":fixture.id, "State":{"Running":false,"Paused":false,"Restarting":false,"Status":"created"}
            }))).expect(1).mount(&server).await;
        Mock::given(method("DELETE"))
            .and(path_regex(r"/containers/[^/]+$"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        let result = start_with_registration(
            fixture.attempt.take().unwrap(),
            config(),
            client(&server),
            process_exit::Record::unenrolled,
        )
        .await;
        let error = match result {
            Ok(_) => panic!("ledger refusal required"),
            Err(error) => error,
        };
        assert!(error.contains("creation ledger failed"));
        assert_eq!(
            fixture.context.ids.lock().unwrap().get(&0),
            Some(&fixture.id)
        );
        assert!(fixture.context.unknown.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn sql_readiness_is_required_before_ready_and_failure_cleans_the_recorded_id() {
        for ready in [false, true] {
            let server = MockServer::start().await;
            let mut fixture = Fixture::new();
            mocks(&server, &fixture, 204, true).await;
            let owned = start_with_registration(
                fixture.attempt.take().unwrap(),
                config(),
                client(&server),
                process_exit::Record::unenrolled,
            )
            .await
            .expect("mock start must succeed");
            assert!(!fixture.records().iter().any(|row| row["event"] == "ready"));
            let result = finish_readiness(owned, async move {
                if ready {
                    Ok(())
                } else {
                    Err(sea_orm::DbErr::Custom("private-sql-error-sentinel".into()))
                }
            })
            .await;
            match result {
                Ok(owned) => {
                    assert!(ready);
                    assert!(fixture
                        .records()
                        .iter()
                        .any(|row| row["event"] == "ready" && row["published_port"] == 45678));
                    owned.rm().await.expect("mock exact-ID teardown");
                }
                Err(error) => {
                    assert!(!ready);
                    assert!(error.contains("stage=readiness reason=sql_unavailable"));
                    assert!(!error.contains("private-sql-error-sentinel"));
                    assert!(!fixture.records().iter().any(|row| row["event"] == "ready"));
                }
            }
            assert!(fixture
                .records()
                .iter()
                .any(|row| row["event"] == "removed" && row["container_id"] == fixture.id));
        }
    }
}
