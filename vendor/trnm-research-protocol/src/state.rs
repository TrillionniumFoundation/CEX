use crate::canonical::{canonical_hash, CanonicalCbor, CanonicalDecodeError, Decoder, Encoder};
use crate::command::{
    AuthorityRole, SignedResearchCommandV1, SignedResearchCommandValidationError,
};
use crate::types::{
    decode_challenge, decode_claim, decode_claim_share, decode_evaluation, decode_external_key,
    decode_license, decode_match, decode_object_ref, decode_resolution, decode_version,
    decode_workload, validate_claim_shares, ClaimChallengeStatus, ClaimResolutionDecision,
    ClaimResolutionV1, ClaimShareV1, ClaimStatus, CreateResearchClaimV1, DeclareLicenseV1,
    EvaluationCommitmentV1, ExternalKey, IssueWorkloadReceiptV1, MatchEvidenceCommitmentV1,
    ObjectRefV1, ResearchCommandV1, ResearchObjectKind, PROTOCOL_VERSION,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AuthorityIdentityV1 {
    pub signer_did: String,
    pub public_key: [u8; 32],
}

impl AuthorityIdentityV1 {
    pub fn new(signer_did: String, public_key: [u8; 32]) -> Result<Self, ProtocolStateError> {
        let identity = Self {
            signer_did,
            public_key,
        };
        identity.validate()?;
        Ok(identity)
    }

    fn validate(&self) -> Result<(), ProtocolStateError> {
        if self.signer_did.len() < 5
            || self.signer_did.len() > 192
            || !self.signer_did.starts_with("did:")
            || !self.signer_did.bytes().all(|byte| byte.is_ascii_graphic())
            || self.public_key == [0; 32]
            || ed25519_dalek::VerifyingKey::from_bytes(&self.public_key).is_err()
        {
            return Err(ProtocolStateError::InvalidAuthoritySet);
        }
        Ok(())
    }
}

impl CanonicalCbor for AuthorityIdentityV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(3);
        encoder.uint(PROTOCOL_VERSION as u64);
        encoder.text(&self.signer_did);
        encoder.bytes(&self.public_key);
    }
}

/// Genesis-derived in-protocol trust anchor. Ingress should still enforce its
/// own capability policy; this second layer prevents an untrusted caller from
/// self-asserting an authority role directly against the state machine.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoritySetV1 {
    pub nakama_authorities: Vec<AuthorityIdentityV1>,
    pub hepta_authorities: Vec<AuthorityIdentityV1>,
}

impl AuthoritySetV1 {
    pub fn new(
        mut nakama_authorities: Vec<AuthorityIdentityV1>,
        mut hepta_authorities: Vec<AuthorityIdentityV1>,
    ) -> Result<Self, ProtocolStateError> {
        nakama_authorities.sort();
        hepta_authorities.sort();
        let authorities = Self {
            nakama_authorities,
            hepta_authorities,
        };
        authorities.validate()?;
        Ok(authorities)
    }

    pub fn validate(&self) -> Result<(), ProtocolStateError> {
        validate_authority_bucket(&self.nakama_authorities)?;
        validate_authority_bucket(&self.hepta_authorities)?;
        Ok(())
    }

    pub fn authorizes(&self, signed: &SignedResearchCommandV1) -> bool {
        let bucket = match signed.signer_role {
            AuthorityRole::NakamaAuthority => &self.nakama_authorities,
            AuthorityRole::HeptaAuthority => &self.hepta_authorities,
        };
        bucket
            .binary_search_by(|identity| {
                (&identity.signer_did, identity.public_key)
                    .cmp(&(&signed.signer_did, signed.public_key))
            })
            .is_ok()
    }
}

impl CanonicalCbor for AuthoritySetV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(3);
        encoder.uint(PROTOCOL_VERSION as u64);
        encoder.array(self.nakama_authorities.len());
        for identity in &self.nakama_authorities {
            identity.encode_canonical(encoder);
        }
        encoder.array(self.hepta_authorities.len());
        for identity in &self.hepta_authorities {
            identity.encode_canonical(encoder);
        }
    }
}

