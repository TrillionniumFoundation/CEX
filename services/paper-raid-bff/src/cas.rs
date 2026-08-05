use std::time::Duration;

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use reqwest::{header, Client, Method, StatusCode};
use sha2::{Digest, Sha256};
use url::Url;

use crate::{config::CasConfig, error::AppError};

type HmacSha256 = Hmac<Sha256>;

const ALLOWED_MEDIA_TYPES: &[&str] = &[
    "application/json",
    "application/pdf",
    "application/zip",
    "application/gzip",
    "text/markdown",
    "text/plain; charset=utf-8",
    "text/csv; charset=utf-8",
    "text/x-bibtex; charset=utf-8",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredObject {
    pub digest: String,
    pub uri: String,
    pub media_type: String,
    pub size: usize,
    pub created: bool,
}

#[derive(Clone)]
pub struct CasClient {
    client: Client,
    config: CasConfig,
}

impl CasClient {
    pub fn new(config: CasConfig) -> Result<Self, String> {
        if config.endpoint.path() != "/" && !config.endpoint.path().is_empty() {
            return Err("CAS endpoint must not contain a path".to_string());
        }
        validate_digest(&config.ready_digest).map_err(|error| error.to_string())?;
        validate_media_type(&config.ready_media_type).map_err(|error| error.to_string())?;
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(20))
            .user_agent("paper-raid-bff/0.1")
            .build()
            .map_err(|error| format!("cannot build CAS client: {error}"))?;
        Ok(Self { client, config })
    }

    pub async fn put_if_absent(
        &self,
        expected_digest: &str,
        media_type: &str,
        bytes: &[u8],
    ) -> Result<StoredObject, AppError> {
        validate_digest(expected_digest)?;
        validate_media_type(media_type)?;
        if bytes.is_empty() || bytes.len() > self.config.max_object_bytes {
            return Err(AppError::Invalid(
                "artifact size is outside the allowed range".into(),
            ));
        }
        let actual = digest_label(bytes);
        if actual != expected_digest {
            return Err(AppError::Invalid(
                "artifact hash does not match bytes".into(),
            ));
        }
        if let Some(existing) = self.get_optional(expected_digest, media_type).await? {
            if existing != bytes {
                return Err(AppError::Conflict(
                    "CAS key already contains different bytes".into(),
                ));
            }
            return Ok(self.stored(expected_digest, media_type, bytes.len(), false));
        }

        let url = self.object_url(expected_digest)?;
        let payload_hash = hex_digest(bytes);
        let request = self
            .signed_request(Method::PUT, url, media_type, &payload_hash, Utc::now())?
            .header(header::IF_NONE_MATCH, "*")
            .body(bytes.to_vec());
        let response = request
            .send()
            .await
            .map_err(|_| AppError::Unavailable("cas"))?;
        if response.status().is_redirection() {
            return Err(AppError::Upstream);
        }
        if response.status() == StatusCode::PRECONDITION_FAILED
            || response.status() == StatusCode::CONFLICT
        {
            let existing = self
                .get_optional(expected_digest, media_type)
                .await?
                .ok_or(AppError::Conflict(
                    "CAS write raced without a readable object".into(),
                ))?;
            if existing != bytes {
                return Err(AppError::Conflict(
                    "CAS key already contains different bytes".into(),
                ));
            }
            return Ok(self.stored(expected_digest, media_type, bytes.len(), false));
        }
        if !response.status().is_success() {
            return Err(AppError::Unavailable("cas"));
        }
        Ok(self.stored(expected_digest, media_type, bytes.len(), true))
    }

    pub async fn get(&self, digest: &str, expected_media_type: &str) -> Result<Vec<u8>, AppError> {
        self.get_optional(digest, expected_media_type)
            .await?
            .ok_or(AppError::NotFound)
    }

    pub async fn ready(&self) -> bool {
        self.get_optional(&self.config.ready_digest, &self.config.ready_media_type)
            .await
            .is_ok_and(|object| object.is_some())
    }

    pub fn expected_uri(&self, digest: &str) -> Result<String, AppError> {
        validate_digest(digest)?;
        Ok(format!(
            "s3://{}/objects/sha256/{}",
            self.config.bucket,
            digest.trim_start_matches("sha256:")
        ))
    }

    async fn get_optional(
        &self,
        digest: &str,
        expected_media_type: &str,
    ) -> Result<Option<Vec<u8>>, AppError> {
        validate_digest(digest)?;
        validate_media_type(expected_media_type)?;
        let url = self.object_url(digest)?;
        let response = self
            .signed_request(Method::GET, url, "", &hex_digest(&[]), Utc::now())?
            .send()
            .await
            .map_err(|_| AppError::Unavailable("cas"))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if response.status().is_redirection() || !response.status().is_success() {
            return Err(AppError::Upstream);
        }
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .ok_or(AppError::Upstream)?;
        if content_type != expected_media_type {
            return Err(AppError::Upstream);
        }
        if response
            .content_length()
            .is_some_and(|length| length > self.config.max_object_bytes as u64)
        {
            return Err(AppError::Upstream);
        }
        let mut response = response;
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| AppError::Upstream)? {
            if body.len().saturating_add(chunk.len()) > self.config.max_object_bytes {
                return Err(AppError::Upstream);
            }
            body.extend_from_slice(&chunk);
        }
        if digest_label(&body) != digest {
            return Err(AppError::Conflict("stored artifact hash is invalid".into()));
        }
        Ok(Some(body))
    }

    fn object_url(&self, digest: &str) -> Result<Url, AppError> {
        let hex = digest
            .strip_prefix("sha256:")
            .ok_or_else(|| AppError::Invalid("artifact digest is invalid".into()))?;
        let mut url = self.config.endpoint.clone();
        url.set_path(&format!("/{}/objects/sha256/{hex}", self.config.bucket));
        url.set_query(None);
        url.set_fragment(None);
        Ok(url)
    }

    fn signed_request(
        &self,
        method: Method,
        url: Url,
        media_type: &str,
        payload_hash: &str,
        now: DateTime<Utc>,
    ) -> Result<reqwest::RequestBuilder, AppError> {
        let host = canonical_host(&url)?;
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date = now.format("%Y%m%d").to_string();
        let (canonical_headers, signed_headers) = if media_type.is_empty() {
            (
                format!(
                    "host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n"
                ),
                "host;x-amz-content-sha256;x-amz-date",
            )
        } else {
            (
                format!(
                    "content-type:{media_type}\nhost:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n"
                ),
                "content-type;host;x-amz-content-sha256;x-amz-date",
            )
        };
        let canonical_request = format!(
            "{}\n{}\n\n{}\n{}\n{}",
            method.as_str(),
            url.path(),
            canonical_headers,
            signed_headers,
            payload_hash
        );
        let scope = format!("{date}/{}/s3/aws4_request", self.config.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
            hex_digest(canonical_request.as_bytes())
        );
        let signing_key = sigv4_key(
            &self.config.secret_access_key,
            &date,
            &self.config.region,
            "s3",
        )?;
        let signature = hmac_hex(&signing_key, string_to_sign.as_bytes())?;
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
            self.config.access_key_id
        );
        let mut request = self
            .client
            .request(method, url)
            .header(header::HOST, host)
            .header("x-amz-content-sha256", payload_hash)
            .header("x-amz-date", amz_date)
            .header(header::AUTHORIZATION, authorization);
        if !media_type.is_empty() {
            request = request.header(header::CONTENT_TYPE, media_type);
        }
        Ok(request)
    }

    fn stored(&self, digest: &str, media_type: &str, size: usize, created: bool) -> StoredObject {
        StoredObject {
            digest: digest.to_string(),
            uri: format!(
                "s3://{}/objects/sha256/{}",
                self.config.bucket,
                digest.trim_start_matches("sha256:")
            ),
            media_type: media_type.to_string(),
            size,
            created,
        }
    }
}

