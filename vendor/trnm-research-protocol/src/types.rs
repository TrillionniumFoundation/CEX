use crate::canonical::{
    encode_option_digest, CanonicalCbor, CanonicalDecodeError, Decoder, Encoder,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use thiserror::Error;

pub const PROTOCOL_VERSION: u8 = 1;
pub type Digest32 = [u8; 32];

const EXTERNAL_KEY_DOMAIN: &[u8] = b"TRNM_RESEARCH_EXTERNAL_KEY_V1\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExternalKey(Digest32);

impl ExternalKey {
    pub fn from_bytes(bytes: Digest32) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &Digest32 {
        &self.0
    }

    pub fn into_bytes(self) -> Digest32 {
        self.0
    }

    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }

    /// Derive a stable key from a canonical lowercase namespace and a strict
    /// canonical UUID string. The UUID is hashed as its 16 raw bytes, so no
    /// adapter-side UUID-to-integer table is required.
    pub fn from_uuid(namespace: &str, uuid: &str) -> Result<Self, ExternalKeyError> {
        validate_namespace(namespace)?;
        let raw = parse_canonical_uuid(uuid)?;
        Ok(Self(derive_external_key(namespace, 1, &raw)))
    }

    /// Derive a stable key from a canonical external identifier.
    ///
    /// External identifiers are deliberately restricted to visible ASCII with
    /// no surrounding whitespace. Producers needing Unicode identifiers must
    /// canonicalize them outside consensus and pass a UUID or digest-derived
    /// ASCII identifier.
    pub fn from_external_id(namespace: &str, external_id: &str) -> Result<Self, ExternalKeyError> {
        validate_namespace(namespace)?;
        if external_id.is_empty() || external_id.len() > 256 {
            return Err(ExternalKeyError::InvalidExternalId);
        }
        if !external_id
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && byte != b'\\')
        {
            return Err(ExternalKeyError::InvalidExternalId);
        }
        Ok(Self(derive_external_key(
            namespace,
            2,
            external_id.as_bytes(),
        )))
    }

    pub(crate) fn validate(
        self,
        field: &'static str,
    ) -> Result<(), ResearchPayloadValidationError> {
        if self.0 == [0; 32] {
            return Err(ResearchPayloadValidationError::ZeroExternalKey(field));
        }
        Ok(())
    }
}

impl fmt::Display for ExternalKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&hex::encode(self.0))
    }
}

impl CanonicalCbor for ExternalKey {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.bytes(&self.0);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExternalKeyError {
    #[error("namespace must be 1..64 canonical lowercase ASCII characters")]
    InvalidNamespace,
    #[error("UUID must use canonical lowercase 8-4-4-4-12 form")]
    InvalidUuid,
    #[error("external id must be 1..256 visible canonical ASCII characters")]
    InvalidExternalId,
}

fn validate_namespace(namespace: &str) -> Result<(), ExternalKeyError> {
    if namespace.is_empty()
        || namespace.len() > 64
        || !namespace.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'-' | b'_' | b':')
        })
    {
        return Err(ExternalKeyError::InvalidNamespace);
    }
    Ok(())
}

fn parse_canonical_uuid(uuid: &str) -> Result<[u8; 16], ExternalKeyError> {
    if uuid.len() != 36
        || uuid.as_bytes().get(8) != Some(&b'-')
        || uuid.as_bytes().get(13) != Some(&b'-')
        || uuid.as_bytes().get(18) != Some(&b'-')
        || uuid.as_bytes().get(23) != Some(&b'-')
    {
        return Err(ExternalKeyError::InvalidUuid);
    }

    let mut compact = [0u8; 32];
    let mut cursor = 0;
    for byte in uuid.bytes() {
        if byte == b'-' {
            continue;
        }
        if !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte) {
            return Err(ExternalKeyError::InvalidUuid);
        }
        compact[cursor] = byte;
        cursor += 1;
    }
    if cursor != 32 {
        return Err(ExternalKeyError::InvalidUuid);
    }

    let decoded = hex::decode(compact).map_err(|_| ExternalKeyError::InvalidUuid)?;
    decoded
        .try_into()
        .map_err(|_| ExternalKeyError::InvalidUuid)
}

