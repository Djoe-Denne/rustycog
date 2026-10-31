use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{Duration, Utc};
use jsonwebtoken::{
    encode,
    jwk::{AlgorithmParameters, CommonParameters, Jwk, RSAKeyParameters, RSAKeyType},
    Algorithm, EncodingKey, Header,
};
use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};
use rsa::rand_core::{CryptoRng, RngCore};
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde::Serialize;
use std::sync::OnceLock;
use uuid::Uuid;

pub const TEST_HS256_SECRET: &str = "rustycog-test-hs256-secret";
pub const TEST_JWT_ISSUER: &str = "iamrusty";
pub const TEST_JWT_AUDIENCE: &str = "aiforall";

/// Opaque RS256 key id for tests (not an org id or URL).
pub const TEST_RS256_KID: &str = "test-rs256-kid-01";

/// Platform issuer matching the custom JWK `iss` field in [`test_rs256_jwks_json`].
pub const TEST_PLATFORM_ISSUER: &str = "http://127.0.0.1/iam";

struct TestRs256Material {
    private_pem: String,
    public_pem: String,
    n: String,
    e: String,
}

/// Deterministic CSPRNG stand-in so the fixture is generated at process start,
/// not pasted as PKCS#8 in the source.
struct SplitMix64(u64);

impl CryptoRng for SplitMix64 {}

impl RngCore for SplitMix64 {
    fn next_u32(&mut self) -> u32 {
        self.next_u64() as u32
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        for chunk in dest.chunks_mut(8) {
            let n = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&n[..chunk.len()]);
        }
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rsa::rand_core::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

fn generate_test_rs256_material() -> TestRs256Material {
    let mut rng = SplitMix64(0xA1F0_4A11_C0DE_5EED);
    let private = RsaPrivateKey::new(&mut rng, 2048)
        .unwrap_or_else(|err| panic!("test RS256 fixture keygen: {err}"));
    let public = RsaPublicKey::from(&private);
    let private_pem = private
        .to_pkcs8_pem(LineEnding::LF)
        .unwrap_or_else(|err| panic!("test RS256 PKCS#8 encode: {err}"))
        .to_string();
    let public_pem = public
        .to_public_key_pem(LineEnding::LF)
        .unwrap_or_else(|err| panic!("test RS256 SPKI encode: {err}"));
    TestRs256Material {
        n: URL_SAFE_NO_PAD.encode(public.n().to_bytes_be()),
        e: URL_SAFE_NO_PAD.encode(public.e().to_bytes_be()),
        private_pem,
        public_pem,
    }
}

fn test_rs256_material() -> &'static TestRs256Material {
    static CELL: OnceLock<TestRs256Material> = OnceLock::new();
    CELL.get_or_init(generate_test_rs256_material)
}

/// Process-local RSA private key (PKCS#8 PEM) for RS256 test tokens.
#[must_use]
pub fn test_rs256_private_pem() -> &'static str {
    test_rs256_material().private_pem.as_str()
}

/// Process-local RSA public key (SPKI PEM) matching [`test_rs256_private_pem`].
#[must_use]
pub fn test_rs256_public_pem() -> &'static str {
    test_rs256_material().public_pem.as_str()
}

fn test_rs256_n() -> &'static str {
    test_rs256_material().n.as_str()
}

fn test_rs256_e() -> &'static str {
    test_rs256_material().e.as_str()
}

#[derive(Debug, Serialize)]
struct TestClaims {
    sub: String,
    iss: String,
    aud: String,
    exp: usize,
    iat: usize,
    jti: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    org: Option<Uuid>,
}

/// Create a JWT token for the given user ID with a shared HS256 test secret
#[must_use]
pub fn create_jwt_token(user_id: Uuid) -> String {
    create_jwt_token_with_secret(user_id, TEST_HS256_SECRET)
}

