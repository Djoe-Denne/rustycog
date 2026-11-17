//! Read-only, bounded observations of an exact owned fixture identity.
use super::{project_container_diagnostic, OwnedContainer};
use futures::StreamExt;
use serde_json::{json, Value};
use std::time::Duration;
use testcontainers::bollard::{
    container::LogsOptions,
    errors::Error,
    models::{ContainerState, ContainerStateStatusEnum},
};

#[derive(Debug)]
pub(crate) struct ContainerObservation {
    pub terminal: bool,
    pub diagnostic: Value,
}

impl ContainerObservation {
    fn from_state(id: &str, state: &ContainerState) -> Self {
        Self {
            // Created, paused, restarting and incomplete observations are not
            // terminal. A non-running flag alone does not establish an exit.
            terminal: state.running == Some(false)
                && matches!(
                    state.status,
                    Some(ContainerStateStatusEnum::EXITED | ContainerStateStatusEnum::DEAD)
                ),
            diagnostic: project_container_diagnostic(id, state),
        }
    }
}

impl OwnedContainer {
    pub(crate) async fn observe(&self) -> ContainerObservation {
        let result = tokio::time::timeout(Duration::from_secs(1), async {
            let docker = testcontainers::core::client::docker_client_instance()
                .await
                .map_err(|_| "unavailable")?;
            match docker.inspect_container(&self.id, None).await {
                Ok(inspect) if inspect.id.as_deref() == Some(self.id.as_str()) => inspect
                    .state
                    .map(|state| ContainerObservation::from_state(&self.id, &state))
                    .ok_or("unavailable"),
                Ok(_) => Err("identity_mismatch"),
                Err(Error::DockerResponseServerError {
                    status_code: 404, ..
                }) => Ok(ContainerObservation {
                    terminal: true,
                    diagnostic: json!({"id": self.id, "observation": "absent"}),
                }),
                Err(_) => Err("unavailable"),
            }
        })
        .await;
        match result {
            Ok(Ok(observed)) => observed,
            other => ContainerObservation {
                terminal: false,
                diagnostic: json!({
                    "id": self.id,
                    "observation": match other { Ok(Err(reason)) => reason, _ => "timeout" },
                }),
            },
        }
    }

    /// Only call after a terminal observation. No raw bytes escape this method.
    pub(crate) async fn startup_log_categories(&self) -> Value {
        let mut sample = LogSample::default();
        let result = tokio::time::timeout(Duration::from_secs(1), async {
            let docker = testcontainers::core::client::docker_client_instance()
                .await
                .map_err(|_| ())?;
            let mut stream = docker.logs(
                &self.id,
                Some(LogsOptions::<String> {
                    stdout: true,
                    stderr: true,
                    follow: false,
                    tail: "100".into(),
                    ..Default::default()
                }),
            );
            while let Some(frame) = stream.next().await {
                let frame = frame.map_err(|_| ())?;
                if !sample.push(frame.as_ref()) {
                    return Ok("limit");
                }
            }
            Ok::<_, ()>("complete")
        })
        .await;
        sample.project(match result {
            Ok(Ok(status)) => status,
            Ok(Err(())) => "unavailable",
            Err(_) => "timeout",
        })
    }
}

const BYTE_LIMIT: usize = 16 * 1024;
const FRAME_LIMIT: usize = 64;

#[derive(Default)]
struct LogSample {
    bytes: Vec<u8>,
    frames: usize,
}

impl LogSample {
    fn push(&mut self, bytes: &[u8]) -> bool {
        if self.frames >= FRAME_LIMIT || self.bytes.len() >= BYTE_LIMIT {
            return false;
        }
        self.frames += 1;
        let remaining = BYTE_LIMIT - self.bytes.len();
        self.bytes
            .extend_from_slice(&bytes[..bytes.len().min(remaining)]);
        bytes.len() < remaining && self.frames < FRAME_LIMIT
    }

    fn project(&self, observation: &'static str) -> Value {
        let contains = |needle: &[u8]| self.bytes.windows(needle.len()).any(|part| part == needle);
        json!({
            "observation": observation,
            "sampled_bytes": self.bytes.len(),
            "sampled_frames": self.frames,
            "ready_marker": contains(b"Ready."),
            "python_traceback": contains(b"Traceback (most recent call last)"),
            "permission_denied": contains(b"Permission denied"),
            "disk_full": contains(b"No space left on device"),
            "address_in_use": contains(b"Address already in use"),
            "memory_error_marker": contains(b"MemoryError") || contains(b"Out of memory"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_confirmed_terminal_states_are_terminal() {
        for (status, running, expected) in [
            ("exited", Some(false), true),
            ("dead", Some(false), true),
            ("running", Some(true), false),
            ("created", Some(false), false),
            ("paused", Some(false), false),
            ("restarting", Some(false), false),
            ("exited", None, false),
            ("exited", Some(true), false),
        ] {
            let state = serde_json::from_value(json!({
                "Status": status, "Running": running, "ExitCode": 245,
                "Error": "private-state-sentinel",
            }))
            .unwrap();
            let observed = ContainerObservation::from_state("owned-id", &state);
            assert_eq!(observed.terminal, expected, "{status} / {running:?}");
            assert!(!observed
                .diagnostic
                .to_string()
                .contains("private-state-sentinel"));
        }
    }

    #[test]
    fn log_projection_is_closed_bounded_and_handles_split_markers() {
        let mut sample = LogSample::default();
        assert!(sample.push(b"private-token-sentinel\nTraceback (most "));
        assert!(sample.push(b"recent call last)\nPermission denied\nReady."));
        let output = sample.project("complete");
        assert_eq!(output["python_traceback"], true);
        assert_eq!(output["permission_denied"], true);
        assert_eq!(output["ready_marker"], true);
        assert_eq!(output["disk_full"], false);
        assert!(!output.to_string().contains("private-token-sentinel"));
        assert!(!sample.push(&vec![b'x'; BYTE_LIMIT + 1]));
        assert_eq!(sample.bytes.len(), BYTE_LIMIT);
        assert!(!sample.push(b"MemoryError"));
        assert_eq!(sample.project("limit")["memory_error_marker"], false);
    }

    #[test]
    fn log_frame_budget_also_bounds_empty_frames() {
        let mut sample = LogSample::default();
        for _ in 0..FRAME_LIMIT - 1 {
            assert!(sample.push(b""));
        }
        assert!(!sample.push(b""));
        assert!(!sample.push(b"private-over-budget"));
        assert_eq!(sample.frames, FRAME_LIMIT);
        assert_eq!(sample.bytes.len(), 0);
    }
}
