use std::{env, net::SocketAddr, sync::Arc, time::Duration};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use url::Url;
use uuid::Uuid;

pub const MIN_ALPHA_AUTHOR_IDENTITIES: usize = 3;
pub const MIN_ALPHA_EVALUATOR_IDENTITIES: usize = 1;
pub const MIN_ALPHA_REVIEWER_IDENTITIES: usize = 2;
pub const MIN_ALPHA_REPRODUCER_IDENTITIES: usize = 1;
pub const MIN_ALPHA_INDEPENDENT_REVIEW_IDENTITIES: usize = 4;
pub const MAX_ALPHA_IDENTITIES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityMode {
    FixedAlpha,
    InviteAlpha,
}

#[derive(Clone, Copy, Debug)]
pub struct DurableQuotaConfig {
    pub login_window: Duration,
    pub login_global_limit: u64,
    pub login_bucket_limit: u64,
    pub mutation_window: Duration,
    pub mutation_account_limit: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct AgentBridgeQuotaConfig {
    pub window: Duration,
    pub pair_global_limit: u64,
    pub pair_bucket_limit: u64,
    pub request_global_limit: u64,
    pub request_bucket_limit: u64,
    pub request_binding_limit: u64,
}

#[derive(Clone, Debug)]
pub struct InviteAlphaConfig {
    pub quota: DurableQuotaConfig,
    pub retention_policy_id: String,
    pub activation: InviteActivationPins,
}

#[derive(Clone, Debug)]
pub struct InviteActivationPins {
    pub activation_id: Uuid,
    pub release_id: String,
    pub local_approval_sha256: String,
    pub approval_sequence: u64,
    pub nonce_sha256: String,
    pub profile_sha256: String,
    pub base_compose_sha256: String,
    pub runtime_acl_sha256: String,
    pub runtime_acl_state_sha256: String,
    pub retention_policy_sha256: String,
    pub image_lock_sha256: String,
    pub release_provenance_sha256: String,
    pub runtime_acl_evidence_sha256: String,
    pub hepta_revision: String,
    pub hepta_source_tree: String,
    pub hepta_fileset_sha256: String,
    pub database_name: String,
    pub database_oid: u32,
    pub cluster_system_identifier: String,
    pub deployment_identity: String,
    pub postgres_image: String,
    pub object_store_image: String,
    pub object_store_client_image: String,
    pub ops_image: String,
    pub nakama_image: String,
    pub hepta_image: String,
    pub bff_image: String,
    pub accessctl_image: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlphaIdentityScope {
    Author,
    Evaluator,
    Reviewer,
    Reproducer,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlphaAuthorRole {
    Captain,
    Evidence,
    Experiment,
}

#[derive(Clone)]
pub struct AlphaIdentity {
    login_key_hash: [u8; 32],
    pub subject_id: String,
    pub display_name: String,
    pub nakama_user_id: Uuid,
    pub player_id: Uuid,
    pub scopes: Arc<[AlphaIdentityScope]>,
    pub author_roles: Arc<[AlphaAuthorRole]>,
}

impl AlphaIdentity {
    pub fn login_matches(&self, candidate: &str) -> bool {
        let hash: [u8; 32] = Sha256::digest(candidate.as_bytes()).into();
        bool::from(self.login_key_hash.ct_eq(&hash))
    }

    pub fn has_scope(&self, scope: AlphaIdentityScope) -> bool {
        self.scopes.contains(&scope)
    }

    pub fn supports_author_role(&self, role: AlphaAuthorRole) -> bool {
        self.author_roles.contains(&role)
    }

    pub(crate) fn from_access_directory(
        subject_id: String,
        display_name: String,
        nakama_user_id: Uuid,
        player_id: Uuid,
        scopes: Vec<AlphaIdentityScope>,
        author_roles: Vec<AlphaAuthorRole>,
    ) -> Result<Self, String> {
        validate_opaque("subject_id", &subject_id, 128)?;
        validate_display_name(&display_name)?;
        validate_distinct_scopes(&scopes)?;
        validate_author_roles(scopes.contains(&AlphaIdentityScope::Author), &author_roles)?;
        Ok(Self {
            // Invite-mode credentials are verified by the PostgreSQL access
            // directory. This non-secret sentinel is never placed in the
            // fixed identity array and therefore cannot authenticate a key.
            login_key_hash: [0_u8; 32],
            subject_id,
            display_name,
            nakama_user_id,
            player_id,
            scopes: scopes.into(),
            author_roles: author_roles.into(),
        })
    }

    #[cfg(test)]
    pub(crate) fn test_identity(subject_id: &str, player_id: Uuid, nakama_user_id: Uuid) -> Self {
        Self {
            login_key_hash: Sha256::digest(b"test-only-login-key-which-is-long").into(),
            subject_id: subject_id.into(),
            display_name: "Test Player".into(),
            nakama_user_id,
            player_id,
            scopes: default_author_scopes().into(),
            author_roles: default_author_roles().into(),
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
    #[serde(default = "default_author_scopes")]
    scopes: Vec<AlphaIdentityScope>,
    #[serde(default, deserialize_with = "deserialize_author_roles")]
    author_roles: Option<Vec<AlphaAuthorRole>>,
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
    pub identity_mode: IdentityMode,
    pub invite_alpha: Option<InviteAlphaConfig>,
    pub agent_bridge_quota: AgentBridgeQuotaConfig,
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
                return Err("loopback_process requires a loopback bind address".to_string());
            }
            "container_loopback_publish" => {
                return Err(
                    "container_loopback_publish requires an unspecified container bind address"
                        .to_string(),
                );
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

        let identity_mode = parse_identity_mode(
            env::var("PAPER_RAID_BFF_IDENTITY_MODE")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .as_deref(),
        )?;
        let (identities, invite_alpha) = match identity_mode {
            IdentityMode::FixedAlpha => (
                parse_alpha_identities(&required("PAPER_RAID_BFF_ALPHA_IDENTITIES_JSON")?)?,
                None,
            ),
            IdentityMode::InviteAlpha => {
                if env::var("PAPER_RAID_BFF_ALPHA_IDENTITIES_JSON")
                    .ok()
                    .is_some_and(|value| !value.trim().is_empty())
                {
                    return Err(
                        "invite_alpha forbids PAPER_RAID_BFF_ALPHA_IDENTITIES_JSON to avoid split identity authority"
                            .to_string(),
                    );
                }
                let retention_policy_id = required("PAPER_RAID_BFF_ACCESS_RETENTION_POLICY_ID")?;
                validate_opaque("access retention policy id", &retention_policy_id, 128)?;
                let activation = InviteActivationPins::from_env()?;
                (
                    Vec::new(),
                    Some(InviteAlphaConfig {
                        quota: DurableQuotaConfig {
                            login_window: configured_duration(
                                "PAPER_RAID_BFF_LOGIN_QUOTA_WINDOW_SECONDS",
                            )?,
                            login_global_limit: configured_limit(
                                "PAPER_RAID_BFF_LOGIN_QUOTA_GLOBAL_LIMIT",
                            )?,
                            login_bucket_limit: configured_limit(
                                "PAPER_RAID_BFF_LOGIN_QUOTA_BUCKET_LIMIT",
                            )?,
                            mutation_window: configured_duration(
                                "PAPER_RAID_BFF_MUTATION_QUOTA_WINDOW_SECONDS",
                            )?,
                            mutation_account_limit: configured_limit(
                                "PAPER_RAID_BFF_MUTATION_QUOTA_ACCOUNT_LIMIT",
                            )?,
                        },
                        retention_policy_id,
                        activation,
                    }),
                )
            }
        };

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
            identity_mode,
            invite_alpha,
            agent_bridge_quota: AgentBridgeQuotaConfig {
                window: configured_optional_duration(
                    "PAPER_RAID_BFF_AGENT_BRIDGE_QUOTA_WINDOW_SECONDS",
                    60,
                )?,
                pair_global_limit: configured_optional_limit(
                    "PAPER_RAID_BFF_AGENT_PAIR_QUOTA_GLOBAL_LIMIT",
                    120,
                )?,
                pair_bucket_limit: configured_optional_limit(
                    "PAPER_RAID_BFF_AGENT_PAIR_QUOTA_BUCKET_LIMIT",
                    8,
                )?,
                request_global_limit: configured_optional_limit(
                    "PAPER_RAID_BFF_AGENT_REQUEST_QUOTA_GLOBAL_LIMIT",
                    2_000,
                )?,
                request_bucket_limit: configured_optional_limit(
                    "PAPER_RAID_BFF_AGENT_REQUEST_QUOTA_BUCKET_LIMIT",
                    120,
                )?,
                request_binding_limit: configured_optional_limit(
                    "PAPER_RAID_BFF_AGENT_REQUEST_QUOTA_BINDING_LIMIT",
                    240,
                )?,
            },
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

    pub fn alpha_identity_topology_is_valid(&self) -> bool {
        match self.identity_mode {
            IdentityMode::FixedAlpha => {
                self.invite_alpha.is_none() && alpha_identity_topology_is_valid(&self.identities)
            }
            IdentityMode::InviteAlpha => self.identities.is_empty() && self.invite_alpha.is_some(),
        }
    }
}

impl InviteActivationPins {
    fn from_env() -> Result<Self, String> {
        let activation_id = required("PAPER_RAID_BFF_INVITE_ACTIVATION_ID")?
            .parse::<Uuid>()
            .map_err(|_| "PAPER_RAID_BFF_INVITE_ACTIVATION_ID must be a UUID".to_string())?;
        let digest = |name: &str| -> Result<String, String> {
            let value = required(name)?;
            validate_sha256(name, &value)?;
            Ok(value)
        };
        let image = |name: &str| -> Result<String, String> {
            let value = required(name)?;
            validate_digest_image(name, &value)?;
            Ok(value)
        };
        let git_oid = |name: &str| {
            let value = required(name)?;
            if value.len() != 40
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(format!(
                    "{name} must be a canonical lowercase 40-hex Git OID"
                ));
            }
            Ok(value)
        };
        let approval_sequence = required("PAPER_RAID_BFF_INVITE_APPROVAL_SEQUENCE")?
            .parse::<u64>()
            .map_err(|_| {
                "PAPER_RAID_BFF_INVITE_APPROVAL_SEQUENCE must be an integer".to_string()
            })?;
        if !(1..=9_007_199_254_740_991).contains(&approval_sequence) {
            return Err(
                "PAPER_RAID_BFF_INVITE_APPROVAL_SEQUENCE must be a positive JSON-safe integer"
                    .to_string(),
            );
        }
        let database_name = required("PAPER_RAID_BFF_INVITE_DATABASE_NAME")?;
        validate_opaque("invite database name", &database_name, 63)?;
        if !database_name
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
            || !database_name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(
                "PAPER_RAID_BFF_INVITE_DATABASE_NAME must be a canonical database identifier"
                    .to_string(),
            );
        }
        let database_oid = required("PAPER_RAID_BFF_INVITE_DATABASE_OID")?
            .parse::<u32>()
            .map_err(|_| "PAPER_RAID_BFF_INVITE_DATABASE_OID must be a positive OID".to_string())?;
        if database_oid == 0 {
            return Err("PAPER_RAID_BFF_INVITE_DATABASE_OID must be a positive OID".to_string());
        }
        let cluster_system_identifier =
            required("PAPER_RAID_BFF_INVITE_CLUSTER_SYSTEM_IDENTIFIER")?;
        if cluster_system_identifier
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .is_none()
            || cluster_system_identifier.starts_with('0')
        {
            return Err(
                "PAPER_RAID_BFF_INVITE_CLUSTER_SYSTEM_IDENTIFIER must be a canonical positive decimal u64"
                    .to_string(),
            );
        }
        let deployment_identity = required("PAPER_RAID_BFF_INVITE_DEPLOYMENT_IDENTITY")?;
        validate_opaque("invite deployment identity", &deployment_identity, 128)?;
        if !deployment_identity
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        {
            return Err(
                "PAPER_RAID_BFF_INVITE_DEPLOYMENT_IDENTITY must begin with an ASCII alphanumeric character"
                    .to_string(),
            );
        }
        let release_id = required("PAPER_RAID_BFF_INVITE_RELEASE_ID")?;
        if release_id.is_empty()
            || release_id.len() > 128
            || !release_id
                .as_bytes()
                .first()
                .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
            || !release_id.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'_' | b'-')
            })
        {
            return Err(
                "PAPER_RAID_BFF_INVITE_RELEASE_ID must be a canonical lowercase release id"
                    .to_string(),
            );
        }
        Ok(Self {
            activation_id,
            release_id,
            local_approval_sha256: digest("PAPER_RAID_BFF_INVITE_ACTIVATION_RECEIPT_SHA256")?,
            approval_sequence,
            nonce_sha256: digest("PAPER_RAID_BFF_INVITE_APPROVAL_NONCE_SHA256")?,
            profile_sha256: digest("PAPER_RAID_BFF_INVITE_PROFILE_SHA256")?,
            base_compose_sha256: digest("PAPER_RAID_BFF_INVITE_BASE_COMPOSE_SHA256")?,
            runtime_acl_sha256: digest("PAPER_RAID_BFF_INVITE_RUNTIME_ACL_SHA256")?,
            runtime_acl_state_sha256: digest("PAPER_RAID_BFF_INVITE_RUNTIME_ACL_STATE_SHA256")?,
            retention_policy_sha256: digest("PAPER_RAID_BFF_INVITE_RETENTION_POLICY_SHA256")?,
            image_lock_sha256: digest("PAPER_RAID_BFF_INVITE_IMAGE_LOCK_SHA256")?,
            release_provenance_sha256: digest("PAPER_RAID_BFF_INVITE_RELEASE_PROVENANCE_SHA256")?,
            runtime_acl_evidence_sha256: digest(
                "PAPER_RAID_BFF_INVITE_RUNTIME_ACL_EVIDENCE_SHA256",
            )?,
            hepta_revision: git_oid("PAPER_RAID_BFF_INVITE_HEPTA_REVISION")?,
            hepta_source_tree: git_oid("PAPER_RAID_BFF_INVITE_HEPTA_SOURCE_TREE")?,
            hepta_fileset_sha256: digest("PAPER_RAID_BFF_INVITE_HEPTA_FILESET_SHA256")?,
            database_name,
            database_oid,
            cluster_system_identifier,
            deployment_identity,
            postgres_image: image("PAPER_RAID_BFF_INVITE_POSTGRES_IMAGE")?,
            object_store_image: image("PAPER_RAID_BFF_INVITE_OBJECT_STORE_IMAGE")?,
            object_store_client_image: image("PAPER_RAID_BFF_INVITE_OBJECT_STORE_CLIENT_IMAGE")?,
            ops_image: image("PAPER_RAID_BFF_INVITE_OPS_IMAGE")?,
            nakama_image: image("PAPER_RAID_BFF_INVITE_NAKAMA_IMAGE")?,
            hepta_image: image("PAPER_RAID_BFF_INVITE_HEPTA_IMAGE")?,
            bff_image: image("PAPER_RAID_BFF_INVITE_BFF_IMAGE")?,
            accessctl_image: image("PAPER_RAID_BFF_INVITE_ACCESSCTL_IMAGE")?,
        })
    }
}

