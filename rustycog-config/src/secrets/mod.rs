//! Explicit, bounded startup resolution of OpenBao KV v2 references.
//! No ambient credentials, fallback, refresh or application-specific logic.

use async_trait::async_trait;
use reqwest::{header, Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::io::AsyncReadExt;
use url::Url;

const RESPONSE_LIMIT: usize = 64 * 1024;
const VALUE_LIMIT: usize = 16 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Sanitized errors intentionally carry no remote or secret-bearing source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SecretError {
    /// Configuration is invalid.
    #[error("invalid secret resolver configuration")]
    InvalidConfig,
    /// Reference is invalid.
    #[error("invalid secret reference")]
    InvalidReference,
    /// Authentication or authorization failed.
    #[error("secret authentication failed")]
    AuthenticationFailed,
    /// Transport failed.
    #[error("secret transport failed")]
    TransportFailed,
    /// Path or field was not found.
    #[error("secret not found")]
    NotFound,
    /// Invalid remote representation.
    #[error("invalid secret response")]
    InvalidResponse,
    /// Empty or invalid secret.
    #[error("invalid secret value")]
    InvalidValue,
    /// A size limit was exceeded.
    #[error("secret size limit exceeded")]
    LimitExceeded,
    /// The bounded operation timed out.
    #[error("secret operation timed out")]
    Timeout,
}

/// A validated KV v2 mount/path/literal field, not an executable expression.
#[derive(Clone, Serialize, Deserialize)]
#[serde(try_from = "ReferenceFields")]
pub struct SecretReference {
    mount: String,
    path: String,
    field: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceFields {
    mount: String,
    path: String,
    field: String,
}

impl TryFrom<ReferenceFields> for SecretReference {
    type Error = SecretError;
    fn try_from(value: ReferenceFields) -> Result<Self, Self::Error> {
        Self::new(value.mount, value.path, value.field)
    }
}

impl SecretReference {
    /// Validate all components without including them in errors.
    ///
    /// # Errors
    /// Returns `InvalidReference` for unsafe, empty or oversized components.
    pub fn new(mount: String, path: String, field: String) -> Result<Self, SecretError> {
        fn segment(value: &str) -> bool {
            !value.is_empty()
                && value != "."
                && value != ".."
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        }
        if [mount.as_str(), path.as_str(), field.as_str()]
            .iter()
            .any(|v| v.len() > 1024)
            || !mount.split('/').all(segment)
            || !path.split('/').all(segment)
            || field.is_empty()
            || !field
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        {
            return Err(SecretError::InvalidReference);
        }
        Ok(Self { mount, path, field })
    }
}

/// An explicitly exposed/transferred secret; never serializable or implicitly cloned.
pub struct SecretValue(String);

impl SecretValue {
    /// Keep the exact nonblank UTF-8 value (no trimming or decoding).
    ///
    /// # Errors
    /// Returns `InvalidValue` for blank values or `LimitExceeded` above 16 KiB.
    pub fn new(value: String) -> Result<Self, SecretError> {
        if value.len() > VALUE_LIMIT {
            return Err(SecretError::LimitExceeded);
        }
        if value.trim().is_empty() {
            return Err(SecretError::InvalidValue);
        }
        Ok(Self(value))
    }
    /// Borrow the value only at its consuming boundary.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
    /// Transfer ownership to the consuming component.
    pub fn into_inner(self) -> String {
        self.0
    }
}

macro_rules! redacted_debug {
    ($($ty:ty),+ $(,)?) => {$(
        impl fmt::Debug for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($ty), "([REDACTED])"))
            }
        }
    )+};
}

/// Explicit authentication. A workload agent or bounded bootstrap supplies the file.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
pub enum OpenBaoAuthConfig {
    /// A plain, non-root token with a remaining lifetime of at most one hour.
    TokenFile {
        /// Explicit credential file; never read from ambient token variables.
        path: PathBuf,
    },
}

