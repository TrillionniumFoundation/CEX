use crate::canonical::{
    canonical_hash, CanonicalCbor, CanonicalDecodeError, Decoder, Encoder, CANONICAL_ENCODING,
};
use crate::command::AuthorityRole;
use crate::types::{Digest32, ExternalKey, ObjectRefV1, ResearchObjectKind};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Frozen Paper Raid finality wire published before the settlement-aware V3
/// appeal lineage extension. Its field count, discriminants, and hash domains
/// are consensus history and must never be reinterpreted.
pub const PAPER_RAID_FINALITY_COMMITMENT_VERSION_V2: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum PaperRaidAppealStatusV2 {
    Open = 1,
    ClosedNoAppeal = 2,
    ResolvedUpheld = 3,
    ResolvedOverturned = 4,
}

impl CanonicalCbor for PaperRaidAppealStatusV2 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.uint(*self as u64);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaperRaidFinalityCommitmentV2 {
    pub commitment_id: ExternalKey,
    pub paper_project_id: ExternalKey,
    pub submission_id: ExternalKey,
    pub match_evidence_ref: ObjectRefV1,
    pub release_candidate_hash: Digest32,
    pub paper_bundle_hash: Digest32,
    pub submission_commitment_hash: Digest32,
    pub author_consent_set_hash: Digest32,
    pub tolerance_policy_hash: Digest32,
    pub evaluation_id: ExternalKey,
    pub evaluation_hash: Digest32,
    pub evaluation_score_bps: u16,
    pub evaluation_accepted: bool,
    pub evaluation_completed_at_unix_s: u64,
    pub latest_reproduction_id: ExternalKey,
    pub latest_reproduction_hash: Digest32,
    pub latest_reproduction_accepted: bool,
    pub latest_reproduction_completed_at_unix_s: u64,
    pub evaluation_superseded_by: Option<ExternalKey>,
    pub reproduction_superseded_by: Option<ExternalKey>,
    pub appeal_status: PaperRaidAppealStatusV2,
    pub appeal_id: Option<ExternalKey>,
    pub appeal_resolution_hash: Option<Digest32>,
    pub appeal_window_closes_at_unix_s: u64,
    pub settlement_policy_hash: Digest32,
    pub scientific_finality: bool,
    pub score_eligible: bool,
    pub ranking_eligible: bool,
    pub reward_eligible: bool,
    pub economic_eligible: bool,
    pub finalized_at_unix_s: u64,
}

impl PaperRaidFinalityCommitmentV2 {
    pub fn validate(&self) -> Result<(), PaperRaidFinalityValidationError> {
        validate_common_fields(
            self.commitment_id,
            self.paper_project_id,
            self.submission_id,
            self.match_evidence_ref,
            &self.release_candidate_hash,
            &self.paper_bundle_hash,
            &self.submission_commitment_hash,
            &self.author_consent_set_hash,
            &self.tolerance_policy_hash,
            self.evaluation_id,
            &self.evaluation_hash,
            self.evaluation_score_bps,
            self.evaluation_accepted,
            self.evaluation_completed_at_unix_s,
            self.latest_reproduction_id,
            &self.latest_reproduction_hash,
            self.latest_reproduction_accepted,
            self.latest_reproduction_completed_at_unix_s,
            self.appeal_window_closes_at_unix_s,
            &self.settlement_policy_hash,
            self.finalized_at_unix_s,
            true,
        )?;
        if let Some(key) = self.evaluation_superseded_by {
            ensure_key("evaluation_superseded_by", key)?;
            return Err(PaperRaidFinalityValidationError::SupersededEvaluation);
        }
        if let Some(key) = self.reproduction_superseded_by {
            ensure_key("reproduction_superseded_by", key)?;
            return Err(PaperRaidFinalityValidationError::SupersededReproduction);
        }
        match self.appeal_status {
            PaperRaidAppealStatusV2::Open => {
                return Err(PaperRaidFinalityValidationError::AppealStillOpen)
            }
            PaperRaidAppealStatusV2::ClosedNoAppeal => {
                if self.appeal_id.is_some() || self.appeal_resolution_hash.is_some() {
                    return Err(PaperRaidFinalityValidationError::InconsistentAppeal);
                }
            }
            PaperRaidAppealStatusV2::ResolvedUpheld
            | PaperRaidAppealStatusV2::ResolvedOverturned => {
                let appeal_id = self
                    .appeal_id
                    .ok_or(PaperRaidFinalityValidationError::InconsistentAppeal)?;
                ensure_key("appeal_id", appeal_id)?;
                let resolution_hash = self
                    .appeal_resolution_hash
                    .as_ref()
                    .ok_or(PaperRaidFinalityValidationError::InconsistentAppeal)?;
                ensure_digest("appeal_resolution_hash", resolution_hash)?;
            }
        }
        if self.appeal_status == PaperRaidAppealStatusV2::ResolvedOverturned
            && self.evaluation_accepted
        {
            return Err(PaperRaidFinalityValidationError::InconsistentAppeal);
        }
        validate_finality_and_eligibility(
            self.scientific_finality,
            self.evaluation_accepted
                && self.latest_reproduction_accepted
                && matches!(
                    self.appeal_status,
                    PaperRaidAppealStatusV2::ClosedNoAppeal
                        | PaperRaidAppealStatusV2::ResolvedUpheld
                ),
            self.score_eligible,
            self.ranking_eligible,
            self.reward_eligible,
            self.economic_eligible,
        )
    }

    pub fn from_canonical_bytes(
        bytes: &[u8],
    ) -> Result<Self, PaperRaidFinalityCommitmentDecodeError> {
        let mut decoder = Decoder::new(bytes);
        decoder.array(32)?;
        let version = decoder.uint()?;
        if version != PAPER_RAID_FINALITY_COMMITMENT_VERSION_V2 as u64 {
            return Err(CanonicalDecodeError::UnsupportedVersion(version).into());
        }
        let commitment = Self {
            commitment_id: decode_key(&mut decoder)?,
            paper_project_id: decode_key(&mut decoder)?,
            submission_id: decode_key(&mut decoder)?,
            match_evidence_ref: decode_object_ref(&mut decoder)?,
            release_candidate_hash: decoder.bytes_exact()?,
            paper_bundle_hash: decoder.bytes_exact()?,
            submission_commitment_hash: decoder.bytes_exact()?,
            author_consent_set_hash: decoder.bytes_exact()?,
            tolerance_policy_hash: decoder.bytes_exact()?,
            evaluation_id: decode_key(&mut decoder)?,
            evaluation_hash: decoder.bytes_exact()?,
            evaluation_score_bps: {
                let value = decoder.uint()?;
                u16::try_from(value)
                    .map_err(|_| PaperRaidFinalityCommitmentDecodeError::ScoreOutOfRange(value))?
            },
            evaluation_accepted: decoder.bool()?,
            evaluation_completed_at_unix_s: decoder.uint()?,
            latest_reproduction_id: decode_key(&mut decoder)?,
            latest_reproduction_hash: decoder.bytes_exact()?,
            latest_reproduction_accepted: decoder.bool()?,
            latest_reproduction_completed_at_unix_s: decoder.uint()?,
            evaluation_superseded_by: decode_option_key(&mut decoder)?,
            reproduction_superseded_by: decode_option_key(&mut decoder)?,
            appeal_status: decode_appeal_status_v2(&mut decoder)?,
            appeal_id: decode_option_key(&mut decoder)?,
            appeal_resolution_hash: decode_option_digest(&mut decoder)?,
            appeal_window_closes_at_unix_s: decoder.uint()?,
            settlement_policy_hash: decoder.bytes_exact()?,
            scientific_finality: decoder.bool()?,
            score_eligible: decoder.bool()?,
            ranking_eligible: decoder.bool()?,
            reward_eligible: decoder.bool()?,
            economic_eligible: decoder.bool()?,
            finalized_at_unix_s: decoder.uint()?,
        };
        decoder.finish()?;
        commitment.validate()?;
        if commitment.canonical_bytes() != bytes {
            return Err(CanonicalDecodeError::NonCanonicalRoundTrip.into());
        }
        Ok(commitment)
    }
}

impl CanonicalCbor for PaperRaidFinalityCommitmentV2 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(32);
        encoder.uint(PAPER_RAID_FINALITY_COMMITMENT_VERSION_V2 as u64);
        self.commitment_id.encode_canonical(encoder);
        self.paper_project_id.encode_canonical(encoder);
        self.submission_id.encode_canonical(encoder);
        self.match_evidence_ref.encode_canonical(encoder);
        encoder.bytes(&self.release_candidate_hash);
        encoder.bytes(&self.paper_bundle_hash);
        encoder.bytes(&self.submission_commitment_hash);
        encoder.bytes(&self.author_consent_set_hash);
        encoder.bytes(&self.tolerance_policy_hash);
        self.evaluation_id.encode_canonical(encoder);
        encoder.bytes(&self.evaluation_hash);
        encoder.uint(self.evaluation_score_bps as u64);
        encoder.bool(self.evaluation_accepted);
        encoder.uint(self.evaluation_completed_at_unix_s);
        self.latest_reproduction_id.encode_canonical(encoder);
        encoder.bytes(&self.latest_reproduction_hash);
        encoder.bool(self.latest_reproduction_accepted);
        encoder.uint(self.latest_reproduction_completed_at_unix_s);
        encode_option_key(encoder, self.evaluation_superseded_by);
        encode_option_key(encoder, self.reproduction_superseded_by);
        self.appeal_status.encode_canonical(encoder);
        encode_option_key(encoder, self.appeal_id);
        encode_option_digest(encoder, &self.appeal_resolution_hash);
        encoder.uint(self.appeal_window_closes_at_unix_s);
        encoder.bytes(&self.settlement_policy_hash);
        encoder.bool(self.scientific_finality);
        encoder.bool(self.score_eligible);
        encoder.bool(self.ranking_eligible);
        encoder.bool(self.reward_eligible);
        encoder.bool(self.economic_eligible);
        encoder.uint(self.finalized_at_unix_s);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedPaperRaidFinalityCommandV2 {
    pub chain_id: String,
    pub command_id: ExternalKey,
    pub signer_did: String,
    pub signer_role: AuthorityRole,
    pub nonce: u64,
    pub public_key: [u8; 32],
    pub commitment: PaperRaidFinalityCommitmentV2,
    pub signature: Vec<u8>,
}

