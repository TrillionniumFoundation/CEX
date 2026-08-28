use crate::contract::{
    canonical_sha256, ServerSignedValueEntitlementV2, ENTITLEMENT_SIGNER_ISSUER,
    EXPECTED_GAME_AUTHORITY_AUDIENCE,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, env};

#[derive(Clone, Debug)]
pub struct AuthorityPrincipal {
    pub authority_id: String,
    pub audience: String,
    pub token_sha256: String,
    pub active: bool,
}

#[derive(Debug, Deserialize)]
struct AuthorityPrincipalConfig {
    authority_id: String,
    audience: String,
    token_sha256: String,
    #[serde(default = "default_true")]
    active: bool,
}

#[derive(Clone, Debug)]
pub struct AuthorityIdentity {
    pub authority_id: String,
}

#[derive(Clone, Debug)]
pub struct AuthorityRegistry {
    principals: Vec<AuthorityPrincipal>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorizationFailure {
    Missing,
    Invalid,
    WrongAudience,
}

impl AuthorityRegistry {
    pub fn from_env() -> Result<Self, String> {
        let raw = env::var("TRNM_GAME_AUTHORITY_PRINCIPALS_JSON")
            .map_err(|_| "TRNM_GAME_AUTHORITY_PRINCIPALS_JSON is required".to_string())?;
        let configs: Vec<AuthorityPrincipalConfig> = serde_json::from_str(&raw)
            .map_err(|error| format!("decode game authority principals: {error}"))?;
        let principals = configs
            .into_iter()
            .map(|config| AuthorityPrincipal {
                authority_id: config.authority_id,
                audience: config.audience,
                token_sha256: config.token_sha256,
                active: config.active,
            })
            .collect::<Vec<_>>();
        Self::new(principals)
    }

    pub fn new(principals: Vec<AuthorityPrincipal>) -> Result<Self, String> {
        if principals.is_empty() {
            return Err("at least one game authority principal is required".to_string());
        }
        for principal in &principals {
            if principal.authority_id.trim().is_empty()
                || principal.audience.trim().is_empty()
                || !canonical_sha256(&principal.token_sha256)
            {
                return Err("invalid game authority principal configuration".to_string());
            }
        }
        Ok(Self { principals })
    }

    pub fn authorize(
        &self,
        supplied_token: Option<&str>,
    ) -> Result<AuthorityIdentity, AuthorizationFailure> {
        let supplied = supplied_token
            .filter(|value| !value.trim().is_empty())
            .ok_or(AuthorizationFailure::Missing)?;
        let supplied_hash = format!("{:x}", Sha256::digest(supplied.as_bytes()));

        let mut matched_wrong_audience = false;
        for principal in &self.principals {
            if !principal.active
                || !constant_time_equal(supplied_hash.as_bytes(), principal.token_sha256.as_bytes())
            {
                continue;
            }
            if principal.audience != EXPECTED_GAME_AUTHORITY_AUDIENCE {
                matched_wrong_audience = true;
                continue;
            }
            return Ok(AuthorityIdentity {
                authority_id: principal.authority_id.clone(),
            });
        }

        if matched_wrong_audience {
            Err(AuthorizationFailure::WrongAudience)
        } else {
            Err(AuthorizationFailure::Invalid)
        }
    }