fn parse_identity_mode(value: Option<&str>) -> Result<IdentityMode, String> {
    match value {
        None | Some("fixed_alpha") => Ok(IdentityMode::FixedAlpha),
        Some("invite_alpha") => Ok(IdentityMode::InviteAlpha),
        Some(_) => Err("PAPER_RAID_BFF_IDENTITY_MODE is invalid".to_string()),
    }
}

fn configured_duration(name: &str) -> Result<Duration, String> {
    let seconds = required(name)?
        .parse::<u64>()
        .map_err(|_| format!("{name} must be an integer number of seconds"))?;
    if !(1..=86_400).contains(&seconds) {
        return Err(format!("{name} must be between 1 and 86400 seconds"));
    }
    Ok(Duration::from_secs(seconds))
}

fn configured_limit(name: &str) -> Result<u64, String> {
    let limit = required(name)?
        .parse::<u64>()
        .map_err(|_| format!("{name} must be an integer"))?;
    if !(1..=1_000_000).contains(&limit) {
        return Err(format!("{name} must be between 1 and 1000000"));
    }
    Ok(limit)
}

fn configured_optional_duration(name: &str, default_seconds: u64) -> Result<Duration, String> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => {
            let seconds = value
                .parse::<u64>()
                .map_err(|_| format!("{name} must be an integer number of seconds"))?;
            if !(1..=86_400).contains(&seconds) {
                return Err(format!("{name} must be between 1 and 86400 seconds"));
            }
            Ok(Duration::from_secs(seconds))
        }
        _ => Ok(Duration::from_secs(default_seconds)),
    }
}

