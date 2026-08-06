//! Versioned research-evidence protocol shared by TRNM, Hepta Research League,
//! and Nakama.
//!
//! The protocol intentionally commits hashes and contribution/accounting
//! metadata only. Raw research content, match event streams, submissions,
//! evaluations, and reproductions remain off-chain.

mod canonical;
mod command;
mod state;
mod types;

pub use canonical::{
    canonical_hash, CanonicalCbor, CanonicalDecodeError, Encoder, CANONICAL_ENCODING,
};
pub use command::{AuthorityRole, SignedResearchCommandV1, SignedResearchCommandValidationError};
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
