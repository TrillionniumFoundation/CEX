//! Shared wire contracts for authenticated AppHash object proofs.
//!
//! These definitions mirror the frozen AppHash-v4 contracts currently used by
//! `trnm-consensus-app`: the JMT key preimage is namespaced with big-endian
//! component lengths, while the committed object wrapper uses Borsh.  Keeping
//! both contracts here lets receipt assemblers and independent verifiers bind
//! an ABCI proof to the exact logical object without importing node storage.

use anyhow::{ensure, Context, Result};
use borsh::{BorshDeserialize, BorshSerialize};
use sha2::{Digest, Sha256};

/// Domain prefix committed into every AppHash-v4 JMT key preimage.
pub const AUTHENTICATED_STATE_KEY_DOMAIN_V4: &[u8] = b"trnm/authenticated-state/v4";

/// Borsh schema version of [`AuthenticatedObjectRecordV1`].
pub const AUTHENTICATED_OBJECT_RECORD_SCHEMA_V1: u16 = 1;

/// Consensus-state namespace discriminants committed by AppHash v4.
///
/// Discriminants are consensus wire values and must never be renumbered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum AuthenticatedStateNamespaceV4 {
    Object = 1,
    Account = 2,
    Task = 3,
    ValidatorLifecycle = 4,
    Config = 5,
    CommandReceipt = 6,
    GovernanceSequence = 7,
}

/// Construct the exact AppHash-v4 JMT key preimage.
///
/// Components use an unsigned 32-bit big-endian length.  The component count
/// is an unsigned 16-bit big-endian value.  Empty component lists and empty
/// components are rejected because proof keys must have recoverable,
/// collision-free preimages.
pub fn authenticated_state_key_v4(
    namespace: AuthenticatedStateNamespaceV4,
    components: &[&[u8]],
) -> Result<Vec<u8>> {
    ensure!(
        !components.is_empty(),
        "authenticated key needs a component"
    );
    ensure!(
        components.len() <= u16::MAX as usize,
        "too many authenticated key components"
    );

    let mut key = Vec::with_capacity(AUTHENTICATED_STATE_KEY_DOMAIN_V4.len() + 8);
    key.extend_from_slice(AUTHENTICATED_STATE_KEY_DOMAIN_V4);
    key.push(0);
    key.push(namespace as u8);
    key.extend_from_slice(&(components.len() as u16).to_be_bytes());

    for component in components {
        ensure!(
            !component.is_empty(),
            "authenticated key components must be non-empty"
        );
        let component_len = u32::try_from(component.len())
            .context("authenticated key component exceeds u32::MAX bytes")?;
        key.extend_from_slice(&component_len.to_be_bytes());
        key.extend_from_slice(component);
    }

    Ok(key)
}

/// Recompute the exact JMT proof key for a logical stored-object key.
///
/// The logical key is committed as its exact UTF-8 bytes.  In particular this
/// helper does not lowercase, hex-decode, or otherwise normalize legacy keys.
pub fn authenticated_object_proof_key_v4(logical_object_key: &str) -> Result<Vec<u8>> {
    authenticated_state_key_v4(
        AuthenticatedStateNamespaceV4::Object,
        &[logical_object_key.as_bytes()],
    )
}

/// Recover the logical stored-object key from an exact AppHash-v4 proof key.
pub fn logical_object_key_from_proof_key_v4(proof_key: &[u8]) -> Result<String> {
    let domain_end = AUTHENTICATED_STATE_KEY_DOMAIN_V4.len();
    let header_end = domain_end
        .checked_add(4)
        .context("authenticated object key header length overflow")?;
    let length_end = header_end
        .checked_add(4)
        .context("authenticated object key length field overflow")?;
    ensure!(
        proof_key.len() >= length_end,
        "authenticated object key preimage is truncated"
    );
    ensure!(
        &proof_key[..domain_end] == AUTHENTICATED_STATE_KEY_DOMAIN_V4,
        "authenticated object key domain mismatch"
    );
    ensure!(
        proof_key[domain_end] == 0,
        "authenticated object key domain separator mismatch"
    );
    ensure!(
        proof_key[domain_end + 1] == AuthenticatedStateNamespaceV4::Object as u8,
        "authenticated key is not in the object namespace"
    );
    ensure!(
        u16::from_be_bytes([proof_key[domain_end + 2], proof_key[domain_end + 3]]) == 1,
        "authenticated object key must have one component"
    );

    let component_len_bytes: [u8; 4] = proof_key[header_end..length_end]
        .try_into()
        .context("authenticated object key length field is malformed")?;
    let component_len = u32::from_be_bytes(component_len_bytes) as usize;
    let component_end = length_end
        .checked_add(component_len)
        .context("authenticated object key component length overflow")?;
    ensure!(
        component_len > 0 && component_end == proof_key.len(),
        "authenticated object key component length mismatch"
    );
    String::from_utf8(proof_key[length_end..component_end].to_vec())
        .context("authenticated object key is not UTF-8")
}