/// Create a JWT token with a caller-provided HS256 secret
///
/// # Panics
///
/// Panics if the token cannot be encoded with HS256.
#[must_use]
#[allow(clippy::expect_used)]
pub fn create_jwt_token_with_secret(user_id: Uuid, secret: &str) -> String {
    let now = Utc::now();
    let claims = TestClaims {
        sub: user_id.to_string(),
        iss: TEST_JWT_ISSUER.to_string(),
        aud: TEST_JWT_AUDIENCE.to_string(),
        exp: unix_ts_as_usize((now + Duration::hours(1)).timestamp()),
        iat: unix_ts_as_usize(now.timestamp()),
        jti: Uuid::new_v4().to_string(),
        org: None,
    };

    let header = Header::new(Algorithm::HS256);
    encode(
        &header,
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("failed to encode test JWT")
}

/// Create a platform RS256 access token (`typ=aiforall-access+jwt`).
///
/// # Panics
///
/// Panics if the token cannot be encoded with the test RSA fixture key.
#[must_use]
#[allow(clippy::expect_used)]
pub fn create_rs256_jwt_token(user_id: Uuid) -> String {
    encode_rs256(user_id, &Rs256TokenOptions::default(), None)
}

/// Mint a platform fixture token with the test instance's configured issuer.
///
/// # Panics
///
/// Panics if the nonsecret test key cannot encode the token.
#[must_use]
pub fn create_rs256_jwt_token_with_issuer(user_id: Uuid, issuer: &str) -> String {
    create_rs256_jwt_token_with_options(
        user_id,
        Rs256TokenOptions {
            iss: Some(issuer),
            ..Default::default()
        },
    )
}

/// Publisher lifecycle metadata for a nonsecret test JWK.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TestSigningKeyStatus {
    Pending,
    Active,
    Retiring,
    /// For explicit negative validator tests; real publishers omit revoked keys.
    Revoked,
}

/// A typed canonical JWK using the process-local RSA fixture material.
///
/// Constructors bind scope and organization together, never infer trust from
/// issuer text. Wire names match the IAM publisher, including platform null.
#[derive(Debug, Clone, Serialize)]
pub struct CanonicalJwk {
    kty: &'static str,
    #[serde(rename = "use")]
    usage: &'static str,
    alg: &'static str,
    kid: String,
    n: &'static str,
    e: &'static str,
    iss: String,
    status: TestSigningKeyStatus,
    trust_scope: &'static str,
    organization_id: Option<Uuid>,
}

impl CanonicalJwk {
    #[must_use]
    pub fn platform(issuer: impl Into<String>) -> Self {
        Self {
            kty: "RSA",
            usage: "sig",
            alg: "RS256",
            kid: TEST_RS256_KID.into(),
            n: test_rs256_n(),
            e: test_rs256_e(),
            iss: issuer.into(),
            status: TestSigningKeyStatus::Active,
            trust_scope: "platform",
            organization_id: None,
        }
    }

    #[must_use]
    pub fn organization(issuer: impl Into<String>, organization_id: Uuid) -> Self {
        Self {
            kid: organization_test_kid(organization_id),
            trust_scope: "organization",
            organization_id: Some(organization_id),
            ..Self::platform(issuer)
        }
    }

    #[must_use]
    pub const fn with_status(mut self, status: TestSigningKeyStatus) -> Self {
        self.status = status;
        self
    }

    #[must_use]
    pub fn with_kid(mut self, kid: impl Into<String>) -> Self {
        self.kid = kid.into();
        self
    }

    #[must_use]
    pub fn kid(&self) -> &str {
        &self.kid
    }

    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.iss
    }

    /// Serialize one fixture key with canonical publisher metadata.
    ///
    /// # Panics
    ///
    /// Panics if these fixed, JSON-compatible fixture fields cannot serialize.
    #[must_use]
    #[allow(clippy::expect_used)]
    pub fn to_jwks_json(&self) -> String {
        serde_json::to_string(&serde_json::json!({ "keys": [self] })).expect("canonical test JWKS")
    }
}

