//! Mesh mode: the gateway verified the JWT and recreated the principal headers.

use axum::http::HeaderMap;
use uuid::Uuid;
use x509_parser::extensions::GeneralName;

use super::jwt_handler::JwtPrincipal;
use super::tls::PeerClientCertificate;

const PRINCIPAL_ISS: &str = "x-principal-iss";
const PRINCIPAL_SUB: &str = "x-principal-sub";

/// Principal recreated by the gateway.
///
/// Accepted only when the mTLS peer certificate carries `gateway_san` as a DNS
/// SAN. The chain itself was already verified by the TLS listener.
pub(crate) fn gateway_principal(
    peer: Option<&PeerClientCertificate>,
    headers: &HeaderMap,
    gateway_san: &str,
) -> Result<JwtPrincipal, &'static str> {
    let peer = peer.ok_or("no client certificate")?;
    if !has_dns_san(&peer.der, gateway_san) {
        return Err("client certificate is not the gateway");
    }
    let iss = single_header(headers, PRINCIPAL_ISS)?;
    let sub = single_header(headers, PRINCIPAL_SUB)?;
    let sub = Uuid::parse_str(sub).map_err(|_| "x-principal-sub is not a UUID")?;
    Ok(JwtPrincipal {
        iss: iss.to_string(),
        sub,
        org: None,
    })
}

fn single_header<'a>(headers: &'a HeaderMap, name: &'static str) -> Result<&'a str, &'static str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next().ok_or("missing principal header")?;
    if values.next().is_some() {
        return Err("duplicate principal header");
    }
    let value = value
        .to_str()
        .map_err(|_| "principal header is not visible ASCII")?
        .trim();
    if value.is_empty() {
        return Err("empty principal header");
    }
    Ok(value)
}

fn has_dns_san(der: &[u8], expected: &str) -> bool {
    let Ok((_, cert)) = x509_parser::parse_x509_certificate(der) else {
        return false;
    };
    let Ok(Some(san)) = cert.subject_alternative_name() else {
        return false;
    };
    san.value
        .general_names
        .iter()
        .any(|name| matches!(name, GeneralName::DNSName(dns) if dns.eq_ignore_ascii_case(expected)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use rcgen::{CertificateParams, DnType, KeyPair};

    const GATEWAY: &str = "envoy-mesh";

    fn peer(sans: &[&str], common_name: &str) -> PeerClientCertificate {
        let mut params =
            CertificateParams::new(sans.iter().map(|s| (*s).to_string()).collect::<Vec<_>>())
                .unwrap();
        params
            .distinguished_name
            .push(DnType::CommonName, common_name);
        let cert = params.self_signed(&KeyPair::generate().unwrap()).unwrap();
        PeerClientCertificate {
            der: cert.der().as_ref().to_vec(),
        }
    }

    fn principal_headers(iss: &str, sub: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(PRINCIPAL_ISS, HeaderValue::from_str(iss).unwrap());
        headers.insert(PRINCIPAL_SUB, HeaderValue::from_str(sub).unwrap());
        headers
    }

    #[test]
    fn gateway_peer_with_headers_yields_principal() {
        let sub = Uuid::new_v4();
        let principal = gateway_principal(
            Some(&peer(&[GATEWAY, "localhost"], GATEWAY)),
            &principal_headers("https://idp.example/iam", &sub.to_string()),
            GATEWAY,
        )
        .expect("gateway principal");
        assert_eq!(principal.iss, "https://idp.example/iam");
        assert_eq!(principal.sub, sub);
        assert_eq!(principal.org, None);
    }

    #[test]
    fn other_mesh_peer_is_rejected() {
        let headers = principal_headers("https://idp.example/iam", &Uuid::new_v4().to_string());
        assert!(gateway_principal(Some(&peer(&["mesh-client"], "mesh-client")), &headers, GATEWAY).is_err());
    }

    #[test]
    fn gateway_common_name_without_san_is_rejected() {
        let headers = principal_headers("https://idp.example/iam", &Uuid::new_v4().to_string());
        assert!(gateway_principal(Some(&peer(&["other"], GATEWAY)), &headers, GATEWAY).is_err());
    }

    #[test]
    fn missing_peer_is_rejected() {
        let headers = principal_headers("https://idp.example/iam", &Uuid::new_v4().to_string());
        assert!(gateway_principal(None, &headers, GATEWAY).is_err());
    }

    #[test]
    fn missing_duplicate_or_invalid_sub_is_rejected() {
        let gateway = peer(&[GATEWAY], GATEWAY);

        let mut missing = HeaderMap::new();
        missing.insert(PRINCIPAL_ISS, HeaderValue::from_static("https://idp.example/iam"));
        assert!(gateway_principal(Some(&gateway), &missing, GATEWAY).is_err());

        let mut duplicate = principal_headers("https://idp.example/iam", &Uuid::new_v4().to_string());
        duplicate.append(
            PRINCIPAL_SUB,
            HeaderValue::from_str(&Uuid::new_v4().to_string()).unwrap(),
        );
        assert!(gateway_principal(Some(&gateway), &duplicate, GATEWAY).is_err());

        let not_uuid = principal_headers("https://idp.example/iam", "not-a-uuid");
        assert!(gateway_principal(Some(&gateway), &not_uuid, GATEWAY).is_err());

        let empty_iss = principal_headers(" ", &Uuid::new_v4().to_string());
        assert!(gateway_principal(Some(&gateway), &empty_iss, GATEWAY).is_err());
    }
}
