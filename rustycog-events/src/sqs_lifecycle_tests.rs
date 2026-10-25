use super::*;
use std::future::pending;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{timeout_at, Instant};

struct LeaseOrderHandler {
    lifecycle: Arc<Mutex<ConsumerLifecycle>>,
    destroyed: Option<oneshot::Sender<bool>>,
    panic_on_message: bool,
}

impl Drop for LeaseOrderHandler {
    fn drop(&mut self) {
        let active = self
            .lifecycle
            .lock()
            .expect("lifecycle observer")
            .active
            .is_some();
        if let Some(destroyed) = self.destroyed.take() {
            let _ = destroyed.send(active);
        }
    }
}

#[async_trait]
impl EventHandler for LeaseOrderHandler {
    async fn handle_event(&self, _event: Box<dyn DomainEvent>) -> Result<(), ServiceError> {
        assert!(!self.panic_on_message, "forced public poller panic");
        pending().await
    }

    fn supports_event_type(&self, _event_type: &str) -> bool {
        true
    }
}

fn lease_order_handler(
    consumer: &SqsEventConsumer,
    panic_on_message: bool,
) -> (LeaseOrderHandler, oneshot::Receiver<bool>) {
    let (destroyed, receiver) = oneshot::channel();
    (
        LeaseOrderHandler {
            lifecycle: consumer.lifecycle.clone(),
            destroyed: Some(destroyed),
            panic_on_message,
        },
        receiver,
    )
}

async fn assert_lease_outlives_handler(
    consumer: &SqsEventConsumer,
    deadline: Instant,
    destroyed: oneshot::Receiver<bool>,
) {
    let active_during_handler_drop = timeout_at(deadline, destroyed)
        .await
        .expect("handler destruction deadline")
        .expect("handler witness");
    assert!(
        active_during_handler_drop,
        "lease must remain active throughout H::drop, not only until poller-local destruction"
    );
    let _next = ConsumerLifecycle::begin(&consumer.lifecycle)
        .expect("restart admitted after complete resource destruction");
}

#[tokio::test]
async fn rp1_lease_outlives_handler_when_children_never_polled() {
    let mut consumer = consumer();
    consumer.config.enabled = true;
    consumer.config.default_queues = vec!["first".into(), "second".into()];
    let (handler, destroyed) = lease_order_handler(&consumer, false);
    let mut parent = Box::pin(consumer.start(handler));
    assert!(futures::poll!(parent.as_mut()).is_pending());
    let deadline = Instant::now() + Duration::from_secs(1);
    drop(parent);
    assert_lease_outlives_handler(&consumer, deadline, destroyed).await;
}

// A loopback listener only witnesses actual ReceiveMessage awaits. It never
// connects to Docker/SQS and never impersonates any shared test fixture.
async fn active_public_consumer(
    panic_on_message: bool,
) -> (
    Arc<SqsEventConsumer>,
    tokio::task::JoinHandle<Result<(), ServiceError>>,
    oneshot::Receiver<bool>,
    Vec<tokio::net::TcpStream>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("loopback poll witness");
    let port = listener.local_addr().expect("loopback address").port();
    let endpoint = format!("http://127.0.0.1:{port}");
    let mut consumer = consumer();
    consumer.client = Client::from_conf(
        Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new("us-east-1"))
            .credentials_provider(Credentials::new("test", "test", None, None, "rp1"))
            .endpoint_url(endpoint)
            .build(),
    );
    consumer.config.enabled = true;
    consumer.config.host = "127.0.0.1".into();
    consumer.config.port = port;
    consumer.config.default_queues = vec!["first".into(), "second".into()];
    let consumer = Arc::new(consumer);
    let (handler, destroyed) = lease_order_handler(&consumer, panic_on_message);
    let parent_consumer = consumer.clone();
    let parent = tokio::spawn(async move { parent_consumer.start(handler).await });
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut polls = Vec::new();
    for _ in 0..2 {
        let (stream, _) = timeout_at(deadline, listener.accept())
            .await
            .expect("both real poll futures entered")
            .expect("poll connection");
        polls.push(stream);
    }
    (consumer, parent, destroyed, polls)
}

