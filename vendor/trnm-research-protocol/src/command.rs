use crate::canonical::{
    canonical_hash, CanonicalCbor, CanonicalDecodeError, Decoder, Encoder, CANONICAL_ENCODING,
};
use crate::types::{
    ExternalKey, ResearchCommandV1, ResearchPayloadValidationError, PROTOCOL_VERSION,
};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityRole {
    NakamaAuthority = 1,
    HeptaAuthority = 2,
}

impl CanonicalCbor for AuthorityRole {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.uint(*self as u64);
    }
}

/// Signature-bearing chain command. The signer role is consensus-visible and
/// covered by the signature.
///
/// Nakama authority keys can sign only match-fact commitments. Every other
/// research object must be issued through a Hepta authority key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedResearchCommandV1 {
    pub chain_id: String,
    pub command_id: ExternalKey,
    pub signer_did: String,
    pub signer_role: AuthorityRole,
    pub nonce: u64,
    pub public_key: [u8; 32],
    pub command: ResearchCommandV1,
    pub signature: Vec<u8>,
}

impl SignedResearchCommandV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn sign(
        chain_id: String,
        command_id: ExternalKey,
        signer_did: String,
        signer_role: AuthorityRole,
        nonce: u64,
        command: ResearchCommandV1,
        signing_key: &SigningKey,
    ) -> Result<Self, SignedResearchCommandValidationError> {
        let mut signed = Self {
            chain_id,
            command_id,
            signer_did,
            signer_role,
            nonce,
            public_key: signing_key.verifying_key().to_bytes(),
            command,
            signature: Vec::new(),
        };
        signed.validate_unsigned()?;
        signed.signature = signing_key
            .sign(&signed.signing_bytes())
            .to_bytes()
            .to_vec();
        Ok(signed)
    }

    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut encoder = Encoder::default();
        encoder.array(9);
        encoder.uint(PROTOCOL_VERSION as u64);
        encoder.text(CANONICAL_ENCODING);
        encoder.text(&self.chain_id);
        self.command_id.encode_canonical(&mut encoder);
        encoder.text(&self.signer_did);
        self.signer_role.encode_canonical(&mut encoder);
        encoder.uint(self.nonce);
        encoder.bytes(&self.public_key);
        self.command.encode_canonical(&mut encoder);
        encoder.finish()
    }

    /// Strict deterministic-CBOR decoder for a complete signed protocol
    /// envelope. Validation includes byte-for-byte canonical re-encoding,
    /// payload validation, role authorization, and Ed25519 verification.
    pub fn from_canonical_bytes(
        bytes: &[u8],
    ) -> Result<Self, SignedResearchCommandValidationError> {
        let mut decoder = Decoder::new(bytes);
        decoder.array(10)?;
        let version = decoder.uint()?;
        if version != PROTOCOL_VERSION as u64 {
            return Err(CanonicalDecodeError::UnsupportedVersion(version).into());
        }
        let encoding = decoder.text()?;
        if encoding != CANONICAL_ENCODING {
            return Err(SignedResearchCommandValidationError::EncodingMismatch(
                encoding,
            ));
        }
        let chain_id = decoder.text()?;
        let command_id = ExternalKey::from_bytes(decoder.bytes_exact()?);
        let signer_did = decoder.text()?;
        let role_raw = decoder.uint()?;
        let signer_role = match role_raw {
            1 => AuthorityRole::NakamaAuthority,
            2 => AuthorityRole::HeptaAuthority,
            value => {
                return Err(CanonicalDecodeError::UnknownDiscriminant {
                    name: "AuthorityRole",
                    value,
                }
                .into())
            }
        };
        let nonce = decoder.uint()?;
        let public_key = decoder.bytes_exact()?;
        let command = ResearchCommandV1::decode(&mut decoder)?;
        let signature = decoder.bytes()?.to_vec();
        decoder.finish()?;

        let signed = Self {
            chain_id,
            command_id,
            signer_did,
            signer_role,
            nonce,
            public_key,
            command,
            signature,
        };
        if signed.canonical_bytes() != bytes {
            return Err(CanonicalDecodeError::NonCanonicalRoundTrip.into());
        }
        signed.validate()?;
        Ok(signed)
    }

    /// Stable fingerprint retained for command-id replay handling.
    pub fn command_fingerprint(&self) -> [u8; 32] {
        canonical_hash(
            "trnm-research-command-fingerprint-v1",
            &self.signing_bytes(),
        )
    }

    pub fn payload_hash(&self) -> [u8; 32] {
        self.command
            .canonical_hash("trnm-research-command-payload-v1")
    }

    pub fn validate(&self) -> Result<(), SignedResearchCommandValidationError> {
        self.validate_unsigned()?;
        let signature_bytes: [u8; 64] = self
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| SignedResearchCommandValidationError::InvalidSignature)?;
        let verifying_key = VerifyingKey::from_bytes(&self.public_key)
            .map_err(|_| SignedResearchCommandValidationError::InvalidPublicKey)?;
        verifying_key
            .verify_strict(
                &self.signing_bytes(),
                &Signature::from_bytes(&signature_bytes),
            )
            .map_err(|_| SignedResearchCommandValidationError::InvalidSignature)
    }

    fn validate_unsigned(&self) -> Result<(), SignedResearchCommandValidationError> {
        validate_chain_id(&self.chain_id)?;
        self.command_id
            .validate("command_id")
            .map_err(SignedResearchCommandValidationError::Payload)?;
        validate_did(&self.signer_did)?;
        if self.nonce == 0 {
            return Err(SignedResearchCommandValidationError::ZeroNonce);
        }
        if self.public_key == [0; 32] {
            return Err(SignedResearchCommandValidationError::InvalidPublicKey);
        }
        self.command
            .validate()
            .map_err(SignedResearchCommandValidationError::Payload)?;

        let authorized = matches!(
            (&self.signer_role, &self.command),
            (
                AuthorityRole::NakamaAuthority,
                ResearchCommandV1::MatchEvidenceCommitment(_)
            ) | (
                AuthorityRole::HeptaAuthority,
                ResearchCommandV1::EvaluationCommitment(_)
                    | ResearchCommandV1::IssueWorkloadReceipt(_)
                    | ResearchCommandV1::CreateResearchClaim(_)
                    | ResearchCommandV1::DeclareLicense(_)
                    | ResearchCommandV1::ChallengeResearchClaim(_)
                    | ResearchCommandV1::ResolveResearchClaim(_)
            )
        );
        if !authorized {
            return Err(SignedResearchCommandValidationError::UnauthorizedCommand {
                role: self.signer_role,
                command_type: self.command.command_type(),
            });
        }
        Ok(())
    }
}