fn validate_digest(value: &str) -> Result<(), AppError> {
    hepta_paper_raid_contracts::decode_digest(value)
        .map(|_| ())
        .map_err(AppError::Invalid)
}

fn validate_media_type(value: &str) -> Result<(), AppError> {
    if !ALLOWED_MEDIA_TYPES.contains(&value) {
        return Err(AppError::Invalid(
            "artifact media type is not allowed".into(),
        ));
    }
    Ok(())
}

fn canonical_host(url: &Url) -> Result<String, AppError> {
    let host = url.host_str().ok_or(AppError::Internal)?;
    Ok(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    })
}

fn digest_label(bytes: &[u8]) -> String {
    format!("sha256:{}", hex_digest(bytes))
}

fn hex_digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn hmac(key: &[u8], value: &[u8]) -> Result<Vec<u8>, AppError> {
    let mut mac = HmacSha256::new_from_slice(key).map_err(|_| AppError::Internal)?;
    mac.update(value);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn hmac_hex(key: &[u8], value: &[u8]) -> Result<String, AppError> {
    hmac(key, value).map(hex::encode)
}

fn sigv4_key(secret: &str, date: &str, region: &str, service: &str) -> Result<Vec<u8>, AppError> {
    let date_key = hmac(format!("AWS4{secret}").as_bytes(), date.as_bytes())?;
    let region_key = hmac(&date_key, region.as_bytes())?;
    let service_key = hmac(&region_key, service.as_bytes())?;
    hmac(&service_key, b"aws4_request")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Bytes,
        extract::{Path, State},
        http::{HeaderMap, StatusCode as AxumStatus},
        response::{IntoResponse, Response},
        routing::get,
        Router,
    };
    use std::{collections::HashMap, sync::Arc};
    use tokio::sync::Mutex;

    #[test]
    fn object_key_is_content_addressed_and_path_safe() {
        let bytes = b"paper bundle";
        let digest = digest_label(bytes);
        let client = CasClient::new(CasConfig {
            endpoint: Url::parse("http://127.0.0.1:9000").expect("endpoint"),
            bucket: "paper-raid-alpha".into(),
            region: "us-east-1".into(),
            access_key_id: "append-only".into(),
            secret_access_key: "test-only-secret".into(),
            max_object_bytes: 1024,
            ready_digest: digest.clone(),
            ready_media_type: "application/json".into(),
        })
        .expect("client");
        let url = client.object_url(&digest).expect("object URL");
        assert!(url.path().starts_with("/paper-raid-alpha/objects/sha256/"));
        assert!(!url.path().contains(".."));
    }

    #[test]
    fn rejects_noncanonical_digest_and_media() {
        assert!(validate_digest("sha256:ABC").is_err());
        assert!(validate_media_type("text/html").is_err());
    }

    #[test]
    fn aws_documented_signing_key_vector() {
        let key = sigv4_key(
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            "20150830",
            "us-east-1",
            "iam",
        )
        .expect("signing key");
        assert_eq!(
            hex::encode(key),
            "c4afb1cc5771d871763a393e44b703571b55cc28424d1a5e86da6ed3c154a4b9"
        );
    }

    #[derive(Default)]
    struct MockObjectStore {
        objects: HashMap<String, (Vec<u8>, String)>,
        put_calls: usize,
    }

    async fn mock_get(
        State(store): State<Arc<Mutex<MockObjectStore>>>,
        Path((_bucket, digest)): Path<(String, String)>,
        headers: HeaderMap,
    ) -> Response {
        if !headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("AWS4-HMAC-SHA256 Credential="))
        {
            return AxumStatus::FORBIDDEN.into_response();
        }
        let store = store.lock().await;
        match store.objects.get(&digest) {
            Some((bytes, media_type)) => (
                AxumStatus::OK,
                [(header::CONTENT_TYPE, media_type.as_str())],
                bytes.clone(),
            )
                .into_response(),
            None => AxumStatus::NOT_FOUND.into_response(),
        }
    }

    async fn mock_put(
        State(store): State<Arc<Mutex<MockObjectStore>>>,
        Path((_bucket, digest)): Path<(String, String)>,
        headers: HeaderMap,
        body: Bytes,
    ) -> Response {
        let authorization_ok = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("AWS4-HMAC-SHA256 Credential="));
        let if_none_match = headers
            .get(header::IF_NONE_MATCH)
            .and_then(|value| value.to_str().ok())
            == Some("*");
        let payload_hash = headers
            .get("x-amz-content-sha256")
            .and_then(|value| value.to_str().ok())
            == Some(hex_digest(&body).as_str());
        let media_type = headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        if !authorization_ok || !if_none_match || !payload_hash {
            return AxumStatus::FORBIDDEN.into_response();
        }
        let mut store = store.lock().await;
        store.put_calls += 1;
        if store.objects.contains_key(&digest) {
            return AxumStatus::PRECONDITION_FAILED.into_response();
        }
        store.objects.insert(digest, (body.to_vec(), media_type));
        AxumStatus::OK.into_response()
    }

    async fn spawn_store(store: Arc<Mutex<MockObjectStore>>) -> Url {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind object mock");
        let address = listener.local_addr().expect("object mock address");
        let router = Router::new()
            .route(
                "/:bucket/objects/sha256/:digest",
                get(mock_get).put(mock_put),
            )
            .with_state(store);
        tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .expect("serve object mock");
        });
        Url::parse(&format!("http://{address}")).expect("object mock URL")
    }

    fn mock_config(endpoint: Url, ready_digest: String) -> CasConfig {
        CasConfig {
            endpoint,
            bucket: "paper-raid-alpha".into(),
            region: "us-east-1".into(),
            access_key_id: "append-only-test".into(),
            secret_access_key: "test-only-secret".into(),
            max_object_bytes: 1024,
            ready_digest,
            ready_media_type: "application/json".into(),
        }
    }

    #[tokio::test]
    async fn append_only_same_bytes_ready_and_corruption_rejection() {
        let bytes = br#"{"canary":true}"#;
        let digest = digest_label(bytes);
        let store = Arc::new(Mutex::new(MockObjectStore::default()));
        let endpoint = spawn_store(store.clone()).await;
        let client = CasClient::new(mock_config(endpoint, digest.clone())).expect("CAS client");
        let created = client
            .put_if_absent(&digest, "application/json", bytes)
            .await
            .expect("first append");
        assert!(created.created);
        let replay = client
            .put_if_absent(&digest, "application/json", bytes)
            .await
            .expect("same bytes are idempotent");
        assert!(!replay.created);
        assert!(client.ready().await);
        assert_eq!(
            client.get(&digest, "application/json").await.unwrap(),
            bytes
        );
        assert_eq!(store.lock().await.put_calls, 1);

        let corrupt_bytes = b"corrupt".to_vec();
        store.lock().await.objects.insert(
            digest.trim_start_matches("sha256:").to_string(),
            (corrupt_bytes, "application/json".into()),
        );
        assert!(matches!(
            client.get(&digest, "application/json").await,
            Err(AppError::Conflict(_))
        ));
    }

    async fn redirect_get() -> axum::response::Redirect {
        axum::response::Redirect::temporary("/elsewhere")
    }

    #[tokio::test]
    async fn rejects_redirect_and_oversized_upload_before_network() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind redirect mock");
        let address = listener.local_addr().expect("redirect mock address");
        tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route("/:bucket/objects/sha256/:digest", get(redirect_get)),
            )
            .await
            .expect("redirect mock");
        });
        let bytes = b"canary";
        let digest = digest_label(bytes);
        let mut config = mock_config(
            Url::parse(&format!("http://{address}")).expect("redirect URL"),
            digest.clone(),
        );
        config.max_object_bytes = 4;
        let client = CasClient::new(config).expect("CAS client");
        assert!(client.get(&digest, "application/json").await.is_err());
        assert!(matches!(
            client
                .put_if_absent(&digest, "application/json", bytes)
                .await,
            Err(AppError::Invalid(_))
        ));
    }
}