fn organization_test_kid(organization_id: Uuid) -> String {
    format!("test-org-rs256-{organization_id}")
}

/// Canonical organization JWKS, with a distinct kid bound to its owner.
#[must_use]
pub fn test_organization_rs256_jwks_json(organization_id: Uuid, issuer: &str) -> String {
    CanonicalJwk::organization(issuer, organization_id).to_jwks_json()
}

/// Mint a token matching [`test_organization_rs256_jwks_json`].
///
/// # Panics
///
/// Panics if the nonsecret RSA fixture key cannot encode the token.
#[must_use]
pub fn create_organization_rs256_jwt_token(
    user_id: Uuid,
    organization_id: Uuid,
    issuer: &str,
) -> String {
    let kid = organization_test_kid(organization_id);
    encode_rs256(
        user_id,
        &Rs256TokenOptions {
            iss: Some(issuer),
            kid: Some(Some(&kid)),
            ..Default::default()
        },
        Some(organization_id),
    )
}

/// JWKS JSON document matching [`test_rs256_private_pem`] / [`TEST_RS256_KID`].
#[must_use]
pub fn test_rs256_jwks_json() -> String {
    test_rs256_jwks_json_with_iss(TEST_PLATFORM_ISSUER)
}

/// JWKS JSON with a caller-provided custom JWK `iss` field.
#[must_use]
pub fn test_rs256_jwks_json_with_iss(iss: &str) -> String {
    CanonicalJwk::platform(iss).to_jwks_json()
}

/// Options for minting deliberately malformed / variant RS256 test tokens.
#[derive(Debug, Clone, Default)]
pub struct Rs256TokenOptions<'a> {
    /// Override `iss` claim (default [`TEST_PLATFORM_ISSUER`]).
    pub iss: Option<&'a str>,
    /// `None` = default kid; `Some(None)` = omit kid; `Some(Some(k))` = custom kid.
    pub kid: Option<Option<&'a str>>,
    /// `None` = `aiforall-access+jwt`; `Some(None)` = omit typ; `Some(Some(t))` = custom.
    pub typ: Option<Option<&'a str>>,
    /// Set a non-empty `jku` header when `Some`.
    pub jku: Option<&'a str>,
    /// Set a non-empty `x5u` header when `Some`.
    pub x5u: Option<&'a str>,
    /// When true, embed an inline `jwk` header (should be rejected by verifiers).
    pub include_jwk_header: bool,
}

/// Mint an RS256 token with custom header/claim overrides for negative tests.
///
/// # Panics
///
/// Panics if encoding fails.
#[must_use]
#[allow(clippy::expect_used)]
pub fn create_rs256_jwt_token_with_options(
    user_id: Uuid,
    options: Rs256TokenOptions<'_>,
) -> String {
    encode_rs256(user_id, &options, None)
}

#[allow(clippy::expect_used)]
fn encode_rs256(user_id: Uuid, options: &Rs256TokenOptions<'_>, org: Option<Uuid>) -> String {
    let iss = options.iss.unwrap_or(TEST_PLATFORM_ISSUER);
    let kid = options.kid.unwrap_or(Some(TEST_RS256_KID));
    let typ = options.typ.unwrap_or(Some("aiforall-access+jwt"));
    let now = Utc::now();
    let claims = TestClaims {
        sub: user_id.to_string(),
        iss: iss.to_string(),
        aud: TEST_JWT_AUDIENCE.to_string(),
        exp: unix_ts_as_usize((now + Duration::hours(1)).timestamp()),
        iat: unix_ts_as_usize(now.timestamp()),
        jti: Uuid::new_v4().to_string(),
        org,
    };

    let mut header = Header::new(Algorithm::RS256);
    header.typ = typ.map(str::to_string);
    header.kid = kid.map(str::to_string);
    header.jku = options.jku.map(str::to_string);
    header.x5u = options.x5u.map(str::to_string);
    if options.include_jwk_header {
        header.jwk = Some(Jwk {
            common: CommonParameters::default(),
            algorithm: AlgorithmParameters::RSA(RSAKeyParameters {
                key_type: RSAKeyType::RSA,
                n: test_rs256_n().to_string(),
                e: test_rs256_e().to_string(),
            }),
        });
    }

    let key = EncodingKey::from_rsa_pem(test_rs256_private_pem().as_bytes())
        .expect("test RSA private key must parse");
    encode(&header, &claims, &key).expect("failed to encode RS256 test JWT")
}