impl SignedPaperRaidFinalityCommandV2 {
    #[allow(clippy::too_many_arguments)]
    pub fn sign(
        chain_id: String,
        command_id: ExternalKey,
        signer_did: String,
        nonce: u64,
        commitment: PaperRaidFinalityCommitmentV2,
        signing_key: &SigningKey,
    ) -> Result<Self, SignedPaperRaidFinalityCommandValidationError> {
        let mut signed = Self {
            chain_id,
            command_id,
            signer_did,
            signer_role: AuthorityRole::HeptaAuthority,
            nonce,
            public_key: signing_key.verifying_key().to_bytes(),
            commitment,
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
        encoder.uint(PAPER_RAID_FINALITY_COMMITMENT_VERSION_V2 as u64);
        encoder.text(CANONICAL_ENCODING);
        encoder.text(&self.chain_id);
        self.command_id.encode_canonical(&mut encoder);
        encoder.text(&self.signer_did);
        self.signer_role.encode_canonical(&mut encoder);
        encoder.uint(self.nonce);
        encoder.bytes(&self.public_key);
        self.commitment.encode_canonical(&mut encoder);
        encoder.finish()
    }

    pub fn from_canonical_bytes(
        bytes: &[u8],
    ) -> Result<Self, SignedPaperRaidFinalityCommandValidationError> {
        let mut decoder = Decoder::new(bytes);
        decoder.array(10)?;
        let version = decoder.uint()?;
        if version != PAPER_RAID_FINALITY_COMMITMENT_VERSION_V2 as u64 {
            return Err(CanonicalDecodeError::UnsupportedVersion(version).into());
        }
        if decoder.text()? != CANONICAL_ENCODING {
            return Err(SignedPaperRaidFinalityCommandValidationError::EncodingMismatch);
        }
        let chain_id = decoder.text()?;
        let command_id = decode_key(&mut decoder)?;
        let signer_did = decoder.text()?;
        let signer_role = decode_authority_role(&mut decoder)?;
        let nonce = decoder.uint()?;
        let public_key = decoder.bytes_exact()?;
        let commitment = decode_commitment_v2(&mut decoder)?;
        let signature = decoder.bytes()?.to_vec();
        decoder.finish()?;
        let signed = Self {
            chain_id,
            command_id,
            signer_did,
            signer_role,
            nonce,
            public_key,
            commitment,
            signature,
        };
        if signed.canonical_bytes() != bytes {
            return Err(CanonicalDecodeError::NonCanonicalRoundTrip.into());
        }
        signed.validate()?;
        Ok(signed)
    }

    pub fn validate(&self) -> Result<(), SignedPaperRaidFinalityCommandValidationError> {
        self.validate_unsigned()?;
        verify_signature(&self.public_key, &self.signature, &self.signing_bytes())
    }

    pub fn command_fingerprint(&self) -> Digest32 {
        canonical_hash(
            "trnm-paper-raid-finality-command-fingerprint-v2",
            &self.signing_bytes(),
        )
    }

    pub fn payload_hash(&self) -> Digest32 {
        self.commitment
            .canonical_hash("trnm-paper-raid-finality-commitment-v2")
    }

    fn validate_unsigned(&self) -> Result<(), SignedPaperRaidFinalityCommandValidationError> {
        validate_signed_command_fields(
            &self.chain_id,
            self.command_id,
            &self.signer_did,
            self.signer_role,
            self.nonce,
            &self.public_key,
        )?;
        self.commitment.validate()?;
        Ok(())
    }
}

impl CanonicalCbor for SignedPaperRaidFinalityCommandV2 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(10);
        encoder.uint(PAPER_RAID_FINALITY_COMMITMENT_VERSION_V2 as u64);
        encoder.text(CANONICAL_ENCODING);
        encoder.text(&self.chain_id);
        self.command_id.encode_canonical(encoder);
        encoder.text(&self.signer_did);
        self.signer_role.encode_canonical(encoder);
        encoder.uint(self.nonce);
        encoder.bytes(&self.public_key);
        self.commitment.encode_canonical(encoder);
        encoder.bytes(&self.signature);
    }
}

/// Paper Raid's settlement-aware commitment is deliberately versioned outside
/// the frozen generic Research V1 command/state schema.
pub const PAPER_RAID_FINALITY_COMMITMENT_VERSION_V3: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum PaperRaidAppealStatusV3 {
    Open = 1,
    ClosedNoAppeal = 2,
    ResolvedDenied = 3,
    ResolvedUpheld = 4,
}

impl CanonicalCbor for PaperRaidAppealStatusV3 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.uint(*self as u64);
    }
}

/// Immutable Paper Raid scientific-finality and settlement eligibility tuple.
///
/// The commitment binds the exact Paper/submission/release/bundle/consent,
/// evaluation, latest reproduction, appeal closure, and policy facts. A Chain
/// receipt can prove inclusion of this object without conflating scientific
/// finality with ranking or reward eligibility.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaperRaidFinalityCommitmentV3 {
    pub commitment_id: ExternalKey,
    pub paper_project_id: ExternalKey,
    pub submission_id: ExternalKey,
    pub match_evidence_ref: ObjectRefV1,
    pub release_candidate_hash: Digest32,
    pub paper_bundle_hash: Digest32,
    pub submission_commitment_hash: Digest32,
    pub author_consent_set_hash: Digest32,
    pub tolerance_policy_hash: Digest32,
    pub evaluation_id: ExternalKey,
    pub evaluation_hash: Digest32,
    pub evaluation_score_bps: u16,
    pub evaluation_accepted: bool,
    pub evaluation_completed_at_unix_s: u64,
    pub latest_reproduction_id: ExternalKey,
    pub latest_reproduction_hash: Digest32,
    pub latest_reproduction_accepted: bool,
    pub latest_reproduction_completed_at_unix_s: u64,
    /// Prior evaluation superseded by the final/latest evaluation. This is
    /// present only when an Appeal was upheld and must equal the appealed
    /// evaluation identity.
    pub evaluation_supersedes: Option<ExternalKey>,
    pub evaluation_superseded_by: Option<ExternalKey>,
    pub reproduction_superseded_by: Option<ExternalKey>,
    pub appeal_status: PaperRaidAppealStatusV3,
    pub appeal_id: Option<ExternalKey>,
    pub appealed_evaluation_id: Option<ExternalKey>,
    pub appeal_resolution_hash: Option<Digest32>,
    pub appeal_window_closes_at_unix_s: u64,
    pub settlement_policy_hash: Digest32,
    pub scientific_finality: bool,
    pub score_eligible: bool,
    pub ranking_eligible: bool,
    pub reward_eligible: bool,
    pub economic_eligible: bool,
    pub finalized_at_unix_s: u64,
}

impl PaperRaidFinalityCommitmentV3 {
    pub fn validate(&self) -> Result<(), PaperRaidFinalityValidationError> {
        for (field, key) in [
            ("commitment_id", self.commitment_id),
            ("paper_project_id", self.paper_project_id),
            ("submission_id", self.submission_id),
            ("evaluation_id", self.evaluation_id),
            ("latest_reproduction_id", self.latest_reproduction_id),
        ] {
            ensure_key(field, key)?;
        }
        ensure_key("match_evidence_ref.key", self.match_evidence_ref.key)?;
        if self.match_evidence_ref.kind != ResearchObjectKind::MatchEvidence
            || self.match_evidence_ref.object_version == 0
        {
            return Err(PaperRaidFinalityValidationError::InvalidMatchEvidenceRef);
        }
        for (field, digest) in [
            ("release_candidate_hash", &self.release_candidate_hash),
            ("paper_bundle_hash", &self.paper_bundle_hash),
            (
                "submission_commitment_hash",
                &self.submission_commitment_hash,
            ),
            ("author_consent_set_hash", &self.author_consent_set_hash),
            ("tolerance_policy_hash", &self.tolerance_policy_hash),
            ("evaluation_hash", &self.evaluation_hash),
            ("latest_reproduction_hash", &self.latest_reproduction_hash),
            ("settlement_policy_hash", &self.settlement_policy_hash),
        ] {
            ensure_digest(field, digest)?;
        }
        if self.evaluation_score_bps > 10_000
            || (self.evaluation_accepted && self.evaluation_score_bps == 0)
        {
            return Err(PaperRaidFinalityValidationError::InconsistentEvaluation);
        }
        if self.evaluation_completed_at_unix_s == 0
            || self.latest_reproduction_completed_at_unix_s < self.evaluation_completed_at_unix_s
            || self.appeal_window_closes_at_unix_s < self.latest_reproduction_completed_at_unix_s
            || self.finalized_at_unix_s < self.appeal_window_closes_at_unix_s
        {
            return Err(PaperRaidFinalityValidationError::TimestampRegression);
        }
        if let Some(key) = self.evaluation_supersedes {
            ensure_key("evaluation_supersedes", key)?;
        }
        if let Some(key) = self.evaluation_superseded_by {
            ensure_key("evaluation_superseded_by", key)?;
            return Err(PaperRaidFinalityValidationError::SupersededEvaluation);
        }
        if let Some(key) = self.reproduction_superseded_by {
            ensure_key("reproduction_superseded_by", key)?;
            return Err(PaperRaidFinalityValidationError::SupersededReproduction);
        }
        match self.appeal_status {
            PaperRaidAppealStatusV3::Open => {
                return Err(PaperRaidFinalityValidationError::AppealStillOpen)
            }
            PaperRaidAppealStatusV3::ClosedNoAppeal => {
                if self.appeal_id.is_some()
                    || self.appealed_evaluation_id.is_some()
                    || self.appeal_resolution_hash.is_some()
                    || self.evaluation_supersedes.is_some()
                {
                    return Err(PaperRaidFinalityValidationError::InconsistentAppeal);
                }
            }
            PaperRaidAppealStatusV3::ResolvedDenied | PaperRaidAppealStatusV3::ResolvedUpheld => {
                let appeal_id = self
                    .appeal_id
                    .ok_or(PaperRaidFinalityValidationError::InconsistentAppeal)?;
                ensure_key("appeal_id", appeal_id)?;
                let appealed_evaluation_id = self
                    .appealed_evaluation_id
                    .ok_or(PaperRaidFinalityValidationError::InconsistentAppeal)?;
                ensure_key("appealed_evaluation_id", appealed_evaluation_id)?;
                let resolution_hash = self
                    .appeal_resolution_hash
                    .as_ref()
                    .ok_or(PaperRaidFinalityValidationError::InconsistentAppeal)?;
                ensure_digest("appeal_resolution_hash", resolution_hash)?;
                match self.appeal_status {
                    PaperRaidAppealStatusV3::ResolvedDenied
                        if appealed_evaluation_id == self.evaluation_id
                            && self.evaluation_supersedes.is_none() => {}
                    PaperRaidAppealStatusV3::ResolvedUpheld
                        if appealed_evaluation_id != self.evaluation_id
                            && self.evaluation_supersedes == Some(appealed_evaluation_id) => {}
                    _ => return Err(PaperRaidFinalityValidationError::InconsistentAppeal),
                }
            }
        }
        if !self.scientific_finality {
            return Err(PaperRaidFinalityValidationError::ScientificFinalityRequired);
        }

        let eligibility_base = self.evaluation_accepted
            && self.latest_reproduction_accepted
            && matches!(
                self.appeal_status,
                PaperRaidAppealStatusV3::ClosedNoAppeal
                    | PaperRaidAppealStatusV3::ResolvedDenied
                    | PaperRaidAppealStatusV3::ResolvedUpheld
            );
        if (self.score_eligible
            || self.ranking_eligible
            || self.reward_eligible
            || self.economic_eligible)
            && !eligibility_base
        {
            return Err(PaperRaidFinalityValidationError::EligibilityWithoutFinalFacts);
        }
        if self.ranking_eligible && !self.score_eligible {
            return Err(PaperRaidFinalityValidationError::InconsistentEligibility);
        }
        if self.reward_eligible
            && !(self.ranking_eligible && self.score_eligible && self.economic_eligible)
        {
            return Err(PaperRaidFinalityValidationError::InconsistentEligibility);
        }
        if self.economic_eligible && !self.reward_eligible {
            return Err(PaperRaidFinalityValidationError::InconsistentEligibility);
        }
        Ok(())
    }

