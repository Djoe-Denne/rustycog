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
    encode_rs256(
        user_id,
        TEST_PLATFORM_ISSUER,
        Some(TEST_RS256_KID),
        Some("aiforall-access+jwt"),
        None,
        None,
        false,
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
    format!(
        r#"{{"keys":[{{"kty":"RSA","use":"sig","alg":"RS256","kid":"{kid}","n":"{n}","e":"{e}","iss":"{iss}"}}]}}"#,
        kid = TEST_RS256_KID,
        n = TEST_RS256_N,
        e = TEST_RS256_E,
        iss = iss,
    )
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
    let iss = options.iss.unwrap_or(TEST_PLATFORM_ISSUER);
    let kid = match options.kid {
        None => Some(TEST_RS256_KID),
        Some(inner) => inner,
    };
    let typ = match options.typ {
        None => Some("aiforall-access+jwt"),
        Some(inner) => inner,
    };
    encode_rs256(
        user_id,
        iss,
        kid,
        typ,
        options.jku,
        options.x5u,
        options.include_jwk_header,
    )
}

#[allow(clippy::expect_used)]
fn encode_rs256(
    user_id: Uuid,
    iss: &str,
    kid: Option<&str>,
    typ: Option<&str>,
    jku: Option<&str>,
    x5u: Option<&str>,
    include_jwk_header: bool,
) -> String {
    let now = Utc::now();
    let claims = TestClaims {
        sub: user_id.to_string(),
        iss: iss.to_string(),
        aud: TEST_JWT_AUDIENCE.to_string(),
        exp: unix_ts_as_usize((now + Duration::hours(1)).timestamp()),
        iat: unix_ts_as_usize(now.timestamp()),
        jti: Uuid::new_v4().to_string(),
    };

    let mut header = Header::new(Algorithm::RS256);
    header.typ = typ.map(str::to_string);
    header.kid = kid.map(str::to_string);
    header.jku = jku.map(str::to_string);
    header.x5u = x5u.map(str::to_string);
    if include_jwk_header {
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