    pub fn active_expected_audience_count(&self) -> usize {
        self.principals
            .iter()
            .filter(|principal| {
                principal.active && principal.audience == EXPECTED_GAME_AUTHORITY_AUDIENCE
            })
            .count()
    }
}

#[derive(Clone, Debug)]
pub struct IssuerKeyRecord {
    pub key_id: String,
    pub issuer: String,
    pub status: String,
    pub signature_algorithm: String,
    pub public_key_sha256: String,
    pub verifying_key: VerifyingKey,
}

#[derive(Debug, Deserialize)]
struct IssuerKeyConfig {
    key_id: String,
    issuer: String,
    status: String,
    signature_algorithm: String,
    public_key_base64: String,
    public_key_sha256: String,
}

#[derive(Clone, Debug)]
pub struct IssuerKeyRegistry {
    keys: BTreeMap<String, IssuerKeyRecord>,
}

impl IssuerKeyRegistry {
    pub fn from_env() -> Result<Self, String> {
        let raw = env::var("TRNM_ENTITLEMENT_ISSUER_KEYS_JSON")
            .map_err(|_| "TRNM_ENTITLEMENT_ISSUER_KEYS_JSON is required".to_string())?;
        let configs: Vec<IssuerKeyConfig> = serde_json::from_str(&raw)
            .map_err(|error| format!("decode entitlement issuer keys: {error}"))?;

        let mut records = Vec::with_capacity(configs.len());
        for config in configs {
            let decoded = STANDARD
                .decode(config.public_key_base64.as_bytes())
                .map_err(|error| format!("decode issuer public key: {error}"))?;
            let bytes: [u8; 32] = decoded
                .try_into()
                .map_err(|_| "issuer public key must contain 32 bytes".to_string())?;
            let fingerprint = format!("{:x}", Sha256::digest(bytes));
            if fingerprint != config.public_key_sha256 {
                return Err("issuer public-key fingerprint mismatch".to_string());
            }
            let verifying_key = VerifyingKey::from_bytes(&bytes)
                .map_err(|error| format!("decode issuer Ed25519 key: {error}"))?;
            records.push(IssuerKeyRecord {
                key_id: config.key_id,
                issuer: config.issuer,
                status: config.status,
                signature_algorithm: config.signature_algorithm,
                public_key_sha256: config.public_key_sha256,
                verifying_key,
            });
        }
        Self::new(records)
    }

    pub fn new(records: Vec<IssuerKeyRecord>) -> Result<Self, String> {
        if records.is_empty() {
            return Err("at least one entitlement issuer key is required".to_string());
        }

        let mut keys = BTreeMap::new();
        for record in records {
            if record.key_id.trim().is_empty()
                || record.issuer != ENTITLEMENT_SIGNER_ISSUER
                || record.signature_algorithm != "ed25519"
                || !canonical_sha256(&record.public_key_sha256)
                || keys.contains_key(&record.key_id)
            {
                return Err("invalid or duplicate entitlement issuer key".to_string());
            }
            keys.insert(record.key_id.clone(), record);
        }
        Ok(Self { keys })
    }

    pub fn get(&self, key_id: &str) -> Option<&IssuerKeyRecord> {
        self.keys.get(key_id)
    }

    pub fn active_count(&self) -> usize {
        self.keys
            .values()
            .filter(|record| record.status == "active")
            .count()
    }

    pub fn verify_entitlement(
        &self,
        entitlement: &ServerSignedValueEntitlementV2,
    ) -> Result<(), String> {
        let key = self
            .get(&entitlement.key_id)
            .ok_or_else(|| "entitlement issuer key is not registered".to_string())?;
        if key.status != "active"
            || key.issuer != entitlement.issuer
            || key.signature_algorithm != entitlement.signature_algorithm
        {
            return Err("entitlement issuer key is not active for this payload".to_string());
        }

        let signature_bytes = STANDARD
            .decode(entitlement.signature.as_bytes())
            .map_err(|error| format!("decode entitlement signature: {error}"))?;
        let signature = Signature::from_slice(&signature_bytes)
            .map_err(|error| format!("decode entitlement Ed25519 signature: {error}"))?;
        let payload = entitlement.signing_payload()?;
        key.verifying_key
            .verify(&payload, &signature)
            .map_err(|_| "entitlement signature verification failed".to_string())
    }
}

fn default_true() -> bool {
    true
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0_u8;
    for (left, right) in left.iter().zip(right.iter()) {
        difference |= left ^ right;
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use super::constant_time_equal;

    #[test]
    fn constant_time_comparison_rejects_length_and_content_mismatch() {
        assert!(constant_time_equal(b"abc", b"abc"));
        assert!(!constant_time_equal(b"abc", b"abd"));
        assert!(!constant_time_equal(b"abc", b"abcd"));
    }
}
