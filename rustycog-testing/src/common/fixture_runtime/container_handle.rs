//! Private ownership backends. Only PostgreSQL uses the explicit Engine path.
use testcontainers::{
    bollard::{
        container::{InspectContainerOptions, RemoveContainerOptions, StopContainerOptions},
        errors::Error,
        Docker,
    },
    core::ContainerPort,
    ContainerAsync, GenericImage,
};

pub(super) enum ContainerHandle {
    Testcontainers(ContainerAsync<GenericImage>),
    Engine { client: Docker, id: String },
}

impl ContainerHandle {
    pub(super) fn id(&self) -> &str {
        match self {
            Self::Testcontainers(container) => container.id(),
            Self::Engine { id, .. } => id,
        }
    }

    pub(super) async fn get_host_port_ipv4(&self, internal: ContainerPort) -> Result<u16, String> {
        match self {
            Self::Testcontainers(container) => container
                .get_host_port_ipv4(internal)
                .await
                .map_err(|_| "cannot obtain daemon-mapped fixture port".into()),
            Self::Engine { client, id } => {
                let inspect = client
                    .inspect_container(id, None::<InspectContainerOptions>)
                    .await
                    .map_err(|_| "cannot inspect owned fixture mapping")?;
                let ports = inspect
                    .network_settings
                    .and_then(|settings| settings.ports)
                    .ok_or("owned fixture has no port mappings")?;
                ports
                    .get(&internal.to_string())
                    .and_then(Option::as_ref)
                    .into_iter()
                    .flatten()
                    .filter(|binding| {
                        binding.host_ip.as_deref().is_none_or(|ip| {
                            ip.is_empty() || ip.parse::<std::net::Ipv4Addr>().is_ok()
                        })
                    })
                    .find_map(|binding| binding.host_port.as_deref()?.parse::<u16>().ok())
                    .filter(|port| *port != 0)
                    .ok_or_else(|| "owned fixture has no usable IPv4 mapped port".into())
            }
        }
    }

    pub(super) async fn stop(&self) -> Result<(), String> {
        match self {
            Self::Testcontainers(container) => container
                .stop()
                .await
                .map_err(|_| "owned container stop failed".into()),
            Self::Engine { client, id } => {
                match client
                    .stop_container(id, Some(StopContainerOptions { t: -1 }))
                    .await
                {
                    Ok(())
                    | Err(Error::DockerResponseServerError {
                        status_code: 304, ..
                    }) => Ok(()),
                    Err(_) => Err("owned container stop failed".into()),
                }
            }
        }
    }

    pub(super) async fn rm(self) -> Result<(), String> {
        match self {
            Self::Testcontainers(container) => container
                .rm()
                .await
                .map_err(|_| "owned container teardown failed".into()),
            Self::Engine { ref client, ref id } => {
                let inspect = match client
                    .inspect_container(id, None::<InspectContainerOptions>)
                    .await
                {
                    Ok(inspect) => inspect,
                    Err(Error::DockerResponseServerError {
                        status_code: 404, ..
                    }) => return Ok(()),
                    Err(_) => return Err("cannot verify owned container teardown state".into()),
                };
                let state = inspect.state.ok_or("owned container state unavailable")?;
                if state.paused == Some(true) || state.restarting == Some(true) {
                    return Err("owned fixture has a transitional teardown state".into());
                }
                if state.running == Some(true) {
                    self.stop().await?;
                } else if state.running != Some(false) {
                    return Err("owned fixture running state unavailable".into());
                }
                // Non-forced removal also rejects a container that raced back to running.
                // Preserve volumes: no volume ownership is inferred from container identity.
                match client
                    .remove_container(
                        id,
                        Some(RemoveContainerOptions {
                            force: false,
                            v: false,
                            link: false,
                        }),
                    )
                    .await
                {
                    Ok(())
                    | Err(Error::DockerResponseServerError {
                        status_code: 404, ..
                    }) => Ok(()),
                    Err(_) => Err("owned container teardown failed".into()),
                }
            }
        }
    }
}