#[tokio::test]
async fn rp1_lease_outlives_handler_when_active_parent_is_aborted() {
    let (consumer, parent, destroyed, _polls) = active_public_consumer(false).await;
    let deadline = Instant::now() + Duration::from_secs(1);
    parent.abort();
    assert!(timeout_at(deadline, parent)
        .await
        .expect("parent joined")
        .expect_err("aborted")
        .is_cancelled());
    assert_lease_outlives_handler(&consumer, deadline, destroyed).await;
}

#[tokio::test]
async fn rp1_lease_outlives_handler_during_normal_public_stop() {
    let (consumer, parent, destroyed, _polls) = active_public_consumer(false).await;
    let deadline = Instant::now() + Duration::from_secs(1);
    consumer.stop().await.expect("stop active polls");
    timeout_at(deadline, parent)
        .await
        .expect("parent joined")
        .expect("parent task")
        .expect("normal stop");
    assert_lease_outlives_handler(&consumer, deadline, destroyed).await;
}

#[tokio::test]
async fn rp1_lease_outlives_handler_during_public_child_panic() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (consumer, parent, destroyed, mut polls) = active_public_consumer(true).await;
    let mut stream = polls.pop().expect("first pending poll");
    let mut request = [0; 4096];
    tokio::time::timeout(Duration::from_secs(1), stream.read(&mut request))
        .await
        .expect("receive request")
        .expect("request bytes");
    let body = json!({"Messages": [{"Body": json!({
        "event_id": uuid::Uuid::new_v4().to_string(), "event_type": "panic-test",
        "aggregate_id": uuid::Uuid::new_v4().to_string(),
        "occurred_at": chrono::Utc::now().to_rfc3339(), "version": 1, "data": {}, "metadata": {}
    }).to_string(), "ReceiptHandle": "must-not-delete"}]})
    .to_string();
    let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/x-amz-json-1.0\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
    let deadline = Instant::now() + Duration::from_secs(1);
    stream
        .write_all(response.as_bytes())
        .await
        .expect("one message response");
    let error = timeout_at(deadline, parent)
        .await
        .expect("panic cleanup deadline")
        .expect("parent task")
        .expect_err("child panic propagated");
    assert!(error.to_string().contains("panicked"));
    assert_lease_outlives_handler(&consumer, deadline, destroyed).await;
}

struct HandlerDropWitness(Option<oneshot::Sender<()>>);

impl Drop for HandlerDropWitness {
    fn drop(&mut self) {
        if let Some(destroyed) = self.0.take() {
            let _ = destroyed.send(());
        }
    }
}

#[async_trait]
impl EventHandler for HandlerDropWitness {
    async fn handle_event(&self, _event: Box<dyn DomainEvent>) -> Result<(), ServiceError> {
        pending().await
    }

    fn supports_event_type(&self, _event_type: &str) -> bool {
        true
    }
}

#[tokio::test]
async fn dropping_start_destroys_all_poller_handler_owners() {
    let config = SqsConfig {
        enabled: true,
        default_queues: vec!["ac7-first".to_string(), "ac7-second".to_string()],
        ..SqsConfig::default()
    };
    let consumer = SqsEventConsumer::new(config).await.expect("consumer");
    let (destroyed, dropped) = oneshot::channel();
    let mut parent = Box::pin(consumer.start(HandlerDropWitness(Some(destroyed))));

    // Poll synchronously through registration of every child, without allowing
    // any real SQS request to run. Dropping the parent must cancel those tasks.
    assert!(futures::poll!(parent.as_mut()).is_pending());
    drop(parent);
    tokio::time::timeout(Duration::from_secs(1), dropped)
        .await
        .expect("all pollers must release the handler after parent drop")
        .expect("handler destruction must be observed");
}