fn validate_authority_bucket(identities: &[AuthorityIdentityV1]) -> Result<(), ProtocolStateError> {
    if identities.len() > 128 {
        return Err(ProtocolStateError::InvalidAuthoritySet);
    }
    let mut previous = None;
    for identity in identities {
        identity.validate()?;
        let key = (&identity.signer_did, identity.public_key);
        if previous.is_some_and(|prior| prior >= key) {
            return Err(ProtocolStateError::InvalidAuthoritySet);
        }
        previous = Some(key);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchEvidenceObjectV1 {
    pub object_ref: ObjectRefV1,
    pub commitment: MatchEvidenceCommitmentV1,
}

impl CanonicalCbor for MatchEvidenceObjectV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(3);
        encoder.uint(PROTOCOL_VERSION as u64);
        self.object_ref.encode_canonical(encoder);
        self.commitment.encode_canonical(encoder);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationCommitmentObjectV1 {
    pub object_ref: ObjectRefV1,
    pub commitment: EvaluationCommitmentV1,
}

impl CanonicalCbor for EvaluationCommitmentObjectV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(3);
        encoder.uint(PROTOCOL_VERSION as u64);
        self.object_ref.encode_canonical(encoder);
        self.commitment.encode_canonical(encoder);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkloadReceiptObjectV1 {
    pub object_ref: ObjectRefV1,
    pub receipt: IssueWorkloadReceiptV1,
}

impl CanonicalCbor for WorkloadReceiptObjectV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(3);
        encoder.uint(PROTOCOL_VERSION as u64);
        self.object_ref.encode_canonical(encoder);
        self.receipt.encode_canonical(encoder);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchClaimObjectV1 {
    pub object_ref: ObjectRefV1,
    /// Immutable originally signed claim payload.
    pub claim: CreateResearchClaimV1,
    /// Current allocation after any versioned resolution amendments.
    pub current_claimants: Vec<ClaimShareV1>,
    pub status: ClaimStatus,
    pub active_challenge: Option<ExternalKey>,
}

impl CanonicalCbor for ResearchClaimObjectV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(6);
        encoder.uint(PROTOCOL_VERSION as u64);
        self.object_ref.encode_canonical(encoder);
        self.claim.encode_canonical(encoder);
        encoder.array(self.current_claimants.len());
        for claimant in &self.current_claimants {
            claimant.encode_canonical(encoder);
        }
        self.status.encode_canonical(encoder);
        match self.active_challenge {
            Some(key) => key.encode_canonical(encoder),
            None => encoder.null(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LicenseDeclarationObjectV1 {
    pub object_ref: ObjectRefV1,
    pub declaration: DeclareLicenseV1,
}

impl CanonicalCbor for LicenseDeclarationObjectV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(3);
        encoder.uint(PROTOCOL_VERSION as u64);
        self.object_ref.encode_canonical(encoder);
        self.declaration.encode_canonical(encoder);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimChallengeObjectV1 {
    pub object_ref: ObjectRefV1,
    pub challenge: crate::types::ChallengeResearchClaimV1,
    pub status: ClaimChallengeStatus,
    pub resolution_ref: Option<ObjectRefV1>,
}

impl CanonicalCbor for ClaimChallengeObjectV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(5);
        encoder.uint(PROTOCOL_VERSION as u64);
        self.object_ref.encode_canonical(encoder);
        self.challenge.encode_canonical(encoder);
        self.status.encode_canonical(encoder);
        match &self.resolution_ref {
            Some(object_ref) => object_ref.encode_canonical(encoder),
            None => encoder.null(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimResolutionObjectV1 {
    pub object_ref: ObjectRefV1,
    pub resolution: ClaimResolutionV1,
}

impl CanonicalCbor for ClaimResolutionObjectV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(3);
        encoder.uint(PROTOCOL_VERSION as u64);
        self.object_ref.encode_canonical(encoder);
        self.resolution.encode_canonical(encoder);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedCommandRecordV1 {
    pub command_id: ExternalKey,
    pub fingerprint: [u8; 32],
    pub primary_object_ref: ObjectRefV1,
}

impl CanonicalCbor for AppliedCommandRecordV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(4);
        encoder.uint(PROTOCOL_VERSION as u64);
        self.command_id.encode_canonical(encoder);
        encoder.bytes(&self.fingerprint);
        self.primary_object_ref.encode_canonical(encoder);
    }
}

impl AppliedCommandRecordV1 {
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ProtocolStateError> {
        let mut decoder = Decoder::new(bytes);
        decoder.array(4)?;
        decode_version(&mut decoder)?;
        let record = Self {
            command_id: decode_external_key(&mut decoder)?,
            fingerprint: decoder.bytes_exact()?,
            primary_object_ref: decode_object_ref(&mut decoder)?,
        };
        decoder.finish()?;
        record.command_id.validate("command_id")?;
        record.primary_object_ref.validate("primary_object_ref")?;
        if record.fingerprint == [0; 32] || record.canonical_bytes() != bytes {
            return Err(ProtocolStateError::InvalidSnapshotGraph);
        }
        Ok(record)
    }
}

/// One independently authenticated Research domain object. Runtime execution
/// loads only the command's explicit read-set and feeds that bounded fragment
/// through the same state-transition implementation used by snapshot tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResearchDomainObjectV1 {
    MatchEvidence(MatchEvidenceObjectV1),
    EvaluationCommitment(EvaluationCommitmentObjectV1),
    WorkloadReceipt(WorkloadReceiptObjectV1),
    ResearchClaim(ResearchClaimObjectV1),
    LicenseDeclaration(LicenseDeclarationObjectV1),
    ClaimChallenge(ClaimChallengeObjectV1),
    ClaimResolution(ClaimResolutionObjectV1),
}

impl ResearchDomainObjectV1 {
    pub fn object_ref(&self) -> ObjectRefV1 {
        match self {
            Self::MatchEvidence(object) => object.object_ref,
            Self::EvaluationCommitment(object) => object.object_ref,
            Self::WorkloadReceipt(object) => object.object_ref,
            Self::ResearchClaim(object) => object.object_ref,
            Self::LicenseDeclaration(object) => object.object_ref,
            Self::ClaimChallenge(object) => object.object_ref,
            Self::ClaimResolution(object) => object.object_ref,
        }
    }

    pub fn from_canonical_bytes(
        kind: ResearchObjectKind,
        bytes: &[u8],
    ) -> Result<Self, ProtocolStateError> {
        let mut decoder = Decoder::new(bytes);
        let object = match kind {
            ResearchObjectKind::MatchEvidence => {
                decoder.array(3)?;
                decode_version(&mut decoder)?;
                Self::MatchEvidence(MatchEvidenceObjectV1 {
                    object_ref: decode_object_ref(&mut decoder)?,
                    commitment: decode_match(&mut decoder)?,
                })
            }
            ResearchObjectKind::EvaluationCommitment => {
                decoder.array(3)?;
                decode_version(&mut decoder)?;
                Self::EvaluationCommitment(EvaluationCommitmentObjectV1 {
                    object_ref: decode_object_ref(&mut decoder)?,
                    commitment: decode_evaluation(&mut decoder)?,
                })
            }
            ResearchObjectKind::WorkloadReceipt => {
                decoder.array(3)?;
                decode_version(&mut decoder)?;
                Self::WorkloadReceipt(WorkloadReceiptObjectV1 {
                    object_ref: decode_object_ref(&mut decoder)?,
                    receipt: decode_workload(&mut decoder)?,
                })
            }
            ResearchObjectKind::ResearchClaim => {
                decoder.array(6)?;
                decode_version(&mut decoder)?;
                let object_ref = decode_object_ref(&mut decoder)?;
                let claim = decode_claim(&mut decoder)?;
                let claimant_count = decoder.array_len()?;
                let mut current_claimants = Vec::with_capacity(claimant_count.min(128));
                for _ in 0..claimant_count {
                    current_claimants.push(decode_claim_share(&mut decoder)?);
                }
                let status = decode_claim_status(&mut decoder)?;
                let active_challenge = if decoder.consume_null() {
                    None
                } else {
                    Some(decode_external_key(&mut decoder)?)
                };
                Self::ResearchClaim(ResearchClaimObjectV1 {
                    object_ref,
                    claim,
                    current_claimants,
                    status,
                    active_challenge,
                })
            }
            ResearchObjectKind::LicenseDeclaration => {
                decoder.array(3)?;
                decode_version(&mut decoder)?;
                Self::LicenseDeclaration(LicenseDeclarationObjectV1 {
                    object_ref: decode_object_ref(&mut decoder)?,
                    declaration: decode_license(&mut decoder)?,
                })
            }
            ResearchObjectKind::ClaimChallenge => {
                decoder.array(5)?;
                decode_version(&mut decoder)?;
                let object_ref = decode_object_ref(&mut decoder)?;
                let challenge = decode_challenge(&mut decoder)?;
                let status = decode_challenge_status(&mut decoder)?;
                let resolution_ref = if decoder.consume_null() {
                    None
                } else {
                    Some(decode_object_ref(&mut decoder)?)
                };
                Self::ClaimChallenge(ClaimChallengeObjectV1 {
                    object_ref,
                    challenge,
                    status,
                    resolution_ref,
                })
            }
            ResearchObjectKind::ClaimResolution => {
                decoder.array(3)?;
                decode_version(&mut decoder)?;
                Self::ClaimResolution(ClaimResolutionObjectV1 {
                    object_ref: decode_object_ref(&mut decoder)?,
                    resolution: decode_resolution(&mut decoder)?,
                })
            }
        };
        decoder.finish()?;
        object.validate_intrinsic()?;
        if object.canonical_bytes() != bytes {
            return Err(CanonicalDecodeError::NonCanonicalRoundTrip.into());
        }
        Ok(object)
    }

    fn validate_intrinsic(&self) -> Result<(), ProtocolStateError> {
        match self {
            Self::MatchEvidence(object) => {
                object.commitment.validate()?;
                validate_snapshot_object_ref(
                    object.object_ref,
                    ResearchObjectKind::MatchEvidence,
                    object.commitment.commitment_id,
                )?;
                require_initial_version(object.object_ref)
            }
            Self::EvaluationCommitment(object) => {
                object.commitment.validate()?;
                validate_snapshot_object_ref(
                    object.object_ref,
                    ResearchObjectKind::EvaluationCommitment,
                    object.commitment.evaluation_id,
                )?;
                require_initial_version(object.object_ref)
            }
            Self::WorkloadReceipt(object) => {
                object.receipt.validate()?;
                validate_snapshot_object_ref(
                    object.object_ref,
                    ResearchObjectKind::WorkloadReceipt,
                    object.receipt.receipt_id,
                )?;
                require_initial_version(object.object_ref)
            }
            Self::ResearchClaim(object) => {
                object.claim.validate()?;
                validate_snapshot_object_ref(
                    object.object_ref,
                    ResearchObjectKind::ResearchClaim,
                    object.claim.claim_id,
                )?;
                validate_claim_shares(&object.current_claimants, false)?;
                let challenge_consistent = match object.status {
                    ClaimStatus::Challenged => object.active_challenge.is_some(),
                    _ => object.active_challenge.is_none(),
                };
                if challenge_consistent {
                    Ok(())
                } else {
                    Err(ProtocolStateError::InvalidSnapshotGraph)
                }
            }
            Self::LicenseDeclaration(object) => {
                object.declaration.validate()?;
                validate_snapshot_object_ref(
                    object.object_ref,
                    ResearchObjectKind::LicenseDeclaration,
                    object.declaration.declaration_id,
                )?;
                require_initial_version(object.object_ref)
            }
            Self::ClaimChallenge(object) => {
                object.challenge.validate()?;
                validate_snapshot_object_ref(
                    object.object_ref,
                    ResearchObjectKind::ClaimChallenge,
                    object.challenge.challenge_id,
                )?;
                let consistent = match object.status {
                    ClaimChallengeStatus::Open => {
                        object.object_ref.object_version == 1 && object.resolution_ref.is_none()
                    }
                    ClaimChallengeStatus::Resolved => {
                        object.object_ref.object_version == 2 && object.resolution_ref.is_some()
                    }
                };
                if consistent {
                    Ok(())
                } else {
                    Err(ProtocolStateError::InvalidSnapshotGraph)
                }
            }
            Self::ClaimResolution(object) => {
                object.resolution.validate()?;
                validate_snapshot_object_ref(
                    object.object_ref,
                    ResearchObjectKind::ClaimResolution,
                    object.resolution.resolution_id,
                )?;
                require_initial_version(object.object_ref)
            }
        }
    }
}

impl CanonicalCbor for ResearchDomainObjectV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        match self {
            Self::MatchEvidence(object) => object.encode_canonical(encoder),
            Self::EvaluationCommitment(object) => object.encode_canonical(encoder),
            Self::WorkloadReceipt(object) => object.encode_canonical(encoder),
            Self::ResearchClaim(object) => object.encode_canonical(encoder),
            Self::LicenseDeclaration(object) => object.encode_canonical(encoder),
            Self::ClaimChallenge(object) => object.encode_canonical(encoder),
            Self::ClaimResolution(object) => object.encode_canonical(encoder),
        }
    }
}

fn decode_claim_status(decoder: &mut Decoder<'_>) -> Result<ClaimStatus, CanonicalDecodeError> {
    let value = decoder.uint()?;
    match value {
        1 => Ok(ClaimStatus::Active),
        2 => Ok(ClaimStatus::Challenged),
        3 => Ok(ClaimStatus::Rejected),
        4 => Ok(ClaimStatus::Amended),
        5 => Ok(ClaimStatus::LicenseAmendmentRequired),
        value => Err(CanonicalDecodeError::UnknownDiscriminant {
            name: "ClaimStatus",
            value,
        }),
    }
}

fn decode_challenge_status(
    decoder: &mut Decoder<'_>,
) -> Result<ClaimChallengeStatus, CanonicalDecodeError> {
    let value = decoder.uint()?;
    match value {
        1 => Ok(ClaimChallengeStatus::Open),
        2 => Ok(ClaimChallengeStatus::Resolved),
        value => Err(CanonicalDecodeError::UnknownDiscriminant {
            name: "ClaimChallengeStatus",
            value,
        }),
    }
}

fn require_initial_version(object_ref: ObjectRefV1) -> Result<(), ProtocolStateError> {
    if object_ref.object_version == 1 {
        Ok(())
    } else {
        Err(ProtocolStateError::InvalidSnapshotGraph)
    }
}

/// Vector-backed persistence form so JSON and other serde formats do not need
/// to encode 32-byte map keys as object-property strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchProtocolSnapshotV1 {
    pub protocol_version: u8,
    pub authorities: AuthoritySetV1,
    pub matches: Vec<MatchEvidenceObjectV1>,
    pub evaluations: Vec<EvaluationCommitmentObjectV1>,
    pub workload_receipts: Vec<WorkloadReceiptObjectV1>,
    pub claims: Vec<ResearchClaimObjectV1>,
    pub licenses: Vec<LicenseDeclarationObjectV1>,
    pub challenges: Vec<ClaimChallengeObjectV1>,
    pub resolutions: Vec<ClaimResolutionObjectV1>,
    pub applied_commands: Vec<AppliedCommandRecordV1>,
}