/// Declarative transport and identity configuration; contains no credentials.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenBaoConfig {
    /// HTTPS origin only, not an API path or a credential-bearing URL.
    pub endpoint: String,
    /// Explicit identity bootstrap.
    pub auth: OpenBaoAuthConfig,
    /// Local-only opt-in: HTTP to a literal loopback IP address.
    #[serde(default)]
    pub allow_loopback_http: bool,
    /// Optional additional trusted PEM roots. Verification stays enabled.
    #[serde(default)]
    pub ca_cert_file: Option<PathBuf>,
}

/// Async, application-agnostic secret resolution port.
#[async_trait]
pub trait SecretResolver: Send + Sync {
    /// Resolve once; failures never select another provider or a default value.
    async fn resolve(&self, reference: &SecretReference) -> Result<SecretValue, SecretError>;
}

/// Dedicated client, discarded after startup by consumers; no refresh/cache engine.
pub struct OpenBaoKv2Resolver {
    client: Client,
    origin: Url,
    token: header::HeaderValue,
}

redacted_debug!(
    SecretReference,
    SecretValue,
    OpenBaoAuthConfig,
    OpenBaoConfig,
    OpenBaoKv2Resolver
);

fn endpoint(config: &OpenBaoConfig) -> Result<Url, SecretError> {
    let url = Url::parse(&config.endpoint).map_err(|_| SecretError::InvalidConfig)?;
    let loopback = match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    };
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || url.host().is_none()
        || !(url.scheme() == "https"
            || (url.scheme() == "http" && config.allow_loopback_http && loopback))
    {
        return Err(SecretError::InvalidConfig);
    }
    Ok(url)
}

async fn read_regular_file(path: &Path, limit: usize) -> Result<Vec<u8>, SecretError> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|_| SecretError::InvalidConfig)?;
    if !metadata.is_file() {
        return Err(SecretError::InvalidConfig);
    }
    if metadata.len() > limit as u64 {
        return Err(SecretError::LimitExceeded);
    }
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|_| SecretError::InvalidConfig)?;
    if !file
        .metadata()
        .await
        .map_err(|_| SecretError::InvalidConfig)?
        .is_file()
    {
        return Err(SecretError::InvalidConfig);
    }
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| SecretError::InvalidConfig)?;
    if bytes.len() > limit {
        return Err(SecretError::LimitExceeded);
    }
    Ok(bytes)
}

fn transport_error(error: &reqwest::Error) -> SecretError {
    if error.is_timeout() {
        SecretError::Timeout
    } else {
        SecretError::TransportFailed
    }
}