fn configured_optional_limit(name: &str, default: u64) -> Result<u64, String> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => {
            let limit = value
                .parse::<u64>()
                .map_err(|_| format!("{name} must be an integer"))?;
            if !(1..=1_000_000).contains(&limit) {
                return Err(format!("{name} must be between 1 and 1000000"));
            }
            Ok(limit)
        }
        _ => Ok(default),
    }
}

fn alpha_identity_topology_is_valid(identities: &[AlphaIdentity]) -> bool {
    if !(MIN_ALPHA_AUTHOR_IDENTITIES..=MAX_ALPHA_IDENTITIES).contains(&identities.len()) {
        return false;
    }
    let authors = identities
        .iter()
        .filter(|identity| identity.has_scope(AlphaIdentityScope::Author))
        .collect::<Vec<_>>();
    // Review independence is scoped to the target Paper, not to a global
    // account class. A player may author Paper A and review Paper B; the Hepta
    // assignment authority still rejects every author of the target Paper.
    let review_capable = identities.iter().collect::<Vec<_>>();
    authors.len() >= MIN_ALPHA_AUTHOR_IDENTITIES
        && review_capable.len() >= MIN_ALPHA_INDEPENDENT_REVIEW_IDENTITIES
        && distinct_author_role_assignment(&authors)
        && review_capable
            .iter()
            .filter(|identity| identity.has_scope(AlphaIdentityScope::Evaluator))
            .count()
            >= MIN_ALPHA_EVALUATOR_IDENTITIES
        && review_capable
            .iter()
            .filter(|identity| identity.has_scope(AlphaIdentityScope::Reviewer))
            .count()
            >= MIN_ALPHA_REVIEWER_IDENTITIES
        && review_capable
            .iter()
            .filter(|identity| identity.has_scope(AlphaIdentityScope::Reproducer))
            .count()
            >= MIN_ALPHA_REPRODUCER_IDENTITIES
        && distinct_independent_review_assignment(&review_capable)
        && every_author_lineup_has_an_independent_review_panel(identities)
        && [
            AlphaAuthorRole::Captain,
            AlphaAuthorRole::Evidence,
            AlphaAuthorRole::Experiment,
        ]
        .into_iter()
        .all(|role| {
            authors
                .iter()
                .any(|identity| identity.supports_author_role(role))
        })
}