impl CanonicalCbor for ResearchProtocolSnapshotV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(10);
        encoder.uint(self.protocol_version as u64);
        self.authorities.encode_canonical(encoder);
        encode_values(encoder, &self.matches);
        encode_values(encoder, &self.evaluations);
        encode_values(encoder, &self.workload_receipts);
        encode_values(encoder, &self.claims);
        encode_values(encoder, &self.licenses);
        encode_values(encoder, &self.challenges);
        encode_values(encoder, &self.resolutions);
        encode_values(encoder, &self.applied_commands);
    }
}

fn encode_values<T: CanonicalCbor>(encoder: &mut Encoder, values: &[T]) {
    encoder.array(values.len());
    for value in values {
        value.encode_canonical(encoder);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOutcome {
    Applied {
        primary_object_ref: ObjectRefV1,
        changed_object_refs: Vec<ObjectRefV1>,
    },
    Idempotent {
        primary_object_ref: ObjectRefV1,
        changed_object_refs: Vec<ObjectRefV1>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResearchProtocolState {
    authorities: AuthoritySetV1,
    matches: BTreeMap<ExternalKey, MatchEvidenceObjectV1>,
    evaluations: BTreeMap<ExternalKey, EvaluationCommitmentObjectV1>,
    workload_receipts: BTreeMap<ExternalKey, WorkloadReceiptObjectV1>,
    claims: BTreeMap<ExternalKey, ResearchClaimObjectV1>,
    licenses: BTreeMap<ExternalKey, LicenseDeclarationObjectV1>,
    challenges: BTreeMap<ExternalKey, ClaimChallengeObjectV1>,
    resolutions: BTreeMap<ExternalKey, ClaimResolutionObjectV1>,
    applied_commands: BTreeMap<ExternalKey, AppliedCommandRecordV1>,
}

impl ResearchProtocolState {
    pub fn with_authorities(authorities: AuthoritySetV1) -> Result<Self, ProtocolStateError> {
        authorities.validate()?;
        Ok(Self {
            authorities,
            ..Self::default()
        })
    }

    pub fn authorities(&self) -> &AuthoritySetV1 {
        &self.authorities
    }

    pub fn authorize(
        authorities: &AuthoritySetV1,
        signed: &SignedResearchCommandV1,
    ) -> Result<(), ProtocolStateError> {
        signed.validate()?;
        authorities.validate()?;
        if authorities.authorizes(signed) {
            Ok(())
        } else {
            Err(ProtocolStateError::UnauthorizedAuthority {
                signer_did: signed.signer_did.clone(),
                role: signed.signer_role,
                public_key: signed.public_key,
            })
        }
    }

    /// Construct a bounded execution fragment from independently authenticated
    /// domain objects. Cross-object invariants remain enforced by [`Self::apply`]
    /// against the command's explicit read-set; unrelated global objects are not
    /// loaded or scanned.
    pub fn from_fragment(
        authorities: AuthoritySetV1,
        objects: impl IntoIterator<Item = ResearchDomainObjectV1>,
    ) -> Result<Self, ProtocolStateError> {
        let mut state = Self::with_authorities(authorities)?;
        for object in objects {
            object.validate_intrinsic()?;
            let object_ref = object.object_ref();
            let duplicate = match object {
                ResearchDomainObjectV1::MatchEvidence(object) => {
                    state.matches.insert(object_ref.key, object).is_some()
                }
                ResearchDomainObjectV1::EvaluationCommitment(object) => {
                    state.evaluations.insert(object_ref.key, object).is_some()
                }
                ResearchDomainObjectV1::WorkloadReceipt(object) => state
                    .workload_receipts
                    .insert(object_ref.key, object)
                    .is_some(),
                ResearchDomainObjectV1::ResearchClaim(object) => {
                    state.claims.insert(object_ref.key, object).is_some()
                }
                ResearchDomainObjectV1::LicenseDeclaration(object) => {
                    state.licenses.insert(object_ref.key, object).is_some()
                }
                ResearchDomainObjectV1::ClaimChallenge(object) => {
                    state.challenges.insert(object_ref.key, object).is_some()
                }
                ResearchDomainObjectV1::ClaimResolution(object) => {
                    state.resolutions.insert(object_ref.key, object).is_some()
                }
            };
            if duplicate {
                return Err(ProtocolStateError::DuplicateObject {
                    kind: object_ref.kind,
                    key: object_ref.key,
                });
            }
        }
        Ok(state)
    }

    pub fn apply(
        &mut self,
        signed: &SignedResearchCommandV1,
    ) -> Result<ApplyOutcome, ProtocolStateError> {
        Self::authorize(&self.authorities, signed)?;
        let fingerprint = signed.command_fingerprint();
        if let Some(existing) = self.applied_commands.get(&signed.command_id) {
            if existing.fingerprint == fingerprint {
                return Ok(ApplyOutcome::Idempotent {
                    primary_object_ref: existing.primary_object_ref,
                    changed_object_refs: Vec::new(),
                });
            }
            return Err(ProtocolStateError::AlteredReplay {
                command_id: signed.command_id,
            });
        }

        let primary_object_ref = match &signed.command {
            ResearchCommandV1::MatchEvidenceCommitment(payload) => self.apply_match(payload)?,
            ResearchCommandV1::EvaluationCommitment(payload) => self.apply_evaluation(payload)?,
            ResearchCommandV1::IssueWorkloadReceipt(payload) => self.apply_workload(payload)?,
            ResearchCommandV1::CreateResearchClaim(payload) => self.apply_claim(payload)?,
            ResearchCommandV1::DeclareLicense(payload) => self.apply_license(payload)?,
            ResearchCommandV1::ChallengeResearchClaim(payload) => self.apply_challenge(payload)?,
            ResearchCommandV1::ResolveResearchClaim(payload) => self.apply_resolution(payload)?,
        };

        self.applied_commands.insert(
            signed.command_id,
            AppliedCommandRecordV1 {
                command_id: signed.command_id,
                fingerprint,
                primary_object_ref,
            },
        );
        let changed_object_refs =
            self.changed_object_refs_for(&signed.command, primary_object_ref)?;
        Ok(ApplyOutcome::Applied {
            primary_object_ref,
            changed_object_refs,
        })
    }

    pub fn get_match(&self, key: ExternalKey) -> Option<&MatchEvidenceObjectV1> {
        self.matches.get(&key)
    }

    pub fn get_evaluation(&self, key: ExternalKey) -> Option<&EvaluationCommitmentObjectV1> {
        self.evaluations.get(&key)
    }

    pub fn get_workload_receipt(&self, key: ExternalKey) -> Option<&WorkloadReceiptObjectV1> {
        self.workload_receipts.get(&key)
    }

    pub fn get_claim(&self, key: ExternalKey) -> Option<&ResearchClaimObjectV1> {
        self.claims.get(&key)
    }

    pub fn get_license(&self, key: ExternalKey) -> Option<&LicenseDeclarationObjectV1> {
        self.licenses.get(&key)
    }

    pub fn get_challenge(&self, key: ExternalKey) -> Option<&ClaimChallengeObjectV1> {
        self.challenges.get(&key)
    }

    pub fn get_resolution(&self, key: ExternalKey) -> Option<&ClaimResolutionObjectV1> {
        self.resolutions.get(&key)
    }

    pub fn get_applied_command(&self, command_id: ExternalKey) -> Option<&AppliedCommandRecordV1> {
        self.applied_commands.get(&command_id)
    }

    pub fn export_snapshot(&self) -> ResearchProtocolSnapshotV1 {
        ResearchProtocolSnapshotV1 {
            protocol_version: PROTOCOL_VERSION,
            authorities: self.authorities.clone(),
            matches: self.matches.values().cloned().collect(),
            evaluations: self.evaluations.values().cloned().collect(),
            workload_receipts: self.workload_receipts.values().cloned().collect(),
            claims: self.claims.values().cloned().collect(),
            licenses: self.licenses.values().cloned().collect(),
            challenges: self.challenges.values().cloned().collect(),
            resolutions: self.resolutions.values().cloned().collect(),
            applied_commands: self.applied_commands.values().cloned().collect(),
        }
    }

    /// Deterministic consensus bytes for the complete, key-sorted state.
    pub fn canonical_snapshot_bytes(&self) -> Vec<u8> {
        self.export_snapshot().canonical_bytes()
    }

    pub fn canonical_snapshot_hash(&self) -> [u8; 32] {
        canonical_hash(
            "trnm-research-protocol-snapshot-v1",
            &self.canonical_snapshot_bytes(),
        )
    }

    /// All current object references in stable `(kind, key)` order.
    pub fn current_object_refs(&self) -> Vec<ObjectRefV1> {
        let mut refs = Vec::with_capacity(
            self.matches.len()
                + self.evaluations.len()
                + self.workload_receipts.len()
                + self.claims.len()
                + self.licenses.len()
                + self.challenges.len()
                + self.resolutions.len(),
        );
        refs.extend(self.matches.values().map(|object| object.object_ref));
        refs.extend(self.evaluations.values().map(|object| object.object_ref));
        refs.extend(
            self.workload_receipts
                .values()
                .map(|object| object.object_ref),
        );
        refs.extend(self.claims.values().map(|object| object.object_ref));
        refs.extend(self.licenses.values().map(|object| object.object_ref));
        refs.extend(self.challenges.values().map(|object| object.object_ref));
        refs.extend(self.resolutions.values().map(|object| object.object_ref));
        refs.sort();
        refs
    }

    /// Export a current object as deterministic consensus bytes. The supplied
    /// version must match the current version; stale refs fail closed.
    pub fn object_canonical_bytes(
        &self,
        object_ref: ObjectRefV1,
    ) -> Result<Vec<u8>, ProtocolStateError> {
        self.require_ref(object_ref)?;
        let bytes = match object_ref.kind {
            ResearchObjectKind::MatchEvidence => self
                .matches
                .get(&object_ref.key)
                .expect("reference already checked")
                .canonical_bytes(),
            ResearchObjectKind::EvaluationCommitment => self
                .evaluations
                .get(&object_ref.key)
                .expect("reference already checked")
                .canonical_bytes(),
            ResearchObjectKind::WorkloadReceipt => self
                .workload_receipts
                .get(&object_ref.key)
                .expect("reference already checked")
                .canonical_bytes(),
            ResearchObjectKind::ResearchClaim => self
                .claims
                .get(&object_ref.key)
                .expect("reference already checked")
                .canonical_bytes(),
            ResearchObjectKind::LicenseDeclaration => self
                .licenses
                .get(&object_ref.key)
                .expect("reference already checked")
                .canonical_bytes(),
            ResearchObjectKind::ClaimChallenge => self
                .challenges
                .get(&object_ref.key)
                .expect("reference already checked")
                .canonical_bytes(),
            ResearchObjectKind::ClaimResolution => self
                .resolutions
                .get(&object_ref.key)
                .expect("reference already checked")
                .canonical_bytes(),
        };
        Ok(bytes)
    }

    pub fn object_leaf_hash(
        &self,
        object_ref: ObjectRefV1,
    ) -> Result<[u8; 32], ProtocolStateError> {
        Ok(canonical_hash(
            "trnm-research-protocol-object-leaf-v1",
            &self.object_canonical_bytes(object_ref)?,
        ))
    }

    pub fn from_snapshot(snapshot: ResearchProtocolSnapshotV1) -> Result<Self, ProtocolStateError> {
        if snapshot.protocol_version != PROTOCOL_VERSION {
            return Err(ProtocolStateError::UnsupportedSnapshotVersion(
                snapshot.protocol_version,
            ));
        }
        snapshot.authorities.validate()?;
        let state = Self {
            authorities: snapshot.authorities,
            matches: collect_objects(
                snapshot.matches,
                ResearchObjectKind::MatchEvidence,
                |object| object.object_ref,
            )?,
            evaluations: collect_objects(
                snapshot.evaluations,
                ResearchObjectKind::EvaluationCommitment,
                |object| object.object_ref,
            )?,
            workload_receipts: collect_objects(
                snapshot.workload_receipts,
                ResearchObjectKind::WorkloadReceipt,
                |object| object.object_ref,
            )?,
            claims: collect_objects(
                snapshot.claims,
                ResearchObjectKind::ResearchClaim,
                |object| object.object_ref,
            )?,
            licenses: collect_objects(
                snapshot.licenses,
                ResearchObjectKind::LicenseDeclaration,
                |object| object.object_ref,
            )?,
            challenges: collect_objects(
                snapshot.challenges,
                ResearchObjectKind::ClaimChallenge,
                |object| object.object_ref,
            )?,
            resolutions: collect_objects(
                snapshot.resolutions,
                ResearchObjectKind::ClaimResolution,
                |object| object.object_ref,
            )?,
            applied_commands: collect_command_records(snapshot.applied_commands)?,
        };
        state.validate_snapshot_graph()?;
        Ok(state)
    }

    fn changed_object_refs_for(
        &self,
        command: &ResearchCommandV1,
        primary_object_ref: ObjectRefV1,
    ) -> Result<Vec<ObjectRefV1>, ProtocolStateError> {
        let mut refs = vec![primary_object_ref];
        match command {
            ResearchCommandV1::ChallengeResearchClaim(payload) => {
                let claim = self.claims.get(&payload.claim_ref.key).ok_or(
                    ProtocolStateError::MissingReferencedObject(payload.claim_ref),
                )?;
                refs.push(claim.object_ref);
            }
            ResearchCommandV1::ResolveResearchClaim(payload) => {
                let challenge = self.challenges.get(&payload.challenge_ref.key).ok_or(
                    ProtocolStateError::MissingReferencedObject(payload.challenge_ref),
                )?;
                refs.push(challenge.object_ref);
                let claim = self.claims.get(&challenge.challenge.claim_ref.key).ok_or(
                    ProtocolStateError::MissingReferencedObject(challenge.challenge.claim_ref),
                )?;
                refs.push(claim.object_ref);
            }
            _ => {}
        }
        refs.sort();
        Ok(refs)
    }

    fn apply_match(
        &mut self,
        payload: &MatchEvidenceCommitmentV1,
    ) -> Result<ObjectRefV1, ProtocolStateError> {
        let object_ref = payload.object_ref();
        ensure_absent(&self.matches, object_ref.kind, payload.commitment_id)?;
        self.matches.insert(
            payload.commitment_id,
            MatchEvidenceObjectV1 {
                object_ref,
                commitment: payload.clone(),
            },
        );
        Ok(object_ref)
    }

    fn apply_evaluation(
        &mut self,
        payload: &EvaluationCommitmentV1,
    ) -> Result<ObjectRefV1, ProtocolStateError> {
        self.require_ref(payload.match_evidence_ref)?;
        let match_object = self
            .matches
            .get(&payload.match_evidence_ref.key)
            .expect("reference check guarantees match object");
        if payload.completed_at_unix_s < match_object.commitment.completed_at_unix_s {
            return Err(ProtocolStateError::TimestampRegression);
        }
        let object_ref = payload.object_ref();
        ensure_absent(&self.evaluations, object_ref.kind, payload.evaluation_id)?;
        self.evaluations.insert(
            payload.evaluation_id,
            EvaluationCommitmentObjectV1 {
                object_ref,
                commitment: payload.clone(),
            },
        );
        Ok(object_ref)
    }

    fn apply_workload(
        &mut self,
        payload: &IssueWorkloadReceiptV1,
    ) -> Result<ObjectRefV1, ProtocolStateError> {
        self.require_ref(payload.evaluation_ref)?;
        let evaluation = self
            .evaluations
            .get(&payload.evaluation_ref.key)
            .expect("reference check guarantees evaluation object");
        if !evaluation.commitment.accepted {
            return Err(ProtocolStateError::RejectedEvaluationCannotIssueWorkload);
        }
        if payload.issued_at_unix_s < evaluation.commitment.completed_at_unix_s {
            return Err(ProtocolStateError::TimestampRegression);
        }
        let object_ref = payload.object_ref();
        ensure_absent(&self.workload_receipts, object_ref.kind, payload.receipt_id)?;
        self.workload_receipts.insert(
            payload.receipt_id,
            WorkloadReceiptObjectV1 {
                object_ref,
                receipt: payload.clone(),
            },
        );
        Ok(object_ref)
    }

    fn apply_claim(
        &mut self,
        payload: &CreateResearchClaimV1,
    ) -> Result<ObjectRefV1, ProtocolStateError> {
        self.require_ref(payload.workload_receipt_ref)?;
        for evidence_ref in &payload.evidence_refs {
            self.require_ref(*evidence_ref)?;
        }
        let workload = self
            .workload_receipts
            .get(&payload.workload_receipt_ref.key)
            .expect("reference check guarantees workload receipt");
        let contributor_keys: BTreeSet<_> = workload
            .receipt
            .contributors
            .iter()
            .map(|entry| entry.contributor)
            .collect();
        if payload
            .claimants
            .iter()
            .any(|share| !contributor_keys.contains(&share.contributor))
        {
            return Err(ProtocolStateError::ClaimantWithoutAcceptedWork);
        }
        if !payload
            .evidence_refs
            .contains(&workload.receipt.evaluation_ref)
        {
            return Err(ProtocolStateError::ClaimMissingWorkloadEvaluation);
        }
        if payload.created_at_unix_s < workload.receipt.issued_at_unix_s {
            return Err(ProtocolStateError::TimestampRegression);
        }
        let object_ref = payload.object_ref();
        ensure_absent(&self.claims, object_ref.kind, payload.claim_id)?;
        self.claims.insert(
            payload.claim_id,
            ResearchClaimObjectV1 {
                object_ref,
                claim: payload.clone(),
                current_claimants: payload.claimants.clone(),
                status: ClaimStatus::Active,
                active_challenge: None,
            },
        );
        Ok(object_ref)
    }

    fn apply_license(
        &mut self,
        payload: &DeclareLicenseV1,
    ) -> Result<ObjectRefV1, ProtocolStateError> {
        self.require_ref(payload.claim_ref)?;
        let claim = self
            .claims
            .get(&payload.claim_ref.key)
            .expect("reference check guarantees claim object");
        if matches!(
            claim.status,
            ClaimStatus::Challenged | ClaimStatus::Rejected
        ) {
            return Err(ProtocolStateError::ClaimNotLicensable(claim.status));
        }
        if !claim
            .current_claimants
            .iter()
            .any(|share| share.contributor == payload.licensor)
        {
            return Err(ProtocolStateError::LicensorIsNotClaimant);
        }
        if payload.effective_at_unix_s < claim.claim.created_at_unix_s {
            return Err(ProtocolStateError::TimestampRegression);
        }
        let object_ref = payload.object_ref();
        ensure_absent(&self.licenses, object_ref.kind, payload.declaration_id)?;
        self.licenses.insert(
            payload.declaration_id,
            LicenseDeclarationObjectV1 {
                object_ref,
                declaration: payload.clone(),
            },
        );
        Ok(object_ref)
    }

    fn apply_challenge(
        &mut self,
        payload: &crate::types::ChallengeResearchClaimV1,
    ) -> Result<ObjectRefV1, ProtocolStateError> {
        self.require_ref(payload.claim_ref)?;
        let claim = self
            .claims
            .get(&payload.claim_ref.key)
            .expect("reference check guarantees claim object");
        if !matches!(
            claim.status,
            ClaimStatus::Active | ClaimStatus::Amended | ClaimStatus::LicenseAmendmentRequired
        ) {
            return Err(ProtocolStateError::ClaimNotChallengeable(claim.status));
        }
        if payload.opened_at_unix_s < claim.claim.created_at_unix_s {
            return Err(ProtocolStateError::TimestampRegression);
        }
        let object_ref = payload.object_ref();
        ensure_absent(&self.challenges, object_ref.kind, payload.challenge_id)?;

        self.challenges.insert(
            payload.challenge_id,
            ClaimChallengeObjectV1 {
                object_ref,
                challenge: payload.clone(),
                status: ClaimChallengeStatus::Open,
                resolution_ref: None,
            },
        );
        let claim = self
            .claims
            .get_mut(&payload.claim_ref.key)
            .expect("reference check guarantees claim object");
        claim.object_ref.object_version += 1;
        claim.status = ClaimStatus::Challenged;
        claim.active_challenge = Some(payload.challenge_id);
        Ok(object_ref)
    }

    fn apply_resolution(
        &mut self,
        payload: &ClaimResolutionV1,
    ) -> Result<ObjectRefV1, ProtocolStateError> {
        self.require_ref(payload.challenge_ref)?;
        let challenge = self
            .challenges
            .get(&payload.challenge_ref.key)
            .expect("reference check guarantees challenge object");
        if challenge.status != ClaimChallengeStatus::Open {
            return Err(ProtocolStateError::ChallengeAlreadyResolved);
        }
        if payload.decided_at_unix_s < challenge.challenge.opened_at_unix_s {
            return Err(ProtocolStateError::TimestampRegression);
        }
        let claim_key = challenge.challenge.claim_ref.key;
        let claim =
            self.claims
                .get(&claim_key)
                .ok_or(ProtocolStateError::MissingReferencedObject(
                    challenge.challenge.claim_ref,
                ))?;
        if claim.status != ClaimStatus::Challenged
            || claim.active_challenge != Some(payload.challenge_ref.key)
        {
            return Err(ProtocolStateError::ChallengeClaimStateMismatch);
        }
        if payload.decision == ClaimResolutionDecision::AmendContributorShares {
            let workload = self
                .workload_receipts
                .get(&claim.claim.workload_receipt_ref.key)
                .ok_or(ProtocolStateError::MissingReferencedObject(
                    claim.claim.workload_receipt_ref,
                ))?;
            let contributor_keys: BTreeSet<_> = workload
                .receipt
                .contributors
                .iter()
                .map(|entry| entry.contributor)
                .collect();
            if payload
                .amended_claimants
                .iter()
                .any(|share| !contributor_keys.contains(&share.contributor))
            {
                return Err(ProtocolStateError::ClaimantWithoutAcceptedWork);
            }
        }

        let object_ref = payload.object_ref();
        ensure_absent(&self.resolutions, object_ref.kind, payload.resolution_id)?;
        self.resolutions.insert(
            payload.resolution_id,
            ClaimResolutionObjectV1 {
                object_ref,
                resolution: payload.clone(),
            },
        );

        let challenge = self
            .challenges
            .get_mut(&payload.challenge_ref.key)
            .expect("reference check guarantees challenge object");
        challenge.object_ref.object_version += 1;
        challenge.status = ClaimChallengeStatus::Resolved;
        challenge.resolution_ref = Some(object_ref);

        let claim = self
            .claims
            .get_mut(&claim_key)
            .expect("challenge graph guarantees claim object");
        claim.object_ref.object_version += 1;
        claim.active_challenge = None;
        match payload.decision {
            ClaimResolutionDecision::Uphold => claim.status = ClaimStatus::Active,
            ClaimResolutionDecision::Reject => claim.status = ClaimStatus::Rejected,
            ClaimResolutionDecision::AmendContributorShares => {
                claim.current_claimants = payload.amended_claimants.clone();
                claim.status = ClaimStatus::Amended;
            }
            ClaimResolutionDecision::RequireLicenseAmendment => {
                claim.status = ClaimStatus::LicenseAmendmentRequired;
            }
        }
        Ok(object_ref)
    }

    fn require_ref(&self, object_ref: ObjectRefV1) -> Result<(), ProtocolStateError> {
        let actual_version = match object_ref.kind {
            ResearchObjectKind::MatchEvidence => self
                .matches
                .get(&object_ref.key)
                .map(|object| object.object_ref.object_version),
            ResearchObjectKind::EvaluationCommitment => self
                .evaluations
                .get(&object_ref.key)
                .map(|object| object.object_ref.object_version),
            ResearchObjectKind::WorkloadReceipt => self
                .workload_receipts
                .get(&object_ref.key)
                .map(|object| object.object_ref.object_version),
            ResearchObjectKind::ResearchClaim => self
                .claims
                .get(&object_ref.key)
                .map(|object| object.object_ref.object_version),
            ResearchObjectKind::LicenseDeclaration => self
                .licenses
                .get(&object_ref.key)
                .map(|object| object.object_ref.object_version),
            ResearchObjectKind::ClaimChallenge => self
                .challenges
                .get(&object_ref.key)
                .map(|object| object.object_ref.object_version),
            ResearchObjectKind::ClaimResolution => self
                .resolutions
                .get(&object_ref.key)
                .map(|object| object.object_ref.object_version),
        };
        match actual_version {
            None => Err(ProtocolStateError::MissingReferencedObject(object_ref)),
            Some(actual) if actual != object_ref.object_version => {
                Err(ProtocolStateError::ObjectVersionMismatch { object_ref, actual })
            }
            Some(_) => Ok(()),
        }
    }

    fn validate_snapshot_graph(&self) -> Result<(), ProtocolStateError> {
        self.authorities.validate()?;
        for object in self.matches.values() {
            object.commitment.validate()?;
            validate_snapshot_object_ref(
                object.object_ref,
                ResearchObjectKind::MatchEvidence,
                object.commitment.commitment_id,
            )?;
            if object.object_ref.object_version != 1 {
                return Err(ProtocolStateError::InvalidSnapshotGraph);
            }
        }
        for object in self.evaluations.values() {
            object.commitment.validate()?;
            validate_snapshot_object_ref(
                object.object_ref,
                ResearchObjectKind::EvaluationCommitment,
                object.commitment.evaluation_id,
            )?;
            if object.object_ref.object_version != 1 {
                return Err(ProtocolStateError::InvalidSnapshotGraph);
            }
            self.require_ref(object.commitment.match_evidence_ref)?;
        }
        for object in self.workload_receipts.values() {
            object.receipt.validate()?;
            validate_snapshot_object_ref(
                object.object_ref,
                ResearchObjectKind::WorkloadReceipt,
                object.receipt.receipt_id,
            )?;
            if object.object_ref.object_version != 1 {
                return Err(ProtocolStateError::InvalidSnapshotGraph);
            }
            self.require_ref(object.receipt.evaluation_ref)?;
            if !self
                .evaluations
                .get(&object.receipt.evaluation_ref.key)
                .is_some_and(|evaluation| evaluation.commitment.accepted)
            {
                return Err(ProtocolStateError::InvalidSnapshotGraph);
            }
        }
        for object in self.claims.values() {
            object.claim.validate()?;
            validate_snapshot_object_ref(
                object.object_ref,
                ResearchObjectKind::ResearchClaim,
                object.claim.claim_id,
            )?;
            validate_claim_shares(&object.current_claimants, false)?;
            self.require_ref(object.claim.workload_receipt_ref)?;
            let challenge_consistent = match object.status {
                ClaimStatus::Challenged => object.active_challenge.is_some(),
                _ => object.active_challenge.is_none(),
            };
            if !challenge_consistent {
                return Err(ProtocolStateError::InvalidSnapshotGraph);
            }
        }
        for object in self.licenses.values() {
            object.declaration.validate()?;
            validate_snapshot_object_ref(
                object.object_ref,
                ResearchObjectKind::LicenseDeclaration,
                object.declaration.declaration_id,
            )?;
            if object.object_ref.object_version != 1 {
                return Err(ProtocolStateError::InvalidSnapshotGraph);
            }
            if !self.claims.contains_key(&object.declaration.claim_ref.key) {
                return Err(ProtocolStateError::InvalidSnapshotGraph);
            }
        }
        for object in self.challenges.values() {
            object.challenge.validate()?;
            validate_snapshot_object_ref(
                object.object_ref,
                ResearchObjectKind::ClaimChallenge,
                object.challenge.challenge_id,
            )?;
            if !self.claims.contains_key(&object.challenge.claim_ref.key) {
                return Err(ProtocolStateError::InvalidSnapshotGraph);
            }
            let status_consistent = match object.status {
                ClaimChallengeStatus::Open => {
                    object.object_ref.object_version == 1 && object.resolution_ref.is_none()
                }
                ClaimChallengeStatus::Resolved => {
                    object.object_ref.object_version == 2 && object.resolution_ref.is_some()
                }
            };
            if !status_consistent {
                return Err(ProtocolStateError::InvalidSnapshotGraph);
            }
        }
        for object in self.resolutions.values() {
            object.resolution.validate()?;
            validate_snapshot_object_ref(
                object.object_ref,
                ResearchObjectKind::ClaimResolution,
                object.resolution.resolution_id,
            )?;
            if object.object_ref.object_version != 1 {
                return Err(ProtocolStateError::InvalidSnapshotGraph);
            }
            if !self
                .challenges
                .contains_key(&object.resolution.challenge_ref.key)
            {
                return Err(ProtocolStateError::InvalidSnapshotGraph);
            }
        }
        for record in self.applied_commands.values() {
            if record.fingerprint == [0; 32] {
                return Err(ProtocolStateError::InvalidSnapshotGraph);
            }
            self.require_ref_relaxed(record.primary_object_ref)?;
        }
        Ok(())
    }

    fn require_ref_relaxed(&self, object_ref: ObjectRefV1) -> Result<(), ProtocolStateError> {
        let exists = match object_ref.kind {
            ResearchObjectKind::MatchEvidence => self.matches.contains_key(&object_ref.key),
            ResearchObjectKind::EvaluationCommitment => {
                self.evaluations.contains_key(&object_ref.key)
            }
            ResearchObjectKind::WorkloadReceipt => {
                self.workload_receipts.contains_key(&object_ref.key)
            }
            ResearchObjectKind::ResearchClaim => self.claims.contains_key(&object_ref.key),
            ResearchObjectKind::LicenseDeclaration => self.licenses.contains_key(&object_ref.key),
            ResearchObjectKind::ClaimChallenge => self.challenges.contains_key(&object_ref.key),
            ResearchObjectKind::ClaimResolution => self.resolutions.contains_key(&object_ref.key),
        };
        if exists {
            Ok(())
        } else {
            Err(ProtocolStateError::MissingReferencedObject(object_ref))
        }
    }
}

fn validate_snapshot_object_ref(
    object_ref: ObjectRefV1,
    expected_kind: ResearchObjectKind,
    expected_key: ExternalKey,
) -> Result<(), ProtocolStateError> {
    if object_ref.kind != expected_kind
        || object_ref.key != expected_key
        || object_ref.object_version == 0
    {
        return Err(ProtocolStateError::InvalidSnapshotGraph);
    }
    Ok(())
}

fn ensure_absent<T>(
    map: &BTreeMap<ExternalKey, T>,
    kind: ResearchObjectKind,
    key: ExternalKey,
) -> Result<(), ProtocolStateError> {
    if map.contains_key(&key) {
        return Err(ProtocolStateError::DuplicateObject { kind, key });
    }
    Ok(())
}

fn collect_objects<T>(
    objects: Vec<T>,
    expected_kind: ResearchObjectKind,
    object_ref: impl Fn(&T) -> ObjectRefV1,
) -> Result<BTreeMap<ExternalKey, T>, ProtocolStateError> {
    let mut map = BTreeMap::new();
    for object in objects {
        let reference = object_ref(&object);
        if reference.kind != expected_kind || reference.object_version == 0 {
            return Err(ProtocolStateError::InvalidSnapshotGraph);
        }
        if map.insert(reference.key, object).is_some() {
            return Err(ProtocolStateError::DuplicateObject {
                kind: expected_kind,
                key: reference.key,
            });
        }
    }
    Ok(map)
}

fn collect_command_records(
    records: Vec<AppliedCommandRecordV1>,
) -> Result<BTreeMap<ExternalKey, AppliedCommandRecordV1>, ProtocolStateError> {
    let mut map = BTreeMap::new();
    for record in records {
        if record.command_id.as_bytes() == &[0; 32]
            || map.insert(record.command_id, record.clone()).is_some()
        {
            return Err(ProtocolStateError::InvalidSnapshotGraph);
        }
    }
    Ok(map)
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProtocolStateError {
    #[error(transparent)]
    InvalidCanonicalObject(#[from] CanonicalDecodeError),
    #[error(transparent)]
    InvalidCommand(#[from] SignedResearchCommandValidationError),
    #[error(transparent)]
    InvalidPayload(#[from] crate::types::ResearchPayloadValidationError),
    #[error("command id {command_id} was replayed with altered signed bytes")]
    AlteredReplay { command_id: ExternalKey },
    #[error("invalid or non-canonically ordered research authority set")]
    InvalidAuthoritySet,
    #[error("{role:?} signer {signer_did} is not in the genesis authority set")]
    UnauthorizedAuthority {
        signer_did: String,
        role: AuthorityRole,
        public_key: [u8; 32],
    },
    #[error("duplicate {kind:?} object {key}")]
    DuplicateObject {
        kind: ResearchObjectKind,
        key: ExternalKey,
    },
    #[error("missing referenced object {0:?}")]
    MissingReferencedObject(ObjectRefV1),
    #[error("object version mismatch for {object_ref:?}: actual version is {actual}")]
    ObjectVersionMismatch {
        object_ref: ObjectRefV1,
        actual: u64,
    },
    #[error("rejected evaluations cannot issue accepted workload")]
    RejectedEvaluationCannotIssueWorkload,
    #[error("claimant has no accepted work in the referenced workload receipt")]
    ClaimantWithoutAcceptedWork,
    #[error("research claim must include its workload evaluation in evidence_refs")]
    ClaimMissingWorkloadEvaluation,
    #[error("licensor is not a claimant")]
    LicensorIsNotClaimant,
    #[error("claim in status {0:?} cannot be licensed")]
    ClaimNotLicensable(ClaimStatus),
    #[error("claim in status {0:?} cannot be challenged")]
    ClaimNotChallengeable(ClaimStatus),
    #[error("challenge was already resolved")]
    ChallengeAlreadyResolved,
    #[error("challenge and claim state disagree")]
    ChallengeClaimStateMismatch,
    #[error("timestamp regresses relative to a referenced object")]
    TimestampRegression,
    #[error("unsupported research snapshot version {0}")]
    UnsupportedSnapshotVersion(u8),
    #[error("snapshot object graph violates protocol invariants")]
    InvalidSnapshotGraph,
}
