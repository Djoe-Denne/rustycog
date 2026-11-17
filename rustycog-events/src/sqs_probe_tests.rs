//! HTTP-boundary tests for the optional observer, not real SQS/LocalStack proof.
use super::super::sqs_probe::SqsConsumerProbe;
use super::*;
use wiremock::{matchers::header, Mock, MockServer, ResponseTemplate};

struct SuspendedHandler(bool);

#[async_trait]
impl EventHandler for SuspendedHandler {
    async fn handle_event(&self, _: Box<dyn DomainEvent>) -> Result<(), ServiceError> {
        std::future::pending().await
    }
    fn supports_event_type(&self, _: &str) -> bool {
        self.0
    }
}

fn client(server: &MockServer) -> Client {
    Client::from_conf(
        Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new("us-east-1"))
            .credentials_provider(Credentials::new("test", "test", None, None, "probe-unit"))
            .endpoint_url(server.uri())
            .retry_config(aws_sdk_sqs::config::retry::RetryConfig::disabled())
            .build(),
    )
}

fn body() -> String {
    json!({
        "event_id": uuid::Uuid::new_v4(), "event_type": "probe",
        "aggregate_id": uuid::Uuid::new_v4(), "occurred_at": chrono::Utc::now(),
        "version": 1, "data": {"private": "private-payload-sentinel"},
    })
    .to_string()
}

async fn run_response(response: ResponseTemplate, probe: &SqsConsumerProbe, supports: bool) {
    let server = MockServer::start().await;
    Mock::given(header("x-amz-target", "AmazonSQS.ReceiveMessage"))
        .respond_with(response)
        .mount(&server)
        .await;
    let client = client(&server);
    let _ = tokio::time::timeout(
        Duration::from_millis(200),
        SqsEventConsumer::poll_queue_and_handle_messages(
            &client,
            &server.uri(),
            &SuspendedHandler(supports),
            Some(probe),
        ),
    )
    .await;
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests.len(),
        1,
        "no retries, extra receives or acknowledgement"
    );
    assert_eq!(
        requests[0].headers.get("x-amz-target").unwrap(),
        "AmazonSQS.ReceiveMessage"
    );
}

#[tokio::test]
async fn probe_correlates_expected_message_and_handler_without_exposing_data() {
    let id = "private-message-id-sentinel";
    let probe = SqsConsumerProbe::new(id.into());
    run_response(
        ResponseTemplate::new(200).set_body_json(json!({"Messages":[{
            "MessageId": id, "Body": body(), "ReceiptHandle": "private-receipt-sentinel"
        }]})),
        &probe,
        true,
    )
    .await;
    let sample = probe.snapshot();
    assert_eq!(sample.receive_started, 1);
    assert_eq!(sample.receive_succeeded, 1);
    assert_eq!(sample.receive_failed, 0);
    assert_eq!(
        sample.receive_cancelled, 0,
        "handler cancellation is not a cancelled receive"
    );
    assert_eq!(sample.messages_returned, 1);
    assert_eq!(sample.expected_message_returned, 1);
    assert_eq!(sample.handler_entered, 1);
    assert_eq!(sample.expected_handler_entered, 1);
    let encoded = serde_json::to_string(&sample).unwrap();
    for private in [id, "private-payload-sentinel", "private-receipt-sentinel"] {
        assert!(!encoded.contains(private));
    }
}

#[tokio::test]
async fn probe_distinguishes_missing_body_parse_failure_and_unsupported_type() {
    for variant in ["missing", "invalid", "unsupported"] {
        let probe = SqsConsumerProbe::new("expected".into());
        let mut message = json!({"MessageId":"expected"});
        if variant != "missing" {
            message["Body"] = json!(if variant == "invalid" {
                "not JSON".into()
            } else {
                body()
            });
        }
        run_response(
            ResponseTemplate::new(200).set_body_json(json!({"Messages":[message]})),
            &probe,
            variant != "unsupported",
        )
        .await;
        let sample = probe.snapshot();
        assert_eq!(sample.receive_succeeded, 1);
        assert_eq!(sample.expected_message_returned, 1);
        assert_eq!(sample.missing_body, u64::from(variant == "missing"));
        assert_eq!(sample.parse_failed, u64::from(variant == "invalid"));
        assert_eq!(sample.unsupported, u64::from(variant == "unsupported"));
        assert_eq!(sample.handler_entered, 0);
        assert_eq!(sample.receive_cancelled, 0);
    }
}

