//! Permission checker client for `RustyCog` microservices
//!
//! This crate exposes the `PermissionChecker` trait plus the OpenFGA-backed
//! implementation used by every `RustyCog` service to ask the centralized
//! authorization engine whether a subject can perform an action on a resource.
//!
//! The trait is engine-neutral: `OpenFgaPermissionChecker` is the production
//! implementation, `InMemoryPermissionChecker` is provided for unit tests, and
//! `CachedPermissionChecker` decorates any underlying checker with a short-TTL
//! LRU cache.

pub use crate::rustycog_config::OpenFgaClientConfig;
use crate::rustycog_core::error::DomainError;
use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod checker;

pub use checker::{
    CachedPermissionChecker, InMemoryPermissionChecker, MetricsPermissionChecker,
    OpenFgaPermissionChecker,
};

/// One stored `OpenFGA` relationship tuple (`user` / `relation` / `object`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RelationshipTuple {
    /// Subject on the wire, e.g. `user:{uuid}` or `organization:{uuid}#member`.
    pub user: String,
    /// Relation name, e.g. `member` or `viewer`.
    pub relation: String,
    /// Object on the wire, e.g. `organization:{uuid}` or `project:{uuid}`.
    pub object: String,
}

/// Permission verbs recognized by the platform.
///
/// Each variant maps to a relation on every object type defined in the `OpenFGA`
/// authorization model (`openfga/model.fga`). The mapping is intentionally flat
/// so client services can describe authorization requirements uniformly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    Read,
    Write,
    Admin,
    Owner,
}

impl Permission {
    /// Every supported permission, in ascending order of privilege.
    #[must_use]
    pub fn all() -> Vec<Self> {
        vec![Self::Read, Self::Write, Self::Admin, Self::Owner]
    }

    /// Human-readable permission name (matches the old Casbin action names).
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Admin => "admin",
            Self::Owner => "owner",
        }
    }

    /// `OpenFGA` relation name for this permission.
    ///
    /// Every object type in the platform model exposes the same four derived
    /// relations (`read`, `write`, `administer`, `own`) so the checker only
    /// needs the `Permission` plus an object type to issue a `Check` call.
    #[must_use]
    pub const fn relation(&self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Admin => "administer",
            Self::Owner => "own",
        }
    }

    /// Parse a permission from its string representation.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError`] if `s` is not `read`, `write`, `admin`, or `owner`.
    ///
    /// Inherent method kept so call sites do not need [`std::str::FromStr`] in scope.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Result<Self, DomainError> {
        match s.to_lowercase().as_str() {
            "read" => Ok(Self::Read),
            "write" => Ok(Self::Write),
            "admin" => Ok(Self::Admin),
            "owner" => Ok(Self::Owner),
            _ => Err(DomainError::invalid_input(&format!(
                "Invalid permission level: {s}"
            ))),
        }
    }
}

impl std::fmt::Display for Permission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl TryFrom<String> for Permission {
    type Error = DomainError;

    /// Converts a known permission name.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError`] if `s` is not `read`, `write`, `admin`, or `owner`.
    /// Prefer [`Permission::from_str`] for `&str` input.
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::from_str(&s)
    }
}

/// Identifier for a resource that a permission applies to.
///
/// Always a UUID; the permission middleware only binds UUID-shaped path
/// segments into a `ResourceId`, so service routes must use UUID path
/// parameters when they want a resource-scoped guard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceId(Uuid);

impl From<Uuid> for ResourceId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl From<ResourceId> for Uuid {
    fn from(id: ResourceId) -> Self {
        id.0
    }
}

impl ResourceId {
    #[must_use]
    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }

    #[must_use]
    pub const fn new(id: Uuid) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.0
    }
}

impl std::fmt::Display for ResourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Discriminant for [`Subject`].
///
/// `User` is an authenticated UUID-bearing caller; `Wildcard` is the
/// anonymous "any user" subject (`user:*` on the `OpenFGA` wire).
/// Anonymous routes can hand a `Subject::wildcard()` to the checker
/// instead of failing closed, so a public-read tuple like
/// `project:{id}#viewer@user:*` is honored.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SubjectKind {
    #[serde(alias = "User")]
    LegacyPlatform,
    Principal(PrincipalRef),
    Wildcard,
}

const fn default_subject_kind() -> SubjectKind {
    SubjectKind::LegacyPlatform
}

/// Exact, verified identity. Deserialization validates the issuer, never normalizes it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "PrincipalRefWire")]
pub struct PrincipalRef {
    pub issuer: String,
    pub subject: Uuid,
}

#[derive(Deserialize)]
struct PrincipalRefWire {
    issuer: String,
    subject: Uuid,
}

impl TryFrom<PrincipalRefWire> for PrincipalRef {
    type Error = PrincipalError;
    fn try_from(value: PrincipalRefWire) -> Result<Self, Self::Error> {
        Self::new(value.issuer, value.subject)
    }
}

