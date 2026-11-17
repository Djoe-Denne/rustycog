//! Readiness contract only: an open HTTP listener is not a ready SQS service.
//! These tests do not claim LocalStack integration or authentication evidence.
use super::TestSqs;
use aws_sdk_sqs::{config::retry::RetryConfig, Client};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use wiremock::{matchers::any, Mock, MockServer, ResponseTemplate};

async fn client(server: &MockServer) -> Client {
    let base = TestSqs::create_sqs_client(&server.uri(), "us-east-1")
        .await
        .unwrap();
    // Disable only the SDK's own retries in unit tests so eventual readiness
    // exercises the fixture's polling, not an implicit transport retry.
    Client::from_conf(
        base.config()
            .to_builder()
            .retry_config(RetryConfig::disabled())
            .build(),
    )
}

async fn assert_only_list_queues(server: &MockServer) {
    let requests = server.received_requests().await.unwrap();
    assert!(!requests.is_empty());
    assert!(requests.iter().all(|request| {
        request
            .headers
            .get("x-amz-target")
            .and_then(|value| value.to_str().ok())
            == Some("AmazonSQS.ListQueues")
    }));
}

#[tokio::test]
async fn an_open_listener_without_sqs_is_not_ready() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let readiness = TestSqs::wait_for_sqs_ready(
        &client(&server).await,
        Duration::from_secs(1),
        Duration::from_millis(100),
    )
    .await;
    assert!(
        readiness.is_err(),
        "an open TCP listener must not establish SQS protocol readiness"
    );
    assert_only_list_queues(&server).await;
}

#[tokio::test]
async fn readiness_waits_for_a_valid_sqs_response_after_transient_failure() {
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    Mock::given(any())
        .respond_with(
            move |_: &wiremock::Request| match observed.fetch_add(1, Ordering::SeqCst) {
                0 => ResponseTemplate::new(503),
                1 => ResponseTemplate::new(429),
                _ => ResponseTemplate::new(200).set_body_json(serde_json::json!({"QueueUrls": []})),
            },
        )
        .mount(&server)
        .await;
    TestSqs::wait_for_sqs_ready(
        &client(&server).await,
        Duration::from_secs(5),
        Duration::from_millis(10),
    )
    .await
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_only_list_queues(&server).await;
}

#[tokio::test]
async fn permanent_sqs_rejection_is_not_retried_or_accepted() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(serde_json::json!({"__type": "AccessDenied", "message": "denied"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let error = TestSqs::wait_for_sqs_ready(
        &client(&server).await,
        Duration::from_secs(5),
        Duration::from_millis(10),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("HTTP 403"));
    assert_only_list_queues(&server).await;
}

#[tokio::test]
async fn an_http_success_without_a_valid_sqs_body_is_not_ready() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_string("not SQS JSON"))
        .expect(1)
        .mount(&server)
        .await;
    let readiness = TestSqs::wait_for_sqs_ready(
        &client(&server).await,
        Duration::from_secs(5),
        Duration::from_millis(10),
    )
    .await;
    assert!(readiness.is_err());
    assert_only_list_queues(&server).await;
}

#[tokio::test]
async fn a_stalled_sqs_request_is_bounded_by_the_readiness_deadline() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({}))
                .set_delay(Duration::from_secs(5)),
        )
        .mount(&server)
        .await;
    let sqs = client(&server).await;
    let readiness = tokio::time::timeout(
        Duration::from_secs(2),
        TestSqs::wait_for_sqs_ready(&sqs, Duration::from_millis(100), Duration::from_millis(10)),
    )
    .await
    .expect("the fixture must enforce its own shorter readiness deadline");
    let error = readiness.unwrap_err().to_string();
    assert!(error.contains("request_pending=true"));
    assert!(error.contains("last_error=no_completed_response"));
    assert!(error.contains("last_request_ms="));
}

#[tokio::test]
async fn stalled_request_retains_the_previous_http_failure_without_body() {
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    Mock::given(any())
        .respond_with(move |_: &wiremock::Request| {
            if observed.fetch_add(1, Ordering::SeqCst) == 0 {
                ResponseTemplate::new(503).set_body_string("private-readiness-sentinel")
            } else {
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({}))
                    .set_delay(Duration::from_secs(5))
            }
        })
        .mount(&server)
        .await;
    let error = TestSqs::wait_for_sqs_ready(
        &client(&server).await,
        Duration::from_secs(1),
        Duration::from_millis(10),
    )
    .await
    .unwrap_err()
    .to_string();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(error.contains("after 2 attempts"));
    assert!(error.contains("request_pending=true"));
    assert!(error.contains("HTTP_503"));
    assert!(error.contains("elapsed_ms="));
    assert!(!error.contains("private-readiness-sentinel"));
    assert!(!error.contains(&server.uri()));
    assert_only_list_queues(&server).await;
}

#[tokio::test]
async fn permanent_rejection_reports_category_without_provider_message() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(403).set_body_json(
            serde_json::json!({"__type": "AccessDenied", "message": "private-provider-sentinel"}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let error = TestSqs::wait_for_sqs_ready(
        &client(&server).await,
        Duration::from_secs(1),
        Duration::from_millis(10),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("HTTP 403"));
    assert!(error.contains("service_error"));
    assert!(!error.contains("private-provider-sentinel"));
    assert!(!error.contains(&server.uri()));
}