/// Exact Borsh value committed for an object leaf in AppHash v4.
///
/// `value_hash` is redundant by design: strict decoding recomputes it before
/// any caller can interpret the inner value.
#[derive(Clone, Debug, Eq, PartialEq, BorshDeserialize, BorshSerialize)]
pub struct AuthenticatedObjectRecordV1 {
    pub schema_version: u16,
    pub object_type: String,
    pub object_version: u64,
    pub value_hash: [u8; 32],
    pub value: Vec<u8>,
}

impl AuthenticatedObjectRecordV1 {
    pub fn new(
        object_type: impl Into<String>,
        object_version: u64,
        value: Vec<u8>,
    ) -> Result<Self> {
        let object_type = object_type.into();
        ensure!(!object_type.is_empty(), "object type must be non-empty");
        let value_hash = Sha256::digest(&value).into();
        Ok(Self {
            schema_version: AUTHENTICATED_OBJECT_RECORD_SCHEMA_V1,
            object_type,
            object_version,
            value_hash,
            value,
        })
    }

    /// Encode the exact consensus Borsh representation.
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        borsh::to_vec(self).context("encode authenticated object record")
    }

    /// Strictly decode one complete consensus Borsh record.
    ///
    /// A layout preflight rejects forged length fields before Borsh allocates,
    /// and `borsh::from_slice` rejects trailing bytes.
    pub fn decode(encoded: &[u8]) -> Result<Self> {
        validate_record_wire_layout(encoded)?;
        let record: Self =
            borsh::from_slice(encoded).context("decode authenticated object record")?;
        record.validate()?;
        ensure!(
            record.encode()? == encoded,
            "authenticated object record is not canonical Borsh"
        );
        Ok(record)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == AUTHENTICATED_OBJECT_RECORD_SCHEMA_V1,
            "unsupported object record schema {}",
            self.schema_version
        );
        ensure!(
            !self.object_type.is_empty(),
            "object type must be non-empty"
        );
        ensure!(
            self.value_hash == <[u8; 32]>::from(Sha256::digest(&self.value)),
            "authenticated object value hash mismatch"
        );
        Ok(())
    }

    /// Bind every semantic field used by a receipt verifier.
    pub fn verify_binding(
        &self,
        expected_object_type: &str,
        expected_object_version: u64,
        expected_value: &[u8],
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            self.object_type == expected_object_type,
            "authenticated object type mismatch"
        );
        ensure!(
            self.object_version == expected_object_version,
            "authenticated object version mismatch"
        );
        ensure!(
            self.value == expected_value,
            "authenticated object value mismatch"
        );
        Ok(())
    }
}