/// Stable internal refusal codes; never includes an issuer or token in diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PrincipalError {
    #[error("PRINCIPAL_INVALID")]
    Invalid,
    #[error("PRINCIPAL_INCOMPATIBLE")]
    Incompatible,
    #[error("PRINCIPAL_POLICY_UNAVAILABLE")]
    PolicyUnavailable,
}

impl PrincipalRef {
    pub fn new(issuer: impl Into<String>, subject: Uuid) -> Result<Self, PrincipalError> {
        let principal = Self {
            issuer: issuer.into(),
            subject,
        };
        principal.validate()?;
        Ok(principal)
    }

    pub fn validate(&self) -> Result<(), PrincipalError> {
        if self.issuer.is_empty() || self.issuer.len() > 256 || self.issuer.trim() != self.issuer {
            return Err(PrincipalError::Invalid);
        }
        Ok(())
    }
}

/// Deployment-pinned platform provenance, independent of the JWT algorithm/Host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrincipalPolicy {
    pub platform_issuer: String,
    pub allow_historical_platform_issuer: bool,
}

impl PrincipalPolicy {
    pub fn new(
        platform_issuer: impl Into<String>,
        allow_historical_platform_issuer: bool,
    ) -> Result<Self, PrincipalError> {
        let policy = Self {
            platform_issuer: platform_issuer.into(),
            allow_historical_platform_issuer,
        };
        PrincipalRef::new(policy.platform_issuer.clone(), Uuid::nil())?;
        Ok(policy)
    }

    pub fn is_platform(&self, principal: &PrincipalRef) -> Result<bool, PrincipalError> {
        principal.validate()?;
        PrincipalRef::new(self.platform_issuer.clone(), Uuid::nil())?;
        Ok(principal.issuer == self.platform_issuer
            || (self.allow_historical_platform_issuer && principal.issuer == "iamrusty"))
    }

    pub fn platform_user_id(&self, principal: &PrincipalRef) -> Result<Uuid, PrincipalError> {
        if self.is_platform(principal)? {
            Ok(principal.subject)
        } else {
            Err(PrincipalError::Incompatible)
        }
    }
}

/// Caller identified on the request — either an authenticated user
/// ([`Subject::new`]) or the wildcard "any user" subject
/// ([`Subject::wildcard`]) used by anonymous-callable routes.
///
/// On the `OpenFGA` wire `Subject::Display` renders as `user:{uuid}` for the
/// `User` kind and `user:*` for the `Wildcard` kind. The wildcard form is
/// only meaningful when the `OpenFGA` model declares the relevant relation
/// with a `[user, user:*]` type restriction (e.g. `project.viewer` in
/// `openfga/model.fga`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Subject {
    pub user_id: Uuid,
    /// Defaults to `SubjectKind::User` so payloads serialized before this
    /// field existed deserialize unchanged.
    #[serde(default = "default_subject_kind")]
    pub kind: SubjectKind,
    #[serde(default)]
    platform_compatible: bool,
}

impl Subject {
    /// Build a subject for a concrete authenticated user.
    #[must_use]
    pub const fn new(user_id: Uuid) -> Self {
        Self {
            user_id,
            kind: SubjectKind::LegacyPlatform,
            platform_compatible: true,
        }
    }

    /// Preserve the exact authority even when its wire identity is proven platform.
    pub fn from_principal(
        principal: PrincipalRef,
        policy: &PrincipalPolicy,
    ) -> Result<Self, PrincipalError> {
        let platform_compatible = policy.is_platform(&principal)?;
        Ok(Self {
            user_id: principal.subject,
            kind: SubjectKind::Principal(principal),
            platform_compatible,
        })
    }

    /// Build the wildcard "any user" subject. Renders as `user:*` on the
    /// `OpenFGA` wire. Use from `optional_permission_middleware` when the
    /// request carries no JWT, so public-read tuples
    /// (`...#viewer@user:*`) can be honored by the centralized checker.
    #[must_use]
    pub const fn wildcard() -> Self {
        Self {
            user_id: Uuid::nil(),
            kind: SubjectKind::Wildcard,
            platform_compatible: false,
        }
    }

    /// Returns `true` when the subject is the wildcard variant.
    #[must_use]
    pub const fn is_wildcard(&self) -> bool {
        matches!(self.kind, SubjectKind::Wildcard)
    }
}

impl std::fmt::Display for Subject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            SubjectKind::Wildcard => write!(f, "user:*"),
            SubjectKind::LegacyPlatform => write!(f, "user:{}", self.user_id),
            SubjectKind::Principal(principal) if self.platform_compatible => {
                write!(f, "user:{}", principal.subject)
            }
            SubjectKind::Principal(principal) => write!(
                f,
                "user:p1.{}.{}",
                URL_SAFE_NO_PAD.encode(principal.issuer.as_bytes()),
                principal.subject
            ),
        }
    }
}

/// Reference to a specific resource instance: a typed object id.
///
/// `object_type` must match an `OpenFGA` type defined in `openfga/model.fga`
/// (for example `"organization"`, `"project"`, `"component"`, `"notification"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceRef {
    pub object_type: &'static str,
    pub object_id: Uuid,
}

