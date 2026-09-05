//! Resolve server-owned filter IDs once, then execute only a reviewed snapshot.
//! Neither credentials nor definition bytes belong in ordinary diagnostics.
use crate::stream_scope;
use anyhow::{anyhow, bail, Result};
use reqwest::{Client, StatusCode, Url};
use sha2::{Digest, Sha256};
use std::env::VarError;
use tokio::time::{timeout, Duration};

const DEFINITION_MAX_BYTES: usize = 4096;

// Deliberately not Debug: definitions may contain private room/user selectors.
#[derive(Clone)]
pub(super) struct FilterDefinition {
    bytes: String,
    sha256: String,
}

impl FilterDefinition {
    pub fn bytes(&self) -> &str {
        &self.bytes
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

fn valid_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// Only ID-backed configurations require the explicit reviewed byte commitment.
/// An unexpected commitment for an inline/absent filter is a configuration error.
pub(super) fn configured_digest(
    filter: Option<&str>,
    value: Result<String, VarError>,
) -> Result<Option<String>, &'static str> {
    let digest = match value {
        Ok(raw) if valid_digest(&raw) => Some(raw),
        Err(VarError::NotPresent) => None,
        _ => return Err("matrix_filter_definition_digest_invalid"),
    };
    let needs_definition = filter.is_some_and(|raw| !raw.trim_start().starts_with('{'));
    if needs_definition != digest.is_some() {
        return Err("matrix_filter_definition_digest_required_or_unexpected");
    }
    Ok(digest)
}

pub(super) fn definition_url(base: &str, user: &str, id: &str) -> Result<Url, &'static str> {
    let scope = stream_scope::describe(base, user, Some(id))?;
    // URL segment APIs normalize dot segments. Do not let such an ID change path.
    if scope["filter"]["kind"] != "id" || matches!(id, "." | "..") {
        return Err("matrix_filter_definition_id_invalid");
    }
    let mut url = Url::parse(base).map_err(|_| "matrix_stream_endpoint_invalid")?;
    url.path_segments_mut()
        .map_err(|_| "matrix_stream_endpoint_invalid")?
        .pop_if_empty()
        .extend(["_matrix", "client", "v3", "user", user, "filter", id]);
    Ok(url)
}

pub(super) fn verify_definition(bytes: &[u8], expected: &str) -> Result<FilterDefinition, &'static str> {
    if bytes.is_empty() || bytes.len() > DEFINITION_MAX_BYTES || !valid_digest(expected) {
        return Err("matrix_filter_definition_invalid");
    }
    let digest = format!("sha256:{:x}", Sha256::digest(bytes));
    if digest != expected {
        return Err("matrix_filter_definition_digest_mismatch");
    }
    let raw = std::str::from_utf8(bytes).map_err(|_| "matrix_filter_definition_invalid")?;
    // Includes identity-projection, duplicate-field and unsupported-shape checks.
    stream_scope::validate_inline(raw)?;
    Ok(FilterDefinition { bytes: raw.to_owned(), sha256: digest })
}

pub(super) async fn resolve(
    http: &Client,
    base: &str,
    user: &str,
    token: &str,
    configured: Option<&str>,
    expected: Option<&str>,
) -> Result<Option<FilterDefinition>> {
    let Some(expected) = expected else {
        // This also rejects unresolved IDs in embedded/test callers.
        effective_filter(configured, None).map_err(|code| anyhow!(code))?;
        return Ok(None);
    };
    if !valid_digest(expected) {
        bail!("matrix_filter_definition_digest_invalid");
    }
    let id = configured.ok_or_else(|| anyhow!("matrix_filter_definition_id_invalid"))?;
    let url = definition_url(base, user, id).map_err(|code| anyhow!(code))?;
    timeout(Duration::from_secs(10), async {
        let response = http.get(url).bearer_auth(token).send().await
            .map_err(|_| anyhow!("matrix_filter_definition_transport_failure"))?;
        if response.status() != StatusCode::OK {
            bail!("matrix_filter_definition_http_rejected");
        }
        let bytes = crate::read_bounded_body(response, DEFINITION_MAX_BYTES).await
            .map_err(|_| anyhow!("matrix_filter_definition_body_failure"))?;
        verify_definition(&bytes, expected).map(Some).map_err(|code| anyhow!(code))
    }).await.map_err(|_| anyhow!("matrix_filter_definition_timeout"))?
}

