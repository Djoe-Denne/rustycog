use chrono::{Duration, Utc};
use jsonwebtoken::{
    encode,
    jwk::{AlgorithmParameters, CommonParameters, Jwk, RSAKeyParameters, RSAKeyType},
    Algorithm, EncodingKey, Header,
};
use serde::Serialize;
use uuid::Uuid;

pub const TEST_HS256_SECRET: &str = "rustycog-test-hs256-secret";
pub const TEST_JWT_ISSUER: &str = "iamrusty";
pub const TEST_JWT_AUDIENCE: &str = "aiforall";

/// Opaque RS256 key id for tests (not an org id or URL).
pub const TEST_RS256_KID: &str = "test-rs256-kid-01";

/// Platform issuer matching the custom JWK `iss` field in [`test_rs256_jwks_json`].
pub const TEST_PLATFORM_ISSUER: &str = "http://127.0.0.1/iam";

/// Fixed RSA private key (PKCS#8 PEM) for RS256 test tokens.
pub const TEST_RS256_PRIVATE_PEM: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCgJlj6aQXeKeST
5lNFKEG5Q6SptXk62gX5k2lAmQCdBdyYqhS/pEBcwemGC+V1zSQAA+p5Fe4HnMyT
f8mLjmjrxufsXmFIyOIjHhywXne4msS4AN16fQprITUxJPr/yLnvCQV+IIR3h6X0
1GqqAKmtR4LXcY7gmMmLOR3PY/2Xk5WIlue9gKUvQbBcXqMuuc4ZlBPpthXoqaTY
LLwZCj5Gz2Xgb+zFYG4w0Y2gcJmYlOplRmEVlmQP2xTkFlS6k4cHsX4asL8ercXY
nFELanLJCq/NzkKPVWF3X2tbBQoBt5MhuXExRYmOTtl97PL+ltqapoQ80UGG6zha
zBVfyhZbAgMBAAECggEAOfl313qiZajbttC3zz7CABulJcxsjOn1JMKA5SIeLzm6
gEd9yFxg8lM+QsjWsZzoDdtdC6VtLDNOeYzWfJ86izPPrGkEJbGW72iMsSoZg+n/
Ea86fgd6+Iomc9pzxJm4+XfWFbEW0yB3athknpMr2W8cRfq1Ysfcmfo8uOF1IWP4
aumf9YqH3cvCyviVzHFhoiP7UKZ59xrPvZWB/z/jRSUIgMP+N4u8DmA5QxyQna2W
MdX4B6KI4WBSN8XZrO+PogdljBQV3erOyxVghxwTnjlNoYcGy6cl2o/BFDE9GYdt
qnilmCFpHWxDnZc/KjXV/egjhKeRwi5LFd63hMdjiQKBgQDXV1dgQFUNBdOSKZtI
rWxyajiyhaT+p5TQQ/nyASwRyLVGF5jEatB7SDIQlCzM1BsKUmNSsvDQiUus0fAY
0Uw+RmQC+KvxC4KS+M2qo3gtNN9Uda8lDytbuDE3p/WlIfbLP9yAiUlM+z4r4Wh7
WulEPbCNr/tjvNTylT1lESIeyQKBgQC+Y0tQW7sEizfNh/otZRwnCalkHTPCc1c3
JWoqYpq2D4y9VtrOFct1Ig7YZDyNNAbDZLRS53NYnv5AUq0OkPt8QH7pXoZ28Pi+
zTleAG/jJ7QqiPq4fnGUbYF586jdWPJVjsMD3aFgr9FdQuHJWILe8AjLGrKWcJAS
vdP9+kDqAwKBgF45NV5ER/K+zehylCOk3oLhv5U9rQhQQ2ktlTwzDxlo/QiCYrHv
GvIWkPF4JHIrjPljO1qAOabFrHseETSKwBWvrystq+543tV4UGWNyZPeQqouJEjO
7mXfnol/0JhE2Dvu4YjMiWpJtNZ2dsUi7laRt6MHkbP+eB789jQ23vshAoGBAIZM
2NXIv3YHFsgfQXVAO8m14Q3EI7zpS/6Un/1iLSx8b5UobZSufyUTb1Fp8+TPbG3s
3d8VcaJ0FXoeWAFMeHo/rMbGbSf9+Bnv/qW2vTaJzWer1ODMISbI0GrMXLQ3iEqe
OCbD8pCXtaKKCWfUzgyhWjKblJrWsGroCWDBZYUtAoGARD8/bBC14SgPtA/oUIks
OLDpRNV1O5ul2myjYW4oDM0ULfSJQY18IPKW89LCkqWKXw4YZFKYYScUWbbIn+V+
RKympZHcMLgYl2GMPtF+/OgFVlKb8TBRInnZQTjcD0T1QZ/bZAVJlcsZdL0KkDFx
nC0J5qLklNHdwGJdrz9IFLU=
-----END PRIVATE KEY-----"#;

