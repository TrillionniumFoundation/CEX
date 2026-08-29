//! Versioned research-evidence protocol shared by TRNM, Hepta Research League,
//! and Nakama.
//!
//! The protocol intentionally commits hashes and contribution/accounting
//! metadata only. Raw research content, match event streams, submissions,
//! evaluations, and reproductions remain off-chain.

mod canonical;
mod command;
mod paper_raid;
mod state;
mod types;

pub use canonical::{
    canonical_hash, CanonicalCbor, CanonicalDecodeError, Encoder, CANONICAL_ENCODING,
};
pub use command::{AuthorityRole, SignedResearchCommandV1, SignedResearchCommandValidationError};
pub use paper_raid::{
    PaperRaidAppealStatusV2, PaperRaidAppealStatusV3, PaperRaidFinalityCommitmentDecodeError,
    PaperRaidFinalityCommitmentV2, PaperRaidFinalityCommitmentV3, PaperRaidFinalityCommitmentV4,
    PaperRaidFinalityValidationError, PaperRaidReworkLineageV1, SignedPaperRaidFinalityCommandV2,
    SignedPaperRaidFinalityCommandV3, SignedPaperRaidFinalityCommandV4,
    SignedPaperRaidFinalityCommandValidationError, HEPTA_APPEAL_EXTERNAL_KEY_NAMESPACE_V1,
    HEPTA_EVALUATION_EXTERNAL_KEY_NAMESPACE_V1, HEPTA_PAPER_EXTERNAL_KEY_NAMESPACE_V1,
    HEPTA_PAPER_RAID_FINALITY_PREPARATION_EXTERNAL_KEY_NAMESPACE_V1,
    HEPTA_REPRODUCTION_EXTERNAL_KEY_NAMESPACE_V1, HEPTA_REVISION_EXTERNAL_KEY_NAMESPACE_V1,
    HEPTA_REWORK_EXTERNAL_KEY_NAMESPACE_V1, HEPTA_SUBMISSION_EXTERNAL_KEY_NAMESPACE_V1,
    PAPER_RAID_FINALITY_COMMITMENT_VERSION_V2, PAPER_RAID_FINALITY_COMMITMENT_VERSION_V3,
    PAPER_RAID_FINALITY_COMMITMENT_VERSION_V4, PAPER_RAID_REWORK_LINEAGE_VERSION_V1,
};
pub use state::{
    AppliedCommandRecordV1, ApplyOutcome, AuthorityIdentityV1, AuthoritySetV1,
    ClaimChallengeObjectV1, ClaimResolutionObjectV1, EvaluationCommitmentObjectV1,
    LicenseDeclarationObjectV1, MatchEvidenceObjectV1, ProtocolStateError, ResearchClaimObjectV1,
    ResearchDomainObjectV1, ResearchProtocolSnapshotV1, ResearchProtocolState,
    WorkloadReceiptObjectV1,
};
pub use types::{
    ChallengeReason, ChallengeResearchClaimV1, ClaimChallengeStatus, ClaimResolutionDecision,
    ClaimResolutionV1, ClaimShareV1, ClaimStatus, ContributionRole, ContributorWorkV1,
    CreateResearchClaimV1, DeclareLicenseV1, Digest32, EvaluationCommitmentV1, ExternalKey,
    ExternalKeyError, IssueWorkloadReceiptV1, LicenseScope, MatchEvidenceCommitmentV1, ObjectRefV1,
    ResearchCommandDecodeError, ResearchCommandV1, ResearchObjectKind,
    ResearchPayloadValidationError, ResolveResearchClaimV1, PROTOCOL_VERSION,
};