    pub fn from_canonical_bytes(
        bytes: &[u8],
    ) -> Result<Self, PaperRaidFinalityCommitmentDecodeError> {
        let mut decoder = Decoder::new(bytes);
        decoder.array(34)?;
        let version = decoder.uint()?;
        if version != PAPER_RAID_FINALITY_COMMITMENT_VERSION_V3 as u64 {
            return Err(CanonicalDecodeError::UnsupportedVersion(version).into());
        }
        let commitment = Self {
            commitment_id: decode_key(&mut decoder)?,
            paper_project_id: decode_key(&mut decoder)?,
            submission_id: decode_key(&mut decoder)?,
            match_evidence_ref: decode_object_ref(&mut decoder)?,
            release_candidate_hash: decoder.bytes_exact()?,
            paper_bundle_hash: decoder.bytes_exact()?,
            submission_commitment_hash: decoder.bytes_exact()?,
            author_consent_set_hash: decoder.bytes_exact()?,
            tolerance_policy_hash: decoder.bytes_exact()?,
            evaluation_id: decode_key(&mut decoder)?,
            evaluation_hash: decoder.bytes_exact()?,
            evaluation_score_bps: {
                let value = decoder.uint()?;
                u16::try_from(value)
                    .map_err(|_| PaperRaidFinalityCommitmentDecodeError::ScoreOutOfRange(value))?
            },
            evaluation_accepted: decoder.bool()?,
            evaluation_completed_at_unix_s: decoder.uint()?,
            latest_reproduction_id: decode_key(&mut decoder)?,
            latest_reproduction_hash: decoder.bytes_exact()?,
            latest_reproduction_accepted: decoder.bool()?,
            latest_reproduction_completed_at_unix_s: decoder.uint()?,
            evaluation_supersedes: decode_option_key(&mut decoder)?,
            evaluation_superseded_by: decode_option_key(&mut decoder)?,
            reproduction_superseded_by: decode_option_key(&mut decoder)?,
            appeal_status: decode_appeal_status_v3(&mut decoder)?,
            appeal_id: decode_option_key(&mut decoder)?,
            appealed_evaluation_id: decode_option_key(&mut decoder)?,
            appeal_resolution_hash: decode_option_digest(&mut decoder)?,
            appeal_window_closes_at_unix_s: decoder.uint()?,
            settlement_policy_hash: decoder.bytes_exact()?,
            scientific_finality: decoder.bool()?,
            score_eligible: decoder.bool()?,
            ranking_eligible: decoder.bool()?,
            reward_eligible: decoder.bool()?,
            economic_eligible: decoder.bool()?,
            finalized_at_unix_s: decoder.uint()?,
        };
        decoder.finish()?;
        commitment.validate()?;
        if commitment.canonical_bytes() != bytes {
            return Err(CanonicalDecodeError::NonCanonicalRoundTrip.into());
        }
        Ok(commitment)
    }
}

impl CanonicalCbor for PaperRaidFinalityCommitmentV3 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(34);
        encoder.uint(PAPER_RAID_FINALITY_COMMITMENT_VERSION_V3 as u64);
        self.commitment_id.encode_canonical(encoder);
        self.paper_project_id.encode_canonical(encoder);
        self.submission_id.encode_canonical(encoder);
        self.match_evidence_ref.encode_canonical(encoder);
        encoder.bytes(&self.release_candidate_hash);
        encoder.bytes(&self.paper_bundle_hash);
        encoder.bytes(&self.submission_commitment_hash);
        encoder.bytes(&self.author_consent_set_hash);
        encoder.bytes(&self.tolerance_policy_hash);
        self.evaluation_id.encode_canonical(encoder);
        encoder.bytes(&self.evaluation_hash);
        encoder.uint(self.evaluation_score_bps as u64);
        encoder.bool(self.evaluation_accepted);
        encoder.uint(self.evaluation_completed_at_unix_s);
        self.latest_reproduction_id.encode_canonical(encoder);
        encoder.bytes(&self.latest_reproduction_hash);
        encoder.bool(self.latest_reproduction_accepted);
        encoder.uint(self.latest_reproduction_completed_at_unix_s);
        encode_option_key(encoder, self.evaluation_supersedes);
        encode_option_key(encoder, self.evaluation_superseded_by);
        encode_option_key(encoder, self.reproduction_superseded_by);
        self.appeal_status.encode_canonical(encoder);
        encode_option_key(encoder, self.appeal_id);
        encode_option_key(encoder, self.appealed_evaluation_id);
        encode_option_digest(encoder, &self.appeal_resolution_hash);
        encoder.uint(self.appeal_window_closes_at_unix_s);
        encoder.bytes(&self.settlement_policy_hash);
        encoder.bool(self.scientific_finality);
        encoder.bool(self.score_eligible);
        encoder.bool(self.ranking_eligible);
        encoder.bool(self.reward_eligible);
        encoder.bool(self.economic_eligible);
        encoder.uint(self.finalized_at_unix_s);
    }
}

/// Hepta-authorized, signature-bearing Paper Raid finality command.  It is a
/// separate V3 envelope so adding Paper settlement semantics cannot change the
/// frozen `SignedResearchCommandV1` wire or its command discriminants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedPaperRaidFinalityCommandV3 {
    pub chain_id: String,
    pub command_id: ExternalKey,
    pub signer_did: String,
    pub signer_role: AuthorityRole,
    pub nonce: u64,
    pub public_key: [u8; 32],
    pub commitment: PaperRaidFinalityCommitmentV3,
    pub signature: Vec<u8>,
}

impl SignedPaperRaidFinalityCommandV3 {
    #[allow(clippy::too_many_arguments)]
    pub fn sign(
        chain_id: String,
        command_id: ExternalKey,
        signer_did: String,
        nonce: u64,
        commitment: PaperRaidFinalityCommitmentV3,
        signing_key: &SigningKey,
    ) -> Result<Self, SignedPaperRaidFinalityCommandValidationError> {
        let mut signed = Self {
            chain_id,
            command_id,
            signer_did,
            signer_role: AuthorityRole::HeptaAuthority,
            nonce,
            public_key: signing_key.verifying_key().to_bytes(),
            commitment,
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
        encoder.uint(PAPER_RAID_FINALITY_COMMITMENT_VERSION_V3 as u64);
        encoder.text(CANONICAL_ENCODING);
        encoder.text(&self.chain_id);
        self.command_id.encode_canonical(&mut encoder);
        encoder.text(&self.signer_did);
        self.signer_role.encode_canonical(&mut encoder);
        encoder.uint(self.nonce);
        encoder.bytes(&self.public_key);
        self.commitment.encode_canonical(&mut encoder);
        encoder.finish()
    }

    pub fn from_canonical_bytes(
        bytes: &[u8],
    ) -> Result<Self, SignedPaperRaidFinalityCommandValidationError> {
        let mut decoder = Decoder::new(bytes);
        decoder.array(10)?;
        let version = decoder.uint()?;
        if version != PAPER_RAID_FINALITY_COMMITMENT_VERSION_V3 as u64 {
            return Err(CanonicalDecodeError::UnsupportedVersion(version).into());
        }
        if decoder.text()? != CANONICAL_ENCODING {
            return Err(SignedPaperRaidFinalityCommandValidationError::EncodingMismatch);
        }
        let chain_id = decoder.text()?;
        let command_id = decode_key(&mut decoder)?;
        let signer_did = decoder.text()?;
        let signer_role = match decoder.uint()? {
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
        let commitment = decode_commitment_v3(&mut decoder)?;
        let signature = decoder.bytes()?.to_vec();
        decoder.finish()?;
        let signed = Self {
            chain_id,
            command_id,
            signer_did,
            signer_role,
            nonce,
            public_key,
            commitment,
            signature,
        };
        if signed.canonical_bytes() != bytes {
            return Err(CanonicalDecodeError::NonCanonicalRoundTrip.into());
        }
        signed.validate()?;
        Ok(signed)
    }

    pub fn validate(&self) -> Result<(), SignedPaperRaidFinalityCommandValidationError> {
        self.validate_unsigned()?;
        let signature_bytes: [u8; 64] = self
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| SignedPaperRaidFinalityCommandValidationError::InvalidSignature)?;
        let verifying_key = VerifyingKey::from_bytes(&self.public_key)
            .map_err(|_| SignedPaperRaidFinalityCommandValidationError::InvalidPublicKey)?;
        verifying_key
            .verify_strict(
                &self.signing_bytes(),
                &Signature::from_bytes(&signature_bytes),
            )
            .map_err(|_| SignedPaperRaidFinalityCommandValidationError::InvalidSignature)
    }

    pub fn command_fingerprint(&self) -> Digest32 {
        canonical_hash(
            "trnm-paper-raid-finality-command-fingerprint-v3",
            &self.signing_bytes(),
        )
    }

    pub fn payload_hash(&self) -> Digest32 {
        self.commitment
            .canonical_hash("trnm-paper-raid-finality-commitment-v3")
    }

    fn validate_unsigned(&self) -> Result<(), SignedPaperRaidFinalityCommandValidationError> {
        validate_chain_id(&self.chain_id)?;
        ensure_key("command_id", self.command_id)?;
        validate_signer_did(&self.signer_did)?;
        if self.signer_role != AuthorityRole::HeptaAuthority {
            return Err(SignedPaperRaidFinalityCommandValidationError::HeptaAuthorityRequired);
        }
        if self.nonce == 0 {
            return Err(SignedPaperRaidFinalityCommandValidationError::ZeroNonce);
        }
        if self.public_key == [0; 32] || VerifyingKey::from_bytes(&self.public_key).is_err() {
            return Err(SignedPaperRaidFinalityCommandValidationError::InvalidPublicKey);
        }
        self.commitment.validate()?;
        Ok(())
    }
}

impl CanonicalCbor for SignedPaperRaidFinalityCommandV3 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(10);
        encoder.uint(PAPER_RAID_FINALITY_COMMITMENT_VERSION_V3 as u64);
        encoder.text(CANONICAL_ENCODING);
        encoder.text(&self.chain_id);
        self.command_id.encode_canonical(encoder);
        encoder.text(&self.signer_did);
        self.signer_role.encode_canonical(encoder);
        encoder.uint(self.nonce);
        encoder.bytes(&self.public_key);
        self.commitment.encode_canonical(encoder);
        encoder.bytes(&self.signature);
    }
}

/// Canonical Paper Raid rework lineage. The wire is a frozen 13-item array:
/// version plus the exact rejected/replacement identity and content pairs.
pub const PAPER_RAID_REWORK_LINEAGE_VERSION_V1: u8 = 1;

/// Frozen UUID-to-`ExternalKey` namespaces shared by Hepta admission and the
/// Chain-owned Paper Raid V4 converter. Changing any value changes consensus
/// object identity for the same canonical Hepta UUID.
pub const HEPTA_PAPER_RAID_FINALITY_PREPARATION_EXTERNAL_KEY_NAMESPACE_V1: &str =
    "hepta.paper-raid.finality-preparation";
pub const HEPTA_PAPER_EXTERNAL_KEY_NAMESPACE_V1: &str = "hepta.paper";
pub const HEPTA_SUBMISSION_EXTERNAL_KEY_NAMESPACE_V1: &str = "hepta.submission";
pub const HEPTA_EVALUATION_EXTERNAL_KEY_NAMESPACE_V1: &str = "hepta.evaluation";
pub const HEPTA_REPRODUCTION_EXTERNAL_KEY_NAMESPACE_V1: &str = "hepta.reproduction";
pub const HEPTA_APPEAL_EXTERNAL_KEY_NAMESPACE_V1: &str = "hepta.appeal";
pub const HEPTA_REWORK_EXTERNAL_KEY_NAMESPACE_V1: &str = "hepta.rework";
pub const HEPTA_REVISION_EXTERNAL_KEY_NAMESPACE_V1: &str = "hepta.revision";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaperRaidReworkLineageV1 {
    pub rework_id: ExternalKey,
    pub rework_cycle: u64,
    pub rejected_submission_id: ExternalKey,
    pub replacement_submission_id: ExternalKey,
    pub rejected_revision_id: ExternalKey,
    pub replacement_revision_id: ExternalKey,
    pub rejected_release_candidate_hash: Digest32,
    pub replacement_release_candidate_hash: Digest32,
    pub rejected_paper_bundle_hash: Digest32,
    pub replacement_paper_bundle_hash: Digest32,
    pub rejected_rework_content_commitment_sha256: Digest32,
    pub replacement_rework_content_commitment_sha256: Digest32,
}