fn consumer() -> SqsEventConsumer {
    let client = Client::from_conf(
        Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new("us-east-1"))
            .credentials_provider(Credentials::new("test", "test", None, None, "ac7"))
            .endpoint_url("http://127.0.0.1:1")
            .build(),
    );
    SqsEventConsumer {
        client,
        config: SqsConfig::default(),
        lifecycle: Arc::default(),
    }
}

fn witness() -> (HandlerDropWitness, oneshot::Receiver<()>) {
    let (sender, receiver) = oneshot::channel();
    (HandlerDropWitness(Some(sender)), receiver)
}

async fn destroyed_before(deadline: Instant, children: Vec<oneshot::Receiver<()>>) {
    for child in children {
        timeout_at(deadline, child)
            .await
            .expect("every child must be destroyed before the common deadline")
            .expect("RAII destruction witness");
    }
}

fn blocked_poller(
    tasks: &mut JoinSet<Result<(), ServiceError>>,
    run: &Arc<ConsumerRun>,
    entered: mpsc::UnboundedSender<()>,
    activity: Arc<AtomicUsize>,
) -> oneshot::Receiver<()> {
    let owner = run.clone();
    let (destroyed, receiver) = witness();
    tasks.spawn(async move {
        let _destroyed = destroyed;
        let _owner = owner;
        poll_until_stopped(_owner.stop.subscribe(), "blocked-test-queue", || {
            let entered = entered.clone();
            let activity = activity.clone();
            async move {
                activity.fetch_add(1, Ordering::SeqCst);
                entered.send(()).expect("start observer");
                pending().await
            }
        })
        .await
    });
    receiver
}

#[tokio::test]
async fn stop_interrupts_long_poll_and_joins_every_child() {
    let consumer = consumer();
    let run = ConsumerLifecycle::begin(&consumer.lifecycle).expect("first start");
    let (entered, mut entries) = mpsc::unbounded_channel();
    let activity = Arc::new(AtomicUsize::new(0));
    let mut tasks = JoinSet::new();
    let children = (0..2)
        .map(|_| blocked_poller(&mut tasks, &run, entered.clone(), activity.clone()))
        .collect();
    for _ in 0..2 {
        entries.recv().await.expect("both polls entered");
    }
    let deadline = Instant::now() + Duration::from_secs(1);
    consumer.stop().await.expect("stop");
    timeout_at(deadline, join_pollers(&mut tasks, &run.stop))
        .await
        .expect("stop must interrupt pending polls")
        .expect("normal shutdown");
    destroyed_before(deadline, children).await;
    drop(run);
    let next = ConsumerLifecycle::begin(&consumer.lifecycle).expect("restart after joined stop");
    assert!(!*next.stop.borrow(), "old stop must not leak into next run");
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    assert_eq!(activity.load(Ordering::SeqCst), 2, "no activity after stop");
}

#[tokio::test]
async fn aborting_parent_destroys_each_active_child_before_restart() {
    let consumer = consumer();
    let run = ConsumerLifecycle::begin(&consumer.lifecycle).expect("start");
    let (entered, mut entries) = mpsc::unbounded_channel();
    let activity = Arc::new(AtomicUsize::new(0));
    let mut tasks = JoinSet::new();
    let children = (0..2)
        .map(|_| blocked_poller(&mut tasks, &run, entered.clone(), activity.clone()))
        .collect();
    let parent = tokio::spawn(async move { join_pollers(&mut tasks, &run.stop).await });
    for _ in 0..2 {
        entries.recv().await.expect("both children active");
    }
    assert!(ConsumerLifecycle::begin(&consumer.lifecycle).is_err());
    let deadline = Instant::now() + Duration::from_secs(1);
    parent.abort();
    assert!(timeout_at(deadline, parent)
        .await
        .expect("parent joined")
        .unwrap_err()
        .is_cancelled());
    destroyed_before(deadline, children).await;
    let _next = ConsumerLifecycle::begin(&consumer.lifecycle).expect("children released ownership");
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    assert_eq!(activity.load(Ordering::SeqCst), 2);
}