impl CanonicalCbor for SignedResearchCommandV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(10);
        encoder.uint(PROTOCOL_VERSION as u64);
        encoder.text(CANONICAL_ENCODING);
        encoder.text(&self.chain_id);
        self.command_id.encode_canonical(encoder);
        encoder.text(&self.signer_did);
        self.signer_role.encode_canonical(encoder);
        encoder.uint(self.nonce);
        encoder.bytes(&self.public_key);
        self.command.encode_canonical(encoder);
        encoder.bytes(&self.signature);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SignedResearchCommandValidationError {
    #[error(transparent)]
    CanonicalDecode(#[from] CanonicalDecodeError),
    #[error("unsupported canonical encoding {0}")]
    EncodingMismatch(String),
    #[error("chain id must be canonical lowercase ASCII")]
    InvalidChainId,
    #[error("signer DID must be a canonical did:* visible ASCII token")]
    InvalidSignerDid,
    #[error("nonce must be greater than zero")]
    ZeroNonce,
    #[error("invalid Ed25519 public key")]
    InvalidPublicKey,
    #[error("invalid Ed25519 signature")]
    InvalidSignature,
    #[error("{role:?} cannot submit {command_type}")]
    UnauthorizedCommand {
        role: AuthorityRole,
        command_type: &'static str,
    },
    #[error(transparent)]
    Payload(#[from] ResearchPayloadValidationError),
}

fn validate_chain_id(chain_id: &str) -> Result<(), SignedResearchCommandValidationError> {
    if chain_id.is_empty()
        || chain_id.len() > 64
        || !chain_id.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'-' | b'_' | b'.' | b':')
        })
    {
        return Err(SignedResearchCommandValidationError::InvalidChainId);
    }
    Ok(())
}

fn validate_did(did: &str) -> Result<(), SignedResearchCommandValidationError> {
    if did.len() < 5
        || did.len() > 192
        || !did.starts_with("did:")
        || !did.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(SignedResearchCommandValidationError::InvalidSignerDid);
    }
    Ok(())
}