impl PaperRaidReworkLineageV1 {
    pub fn validate(&self) -> Result<(), PaperRaidFinalityValidationError> {
        for (field, key) in [
            ("rework_lineage.rework_id", self.rework_id),
            (
                "rework_lineage.rejected_submission_id",
                self.rejected_submission_id,
            ),
            (
                "rework_lineage.replacement_submission_id",
                self.replacement_submission_id,
            ),
            (
                "rework_lineage.rejected_revision_id",
                self.rejected_revision_id,
            ),
            (
                "rework_lineage.replacement_revision_id",
                self.replacement_revision_id,
            ),
        ] {
            ensure_key(field, key)?;
        }
        for (field, digest) in [
            (
                "rework_lineage.rejected_release_candidate_hash",
                &self.rejected_release_candidate_hash,
            ),
            (
                "rework_lineage.replacement_release_candidate_hash",
                &self.replacement_release_candidate_hash,
            ),
            (
                "rework_lineage.rejected_paper_bundle_hash",
                &self.rejected_paper_bundle_hash,
            ),
            (
                "rework_lineage.replacement_paper_bundle_hash",
                &self.replacement_paper_bundle_hash,
            ),
            (
                "rework_lineage.rejected_rework_content_commitment_sha256",
                &self.rejected_rework_content_commitment_sha256,
            ),
            (
                "rework_lineage.replacement_rework_content_commitment_sha256",
                &self.replacement_rework_content_commitment_sha256,
            ),
        ] {
            ensure_digest(field, digest)?;
        }
        if self.rework_cycle < 2 {
            return Err(PaperRaidFinalityValidationError::ReworkCycleTooLow);
        }
        for (field, same) in [
            (
                "submission_id",
                self.rejected_submission_id == self.replacement_submission_id,
            ),
            (
                "revision_id",
                self.rejected_revision_id == self.replacement_revision_id,
            ),
            (
                "release_candidate_hash",
                self.rejected_release_candidate_hash == self.replacement_release_candidate_hash,
            ),
            (
                "paper_bundle_hash",
                self.rejected_paper_bundle_hash == self.replacement_paper_bundle_hash,
            ),
            (
                "rework_content_commitment_sha256",
                self.rejected_rework_content_commitment_sha256
                    == self.replacement_rework_content_commitment_sha256,
            ),
        ] {
            if same {
                return Err(PaperRaidFinalityValidationError::ReworkPairMustDiffer(
                    field,
                ));
            }
        }
        Ok(())
    }

    pub fn from_canonical_bytes(
        bytes: &[u8],
    ) -> Result<Self, PaperRaidFinalityCommitmentDecodeError> {
        let mut decoder = Decoder::new(bytes);
        let lineage = decode_rework_lineage_v1(&mut decoder)?;
        decoder.finish()?;
        lineage.validate()?;
        if lineage.canonical_bytes() != bytes {
            return Err(CanonicalDecodeError::NonCanonicalRoundTrip.into());
        }
        Ok(lineage)
    }
}

impl CanonicalCbor for PaperRaidReworkLineageV1 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(13);
        encoder.uint(PAPER_RAID_REWORK_LINEAGE_VERSION_V1 as u64);
        self.rework_id.encode_canonical(encoder);
        encoder.uint(self.rework_cycle);
        self.rejected_submission_id.encode_canonical(encoder);
        self.replacement_submission_id.encode_canonical(encoder);
        self.rejected_revision_id.encode_canonical(encoder);
        self.replacement_revision_id.encode_canonical(encoder);
        encoder.bytes(&self.rejected_release_candidate_hash);
        encoder.bytes(&self.replacement_release_candidate_hash);
        encoder.bytes(&self.rejected_paper_bundle_hash);
        encoder.bytes(&self.replacement_paper_bundle_hash);
        encoder.bytes(&self.rejected_rework_content_commitment_sha256);
        encoder.bytes(&self.replacement_rework_content_commitment_sha256);
    }
}

/// Paper Raid finality V4 preserves every V3 fact and optionally carries a
/// rework lineage. `None` is the original-submission path; `Some` binds the
/// rejected and replacement facts exactly.
pub const PAPER_RAID_FINALITY_COMMITMENT_VERSION_V4: u8 = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaperRaidFinalityCommitmentV4 {
    pub commitment_id: ExternalKey,
    pub paper_project_id: ExternalKey,
    pub submission_id: ExternalKey,
    pub match_evidence_ref: ObjectRefV1,
    pub release_candidate_hash: Digest32,
    pub paper_bundle_hash: Digest32,
    pub submission_commitment_hash: Digest32,
    pub author_consent_set_hash: Digest32,
    pub tolerance_policy_hash: Digest32,
    pub evaluation_id: ExternalKey,
    pub evaluation_hash: Digest32,
    pub evaluation_score_bps: u16,
    pub evaluation_accepted: bool,
    pub evaluation_completed_at_unix_s: u64,
    pub latest_reproduction_id: ExternalKey,
    pub latest_reproduction_hash: Digest32,
    pub latest_reproduction_accepted: bool,
    pub latest_reproduction_completed_at_unix_s: u64,
    pub evaluation_supersedes: Option<ExternalKey>,
    pub evaluation_superseded_by: Option<ExternalKey>,
    pub reproduction_superseded_by: Option<ExternalKey>,
    pub appeal_status: PaperRaidAppealStatusV3,
    pub appeal_id: Option<ExternalKey>,
    pub appealed_evaluation_id: Option<ExternalKey>,
    pub appeal_resolution_hash: Option<Digest32>,
    pub appeal_window_closes_at_unix_s: u64,
    pub settlement_policy_hash: Digest32,
    pub scientific_finality: bool,
    pub score_eligible: bool,
    pub ranking_eligible: bool,
    pub reward_eligible: bool,
    pub economic_eligible: bool,
    pub finalized_at_unix_s: u64,
    pub rework_lineage: Option<PaperRaidReworkLineageV1>,
}

