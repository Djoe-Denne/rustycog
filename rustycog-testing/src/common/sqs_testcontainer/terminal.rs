//! Preserve the first confirmed terminal observation without replacing its owner.
use crate::testing::common::fixture_runtime::ContainerObservation;
use serde_json::Value;
use std::future::Future;
use tokio::sync::Mutex;

#[derive(Default)]
pub(super) struct TerminalFixture(Mutex<Option<String>>);

impl TerminalFixture {
    pub(super) async fn check(
        &self,
        observe: impl Future<Output = ContainerObservation>,
        logs: impl Future<Output = Value>,
    ) -> Result<ContainerObservation, String> {
        let mut first = self.0.lock().await;
        if let Some(error) = first.as_ref() {
            return Err(error.clone());
        }
        let observed = observe.await;
        if !observed.terminal {
            return Ok(observed);
        }
        // Publish the identity before awaiting logs: cancellation must not lose
        // the first confirmed exit or allow later protocol waits against it.
        *first = Some(format!(
            "LocalStack SQS fixture terminal; fixture={}; startup_log_categories={{\"observation\":\"not_completed\"}}",
            observed.diagnostic,
        ));
        let categories = logs.await;
        let error = format!(
            "LocalStack SQS fixture terminal; fixture={}; startup_log_categories={}",
            observed.diagnostic, categories,
        );
        *first = Some(error.clone());
        eprintln!(
            "RUSTYCOG_SQS_TERMINAL {}",
            serde_json::json!({
                "fixture": observed.diagnostic,
                "startup_log_categories": categories,
            })
        );
        Err(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn observed(terminal: bool) -> ContainerObservation {
        ContainerObservation {
            terminal,
            diagnostic: json!({"id": "first-owned-id"}),
        }
    }

    #[tokio::test]
    async fn terminal_fixture_reuses_first_error_without_inspection_logs_or_protocol_wait() {
        let fixture = TerminalFixture::default();
        let protocol_calls = AtomicUsize::new(0);
        let first = fixture
            .check(async { observed(true) }, async {
                json!({"python_traceback": true})
            })
            .await
            .unwrap_err();
        for _ in 0..3 {
            let result = fixture
                .check(
                    async { panic!("latched fixture must not inspect again") },
                    async { panic!("latched fixture must not collect logs again") },
                )
                .await;
            if result.is_ok() {
                protocol_calls.fetch_add(1, Ordering::SeqCst);
            }
            assert_eq!(result.err().unwrap(), first);
        }
        assert_eq!(protocol_calls.load(Ordering::SeqCst), 0);
        assert!(first.contains("first-owned-id"));
        assert!(first.contains("python_traceback"));
    }

    #[tokio::test]
    async fn running_and_unknown_do_not_latch_or_collect_terminal_logs() {
        let fixture = TerminalFixture::default();
        for status in ["running", "unavailable", "timeout", "identity_mismatch"] {
            let result = fixture
                .check(
                    async {
                        ContainerObservation {
                            terminal: false,
                            diagnostic: json!({"observation": status}),
                        }
                    },
                    async { panic!("nonterminal observations must not request logs") },
                )
                .await
                .unwrap();
            assert_eq!(result.diagnostic["observation"], status);
        }
        assert!(fixture
            .check(async { observed(true) }, async { json!({}) })
            .await
            .is_err());
    }

    #[tokio::test]
    async fn cancellation_during_logs_preserves_the_first_terminal_identity() {
        let fixture = TerminalFixture::default();
        let mut check = Box::pin(fixture.check(async { observed(true) }, std::future::pending()));
        assert!(futures::poll!(check.as_mut()).is_pending());
        drop(check);
        let error = fixture
            .check(
                async { panic!("the terminal identity was already observed") },
                async { panic!("cancelled diagnostics must not be retried") },
            )
            .await
            .err()
            .unwrap();
        assert!(error.contains("first-owned-id"));
        assert!(error.contains("not_completed"));
    }
}