/// Fixed RSA public key (SPKI PEM) matching [`TEST_RS256_PRIVATE_PEM`].
pub const TEST_RS256_PUBLIC_PEM: &str = r#"-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAoCZY+mkF3inkk+ZTRShB
uUOkqbV5OtoF+ZNpQJkAnQXcmKoUv6RAXMHphgvldc0kAAPqeRXuB5zMk3/Ji45o
68bn7F5hSMjiIx4csF53uJrEuADden0KayE1MST6/8i57wkFfiCEd4el9NRqqgCp
rUeC13GO4JjJizkdz2P9l5OViJbnvYClL0GwXF6jLrnOGZQT6bYV6Kmk2Cy8GQo+
Rs9l4G/sxWBuMNGNoHCZmJTqZUZhFZZkD9sU5BZUupOHB7F+GrC/Hq3F2JxRC2py
yQqvzc5Cj1Vhd19rWwUKAbeTIblxMUWJjk7Zfezy/pbamqaEPNFBhus4WswVX8oW
WwIDAQAB
-----END PUBLIC KEY-----"#;

const TEST_RS256_N: &str = "oCZY-mkF3inkk-ZTRShBuUOkqbV5OtoF-ZNpQJkAnQXcmKoUv6RAXMHphgvldc0kAAPqeRXuB5zMk3_Ji45o68bn7F5hSMjiIx4csF53uJrEuADden0KayE1MST6_8i57wkFfiCEd4el9NRqqgCprUeC13GO4JjJizkdz2P9l5OViJbnvYClL0GwXF6jLrnOGZQT6bYV6Kmk2Cy8GQo-Rs9l4G_sxWBuMNGNoHCZmJTqZUZhFZZkD9sU5BZUupOHB7F-GrC_Hq3F2JxRC2pyyQqvzc5Cj1Vhd19rWwUKAbeTIblxMUWJjk7Zfezy_pbamqaEPNFBhus4WswVX8oWWw";
const TEST_RS256_E: &str = "AQAB";

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
/// Panics if the token cannot be encoded with the fixed test RSA key.
#[must_use]
#[allow(clippy::expect_used)]
pub fn create_rs256_jwt_token(user_id: Uuid) -> String {
    encode_rs256(user_id, Rs256TokenOptions::default(), None)
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

/// A typed canonical JWK using the fixed nonsecret RSA fixture material.
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
            n: TEST_RS256_N,
            e: TEST_RS256_E,
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
        Rs256TokenOptions {
            iss: Some(issuer),
            kid: Some(Some(&kid)),
            ..Default::default()
        },
        Some(organization_id),
    )
}

/// JWKS JSON document matching [`TEST_RS256_PRIVATE_PEM`] / [`TEST_RS256_KID`].
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
    encode_rs256(user_id, options, None)
}

#[allow(clippy::expect_used)]
fn encode_rs256(user_id: Uuid, options: Rs256TokenOptions<'_>, org: Option<Uuid>) -> String {
    let iss = options.iss.unwrap_or(TEST_PLATFORM_ISSUER);
    let kid = match options.kid {
        None => Some(TEST_RS256_KID),
        Some(inner) => inner,
    };
    let typ = match options.typ {
        None => Some("aiforall-access+jwt"),
        Some(inner) => inner,
    };
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
                n: TEST_RS256_N.to_string(),
                e: TEST_RS256_E.to_string(),
            }),
        });
    }

    let key = EncodingKey::from_rsa_pem(TEST_RS256_PRIVATE_PEM.as_bytes())
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