impl PaperRaidFinalityCommitmentV4 {
    pub fn validate(&self) -> Result<(), PaperRaidFinalityValidationError> {
        self.v3_facts().validate()?;
        if let Some(lineage) = &self.rework_lineage {
            lineage.validate()?;
            for (field, bound) in [
                (
                    "replacement_submission_id",
                    lineage.replacement_submission_id == self.submission_id,
                ),
                (
                    "replacement_release_candidate_hash",
                    lineage.replacement_release_candidate_hash == self.release_candidate_hash,
                ),
                (
                    "replacement_paper_bundle_hash",
                    lineage.replacement_paper_bundle_hash == self.paper_bundle_hash,
                ),
            ] {
                if !bound {
                    return Err(PaperRaidFinalityValidationError::ReworkReplacementMismatch(
                        field,
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn from_canonical_bytes(
        bytes: &[u8],
    ) -> Result<Self, PaperRaidFinalityCommitmentDecodeError> {
        let mut decoder = Decoder::new(bytes);
        let commitment = decode_commitment_v4(&mut decoder).map_err(|error| match error {
            CanonicalDecodeError::UnknownDiscriminant {
                name: "evaluation_score_bps",
                value,
            } => PaperRaidFinalityCommitmentDecodeError::ScoreOutOfRange(value),
            error => error.into(),
        })?;
        decoder.finish()?;
        commitment.validate()?;
        if commitment.canonical_bytes() != bytes {
            return Err(CanonicalDecodeError::NonCanonicalRoundTrip.into());
        }
        Ok(commitment)
    }

    fn v3_facts(&self) -> PaperRaidFinalityCommitmentV3 {
        PaperRaidFinalityCommitmentV3 {
            commitment_id: self.commitment_id,
            paper_project_id: self.paper_project_id,
            submission_id: self.submission_id,
            match_evidence_ref: self.match_evidence_ref,
            release_candidate_hash: self.release_candidate_hash,
            paper_bundle_hash: self.paper_bundle_hash,
            submission_commitment_hash: self.submission_commitment_hash,
            author_consent_set_hash: self.author_consent_set_hash,
            tolerance_policy_hash: self.tolerance_policy_hash,
            evaluation_id: self.evaluation_id,
            evaluation_hash: self.evaluation_hash,
            evaluation_score_bps: self.evaluation_score_bps,
            evaluation_accepted: self.evaluation_accepted,
            evaluation_completed_at_unix_s: self.evaluation_completed_at_unix_s,
            latest_reproduction_id: self.latest_reproduction_id,
            latest_reproduction_hash: self.latest_reproduction_hash,
            latest_reproduction_accepted: self.latest_reproduction_accepted,
            latest_reproduction_completed_at_unix_s: self.latest_reproduction_completed_at_unix_s,
            evaluation_supersedes: self.evaluation_supersedes,
            evaluation_superseded_by: self.evaluation_superseded_by,
            reproduction_superseded_by: self.reproduction_superseded_by,
            appeal_status: self.appeal_status,
            appeal_id: self.appeal_id,
            appealed_evaluation_id: self.appealed_evaluation_id,
            appeal_resolution_hash: self.appeal_resolution_hash,
            appeal_window_closes_at_unix_s: self.appeal_window_closes_at_unix_s,
            settlement_policy_hash: self.settlement_policy_hash,
            scientific_finality: self.scientific_finality,
            score_eligible: self.score_eligible,
            ranking_eligible: self.ranking_eligible,
            reward_eligible: self.reward_eligible,
            economic_eligible: self.economic_eligible,
            finalized_at_unix_s: self.finalized_at_unix_s,
        }
    }
}

impl CanonicalCbor for PaperRaidFinalityCommitmentV4 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(35);
        encoder.uint(PAPER_RAID_FINALITY_COMMITMENT_VERSION_V4 as u64);
        self.commitment_id.encode_canonical(encoder);
        self.paper_project_id.encode_canonical(encoder);
        self.submission_id.encode_canonical(encoder);
        self.match_evidence_ref.encode_canonical(encoder);
        encoder.bytes(&self.release_candidate_hash);
        encoder.bytes(&self.paper_bundle_hash);
        encoder.bytes(&self.submission_commitment_hash);
        encoder.bytes(&self.author_consent_set_hash);
        encoder.bytes(&self.tolerance_policy_hash);
        self.evaluation_id.encode_canonical(encoder);
        encoder.bytes(&self.evaluation_hash);
        encoder.uint(self.evaluation_score_bps as u64);
        encoder.bool(self.evaluation_accepted);
        encoder.uint(self.evaluation_completed_at_unix_s);
        self.latest_reproduction_id.encode_canonical(encoder);
        encoder.bytes(&self.latest_reproduction_hash);
        encoder.bool(self.latest_reproduction_accepted);
        encoder.uint(self.latest_reproduction_completed_at_unix_s);
        encode_option_key(encoder, self.evaluation_supersedes);
        encode_option_key(encoder, self.evaluation_superseded_by);
        encode_option_key(encoder, self.reproduction_superseded_by);
        self.appeal_status.encode_canonical(encoder);
        encode_option_key(encoder, self.appeal_id);
        encode_option_key(encoder, self.appealed_evaluation_id);
        encode_option_digest(encoder, &self.appeal_resolution_hash);
        encoder.uint(self.appeal_window_closes_at_unix_s);
        encoder.bytes(&self.settlement_policy_hash);
        encoder.bool(self.scientific_finality);
        encoder.bool(self.score_eligible);
        encoder.bool(self.ranking_eligible);
        encoder.bool(self.reward_eligible);
        encoder.bool(self.economic_eligible);
        encoder.uint(self.finalized_at_unix_s);
        match &self.rework_lineage {
            Some(lineage) => lineage.encode_canonical(encoder),
            None => encoder.null(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedPaperRaidFinalityCommandV4 {
    pub chain_id: String,
    pub command_id: ExternalKey,
    pub signer_did: String,
    pub signer_role: AuthorityRole,
    pub nonce: u64,
    pub public_key: [u8; 32],
    pub commitment: PaperRaidFinalityCommitmentV4,
    pub signature: Vec<u8>,
}

impl SignedPaperRaidFinalityCommandV4 {
    #[allow(clippy::too_many_arguments)]
    pub fn sign(
        chain_id: String,
        command_id: ExternalKey,
        signer_did: String,
        nonce: u64,
        commitment: PaperRaidFinalityCommitmentV4,
        signing_key: &SigningKey,
    ) -> Result<Self, SignedPaperRaidFinalityCommandValidationError> {
        let mut signed = Self {
            chain_id,
            command_id,
            signer_did,
            signer_role: AuthorityRole::HeptaAuthority,
            nonce,
            public_key: signing_key.verifying_key().to_bytes(),
            commitment,
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
        encoder.uint(PAPER_RAID_FINALITY_COMMITMENT_VERSION_V4 as u64);
        encoder.text(CANONICAL_ENCODING);
        encoder.text(&self.chain_id);
        self.command_id.encode_canonical(&mut encoder);
        encoder.text(&self.signer_did);
        self.signer_role.encode_canonical(&mut encoder);
        encoder.uint(self.nonce);
        encoder.bytes(&self.public_key);
        self.commitment.encode_canonical(&mut encoder);
        encoder.finish()
    }

    pub fn from_canonical_bytes(
        bytes: &[u8],
    ) -> Result<Self, SignedPaperRaidFinalityCommandValidationError> {
        let mut decoder = Decoder::new(bytes);
        decoder.array(10)?;
        let version = decoder.uint()?;
        if version != PAPER_RAID_FINALITY_COMMITMENT_VERSION_V4 as u64 {
            return Err(CanonicalDecodeError::UnsupportedVersion(version).into());
        }
        if decoder.text()? != CANONICAL_ENCODING {
            return Err(SignedPaperRaidFinalityCommandValidationError::EncodingMismatch);
        }
        let signed = Self {
            chain_id: decoder.text()?,
            command_id: decode_key(&mut decoder)?,
            signer_did: decoder.text()?,
            signer_role: decode_authority_role(&mut decoder)?,
            nonce: decoder.uint()?,
            public_key: decoder.bytes_exact()?,
            commitment: decode_commitment_v4(&mut decoder)?,
            signature: decoder.bytes()?.to_vec(),
        };
        decoder.finish()?;
        if signed.canonical_bytes() != bytes {
            return Err(CanonicalDecodeError::NonCanonicalRoundTrip.into());
        }
        signed.validate()?;
        Ok(signed)
    }

    pub fn validate(&self) -> Result<(), SignedPaperRaidFinalityCommandValidationError> {
        self.validate_unsigned()?;
        verify_signature(&self.public_key, &self.signature, &self.signing_bytes())
    }

    pub fn command_fingerprint(&self) -> Digest32 {
        canonical_hash(
            "trnm-paper-raid-finality-command-fingerprint-v4",
            &self.signing_bytes(),
        )
    }

    pub fn payload_hash(&self) -> Digest32 {
        self.commitment
            .canonical_hash("trnm-paper-raid-finality-commitment-v4")
    }

    fn validate_unsigned(&self) -> Result<(), SignedPaperRaidFinalityCommandValidationError> {
        validate_signed_command_fields(
            &self.chain_id,
            self.command_id,
            &self.signer_did,
            self.signer_role,
            self.nonce,
            &self.public_key,
        )?;
        self.commitment.validate()?;
        Ok(())
    }
}

impl CanonicalCbor for SignedPaperRaidFinalityCommandV4 {
    fn encode_canonical(&self, encoder: &mut Encoder) {
        encoder.array(10);
        encoder.uint(PAPER_RAID_FINALITY_COMMITMENT_VERSION_V4 as u64);
        encoder.text(CANONICAL_ENCODING);
        encoder.text(&self.chain_id);
        self.command_id.encode_canonical(encoder);
        encoder.text(&self.signer_did);
        self.signer_role.encode_canonical(encoder);
        encoder.uint(self.nonce);
        encoder.bytes(&self.public_key);
        self.commitment.encode_canonical(encoder);
        encoder.bytes(&self.signature);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PaperRaidFinalityValidationError {
    #[error("{0} must be a non-zero ExternalKey")]
    ZeroExternalKey(&'static str),
    #[error("{0} must be a non-zero digest")]
    ZeroDigest(&'static str),
    #[error("match_evidence_ref must reference a positive-version MatchEvidence object")]
    InvalidMatchEvidenceRef,
    #[error("evaluation and reproduction decisions are inconsistent")]
    InconsistentEvaluation,
    #[error("Paper Raid finality timestamps regress")]
    TimestampRegression,
    #[error("superseded evaluation cannot be finalized")]
    SupersededEvaluation,
    #[error("superseded reproduction cannot be finalized")]
    SupersededReproduction,
    #[error("appeal window is still open")]
    AppealStillOpen,
    #[error("appeal identity, resolution, and outcome are inconsistent")]
    InconsistentAppeal,
    #[error("scientific_finality must be true for a finality commitment")]
    ScientificFinalityRequired,
    #[error("eligibility cannot be granted before accepted final facts")]
    EligibilityWithoutFinalFacts,
    #[error("ranking, score, reward, and economic eligibility are inconsistent")]
    InconsistentEligibility,
    #[error("rework cycle must be at least 2")]
    ReworkCycleTooLow,
    #[error("rework rejected and replacement {0} must differ")]
    ReworkPairMustDiffer(&'static str),
    #[error("rework lineage {0} does not bind the finalized replacement")]
    ReworkReplacementMismatch(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PaperRaidFinalityCommitmentDecodeError {
    #[error(transparent)]
    Canonical(#[from] CanonicalDecodeError),
    #[error(transparent)]
    Validation(#[from] PaperRaidFinalityValidationError),
    #[error("evaluation_score_bps {0} exceeds u16")]
    ScoreOutOfRange(u64),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SignedPaperRaidFinalityCommandValidationError {
    #[error(transparent)]
    Canonical(#[from] CanonicalDecodeError),
    #[error(transparent)]
    Commitment(#[from] PaperRaidFinalityValidationError),
    #[error("unsupported canonical encoding")]
    EncodingMismatch,
    #[error("chain_id must be canonical lowercase ASCII")]
    InvalidChainId,
    #[error("signer_did must be a canonical did:* token")]
    InvalidSignerDid,
    #[error("Paper Raid finality must be signed by a Hepta authority")]
    HeptaAuthorityRequired,
    #[error("nonce must be positive")]
    ZeroNonce,
    #[error("invalid Ed25519 public key")]
    InvalidPublicKey,
    #[error("invalid Ed25519 signature")]
    InvalidSignature,
}

fn ensure_key(
    field: &'static str,
    key: ExternalKey,
) -> Result<(), PaperRaidFinalityValidationError> {
    if key.as_bytes() == &[0; 32] {
        Err(PaperRaidFinalityValidationError::ZeroExternalKey(field))
    } else {
        Ok(())
    }
}

fn ensure_digest(
    field: &'static str,
    digest: &Digest32,
) -> Result<(), PaperRaidFinalityValidationError> {
    if digest == &[0; 32] {
        Err(PaperRaidFinalityValidationError::ZeroDigest(field))
    } else {
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_common_fields(
    commitment_id: ExternalKey,
    paper_project_id: ExternalKey,
    submission_id: ExternalKey,
    match_evidence_ref: ObjectRefV1,
    release_candidate_hash: &Digest32,
    paper_bundle_hash: &Digest32,
    submission_commitment_hash: &Digest32,
    author_consent_set_hash: &Digest32,
    tolerance_policy_hash: &Digest32,
    evaluation_id: ExternalKey,
    evaluation_hash: &Digest32,
    evaluation_score_bps: u16,
    evaluation_accepted: bool,
    evaluation_completed_at_unix_s: u64,
    latest_reproduction_id: ExternalKey,
    latest_reproduction_hash: &Digest32,
    latest_reproduction_accepted: bool,
    latest_reproduction_completed_at_unix_s: u64,
    appeal_window_closes_at_unix_s: u64,
    settlement_policy_hash: &Digest32,
    finalized_at_unix_s: u64,
    reject_accepted_reproduction_after_rejected_evaluation: bool,
) -> Result<(), PaperRaidFinalityValidationError> {
    for (field, key) in [
        ("commitment_id", commitment_id),
        ("paper_project_id", paper_project_id),
        ("submission_id", submission_id),
        ("evaluation_id", evaluation_id),
        ("latest_reproduction_id", latest_reproduction_id),
    ] {
        ensure_key(field, key)?;
    }
    ensure_key("match_evidence_ref.key", match_evidence_ref.key)?;
    if match_evidence_ref.kind != ResearchObjectKind::MatchEvidence
        || match_evidence_ref.object_version == 0
    {
        return Err(PaperRaidFinalityValidationError::InvalidMatchEvidenceRef);
    }
    for (field, digest) in [
        ("release_candidate_hash", release_candidate_hash),
        ("paper_bundle_hash", paper_bundle_hash),
        ("submission_commitment_hash", submission_commitment_hash),
        ("author_consent_set_hash", author_consent_set_hash),
        ("tolerance_policy_hash", tolerance_policy_hash),
        ("evaluation_hash", evaluation_hash),
        ("latest_reproduction_hash", latest_reproduction_hash),
        ("settlement_policy_hash", settlement_policy_hash),
    ] {
        ensure_digest(field, digest)?;
    }
    if evaluation_score_bps > 10_000
        || (evaluation_accepted && evaluation_score_bps == 0)
        || (reject_accepted_reproduction_after_rejected_evaluation
            && !evaluation_accepted
            && latest_reproduction_accepted)
    {
        return Err(PaperRaidFinalityValidationError::InconsistentEvaluation);
    }
    if evaluation_completed_at_unix_s == 0
        || latest_reproduction_completed_at_unix_s < evaluation_completed_at_unix_s
        || appeal_window_closes_at_unix_s < latest_reproduction_completed_at_unix_s
        || finalized_at_unix_s < appeal_window_closes_at_unix_s
    {
        return Err(PaperRaidFinalityValidationError::TimestampRegression);
    }
    Ok(())
}

fn validate_finality_and_eligibility(
    scientific_finality: bool,
    eligibility_base: bool,
    score_eligible: bool,
    ranking_eligible: bool,
    reward_eligible: bool,
    economic_eligible: bool,
) -> Result<(), PaperRaidFinalityValidationError> {
    if !scientific_finality {
        return Err(PaperRaidFinalityValidationError::ScientificFinalityRequired);
    }
    if (score_eligible || ranking_eligible || reward_eligible || economic_eligible)
        && !eligibility_base
    {
        return Err(PaperRaidFinalityValidationError::EligibilityWithoutFinalFacts);
    }
    if ranking_eligible && !score_eligible {
        return Err(PaperRaidFinalityValidationError::InconsistentEligibility);
    }
    if reward_eligible && !(ranking_eligible && score_eligible && economic_eligible) {
        return Err(PaperRaidFinalityValidationError::InconsistentEligibility);
    }
    if economic_eligible && !reward_eligible {
        return Err(PaperRaidFinalityValidationError::InconsistentEligibility);
    }
    Ok(())
}

fn encode_option_key(encoder: &mut Encoder, key: Option<ExternalKey>) {
    match key {
        Some(key) => key.encode_canonical(encoder),
        None => encoder.null(),
    }
}

fn encode_option_digest(encoder: &mut Encoder, digest: &Option<Digest32>) {
    match digest {
        Some(digest) => encoder.bytes(digest),
        None => encoder.null(),
    }
}

fn decode_key(decoder: &mut Decoder<'_>) -> Result<ExternalKey, CanonicalDecodeError> {
    Ok(ExternalKey::from_bytes(decoder.bytes_exact()?))
}

fn decode_option_key(
    decoder: &mut Decoder<'_>,
) -> Result<Option<ExternalKey>, CanonicalDecodeError> {
    if decoder.consume_null() {
        Ok(None)
    } else {
        decode_key(decoder).map(Some)
    }
}

fn decode_option_digest(
    decoder: &mut Decoder<'_>,
) -> Result<Option<Digest32>, CanonicalDecodeError> {
    if decoder.consume_null() {
        Ok(None)
    } else {
        decoder.bytes_exact().map(Some)
    }
}

fn decode_object_ref(decoder: &mut Decoder<'_>) -> Result<ObjectRefV1, CanonicalDecodeError> {
    decoder.array(4)?;
    let version = decoder.uint()?;
    if version != crate::types::PROTOCOL_VERSION as u64 {
        return Err(CanonicalDecodeError::UnsupportedVersion(version));
    }
    let kind = match decoder.uint()? {
        1 => ResearchObjectKind::MatchEvidence,
        2 => ResearchObjectKind::EvaluationCommitment,
        3 => ResearchObjectKind::WorkloadReceipt,
        4 => ResearchObjectKind::ResearchClaim,
        5 => ResearchObjectKind::LicenseDeclaration,
        6 => ResearchObjectKind::ClaimChallenge,
        7 => ResearchObjectKind::ClaimResolution,
        value => {
            return Err(CanonicalDecodeError::UnknownDiscriminant {
                name: "ResearchObjectKind",
                value,
            })
        }
    };
    Ok(ObjectRefV1 {
        kind,
        key: decode_key(decoder)?,
        object_version: decoder.uint()?,
    })
}

fn decode_appeal_status_v2(
    decoder: &mut Decoder<'_>,
) -> Result<PaperRaidAppealStatusV2, CanonicalDecodeError> {
    match decoder.uint()? {
        1 => Ok(PaperRaidAppealStatusV2::Open),
        2 => Ok(PaperRaidAppealStatusV2::ClosedNoAppeal),
        3 => Ok(PaperRaidAppealStatusV2::ResolvedUpheld),
        4 => Ok(PaperRaidAppealStatusV2::ResolvedOverturned),
        value => Err(CanonicalDecodeError::UnknownDiscriminant {
            name: "PaperRaidAppealStatusV2",
            value,
        }),
    }
}

fn decode_commitment_v2(
    decoder: &mut Decoder<'_>,
) -> Result<PaperRaidFinalityCommitmentV2, CanonicalDecodeError> {
    decoder.array(32)?;
    let version = decoder.uint()?;
    if version != PAPER_RAID_FINALITY_COMMITMENT_VERSION_V2 as u64 {
        return Err(CanonicalDecodeError::UnsupportedVersion(version));
    }
    Ok(PaperRaidFinalityCommitmentV2 {
        commitment_id: decode_key(decoder)?,
        paper_project_id: decode_key(decoder)?,
        submission_id: decode_key(decoder)?,
        match_evidence_ref: decode_object_ref(decoder)?,
        release_candidate_hash: decoder.bytes_exact()?,
        paper_bundle_hash: decoder.bytes_exact()?,
        submission_commitment_hash: decoder.bytes_exact()?,
        author_consent_set_hash: decoder.bytes_exact()?,
        tolerance_policy_hash: decoder.bytes_exact()?,
        evaluation_id: decode_key(decoder)?,
        evaluation_hash: decoder.bytes_exact()?,
        evaluation_score_bps: {
            let value = decoder.uint()?;
            u16::try_from(value).map_err(|_| CanonicalDecodeError::UnknownDiscriminant {
                name: "evaluation_score_bps",
                value,
            })?
        },
        evaluation_accepted: decoder.bool()?,
        evaluation_completed_at_unix_s: decoder.uint()?,
        latest_reproduction_id: decode_key(decoder)?,
        latest_reproduction_hash: decoder.bytes_exact()?,
        latest_reproduction_accepted: decoder.bool()?,
        latest_reproduction_completed_at_unix_s: decoder.uint()?,
        evaluation_superseded_by: decode_option_key(decoder)?,
        reproduction_superseded_by: decode_option_key(decoder)?,
        appeal_status: decode_appeal_status_v2(decoder)?,
        appeal_id: decode_option_key(decoder)?,
        appeal_resolution_hash: decode_option_digest(decoder)?,
        appeal_window_closes_at_unix_s: decoder.uint()?,
        settlement_policy_hash: decoder.bytes_exact()?,
        scientific_finality: decoder.bool()?,
        score_eligible: decoder.bool()?,
        ranking_eligible: decoder.bool()?,
        reward_eligible: decoder.bool()?,
        economic_eligible: decoder.bool()?,
        finalized_at_unix_s: decoder.uint()?,
    })
}

fn decode_appeal_status_v3(
    decoder: &mut Decoder<'_>,
) -> Result<PaperRaidAppealStatusV3, CanonicalDecodeError> {
    match decoder.uint()? {
        1 => Ok(PaperRaidAppealStatusV3::Open),
        2 => Ok(PaperRaidAppealStatusV3::ClosedNoAppeal),
        3 => Ok(PaperRaidAppealStatusV3::ResolvedDenied),
        4 => Ok(PaperRaidAppealStatusV3::ResolvedUpheld),
        value => Err(CanonicalDecodeError::UnknownDiscriminant {
            name: "PaperRaidAppealStatusV3",
            value,
        }),
    }
}

fn decode_commitment_v3(
    decoder: &mut Decoder<'_>,
) -> Result<PaperRaidFinalityCommitmentV3, CanonicalDecodeError> {
    decoder.array(34)?;
    let version = decoder.uint()?;
    if version != PAPER_RAID_FINALITY_COMMITMENT_VERSION_V3 as u64 {
        return Err(CanonicalDecodeError::UnsupportedVersion(version));
    }
    Ok(PaperRaidFinalityCommitmentV3 {
        commitment_id: decode_key(decoder)?,
        paper_project_id: decode_key(decoder)?,
        submission_id: decode_key(decoder)?,
        match_evidence_ref: decode_object_ref(decoder)?,
        release_candidate_hash: decoder.bytes_exact()?,
        paper_bundle_hash: decoder.bytes_exact()?,
        submission_commitment_hash: decoder.bytes_exact()?,
        author_consent_set_hash: decoder.bytes_exact()?,
        tolerance_policy_hash: decoder.bytes_exact()?,
        evaluation_id: decode_key(decoder)?,
        evaluation_hash: decoder.bytes_exact()?,
        evaluation_score_bps: {
            let value = decoder.uint()?;
            u16::try_from(value).map_err(|_| CanonicalDecodeError::UnknownDiscriminant {
                name: "evaluation_score_bps",
                value,
            })?
        },
        evaluation_accepted: decoder.bool()?,
        evaluation_completed_at_unix_s: decoder.uint()?,
        latest_reproduction_id: decode_key(decoder)?,
        latest_reproduction_hash: decoder.bytes_exact()?,
        latest_reproduction_accepted: decoder.bool()?,
        latest_reproduction_completed_at_unix_s: decoder.uint()?,
        evaluation_supersedes: decode_option_key(decoder)?,
        evaluation_superseded_by: decode_option_key(decoder)?,
        reproduction_superseded_by: decode_option_key(decoder)?,
        appeal_status: decode_appeal_status_v3(decoder)?,
        appeal_id: decode_option_key(decoder)?,
        appealed_evaluation_id: decode_option_key(decoder)?,
        appeal_resolution_hash: decode_option_digest(decoder)?,
        appeal_window_closes_at_unix_s: decoder.uint()?,
        settlement_policy_hash: decoder.bytes_exact()?,
        scientific_finality: decoder.bool()?,
        score_eligible: decoder.bool()?,
        ranking_eligible: decoder.bool()?,
        reward_eligible: decoder.bool()?,
        economic_eligible: decoder.bool()?,
        finalized_at_unix_s: decoder.uint()?,
    })
}

fn decode_rework_lineage_v1(
    decoder: &mut Decoder<'_>,
) -> Result<PaperRaidReworkLineageV1, CanonicalDecodeError> {
    decoder.array(13)?;
    let version = decoder.uint()?;
    if version != PAPER_RAID_REWORK_LINEAGE_VERSION_V1 as u64 {
        return Err(CanonicalDecodeError::UnsupportedVersion(version));
    }
    Ok(PaperRaidReworkLineageV1 {
        rework_id: decode_key(decoder)?,
        rework_cycle: decoder.uint()?,
        rejected_submission_id: decode_key(decoder)?,
        replacement_submission_id: decode_key(decoder)?,
        rejected_revision_id: decode_key(decoder)?,
        replacement_revision_id: decode_key(decoder)?,
        rejected_release_candidate_hash: decoder.bytes_exact()?,
        replacement_release_candidate_hash: decoder.bytes_exact()?,
        rejected_paper_bundle_hash: decoder.bytes_exact()?,
        replacement_paper_bundle_hash: decoder.bytes_exact()?,
        rejected_rework_content_commitment_sha256: decoder.bytes_exact()?,
        replacement_rework_content_commitment_sha256: decoder.bytes_exact()?,
    })
}

fn decode_commitment_v4(
    decoder: &mut Decoder<'_>,
) -> Result<PaperRaidFinalityCommitmentV4, CanonicalDecodeError> {
    decoder.array(35)?;
    let version = decoder.uint()?;
    if version != PAPER_RAID_FINALITY_COMMITMENT_VERSION_V4 as u64 {
        return Err(CanonicalDecodeError::UnsupportedVersion(version));
    }
    Ok(PaperRaidFinalityCommitmentV4 {
        commitment_id: decode_key(decoder)?,
        paper_project_id: decode_key(decoder)?,
        submission_id: decode_key(decoder)?,
        match_evidence_ref: decode_object_ref(decoder)?,
        release_candidate_hash: decoder.bytes_exact()?,
        paper_bundle_hash: decoder.bytes_exact()?,
        submission_commitment_hash: decoder.bytes_exact()?,
        author_consent_set_hash: decoder.bytes_exact()?,
        tolerance_policy_hash: decoder.bytes_exact()?,
        evaluation_id: decode_key(decoder)?,
        evaluation_hash: decoder.bytes_exact()?,
        evaluation_score_bps: {
            let value = decoder.uint()?;
            u16::try_from(value).map_err(|_| CanonicalDecodeError::UnknownDiscriminant {
                name: "evaluation_score_bps",
                value,
            })?
        },
        evaluation_accepted: decoder.bool()?,
        evaluation_completed_at_unix_s: decoder.uint()?,
        latest_reproduction_id: decode_key(decoder)?,
        latest_reproduction_hash: decoder.bytes_exact()?,
        latest_reproduction_accepted: decoder.bool()?,
        latest_reproduction_completed_at_unix_s: decoder.uint()?,
        evaluation_supersedes: decode_option_key(decoder)?,
        evaluation_superseded_by: decode_option_key(decoder)?,
        reproduction_superseded_by: decode_option_key(decoder)?,
        appeal_status: decode_appeal_status_v3(decoder)?,
        appeal_id: decode_option_key(decoder)?,
        appealed_evaluation_id: decode_option_key(decoder)?,
        appeal_resolution_hash: decode_option_digest(decoder)?,
        appeal_window_closes_at_unix_s: decoder.uint()?,
        settlement_policy_hash: decoder.bytes_exact()?,
        scientific_finality: decoder.bool()?,
        score_eligible: decoder.bool()?,
        ranking_eligible: decoder.bool()?,
        reward_eligible: decoder.bool()?,
        economic_eligible: decoder.bool()?,
        finalized_at_unix_s: decoder.uint()?,
        rework_lineage: if decoder.consume_null() {
            None
        } else {
            Some(decode_rework_lineage_v1(decoder)?)
        },
    })
}

fn decode_authority_role(decoder: &mut Decoder<'_>) -> Result<AuthorityRole, CanonicalDecodeError> {
    match decoder.uint()? {
        1 => Ok(AuthorityRole::NakamaAuthority),
        2 => Ok(AuthorityRole::HeptaAuthority),
        value => Err(CanonicalDecodeError::UnknownDiscriminant {
            name: "AuthorityRole",
            value,
        }),
    }
}

fn verify_signature(
    public_key: &[u8; 32],
    signature: &[u8],
    signing_bytes: &[u8],
) -> Result<(), SignedPaperRaidFinalityCommandValidationError> {
    let signature_bytes: [u8; 64] = signature
        .try_into()
        .map_err(|_| SignedPaperRaidFinalityCommandValidationError::InvalidSignature)?;
    let verifying_key = VerifyingKey::from_bytes(public_key)
        .map_err(|_| SignedPaperRaidFinalityCommandValidationError::InvalidPublicKey)?;
    verifying_key
        .verify_strict(signing_bytes, &Signature::from_bytes(&signature_bytes))
        .map_err(|_| SignedPaperRaidFinalityCommandValidationError::InvalidSignature)
}

fn validate_signed_command_fields(
    chain_id: &str,
    command_id: ExternalKey,
    signer_did: &str,
    signer_role: AuthorityRole,
    nonce: u64,
    public_key: &[u8; 32],
) -> Result<(), SignedPaperRaidFinalityCommandValidationError> {
    validate_chain_id(chain_id)?;
    ensure_key("command_id", command_id)?;
    validate_signer_did(signer_did)?;
    if signer_role != AuthorityRole::HeptaAuthority {
        return Err(SignedPaperRaidFinalityCommandValidationError::HeptaAuthorityRequired);
    }
    if nonce == 0 {
        return Err(SignedPaperRaidFinalityCommandValidationError::ZeroNonce);
    }
    if public_key == &[0; 32] || VerifyingKey::from_bytes(public_key).is_err() {
        return Err(SignedPaperRaidFinalityCommandValidationError::InvalidPublicKey);
    }
    Ok(())
}

fn validate_chain_id(chain_id: &str) -> Result<(), SignedPaperRaidFinalityCommandValidationError> {
    if chain_id.is_empty()
        || chain_id.len() > 64
        || !chain_id.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'-' | b'_' | b'.' | b':')
        })
    {
        Err(SignedPaperRaidFinalityCommandValidationError::InvalidChainId)
    } else {
        Ok(())
    }
}

fn validate_signer_did(
    signer_did: &str,
) -> Result<(), SignedPaperRaidFinalityCommandValidationError> {
    if signer_did.len() < 5
        || signer_did.len() > 192
        || !signer_did.starts_with("did:")
        || !signer_did.bytes().all(|byte| byte.is_ascii_graphic())
    {
        Err(SignedPaperRaidFinalityCommandValidationError::InvalidSignerDid)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(byte: u8) -> ExternalKey {
        ExternalKey::from_bytes([byte; 32])
    }

    fn commitment_v2() -> PaperRaidFinalityCommitmentV2 {
        PaperRaidFinalityCommitmentV2 {
            commitment_id: key(1),
            paper_project_id: key(2),
            submission_id: key(3),
            match_evidence_ref: ObjectRefV1::new(ResearchObjectKind::MatchEvidence, key(4), 1),
            release_candidate_hash: [5; 32],
            paper_bundle_hash: [6; 32],
            submission_commitment_hash: [7; 32],
            author_consent_set_hash: [8; 32],
            tolerance_policy_hash: [9; 32],
            evaluation_id: key(10),
            evaluation_hash: [11; 32],
            evaluation_score_bps: 8_500,
            evaluation_accepted: true,
            evaluation_completed_at_unix_s: 100,
            latest_reproduction_id: key(12),
            latest_reproduction_hash: [13; 32],
            latest_reproduction_accepted: true,
            latest_reproduction_completed_at_unix_s: 110,
            evaluation_superseded_by: None,
            reproduction_superseded_by: None,
            appeal_status: PaperRaidAppealStatusV2::ClosedNoAppeal,
            appeal_id: None,
            appeal_resolution_hash: None,
            appeal_window_closes_at_unix_s: 120,
            settlement_policy_hash: [14; 32],
            scientific_finality: true,
            score_eligible: true,
            ranking_eligible: true,
            reward_eligible: true,
            economic_eligible: true,
            finalized_at_unix_s: 121,
        }
    }

    fn commitment_v3() -> PaperRaidFinalityCommitmentV3 {
        PaperRaidFinalityCommitmentV3 {
            commitment_id: key(1),
            paper_project_id: key(2),
            submission_id: key(3),
            match_evidence_ref: ObjectRefV1::new(ResearchObjectKind::MatchEvidence, key(4), 1),
            release_candidate_hash: [5; 32],
            paper_bundle_hash: [6; 32],
            submission_commitment_hash: [7; 32],
            author_consent_set_hash: [8; 32],
            tolerance_policy_hash: [9; 32],
            evaluation_id: key(10),
            evaluation_hash: [11; 32],
            evaluation_score_bps: 8_500,
            evaluation_accepted: true,
            evaluation_completed_at_unix_s: 100,
            latest_reproduction_id: key(12),
            latest_reproduction_hash: [13; 32],
            latest_reproduction_accepted: true,
            latest_reproduction_completed_at_unix_s: 110,
            evaluation_supersedes: None,
            evaluation_superseded_by: None,
            reproduction_superseded_by: None,
            appeal_status: PaperRaidAppealStatusV3::ClosedNoAppeal,
            appeal_id: None,
            appealed_evaluation_id: None,
            appeal_resolution_hash: None,
            appeal_window_closes_at_unix_s: 120,
            settlement_policy_hash: [14; 32],
            scientific_finality: true,
            score_eligible: true,
            ranking_eligible: true,
            reward_eligible: true,
            economic_eligible: true,
            finalized_at_unix_s: 121,
        }
    }

    fn commitment_v4() -> PaperRaidFinalityCommitmentV4 {
        let v3 = commitment_v3();
        PaperRaidFinalityCommitmentV4 {
            commitment_id: v3.commitment_id,
            paper_project_id: v3.paper_project_id,
            submission_id: v3.submission_id,
            match_evidence_ref: v3.match_evidence_ref,
            release_candidate_hash: v3.release_candidate_hash,
            paper_bundle_hash: v3.paper_bundle_hash,
            submission_commitment_hash: v3.submission_commitment_hash,
            author_consent_set_hash: v3.author_consent_set_hash,
            tolerance_policy_hash: v3.tolerance_policy_hash,
            evaluation_id: v3.evaluation_id,
            evaluation_hash: v3.evaluation_hash,
            evaluation_score_bps: v3.evaluation_score_bps,
            evaluation_accepted: v3.evaluation_accepted,
            evaluation_completed_at_unix_s: v3.evaluation_completed_at_unix_s,
            latest_reproduction_id: v3.latest_reproduction_id,
            latest_reproduction_hash: v3.latest_reproduction_hash,
            latest_reproduction_accepted: v3.latest_reproduction_accepted,
            latest_reproduction_completed_at_unix_s: v3.latest_reproduction_completed_at_unix_s,
            evaluation_supersedes: v3.evaluation_supersedes,
            evaluation_superseded_by: v3.evaluation_superseded_by,
            reproduction_superseded_by: v3.reproduction_superseded_by,
            appeal_status: v3.appeal_status,
            appeal_id: v3.appeal_id,
            appealed_evaluation_id: v3.appealed_evaluation_id,
            appeal_resolution_hash: v3.appeal_resolution_hash,
            appeal_window_closes_at_unix_s: v3.appeal_window_closes_at_unix_s,
            settlement_policy_hash: v3.settlement_policy_hash,
            scientific_finality: v3.scientific_finality,
            score_eligible: v3.score_eligible,
            ranking_eligible: v3.ranking_eligible,
            reward_eligible: v3.reward_eligible,
            economic_eligible: v3.economic_eligible,
            finalized_at_unix_s: v3.finalized_at_unix_s,
            rework_lineage: Some(PaperRaidReworkLineageV1 {
                rework_id: key(21),
                rework_cycle: 2,
                rejected_submission_id: key(22),
                replacement_submission_id: v3.submission_id,
                rejected_revision_id: key(23),
                replacement_revision_id: key(24),
                rejected_release_candidate_hash: [25; 32],
                replacement_release_candidate_hash: v3.release_candidate_hash,
                rejected_paper_bundle_hash: [26; 32],
                replacement_paper_bundle_hash: v3.paper_bundle_hash,
                rejected_rework_content_commitment_sha256: [27; 32],
                // Deliberately independent from submission_commitment_hash ([7; 32]).
                replacement_rework_content_commitment_sha256: [28; 32],
            }),
        }
    }

    #[test]
    fn frozen_v2_canonical_fixture_and_status_discriminants_are_stable() {
        let commitment = commitment_v2();
        commitment.validate().unwrap();
        let bytes = commitment.canonical_bytes();
        assert_eq!(
            PaperRaidFinalityCommitmentV2::from_canonical_bytes(&bytes).unwrap(),
            commitment
        );
        assert_eq!(
            hex::encode(&bytes),
            "9820025820010101010101010101010101010101010101010101010101010101010101010158200202020202020202020202020202020202020202020202020202020202020202582003030303030303030303030303030303030303030303030303030303030303038401015820040404040404040404040404040404040404040404040404040404040404040401582005050505050505050505050505050505050505050505050505050505050505055820060606060606060606060606060606060606060606060606060606060606060658200707070707070707070707070707070707070707070707070707070707070707582008080808080808080808080808080808080808080808080808080808080808085820090909090909090909090909090909090909090909090909090909090909090958200a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a58200b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b192134f5186458200c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c58200d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0df5186ef6f602f6f6187858200e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0ef5f5f5f5f51879"
        );
        assert_eq!(
            hex::encode(canonical_hash(
                "trnm-paper-raid-finality-commitment-v2",
                &bytes
            )),
            "17f7d45f96b24897723362045911e9913d0bffe7f31505e735c520a9f5729d46"
        );

        let mut status = Encoder::default();
        PaperRaidAppealStatusV2::ResolvedUpheld.encode_canonical(&mut status);
        assert_eq!(status.finish(), vec![3]);
        let mut status = Encoder::default();
        PaperRaidAppealStatusV2::ResolvedOverturned.encode_canonical(&mut status);
        assert_eq!(status.finish(), vec![4]);

        let mut trailing = bytes;
        trailing.push(0);
        assert!(PaperRaidFinalityCommitmentV2::from_canonical_bytes(&trailing).is_err());
    }

    #[test]
    fn frozen_v2_rejects_open_superseded_and_overturned_accepted_facts() {
        let mut value = commitment_v2();
        value.appeal_status = PaperRaidAppealStatusV2::Open;
        assert_eq!(
            value.validate(),
            Err(PaperRaidFinalityValidationError::AppealStillOpen)
        );

        let mut value = commitment_v2();
        value.evaluation_superseded_by = Some(key(15));
        assert_eq!(
            value.validate(),
            Err(PaperRaidFinalityValidationError::SupersededEvaluation)
        );

        let mut value = commitment_v2();
        value.reproduction_superseded_by = Some(key(16));
        assert_eq!(
            value.validate(),
            Err(PaperRaidFinalityValidationError::SupersededReproduction)
        );
        let mut value = commitment_v2();
        value.appeal_status = PaperRaidAppealStatusV2::ResolvedOverturned;
        value.appeal_id = Some(key(17));
        value.appeal_resolution_hash = Some([18; 32]);
        assert_eq!(
            value.validate(),
            Err(PaperRaidFinalityValidationError::InconsistentAppeal)
        );
    }

    #[test]
    fn frozen_v2_score_overflow_keeps_public_error_classification() {
        let commitment_bytes = commitment_v2().canonical_bytes();
        let score_offset = commitment_bytes
            .windows(3)
            .position(|window| window == [0x19, 0x21, 0x34])
            .expect("fixture contains the canonical 8,500 score");
        let mut overflow = commitment_bytes;
        overflow.splice(score_offset..score_offset + 3, [0x1a, 0, 1, 0, 0]);
        assert_eq!(
            PaperRaidFinalityCommitmentV2::from_canonical_bytes(&overflow),
            Err(PaperRaidFinalityCommitmentDecodeError::ScoreOutOfRange(
                65_536
            ))
        );

        let signing_key = SigningKey::from_bytes(&[0x42; 32]);
        let signed = SignedPaperRaidFinalityCommandV2::sign(
            "trnm-paper-raid-test".to_string(),
            key(19),
            "did:trnm:hepta-paper-raid".to_string(),
            7,
            commitment_v2(),
            &signing_key,
        )
        .unwrap();
        let mut signed_bytes = signed.canonical_bytes();
        let score_offset = signed_bytes
            .windows(3)
            .position(|window| window == [0x19, 0x21, 0x34])
            .expect("signed fixture contains the canonical 8,500 score");
        signed_bytes.splice(score_offset..score_offset + 3, [0x1a, 0, 1, 0, 0]);
        assert_eq!(
            SignedPaperRaidFinalityCommandV2::from_canonical_bytes(&signed_bytes),
            Err(SignedPaperRaidFinalityCommandValidationError::Canonical(
                CanonicalDecodeError::UnknownDiscriminant {
                    name: "evaluation_score_bps",
                    value: 65_536,
                }
            ))
        );
    }

    #[test]
    fn frozen_v2_signed_command_roundtrips_and_binds_every_field() {
        let signing_key = SigningKey::from_bytes(&[0x42; 32]);
        let signed = SignedPaperRaidFinalityCommandV2::sign(
            "trnm-paper-raid-test".to_string(),
            key(19),
            "did:trnm:hepta-paper-raid".to_string(),
            7,
            commitment_v2(),
            &signing_key,
        )
        .unwrap();
        let bytes = signed.canonical_bytes();
        assert_eq!(
            SignedPaperRaidFinalityCommandV2::from_canonical_bytes(&bytes).unwrap(),
            signed
        );

        let mut tampered = signed.clone();
        tampered.commitment.paper_bundle_hash = [0x77; 32];
        assert_eq!(
            tampered.validate(),
            Err(SignedPaperRaidFinalityCommandValidationError::InvalidSignature)
        );
        assert_ne!(tampered.command_fingerprint(), signed.command_fingerprint());
    }

    #[test]
    fn v3_canonical_roundtrip_binds_complete_extended_tuple() {
        let commitment = commitment_v3();
        commitment.validate().unwrap();
        let bytes = commitment.canonical_bytes();
        assert_eq!(
            PaperRaidFinalityCommitmentV3::from_canonical_bytes(&bytes).unwrap(),
            commitment
        );
        assert_ne!(bytes, commitment_v2().canonical_bytes());
    }

    #[test]
    fn v3_evaluation_and_reproduction_outcomes_are_independent_but_not_eligible() {
        let mut value = commitment_v3();
        value.evaluation_accepted = false;
        value.evaluation_score_bps = 0;
        value.latest_reproduction_accepted = true;
        value.score_eligible = false;
        value.ranking_eligible = false;
        value.reward_eligible = false;
        value.economic_eligible = false;
        value.validate().unwrap();

        value.score_eligible = true;
        assert_eq!(
            value.validate(),
            Err(PaperRaidFinalityValidationError::EligibilityWithoutFinalFacts)
        );
    }

    #[test]
    fn v3_denied_appeal_binds_the_same_final_evaluation() {
        let mut value = commitment_v3();
        value.appeal_status = PaperRaidAppealStatusV3::ResolvedDenied;
        value.appeal_id = Some(key(17));
        value.appealed_evaluation_id = Some(value.evaluation_id);
        value.appeal_resolution_hash = Some([18; 32]);
        value.validate().unwrap();

        value.evaluation_supersedes = Some(key(19));
        assert_eq!(
            value.validate(),
            Err(PaperRaidFinalityValidationError::InconsistentAppeal)
        );
    }

    #[test]
    fn v3_upheld_appeal_binds_replacement_to_the_appealed_evaluation() {
        let appealed_evaluation_id = key(10);
        let replacement_evaluation_id = key(19);
        let mut value = commitment_v3();
        value.evaluation_id = replacement_evaluation_id;
        value.evaluation_supersedes = Some(appealed_evaluation_id);
        value.appeal_status = PaperRaidAppealStatusV3::ResolvedUpheld;
        value.appeal_id = Some(key(17));
        value.appealed_evaluation_id = Some(appealed_evaluation_id);
        value.appeal_resolution_hash = Some([18; 32]);
        value.validate().unwrap();

        value.evaluation_supersedes = Some(key(20));
        assert_eq!(
            value.validate(),
            Err(PaperRaidFinalityValidationError::InconsistentAppeal)
        );
    }

    #[test]
    fn signed_v3_command_roundtrips_and_binds_every_finality_field() {
        let signing_key = SigningKey::from_bytes(&[0x42; 32]);
        let signed = SignedPaperRaidFinalityCommandV3::sign(
            "trnm-paper-raid-test".to_string(),
            key(19),
            "did:trnm:hepta-paper-raid".to_string(),
            7,
            commitment_v3(),
            &signing_key,
        )
        .unwrap();
        let bytes = signed.canonical_bytes();
        assert_eq!(
            SignedPaperRaidFinalityCommandV3::from_canonical_bytes(&bytes).unwrap(),
            signed
        );

        let mut tampered = signed.clone();
        tampered.commitment.paper_bundle_hash = [0x77; 32];
        assert_eq!(
            tampered.validate(),
            Err(SignedPaperRaidFinalityCommandValidationError::InvalidSignature)
        );
        assert_ne!(tampered.command_fingerprint(), signed.command_fingerprint());
    }

    #[test]
    fn signed_v3_command_rejects_non_hepta_role_before_signature_use() {
        let signing_key = SigningKey::from_bytes(&[0x43; 32]);
        let mut signed = SignedPaperRaidFinalityCommandV3::sign(
            "trnm-paper-raid-test".to_string(),
            key(20),
            "did:trnm:hepta-paper-raid".to_string(),
            8,
            commitment_v3(),
            &signing_key,
        )
        .unwrap();
        signed.signer_role = AuthorityRole::NakamaAuthority;
        assert_eq!(
            signed.validate(),
            Err(SignedPaperRaidFinalityCommandValidationError::HeptaAuthorityRequired)
        );
    }

    #[test]
    fn v4_lineage_is_exact_canonical_array_and_rejects_trailing_bytes() {
        let lineage = commitment_v4().rework_lineage.unwrap();
        lineage.validate().unwrap();
        let bytes = lineage.canonical_bytes();
        assert_eq!(bytes[0], 0x8d, "lineage must be a 13-item CBOR array");
        assert_eq!(
            PaperRaidReworkLineageV1::from_canonical_bytes(&bytes).unwrap(),
            lineage
        );
        let mut trailing = bytes;
        trailing.push(0);
        assert!(PaperRaidReworkLineageV1::from_canonical_bytes(&trailing).is_err());
    }

    #[test]
    fn v4_binds_only_the_three_top_level_replacement_identities() {
        let value = commitment_v4();
        assert_ne!(
            value
                .rework_lineage
                .as_ref()
                .unwrap()
                .replacement_rework_content_commitment_sha256,
            value.submission_commitment_hash,
            "scientific content commitment is not the aggregate submission commitment"
        );
        value.validate().unwrap();

        let mut mismatch = value.clone();
        mismatch
            .rework_lineage
            .as_mut()
            .unwrap()
            .replacement_submission_id = key(29);
        assert_eq!(
            mismatch.validate(),
            Err(PaperRaidFinalityValidationError::ReworkReplacementMismatch(
                "replacement_submission_id"
            ))
        );

        let mut mismatch = value.clone();
        mismatch
            .rework_lineage
            .as_mut()
            .unwrap()
            .replacement_release_candidate_hash = [29; 32];
        assert_eq!(
            mismatch.validate(),
            Err(PaperRaidFinalityValidationError::ReworkReplacementMismatch(
                "replacement_release_candidate_hash"
            ))
        );

        let mut mismatch = value;
        mismatch
            .rework_lineage
            .as_mut()
            .unwrap()
            .replacement_paper_bundle_hash = [29; 32];
        assert_eq!(
            mismatch.validate(),
            Err(PaperRaidFinalityValidationError::ReworkReplacementMismatch(
                "replacement_paper_bundle_hash"
            ))
        );
    }

    #[test]
    fn v4_rejects_invalid_cycle_zero_values_and_equal_pairs() {
        let mut value = commitment_v4();
        value.rework_lineage.as_mut().unwrap().rework_cycle = 1;
        assert_eq!(
            value.validate(),
            Err(PaperRaidFinalityValidationError::ReworkCycleTooLow)
        );

        let mut value = commitment_v4();
        value.rework_lineage.as_mut().unwrap().rework_id = key(0);
        assert_eq!(
            value.validate(),
            Err(PaperRaidFinalityValidationError::ZeroExternalKey(
                "rework_lineage.rework_id"
            ))
        );

        let mut value = commitment_v4();
        let lineage = value.rework_lineage.as_mut().unwrap();
        lineage.rejected_revision_id = lineage.replacement_revision_id;
        assert_eq!(
            value.validate(),
            Err(PaperRaidFinalityValidationError::ReworkPairMustDiffer(
                "revision_id"
            ))
        );

        let mut value = commitment_v4();
        value
            .rework_lineage
            .as_mut()
            .unwrap()
            .rejected_rework_content_commitment_sha256 = [0; 32];
        assert_eq!(
            value.validate(),
            Err(PaperRaidFinalityValidationError::ZeroDigest(
                "rework_lineage.rejected_rework_content_commitment_sha256"
            ))
        );
    }

    #[test]
    fn v4_original_submission_uses_canonical_null_and_lineage_cannot_be_stripped() {
        let mut original = commitment_v4();
        original.rework_lineage = None;
        original.validate().unwrap();
        let bytes = original.canonical_bytes();
        assert_eq!(bytes[0], 0x98, "V4 remains a 35-item CBOR array");
        assert_eq!(
            bytes.last(),
            Some(&0xf6),
            "lineage None must encode as null"
        );
        assert_eq!(
            PaperRaidFinalityCommitmentV4::from_canonical_bytes(&bytes).unwrap(),
            original
        );

        let signing_key = SigningKey::from_bytes(&[0x52; 32]);
        let signed = SignedPaperRaidFinalityCommandV4::sign(
            "trnm-paper-raid-test".to_string(),
            key(31),
            "did:trnm:hepta-paper-raid".to_string(),
            9,
            commitment_v4(),
            &signing_key,
        )
        .unwrap();
        let mut stripped = signed.clone();
        stripped.commitment.rework_lineage = None;
        assert_eq!(
            stripped.validate(),
            Err(SignedPaperRaidFinalityCommandValidationError::InvalidSignature)
        );
        assert_ne!(stripped.payload_hash(), signed.payload_hash());
        assert_ne!(stripped.command_fingerprint(), signed.command_fingerprint());
    }

    #[test]
    fn signed_v4_roundtrips_and_v3_v4_domains_cannot_replay() {
        let signing_key = SigningKey::from_bytes(&[0x42; 32]);
        let signed_v4 = SignedPaperRaidFinalityCommandV4::sign(
            "trnm-paper-raid-test".to_string(),
            key(30),
            "did:trnm:hepta-paper-raid".to_string(),
            9,
            commitment_v4(),
            &signing_key,
        )
        .unwrap();
        let bytes = signed_v4.canonical_bytes();
        assert_eq!(
            SignedPaperRaidFinalityCommandV4::from_canonical_bytes(&bytes).unwrap(),
            signed_v4
        );
        assert!(SignedPaperRaidFinalityCommandV3::from_canonical_bytes(&bytes).is_err());

        let signed_v3 = SignedPaperRaidFinalityCommandV3::sign(
            "trnm-paper-raid-test".to_string(),
            key(30),
            "did:trnm:hepta-paper-raid".to_string(),
            9,
            commitment_v3(),
            &signing_key,
        )
        .unwrap();
        assert_ne!(
            signed_v3.command_fingerprint(),
            signed_v4.command_fingerprint()
        );
        let mut replay = signed_v4;
        replay.signature = signed_v3.signature;
        assert_eq!(
            replay.validate(),
            Err(SignedPaperRaidFinalityCommandValidationError::InvalidSignature)
        );
    }
}