async fn failing_child_cleans_up_sibling(panic: bool) {
    let consumer = consumer();
    let run = ConsumerLifecycle::begin(&consumer.lifecycle).expect("start");
    let (entered, mut entries) = mpsc::unbounded_channel();
    let mut tasks = JoinSet::new();
    let sibling = blocked_poller(&mut tasks, &run, entered, Arc::new(AtomicUsize::new(0)));
    entries
        .recv()
        .await
        .expect("first spawned child stays blocked");
    let (destroyed, failed) = witness();
    let owner = run.clone();
    tasks.spawn(async move {
        let _destroyed = destroyed;
        let _owner = owner;
        assert!(!panic, "forced child panic");
        Err(ServiceError::infrastructure("forced terminal error"))
    });
    let deadline = Instant::now() + Duration::from_secs(1);
    let error = timeout_at(deadline, join_pollers(&mut tasks, &run.stop))
        .await
        .expect("completed later child must be observed without waiting for first")
        .expect_err("terminal failure must be returned");
    assert!(error.to_string().contains(if panic {
        "panicked"
    } else {
        "forced terminal error"
    }));
    assert!(*run.stop.borrow(), "failure signals siblings");
    assert!(tasks.is_empty(), "every result consumed");
    destroyed_before(deadline, vec![sibling, failed]).await;
}

#[tokio::test]
async fn terminal_child_error_cancels_and_joins_active_sibling() {
    failing_child_cleans_up_sibling(false).await;
}

#[tokio::test]
async fn child_panic_cancels_and_joins_active_sibling() {
    failing_child_cleans_up_sibling(true).await;
}

#[tokio::test]
async fn stop_at_start_is_not_lost_and_double_start_is_rejected() {
    let consumer = consumer();
    consumer
        .stop()
        .await
        .expect("stop before first poll of start");
    let run = ConsumerLifecycle::begin(&consumer.lifecycle).expect("start");
    assert!(*run.stop.borrow(), "queued stop must not be reset");
    assert!(ConsumerLifecycle::begin(&consumer.lifecycle).is_err());
    drop(run);
    let next = ConsumerLifecycle::begin(&consumer.lifecycle).expect("new cycle");
    assert!(!*next.stop.borrow());
    consumer
        .stop()
        .await
        .expect("stop after startup linearization");
    assert!(*next.stop.borrow());
}

#[tokio::test]
async fn normal_public_stop_joins_children_and_resets_for_next_start() {
    let mut consumer = consumer();
    consumer.config.enabled = true;
    consumer.config.default_queues = vec!["first".to_string(), "second".to_string()];
    for _ in 0..2 {
        let (destroyed, dropped) = witness();
        let mut parent = Box::pin(consumer.start(destroyed));
        assert!(futures::poll!(parent.as_mut()).is_pending());
        let deadline = Instant::now() + Duration::from_secs(1);
        consumer.stop().await.expect("normal stop");
        timeout_at(deadline, parent)
            .await
            .expect("parent joined")
            .expect("stop result");
        destroyed_before(deadline, vec![dropped]).await;
    }
    consumer.config.enabled = false;
    let (destroyed, dropped) = witness();
    consumer
        .start(destroyed)
        .await
        .expect("disabled is a normal exit");
    destroyed_before(Instant::now() + Duration::from_secs(1), vec![dropped]).await;
}