/// All cursor-bearing sync and gap requests use this same immutable selection.
/// A remote ID is never sent to /sync after resolution, avoiding lookup/use drift.
pub(super) fn effective_filter<'a>(
    configured: Option<&'a str>,
    definition: Option<&'a FilterDefinition>,
) -> Result<Option<&'a str>, &'static str> {
    match (configured, definition) {
        (None, None) => Ok(None),
        (Some(raw), None) if raw.starts_with('{') => {
            stream_scope::validate_inline(raw)?;
            Ok(Some(raw))
        }
        (Some(raw), Some(definition)) if !raw.trim_start().starts_with('{') => Ok(Some(definition.bytes())),
        (Some(_), None) => Err("matrix_filter_definition_unresolved"),
        _ => Err("matrix_filter_definition_unexpected"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn digest(raw: &[u8]) -> String {
        format!("sha256:{:x}", Sha256::digest(raw))
    }

    #[test]
    fn only_ids_require_a_commitment_including_zero() {
        let pin = digest(b"{}");
        for id in ["0", "opaque-id"] {
            assert!(configured_digest(Some(id), Err(VarError::NotPresent)).is_err());
            assert_eq!(configured_digest(Some(id), Ok(pin.clone())), Ok(Some(pin.clone())));
        }
        for raw in [None, Some("{}"), Some(" {} ")] {
            assert!(configured_digest(raw, Ok(pin.clone())).is_err());
            assert_eq!(configured_digest(raw, Err(VarError::NotPresent)), Ok(None));
        }
    }

    #[test]
    fn malformed_and_nonunicode_commitments_fail() {
        for value in ["", "sha256:abc", &format!("sha256:{}", "A".repeat(64)), &format!(" {}", digest(b"{}"))] {
            assert!(configured_digest(Some("0"), Ok(value.to_owned())).is_err());
        }
        assert!(configured_digest(Some("0"), Err(VarError::NotUnicode("fixture".into()))).is_err());
    }

    #[test]
    fn verified_bytes_not_server_id_drive_both_requests() {
        let bytes = br#"{"room":{"timeline":{"senders":["@a:e"],"limit":9}}}"#;
        let pin = verify_definition(bytes, &digest(bytes)).unwrap();
        let effective = effective_filter(Some("0"), Some(&pin)).unwrap();
        assert_eq!(effective, Some(std::str::from_utf8(bytes).unwrap()));
        let gap = stream_scope::backfill_filter(effective, "!r:e").unwrap().unwrap();
        let actual: Value = serde_json::from_str(&gap).unwrap();
        assert_eq!(actual["senders"][0], "@a:e");
        assert_eq!(actual["limit"], 9);
        assert_eq!(pin.sha256(), digest(bytes));
    }

    #[test]
    fn wrong_digest_or_even_whitespace_drift_is_rejected() {
        assert!(verify_definition(b"{} ", &digest(b"{}")).is_err());
        assert!(verify_definition(b"{}", &digest(b"{\"room\":{}}")).is_err());
        assert!(verify_definition(b"{} \n", &digest(b"{} \n")).is_ok());
    }

    #[test]
    fn matching_hash_does_not_authorize_an_unsupported_definition() {
        for raw in [b"{\"event_fields\":[\"content\"]}".as_slice(), b"{\"room\":null}",
            b"{\"room\":{},\"room\":{}}", b"{\"errcode\":\"M_FORBIDDEN\"}", b" {}", b"[]", b"null", b"\xff"] {
            assert!(verify_definition(raw, &digest(raw)).is_err());
        }
        let large = vec![b' '; DEFINITION_MAX_BYTES + 1];
        assert!(verify_definition(&large, &digest(&large)).is_err());
    }

    #[test]
    fn unresolved_or_misapplied_definition_never_becomes_unfiltered_sync() {
        let pin = verify_definition(b"{}", &digest(b"{}")).unwrap();
        assert!(effective_filter(Some("0"), None).is_err());
        assert!(effective_filter(Some("{}"), Some(&pin)).is_err());
        assert!(effective_filter(None, Some(&pin)).is_err());
        assert_eq!(effective_filter(None, None), Ok(None));
        assert_eq!(effective_filter(Some("{}"), None), Ok(Some("{}")));
    }

    #[test]
    fn definition_endpoint_preserves_authority_and_encodes_both_ids() {
        let url = definition_url("https://matrix.example/proxy", "@bot/?:e", "id/?&part").unwrap();
        assert_eq!(url.host_str(), Some("matrix.example"));
        assert!(url.path().starts_with("/proxy/_matrix/client/v3/user/"));
        assert!(url.path().contains("/filter/id%2F%3F"));
        assert_eq!(url.path_segments().unwrap().count(), 8);
        assert!(url.query().is_none());
        assert!(url.fragment().is_none());
        for id in [".", "..", "{}", " id", ""] {
            assert!(definition_url("https://matrix.example", "@bot:e", id).is_err());
        }
        assert!(definition_url("https://u:secret@matrix.example", "@bot:e", "0").is_err());
    }
}

#[cfg(test)]
mod http_tests {
    use super::*;
    use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpListener};

    async fn serve_once(status: u16, body: &'static str, declared: usize) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/proxy", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            timeout(Duration::from_secs(3), async {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let mut byte = [0; 1];
                    stream.read_exact(&mut byte).await.unwrap();
                    request.extend(byte);
                    assert!(request.len() <= 8192);
                }
                let reply = format!("HTTP/1.1 {status} Fixture\r\nContent-Length: {declared}\r\nConnection: close\r\n\r\n{body}");
                stream.write_all(reply.as_bytes()).await.unwrap();
                stream.shutdown().await.unwrap();
                String::from_utf8(request).unwrap()
            }).await.unwrap()
        });
        (base, task)
    }

    fn client() -> Client {
        Client::builder().redirect(reqwest::redirect::Policy::none())
            .no_proxy().timeout(Duration::from_secs(2)).build().unwrap()
    }

    #[tokio::test]
    async fn real_http_resolver_binds_account_path_and_snapshot() {
        let body = r#"{"room":{"timeline":{"types":["m.room.message"]}}}"#;
        let digest = format!("sha256:{:x}", Sha256::digest(body.as_bytes()));
        let (base, task) = serve_once(200, body, body.len()).await;
        let def = resolve(&client(), &base, "@bot:example", "fixture-token", Some("0"), Some(&digest))
            .await.unwrap().unwrap();
        let request = task.await.unwrap();
        assert!(request.starts_with("GET /proxy/_matrix/client/v3/user/@bot:example/filter/0 "));
        assert!(request.to_ascii_lowercase().contains("authorization: bearer fixture-token"));
        assert_eq!(def.bytes(), body);
        assert_eq!(effective_filter(Some("0"), Some(&def)).unwrap(), Some(body));
    }

    #[tokio::test]
    async fn http_error_truncation_oversize_and_hash_drift_never_resolve() {
        let digest = format!("sha256:{:x}", Sha256::digest(b"{}"));
        for (status, body, length, expected) in [
            (403, "{}", 2, "matrix_filter_definition_http_rejected"),
            (200, "{}", 9999, "matrix_filter_definition_body_failure"),
            (200, "{", 2, "matrix_filter_definition_body_failure"),
            (200, "{} ", 3, "matrix_filter_definition_digest_mismatch"),
        ] {
            let (base, task) = serve_once(status, body, length).await;
            let outcome = resolve(&client(), &base, "@bot:example", "fixture-token", Some("0"), Some(&digest)).await;
            // Do not require Debug for private definition contents.
            let error = match outcome { Err(error) => error, Ok(_) => panic!("unverified definition accepted") };
            assert_eq!(error.to_string(), expected);
            task.await.unwrap();
        }
    }
}