impl OpenBaoKv2Resolver {
    /// Authenticate the explicit token, rejecting root policies and unbounded TTLs.
    ///
    /// # Errors
    /// Returns only sanitized configuration, authentication, limit or transport errors.
    pub async fn connect(config: &OpenBaoConfig) -> Result<Self, SecretError> {
        let origin = endpoint(config)?;
        let mut builder = Client::builder()
            .no_proxy()
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .no_zstd()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(Duration::from_secs(3));
        if let Some(path) = &config.ca_cert_file {
            let pem = read_regular_file(path, 1024 * 1024).await?;
            let certificates = reqwest::Certificate::from_pem_bundle(&pem)
                .map_err(|_| SecretError::InvalidConfig)?;
            if certificates.is_empty() {
                return Err(SecretError::InvalidConfig);
            }
            for certificate in certificates {
                builder = builder.add_root_certificate(certificate);
            }
        }
        let OpenBaoAuthConfig::TokenFile { path } = &config.auth;
        let raw = read_regular_file(path, VALUE_LIMIT).await?;
        let raw = std::str::from_utf8(&raw).map_err(|_| SecretError::InvalidConfig)?;
        let token = raw
            .strip_suffix("\r\n")
            .or_else(|| raw.strip_suffix('\n'))
            .unwrap_or(raw);
        if token.is_empty() || token.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(SecretError::InvalidConfig);
        }
        let mut token =
            header::HeaderValue::from_str(token).map_err(|_| SecretError::InvalidConfig)?;
        token.set_sensitive(true);
        let resolver = Self {
            client: builder.build().map_err(|_| SecretError::InvalidConfig)?,
            origin,
            token,
        };
        let value = resolver
            .get(&["auth", "token", "lookup-self"], true)
            .await
            .map_err(|error| match error {
                SecretError::LimitExceeded | SecretError::Timeout => error,
                _ => SecretError::AuthenticationFailed,
            })?;
        let data = value
            .get("data")
            .and_then(serde_json::Value::as_object)
            .ok_or(SecretError::AuthenticationFailed)?;
        let valid_policies = |value: &serde_json::Value| {
            value.as_array().is_some_and(|policies| {
                policies
                    .iter()
                    .all(|v| v.as_str().is_some_and(|p| p != "root"))
            })
        };
        if !data.get("policies").is_some_and(valid_policies)
            || data
                .get("identity_policies")
                .is_some_and(|p| !valid_policies(p))
            || !data
                .get("ttl")
                .and_then(serde_json::Value::as_u64)
                .is_some_and(|ttl| (1..=3600).contains(&ttl))
        {
            return Err(SecretError::AuthenticationFailed);
        }
        Ok(resolver)
    }

    async fn get(&self, segments: &[&str], lookup: bool) -> Result<serde_json::Value, SecretError> {
        let operation = async {
            let mut url = self.origin.clone();
            url.path_segments_mut()
                .map_err(|_| SecretError::InvalidConfig)?
                .extend(["v1"].into_iter().chain(segments.iter().copied()));
            let mut response = self
                .client
                .get(url)
                .header("X-Vault-Token", self.token.clone())
                .send()
                .await
                .map_err(|e| transport_error(&e))?;
            if response.status() != StatusCode::OK {
                return Err(
                    if lookup
                        || matches!(
                            response.status(),
                            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
                        )
                    {
                        SecretError::AuthenticationFailed
                    } else if response.status() == StatusCode::NOT_FOUND {
                        SecretError::NotFound
                    } else {
                        SecretError::TransportFailed
                    },
                );
            }
            if response
                .headers()
                .get(header::CONTENT_ENCODING)
                .is_some_and(|h| h != "identity")
            {
                return Err(SecretError::InvalidResponse);
            }
            if response
                .content_length()
                .is_some_and(|len| len > RESPONSE_LIMIT as u64)
            {
                return Err(SecretError::LimitExceeded);
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|e| transport_error(&e))? {
                if chunk.len() > RESPONSE_LIMIT - bytes.len() {
                    return Err(SecretError::LimitExceeded);
                }
                bytes.extend_from_slice(&chunk);
            }
            serde_json::from_slice(&bytes).map_err(|_| {
                if lookup {
                    SecretError::AuthenticationFailed
                } else {
                    SecretError::InvalidResponse
                }
            })
        };
        tokio::time::timeout(REQUEST_TIMEOUT, operation)
            .await
            .map_err(|_| SecretError::Timeout)?
    }
}

#[async_trait]
impl SecretResolver for OpenBaoKv2Resolver {
    async fn resolve(&self, reference: &SecretReference) -> Result<SecretValue, SecretError> {
        let segments: Vec<_> = reference
            .mount
            .split('/')
            .chain(["data"])
            .chain(reference.path.split('/'))
            .collect();
        let value = self.get(&segments, false).await?;
        let data = value
            .get("data")
            .and_then(|v| v.get("data"))
            .and_then(serde_json::Value::as_object)
            .ok_or(SecretError::InvalidResponse)?;
        let field = data.get(&reference.field).ok_or(SecretError::NotFound)?;
        SecretValue::new(
            field
                .as_str()
                .ok_or(SecretError::InvalidResponse)?
                .to_owned(),
        )
    }
}

#[cfg(test)]
mod tests;