#[tokio::test]
async fn polling_errors_remain_retryable_and_stop_interrupts_backoff() {
    let (stop, stopped) = watch::channel(false);
    let (entered, mut entries) = mpsc::unbounded_channel();
    let (destroyed, dropped) = witness();
    let task = tokio::spawn(async move {
        let _destroyed = destroyed;
        poll_until_stopped(stopped, "retryable-queue", || {
            entered.send(()).expect("poll observer");
            async { Err(ServiceError::infrastructure("retryable receive failure")) }
        })
        .await
    });
    for _ in 0..2 {
        tokio::time::timeout(Duration::from_secs(2), entries.recv())
            .await
            .expect("ordinary polling failure must be retried")
            .expect("poll entry");
    }
    let deadline = Instant::now() + Duration::from_secs(1);
    stop.send_replace(true);
    timeout_at(deadline, task)
        .await
        .expect("backoff interrupted")
        .expect("joined")
        .expect("normal stop");
    destroyed_before(deadline, vec![dropped]).await;
}

struct BlockedHandler {
    entered: mpsc::UnboundedSender<()>,
    destroyed: Mutex<Option<oneshot::Sender<()>>>,
}

#[async_trait]
impl EventHandler for BlockedHandler {
    async fn handle_event(&self, _event: Box<dyn DomainEvent>) -> Result<(), ServiceError> {
        let _destroyed = HandlerDropWitness(self.destroyed.lock().expect("witness lock").take());
        self.entered.send(()).expect("handler entered");
        pending().await
    }

    fn supports_event_type(&self, _event_type: &str) -> bool {
        true
    }
}

#[tokio::test]
async fn stop_cancels_blocked_handler_without_acknowledging_message() {
    // No HTTP mock or broker: this listener detects any accidental DeleteMessage
    // request; ReceiveMessage is replaced only by a local already-received body.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("ack detector");
    listener
        .set_nonblocking(true)
        .expect("nonblocking detector");
    let endpoint = format!("http://{}", listener.local_addr().expect("address"));
    let client = Client::from_conf(
        Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new("us-east-1"))
            .credentials_provider(Credentials::new("test", "test", None, None, "ac7"))
            .endpoint_url(&endpoint)
            .build(),
    );
    let consumer = consumer();
    let run = ConsumerLifecycle::begin(&consumer.lifecycle).expect("start");
    let (entered, mut entries) = mpsc::unbounded_channel();
    let (handler_destroyed, handler_dropped) = oneshot::channel();
    let handler = BlockedHandler {
        entered,
        destroyed: Mutex::new(Some(handler_destroyed)),
    };
    let body = json!({
        "event_id": uuid::Uuid::new_v4().to_string(),
        "event_type": "blocked", "aggregate_id": uuid::Uuid::new_v4().to_string(),
        "occurred_at": chrono::Utc::now().to_rfc3339(), "version": 1, "data": {}, "metadata": {}
    })
    .to_string();
    let message = Message::builder()
        .body(body)
        .receipt_handle("must-not-delete")
        .build();
    let (destroyed, child_dropped) = witness();
    let owner = run.clone();
    let mut tasks = JoinSet::new();
    tasks.spawn(async move {
        let _destroyed = destroyed;
        let _owner = owner;
        poll_until_stopped(_owner.stop.subscribe(), &endpoint, || async {
            SqsEventConsumer::handle_sqs_message(&client, &endpoint, &handler, message.clone())
                .await;
            Ok(())
        })
        .await
    });
    tokio::time::timeout(Duration::from_secs(1), entries.recv())
        .await
        .expect("real handler was invoked")
        .expect("entry witness");
    let deadline = Instant::now() + Duration::from_secs(1);
    consumer.stop().await.expect("stop blocked handler");
    timeout_at(deadline, join_pollers(&mut tasks, &run.stop))
        .await
        .expect("bounded stop")
        .expect("normal stop");
    destroyed_before(deadline, vec![handler_dropped, child_dropped]).await;
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
        "unfinished handler must never issue DeleteMessage"
    );
}
