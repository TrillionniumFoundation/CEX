use std::{env, net::SocketAddr, sync::Arc, time::Duration};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::SigningKey;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use url::Url;
use uuid::Uuid;

#[derive(Clone)]
pub struct AlphaIdentity {
    login_key_hash: [u8; 32],
    pub subject_id: String,
    pub display_name: String,
    pub nakama_user_id: Uuid,
    pub player_id: Uuid,
}

impl AlphaIdentity {
    pub fn login_matches(&self, candidate: &str) -> bool {
        let hash: [u8; 32] = Sha256::digest(candidate.as_bytes()).into();
        bool::from(self.login_key_hash.ct_eq(&hash))
    }

    #[cfg(test)]
    pub(crate) fn test_identity(subject_id: &str, player_id: Uuid, nakama_user_id: Uuid) -> Self {
        Self {
            login_key_hash: Sha256::digest(b"test-only-login-key-which-is-long").into(),
            subject_id: subject_id.into(),
            display_name: "Test Player".into(),
            nakama_user_id,
            player_id,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AlphaIdentityInput {
    login_key: String,
    subject_id: String,
    display_name: String,
    nakama_user_id: Uuid,
    player_id: Uuid,
}

#[derive(Clone)]
pub struct ConsumerAssertionConfig {
    pub issuer: String,
    pub audience: String,
    pub key_id: String,
    pub signing_key: Arc<SigningKey>,
    pub ttl: Duration,
}

#[derive(Clone)]
pub struct CasConfig {
    pub endpoint: Url,
    pub bucket: String,
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub max_object_bytes: usize,
    pub ready_digest: String,
    pub ready_media_type: String,
}

#[derive(Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub edge_scope: EdgeScope,
    pub public_origin: Url,
    pub database_url: String,
    pub session_key: [u8; 32],
    pub session_ttl: Duration,
    pub identities: Arc<[AlphaIdentity]>,
    pub hepta_base: Url,
    pub nakama_base: Url,
    pub nakama_http_key: String,
    pub assertions: ConsumerAssertionConfig,
    pub cas: CasConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeScope {
    LoopbackProcess,
    ContainerLoopbackPublish,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let bind = required("PAPER_RAID_BFF_BIND")?
            .parse::<SocketAddr>()
            .map_err(|_| "PAPER_RAID_BFF_BIND must be a socket address".to_string())?;
        let edge_scope = match required("PAPER_RAID_BFF_EDGE_SCOPE")?.as_str() {
            "loopback_process" if bind.ip().is_loopback() => EdgeScope::LoopbackProcess,
            "container_loopback_publish" if bind.ip().is_unspecified() => {
                EdgeScope::ContainerLoopbackPublish
            }
            "loopback_process" => {
                return Err("loopback_process requires a loopback bind address".to_string())
            }
            "container_loopback_publish" => {
                return Err(
                    "container_loopback_publish requires an unspecified container bind address"
                        .to_string(),
                )
            }
            _ => return Err("PAPER_RAID_BFF_EDGE_SCOPE is invalid".to_string()),
        };

        let public_origin = parse_origin(&required("PAPER_RAID_BFF_PUBLIC_ORIGIN")?)?;
        if public_origin.scheme() != "http" {
            return Err("alpha profile requires an explicit http public origin".to_string());
        }
        let host = public_origin
            .host_str()
            .ok_or_else(|| "public origin requires a host".to_string())?;
        if !matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1") {
            return Err("alpha public origin must be loopback/SSH-forwarded".to_string());
        }

        let session_key = decode_exact::<32>(
            "PAPER_RAID_BFF_SESSION_KEY_B64",
            &required("PAPER_RAID_BFF_SESSION_KEY_B64")?,
        )?;
        let signing_seed = decode_exact::<32>(
            "PAPER_RAID_BFF_CONSUMER_SIGNING_SEED_B64",
            &required("PAPER_RAID_BFF_CONSUMER_SIGNING_SEED_B64")?,
        )?;

        let identities =
            parse_alpha_identities(&required("PAPER_RAID_BFF_ALPHA_IDENTITIES_JSON")?)?;

        let hepta_base = parse_service_url("PAPER_RAID_BFF_HEPTA_BASE_URL")?;
        let nakama_base = parse_service_url("PAPER_RAID_BFF_NAKAMA_BASE_URL")?;
        let nakama_http_key = required("PAPER_RAID_BFF_NAKAMA_HTTP_KEY")?;
        if nakama_http_key.len() < 8 || nakama_http_key.len() > 256 {
            return Err("PAPER_RAID_BFF_NAKAMA_HTTP_KEY length is invalid".to_string());
        }
        let cas_endpoint = parse_service_url("PAPER_RAID_BFF_CAS_ENDPOINT")?;
        let cas_bucket = required("PAPER_RAID_BFF_CAS_BUCKET")?;
        validate_bucket(&cas_bucket)?;

        Ok(Self {
            bind,
            edge_scope,
            public_origin,
            database_url: required("PAPER_RAID_BFF_DATABASE_URL")?,
            session_key,
            session_ttl: Duration::from_secs(8 * 60 * 60),
            identities: identities.into(),
            hepta_base,
            nakama_base,
            nakama_http_key,
            assertions: ConsumerAssertionConfig {
                issuer: required("PAPER_RAID_BFF_CONSUMER_ISSUER")?,
                audience: required("PAPER_RAID_BFF_HEPTA_AUDIENCE")?,
                key_id: required("PAPER_RAID_BFF_CONSUMER_KEY_ID")?,
                signing_key: Arc::new(SigningKey::from_bytes(&signing_seed)),
                ttl: Duration::from_secs(30),
            },
            cas: CasConfig {
                endpoint: cas_endpoint,
                bucket: cas_bucket,
                region: required("PAPER_RAID_BFF_CAS_REGION")?,
                access_key_id: required("PAPER_RAID_BFF_CAS_ACCESS_KEY_ID")?,
                secret_access_key: required("PAPER_RAID_BFF_CAS_SECRET_ACCESS_KEY")?,
                max_object_bytes: 32 * 1024 * 1024,
                ready_digest: required("PAPER_RAID_BFF_CAS_READY_DIGEST")?,
                ready_media_type: required("PAPER_RAID_BFF_CAS_READY_MEDIA_TYPE")?,
            },
        })
    }

    pub fn find_identity(&self, login_key: &str) -> Option<AlphaIdentity> {
        find_identity(&self.identities, login_key)
    }

    pub fn identity_for_subject(&self, subject: &str) -> Option<AlphaIdentity> {
        self.identities
            .iter()
            .find(|identity| identity.subject_id == subject)
            .cloned()
    }
}

fn find_identity(identities: &[AlphaIdentity], login_key: &str) -> Option<AlphaIdentity> {
    let mut result = None;
    for identity in identities {
        if identity.login_matches(login_key) {
            result = Some(identity.clone());
        }
    }
    result
}

fn parse_alpha_identities(raw: &str) -> Result<Vec<AlphaIdentity>, String> {
    let identity_inputs: Vec<AlphaIdentityInput> = serde_json::from_str(raw)
        .map_err(|_| "PAPER_RAID_BFF_ALPHA_IDENTITIES_JSON is invalid".to_string())?;
    if identity_inputs.len() != 3 {
        return Err("alpha profile requires exactly three identities".to_string());
    }
    let mut identities = Vec::with_capacity(3);
    for input in identity_inputs {
        validate_opaque("subject_id", &input.subject_id, 128)?;
        validate_display_name(&input.display_name)?;
        if input.login_key.len() < 32 {
            return Err("each alpha login key must contain at least 32 bytes".to_string());
        }
        let login_key_hash = Sha256::digest(input.login_key.as_bytes()).into();
        identities.push(AlphaIdentity {
            login_key_hash,
            subject_id: input.subject_id,
            display_name: input.display_name,
            nakama_user_id: input.nakama_user_id,
            player_id: input.player_id,
        });
    }
    for left in 0..identities.len() {
        for right in left + 1..identities.len() {
            if identities[left].subject_id == identities[right].subject_id
                || identities[left].player_id == identities[right].player_id
                || identities[left].nakama_user_id == identities[right].nakama_user_id
                || identities[left].login_key_hash == identities[right].login_key_hash
            {
                return Err("alpha identities must be pairwise distinct".to_string());
            }
        }
    }
    Ok(identities)
}

fn required(name: &str) -> Result<String, String> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{name} is required"))
}