fn every_author_lineup_has_an_independent_review_panel(identities: &[AlphaIdentity]) -> bool {
    let author_indexes = identities
        .iter()
        .enumerate()
        .filter_map(|(index, identity)| {
            identity
                .has_scope(AlphaIdentityScope::Author)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    let mut found_author_lineup = false;
    for left in 0..author_indexes.len() {
        for middle in left + 1..author_indexes.len() {
            for right in middle + 1..author_indexes.len() {
                let lineup_indexes = [
                    author_indexes[left],
                    author_indexes[middle],
                    author_indexes[right],
                ];
                let lineup = lineup_indexes
                    .iter()
                    .map(|index| &identities[*index])
                    .collect::<Vec<_>>();
                if !distinct_author_role_assignment(&lineup) {
                    continue;
                }
                found_author_lineup = true;
                let independent = identities
                    .iter()
                    .enumerate()
                    .filter_map(|(index, identity)| {
                        (!lineup_indexes.contains(&index)).then_some(identity)
                    })
                    .collect::<Vec<_>>();
                if !distinct_independent_review_assignment(&independent) {
                    return false;
                }
            }
        }
    }
    found_author_lineup
}

fn distinct_author_role_assignment(identities: &[&AlphaIdentity]) -> bool {
    fn assign(
        identities: &[&AlphaIdentity],
        roles: &[AlphaAuthorRole],
        role: usize,
        used: &mut [bool],
    ) -> bool {
        if role == roles.len() {
            return true;
        }
        for (index, identity) in identities.iter().enumerate() {
            if !used[index] && identity.supports_author_role(roles[role]) {
                used[index] = true;
                if assign(identities, roles, role + 1, used) {
                    return true;
                }
                used[index] = false;
            }
        }
        false
    }

    assign(
        identities,
        &[
            AlphaAuthorRole::Captain,
            AlphaAuthorRole::Evidence,
            AlphaAuthorRole::Experiment,
        ],
        0,
        &mut vec![false; identities.len()],
    )
}

fn distinct_independent_review_assignment(identities: &[&AlphaIdentity]) -> bool {
    fn assign(
        identities: &[&AlphaIdentity],
        slots: &[AlphaIdentityScope],
        slot: usize,
        used: &mut [bool],
    ) -> bool {
        if slot == slots.len() {
            return true;
        }
        for (index, identity) in identities.iter().enumerate() {
            if !used[index] && identity.has_scope(slots[slot]) {
                used[index] = true;
                if assign(identities, slots, slot + 1, used) {
                    return true;
                }
                used[index] = false;
            }
        }
        false
    }

    // The review protocol requires four pairwise-distinct humans: one
    // evaluator, two reviewers, and a reproducer outside that panel. Counting
    // scope labels alone is insufficient when one identity has several scopes.
    let slots = [
        AlphaIdentityScope::Evaluator,
        AlphaIdentityScope::Reviewer,
        AlphaIdentityScope::Reviewer,
        AlphaIdentityScope::Reproducer,
    ];
    assign(identities, &slots, 0, &mut vec![false; identities.len()])
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
    if identity_inputs.len() < MIN_ALPHA_AUTHOR_IDENTITIES {
        return Err(format!(
            "alpha profile requires at least {MIN_ALPHA_AUTHOR_IDENTITIES} identities"
        ));
    }
    if identity_inputs.len() > MAX_ALPHA_IDENTITIES {
        return Err(format!(
            "alpha profile permits at most {MAX_ALPHA_IDENTITIES} identities"
        ));
    }
    let mut identities = Vec::with_capacity(identity_inputs.len());
    for input in identity_inputs {
        validate_opaque("subject_id", &input.subject_id, 128)?;
        validate_display_name(&input.display_name)?;
        if input.login_key.len() < 32 {
            return Err("each alpha login key must contain at least 32 bytes".to_string());
        }
        let scopes = input.scopes;
        validate_distinct_scopes(&scopes)?;
        let is_author = scopes.contains(&AlphaIdentityScope::Author);
        let author_roles = match input.author_roles {
            Some(roles) => roles,
            None if is_author => default_author_roles(),
            None => Vec::new(),
        };
        validate_author_roles(is_author, &author_roles)?;
        let login_key_hash = Sha256::digest(input.login_key.as_bytes()).into();
        identities.push(AlphaIdentity {
            login_key_hash,
            subject_id: input.subject_id,
            display_name: input.display_name,
            nakama_user_id: input.nakama_user_id,
            player_id: input.player_id,
            scopes: scopes.into(),
            author_roles: author_roles.into(),
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
    if !alpha_identity_topology_is_valid(&identities) {
        return Err(format!(
            "alpha profile requires every valid three-Author captain/evidence/experiment lineup to leave {MIN_ALPHA_INDEPENDENT_REVIEW_IDENTITIES} pairwise-distinct evaluator/reviewer/reviewer/reproducer identities outside that target Paper"
        ));
    }
    Ok(identities)
}

fn default_author_scopes() -> Vec<AlphaIdentityScope> {
    vec![AlphaIdentityScope::Author]
}

fn default_author_roles() -> Vec<AlphaAuthorRole> {
    vec![
        AlphaAuthorRole::Captain,
        AlphaAuthorRole::Evidence,
        AlphaAuthorRole::Experiment,
    ]
}

fn deserialize_author_roles<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<AlphaAuthorRole>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Vec::<AlphaAuthorRole>::deserialize(deserializer).map(Some)
}

fn validate_distinct_scopes(scopes: &[AlphaIdentityScope]) -> Result<(), String> {
    if scopes.is_empty() {
        return Err("each alpha identity requires at least one scope".to_string());
    }
    for left in 0..scopes.len() {
        if scopes[left + 1..].contains(&scopes[left]) {
            return Err("alpha identity scopes must be distinct".to_string());
        }
    }
    Ok(())
}

fn validate_author_roles(is_author: bool, roles: &[AlphaAuthorRole]) -> Result<(), String> {
    if !is_author && !roles.is_empty() {
        return Err("author_roles require the author scope".to_string());
    }
    if is_author && roles.is_empty() {
        return Err("author-scoped identities require at least one author role".to_string());
    }
    for left in 0..roles.len() {
        if roles[left + 1..].contains(&roles[left]) {
            return Err("alpha identity author_roles must be distinct".to_string());
        }
    }
    Ok(())
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

fn validate_sha256(field: &str, value: &str) -> Result<(), String> {
    if value.len() != 71
        || !value.starts_with("sha256:")
        || !value[7..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("{field} must be a canonical sha256 digest"));
    }
    Ok(())
}

fn validate_digest_image(field: &str, value: &str) -> Result<(), String> {
    let Some((name, digest)) = value.rsplit_once('@') else {
        return Err(format!("{field} must be a digest-pinned image"));
    };
    if name.is_empty()
        || name.len() > 255
        || !name
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        || name.bytes().any(|byte| {
            !(byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'_' | b':' | b'/' | b'-'))
        })
    {
        return Err(format!("{field} image name is invalid"));
    }
    validate_sha256(field, digest)
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
            scopes: default_author_scopes().into(),
            author_roles: default_author_roles().into(),
        };
        assert!(identity.login_matches("a-very-long-alpha-login-key-value"));
        assert!(!identity.login_matches("wrong"));
    }

    #[test]
    fn identity_mode_defaults_fixed_and_is_deny_unknown() {
        assert_eq!(parse_identity_mode(None), Ok(IdentityMode::FixedAlpha));
        assert_eq!(
            parse_identity_mode(Some("fixed_alpha")),
            Ok(IdentityMode::FixedAlpha)
        );
        assert_eq!(
            parse_identity_mode(Some("invite_alpha")),
            Ok(IdentityMode::InviteAlpha)
        );
        assert!(parse_identity_mode(Some("oidc")).is_err());
        assert!(parse_identity_mode(Some("")).is_err());
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
    fn legacy_three_identity_topology_is_rejected_before_serving() {
        let keys = ["a".repeat(32), "b".repeat(32), "c".repeat(32)];
        let players = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let users = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let raw = serde_json::json!([
            {"login_key":keys[0], "subject_id":"subject-1", "display_name":"One", "nakama_user_id":users[0], "player_id":players[0]},
            {"login_key":keys[1], "subject_id":"subject-2", "display_name":"Two", "nakama_user_id":users[1], "player_id":players[1]},
            {"login_key":keys[2], "subject_id":"subject-3", "display_name":"Three", "nakama_user_id":users[2], "player_id":players[2]}
        ]);
        assert!(parse_alpha_identities(&raw.to_string()).is_err());

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

    #[test]
    fn seven_identity_topology_expresses_authors_and_independent_review_scopes() {
        let identities = serde_json::json!([
            identity_json(
                "author-1",
                "a",
                ["author"],
                Some(["captain", "evidence", "experiment"])
            ),
            identity_json("author-2", "b", ["author"], Some(["evidence"])),
            identity_json("author-3", "c", ["author"], Some(["experiment"])),
            identity_json("evaluator-1", "d", ["evaluator"], None::<[&str; 0]>),
            identity_json("reviewer-1", "e", ["reviewer"], None::<[&str; 0]>),
            identity_json("reviewer-2", "f", ["reviewer"], None::<[&str; 0]>),
            identity_json("reproducer-1", "g", ["reproducer"], None::<[&str; 0]>)
        ]);

        let parsed = parse_alpha_identities(&identities.to_string()).expect("seven identities");
        assert_eq!(parsed.len(), 7);
        assert_eq!(
            parsed
                .iter()
                .filter(|identity| identity.has_scope(AlphaIdentityScope::Author))
                .count(),
            3
        );
        assert_eq!(
            parsed
                .iter()
                .filter(|identity| identity.has_scope(AlphaIdentityScope::Reviewer))
                .count(),
            2
        );
        assert!(parsed[3].author_roles.is_empty());
        assert!(parsed[6].has_scope(AlphaIdentityScope::Reproducer));
        assert!(alpha_identity_topology_is_valid(&parsed));
    }

    #[test]
    fn deploy_alpha_env_example_is_accepted_by_the_production_identity_parser() {
        let raw = include_str!("../deploy/alpha.env.example")
            .lines()
            .find_map(|line| line.strip_prefix("PAPER_RAID_BFF_ALPHA_IDENTITIES_JSON="))
            .expect("alpha deployment example identity variable");
        let parsed = parse_alpha_identities(raw)
            .expect("alpha deployment example must remain bootable under the production parser");
        assert_eq!(parsed.len(), 7);
        assert!(alpha_identity_topology_is_valid(&parsed));
    }

    #[test]
    fn alpha_identity_topology_fails_closed_on_invalid_scope_and_role_sets() {
        let too_few_authors = serde_json::json!([
            identity_json("author-1", "a", ["author"], Some(["captain"])),
            identity_json("author-2", "b", ["author"], Some(["evidence"])),
            identity_json("reviewer-1", "c", ["reviewer"], None::<[&str; 0]>)
        ]);
        assert!(parse_alpha_identities(&too_few_authors.to_string()).is_err());

        let duplicate_scope = serde_json::json!([
            identity_json("author-1", "a", ["author", "author"], Some(["captain"])),
            identity_json("author-2", "b", ["author"], Some(["evidence"])),
            identity_json("author-3", "c", ["author"], Some(["experiment"]))
        ]);
        assert!(parse_alpha_identities(&duplicate_scope.to_string()).is_err());

        let reviewer_with_author_role = serde_json::json!([
            identity_json("author-1", "a", ["author"], Some(["captain"])),
            identity_json("author-2", "b", ["author"], Some(["evidence"])),
            identity_json("author-3", "c", ["author"], Some(["experiment"])),
            identity_json("reviewer-1", "d", ["reviewer"], Some(["evidence"]))
        ]);
        assert!(parse_alpha_identities(&reviewer_with_author_role.to_string()).is_err());

        let empty_author_roles = serde_json::json!([
            identity_json("author-1", "a", ["author"], Some::<[&str; 0]>([])),
            identity_json("author-2", "b", ["author"], Some(["evidence"])),
            identity_json("author-3", "c", ["author"], Some(["experiment"]))
        ]);
        assert!(parse_alpha_identities(&empty_author_roles.to_string()).is_err());

        let missing_second_reviewer = serde_json::json!([
            identity_json("author-1", "a", ["author"], Some(["captain"])),
            identity_json("author-2", "b", ["author"], Some(["evidence"])),
            identity_json("author-3", "c", ["author"], Some(["experiment"])),
            identity_json("evaluator-1", "d", ["evaluator"], None::<[&str; 0]>),
            identity_json("reviewer-1", "e", ["reviewer"], None::<[&str; 0]>),
            identity_json("observer-1", "f", ["evaluator"], None::<[&str; 0]>),
            identity_json("reproducer-1", "g", ["reproducer"], None::<[&str; 0]>)
        ]);
        assert!(parse_alpha_identities(&missing_second_reviewer.to_string()).is_err());

        let overlapping_scopes_cannot_fake_four_distinct_review_actors = serde_json::json!([
            identity_json("author-1", "a", ["author"], Some(["captain"])),
            identity_json("author-2", "b", ["author"], Some(["evidence"])),
            identity_json("author-3", "c", ["author"], Some(["experiment"])),
            identity_json(
                "evaluation-reviewer",
                "d",
                ["evaluator", "reviewer"],
                None::<[&str; 0]>
            ),
            identity_json("reviewer-1", "e", ["reviewer"], None::<[&str; 0]>),
            identity_json("reproducer-1", "f", ["reproducer"], None::<[&str; 0]>),
            identity_json("reproducer-2", "g", ["reproducer"], None::<[&str; 0]>)
        ]);
        assert!(parse_alpha_identities(
            &overlapping_scopes_cannot_fake_four_distinct_review_actors.to_string()
        )
        .is_err());

        let four_people_cannot_review_their_own_three_author_paper = serde_json::json!([
            identity_json("author-1", "a", ["author", "evaluator"], Some(["captain"])),
            identity_json("author-2", "b", ["author", "reviewer"], Some(["evidence"])),
            identity_json(
                "author-3",
                "c",
                ["author", "reviewer"],
                Some(["experiment"])
            ),
            identity_json("reproducer-1", "d", ["reproducer"], None::<[&str; 0]>)
        ]);
        assert!(parse_alpha_identities(
            &four_people_cannot_review_their_own_three_author_paper.to_string()
        )
        .is_err());

        let six_person_dual_team_without_floating_reproducer = serde_json::json!([
            identity_json("captain-1", "a", ["author", "evaluator"], Some(["captain"])),
            identity_json("captain-2", "b", ["author", "evaluator"], Some(["captain"])),
            identity_json(
                "evidence-1",
                "c",
                ["author", "reviewer"],
                Some(["evidence"])
            ),
            identity_json(
                "evidence-2",
                "d",
                ["author", "reviewer"],
                Some(["evidence"])
            ),
            identity_json(
                "experiment-1",
                "e",
                ["author", "reviewer"],
                Some(["experiment"])
            ),
            identity_json(
                "experiment-2",
                "f",
                ["author", "reviewer"],
                Some(["experiment"])
            )
        ]);
        assert!(parse_alpha_identities(
            &six_person_dual_team_without_floating_reproducer.to_string()
        )
        .is_err());

        let seven_person_dual_team_with_floating_reproducer = serde_json::json!([
            identity_json("captain-1", "a", ["author", "evaluator"], Some(["captain"])),
            identity_json("captain-2", "b", ["author", "evaluator"], Some(["captain"])),
            identity_json(
                "evidence-1",
                "c",
                ["author", "reviewer"],
                Some(["evidence"])
            ),
            identity_json(
                "evidence-2",
                "d",
                ["author", "reviewer"],
                Some(["evidence"])
            ),
            identity_json(
                "experiment-1",
                "e",
                ["author", "reviewer"],
                Some(["experiment"])
            ),
            identity_json(
                "experiment-2",
                "f",
                ["author", "reviewer"],
                Some(["experiment"])
            ),
            identity_json(
                "floating-reproducer",
                "g",
                ["reproducer"],
                None::<[&str; 0]>
            )
        ]);
        assert!(parse_alpha_identities(
            &seven_person_dual_team_with_floating_reproducer.to_string()
        )
        .is_ok());

        let overlapping_author_capabilities_cannot_fake_three_distinct_roles = serde_json::json!([
            identity_json(
                "author-1",
                "a",
                ["author"],
                Some(["captain", "evidence", "experiment"])
            ),
            identity_json("author-2", "b", ["author"], Some(["captain"])),
            identity_json("author-3", "c", ["author"], Some(["captain"])),
            identity_json("evaluator-1", "d", ["evaluator"], None::<[&str; 0]>),
            identity_json("reviewer-1", "e", ["reviewer"], None::<[&str; 0]>),
            identity_json("reviewer-2", "f", ["reviewer"], None::<[&str; 0]>),
            identity_json("reproducer-1", "g", ["reproducer"], None::<[&str; 0]>)
        ]);
        assert!(parse_alpha_identities(
            &overlapping_author_capabilities_cannot_fake_three_distinct_roles.to_string()
        )
        .is_err());

        let mut null_scope = duplicate_scope;
        null_scope[0]["scopes"] = serde_json::Value::Null;
        assert!(parse_alpha_identities(&null_scope.to_string()).is_err());

        let mut null_roles = reviewer_with_author_role;
        null_roles[3]["author_roles"] = serde_json::Value::Null;
        assert!(parse_alpha_identities(&null_roles.to_string()).is_err());

        let too_many = serde_json::Value::Array(
            (0..=MAX_ALPHA_IDENTITIES)
                .map(|index| {
                    serde_json::json!({
                        "login_key": format!("alpha-login-key-{index}-{}", "x".repeat(32)),
                        "subject_id": format!("author-{index}"),
                        "display_name": format!("Author {index}"),
                        "nakama_user_id": Uuid::new_v4(),
                        "player_id": Uuid::new_v4()
                    })
                })
                .collect(),
        );
        assert!(parse_alpha_identities(&too_many.to_string()).is_err());
    }

    fn identity_json<const S: usize, const R: usize>(
        subject: &str,
        key_prefix: &str,
        scopes: [&str; S],
        author_roles: Option<[&str; R]>,
    ) -> serde_json::Value {
        let mut value = serde_json::json!({
            "login_key": key_prefix.repeat(32),
            "subject_id": subject,
            "display_name": subject,
            "nakama_user_id": Uuid::new_v4(),
            "player_id": Uuid::new_v4(),
            "scopes": scopes.as_slice()
        });
        if let Some(author_roles) = author_roles {
            value["author_roles"] = serde_json::json!(author_roles.as_slice());
        }
        value
    }
}