impl ResourceRef {
    #[must_use]
    pub const fn new(object_type: &'static str, object_id: Uuid) -> Self {
        Self {
            object_type,
            object_id,
        }
    }

    /// `type:id` rendering used by `OpenFGA` tuple encoding.
    #[must_use]
    pub fn as_object_string(&self) -> String {
        format!("{}:{}", self.object_type, self.object_id)
    }
}

impl std::fmt::Display for ResourceRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_object_string())
    }
}

/// Engine-neutral permission checker used by HTTP middleware and domain code.
///
/// The production implementation (`OpenFgaPermissionChecker`) issues a
/// `Check` call against the `OpenFGA` server; tests use the in-memory
/// implementation. Wrap any checker in `CachedPermissionChecker` to add a
/// short-TTL LRU cache in front of the network call.
#[async_trait]
pub trait PermissionChecker: Send + Sync {
    /// Return `Ok(true)` when `subject` has `action` on `resource`.
    async fn check(
        &self,
        subject: Subject,
        action: Permission,
        resource: ResourceRef,
    ) -> Result<bool, DomainError>;
}

#[cfg(test)]
mod subject_tests {
    use super::*;

    #[test]
    fn qualified_principal_encoding_preserves_utf8_and_size_bound() {
        let policy = PrincipalPolicy::new("https://platform.example/iam", false).unwrap();
        let id = Uuid::new_v4();
        for issuer in [
            "x".to_string(),
            "a".repeat(256),
            "é".repeat(128),
            "https://idp.example/é/iam".to_string(),
        ] {
            let subject =
                Subject::from_principal(PrincipalRef::new(issuer.clone(), id).unwrap(), &policy)
                    .unwrap();
            let wire = subject.to_string();
            assert!(wire.len() <= 387, "API fixture accepts at most 512 bytes");
            let encoded = wire
                .strip_prefix("user:p1.")
                .unwrap()
                .split('.')
                .next()
                .unwrap();
            assert_eq!(URL_SAFE_NO_PAD.decode(encoded).unwrap(), issuer.as_bytes());
            assert!(wire.ends_with(&id.to_string()));
        }
        for issuer in [
            String::new(),
            "a".repeat(257),
            format!("{}x", "é".repeat(128)),
            " padded".to_string(),
            "padded ".to_string(),
        ] {
            assert_eq!(PrincipalRef::new(issuer, id), Err(PrincipalError::Invalid));
        }
    }

    #[test]
    fn legacy_platform_grants_require_provenance() {
        let id = Uuid::new_v4();
        let policy = PrincipalPolicy::new("https://platform.example/iam", false).unwrap();
        let platform = Subject::from_principal(
            PrincipalRef::new(&policy.platform_issuer, id).unwrap(),
            &policy,
        )
        .unwrap();
        assert_eq!(platform.to_string(), Subject::new(id).to_string());
        assert_ne!(
            platform,
            Subject::new(id),
            "cache keys retain principal kind and authority"
        );
        for issuer in [
            "iamrusty",
            "https://unrelated.example/iam",
            "https://platform.example/iam/",
        ] {
            let principal = PrincipalRef::new(issuer, id).unwrap();
            assert_eq!(
                policy.platform_user_id(&principal),
                Err(PrincipalError::Incompatible)
            );
            assert!(Subject::from_principal(principal, &policy)
                .unwrap()
                .to_string()
                .starts_with("user:p1."));
        }
        let historical_policy = PrincipalPolicy::new(&policy.platform_issuer, true).unwrap();
        let historical = PrincipalRef::new("iamrusty", id).unwrap();
        assert_eq!(historical_policy.platform_user_id(&historical).unwrap(), id);
        assert_eq!(
            Subject::from_principal(historical, &historical_policy)
                .unwrap()
                .to_string(),
            Subject::new(id).to_string()
        );
    }

    #[test]
    fn user_subject_renders_as_user_uuid() {
        let id = Uuid::parse_str("01010101-0101-0101-0101-010101010101").unwrap();
        let subject = Subject::new(id);
        assert_eq!(
            subject.to_string(),
            "user:01010101-0101-0101-0101-010101010101"
        );
        assert!(!subject.is_wildcard());
    }

    #[test]
    fn wildcard_subject_renders_as_user_star() {
        let subject = Subject::wildcard();
        assert_eq!(subject.to_string(), "user:*");
        assert!(subject.is_wildcard());
    }

    #[test]
    fn legacy_subject_payload_without_kind_field_deserializes_as_user() {
        // Payloads serialized before `SubjectKind` existed only carry
        // `user_id`; `#[serde(default)]` on `kind` keeps them readable.
        let json = r#"{"user_id":"01010101-0101-0101-0101-010101010101"}"#;
        let subject: Subject = serde_json::from_str(json).expect("legacy payload should parse");
        assert!(matches!(subject.kind, SubjectKind::LegacyPlatform));
        assert_eq!(
            subject.to_string(),
            "user:01010101-0101-0101-0101-010101010101"
        );
    }
}
