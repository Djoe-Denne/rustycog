//! Opt-in, per-consumer test diagnostics. No payload, receipt or credential data
//! is exposed. Live snapshots are not transactional across concurrent pollers.

use std::sync::atomic::{AtomicU64, Ordering};

/// Counters owned by one test consumer, correlated to one expected message ID.
/// This facility is absent without `test-utils` (or SDK unit tests).
#[derive(Default)]
pub struct SqsConsumerProbe {
    expected_message_id: String,
    pub(super) receive_started: AtomicU64,
    pub(super) receive_succeeded: AtomicU64,
    pub(super) receive_failed: AtomicU64,
    pub(super) receive_cancelled: AtomicU64,
    pub(super) messages_returned: AtomicU64,
    pub(super) expected_message_returned: AtomicU64,
    pub(super) messages_without_id: AtomicU64,
    pub(super) missing_body: AtomicU64,
    pub(super) parse_failed: AtomicU64,
    pub(super) unsupported: AtomicU64,
    pub(super) handler_entered: AtomicU64,
    pub(super) expected_handler_entered: AtomicU64,
}

/// A payload-free sample, not an assertion about other consumers or broker state.
#[derive(Debug, serde::Serialize)]
pub struct SqsConsumerSnapshot {
    pub receive_started: u64,
    pub receive_succeeded: u64,
    pub receive_failed: u64,
    pub receive_cancelled: u64,
    pub messages_returned: u64,
    pub expected_message_returned: u64,
    pub messages_without_id: u64,
    pub missing_body: u64,
    pub parse_failed: u64,
    pub unsupported: u64,
    pub handler_entered: u64,
    pub expected_handler_entered: u64,
}

impl SqsConsumerProbe {
    #[must_use]
    pub fn new(expected_message_id: String) -> Self {
        Self {
            expected_message_id,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> SqsConsumerSnapshot {
        SqsConsumerSnapshot {
            receive_started: self.receive_started.load(Ordering::SeqCst),
            receive_succeeded: self.receive_succeeded.load(Ordering::SeqCst),
            receive_failed: self.receive_failed.load(Ordering::SeqCst),
            receive_cancelled: self.receive_cancelled.load(Ordering::SeqCst),
            messages_returned: self.messages_returned.load(Ordering::SeqCst),
            expected_message_returned: self.expected_message_returned.load(Ordering::SeqCst),
            messages_without_id: self.messages_without_id.load(Ordering::SeqCst),
            missing_body: self.missing_body.load(Ordering::SeqCst),
            parse_failed: self.parse_failed.load(Ordering::SeqCst),
            unsupported: self.unsupported.load(Ordering::SeqCst),
            handler_entered: self.handler_entered.load(Ordering::SeqCst),
            expected_handler_entered: self.expected_handler_entered.load(Ordering::SeqCst),
        }
    }

    pub(super) fn is_expected(&self, id: Option<&str>) -> bool {
        id.is_some_and(|id| id == self.expected_message_id)
    }

    pub(super) fn begin_receive(&self) -> ReceiveAttempt<'_> {
        self.receive_started.fetch_add(1, Ordering::SeqCst);
        ReceiveAttempt {
            probe: self,
            finished: false,
        }
    }
}

pub(super) struct ReceiveAttempt<'a> {
    probe: &'a SqsConsumerProbe,
    finished: bool,
}

impl ReceiveAttempt<'_> {
    pub(super) fn finish(mut self, succeeded: bool) {
        if succeeded {
            self.probe.receive_succeeded.fetch_add(1, Ordering::SeqCst);
        } else {
            self.probe.receive_failed.fetch_add(1, Ordering::SeqCst);
        }
        self.finished = true;
    }
}

impl Drop for ReceiveAttempt<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.probe.receive_cancelled.fetch_add(1, Ordering::SeqCst);
        }
    }
}
