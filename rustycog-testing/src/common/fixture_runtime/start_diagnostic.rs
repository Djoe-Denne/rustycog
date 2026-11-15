//! Closed diagnostic vocabulary: never format arbitrary library/daemon errors.
use testcontainers::{
    bollard::errors::Error as DockerError,
    core::{
        client::ClientError,
        error::{TestcontainersError, WaitContainerError},
    },
};

pub(super) fn describe(error: &TestcontainersError) -> String {
    match error {
        TestcontainersError::Client(error) => match error {
            ClientError::Init(error) => docker("client_init", error),
            ClientError::CreateContainer(error) => docker("create_container", error),
            ClientError::StartContainer(error) => docker("start_container", error),
            ClientError::InspectContainer(error) => docker("inspect_container", error),
            ClientError::PullImage { err, .. } => docker("pull_image", err),
            ClientError::PortMapping(_) => "stage=port_mapping".into(),
            _ => "stage=docker_client reason=unclassified".into(),
        },
        TestcontainersError::WaitContainer(error) => {
            let reason = match error {
                WaitContainerError::StartupTimeout => "startup_timeout",
                WaitContainerError::Unhealthy => "unhealthy",
                WaitContainerError::UnexpectedExitCode { .. } => "unexpected_exit",
                _ => "readiness_failed",
            };
            format!("stage=readiness reason={reason}")
        }
        TestcontainersError::Io(error) => io("io", error),
        TestcontainersError::PortNotExposed { .. } => "stage=port_mapping".into(),
        TestcontainersError::MissingInfo(_) => "stage=container_metadata".into(),
        TestcontainersError::Exec(_) => "stage=exec".into(),
        TestcontainersError::Other(_) => "stage=other reason=unclassified".into(),
    }
}

pub(super) fn docker(stage: &str, error: &DockerError) -> String {
    match error {
        DockerError::DockerResponseServerError {
            status_code,
            message,
        } => format!(
            "stage={stage} http_status={status_code} reason={}",
            daemon_reason(message)
        ),
        DockerError::IOError { err } => io(stage, err),
        DockerError::RequestTimeoutError => format!("stage={stage} reason=request_timeout"),
        DockerError::SocketNotFoundError(_) => format!("stage={stage} reason=socket_not_found"),
        _ => format!("stage={stage} reason=docker_transport_or_protocol"),
    }
}

fn io(stage: &str, error: &std::io::Error) -> String {
    // ErrorKind and numeric OS codes are safe; Display/Debug of Error is not.
    format!(
        "stage={stage} io_kind={:?} os_code={:?}",
        error.kind(),
        error.raw_os_error()
    )
}

fn daemon_reason(message: &str) -> &'static str {
    let message = message.to_ascii_lowercase();
    if message.contains("permission denied")
        || message.contains("forbidden by its access permissions")
    {
        "permission_denied"
    } else if message.contains("port is already allocated")
        || message.contains("address already in use")
    {
        "port_in_use"
    } else if message.contains("ports are not available") {
        "port_unavailable"
    } else if message.contains("container name") && message.contains("already in use") {
        "name_in_use"
    } else {
        "unclassified"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_diagnostics_classify_without_echoing_any_message() {
        for (message, reason) in [
            ("bind: permission denied", "permission_denied"),
            ("port is already allocated", "port_in_use"),
            ("bind: address already in use", "port_in_use"),
            ("Ports are not available", "port_unavailable"),
            ("The container name is already in use", "name_in_use"),
            ("unexpected daemon failure", "unclassified"),
        ] {
            let error = TestcontainersError::Client(ClientError::StartContainer(
                DockerError::DockerResponseServerError {
                    status_code: 500,
                    message: format!("{message}; password=secret-alpha token=secret-beta\n"),
                },
            ));
            assert_eq!(
                describe(&error),
                format!("stage=start_container http_status=500 reason={reason}")
            );
        }
    }

    #[test]
    fn typed_diagnostics_hide_paths_image_credentials_and_other_payloads() {
        let errors = [
            (
                TestcontainersError::Client(ClientError::PullImage {
                    descriptor: "secret-alpha:secret-beta@private.registry/image".into(),
                    err: DockerError::RequestTimeoutError,
                }),
                "stage=pull_image reason=request_timeout".to_string(),
            ),
            (
                TestcontainersError::Client(ClientError::Init(DockerError::SocketNotFoundError(
                    "secret-alpha/secret-beta".into(),
                ))),
                "stage=client_init reason=socket_not_found".to_string(),
            ),
            (
                TestcontainersError::Io(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "secret-alpha secret-beta",
                )),
                "stage=io io_kind=PermissionDenied os_code=None".to_string(),
            ),
            (
                TestcontainersError::WaitContainer(WaitContainerError::StartupTimeout),
                "stage=readiness reason=startup_timeout".to_string(),
            ),
            (
                TestcontainersError::other("secret-alpha secret-beta"),
                "stage=other reason=unclassified".to_string(),
            ),
        ];
        for (error, expected) in errors {
            assert_eq!(describe(&error), expected);
        }
        let error = TestcontainersError::Io(std::io::Error::from_raw_os_error(10013));
        assert!(describe(&error).contains("os_code=Some(10013)"));
    }
}