fn validate_record_wire_layout(encoded: &[u8]) -> Result<()> {
    const SCHEMA_BYTES: usize = 2;
    const LENGTH_BYTES: usize = 4;
    const VERSION_BYTES: usize = 8;
    const HASH_BYTES: usize = 32;

    let object_type_length_end = SCHEMA_BYTES + LENGTH_BYTES;
    ensure!(
        encoded.len() >= object_type_length_end,
        "authenticated object record is truncated before object type"
    );
    let object_type_len_bytes: [u8; 4] =
        encoded[SCHEMA_BYTES..object_type_length_end]
            .try_into()
            .context("authenticated object type length field is malformed")?;
    let object_type_len = u32::from_le_bytes(object_type_len_bytes) as usize;
    ensure!(object_type_len > 0, "object type must be non-empty");

    let object_type_end = object_type_length_end
        .checked_add(object_type_len)
        .context("authenticated object type length overflow")?;
    let value_length_start = object_type_end
        .checked_add(VERSION_BYTES)
        .and_then(|end| end.checked_add(HASH_BYTES))
        .context("authenticated object record fixed fields overflow")?;
    let value_length_end = value_length_start
        .checked_add(LENGTH_BYTES)
        .context("authenticated object value length field overflow")?;
    ensure!(
        encoded.len() >= value_length_end,
        "authenticated object record is truncated before value"
    );
    let value_len_bytes: [u8; 4] = encoded[value_length_start..value_length_end]
        .try_into()
        .context("authenticated object value length field is malformed")?;
    let value_len = u32::from_le_bytes(value_len_bytes) as usize;
    let expected_end = value_length_end
        .checked_add(value_len)
        .context("authenticated object value length overflow")?;
    ensure!(
        expected_end == encoded.len(),
        "authenticated object record length mismatch"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consensus_object_proof_keys_match_existing_fixtures() {
        assert_eq!(
            hex::encode(authenticated_object_proof_key_v4("aa").unwrap()),
            "74726e6d2f61757468656e746963617465642d73746174652f763400010001000000026161"
        );
        assert_eq!(
            hex::encode(authenticated_object_proof_key_v4("ab12").unwrap()),
            "74726e6d2f61757468656e746963617465642d73746174652f7634000100010000000461623132"
        );

        let logical = "0fc3a6daebb13c878397ce926ba5084d9d9451202ea2b49fde828cad849d12a4";
        let proof_key = authenticated_object_proof_key_v4(logical).unwrap();
        assert_eq!(
            hex::encode(&proof_key),
            "74726e6d2f61757468656e746963617465642d73746174652f7634000100010000004030666333613664616562623133633837383339376365393236626135303834643964393435313230326561326234396664653832386361643834396431326134"
        );
        assert_eq!(
            logical_object_key_from_proof_key_v4(&proof_key).unwrap(),
            logical
        );
    }

    #[test]
    fn namespaced_components_remain_unambiguous_and_nonempty() {
        let left =
            authenticated_state_key_v4(AuthenticatedStateNamespaceV4::Object, &[b"ab", b"c"])
                .unwrap();
        let right =
            authenticated_state_key_v4(AuthenticatedStateNamespaceV4::Object, &[b"a", b"bc"])
                .unwrap();
        assert_ne!(left, right);
        assert!(authenticated_state_key_v4(AuthenticatedStateNamespaceV4::Object, &[]).is_err());
        assert!(authenticated_state_key_v4(AuthenticatedStateNamespaceV4::Object, &[b""]).is_err());
    }

    #[test]
    fn consensus_authenticated_record_matches_existing_fixture_bytes() {
        let record =
            AuthenticatedObjectRecordV1::new("account", 7, b"balance=42".to_vec()).unwrap();
        let encoded = record.encode().unwrap();
        assert_eq!(
            hex::encode(&encoded),
            "0100070000006163636f756e7407000000000000006caaa99bd3df253387ca038ea0be01d832c479983afa31eb5fff43da5445a0d30a00000062616c616e63653d3432"
        );
        assert_eq!(
            AuthenticatedObjectRecordV1::decode(&encoded).unwrap(),
            record
        );
    }

    #[test]
    fn object_key_decoder_rejects_domain_namespace_length_and_utf8_drift() {
        let baseline = authenticated_object_proof_key_v4("aa").unwrap();

        let mut wrong_domain = baseline.clone();
        wrong_domain[0] ^= 1;
        assert!(logical_object_key_from_proof_key_v4(&wrong_domain).is_err());

        let mut wrong_separator = baseline.clone();
        wrong_separator[AUTHENTICATED_STATE_KEY_DOMAIN_V4.len()] = 1;
        assert!(logical_object_key_from_proof_key_v4(&wrong_separator).is_err());

        let mut wrong_namespace = baseline.clone();
        wrong_namespace[AUTHENTICATED_STATE_KEY_DOMAIN_V4.len() + 1] =
            AuthenticatedStateNamespaceV4::Account as u8;
        assert!(logical_object_key_from_proof_key_v4(&wrong_namespace).is_err());

        let mut wrong_component_count = baseline.clone();
        wrong_component_count[AUTHENTICATED_STATE_KEY_DOMAIN_V4.len() + 3] = 2;
        assert!(logical_object_key_from_proof_key_v4(&wrong_component_count).is_err());

        let mut wrong_length = baseline.clone();
        let length_offset = AUTHENTICATED_STATE_KEY_DOMAIN_V4.len() + 4;
        wrong_length[length_offset + 3] = 3;
        assert!(logical_object_key_from_proof_key_v4(&wrong_length).is_err());

        let mut invalid_utf8 = baseline;
        *invalid_utf8.last_mut().unwrap() = 0xff;
        assert!(logical_object_key_from_proof_key_v4(&invalid_utf8).is_err());
    }

    #[test]
    fn record_decoder_rejects_schema_hash_length_and_trailing_drift() {
        let record =
            AuthenticatedObjectRecordV1::new("account", 7, b"balance=42".to_vec()).unwrap();
        let encoded = record.encode().unwrap();

        let mut wrong_schema = encoded.clone();
        wrong_schema[..2].copy_from_slice(&2u16.to_le_bytes());
        assert!(AuthenticatedObjectRecordV1::decode(&wrong_schema).is_err());

        let mut wrong_hash = encoded.clone();
        let hash_offset = 2 + 4 + "account".len() + 8;
        wrong_hash[hash_offset] ^= 1;
        assert!(AuthenticatedObjectRecordV1::decode(&wrong_hash).is_err());

        let mut wrong_length = encoded.clone();
        let value_length_offset = hash_offset + 32;
        wrong_length[value_length_offset..value_length_offset + 4]
            .copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(AuthenticatedObjectRecordV1::decode(&wrong_length).is_err());

        let mut trailing = encoded;
        trailing.push(0);
        assert!(AuthenticatedObjectRecordV1::decode(&trailing).is_err());
    }

    #[test]
    fn record_binding_checks_type_version_and_exact_value() {
        let record = AuthenticatedObjectRecordV1::new("research", 1, b"command".to_vec()).unwrap();
        record.verify_binding("research", 1, b"command").unwrap();
        assert!(record.verify_binding("other", 1, b"command").is_err());
        assert!(record.verify_binding("research", 2, b"command").is_err());
        assert!(record.verify_binding("research", 1, b"other").is_err());
    }
}