fn unix_ts_as_usize(ts: i64) -> usize {
    usize::try_from(ts).unwrap_or(0)
}

#[cfg(test)]
mod canonical_fixture_tests {
    use super::*;
    use jsonwebtoken::{decode, decode_header, DecodingKey, Validation};
    use serde_json::Value;

    fn verify_fixture_token(doc: &str, token: &str, issuer: &str) -> Value {
        let doc: Value = serde_json::from_str(doc).unwrap();
        let jwk: Jwk = serde_json::from_value(doc["keys"][0].clone()).unwrap();
        let key = DecodingKey::from_jwk(&jwk).unwrap();
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&[TEST_JWT_AUDIENCE]);
        validation.set_issuer(&[issuer]);
        assert_eq!(
            decode_header(token).unwrap().kid.as_deref(),
            doc["keys"][0]["kid"].as_str()
        );
        decode::<Value>(token, &key, &validation).unwrap().claims
    }

    #[test]
    fn platform_fixture_matches_explicit_issuer_and_serializes_null_owner() {
        let issuer = "https://configured.test/iam";
        let doc = test_rs256_jwks_json_with_iss(issuer);
        let raw: Value = serde_json::from_str(&doc).unwrap();
        assert_eq!(raw["keys"][0]["status"], "active");
        assert_eq!(raw["keys"][0]["trust_scope"], "platform");
        assert!(raw["keys"][0]["organization_id"].is_null());
        let user = Uuid::new_v4();
        let token = create_rs256_jwt_token_with_issuer(user, issuer);
        let claims = verify_fixture_token(&doc, &token, issuer);
        assert_eq!(claims["sub"], user.to_string());
        assert!(claims.get("org").is_none());
    }

    #[test]
    fn organization_fixture_has_distinct_owned_kid_and_matching_signed_claims() {
        let organization = Uuid::new_v4();
        let issuer = format!("https://configured.test/iam/orgs/{organization}");
        let fixture = CanonicalJwk::organization(&issuer, organization);
        assert_ne!(fixture.kid(), TEST_RS256_KID);
        assert_eq!(fixture.issuer(), issuer);
        let doc = test_organization_rs256_jwks_json(organization, &issuer);
        let raw: Value = serde_json::from_str(&doc).unwrap();
        assert_eq!(raw["keys"][0]["trust_scope"], "organization");
        assert_eq!(raw["keys"][0]["organization_id"], organization.to_string());
        let token = create_organization_rs256_jwt_token(Uuid::new_v4(), organization, &issuer);
        assert_eq!(
            verify_fixture_token(&doc, &token, &issuer)["org"],
            organization.to_string()
        );
    }

    #[test]
    fn lifecycle_statuses_use_publisher_wire_names_without_inferred_scope() {
        for (status, name) in [
            (TestSigningKeyStatus::Pending, "pending"),
            (TestSigningKeyStatus::Active, "active"),
            (TestSigningKeyStatus::Retiring, "retiring"),
            (TestSigningKeyStatus::Revoked, "revoked"),
        ] {
            let doc = CanonicalJwk::platform("https://configured.test/iam/orgs/not-an-owner")
                .with_status(status)
                .to_jwks_json();
            let raw: Value = serde_json::from_str(&doc).unwrap();
            assert_eq!(raw["keys"][0]["status"], name);
            assert_eq!(raw["keys"][0]["trust_scope"], "platform");
            assert!(raw["keys"][0]["organization_id"].is_null());
        }
    }
}