#[tokio::test]
async fn probe_records_cancellation_before_receive_response() {
    let probe = SqsConsumerProbe::new("expected".into());
    run_response(
        ResponseTemplate::new(200)
            .set_body_json(json!({}))
            .set_delay(Duration::from_secs(5)),
        &probe,
        true,
    )
    .await;
    let sample = probe.snapshot();
    assert_eq!(sample.receive_started, 1);
    assert_eq!(sample.receive_succeeded, 0);
    assert_eq!(sample.receive_failed, 0);
    assert_eq!(sample.receive_cancelled, 1);
    assert_eq!(sample.messages_returned, 0);
}

#[tokio::test]
async fn probe_records_receive_failure_without_inventing_an_empty_success() {
    let probe = SqsConsumerProbe::new("expected".into());
    run_response(
        ResponseTemplate::new(403).set_body_json(json!({
            "__type":"AccessDenied", "message":"private-provider-sentinel"
        })),
        &probe,
        true,
    )
    .await;
    let sample = probe.snapshot();
    assert_eq!(sample.receive_started, 1);
    assert_eq!(sample.receive_succeeded, 0);
    assert_eq!(sample.receive_failed, 1);
    assert_eq!(sample.receive_cancelled, 0);
    assert_eq!(sample.handler_entered, 0);
}

#[tokio::test]
async fn probe_does_not_attribute_another_message_to_the_expected_identity() {
    let probe = SqsConsumerProbe::new("expected".into());
    run_response(
        ResponseTemplate::new(200).set_body_json(json!({"Messages":[{
            "MessageId":"different", "Body":body()
        }]})),
        &probe,
        true,
    )
    .await;
    let sample = probe.snapshot();
    assert_eq!(sample.messages_returned, 1);
    assert_eq!(sample.expected_message_returned, 0);
    assert_eq!(sample.handler_entered, 1);
    assert_eq!(sample.expected_handler_entered, 0);
    assert_eq!(
        SqsConsumerProbe::new("other-consumer".into())
            .snapshot()
            .receive_started,
        0
    );
}

#[tokio::test]
async fn probe_is_carried_through_the_public_start_and_stop_path() {
    struct EntryHandler(Arc<tokio::sync::Notify>);
    #[async_trait]
    impl EventHandler for EntryHandler {
        async fn handle_event(&self, _: Box<dyn DomainEvent>) -> Result<(), ServiceError> {
            self.0.notify_one();
            std::future::pending().await
        }
        fn supports_event_type(&self, _: &str) -> bool {
            true
        }
    }
    let server = MockServer::start().await;
    Mock::given(header("x-amz-target", "AmazonSQS.ReceiveMessage"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"Messages":[{
                "MessageId":"expected", "Body":body(), "ReceiptHandle":"no-ack"
            }]})),
        )
        .mount(&server)
        .await;
    let probe = Arc::new(SqsConsumerProbe::new("expected".into()));
    let consumer = Arc::new(
        SqsEventConsumer {
            client: client(&server),
            config: SqsConfig {
                enabled: true,
                host: "localhost".into(),
                port: server.address().port(),
                default_queues: vec!["probe".into()],
                ..SqsConfig::default()
            },
            lifecycle: Arc::default(),
            probe: None,
        }
        .with_poll_probe(probe.clone()),
    );
    let entered = Arc::new(tokio::sync::Notify::new());
    let task_consumer = consumer.clone();
    let handler = EntryHandler(entered.clone());
    let mut parent = tokio::spawn(async move { task_consumer.start(handler).await });
    let entry = tokio::time::timeout(Duration::from_secs(1), entered.notified()).await;
    let stop = consumer.stop().await;
    let joined = tokio::time::timeout(Duration::from_secs(1), &mut parent).await;
    // Even a failed observer assertion must not detach the spawned parent.
    if joined.is_err() {
        parent.abort();
        tokio::time::timeout(Duration::from_secs(1), parent)
            .await
            .expect("aborted parent joined")
            .expect_err("parent was aborted");
    }
    entry.expect("public start reached the real handler");
    stop.unwrap();
    joined.expect("stop joined the parent").unwrap().unwrap();
    assert_eq!(probe.snapshot().expected_handler_entered, 1);
    let requests = server.received_requests().await.unwrap();
    assert!(
        requests
            .iter()
            .all(|request| request.headers.get("x-amz-target").unwrap()
                == "AmazonSQS.ReceiveMessage"),
        "suspended handler must not acknowledge"
    );
}