fn decode_exact<const N: usize>(name: &str, encoded: &str) -> Result<[u8; N], String> {
    let decoded = BASE64
        .decode(encoded)
        .map_err(|_| format!("{name} must be canonical padded base64"))?;
    if BASE64.encode(&decoded) != encoded {
        return Err(format!("{name} must be canonical padded base64"));
    }
    decoded
        .try_into()
        .map_err(|_| format!("{name} must decode to exactly {N} bytes"))
}

fn parse_origin(value: &str) -> Result<Url, String> {
    let url = Url::parse(value).map_err(|_| "public origin is invalid".to_string())?;
    if url.cannot_be_a_base()
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err("public origin must contain only scheme, host and port".to_string());
    }
    Ok(url)
}

fn parse_service_url(name: &str) -> Result<Url, String> {
    let value = required(name)?;
    let mut url = Url::parse(&value).map_err(|_| format!("{name} is invalid"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.cannot_be_a_base()
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(format!("{name} must be a plain http(s) base URL"));
    }
    if url.path() != "/" && !url.path().is_empty() {
        return Err(format!("{name} must not contain a path"));
    }
    url.set_path("");
    Ok(url)
}

fn validate_opaque(field: &str, value: &str, max: usize) -> Result<(), String> {
    if value.is_empty()
        || value.len() > max
        || value.bytes().any(|byte| {
            !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
        })
    {
        return Err(format!("{field} must be an opaque ASCII identifier"));
    }
    Ok(())
}

fn validate_display_name(value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.chars().count() > 80 || value.as_bytes().contains(&0) {
        return Err("display_name must contain 1..80 characters".to_string());
    }
    Ok(())
}

fn validate_bucket(value: &str) -> Result<(), String> {
    if value.len() < 3
        || value.len() > 63
        || value.starts_with('-')
        || value.ends_with('-')
        || value
            .bytes()
            .any(|byte| !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'))
    {
        return Err("CAS bucket must be a DNS-compatible lowercase name".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_login_comparison_uses_hash() {
        let identity = AlphaIdentity {
            login_key_hash: Sha256::digest(b"a-very-long-alpha-login-key-value").into(),
            subject_id: "sub-1".into(),
            display_name: "One".into(),
            nakama_user_id: Uuid::nil(),
            player_id: Uuid::nil(),
        };
        assert!(identity.login_matches("a-very-long-alpha-login-key-value"));
        assert!(!identity.login_matches("wrong"));
    }

    #[test]
    fn service_urls_reject_base_paths() {
        let previous = std::env::var("PAPER_RAID_BFF_TEST_SERVICE_URL").ok();
        std::env::set_var(
            "PAPER_RAID_BFF_TEST_SERVICE_URL",
            "http://127.0.0.1:8080/untrusted-prefix",
        );
        assert!(parse_service_url("PAPER_RAID_BFF_TEST_SERVICE_URL").is_err());
        match previous {
            Some(value) => std::env::set_var("PAPER_RAID_BFF_TEST_SERVICE_URL", value),
            None => std::env::remove_var("PAPER_RAID_BFF_TEST_SERVICE_URL"),
        }
    }

    #[test]
    fn exactly_three_alpha_keys_map_only_to_their_fixed_identities() {
        let keys = ["a".repeat(32), "b".repeat(32), "c".repeat(32)];
        let players = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let users = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let raw = serde_json::json!([
            {"login_key":keys[0], "subject_id":"subject-1", "display_name":"One", "nakama_user_id":users[0], "player_id":players[0]},
            {"login_key":keys[1], "subject_id":"subject-2", "display_name":"Two", "nakama_user_id":users[1], "player_id":players[1]},
            {"login_key":keys[2], "subject_id":"subject-3", "display_name":"Three", "nakama_user_id":users[2], "player_id":players[2]}
        ]);
        let identities = parse_alpha_identities(&raw.to_string()).expect("three identities");
        for index in 0..3 {
            let identity = find_identity(&identities, &keys[index]).expect("mapped identity");
            assert_eq!(identity.subject_id, format!("subject-{}", index + 1));
            assert_eq!(identity.player_id, players[index]);
            assert_eq!(identity.nakama_user_id, users[index]);
        }
        assert!(find_identity(&identities, &"z".repeat(32)).is_none());

        let too_few = serde_json::json!([
            {"login_key":keys[0], "subject_id":"subject-1", "display_name":"One", "nakama_user_id":users[0], "player_id":players[0]}
        ]);
        assert!(parse_alpha_identities(&too_few.to_string()).is_err());

        let duplicate = serde_json::json!([
            {"login_key":keys[0], "subject_id":"subject-1", "display_name":"One", "nakama_user_id":users[0], "player_id":players[0]},
            {"login_key":keys[1], "subject_id":"subject-2", "display_name":"Two", "nakama_user_id":users[1], "player_id":players[0]},
            {"login_key":keys[2], "subject_id":"subject-3", "display_name":"Three", "nakama_user_id":users[2], "player_id":players[2]}
        ]);
        assert!(parse_alpha_identities(&duplicate.to_string()).is_err());
    }
}