fn derive_external_key(namespace: &str, id_kind: u8, external_id: &[u8]) -> Digest32 {
    let mut hasher = Sha256::new();
    hasher.update(EXTERNAL_KEY_DOMAIN);
    hasher.update((namespace.len() as u16).to_be_bytes());
    hasher.update(namespace.as_bytes());
    hasher.update([id_kind]);
    hasher.update((external_id.len() as u16).to_be_bytes());
    hasher.update(external_id);
    hasher.finalize().into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum ResearchObjectKind {
    MatchEvidence = 1,
    EvaluationCommitment = 2,
    WorkloadReceipt = 3,
    ResearchClaim = 4,
    LicenseDeclaration = 5,
    ClaimChallenge = 6,
    ClaimResolution = 7,
}

impl CanonicalCbor for ResearchObjectKind {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.uint(*self as u64);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ObjectRefV1 {
    pub kind: ResearchObjectKind,
    pub key: ExternalKey,
    pub object_version: u64,
}

impl ObjectRefV1 {
    pub fn new(kind: ResearchObjectKind, key: ExternalKey, object_version: u64) -> Self {
        Self {
            kind,
            key,
            object_version,
        }
    }

    pub(crate) fn validate(
        self,
        field: &'static str,
    ) -> Result<(), ResearchPayloadValidationError> {
        self.key.validate(field)?;
        if self.object_version == 0 {
            return Err(ResearchPayloadValidationError::ZeroObjectVersion(field));
        }
        Ok(())
    }
}

impl CanonicalCbor for ObjectRefV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(4);
        encoder.uint(PROTOCOL_VERSION as u64);
        self.kind.encode_canonical(encoder);
        self.key.encode_canonical(encoder);
        encoder.uint(self.object_version);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchEvidenceCommitmentV1 {
    pub commitment_id: ExternalKey,
    pub match_id: ExternalKey,
    pub challenge_id: ExternalKey,
    pub event_root: Digest32,
    pub roster_root: Digest32,
    pub ruleset_hash: Digest32,
    pub dataset_hash: Digest32,
    pub archive_hash: Digest32,
    pub event_count: u64,
    pub completed_at_unix_s: u64,
}

impl MatchEvidenceCommitmentV1 {
    pub fn validate(&self) -> Result<(), ResearchPayloadValidationError> {
        self.commitment_id.validate("commitment_id")?;
        self.match_id.validate("match_id")?;
        self.challenge_id.validate("challenge_id")?;
        validate_digest("event_root", &self.event_root)?;
        validate_digest("roster_root", &self.roster_root)?;
        validate_digest("ruleset_hash", &self.ruleset_hash)?;
        validate_digest("dataset_hash", &self.dataset_hash)?;
        validate_digest("archive_hash", &self.archive_hash)?;
        validate_positive("event_count", self.event_count)?;
        validate_timestamp(self.completed_at_unix_s)
    }

    pub fn object_ref(&self) -> ObjectRefV1 {
        ObjectRefV1::new(ResearchObjectKind::MatchEvidence, self.commitment_id, 1)
    }
}

impl CanonicalCbor for MatchEvidenceCommitmentV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(11);
        encoder.uint(PROTOCOL_VERSION as u64);
        self.commitment_id.encode_canonical(encoder);
        self.match_id.encode_canonical(encoder);
        self.challenge_id.encode_canonical(encoder);
        encoder.bytes(&self.event_root);
        encoder.bytes(&self.roster_root);
        encoder.bytes(&self.ruleset_hash);
        encoder.bytes(&self.dataset_hash);
        encoder.bytes(&self.archive_hash);
        encoder.uint(self.event_count);
        encoder.uint(self.completed_at_unix_s);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationCommitmentV1 {
    pub evaluation_id: ExternalKey,
    pub match_evidence_ref: ObjectRefV1,
    pub submission_hash: Digest32,
    pub rubric_hash: Digest32,
    pub evaluation_hash: Digest32,
    pub reproduction_hash: Option<Digest32>,
    pub score_bps: u16,
    pub accepted: bool,
    pub completed_at_unix_s: u64,
}

impl EvaluationCommitmentV1 {
    pub fn validate(&self) -> Result<(), ResearchPayloadValidationError> {
        self.evaluation_id.validate("evaluation_id")?;
        validate_ref_kind(
            "match_evidence_ref",
            self.match_evidence_ref,
            ResearchObjectKind::MatchEvidence,
        )?;
        validate_digest("submission_hash", &self.submission_hash)?;
        validate_digest("rubric_hash", &self.rubric_hash)?;
        validate_digest("evaluation_hash", &self.evaluation_hash)?;
        if let Some(hash) = &self.reproduction_hash {
            validate_digest("reproduction_hash", hash)?;
        }
        if self.score_bps > 10_000 {
            return Err(ResearchPayloadValidationError::BasisPointsOutOfRange(
                "score_bps",
            ));
        }
        if self.accepted && self.score_bps == 0 {
            return Err(ResearchPayloadValidationError::InconsistentEvaluation);
        }
        validate_timestamp(self.completed_at_unix_s)
    }

    pub fn object_ref(&self) -> ObjectRefV1 {
        ObjectRefV1::new(
            ResearchObjectKind::EvaluationCommitment,
            self.evaluation_id,
            1,
        )
    }
}

impl CanonicalCbor for EvaluationCommitmentV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(10);
        encoder.uint(PROTOCOL_VERSION as u64);
        self.evaluation_id.encode_canonical(encoder);
        self.match_evidence_ref.encode_canonical(encoder);
        encoder.bytes(&self.submission_hash);
        encoder.bytes(&self.rubric_hash);
        encoder.bytes(&self.evaluation_hash);
        encode_option_digest(encoder, &self.reproduction_hash);
        encoder.uint(self.score_bps as u64);
        encoder.bool(self.accepted);
        encoder.uint(self.completed_at_unix_s);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum ContributionRole {
    Researcher = 1,
    DataProvider = 2,
    Evaluator = 3,
    Reproducer = 4,
    AgentOperator = 5,
}

impl CanonicalCbor for ContributionRole {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.uint(*self as u64);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContributorWorkV1 {
    pub contributor: ExternalKey,
    pub role: ContributionRole,
    pub accepted_work_units: u64,
    pub contribution_hash: Digest32,
}

impl ContributorWorkV1 {
    fn validate(&self) -> Result<(), ResearchPayloadValidationError> {
        self.contributor.validate("contributor")?;
        validate_positive("accepted_work_units", self.accepted_work_units)?;
        validate_digest("contribution_hash", &self.contribution_hash)
    }
}

impl CanonicalCbor for ContributorWorkV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(5);
        encoder.uint(PROTOCOL_VERSION as u64);
        self.contributor.encode_canonical(encoder);
        self.role.encode_canonical(encoder);
        encoder.uint(self.accepted_work_units);
        encoder.bytes(&self.contribution_hash);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueWorkloadReceiptV1 {
    pub receipt_id: ExternalKey,
    pub evaluation_ref: ObjectRefV1,
    pub contributors: Vec<ContributorWorkV1>,
    pub total_accepted_work_units: u64,
    pub policy_hash: Digest32,
    pub issued_at_unix_s: u64,
}

impl IssueWorkloadReceiptV1 {
    pub fn validate(&self) -> Result<(), ResearchPayloadValidationError> {
        self.receipt_id.validate("receipt_id")?;
        validate_ref_kind(
            "evaluation_ref",
            self.evaluation_ref,
            ResearchObjectKind::EvaluationCommitment,
        )?;
        if self.contributors.is_empty() || self.contributors.len() > 128 {
            return Err(ResearchPayloadValidationError::InvalidCollectionSize(
                "contributors",
            ));
        }
        let mut total = 0u64;
        let mut previous = None;
        for contribution in &self.contributors {
            contribution.validate()?;
            if previous.is_some_and(|key| key >= contribution.contributor) {
                return Err(ResearchPayloadValidationError::NonCanonicalOrdering(
                    "contributors",
                ));
            }
            previous = Some(contribution.contributor);
            total = total
                .checked_add(contribution.accepted_work_units)
                .ok_or(ResearchPayloadValidationError::WorkUnitOverflow)?;
        }
        if total == 0 || total != self.total_accepted_work_units {
            return Err(ResearchPayloadValidationError::WorkUnitTotalMismatch {
                declared: self.total_accepted_work_units,
                computed: total,
            });
        }
        validate_digest("policy_hash", &self.policy_hash)?;
        validate_timestamp(self.issued_at_unix_s)
    }

    pub fn object_ref(&self) -> ObjectRefV1 {
        ObjectRefV1::new(ResearchObjectKind::WorkloadReceipt, self.receipt_id, 1)
    }
}

impl CanonicalCbor for IssueWorkloadReceiptV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(7);
        encoder.uint(PROTOCOL_VERSION as u64);
        self.receipt_id.encode_canonical(encoder);
        self.evaluation_ref.encode_canonical(encoder);
        encoder.array(self.contributors.len());
        for contributor in &self.contributors {
            contributor.encode_canonical(encoder);
        }
        encoder.uint(self.total_accepted_work_units);
        encoder.bytes(&self.policy_hash);
        encoder.uint(self.issued_at_unix_s);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimShareV1 {
    pub contributor: ExternalKey,
    pub share_bps: u16,
}

impl ClaimShareV1 {
    fn validate(&self) -> Result<(), ResearchPayloadValidationError> {
        self.contributor.validate("claimant")?;
        if self.share_bps == 0 || self.share_bps > 10_000 {
            return Err(ResearchPayloadValidationError::BasisPointsOutOfRange(
                "share_bps",
            ));
        }
        Ok(())
    }
}

impl CanonicalCbor for ClaimShareV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(3);
        encoder.uint(PROTOCOL_VERSION as u64);
        self.contributor.encode_canonical(encoder);
        encoder.uint(self.share_bps as u64);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateResearchClaimV1 {
    pub claim_id: ExternalKey,
    pub workload_receipt_ref: ObjectRefV1,
    pub evidence_refs: Vec<ObjectRefV1>,
    pub artifact_hash: Digest32,
    pub claim_scope_hash: Digest32,
    pub claimants: Vec<ClaimShareV1>,
    pub created_at_unix_s: u64,
}

impl CreateResearchClaimV1 {
    pub fn validate(&self) -> Result<(), ResearchPayloadValidationError> {
        self.claim_id.validate("claim_id")?;
        validate_ref_kind(
            "workload_receipt_ref",
            self.workload_receipt_ref,
            ResearchObjectKind::WorkloadReceipt,
        )?;
        validate_evidence_refs(&self.evidence_refs)?;
        validate_digest("artifact_hash", &self.artifact_hash)?;
        validate_digest("claim_scope_hash", &self.claim_scope_hash)?;
        validate_claim_shares(&self.claimants, false)?;
        validate_timestamp(self.created_at_unix_s)
    }

    pub fn object_ref(&self) -> ObjectRefV1 {
        ObjectRefV1::new(ResearchObjectKind::ResearchClaim, self.claim_id, 1)
    }
}

impl CanonicalCbor for CreateResearchClaimV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(8);
        encoder.uint(PROTOCOL_VERSION as u64);
        self.claim_id.encode_canonical(encoder);
        self.workload_receipt_ref.encode_canonical(encoder);
        encoder.array(self.evidence_refs.len());
        for evidence_ref in &self.evidence_refs {
            evidence_ref.encode_canonical(encoder);
        }
        encoder.bytes(&self.artifact_hash);
        encoder.bytes(&self.claim_scope_hash);
        encoder.array(self.claimants.len());
        for claimant in &self.claimants {
            claimant.encode_canonical(encoder);
        }
        encoder.uint(self.created_at_unix_s);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum LicenseScope {
    Artifact = 1,
    Dataset = 2,
    Method = 3,
    AllClaimedMaterial = 4,
}

impl CanonicalCbor for LicenseScope {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.uint(*self as u64);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclareLicenseV1 {
    pub declaration_id: ExternalKey,
    pub claim_ref: ObjectRefV1,
    pub licensor: ExternalKey,
    pub scope: LicenseScope,
    pub spdx_expression: String,
    pub additional_terms_hash: Option<Digest32>,
    pub effective_at_unix_s: u64,
}

impl DeclareLicenseV1 {
    pub fn validate(&self) -> Result<(), ResearchPayloadValidationError> {
        self.declaration_id.validate("declaration_id")?;
        validate_ref_kind(
            "claim_ref",
            self.claim_ref,
            ResearchObjectKind::ResearchClaim,
        )?;
        self.licensor.validate("licensor")?;
        validate_spdx(&self.spdx_expression)?;
        if let Some(hash) = &self.additional_terms_hash {
            validate_digest("additional_terms_hash", hash)?;
        }
        validate_timestamp(self.effective_at_unix_s)
    }

    pub fn object_ref(&self) -> ObjectRefV1 {
        ObjectRefV1::new(
            ResearchObjectKind::LicenseDeclaration,
            self.declaration_id,
            1,
        )
    }
}

impl CanonicalCbor for DeclareLicenseV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(8);
        encoder.uint(PROTOCOL_VERSION as u64);
        self.declaration_id.encode_canonical(encoder);
        self.claim_ref.encode_canonical(encoder);
        self.licensor.encode_canonical(encoder);
        self.scope.encode_canonical(encoder);
        encoder.text(&self.spdx_expression);
        encode_option_digest(encoder, &self.additional_terms_hash);
        encoder.uint(self.effective_at_unix_s);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeReason {
    Authorship = 1,
    ContributionAllocation = 2,
    EvidenceIntegrity = 3,
    Reproducibility = 4,
    LicenseConflict = 5,
}

impl CanonicalCbor for ChallengeReason {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.uint(*self as u64);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChallengeResearchClaimV1 {
    pub challenge_id: ExternalKey,
    pub claim_ref: ObjectRefV1,
    pub challenger: ExternalKey,
    pub reason: ChallengeReason,
    pub evidence_hash: Digest32,
    pub opened_at_unix_s: u64,
}

impl ChallengeResearchClaimV1 {
    pub fn validate(&self) -> Result<(), ResearchPayloadValidationError> {
        self.challenge_id.validate("challenge_id")?;
        validate_ref_kind(
            "claim_ref",
            self.claim_ref,
            ResearchObjectKind::ResearchClaim,
        )?;
        self.challenger.validate("challenger")?;
        validate_digest("evidence_hash", &self.evidence_hash)?;
        validate_timestamp(self.opened_at_unix_s)
    }

    pub fn object_ref(&self) -> ObjectRefV1 {
        ObjectRefV1::new(ResearchObjectKind::ClaimChallenge, self.challenge_id, 1)
    }
}

impl CanonicalCbor for ChallengeResearchClaimV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(7);
        encoder.uint(PROTOCOL_VERSION as u64);
        self.challenge_id.encode_canonical(encoder);
        self.claim_ref.encode_canonical(encoder);
        self.challenger.encode_canonical(encoder);
        self.reason.encode_canonical(encoder);
        encoder.bytes(&self.evidence_hash);
        encoder.uint(self.opened_at_unix_s);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum ClaimResolutionDecision {
    Uphold = 1,
    Reject = 2,
    AmendContributorShares = 3,
    RequireLicenseAmendment = 4,
}

impl CanonicalCbor for ClaimResolutionDecision {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.uint(*self as u64);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimResolutionV1 {
    pub resolution_id: ExternalKey,
    pub challenge_ref: ObjectRefV1,
    pub decision: ClaimResolutionDecision,
    pub resolution_hash: Digest32,
    pub amended_claimants: Vec<ClaimShareV1>,
    pub decided_at_unix_s: u64,
}

pub type ResolveResearchClaimV1 = ClaimResolutionV1;

impl ClaimResolutionV1 {
    pub fn validate(&self) -> Result<(), ResearchPayloadValidationError> {
        self.resolution_id.validate("resolution_id")?;
        validate_ref_kind(
            "challenge_ref",
            self.challenge_ref,
            ResearchObjectKind::ClaimChallenge,
        )?;
        validate_digest("resolution_hash", &self.resolution_hash)?;
        match self.decision {
            ClaimResolutionDecision::AmendContributorShares => {
                validate_claim_shares(&self.amended_claimants, false)?;
            }
            _ if !self.amended_claimants.is_empty() => {
                return Err(ResearchPayloadValidationError::UnexpectedAmendedClaimants)
            }
            _ => {}
        }
        validate_timestamp(self.decided_at_unix_s)
    }

    pub fn object_ref(&self) -> ObjectRefV1 {
        ObjectRefV1::new(ResearchObjectKind::ClaimResolution, self.resolution_id, 1)
    }
}

impl CanonicalCbor for ClaimResolutionV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(7);
        encoder.uint(PROTOCOL_VERSION as u64);
        self.resolution_id.encode_canonical(encoder);
        self.challenge_ref.encode_canonical(encoder);
        self.decision.encode_canonical(encoder);
        encoder.bytes(&self.resolution_hash);
        encoder.array(self.amended_claimants.len());
        for claimant in &self.amended_claimants {
            claimant.encode_canonical(encoder);
        }
        encoder.uint(self.decided_at_unix_s);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ResearchCommandV1 {
    MatchEvidenceCommitment(MatchEvidenceCommitmentV1),
    EvaluationCommitment(EvaluationCommitmentV1),
    IssueWorkloadReceipt(IssueWorkloadReceiptV1),
    CreateResearchClaim(CreateResearchClaimV1),
    DeclareLicense(DeclareLicenseV1),
    ChallengeResearchClaim(ChallengeResearchClaimV1),
    ResolveResearchClaim(ClaimResolutionV1),
}

impl ResearchCommandV1 {
    pub fn command_type(&self) -> &'static str {
        match self {
            Self::MatchEvidenceCommitment(_) => "match_evidence_commitment_v1",
            Self::EvaluationCommitment(_) => "evaluation_commitment_v1",
            Self::IssueWorkloadReceipt(_) => "issue_workload_receipt_v1",
            Self::CreateResearchClaim(_) => "create_research_claim_v1",
            Self::DeclareLicense(_) => "declare_license_v1",
            Self::ChallengeResearchClaim(_) => "challenge_research_claim_v1",
            Self::ResolveResearchClaim(_) => "resolve_research_claim_v1",
        }
    }

    pub fn validate(&self) -> Result<(), ResearchPayloadValidationError> {
        match self {
            Self::MatchEvidenceCommitment(payload) => payload.validate(),
            Self::EvaluationCommitment(payload) => payload.validate(),
            Self::IssueWorkloadReceipt(payload) => payload.validate(),
            Self::CreateResearchClaim(payload) => payload.validate(),
            Self::DeclareLicense(payload) => payload.validate(),
            Self::ChallengeResearchClaim(payload) => payload.validate(),
            Self::ResolveResearchClaim(payload) => payload.validate(),
        }
    }

    pub fn primary_object_ref(&self) -> ObjectRefV1 {
        match self {
            Self::MatchEvidenceCommitment(payload) => payload.object_ref(),
            Self::EvaluationCommitment(payload) => payload.object_ref(),
            Self::IssueWorkloadReceipt(payload) => payload.object_ref(),
            Self::CreateResearchClaim(payload) => payload.object_ref(),
            Self::DeclareLicense(payload) => payload.object_ref(),
            Self::ChallengeResearchClaim(payload) => payload.object_ref(),
            Self::ResolveResearchClaim(payload) => payload.object_ref(),
        }
    }

    /// Strict deterministic-CBOR decoder for node ingress.
    ///
    /// It rejects unknown versions/discriminants, non-minimal integer/length
    /// encodings, indefinite values, malformed lengths, trailing bytes, and
    /// any representation that does not re-encode byte-for-byte identically.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ResearchCommandDecodeError> {
        let mut decoder = Decoder::new(bytes);
        let command = Self::decode(&mut decoder)?;
        decoder.finish()?;
        if command.canonical_bytes() != bytes {
            return Err(CanonicalDecodeError::NonCanonicalRoundTrip.into());
        }
        command.validate()?;
        Ok(command)
    }

    pub(crate) fn decode(decoder: &mut Decoder<'_>) -> Result<Self, CanonicalDecodeError> {
        decoder.array(3)?;
        decode_version(decoder)?;
        let tag = decoder.uint()?;
        match tag {
            1 => Ok(Self::MatchEvidenceCommitment(decode_match(decoder)?)),
            2 => Ok(Self::EvaluationCommitment(decode_evaluation(decoder)?)),
            3 => Ok(Self::IssueWorkloadReceipt(decode_workload(decoder)?)),
            4 => Ok(Self::CreateResearchClaim(decode_claim(decoder)?)),
            5 => Ok(Self::DeclareLicense(decode_license(decoder)?)),
            6 => Ok(Self::ChallengeResearchClaim(decode_challenge(decoder)?)),
            7 => Ok(Self::ResolveResearchClaim(decode_resolution(decoder)?)),
            value => Err(CanonicalDecodeError::UnknownDiscriminant {
                name: "ResearchCommandV1",
                value,
            }),
        }
    }
}

impl CanonicalCbor for ResearchCommandV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(3);
        encoder.uint(PROTOCOL_VERSION as u64);
        match self {
            Self::MatchEvidenceCommitment(payload) => {
                encoder.uint(1);
                payload.encode_canonical(encoder);
            }
            Self::EvaluationCommitment(payload) => {
                encoder.uint(2);
                payload.encode_canonical(encoder);
            }
            Self::IssueWorkloadReceipt(payload) => {
                encoder.uint(3);
                payload.encode_canonical(encoder);
            }
            Self::CreateResearchClaim(payload) => {
                encoder.uint(4);
                payload.encode_canonical(encoder);
            }
            Self::DeclareLicense(payload) => {
                encoder.uint(5);
                payload.encode_canonical(encoder);
            }
            Self::ChallengeResearchClaim(payload) => {
                encoder.uint(6);
                payload.encode_canonical(encoder);
            }
            Self::ResolveResearchClaim(payload) => {
                encoder.uint(7);
                payload.encode_canonical(encoder);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    Active = 1,
    Challenged = 2,
    Rejected = 3,
    Amended = 4,
    LicenseAmendmentRequired = 5,
}

impl CanonicalCbor for ClaimStatus {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.uint(*self as u64);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum ClaimChallengeStatus {
    Open = 1,
    Resolved = 2,
}

impl CanonicalCbor for ClaimChallengeStatus {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.uint(*self as u64);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResearchPayloadValidationError {
    #[error("{0} cannot be the zero external key")]
    ZeroExternalKey(&'static str),
    #[error("{0} cannot use object version zero")]
    ZeroObjectVersion(&'static str),
    #[error("{field} must reference {expected:?}, got {got:?}")]
    WrongReferenceKind {
        field: &'static str,
        expected: ResearchObjectKind,
        got: ResearchObjectKind,
    },
    #[error("{0} cannot be the all-zero digest")]
    ZeroDigest(&'static str),
    #[error("{0} must be greater than zero")]
    ZeroValue(&'static str),
    #[error("timestamp must be greater than zero")]
    ZeroTimestamp,
    #[error("{0} collection size is outside protocol limits")]
    InvalidCollectionSize(&'static str),
    #[error("{0} must be strictly sorted by external key and contain no duplicates")]
    NonCanonicalOrdering(&'static str),
    #[error("basis points in {0} are outside 1..=10000")]
    BasisPointsOutOfRange(&'static str),
    #[error("claimant basis points must sum to 10000, got {0}")]
    ClaimShareTotalMismatch(u32),
    #[error("declared accepted work units {declared} do not match contributor total {computed}")]
    WorkUnitTotalMismatch { declared: u64, computed: u64 },
    #[error("accepted work unit summation overflowed")]
    WorkUnitOverflow,
    #[error("accepted evaluation must have a non-zero score")]
    InconsistentEvaluation,
    #[error("SPDX expression must be canonical visible ASCII without leading/trailing whitespace")]
    InvalidSpdxExpression,
    #[error("only amend-contributor-shares may carry amended claimant shares")]
    UnexpectedAmendedClaimants,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResearchCommandDecodeError {
    #[error(transparent)]
    Canonical(#[from] CanonicalDecodeError),
    #[error(transparent)]
    InvalidPayload(#[from] ResearchPayloadValidationError),
}

fn validate_digest(
    field: &'static str,
    digest: &Digest32,
) -> Result<(), ResearchPayloadValidationError> {
    if *digest == [0; 32] {
        return Err(ResearchPayloadValidationError::ZeroDigest(field));
    }
    Ok(())
}

fn validate_positive(
    field: &'static str,
    value: u64,
) -> Result<(), ResearchPayloadValidationError> {
    if value == 0 {
        return Err(ResearchPayloadValidationError::ZeroValue(field));
    }
    Ok(())
}

fn validate_timestamp(timestamp: u64) -> Result<(), ResearchPayloadValidationError> {
    if timestamp == 0 {
        return Err(ResearchPayloadValidationError::ZeroTimestamp);
    }
    Ok(())
}

fn validate_ref_kind(
    field: &'static str,
    object_ref: ObjectRefV1,
    expected: ResearchObjectKind,
) -> Result<(), ResearchPayloadValidationError> {
    object_ref.validate(field)?;
    if object_ref.kind != expected {
        return Err(ResearchPayloadValidationError::WrongReferenceKind {
            field,
            expected,
            got: object_ref.kind,
        });
    }
    Ok(())
}

fn validate_evidence_refs(refs: &[ObjectRefV1]) -> Result<(), ResearchPayloadValidationError> {
    if refs.is_empty() || refs.len() > 64 {
        return Err(ResearchPayloadValidationError::InvalidCollectionSize(
            "evidence_refs",
        ));
    }
    let mut previous = None;
    for object_ref in refs {
        object_ref.validate("evidence_ref")?;
        if !matches!(
            object_ref.kind,
            ResearchObjectKind::MatchEvidence | ResearchObjectKind::EvaluationCommitment
        ) {
            return Err(ResearchPayloadValidationError::WrongReferenceKind {
                field: "evidence_ref",
                expected: ResearchObjectKind::EvaluationCommitment,
                got: object_ref.kind,
            });
        }
        let ordering_key = (object_ref.kind, object_ref.key, object_ref.object_version);
        if previous.is_some_and(|value| value >= ordering_key) {
            return Err(ResearchPayloadValidationError::NonCanonicalOrdering(
                "evidence_refs",
            ));
        }
        previous = Some(ordering_key);
    }
    Ok(())
}

pub(crate) fn validate_claim_shares(
    shares: &[ClaimShareV1],
    allow_empty: bool,
) -> Result<(), ResearchPayloadValidationError> {
    if (!allow_empty && shares.is_empty()) || shares.len() > 128 {
        return Err(ResearchPayloadValidationError::InvalidCollectionSize(
            "claimants",
        ));
    }
    let mut total = 0u32;
    let mut previous = None;
    for share in shares {
        share.validate()?;
        if previous.is_some_and(|key| key >= share.contributor) {
            return Err(ResearchPayloadValidationError::NonCanonicalOrdering(
                "claimants",
            ));
        }
        previous = Some(share.contributor);
        total += u32::from(share.share_bps);
    }
    if !allow_empty && total != 10_000 {
        return Err(ResearchPayloadValidationError::ClaimShareTotalMismatch(
            total,
        ));
    }
    Ok(())
}

fn validate_spdx(expression: &str) -> Result<(), ResearchPayloadValidationError> {
    if expression.is_empty()
        || expression.len() > 128
        || expression.trim() != expression
        || !expression
            .bytes()
            .all(|byte| byte.is_ascii_graphic() || byte == b' ')
        || expression.contains("  ")
    {
        return Err(ResearchPayloadValidationError::InvalidSpdxExpression);
    }
    Ok(())
}

pub(crate) fn decode_version(decoder: &mut Decoder<'_>) -> Result<(), CanonicalDecodeError> {
    let version = decoder.uint()?;
    if version != PROTOCOL_VERSION as u64 {
        return Err(CanonicalDecodeError::UnsupportedVersion(version));
    }
    Ok(())
}

pub(crate) fn decode_external_key(
    decoder: &mut Decoder<'_>,
) -> Result<ExternalKey, CanonicalDecodeError> {
    Ok(ExternalKey::from_bytes(decoder.bytes_exact()?))
}

fn decode_kind(decoder: &mut Decoder<'_>) -> Result<ResearchObjectKind, CanonicalDecodeError> {
    let value = decoder.uint()?;
    match value {
        1 => Ok(ResearchObjectKind::MatchEvidence),
        2 => Ok(ResearchObjectKind::EvaluationCommitment),
        3 => Ok(ResearchObjectKind::WorkloadReceipt),
        4 => Ok(ResearchObjectKind::ResearchClaim),
        5 => Ok(ResearchObjectKind::LicenseDeclaration),
        6 => Ok(ResearchObjectKind::ClaimChallenge),
        7 => Ok(ResearchObjectKind::ClaimResolution),
        value => Err(CanonicalDecodeError::UnknownDiscriminant {
            name: "ResearchObjectKind",
            value,
        }),
    }
}

pub(crate) fn decode_object_ref(
    decoder: &mut Decoder<'_>,
) -> Result<ObjectRefV1, CanonicalDecodeError> {
    decoder.array(4)?;
    decode_version(decoder)?;
    Ok(ObjectRefV1 {
        kind: decode_kind(decoder)?,
        key: decode_external_key(decoder)?,
        object_version: decoder.uint()?,
    })
}

pub(crate) fn decode_match(
    decoder: &mut Decoder<'_>,
) -> Result<MatchEvidenceCommitmentV1, CanonicalDecodeError> {
    decoder.array(11)?;
    decode_version(decoder)?;
    Ok(MatchEvidenceCommitmentV1 {
        commitment_id: decode_external_key(decoder)?,
        match_id: decode_external_key(decoder)?,
        challenge_id: decode_external_key(decoder)?,
        event_root: decoder.bytes_exact()?,
        roster_root: decoder.bytes_exact()?,
        ruleset_hash: decoder.bytes_exact()?,
        dataset_hash: decoder.bytes_exact()?,
        archive_hash: decoder.bytes_exact()?,
        event_count: decoder.uint()?,
        completed_at_unix_s: decoder.uint()?,
    })
}

pub(crate) fn decode_evaluation(
    decoder: &mut Decoder<'_>,
) -> Result<EvaluationCommitmentV1, CanonicalDecodeError> {
    decoder.array(10)?;
    decode_version(decoder)?;
    Ok(EvaluationCommitmentV1 {
        evaluation_id: decode_external_key(decoder)?,
        match_evidence_ref: decode_object_ref(decoder)?,
        submission_hash: decoder.bytes_exact()?,
        rubric_hash: decoder.bytes_exact()?,
        evaluation_hash: decoder.bytes_exact()?,
        reproduction_hash: decoder.option_digest()?,
        score_bps: u16::try_from(decoder.uint()?).map_err(|_| {
            CanonicalDecodeError::UnknownDiscriminant {
                name: "score_bps",
                value: u64::MAX,
            }
        })?,
        accepted: decoder.bool()?,
        completed_at_unix_s: decoder.uint()?,
    })
}

fn decode_contribution(
    decoder: &mut Decoder<'_>,
) -> Result<ContributorWorkV1, CanonicalDecodeError> {
    decoder.array(5)?;
    decode_version(decoder)?;
    let contributor = decode_external_key(decoder)?;
    let role_raw = decoder.uint()?;
    let role = match role_raw {
        1 => ContributionRole::Researcher,
        2 => ContributionRole::DataProvider,
        3 => ContributionRole::Evaluator,
        4 => ContributionRole::Reproducer,
        5 => ContributionRole::AgentOperator,
        value => {
            return Err(CanonicalDecodeError::UnknownDiscriminant {
                name: "ContributionRole",
                value,
            })
        }
    };
    Ok(ContributorWorkV1 {
        contributor,
        role,
        accepted_work_units: decoder.uint()?,
        contribution_hash: decoder.bytes_exact()?,
    })
}

pub(crate) fn decode_workload(
    decoder: &mut Decoder<'_>,
) -> Result<IssueWorkloadReceiptV1, CanonicalDecodeError> {
    decoder.array(7)?;
    decode_version(decoder)?;
    let receipt_id = decode_external_key(decoder)?;
    let evaluation_ref = decode_object_ref(decoder)?;
    let len = decoder.array_len()?;
    let mut contributors = Vec::with_capacity(len.min(128));
    for _ in 0..len {
        contributors.push(decode_contribution(decoder)?);
    }
    Ok(IssueWorkloadReceiptV1 {
        receipt_id,
        evaluation_ref,
        contributors,
        total_accepted_work_units: decoder.uint()?,
        policy_hash: decoder.bytes_exact()?,
        issued_at_unix_s: decoder.uint()?,
    })
}

pub(crate) fn decode_claim_share(
    decoder: &mut Decoder<'_>,
) -> Result<ClaimShareV1, CanonicalDecodeError> {
    decoder.array(3)?;
    decode_version(decoder)?;
    let contributor = decode_external_key(decoder)?;
    let raw_share = decoder.uint()?;
    let share_bps =
        u16::try_from(raw_share).map_err(|_| CanonicalDecodeError::UnknownDiscriminant {
            name: "share_bps",
            value: raw_share,
        })?;
    Ok(ClaimShareV1 {
        contributor,
        share_bps,
    })
}

pub(crate) fn decode_claim(
    decoder: &mut Decoder<'_>,
) -> Result<CreateResearchClaimV1, CanonicalDecodeError> {
    decoder.array(8)?;
    decode_version(decoder)?;
    let claim_id = decode_external_key(decoder)?;
    let workload_receipt_ref = decode_object_ref(decoder)?;
    let evidence_len = decoder.array_len()?;
    let mut evidence_refs = Vec::with_capacity(evidence_len.min(64));
    for _ in 0..evidence_len {
        evidence_refs.push(decode_object_ref(decoder)?);
    }
    let artifact_hash = decoder.bytes_exact()?;
    let claim_scope_hash = decoder.bytes_exact()?;
    let claimant_len = decoder.array_len()?;
    let mut claimants = Vec::with_capacity(claimant_len.min(128));
    for _ in 0..claimant_len {
        claimants.push(decode_claim_share(decoder)?);
    }
    Ok(CreateResearchClaimV1 {
        claim_id,
        workload_receipt_ref,
        evidence_refs,
        artifact_hash,
        claim_scope_hash,
        claimants,
        created_at_unix_s: decoder.uint()?,
    })
}

fn decode_license_scope(decoder: &mut Decoder<'_>) -> Result<LicenseScope, CanonicalDecodeError> {
    let value = decoder.uint()?;
    match value {
        1 => Ok(LicenseScope::Artifact),
        2 => Ok(LicenseScope::Dataset),
        3 => Ok(LicenseScope::Method),
        4 => Ok(LicenseScope::AllClaimedMaterial),
        value => Err(CanonicalDecodeError::UnknownDiscriminant {
            name: "LicenseScope",
            value,
        }),
    }
}

pub(crate) fn decode_license(
    decoder: &mut Decoder<'_>,
) -> Result<DeclareLicenseV1, CanonicalDecodeError> {
    decoder.array(8)?;
    decode_version(decoder)?;
    Ok(DeclareLicenseV1 {
        declaration_id: decode_external_key(decoder)?,
        claim_ref: decode_object_ref(decoder)?,
        licensor: decode_external_key(decoder)?,
        scope: decode_license_scope(decoder)?,
        spdx_expression: decoder.text()?,
        additional_terms_hash: decoder.option_digest()?,
        effective_at_unix_s: decoder.uint()?,
    })
}

fn decode_challenge_reason(
    decoder: &mut Decoder<'_>,
) -> Result<ChallengeReason, CanonicalDecodeError> {
    let value = decoder.uint()?;
    match value {
        1 => Ok(ChallengeReason::Authorship),
        2 => Ok(ChallengeReason::ContributionAllocation),
        3 => Ok(ChallengeReason::EvidenceIntegrity),
        4 => Ok(ChallengeReason::Reproducibility),
        5 => Ok(ChallengeReason::LicenseConflict),
        value => Err(CanonicalDecodeError::UnknownDiscriminant {
            name: "ChallengeReason",
            value,
        }),
    }
}

pub(crate) fn decode_challenge(
    decoder: &mut Decoder<'_>,
) -> Result<ChallengeResearchClaimV1, CanonicalDecodeError> {
    decoder.array(7)?;
    decode_version(decoder)?;
    Ok(ChallengeResearchClaimV1 {
        challenge_id: decode_external_key(decoder)?,
        claim_ref: decode_object_ref(decoder)?,
        challenger: decode_external_key(decoder)?,
        reason: decode_challenge_reason(decoder)?,
        evidence_hash: decoder.bytes_exact()?,
        opened_at_unix_s: decoder.uint()?,
    })
}

fn decode_resolution_decision(
    decoder: &mut Decoder<'_>,
) -> Result<ClaimResolutionDecision, CanonicalDecodeError> {
    let value = decoder.uint()?;
    match value {
        1 => Ok(ClaimResolutionDecision::Uphold),
        2 => Ok(ClaimResolutionDecision::Reject),
        3 => Ok(ClaimResolutionDecision::AmendContributorShares),
        4 => Ok(ClaimResolutionDecision::RequireLicenseAmendment),
        value => Err(CanonicalDecodeError::UnknownDiscriminant {
            name: "ClaimResolutionDecision",
            value,
        }),
    }
}

pub(crate) fn decode_resolution(
    decoder: &mut Decoder<'_>,
) -> Result<ClaimResolutionV1, CanonicalDecodeError> {
    decoder.array(7)?;
    decode_version(decoder)?;
    let resolution_id = decode_external_key(decoder)?;
    let challenge_ref = decode_object_ref(decoder)?;
    let decision = decode_resolution_decision(decoder)?;
    let resolution_hash = decoder.bytes_exact()?;
    let len = decoder.array_len()?;
    let mut amended_claimants = Vec::with_capacity(len.min(128));
    for _ in 0..len {
        amended_claimants.push(decode_claim_share(decoder)?);
    }
    Ok(ClaimResolutionV1 {
        resolution_id,
        challenge_ref,
        decision,
        resolution_hash,
        amended_claimants,
        decided_at_unix_s: decoder.uint()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_mapping_is_strict_and_domain_separated() {
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let match_key = ExternalKey::from_uuid("hepta.match", uuid).unwrap();
        let claim_key = ExternalKey::from_uuid("hepta.claim", uuid).unwrap();
        assert_ne!(match_key, claim_key);
        assert_eq!(
            ExternalKey::from_uuid("hepta.match", uuid).unwrap(),
            match_key
        );
        assert_eq!(
            ExternalKey::from_uuid("hepta.match", &uuid.to_uppercase()).unwrap_err(),
            ExternalKeyError::InvalidUuid
        );
    }

    #[test]
    fn external_id_mapping_rejects_aliasing_whitespace() {
        assert!(ExternalKey::from_external_id("nakama.match", "match-001").is_ok());
        assert_eq!(
            ExternalKey::from_external_id("nakama.match", " match-001").unwrap_err(),
            ExternalKeyError::InvalidExternalId
        );
    }
}
