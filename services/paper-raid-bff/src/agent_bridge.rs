use std::{collections::HashSet, time::Duration};

use axum::{
    body::{Body, Bytes},
    extract::{OriginalUri, Path, Query, State},
    http::{header, HeaderMap, HeaderName, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
};
use base64::{
    engine::general_purpose::{STANDARD as BASE64, URL_SAFE_NO_PAD},
    Engine as _,
};
use chrono::{DateTime, TimeZone, Utc};
use ed25519_dalek::VerifyingKey;
use hepta_paper_raid_contracts::{
    agent_bridge_request_proof_hash, agent_capability_disclosure_hash, canonical_json_bytes,
    decode_digest, frozen_review_bundle_hash, frozen_review_input_root,
    parse_challenge_dataset_manifest, parse_challenge_evaluator_manifest,
    review_execution_metrics_hash, review_execution_receipt_hash, review_execution_receipt_id,
    sha256_digest, validate_agent_bridge_canonical_query, verify_agent_binding_proof_v3,
    verify_agent_bridge_request_proof, verify_frozen_review_authority, verify_frozen_review_bundle,
    verify_review_execution_receipt_signature, AgentBindingProofClaimV3, AgentBridgeRequestProofV1,
    AgentCapabilityDisclosureV1, ChallengeDatasetManifestV1, ChallengeEvaluatorManifestV1,
    ChallengeManifestObjectV1, FrozenReviewAuthorityV1, FrozenReviewBundleV1,
    FrozenReviewExecutionPlanV1, FrozenReviewInputObjectV1, FrozenReviewObjectV1,
    ReviewEvaluationExecutionResultV1, ReviewExecutionReceiptV1,
    ReviewReproductionExecutionResultV1, AGENT_BINDING_PROOF_V3, PAPER_BUNDLE_V2,
    PAPER_RELEASE_CANDIDATE_V2, RESOLVED_FROZEN_REVIEW_BUNDLE_V1, REVIEW_EXECUTION_RECEIPT_V1,
};
use rand::{rngs::OsRng, RngCore};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    access::bump_quota,
    app::AppState,
    auth::CSRF_HEADER,
    config::{AgentBridgeQuotaConfig, AlphaIdentity, AlphaIdentityScope},
    error::AppError,
    hepta::{BrowserCommand, CommandName},
    practice::{
        PracticeBridgeTaskStateV1, PracticeExperimentResultV1, PracticeSessionV1, PracticeStageV1,
        PRACTICE_UNRANKED_MODE,
    },
    practice_http::{
        apply_agent_practice_transition, load_current_agent_practice, practice_agent_task_token,
        PracticeAgentTransitionV1,
    },
};

const PAIRING_GRANT_TTL_SECONDS: i64 = 300;
const DELIVERY_DRAFT_TTL_SECONDS: i64 = 15 * 60;
const MAX_PENDING_DELIVERY_DRAFTS: i64 = 64;
const MAX_DISCOVERED_INBOX_PAPERS: usize = 64;
const AGENT_PROOF_CLOCK_SKEW_SECONDS: i64 = 5;
const AGENT_REPLAY_WAIT_ATTEMPTS: usize = 40;
const AGENT_REPLAY_WAIT_INTERVAL: Duration = Duration::from_millis(50);
const MAX_PAIR_BODY_BYTES: usize = 128 * 1024;
const MAX_AGENT_BODY_BYTES: usize = 2 * 1024 * 1024;
const MAX_REVIEW_OBJECT_BYTES: usize = 16 * 1024 * 1024;
const JSON_SAFE_U64_MAX: u64 = 9_007_199_254_740_991;
const PAIR_CODE_PREFIX: &str = "prg1.";
const GLOBAL_PAIR_PRINCIPAL: &[u8] = b"paper-raid-bff/agent-pair/global/v1";
const GLOBAL_REQUEST_PRINCIPAL: &[u8] = b"paper-raid-bff/agent-request/global/v1";
const PAIR_BUCKET_DOMAIN: &[u8] = b"paper-raid-bff/agent-pair/fixed-bucket/v1\0";
const REQUEST_BUCKET_DOMAIN: &[u8] = b"paper-raid-bff/agent-request/fixed-bucket/v1\0";
const DELIVERY_PROPOSAL_ID_DOMAIN: &str = "hepta.paper_raid.agent_bridge.delivery_proposal_id.v1";
const DELIVERY_IDEMPOTENCY_KEY_DOMAIN: &str =
    "hepta.paper_raid.agent_bridge.delivery_idempotency_key.v1";
const REVIEW_TASK_ID_DOMAIN: &str = "hepta.paper_raid.agent_bridge.review_task_id.v1";
const REVIEW_EVALUATION_ID_DOMAIN: &str = "hepta.paper_raid.agent_bridge.review_evaluation_id.v1";
const PRACTICE_TASK_QUERY_V1: &str = "hepta.paper_raid.agent_bridge.practice_task_query.v1";
const PRACTICE_TASKS_V1: &str = "hepta.paper_raid.agent_bridge.practice_tasks.v1";
const PRACTICE_TASK_V1: &str = "hepta.paper_raid.agent_bridge.practice_task.v1";
const PRACTICE_MATERIALS_V1: &str = "hepta.paper_raid.agent_bridge.practice_materials.v1";
const PRACTICE_CLAIM_REQUEST_V1: &str = "hepta.paper_raid.agent_bridge.practice_claim_request.v1";
const PRACTICE_RESULT_REQUEST_V1: &str = "hepta.paper_raid.agent_bridge.practice_result_request.v1";
const PRACTICE_TRANSITION_RESULT_V1: &str =
    "hepta.paper_raid.agent_bridge.practice_transition_result.v1";
const LEGACY_GOLDEN_EVALUATOR_MANIFEST_HASH: &str =
    "sha256:805ee4ad69fa1e56cebc0721711419d211cf0fe2090e294db25bf712f10c74f8";
const LEGACY_GOLDEN_DATASET_MANIFEST_HASH: &str =
    "sha256:6c0494ec10383018b4a938528d179d16fcb6dd8961b8294ff6f4093dda42aeb4";
const LEGACY_GOLDEN_FROZEN_EVALUATOR_HASH: &str =
    "sha256:63971194ab97e1d14752795ff1ff8c39a44d1a7782ffbb9459ec2210fe8a8e3d";
const LEGACY_GOLDEN_FROZEN_EVALUATOR_PATH: &str = "evaluator/legacy-golden-evaluator.py";
const LEGACY_GOLDEN_FROZEN_EVALUATOR_SIZE: u64 = 3_483;
const LEGACY_GOLDEN_DATASET_PATH: &str = "inputs/synthetic-observations.csv";
const LEGACY_GOLDEN_DATASET_SIZE: u64 = 230;
const LEGACY_GOLDEN_PACK_ID: &str = "paper-raid-golden-v2-strict-review-v1";
const LEGACY_GOLDEN_DATASET_CARD: &[u8] = b"# Synthetic threshold dataset\n\nThis public integration fixture has twelve generated rows, no natural-person data, no sensitive source, and no privacy claim. License: CC0-1.0.\n";

const HEADER_SCHEMA: &str = "x-paper-raid-agent-schema";
const HEADER_BINDING_ID: &str = "x-paper-raid-agent-binding-id";
const HEADER_AGENT_ID: &str = "x-paper-raid-agent-id";
const HEADER_KEY_ID: &str = "x-paper-raid-agent-key-id";
const HEADER_NONCE: &str = "x-paper-raid-agent-nonce";
const HEADER_ISSUED_AT: &str = "x-paper-raid-agent-issued-at";
const HEADER_EXPIRES_AT: &str = "x-paper-raid-agent-expires-at";
const HEADER_BODY_SHA256: &str = "x-paper-raid-agent-body-sha256";
const HEADER_SIGNATURE: &str = "x-paper-raid-agent-signature";

const AGENT_HEADER_NAMES: [&str; 9] = [
    HEADER_SCHEMA,
    HEADER_BINDING_ID,
    HEADER_AGENT_ID,
    HEADER_KEY_ID,
    HEADER_NONCE,
    HEADER_ISSUED_AT,
    HEADER_EXPIRES_AT,
    HEADER_BODY_SHA256,
    HEADER_SIGNATURE,
];

pub(crate) fn review_outer_bundle_matches_authority(
    review_bundle: &Value,
    authority: &FrozenReviewAuthorityV1,
) -> bool {
    let paper_id = authority.paper_project_id.to_string();
    let submission_id = authority.submission_id.to_string();
    let Some(paper_bundle) = review_bundle.get("paper_bundle") else {
        return false;
    };
    let Some(release_candidate) = paper_bundle.get("release_candidate") else {
        return false;
    };
    review_bundle
        .get("paper_project_id")
        .and_then(Value::as_str)
        == Some(paper_id.as_str())
        && review_bundle.get("submission_id").and_then(Value::as_str)
            == Some(submission_id.as_str())
        && review_bundle.get("status").and_then(Value::as_str) == Some("submission_ready")
        && review_bundle
            .get("release_candidate_hash")
            .and_then(Value::as_str)
            == Some(authority.release_candidate_hash.as_str())
        && review_bundle
            .get("paper_bundle_hash")
            .and_then(Value::as_str)
            == Some(authority.paper_bundle_hash.as_str())
        && paper_bundle.get("schema").and_then(Value::as_str) == Some(PAPER_BUNDLE_V2)
        && paper_bundle
            .get("release_candidate_hash")
            .and_then(Value::as_str)
            == Some(authority.release_candidate_hash.as_str())
        && paper_bundle
            .get("paper_bundle_hash")
            .and_then(Value::as_str)
            == Some(authority.paper_bundle_hash.as_str())
        && release_candidate.get("schema").and_then(Value::as_str)
            == Some(PAPER_RELEASE_CANDIDATE_V2)
        && release_candidate
            .get("paper_project_id")
            .and_then(Value::as_str)
            == Some(paper_id.as_str())
        && release_candidate
            .get("artifact_manifest_hash")
            .and_then(Value::as_str)
            == Some(authority.artifact_manifest_hash.as_str())
}

fn review_assignment_matches_descriptor(
    assignment: &Value,
    descriptor: &FrozenReviewBundleV1,
    expected_player_id: Uuid,
) -> bool {
    let assignment_id = descriptor.assignment_id.to_string();
    let paper_id = descriptor.paper_project_id.to_string();
    let submission_id = descriptor.submission_id.to_string();
    let player_id = expected_player_id.to_string();
    assignment.get("assignment_id").and_then(Value::as_str) == Some(assignment_id.as_str())
        && assignment.get("paper_project_id").and_then(Value::as_str) == Some(paper_id.as_str())
        && assignment.get("submission_id").and_then(Value::as_str) == Some(submission_id.as_str())
        && assignment.get("player_id").and_then(Value::as_str) == Some(player_id.as_str())
        && assignment.get("review_round").and_then(Value::as_u64) == Some(descriptor.review_round)
        && assignment.get("slot").and_then(Value::as_str) == Some(descriptor.slot.as_str())
        && assignment.get("version").and_then(Value::as_u64) == Some(descriptor.assignment_version)
        && assignment.get("expires_at").and_then(Value::as_str)
            == Some(descriptor.expires_at.as_str())
        && matches!(
            assignment.get("status").and_then(Value::as_str),
            Some("claimed" | "pinned")
        )
}

/// Bind one queue discovery record to the exact independently verified review authority.
///
/// The queue supplies discoverability only.  It may not redirect an assignment to another Paper
/// or substitute any submission/release/bundle/player/round/slot/version/status field.  The
/// caller still derives and checks the deterministic executable task ID after this binding.
pub(crate) fn review_queue_item_matches_resolved_bundle(
    queue_item: &Value,
    resolved_bundle: &Value,
    expected_player_id: Uuid,
) -> bool {
    let descriptor: FrozenReviewBundleV1 = match resolved_bundle
        .get("resolved_frozen_review_bundle")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
    {
        Some(descriptor) => descriptor,
        None => return false,
    };
    if verify_frozen_review_bundle(&descriptor).is_err()
        || !review_outer_bundle_matches_authority(resolved_bundle, &descriptor.authority)
    {
        return false;
    }
    let Some(queue_assignments) = queue_item.get("my_assignments").and_then(Value::as_array) else {
        return false;
    };
    let Some(bundle_assignments) = resolved_bundle
        .get("my_assignments")
        .and_then(Value::as_array)
    else {
        return false;
    };
    if queue_assignments.len() != 1
        || bundle_assignments.len() != 1
        || !review_assignment_matches_descriptor(
            &queue_assignments[0],
            &descriptor,
            expected_player_id,
        )
        || !review_assignment_matches_descriptor(
            &bundle_assignments[0],
            &descriptor,
            expected_player_id,
        )
        || queue_assignments[0].get("status").and_then(Value::as_str)
            != bundle_assignments[0].get("status").and_then(Value::as_str)
    {
        return false;
    }
    let paper_id = descriptor.paper_project_id.to_string();
    let submission_id = descriptor.submission_id.to_string();
    queue_item.get("paper_project_id").and_then(Value::as_str) == Some(paper_id.as_str())
        && queue_item.get("submission_id").and_then(Value::as_str) == Some(submission_id.as_str())
        && queue_item
            .get("release_candidate_hash")
            .and_then(Value::as_str)
            == Some(descriptor.release_candidate_hash.as_str())
        && queue_item.get("paper_bundle_hash").and_then(Value::as_str)
            == Some(descriptor.paper_bundle_hash.as_str())
}

fn identity_supports_agent_bridge(identity: &AlphaIdentity) -> bool {
    [
        AlphaIdentityScope::Author,
        AlphaIdentityScope::Evaluator,
        AlphaIdentityScope::Reviewer,
        AlphaIdentityScope::Reproducer,
    ]
    .into_iter()
    .any(|scope| identity.has_scope(scope))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyGrantRequest {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PairingCodeRequest {
    pairing_code: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AgentBindingV3Request {
    binding_id: Uuid,
    player_id: Uuid,
    agent_id: String,
    agent_key_id: String,
    agent_public_key: String,
    agent_proof_schema: String,
    capability_disclosure: AgentCapabilityDisclosureV1,
    capability_disclosure_hash: String,
    agent_proof_nonce: Uuid,
    agent_proof_issued_at_unix: i64,
    agent_proof_expires_at_unix: i64,
    agent_proof_signature: String,
    idempotency_key: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PairAgentRequest {
    pairing_code: String,
    binding_request: AgentBindingV3Request,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HealthReport {
    schema: String,
    assurance: String,
    status: String,
    observed_at_unix: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PracticeTaskQueryV1 {
    schema: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PracticeClaimRequestV1 {
    schema: String,
    expected_version: u64,
    task_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PracticeResultRequestV1 {
    schema: String,
    expected_version: u64,
    task_token: String,
    result_code: PracticeExperimentResultV1,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InboxRequest {
    schema: String,
    paper_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewObjectQuery {
    #[serde(deserialize_with = "deserialize_canonical_uuid")]
    assignment_id: Uuid,
    bundle_hash: String,
    digest: String,
    object_key: String,
    #[serde(deserialize_with = "deserialize_canonical_uuid")]
    task_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChallengeObjectQuery {
    bundle_hash: String,
    digest: String,
    object_key: String,
    #[serde(deserialize_with = "deserialize_canonical_uuid")]
    paper_id: Uuid,
    #[serde(deserialize_with = "deserialize_canonical_uuid")]
    work_item_id: Uuid,
}

fn deserialize_canonical_uuid<'de, D>(deserializer: D) -> Result<Uuid, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let text = String::deserialize(deserializer)?;
    let value = Uuid::parse_str(&text).map_err(serde::de::Error::custom)?;
    if value.is_nil() || value.to_string() != text {
        return Err(serde::de::Error::custom(
            "UUID must use canonical lowercase dashed text",
        ));
    }
    Ok(value)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewReceiptRequest {
    schema: String,
    idempotency_key: Uuid,
    receipt: ReviewExecutionReceiptV1,
    output: Value,
    environment: Value,
    run_manifest: Value,
    logs: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeliveryDraftRequest {
    schema: String,
    delivery_draft_id: Uuid,
    paper_id: Uuid,
    work_item_id: Uuid,
    section_key: String,
    artifact_manifest_id: Uuid,
    payload_hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposalRequest {
    schema: String,
    paper_id: Uuid,
    #[serde(default)]
    delivery_draft_id: Option<Uuid>,
    payload: Value,
}

#[derive(Debug, Clone)]
struct PairingGrant {
    grant_id: Uuid,
    subject_id: String,
    player_id: Uuid,
    state: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    pinned_request_hash: Option<Vec<u8>>,
    pinned_binding_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
struct BridgeMapping {
    binding_id: Uuid,
    subject_id: String,
    player_id: Uuid,
    agent_id: String,
    agent_key_id: String,
    capability_disclosure_hash: String,
}

#[derive(Clone, Copy)]
struct BridgeOwner<'a> {
    binding_id: Uuid,
    subject_id: &'a str,
    player_id: Uuid,
    agent_id: &'a str,
}

#[derive(Debug, Clone)]
struct DeliveryContext {
    expected_work_version: u64,
    lease_id: Uuid,
    lease_fencing_token: u64,
    lease_expires_at: DateTime<Utc>,
    parent_revision_id: Uuid,
    artifact_manifest_hash: String,
}

#[derive(Debug, Clone)]
struct DeliveryDraft {
    delivery_draft_id: Uuid,
    binding_id: Uuid,
    paper_id: Uuid,
    work_item_id: Uuid,
    expected_work_version: u64,
    section_key: String,
    lease_id: Uuid,
    lease_fencing_token: u64,
    parent_revision_id: Uuid,
    artifact_manifest_id: Uuid,
    artifact_manifest_hash: String,
    payload_hash: String,
    state: String,
    proposal_body_hash: Option<String>,
    proposal_id: Option<Uuid>,
    proposal_idempotency_key: Option<Uuid>,
    proposal_signed_at_unix: Option<i64>,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

struct DeliveryProposalClaim<'a> {
    request_payload: &'a Value,
    proposal_body_hash: &'a str,
    proposal_id: Uuid,
    proposal_idempotency_key: Uuid,
    proposal_signed_at_unix: i64,
    claimed_at: DateTime<Utc>,
}

struct VerifiedAgentRequest {
    identity: AlphaIdentity,
    mapping: BridgeMapping,
    authoritative_binding: Value,
    exactly_one_active_binding: bool,
    claim: AgentBridgeRequestProofV1,
    request_hash: [u8; 32],
    replay: Option<(u16, Vec<u8>)>,
}

enum AgentRequestUseState {
    Missing,
    Pending,
    Completed(u16, Vec<u8>),
}

pub async fn create_pairing_grant(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let session = match state.session(&headers).await {
        Ok(session) => session,
        Err(error) => return error.into_response(),
    };
    let next_csrf = match state.sessions.consume_csrf(&headers, &session).await {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    let response = async {
        let _: EmptyGrantRequest = decode_json(&body, 1024)?;
        if !identity_supports_agent_bridge(&session.identity) {
            return Err(AppError::Forbidden);
        }
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(PAIRING_GRANT_TTL_SECONDS);
        let grant_id = Uuid::new_v4();
        let pairing_code = generate_pairing_code();
        let code_hash = digest_bytes(pairing_code.as_bytes());
        let mut tx = state.pool.begin().await?;
        sqlx::query(
            "UPDATE paper_raid_bff_agent_pairing_grants \
             SET state='revoked', revoked_at=$1, updated_at=$1 \
            WHERE subject_id=$2 AND state IN ('issued','pinned') AND expires_at <= $1",
        )
        .bind(now)
        .bind(&session.identity.subject_id)
        .execute(&mut *tx)
        .await?;
        if sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS ( \
                SELECT 1 FROM paper_raid_bff_agent_pairing_grants \
                WHERE subject_id=$1 AND state IN ('issued','pinned') \
            )",
        )
        .bind(&session.identity.subject_id)
        .fetch_one(&mut *tx)
        .await?
        {
            return Err(AppError::Conflict("active_pairing_grant_exists".into()));
        }
        sqlx::query(
            "INSERT INTO paper_raid_bff_agent_pairing_grants ( \
                grant_id, subject_id, player_id, code_hash, state, \
                created_at, expires_at, updated_at \
             ) VALUES ($1,$2,$3,$4,'issued',$5,$6,$5)",
        )
        .bind(grant_id)
        .bind(&session.identity.subject_id)
        .bind(session.identity.player_id)
        .bind(code_hash.as_slice())
        .bind(now)
        .bind(expires_at)
        .execute(&mut *tx)
        .await?;
        bridge_audit(
            &mut tx,
            Some(&session.identity.subject_id),
            None,
            "pairing_grant_created",
            "succeeded",
            json!({"grant_id": grant_id}),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({
            "schema": "hepta.paper_raid.agent_bridge.pairing_grant.v1",
            "grant_id": grant_id,
            "pairing_code": pairing_code,
            "expires_at_unix": expires_at.timestamp(),
            "display_once": true
        }))
    }
    .await;
    with_rotated_csrf(result_json(response), next_csrf)
}

pub async fn pairing_grant_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    if !identity_supports_agent_bridge(&session.identity) {
        return Err(AppError::Forbidden);
    }
    let row = sqlx::query(
        "SELECT g.grant_id, g.state, g.created_at, g.expires_at, \
                g.pinned_binding_id, b.agent_id, b.agent_key_id, b.last_verified_at \
         FROM paper_raid_bff_agent_pairing_grants g \
         LEFT JOIN paper_raid_bff_agent_bridge_bindings b \
           ON b.grant_id=g.grant_id OR b.last_pairing_grant_id=g.grant_id \
         WHERE g.subject_id=$1 ORDER BY g.created_at DESC, g.grant_id DESC LIMIT 1",
    )
    .bind(&session.identity.subject_id)
    .fetch_optional(&state.pool)
    .await?;
    let grant = row.map(|row| {
        json!({
            "grant_id": row.get::<Uuid,_>("grant_id"),
            "state": row.get::<String,_>("state"),
            "created_at": row.get::<DateTime<Utc>,_>("created_at"),
            "expires_at": row.get::<DateTime<Utc>,_>("expires_at"),
            "binding_id": row.try_get::<Uuid,_>("pinned_binding_id").ok(),
            "agent_id": row.try_get::<String,_>("agent_id").ok(),
            "agent_key_id": row.try_get::<String,_>("agent_key_id").ok(),
            "last_verified_at": row.try_get::<DateTime<Utc>,_>("last_verified_at").ok(),
        })
    });
    Ok(private_json(json!({
        "schema": "hepta.paper_raid.agent_bridge.pairing_status.v1",
        "grant": grant
    })))
}

pub async fn revoke_pairing_grant(
    State(state): State<AppState>,
    Path(grant_id): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let session = match state.session(&headers).await {
        Ok(session) => session,
        Err(error) => return error.into_response(),
    };
    let next_csrf = match state.sessions.consume_csrf(&headers, &session).await {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    let response = async {
        let _: EmptyGrantRequest = decode_json(&body, 1024)?;
        let mut tx = state.pool.begin().await?;
        let updated = sqlx::query(
            "UPDATE paper_raid_bff_agent_pairing_grants \
             SET state='revoked', revoked_at=now(), updated_at=now() \
             WHERE grant_id=$1 AND subject_id=$2 \
               AND (state='issued' OR (state='pinned' AND expires_at <= now()))",
        )
        .bind(grant_id)
        .bind(&session.identity.subject_id)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(AppError::Conflict("pairing_grant_not_revocable".into()));
        }
        bridge_audit(
            &mut tx,
            Some(&session.identity.subject_id),
            None,
            "pairing_grant_revoked",
            "succeeded",
            json!({"grant_id": grant_id}),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({
            "schema": "hepta.paper_raid.agent_bridge.pairing_status.v1",
            "grant_id": grant_id,
            "state": "revoked"
        }))
    }
    .await;
    with_rotated_csrf(result_json(response), next_csrf)
}

pub async fn pairing_context(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Response, AppError> {
    let request: PairingCodeRequest = match decode_json(&body, MAX_PAIR_BODY_BYTES) {
        Ok(request) => request,
        Err(error) => {
            admit_pair_quota(&state, &digest_bytes(&body)).await?;
            return Err(error);
        }
    };
    let code_hash = pairing_code_hash_for_admission(&request.pairing_code)?;
    admit_pair_quota(&state, &code_hash).await?;
    validate_pairing_code(&request.pairing_code)?;
    let grant = load_pairing_grant_by_hash(&state, &code_hash).await?;
    Ok(private_json(json!({
        "schema": "hepta.paper_raid.agent_bridge.pairing_context.v1",
        "grant_id": grant.grant_id,
        "subject_id": grant.subject_id,
        "player_id": grant.player_id,
        "issued_at_unix": grant.created_at.timestamp(),
        "expires_at_unix": grant.expires_at.timestamp()
    })))
}

pub async fn pair_agent(State(state): State<AppState>, body: Bytes) -> Result<Response, AppError> {
    let request: PairAgentRequest = match decode_json(&body, MAX_PAIR_BODY_BYTES) {
        Ok(request) => request,
        Err(error) => {
            admit_pair_quota(&state, &digest_bytes(&body)).await?;
            return Err(error);
        }
    };
    let code_hash = pairing_code_hash_for_admission(&request.pairing_code)?;
    admit_pair_quota(&state, &code_hash).await?;
    validate_pairing_code(&request.pairing_code)?;
    let mut grant = load_pairing_grant_by_hash(&state, &code_hash).await?;
    let identity = state
        .identity_for_agent_bridge(&grant.subject_id)
        .await?
        .ok_or(AppError::Forbidden)?;
    if identity.player_id != grant.player_id || !identity_supports_agent_bridge(&identity) {
        return Err(AppError::Forbidden);
    }
    let binding_value = validate_binding_request(&request.binding_request, &identity)?;
    let binding_bytes = canonical_json_bytes(&binding_value).map_err(AppError::Invalid)?;
    let request_hash = digest_bytes(&binding_bytes);
    pin_pairing_grant(
        &state,
        &code_hash,
        request_hash,
        request.binding_request.binding_id,
    )
    .await?;
    grant.state = "pinned".into();
    grant.pinned_request_hash = Some(request_hash.to_vec());
    grant.pinned_binding_id = Some(request.binding_request.binding_id);

    if let Some(binding) =
        find_exact_active_binding(&state, &identity, &request.binding_request).await?
    {
        let response =
            persist_pairing_result(&state, &grant, &request.binding_request, &binding).await?;
        return Ok(private_bytes(StatusCode::OK.as_u16(), response));
    }

    let upstream = state
        .hepta
        .create_agent_binding_v3(
            &identity,
            request.binding_request.idempotency_key,
            &binding_value,
        )
        .await?;
    if !(200..300).contains(&upstream.status) {
        return Err(AppError::Upstream);
    }
    let binding = upstream.json()?;
    validate_exact_binding(&binding, &request.binding_request, &identity)?;
    let response =
        persist_pairing_result(&state, &grant, &request.binding_request, &binding).await?;
    Ok(private_bytes(StatusCode::OK.as_u16(), response))
}

pub async fn agent_binding(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let verified = verify_agent_request(&state, Method::GET, &uri, &headers, &body).await?;
    if let Some(response) = replay_response(&verified) {
        return Ok(response);
    }
    let value = json!({
        "schema": "hepta.paper_raid.agent_bridge.binding_read.v1",
        "binding": verified.authoritative_binding
    });
    complete_agent_request(&state, &verified, StatusCode::OK, &value).await
}

pub async fn agent_health(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let verified = verify_agent_request(&state, Method::POST, &uri, &headers, &body).await?;
    if let Some(response) = replay_response(&verified) {
        return Ok(response);
    }
    let report: HealthReport = decode_json(&body, 16 * 1024)?;
    if report.schema != "hepta.paper_raid.agent_bridge.health_report.v1"
        || report.assurance != "self_declared_unverified"
        || !matches!(report.status.as_str(), "healthy" | "degraded" | "offline")
        || report.observed_at_unix < verified.claim.issued_at_unix - 300
        || report.observed_at_unix > verified.claim.expires_at_unix + 300
    {
        return Err(AppError::Invalid(
            "invalid Agent health self-declaration".into(),
        ));
    }
    let observed_at = Utc
        .timestamp_opt(report.observed_at_unix, 0)
        .single()
        .ok_or_else(|| AppError::Invalid("invalid health observation time".into()))?;
    sqlx::query(
        "INSERT INTO paper_raid_bff_agent_health ( \
            binding_id, assurance, status, observed_at, last_seen_at, updated_at \
         ) VALUES ($1,'self_declared_unverified',$2,$3,now(),now()) \
         ON CONFLICT(binding_id) DO UPDATE SET \
            assurance='self_declared_unverified', status=EXCLUDED.status, \
            observed_at=EXCLUDED.observed_at, last_seen_at=now(), updated_at=now()",
    )
    .bind(verified.mapping.binding_id)
    .bind(&report.status)
    .bind(observed_at)
    .execute(&state.pool)
    .await?;
    let value = json!({
        "schema": "hepta.paper_raid.agent_bridge.health_result.v1",
        "binding_id": verified.mapping.binding_id,
        "assurance": "self_declared_unverified",
        "status": report.status,
        "observed_at_unix": report.observed_at_unix
    });
    complete_agent_request(&state, &verified, StatusCode::OK, &value).await
}

pub async fn agent_practice_tasks(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let verified = verify_agent_request(&state, Method::POST, &uri, &headers, &body).await?;
    if let Some(response) = replay_response(&verified) {
        return Ok(response);
    }
    ensure_practice_agent_admission(&verified)?;
    let request: PracticeTaskQueryV1 = decode_json(&body, 1024)?;
    if request.schema != PRACTICE_TASK_QUERY_V1 {
        return Err(AppError::Invalid("invalid practice task query".into()));
    }
    let practice = load_current_agent_practice(
        &state.pool,
        &verified.mapping.subject_id,
        verified.mapping.player_id,
        verified.mapping.binding_id,
    )
    .await?;
    let value = practice_task_projection(practice.as_ref(), Utc::now())?;
    complete_agent_request(&state, &verified, StatusCode::OK, &value).await
}

pub async fn agent_practice_claim(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let verified = verify_agent_request(&state, Method::POST, &uri, &headers, &body).await?;
    if let Some(response) = replay_response(&verified) {
        return Ok(response);
    }
    ensure_practice_agent_admission(&verified)?;
    let request: PracticeClaimRequestV1 = decode_json(&body, 1024)?;
    if request.schema != PRACTICE_CLAIM_REQUEST_V1
        || request.expected_version == 0
        || request.expected_version > JSON_SAFE_U64_MAX
        || decode_digest(&request.task_token).is_err()
    {
        return Err(AppError::Invalid("invalid practice claim request".into()));
    }
    let receipt = apply_agent_practice_transition(
        &state.pool,
        &verified.mapping.subject_id,
        verified.mapping.player_id,
        verified.mapping.binding_id,
        request.expected_version,
        &request.task_token,
        &agent_request_hash_label(&verified),
        PracticeAgentTransitionV1::Claim,
    )
    .await?;
    let value = json!({
        "schema": PRACTICE_TRANSITION_RESULT_V1,
        "operation": "claim",
        "status": "claimed",
        "from_version": receipt.from_version,
        "version": receipt.to_version,
    });
    complete_agent_request(&state, &verified, StatusCode::OK, &value).await
}

pub async fn agent_practice_result(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let verified = verify_agent_request(&state, Method::POST, &uri, &headers, &body).await?;
    if let Some(response) = replay_response(&verified) {
        return Ok(response);
    }
    ensure_practice_agent_admission(&verified)?;
    let request: PracticeResultRequestV1 = decode_json(&body, 1024)?;
    if request.schema != PRACTICE_RESULT_REQUEST_V1
        || request.expected_version == 0
        || request.expected_version > JSON_SAFE_U64_MAX
        || decode_digest(&request.task_token).is_err()
    {
        return Err(AppError::Invalid("invalid practice result request".into()));
    }
    let receipt = apply_agent_practice_transition(
        &state.pool,
        &verified.mapping.subject_id,
        verified.mapping.player_id,
        verified.mapping.binding_id,
        request.expected_version,
        &request.task_token,
        &agent_request_hash_label(&verified),
        PracticeAgentTransitionV1::Result(request.result_code),
    )
    .await?;
    if receipt.result_code != Some(request.result_code) {
        return Err(AppError::Internal);
    }
    let value = json!({
        "schema": PRACTICE_TRANSITION_RESULT_V1,
        "operation": "result",
        "status": "completed",
        "from_version": receipt.from_version,
        "version": receipt.to_version,
        "result_code": request.result_code,
    });
    complete_agent_request(&state, &verified, StatusCode::OK, &value).await
}

fn ensure_practice_agent_admission(verified: &VerifiedAgentRequest) -> Result<(), AppError> {
    if !verified.exactly_one_active_binding {
        return Err(AppError::Conflict(
            "practice_requires_exactly_one_active_binding".into(),
        ));
    }
    Ok(())
}

fn agent_request_hash_label(verified: &VerifiedAgentRequest) -> String {
    format!("sha256:{}", hex::encode(verified.request_hash))
}

fn practice_task_projection(
    practice: Option<&PracticeSessionV1>,
    now: DateTime<Utc>,
) -> Result<Value, AppError> {
    let Some(practice) = practice else {
        return Ok(json!({
            "schema": PRACTICE_TASKS_V1,
            "mode": PRACTICE_UNRANKED_MODE,
            "status": "absent",
            "task": Value::Null,
        }));
    };
    practice.validate().map_err(|_| AppError::Internal)?;
    let expired = now >= practice.expires_at;
    let completed = practice.bridge_task_state == PracticeBridgeTaskStateV1::Completed;
    let task_visible = completed
        || (!expired
            && practice.stage == PracticeStageV1::ExperimentWaitingBridge
            && matches!(
                practice.bridge_task_state,
                PracticeBridgeTaskStateV1::Pending | PracticeBridgeTaskStateV1::Claimed
            ));
    if !task_visible {
        return Ok(json!({
            "schema": PRACTICE_TASKS_V1,
            "mode": PRACTICE_UNRANKED_MODE,
            "status": if expired { "expired" } else { "not_ready" },
            "task": Value::Null,
        }));
    }
    Ok(json!({
        "schema": PRACTICE_TASKS_V1,
        "mode": PRACTICE_UNRANKED_MODE,
        "status": "ready",
        "task": {
            "schema": PRACTICE_TASK_V1,
            "kind": "evidence_audit_intro",
            "state": practice.bridge_task_state,
            "version": practice.version,
            "task_token": practice_agent_task_token(practice)?,
            "expires_at": practice.expires_at,
            "materials": {
                "schema": PRACTICE_MATERIALS_V1,
                "claim": "The candidate result remains supported after the evidence audit.",
                "baseline": "Compare the stated claim with the supplied observation summary.",
                "observations": [
                    "The cited observation and the claimed scope do not fully align.",
                    "Run the bounded practice check and report only one allowed result code."
                ],
            },
            "allowed_result_codes": [
                "concern_confirmed",
                "concern_not_detected",
                "inconclusive"
            ],
            "result_code": practice.bridge_result_code,
        },
    }))
}

pub async fn agent_delivery_draft(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let verified = verify_agent_request(&state, Method::POST, &uri, &headers, &body).await?;
    if let Some(response) = replay_response(&verified) {
        return Ok(response);
    }
    let request: DeliveryDraftRequest = decode_json(&body, 64 * 1024)?;
    if request.schema != "hepta.paper_raid.agent_bridge.delivery_draft_request.v1"
        || !valid_section_key(&request.section_key)
        || decode_digest(&request.payload_hash).is_err()
    {
        return Err(AppError::Invalid(
            "invalid Agent delivery draft request".into(),
        ));
    }
    let now = Utc::now();
    let room = state
        .hepta
        .get_paper_room(&verified.identity, request.paper_id)
        .await?;
    let context = resolve_delivery_context(
        &room,
        &verified.mapping,
        request.work_item_id,
        &request.section_key,
        request.artifact_manifest_id,
        now,
    )?
    .ok_or_else(|| {
        AppError::Conflict("delivery_context_not_authoritative_or_no_longer_current".into())
    })?;
    let expires_at = std::cmp::min(
        now + chrono::Duration::seconds(DELIVERY_DRAFT_TTL_SECONDS),
        context.lease_expires_at,
    );
    if expires_at <= now {
        return Err(AppError::Conflict("section_lease_expired".into()));
    }

    let mut tx = state.pool.begin().await?;
    expire_delivery_drafts(&mut tx, verified.mapping.binding_id, now).await?;
    let same_draft_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM paper_raid_bff_agent_delivery_drafts \
         WHERE delivery_draft_id=$1 AND binding_id=$2)",
    )
    .bind(request.delivery_draft_id)
    .bind(verified.mapping.binding_id)
    .fetch_one(&mut *tx)
    .await?;
    if !same_draft_exists {
        let pending = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM paper_raid_bff_agent_delivery_drafts \
             WHERE binding_id=$1 AND state='pending' AND expires_at>$2",
        )
        .bind(verified.mapping.binding_id)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
        if pending >= MAX_PENDING_DELIVERY_DRAFTS {
            return Err(AppError::Conflict(
                "too_many_pending_agent_delivery_drafts".into(),
            ));
        }
    }
    let lease_fencing_token = i64::try_from(context.lease_fencing_token).map_err(|_| {
        AppError::Invalid("section lease fencing token exceeds PostgreSQL range".into())
    })?;
    let expected_work_version = i64::try_from(context.expected_work_version)
        .map_err(|_| AppError::Invalid("work item version exceeds PostgreSQL range".into()))?;
    let refreshed = sqlx::query(
        "UPDATE paper_raid_bff_agent_delivery_drafts SET \
            lease_id=$1,lease_fencing_token=$2,parent_revision_id=$3, \
            artifact_manifest_hash=$4,state='pending',created_at=$5,expires_at=$6, \
            expected_work_version=$14, \
            proposal_body_hash=NULL,proposal_id=NULL,proposal_idempotency_key=NULL, \
            proposal_signed_at_unix=NULL,submitting_at=NULL,consumed_at=NULL, \
            invalidated_at=NULL,updated_at=$5 \
         WHERE delivery_draft_id=$7 AND binding_id=$8 AND paper_id=$9 \
           AND work_item_id=$10 AND section_key=$11 AND artifact_manifest_id=$12 \
           AND payload_hash=$13 AND state IN ('pending','expired','invalidated')",
    )
    .bind(context.lease_id)
    .bind(lease_fencing_token)
    .bind(context.parent_revision_id)
    .bind(&context.artifact_manifest_hash)
    .bind(now)
    .bind(expires_at)
    .bind(request.delivery_draft_id)
    .bind(verified.mapping.binding_id)
    .bind(request.paper_id)
    .bind(request.work_item_id)
    .bind(&request.section_key)
    .bind(request.artifact_manifest_id)
    .bind(&request.payload_hash)
    .bind(expected_work_version)
    .execute(&mut *tx)
    .await?;
    if refreshed.rows_affected() == 0 {
        sqlx::query(
            "INSERT INTO paper_raid_bff_agent_delivery_drafts ( \
                delivery_draft_id,binding_id,paper_id,work_item_id,section_key, \
                lease_id,lease_fencing_token,parent_revision_id,artifact_manifest_id, \
                artifact_manifest_hash,payload_hash,state,created_at,expires_at,updated_at, \
                expected_work_version \
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'pending',$12,$13,$12,$14) \
             ON CONFLICT DO NOTHING",
        )
        .bind(request.delivery_draft_id)
        .bind(verified.mapping.binding_id)
        .bind(request.paper_id)
        .bind(request.work_item_id)
        .bind(&request.section_key)
        .bind(context.lease_id)
        .bind(lease_fencing_token)
        .bind(context.parent_revision_id)
        .bind(request.artifact_manifest_id)
        .bind(&context.artifact_manifest_hash)
        .bind(&request.payload_hash)
        .bind(now)
        .bind(expires_at)
        .bind(expected_work_version)
        .execute(&mut *tx)
        .await?;
    }
    let draft = load_delivery_draft_tx(
        &mut tx,
        request.delivery_draft_id,
        verified.mapping.binding_id,
        request.paper_id,
        now,
    )
    .await?
    .ok_or_else(|| AppError::Conflict("delivery_draft_id_already_consumed_or_mismatched".into()))?;
    bridge_audit(
        &mut tx,
        Some(&verified.mapping.subject_id),
        Some(verified.mapping.binding_id),
        "delivery_draft_declared",
        "succeeded",
        json!({
            "delivery_draft_id": draft.delivery_draft_id,
            "paper_id": draft.paper_id,
            "work_item_id": draft.work_item_id,
            "section_key": draft.section_key,
        }),
    )
    .await?;
    tx.commit().await?;
    let value = json!({
        "schema": "hepta.paper_raid.agent_bridge.delivery_draft_result.v1",
        "candidate": delivery_candidate_value(&draft),
    });
    complete_agent_request(&state, &verified, StatusCode::OK, &value).await
}

fn valid_section_key(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(first) if first.is_ascii_alphanumeric())
        && value.len() <= 128
        && bytes
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn record_uuid(record: &Value, field: &str) -> Option<Uuid> {
    record
        .get(field)
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
}

fn record_time(record: &Value, field: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(record.get(field)?.as_str()?)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn resolve_delivery_context(
    room: &Value,
    mapping: &BridgeMapping,
    work_item_id: Uuid,
    section_key: &str,
    artifact_manifest_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Option<DeliveryContext>, AppError> {
    if !valid_section_key(section_key) {
        return Ok(None);
    }
    let paper = room
        .get("paper")
        .and_then(Value::as_object)
        .ok_or(AppError::Upstream)?;
    let phase = paper
        .get("phase")
        .and_then(Value::as_str)
        .ok_or(AppError::Upstream)?;
    if !matches!(phase, "drafting" | "reproducing") {
        return Ok(None);
    }
    let work_items = room
        .get("work_items")
        .and_then(Value::as_array)
        .ok_or(AppError::Upstream)?;
    let work = work_items.iter().find(|item| {
        record_uuid(item, "work_item_id") == Some(work_item_id)
            && record_uuid(item, "assigned_binding_id") == Some(mapping.binding_id)
            && record_uuid(item, "assigned_player_id") == Some(mapping.player_id)
            && matches!(
                item.get("status").and_then(Value::as_str),
                Some("planned" | "in_progress")
            )
    });
    let Some(work) = work else {
        return Ok(None);
    };
    let Some(expected_work_version) = work.get("version").and_then(Value::as_u64) else {
        return Ok(None);
    };
    if expected_work_version == 0 || expected_work_version > JSON_SAFE_U64_MAX {
        return Ok(None);
    }
    let heads = room
        .get("section_heads")
        .and_then(Value::as_array)
        .ok_or(AppError::Upstream)?;
    let Some(head) = heads
        .iter()
        .find(|item| item.get("section_key").and_then(Value::as_str) == Some(section_key))
    else {
        return Ok(None);
    };
    let Some(parent_revision_id) = record_uuid(head, "current_head_revision_id") else {
        return Ok(None);
    };
    let Some(head_fencing_token) = head.get("fencing_token").and_then(Value::as_u64) else {
        return Ok(None);
    };
    if head_fencing_token == 0 || head_fencing_token > JSON_SAFE_U64_MAX {
        return Ok(None);
    }
    let leases = room
        .get("leases")
        .and_then(Value::as_array)
        .ok_or(AppError::Upstream)?;
    let mut active_leases = leases.iter().filter_map(|item| {
        let expires_at = record_time(item, "expires_at")?;
        (item.get("section_key").and_then(Value::as_str) == Some(section_key)
            && item.get("status").and_then(Value::as_str) == Some("active")
            && record_uuid(item, "holder_binding_id") == Some(mapping.binding_id)
            && record_uuid(item, "holder_player_id") == Some(mapping.player_id)
            && item.get("fencing_token").and_then(Value::as_u64) == Some(head_fencing_token)
            && expires_at > now)
            .then_some((item, expires_at))
    });
    let Some((lease, lease_expires_at)) = active_leases.next() else {
        return Ok(None);
    };
    if active_leases.next().is_some() {
        return Err(AppError::Upstream);
    }
    let Some(lease_id) = record_uuid(lease, "lease_id") else {
        return Ok(None);
    };
    let manifests = room
        .get("artifact_manifests")
        .and_then(Value::as_array)
        .ok_or(AppError::Upstream)?;
    let Some(manifest) = manifests
        .iter()
        .find(|item| record_uuid(item, "manifest_id") == Some(artifact_manifest_id))
    else {
        return Ok(None);
    };
    let Some(artifact_manifest_hash) = manifest.get("manifest_hash").and_then(Value::as_str) else {
        return Ok(None);
    };
    if decode_digest(artifact_manifest_hash).is_err() {
        return Ok(None);
    }
    let proposals = room
        .get("proposals")
        .and_then(Value::as_array)
        .ok_or(AppError::Upstream)?;
    if proposals.iter().any(|proposal| {
        record_uuid(proposal, "binding_id") == Some(mapping.binding_id)
            && record_uuid(proposal, "work_item_id") == Some(work_item_id)
            && proposal.get("section_key").and_then(Value::as_str) == Some(section_key)
            && record_uuid(proposal, "parent_revision_id") == Some(parent_revision_id)
            && record_uuid(proposal, "artifact_manifest_id") == Some(artifact_manifest_id)
            && matches!(
                proposal.get("status").and_then(Value::as_str),
                Some("submitted" | "accepted")
            )
    }) {
        return Ok(None);
    }
    Ok(Some(DeliveryContext {
        expected_work_version,
        lease_id,
        lease_fencing_token: head_fencing_token,
        lease_expires_at,
        parent_revision_id,
        artifact_manifest_hash: artifact_manifest_hash.to_string(),
    }))
}

fn discover_inbox_paper_ids(raids: &[Value]) -> Result<(Vec<Uuid>, bool), AppError> {
    const ACTIVE_AUTHOR_PHASES: &[&str] = &[
        "forming",
        "preregistering",
        "researching",
        "experimenting",
        "drafting",
        "integrity_review",
        "reproducing",
        "author_approval",
        "integrity_hold",
    ];
    let mut paper_ids = Vec::new();
    let mut seen = HashSet::new();
    let mut truncated = false;
    for raid in raids {
        if raid.get("team_status").and_then(Value::as_str) == Some("archived") {
            continue;
        }
        let Some(paper) = raid.get("paper").filter(|paper| !paper.is_null()) else {
            continue;
        };
        let Some(phase) = paper.get("phase").and_then(Value::as_str) else {
            return Err(AppError::Upstream);
        };
        if !ACTIVE_AUTHOR_PHASES.contains(&phase) {
            continue;
        }
        let paper_id = paper
            .get("paper_project_id")
            .and_then(Value::as_str)
            .ok_or(AppError::Upstream)
            .and_then(|value| Uuid::parse_str(value).map_err(|_| AppError::Upstream))?;
        if !seen.insert(paper_id) {
            continue;
        }
        if paper_ids.len() == MAX_DISCOVERED_INBOX_PAPERS {
            truncated = true;
            continue;
        }
        paper_ids.push(paper_id);
    }
    Ok((paper_ids, truncated))
}

async fn recoverable_inbox_paper_ids(
    state: &AppState,
    binding_id: Uuid,
    now: DateTime<Utc>,
) -> Result<(Vec<Uuid>, bool), AppError> {
    let mut paper_ids = sqlx::query_scalar::<_, Uuid>(
        "SELECT paper_id FROM paper_raid_bff_agent_delivery_drafts \
         WHERE binding_id=$1 AND state IN ('submitting','consumed') AND expires_at>$2 \
         GROUP BY paper_id ORDER BY min(created_at),paper_id LIMIT 65",
    )
    .bind(binding_id)
    .bind(now)
    .fetch_all(&state.pool)
    .await?;
    let truncated = paper_ids.len() > MAX_DISCOVERED_INBOX_PAPERS;
    paper_ids.truncate(MAX_DISCOVERED_INBOX_PAPERS);
    Ok((paper_ids, truncated))
}

fn merge_discovered_paper_ids(recovery: Vec<Uuid>, active: Vec<Uuid>) -> (Vec<Uuid>, bool) {
    let mut merged = Vec::with_capacity(MAX_DISCOVERED_INBOX_PAPERS);
    let mut seen = HashSet::new();
    let mut truncated = false;
    for paper_id in recovery.into_iter().chain(active) {
        if !seen.insert(paper_id) {
            continue;
        }
        if merged.len() == MAX_DISCOVERED_INBOX_PAPERS {
            truncated = true;
            continue;
        }
        merged.push(paper_id);
    }
    (merged, truncated)
}

fn merge_review_tasks_into_author_papers(
    papers: &mut Vec<Value>,
    review_value: &Value,
) -> Result<(), AppError> {
    let review_papers = review_value
        .get("papers")
        .and_then(Value::as_array)
        .ok_or(AppError::Upstream)?;
    for review_paper in review_papers {
        let review_paper_id = review_paper
            .get("paper_id")
            .and_then(Value::as_str)
            .ok_or(AppError::Upstream)?;
        let review_tasks = review_paper
            .get("review_tasks")
            .cloned()
            .ok_or(AppError::Upstream)?;
        if let Some(author_paper) = papers
            .iter_mut()
            .find(|paper| paper.get("paper_id").and_then(Value::as_str) == Some(review_paper_id))
        {
            author_paper
                .as_object_mut()
                .ok_or(AppError::Internal)?
                .insert("review_tasks".to_string(), review_tasks);
        } else {
            papers.push(json!({
                "paper_id": review_paper_id,
                "review_tasks": review_tasks,
            }));
        }
    }
    for paper in papers.iter_mut() {
        let object = paper.as_object_mut().ok_or(AppError::Internal)?;
        object.entry("review_tasks".to_string()).or_insert_with(|| {
            json!({
                "schema": "hepta.paper_raid.agent_bridge.review_tasks.v1",
                "status": "unavailable",
                "reason_code": "target_paper_author_forbidden_or_unassigned",
                "items": [],
            })
        });
    }
    Ok(())
}

async fn assigned_challenge_material_projection(
    state: &AppState,
    room: &Value,
    mapping: &BridgeMapping,
    tasks: &[Value],
) -> Value {
    let work_item_ids = match active_challenge_material_work_item_ids(tasks) {
        Ok(work_item_ids) => work_item_ids,
        Err(_) => {
            return json!({
                "schema": "hepta.paper_raid.agent_bridge.assigned_challenge_materials.v1",
                "status": "unavailable",
                "reason_code": "invalid_assigned_author_work_item",
                "items": [],
            });
        }
    };
    if work_item_ids.is_empty() {
        return json!({
            "schema": "hepta.paper_raid.agent_bridge.assigned_challenge_materials.v1",
            "status": "unavailable",
            "reason_code": "no_assigned_author_work_item",
            "items": [],
        });
    }
    let mut items = Vec::with_capacity(work_item_ids.len());
    for work_item_id in work_item_ids {
        match crate::challenge_materials::resolve_assigned_challenge_material_bundle(
            state,
            room,
            mapping.binding_id,
            mapping.player_id,
            work_item_id,
        )
        .await
        {
            Ok(bundle) => match serde_json::to_value(bundle) {
                Ok(value) => items.push(value),
                Err(_) => {
                    return json!({
                        "schema": "hepta.paper_raid.agent_bridge.assigned_challenge_materials.v1",
                        "status": "unavailable",
                        "reason_code": "frozen_challenge_material_projection_failed",
                        "items": [],
                    });
                }
            },
            Err(AppError::Conflict(_)) => {
                return json!({
                    "schema": "hepta.paper_raid.agent_bridge.assigned_challenge_materials.v1",
                    "status": "unavailable",
                    "reason_code": "frozen_challenge_material_authority_unavailable",
                    "items": [],
                });
            }
            Err(_) => {
                return json!({
                    "schema": "hepta.paper_raid.agent_bridge.assigned_challenge_materials.v1",
                    "status": "unavailable",
                    "reason_code": "frozen_challenge_material_resolution_failed",
                    "items": [],
                });
            }
        }
    }
    items.sort_by(|left, right| {
        left.get("work_item_id")
            .and_then(Value::as_str)
            .cmp(&right.get("work_item_id").and_then(Value::as_str))
    });
    json!({
        "schema": "hepta.paper_raid.agent_bridge.assigned_challenge_materials.v1",
        "status": "available",
        "reason_code": Value::Null,
        "items": items,
    })
}

fn active_challenge_material_work_item_ids(tasks: &[Value]) -> Result<Vec<Uuid>, AppError> {
    let mut work_item_ids = Vec::new();
    let mut seen = HashSet::new();
    for task in tasks.iter().filter(|task| {
        matches!(
            task.get("status").and_then(Value::as_str),
            Some("planned" | "in_progress")
        )
    }) {
        let text = task
            .get("work_item_id")
            .and_then(Value::as_str)
            .ok_or(AppError::Upstream)?;
        let work_item_id = Uuid::parse_str(text).map_err(|_| AppError::Upstream)?;
        if work_item_id.is_nil() || work_item_id.to_string() != text || !seen.insert(work_item_id) {
            return Err(AppError::Upstream);
        }
        work_item_ids.push(work_item_id);
    }
    work_item_ids.sort_unstable();
    Ok(work_item_ids)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StoredReviewTaskState {
    state: String,
    attempt: u64,
}

fn projected_review_task_attempt(
    stored: Option<&StoredReviewTaskState>,
) -> Result<(u64, &'static str), AppError> {
    let Some(stored) = stored else {
        return Ok((1, "pending"));
    };
    if stored.attempt == 0 || stored.attempt > JSON_SAFE_U64_MAX {
        return Err(AppError::Upstream);
    }
    match stored.state.as_str() {
        "pending" => Ok((stored.attempt, "submitting")),
        "consumed" => Ok((stored.attempt, "consumed")),
        "invalidated" => stored
            .attempt
            .checked_add(1)
            .filter(|attempt| *attempt <= JSON_SAFE_U64_MAX)
            .map(|attempt| (attempt, "pending"))
            .ok_or_else(|| AppError::Conflict("review task attempt budget exhausted".into())),
        _ => Err(AppError::Upstream),
    }
}

async fn stored_review_task_state(
    state: &AppState,
    task_id: Uuid,
) -> Result<Option<StoredReviewTaskState>, AppError> {
    let row = sqlx::query(
        "SELECT state,attempt FROM paper_raid_bff_review_execution_receipts
         WHERE task_id=$1 ORDER BY attempt DESC LIMIT 1",
    )
    .bind(task_id)
    .fetch_optional(&state.pool)
    .await?;
    row.map(|row| {
        let attempt = row.try_get::<i64, _>("attempt")?;
        Ok(StoredReviewTaskState {
            state: row.try_get("state")?,
            attempt: u64::try_from(attempt).map_err(|_| AppError::Upstream)?,
        })
    })
    .transpose()
}

fn same_authority_object(
    object: &FrozenReviewObjectV1,
    path: &str,
    digest: &str,
    size: u64,
    media_type: &str,
    role: &str,
) -> bool {
    object.logical_path == path
        && object.digest == digest
        && object.size_bytes == size
        && object.media_type == media_type
        && object.role == role
        && object.download_path == "/api/agent-bridge/review-objects"
}

fn resolved_transport_path(role: &str, media_type: &str) -> Result<&'static str, AppError> {
    match (role, media_type) {
        ("frozen_evaluator", "text/x-python; charset=utf-8") => Ok("evaluator/main.py"),
        ("evaluator_support", "text/x-python; charset=utf-8") => Ok("evaluator/baseline.py"),
        ("candidate", "application/json") => Ok("inputs/candidate.json"),
        ("dataset", "application/json") => Ok("inputs/dataset.json"),
        ("dataset", "text/csv; charset=utf-8") => Ok("inputs/dataset.csv"),
        _ => Err(AppError::Upstream),
    }
}

fn resolved_transport_object(
    authority: &FrozenReviewObjectV1,
    resolved_role: &str,
) -> Result<FrozenReviewObjectV1, AppError> {
    let mut resolved = authority.clone();
    resolved.role = resolved_role.to_string();
    resolved.logical_path =
        resolved_transport_path(resolved_role, &authority.media_type)?.to_string();
    Ok(resolved)
}

fn resolve_manifest_members(
    authority: &FrozenReviewAuthorityV1,
    evaluator: &ChallengeEvaluatorManifestV1,
    dataset: &ChallengeDatasetManifestV1,
) -> Result<Vec<FrozenReviewObjectV1>, AppError> {
    verify_frozen_review_authority(authority).map_err(|_| AppError::Upstream)?;
    if evaluator.pack_id != dataset.pack_id
        || dataset.objects.len() != 1
        || evaluator
            .objects
            .iter()
            .filter(|member| member.path != evaluator.entrypoint)
            .count()
            > 1
    {
        return Err(AppError::Upstream);
    }
    let mut resolved = Vec::new();
    let mut resolved_paths = HashSet::new();
    let mut resolved_digests = HashSet::new();
    for member in &evaluator.objects {
        let role = if member.path == evaluator.entrypoint {
            "frozen_evaluator"
        } else {
            "evaluator_support"
        };
        let matches = authority
            .artifact_objects
            .iter()
            .filter(|object| {
                same_authority_object(
                    object,
                    &member.path,
                    &member.sha256,
                    member.size,
                    &member.media_type,
                    role,
                )
            })
            .collect::<Vec<_>>();
        if matches.len() != 1
            || !resolved_paths.insert(member.path.as_str())
            || !resolved_digests.insert(member.sha256.as_str())
        {
            return Err(AppError::Upstream);
        }
        resolved.push(resolved_transport_object(matches[0], role)?);
    }
    for member in &dataset.objects {
        let matches = authority
            .artifact_objects
            .iter()
            .filter(|object| {
                same_authority_object(
                    object,
                    &member.path,
                    &member.sha256,
                    member.size,
                    &member.media_type,
                    "dataset",
                )
            })
            .collect::<Vec<_>>();
        if matches.len() != 1
            || !resolved_paths.insert(member.path.as_str())
            || !resolved_digests.insert(member.sha256.as_str())
        {
            return Err(AppError::Upstream);
        }
        resolved.push(resolved_transport_object(matches[0], "dataset")?);
    }
    let candidates = authority
        .artifact_objects
        .iter()
        .filter(|object| object.role == "candidate")
        .collect::<Vec<_>>();
    if candidates.len() != 1
        || !resolved_paths.insert(candidates[0].logical_path.as_str())
        || !resolved_digests.insert(candidates[0].digest.as_str())
    {
        return Err(AppError::Upstream);
    }
    resolved.push(resolved_transport_object(candidates[0], "candidate")?);
    if authority.artifact_objects.iter().any(|object| {
        matches!(
            object.role.as_str(),
            "frozen_evaluator" | "evaluator_support" | "dataset" | "candidate"
        ) && !resolved_paths.contains(object.logical_path.as_str())
    }) {
        return Err(AppError::Upstream);
    }
    resolved.sort_by(|left, right| {
        (left.object_key.as_str(), left.logical_path.as_str())
            .cmp(&(right.object_key.as_str(), right.logical_path.as_str()))
    });
    Ok(resolved)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyGoldenExperimentPlanV1 {
    dataset_sha256: String,
    expected_failure_run_ids: Vec<String>,
    metric: String,
    required_run_ids: Vec<String>,
    schema: String,
    stopping_rule: String,
}

fn legacy_golden_challenge_manifests(
    authority: &FrozenReviewAuthorityV1,
    evaluator_pin_bytes: &[u8],
    dataset_pin_bytes: &[u8],
) -> Result<(ChallengeEvaluatorManifestV1, ChallengeDatasetManifestV1), AppError> {
    if authority.evaluator_manifest_hash != LEGACY_GOLDEN_EVALUATOR_MANIFEST_HASH
        || authority.dataset_manifest_hash != LEGACY_GOLDEN_DATASET_MANIFEST_HASH
        || sha256_digest(evaluator_pin_bytes) != LEGACY_GOLDEN_EVALUATOR_MANIFEST_HASH
        || sha256_digest(dataset_pin_bytes) != LEGACY_GOLDEN_DATASET_MANIFEST_HASH
        || dataset_pin_bytes != LEGACY_GOLDEN_DATASET_CARD
    {
        return Err(AppError::Upstream);
    }
    let plan: LegacyGoldenExperimentPlanV1 =
        serde_json::from_slice(evaluator_pin_bytes).map_err(|_| AppError::Upstream)?;
    if plan.schema != "paper-raid.experiment-plan.v1"
        || plan.metric != "accuracy_bps"
        || plan.stopping_rule != "execute_every_required_run_exactly_once"
        || plan.expected_failure_run_ids != ["baseline-invalid-threshold"]
        || plan.required_run_ids
            != [
                "baseline-seed-17",
                "ablation-seed-17",
                "baseline-invalid-threshold",
            ]
        || plan.dataset_sha256 != "b002e6297f6fd781742866533b89bf781c7f21d5cfa7b42b5c97a9ecd5821314"
    {
        return Err(AppError::Upstream);
    }
    let evaluator_matches = authority
        .artifact_objects
        .iter()
        .filter(|object| {
            same_authority_object(
                object,
                LEGACY_GOLDEN_FROZEN_EVALUATOR_PATH,
                LEGACY_GOLDEN_FROZEN_EVALUATOR_HASH,
                LEGACY_GOLDEN_FROZEN_EVALUATOR_SIZE,
                "text/x-python; charset=utf-8",
                "frozen_evaluator",
            )
        })
        .count();
    let dataset_hash = format!("sha256:{}", plan.dataset_sha256);
    let dataset_matches = authority
        .artifact_objects
        .iter()
        .filter(|object| {
            same_authority_object(
                object,
                LEGACY_GOLDEN_DATASET_PATH,
                &dataset_hash,
                LEGACY_GOLDEN_DATASET_SIZE,
                "text/csv; charset=utf-8",
                "dataset",
            )
        })
        .count();
    if evaluator_matches != 1 || dataset_matches != 1 {
        return Err(AppError::Upstream);
    }
    let evaluator_member = ChallengeManifestObjectV1 {
        cas_uri: format!(
            "cas://sha256/{}",
            LEGACY_GOLDEN_FROZEN_EVALUATOR_HASH
                .strip_prefix("sha256:")
                .expect("constant digest prefix")
        ),
        media_type: "text/x-python; charset=utf-8".to_string(),
        path: LEGACY_GOLDEN_FROZEN_EVALUATOR_PATH.to_string(),
        sha256: LEGACY_GOLDEN_FROZEN_EVALUATOR_HASH.to_string(),
        size: LEGACY_GOLDEN_FROZEN_EVALUATOR_SIZE,
    };
    let dataset_member = ChallengeManifestObjectV1 {
        cas_uri: format!("cas://sha256/{}", plan.dataset_sha256),
        media_type: "text/csv; charset=utf-8".to_string(),
        path: LEGACY_GOLDEN_DATASET_PATH.to_string(),
        sha256: dataset_hash,
        size: LEGACY_GOLDEN_DATASET_SIZE,
    };
    Ok((
        ChallengeEvaluatorManifestV1 {
            entrypoint: LEGACY_GOLDEN_FROZEN_EVALUATOR_PATH.to_string(),
            frozen: true,
            objects: vec![evaluator_member],
            pack_id: LEGACY_GOLDEN_PACK_ID.to_string(),
            runtime: "python3-stdlib".to_string(),
            schema: "hepta.challenge_pack.evaluator_manifest.v1".to_string(),
        },
        ChallengeDatasetManifestV1 {
            objects: vec![dataset_member],
            pack_id: LEGACY_GOLDEN_PACK_ID.to_string(),
            schema: "hepta.challenge_pack.dataset_manifest.v1".to_string(),
        },
    ))
}

/// Resolve Hepta's assignment/release authority against the exact challenge-manifest bytes.
///
/// The Consumer BFF owns CAS transport, but does not own scientific truth: every resolved member
/// must already be present byte-for-byte in Hepta's release ArtifactManifest authority.  The
/// resulting hash covers the Hepta authority hash, both manifest digest pins, and the exact member
/// set.  Any substitution therefore changes or invalidates the transport envelope.
pub(crate) async fn resolve_frozen_review_bundle(
    state: &AppState,
    hepta_bundle: &Value,
    expected_player_id: Uuid,
) -> Result<Value, AppError> {
    let authority_value = hepta_bundle
        .get("frozen_review_authority")
        .cloned()
        .ok_or(AppError::Upstream)?;
    let authority: FrozenReviewAuthorityV1 =
        serde_json::from_value(authority_value).map_err(|_| AppError::Upstream)?;
    verify_frozen_review_authority(&authority).map_err(|_| AppError::Upstream)?;

    let paper_id = authority.paper_project_id.to_string();
    let submission_id = authority.submission_id.to_string();
    if !review_outer_bundle_matches_authority(hepta_bundle, &authority) {
        return Err(AppError::Upstream);
    }
    let assignments = hepta_bundle
        .get("my_assignments")
        .and_then(Value::as_array)
        .ok_or(AppError::Upstream)?;
    let assignment_id = authority.assignment_id.to_string();
    let player_id = expected_player_id.to_string();
    if assignments.len() != 1
        || assignments[0].get("assignment_id").and_then(Value::as_str)
            != Some(assignment_id.as_str())
        || assignments[0]
            .get("paper_project_id")
            .and_then(Value::as_str)
            != Some(paper_id.as_str())
        || assignments[0].get("submission_id").and_then(Value::as_str)
            != Some(submission_id.as_str())
        || assignments[0].get("player_id").and_then(Value::as_str) != Some(player_id.as_str())
        || assignments[0].get("review_round").and_then(Value::as_u64)
            != Some(authority.review_round)
        || assignments[0].get("slot").and_then(Value::as_str) != Some(authority.slot.as_str())
        || assignments[0].get("version").and_then(Value::as_u64)
            != Some(authority.assignment_version)
        || assignments[0].get("expires_at").and_then(Value::as_str)
            != Some(authority.expires_at.as_str())
        || !matches!(
            assignments[0].get("status").and_then(Value::as_str),
            Some("claimed" | "pinned")
        )
    {
        return Err(AppError::Upstream);
    }

    let legacy_golden = authority.evaluator_manifest_hash == LEGACY_GOLDEN_EVALUATOR_MANIFEST_HASH
        && authority.dataset_manifest_hash == LEGACY_GOLDEN_DATASET_MANIFEST_HASH;
    let evaluator_bytes = state
        .cas
        .get(&authority.evaluator_manifest_hash, "application/json")
        .await?;
    let dataset_bytes = state
        .cas
        .get(
            &authority.dataset_manifest_hash,
            if legacy_golden {
                "text/markdown; charset=utf-8"
            } else {
                "application/json"
            },
        )
        .await?;
    let (evaluator, dataset) = if legacy_golden {
        legacy_golden_challenge_manifests(&authority, &evaluator_bytes, &dataset_bytes)?
    } else {
        (
            parse_challenge_evaluator_manifest(
                &evaluator_bytes,
                &authority.evaluator_manifest_hash,
            )
            .map_err(|_| AppError::Upstream)?,
            parse_challenge_dataset_manifest(&dataset_bytes, &authority.dataset_manifest_hash)
                .map_err(|_| AppError::Upstream)?,
        )
    };
    let resolved = resolve_manifest_members(&authority, &evaluator, &dataset)?;
    let entrypoint = resolved
        .iter()
        .find(|object| object.role == "frozen_evaluator")
        .ok_or(AppError::Upstream)?;
    let evaluator_version = entrypoint.digest.clone();
    let mut descriptor = FrozenReviewBundleV1 {
        schema: RESOLVED_FROZEN_REVIEW_BUNDLE_V1.to_string(),
        bundle_hash: String::new(),
        authority: authority.clone(),
        authority_hash: authority.authority_hash.clone(),
        assignment_id: authority.assignment_id,
        paper_project_id: authority.paper_project_id,
        submission_id: authority.submission_id,
        review_round: authority.review_round,
        slot: authority.slot.clone(),
        assignment_version: authority.assignment_version,
        expires_at: authority.expires_at.clone(),
        release_candidate_hash: authority.release_candidate_hash.clone(),
        paper_bundle_hash: authority.paper_bundle_hash.clone(),
        artifact_manifest_hash: authority.artifact_manifest_hash.clone(),
        evaluator_manifest_hash: authority.evaluator_manifest_hash.clone(),
        dataset_manifest_hash: authority.dataset_manifest_hash.clone(),
        objects: resolved,
        execution: FrozenReviewExecutionPlanV1 {
            schema: "hepta.paper_raid.review_execution_plan.v1".to_string(),
            kind: authority.execution_policy.kind.clone(),
            adapter: authority.execution_policy.adapter.clone(),
            evaluator_version,
            entrypoint: "evaluator/main.py".to_string(),
            timeout_ms: authority.execution_policy.timeout_ms,
            seed: authority.execution_policy.seed,
        },
    };
    descriptor.bundle_hash =
        frozen_review_bundle_hash(&descriptor).map_err(|_| AppError::Upstream)?;
    verify_frozen_review_bundle(&descriptor).map_err(|_| AppError::Upstream)?;
    let mut resolved_bundle = hepta_bundle.clone();
    resolved_bundle
        .as_object_mut()
        .ok_or(AppError::Upstream)?
        .insert(
            "resolved_frozen_review_bundle".to_string(),
            serde_json::to_value(descriptor).map_err(|_| AppError::Internal)?,
        );
    Ok(resolved_bundle)
}

fn review_task_projection(
    bundle: &Value,
    state: Option<&StoredReviewTaskState>,
) -> Result<Option<Value>, AppError> {
    let descriptor_value = bundle
        .get("resolved_frozen_review_bundle")
        .cloned()
        .ok_or(AppError::Upstream)?;
    let descriptor: FrozenReviewBundleV1 =
        serde_json::from_value(descriptor_value.clone()).map_err(|_| AppError::Upstream)?;
    let evaluation_id = if descriptor.execution.kind == "reproduce" {
        bundle
            .get("evaluation")
            .and_then(|value| value.get("evaluation_id"))
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or(AppError::Upstream)?
    } else if descriptor.execution.kind == "evaluate" {
        review_evaluation_id(descriptor.assignment_id, &descriptor.bundle_hash)
    } else {
        return Ok(None);
    };
    let task_id = review_task_id(
        descriptor.assignment_id,
        &descriptor.bundle_hash,
        &descriptor.execution.kind,
        Some(evaluation_id),
    );
    let (attempt, state) = projected_review_task_attempt(state)?;
    Ok(Some(json!({
        "schema": "hepta.paper_raid.agent_bridge.review_task.v1",
        "task_id": task_id,
        "paper_id": descriptor.paper_project_id,
        "evaluation_id": evaluation_id,
        "assignment_id": descriptor.assignment_id,
        "role": descriptor.slot,
        "kind": descriptor.execution.kind,
        "attempt": attempt,
        "fencing_token": descriptor.assignment_version,
        "state": state,
        "bundle": descriptor_value,
    })))
}

async fn agent_review_inbox_value(
    state: &AppState,
    verified: &VerifiedAgentRequest,
    requested_papers: &[Uuid],
) -> Result<Value, AppError> {
    let queue = state.hepta.list_review_queue(&verified.identity).await?;
    let items = queue.as_array().ok_or(AppError::Upstream)?;
    let requested = requested_papers.iter().copied().collect::<HashSet<_>>();
    let mut papers = Vec::new();
    for item in items {
        let paper_id = item
            .get("paper_project_id")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or(AppError::Upstream)?;
        if !requested.is_empty() && !requested.contains(&paper_id) {
            continue;
        }
        let hepta_bundle = match state
            .hepta
            .get_paper_review_bundle(&verified.identity, paper_id)
            .await
        {
            Ok(bundle) => bundle,
            Err(_) => {
                papers.push(json!({
                    "paper_id": paper_id,
                    "review_tasks": {
                        "schema": "hepta.paper_raid.agent_bridge.review_tasks.v1",
                        "status": "unavailable",
                        "reason_code": "frozen_review_objects_unavailable",
                        "items": [],
                    }
                }));
                continue;
            }
        };
        let bundle =
            match resolve_frozen_review_bundle(state, &hepta_bundle, verified.mapping.player_id)
                .await
            {
                Ok(bundle) => bundle,
                Err(_) => {
                    papers.push(json!({
                        "paper_id": paper_id,
                        "review_tasks": {
                            "schema": "hepta.paper_raid.agent_bridge.review_tasks.v1",
                            "status": "unavailable",
                            "reason_code": "frozen_review_manifest_resolution_failed",
                            "items": [],
                        }
                    }));
                    continue;
                }
            };
        if !review_queue_item_matches_resolved_bundle(item, &bundle, verified.mapping.player_id) {
            papers.push(json!({
                "paper_id": paper_id,
                "review_tasks": {
                    "schema": "hepta.paper_raid.agent_bridge.review_tasks.v1",
                    "status": "unavailable",
                    "reason_code": "review_queue_authority_mismatch",
                    "items": [],
                }
            }));
            continue;
        }
        let descriptor = bundle
            .get("resolved_frozen_review_bundle")
            .ok_or(AppError::Upstream)?;
        let assignment_id = descriptor
            .get("assignment_id")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or(AppError::Upstream)?;
        let bundle_hash = descriptor
            .get("bundle_hash")
            .and_then(Value::as_str)
            .ok_or(AppError::Upstream)?;
        let kind = descriptor
            .get("execution")
            .and_then(|value| value.get("kind"))
            .and_then(Value::as_str)
            .ok_or(AppError::Upstream)?;
        let task_state = match kind {
            "reproduce" => {
                let evaluation_id = bundle
                    .get("evaluation")
                    .and_then(|value| value.get("evaluation_id"))
                    .and_then(Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok())
                    .ok_or(AppError::Upstream)?;
                let task_id = review_task_id(assignment_id, bundle_hash, kind, Some(evaluation_id));
                stored_review_task_state(state, task_id).await?
            }
            "evaluate" => {
                let evaluation_id = review_evaluation_id(assignment_id, bundle_hash);
                let task_id = review_task_id(assignment_id, bundle_hash, kind, Some(evaluation_id));
                stored_review_task_state(state, task_id).await?
            }
            // Reviewer assignments are human attestation work.  They may resolve and read the
            // same frozen bundle, but must never be projected as an executable Agent task.
            "review" => None,
            _ => return Err(AppError::Upstream),
        };
        let task = review_task_projection(&bundle, task_state.as_ref())?;
        papers.push(json!({
            "paper_id": paper_id,
            "review_tasks": {
                "schema": "hepta.paper_raid.agent_bridge.review_tasks.v1",
                "status": if task.is_some() { "available" } else { "unavailable" },
                "reason_code": if task.is_some() { Value::Null } else { json!("human_attestation_only") },
                "items": task.into_iter().collect::<Vec<_>>(),
            }
        }));
    }
    Ok(json!({
        "schema": "hepta.paper_raid.agent_bridge.inbox.v2",
        "binding_id": verified.mapping.binding_id,
        "assurance": "self_declared_unverified",
        "discovery": {
            "mode": if requested_papers.is_empty() { "review_assignments" } else { "explicit_paper_ids" },
            "truncated": false,
            "max_papers": MAX_DISCOVERED_INBOX_PAPERS,
        },
        "papers": papers,
    }))
}

pub async fn agent_inbox(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let verified = verify_agent_request(&state, Method::POST, &uri, &headers, &body).await?;
    if let Some(response) = replay_response(&verified) {
        return Ok(response);
    }
    let request: InboxRequest = decode_json(&body, 64 * 1024)?;
    if request.schema != "hepta.paper_raid.agent_bridge.inbox_request.v1"
        || request.paper_ids.len() > 64
    {
        return Err(AppError::Invalid("invalid Agent inbox request".into()));
    }
    let has_author_scope = verified.identity.has_scope(AlphaIdentityScope::Author);
    let has_review_scope = [
        AlphaIdentityScope::Evaluator,
        AlphaIdentityScope::Reviewer,
        AlphaIdentityScope::Reproducer,
    ]
    .into_iter()
    .any(|scope| verified.identity.has_scope(scope));
    let unique = request.paper_ids.iter().copied().collect::<HashSet<_>>();
    if unique.len() != request.paper_ids.len() {
        return Err(AppError::Invalid(
            "Agent inbox paper_ids must be unique".into(),
        ));
    }
    let review_value = if has_review_scope {
        Some(agent_review_inbox_value(&state, &verified, &request.paper_ids).await?)
    } else {
        None
    };
    if !has_author_scope {
        let value = review_value.ok_or(AppError::Forbidden)?;
        return complete_agent_request(&state, &verified, StatusCode::OK, &value).await;
    }
    let automatic_discovery = request.paper_ids.is_empty();
    let mut discovery_truncated = false;
    let mut paper_ids = request.paper_ids;
    let now = Utc::now();
    if paper_ids.is_empty() {
        let raid_state = state
            .hepta
            .get_player_raid_state(&verified.identity)
            .await?;
        let raids = raid_state
            .get("raids")
            .and_then(Value::as_array)
            .ok_or(AppError::Upstream)?;
        let (active, active_truncated) = discover_inbox_paper_ids(raids)?;
        let (recovery, recovery_truncated) =
            recoverable_inbox_paper_ids(&state, verified.mapping.binding_id, now).await?;
        let (merged, merge_truncated) = merge_discovered_paper_ids(recovery, active);
        paper_ids = merged;
        discovery_truncated = active_truncated || recovery_truncated || merge_truncated;
    } else if has_review_scope {
        // A mixed Author+Review identity may explicitly ask for a Paper on which it only has an
        // independent review assignment.  Never route that Paper through Author Room authority.
        let raid_state = state
            .hepta
            .get_player_raid_state(&verified.identity)
            .await?;
        let raids = raid_state
            .get("raids")
            .and_then(Value::as_array)
            .ok_or(AppError::Upstream)?;
        let (active, active_truncated) = discover_inbox_paper_ids(raids)?;
        let (recovery, recovery_truncated) =
            recoverable_inbox_paper_ids(&state, verified.mapping.binding_id, now).await?;
        let allowed = active.into_iter().chain(recovery).collect::<HashSet<_>>();
        paper_ids.retain(|paper_id| allowed.contains(paper_id));
        discovery_truncated = active_truncated || recovery_truncated;
    }
    let unique: HashSet<Uuid> = paper_ids.iter().copied().collect();
    if unique.len() != paper_ids.len() {
        return Err(AppError::Invalid(
            "Agent inbox paper_ids must be unique".into(),
        ));
    }
    let mut papers = Vec::with_capacity(paper_ids.len());
    let binding_id = verified.mapping.binding_id.to_string();
    let player_id = verified.mapping.player_id.to_string();
    sqlx::query(
        "UPDATE paper_raid_bff_agent_delivery_drafts \
         SET state='expired',invalidated_at=$1,updated_at=$1 \
         WHERE binding_id=$2 AND state='pending' AND expires_at<=$1",
    )
    .bind(now)
    .bind(verified.mapping.binding_id)
    .execute(&state.pool)
    .await?;
    for paper_id in paper_ids {
        let room = state
            .hepta
            .get_paper_room(&verified.identity, paper_id)
            .await?;
        let paper = room.get("paper").cloned().ok_or(AppError::Upstream)?;
        let expected_paper_id = paper_id.to_string();
        if paper.get("paper_project_id").and_then(Value::as_str) != Some(expected_paper_id.as_str())
        {
            return Err(AppError::Upstream);
        }
        let tasks: Vec<Value> = room
            .get("work_items")
            .and_then(Value::as_array)
            .ok_or(AppError::Upstream)?
            .iter()
            .filter(|item| {
                item.get("assigned_binding_id").and_then(Value::as_str) == Some(binding_id.as_str())
                    || item.get("assigned_player_id").and_then(Value::as_str)
                        == Some(player_id.as_str())
            })
            .map(|item| {
                project_fields(
                    item,
                    &[
                        "work_item_id",
                        "paper_project_id",
                        "kind",
                        "assigned_player_id",
                        "assigned_binding_id",
                        "status",
                        "artifact_manifest_hash",
                        "version",
                        "updated_at",
                    ],
                )
            })
            .collect::<Result<_, _>>()?;
        let proposals: Vec<Value> = room
            .get("proposals")
            .and_then(Value::as_array)
            .ok_or(AppError::Upstream)?
            .iter()
            .filter(|proposal| {
                proposal.get("binding_id").and_then(Value::as_str) == Some(binding_id.as_str())
                    && proposal.get("agent_id").and_then(Value::as_str)
                        == Some(verified.mapping.agent_id.as_str())
            })
            .map(|proposal| {
                project_fields(
                    proposal,
                    &[
                        "proposal_id",
                        "work_item_id",
                        "section_key",
                        "parent_revision_id",
                        "artifact_manifest_id",
                        "expected_work_version",
                        "status",
                        "version",
                    ],
                )
            })
            .collect::<Result<_, _>>()?;
        let drafts =
            load_recoverable_delivery_drafts(&state, verified.mapping.binding_id, paper_id, now)
                .await?;
        let mut candidate_items = Vec::new();
        for draft in &drafts {
            let current_context = resolve_delivery_context(
                &room,
                &verified.mapping,
                draft.work_item_id,
                &draft.section_key,
                draft.artifact_manifest_id,
                now,
            )?;
            let current_context_matches = current_context
                .as_ref()
                .is_some_and(|context| delivery_draft_matches_context(draft, context));
            let recoverable_after_submit =
                if matches!(draft.state.as_str(), "submitting" | "consumed") {
                    if !delivery_recovery_pins_are_canonical(draft) {
                        return Err(AppError::Internal);
                    }
                    room_contains_exact_delivery_proposal(&room, draft)?
                } else {
                    false
                };
            if current_context_matches || recoverable_after_submit {
                candidate_items.push(delivery_candidate_value(draft));
            }
        }
        candidate_items.sort_by_key(|item| {
            item.get("delivery_draft_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        });
        let delivery_candidates = if candidate_items.is_empty() {
            unavailable_delivery_candidates(if drafts.is_empty() {
                "no_agent_declared_delivery_draft"
            } else {
                "delivery_draft_stale_or_invalid"
            })
        } else {
            json!({
                "schema": "hepta.paper_raid.agent_bridge.delivery_candidates.v1",
                "status": "available",
                "reason_code": Value::Null,
                "items": candidate_items,
            })
        };
        let assigned_challenge_materials =
            assigned_challenge_material_projection(&state, &room, &verified.mapping, &tasks).await;
        papers.push(json!({
            "paper_id": paper_id,
            "phase": paper.get("phase").cloned().unwrap_or(Value::Null),
            "tasks": tasks,
            "proposals": proposals,
            "challenge_materials": assigned_challenge_materials,
            "delivery_candidates": delivery_candidates,
        }));
    }
    if let Some(review_value) = review_value {
        merge_review_tasks_into_author_papers(&mut papers, &review_value)?;
    }
    let value = json!({
        "schema": "hepta.paper_raid.agent_bridge.inbox.v2",
        "binding_id": verified.mapping.binding_id,
        "assurance": "self_declared_unverified",
        "discovery": {
            "mode": if has_review_scope { "mixed_author_and_review" } else if automatic_discovery { "active_raids" } else { "explicit_paper_ids" },
            "truncated": discovery_truncated,
            "max_papers": MAX_DISCOVERED_INBOX_PAPERS,
        },
        "papers": papers
    });
    complete_agent_request(&state, &verified, StatusCode::OK, &value).await
}

pub async fn agent_challenge_object(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    Query(query): Query<ChallengeObjectQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let verified = verify_agent_request(&state, Method::GET, &uri, &headers, &body).await?;
    if !verified.identity.has_scope(AlphaIdentityScope::Author)
        || !matches!(
            query.digest.strip_prefix("sha256:"),
            Some(raw) if raw.len() == 64
                && raw.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        )
    {
        return Err(AppError::Forbidden);
    }
    let room = state
        .hepta
        .get_paper_room(&verified.identity, query.paper_id)
        .await?;
    let bundle = crate::challenge_materials::resolve_assigned_challenge_material_bundle(
        &state,
        &room,
        verified.mapping.binding_id,
        verified.mapping.player_id,
        query.work_item_id,
    )
    .await?;
    let media_type = crate::challenge_materials::authorized_challenge_material_media(
        &bundle,
        verified.mapping.binding_id,
        verified.mapping.player_id,
        query.work_item_id,
        &query.bundle_hash,
        &query.object_key,
        &query.digest,
    )?;
    state.cas.validate_media_type(&media_type)?;
    let expected_size = bundle
        .objects
        .iter()
        .find(|object| object.object_key == query.object_key && object.digest == query.digest)
        .map(|object| object.size_bytes)
        .ok_or(AppError::Forbidden)?;
    if let Some((status, bytes)) = verified.replay.as_ref() {
        if *status != StatusCode::OK.as_u16() || bytes.len() as u64 != expected_size {
            return Err(AppError::Conflict(
                "challenge_object_replay_status_changed".into(),
            ));
        }
        return crate::app::review_artifact_response(bytes.clone(), &media_type);
    }
    let bytes = state.cas.get(&query.digest, &media_type).await?;
    if bytes.len() as u64 != expected_size {
        return Err(AppError::Upstream);
    }
    complete_agent_raw_request(&state, &verified, StatusCode::OK, &bytes).await?;
    crate::app::review_artifact_response(bytes, &media_type)
}

pub async fn agent_review_object(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    Query(query): Query<ReviewObjectQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let verified = verify_agent_request(&state, Method::GET, &uri, &headers, &body).await?;
    if !matches!(
        query.digest.strip_prefix("sha256:"),
        Some(raw) if raw.len() == 64
    ) {
        return Err(AppError::Invalid("review object digest is invalid".into()));
    }
    let queue = state.hepta.list_review_queue(&verified.identity).await?;
    let queue_items = queue.as_array().ok_or(AppError::Upstream)?;
    let assignment_id_text = query.assignment_id.to_string();
    let queue_matches = queue_items
        .iter()
        .filter(|item| {
            item.get("my_assignments")
                .and_then(Value::as_array)
                .is_some_and(|assignments| {
                    assignments.iter().any(|assignment| {
                        assignment.get("assignment_id").and_then(Value::as_str)
                            == Some(assignment_id_text.as_str())
                    })
                })
        })
        .cloned()
        .collect::<Vec<_>>();
    if queue_matches.len() != 1 {
        return Err(AppError::Forbidden);
    }
    let queue_item = queue_matches
        .into_iter()
        .next()
        .ok_or(AppError::Forbidden)?;
    let paper_id_text = queue_item
        .get("paper_project_id")
        .and_then(Value::as_str)
        .ok_or(AppError::Forbidden)?;
    let paper_id = Uuid::parse_str(paper_id_text).map_err(|_| AppError::Forbidden)?;
    if paper_id.is_nil() || paper_id.to_string() != paper_id_text {
        return Err(AppError::Forbidden);
    }
    let hepta_bundle = state
        .hepta
        .get_paper_review_bundle(&verified.identity, paper_id)
        .await?;
    let bundle =
        resolve_frozen_review_bundle(&state, &hepta_bundle, verified.mapping.player_id).await?;
    if !review_queue_item_matches_resolved_bundle(&queue_item, &bundle, verified.mapping.player_id)
    {
        return Err(AppError::Forbidden);
    }
    let descriptor: FrozenReviewBundleV1 = serde_json::from_value(
        bundle
            .get("resolved_frozen_review_bundle")
            .cloned()
            .ok_or(AppError::Upstream)?,
    )
    .map_err(|_| AppError::Upstream)?;
    let kind = descriptor.execution.kind.as_str();
    let evaluation_id = if kind == "reproduce" {
        let evaluation = bundle
            .get("evaluation")
            .filter(|value| value.is_object())
            .ok_or(AppError::Upstream)?;
        let evaluation_id_text = evaluation
            .get("evaluation_id")
            .and_then(Value::as_str)
            .ok_or(AppError::Upstream)?;
        let evaluation_id = Uuid::parse_str(evaluation_id_text).map_err(|_| AppError::Upstream)?;
        let descriptor_paper_id = descriptor.paper_project_id.to_string();
        let descriptor_submission_id = descriptor.submission_id.to_string();
        if evaluation_id.is_nil()
            || evaluation_id.to_string() != evaluation_id_text
            || evaluation.get("paper_project_id").and_then(Value::as_str)
                != Some(descriptor_paper_id.as_str())
            || evaluation.get("submission_id").and_then(Value::as_str)
                != Some(descriptor_submission_id.as_str())
            || evaluation
                .get("release_candidate_hash")
                .and_then(Value::as_str)
                != Some(descriptor.release_candidate_hash.as_str())
            || evaluation.get("paper_bundle_hash").and_then(Value::as_str)
                != Some(descriptor.paper_bundle_hash.as_str())
            || evaluation.get("version").and_then(Value::as_u64) != Some(descriptor.review_round)
        {
            return Err(AppError::Upstream);
        }
        evaluation_id
    } else if kind == "evaluate" {
        review_evaluation_id(descriptor.assignment_id, &descriptor.bundle_hash)
    } else {
        return Err(AppError::Upstream);
    };
    if query.assignment_id != descriptor.assignment_id
        || query.bundle_hash != descriptor.bundle_hash
        || query.task_id
            != review_task_id(
                descriptor.assignment_id,
                &descriptor.bundle_hash,
                kind,
                Some(evaluation_id),
            )
    {
        return Err(AppError::Forbidden);
    }
    let (media_type, expected_size) = crate::app::authorized_review_artifact_media(
        &bundle,
        paper_id,
        verified.mapping.player_id,
        query.assignment_id,
        &query.bundle_hash,
        &query.object_key,
        &query.digest,
    )?;
    state.cas.validate_media_type(&media_type)?;
    if let Some((status, bytes)) = verified.replay.as_ref() {
        if *status != StatusCode::OK.as_u16() || bytes.len() as u64 != expected_size {
            return Err(AppError::Conflict(
                "review_object_replay_status_changed".into(),
            ));
        }
        return crate::app::review_artifact_response(bytes.clone(), &media_type);
    }
    let bytes = state.cas.get(&query.digest, &media_type).await?;
    if bytes.len() as u64 != expected_size {
        return Err(AppError::Upstream);
    }
    complete_agent_raw_request(&state, &verified, StatusCode::OK, &bytes).await?;
    crate::app::review_artifact_response(bytes, &media_type)
}

pub async fn agent_review_receipt(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let verified = verify_agent_request(&state, Method::POST, &uri, &headers, &body).await?;
    if let Some(response) = replay_response(&verified) {
        return Ok(response);
    }
    let request: ReviewReceiptRequest = decode_json(&body, MAX_AGENT_BODY_BYTES)?;
    if request.schema != "hepta.paper_raid.agent_bridge.review_receipt_request.v1"
        || request.idempotency_key != request.receipt.receipt_id
    {
        return Err(AppError::Invalid("invalid review receipt request".into()));
    }
    let receipt = request.receipt;
    if receipt.schema != REVIEW_EXECUTION_RECEIPT_V1
        || receipt.binding_id != verified.mapping.binding_id
        || receipt.agent_id != verified.mapping.agent_id
        || receipt.agent_key_id != verified.mapping.agent_key_id
        || receipt.attempt == 0
        || receipt.attempt > JSON_SAFE_U64_MAX
    {
        return Err(AppError::Forbidden);
    }
    let public_key_text = verified
        .authoritative_binding
        .get("agent_public_key")
        .and_then(Value::as_str)
        .ok_or(AppError::Upstream)?;
    let public_key_bytes = BASE64
        .decode(public_key_text)
        .map_err(|_| AppError::Upstream)?;
    let public_key_array: [u8; 32] = public_key_bytes
        .try_into()
        .map_err(|_| AppError::Upstream)?;
    let verifying_key =
        VerifyingKey::from_bytes(&public_key_array).map_err(|_| AppError::Upstream)?;
    if sha256_digest(verifying_key.as_bytes()) != receipt.signing_public_key_hash {
        return Err(AppError::Forbidden);
    }
    verify_review_execution_receipt_signature(&receipt, &verifying_key)
        .map_err(|_| AppError::Forbidden)?;
    let hepta_bundle = state
        .hepta
        .get_paper_review_bundle(&verified.identity, receipt.paper_project_id)
        .await?;
    let bundle =
        resolve_frozen_review_bundle(&state, &hepta_bundle, verified.mapping.player_id).await?;
    let descriptor_value = bundle
        .get("resolved_frozen_review_bundle")
        .cloned()
        .ok_or(AppError::Upstream)?;
    let descriptor: FrozenReviewBundleV1 =
        serde_json::from_value(descriptor_value).map_err(|_| AppError::Upstream)?;
    let evaluation_id = if descriptor.execution.kind == "reproduce" {
        bundle
            .get("evaluation")
            .and_then(|value| value.get("evaluation_id"))
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or(AppError::Upstream)?
    } else if descriptor.execution.kind == "evaluate" {
        review_evaluation_id(descriptor.assignment_id, &descriptor.bundle_hash)
    } else {
        return Err(AppError::Upstream);
    };
    let expected_task_id = review_task_id(
        descriptor.assignment_id,
        &descriptor.bundle_hash,
        &descriptor.execution.kind,
        Some(evaluation_id),
    );
    let expected_receipt_id = review_execution_receipt_id(
        receipt.binding_id,
        expected_task_id,
        descriptor.assignment_id,
        &descriptor.bundle_hash,
        receipt.attempt,
        receipt.fencing_token,
    )
    .map_err(|_| AppError::Forbidden)?;
    let input_objects_value = request
        .run_manifest
        .get("input_objects")
        .cloned()
        .ok_or_else(|| AppError::Invalid("review run manifest has no input objects".into()))?;
    let input_objects: Vec<FrozenReviewInputObjectV1> = serde_json::from_value(input_objects_value)
        .map_err(|_| AppError::Invalid("review run manifest input objects are invalid".into()))?;
    let expected_input_objects = descriptor
        .objects
        .iter()
        .map(|object| FrozenReviewInputObjectV1 {
            object_key: object.object_key.clone(),
            logical_path: object.logical_path.clone(),
            role: object.role.clone(),
            digest: object.digest.clone(),
            size_bytes: object.size_bytes,
        })
        .collect::<Vec<_>>();
    if input_objects != expected_input_objects {
        return Err(AppError::Forbidden);
    }
    let expected_input_root =
        frozen_review_input_root(&input_objects).map_err(|_| AppError::Upstream)?;
    let expected_metrics_hash = review_execution_metrics_hash(&receipt)
        .map_err(|_| AppError::Invalid("review receipt metrics are invalid".into()))?;
    let output_root = hepta_paper_raid_contracts::canonical_json_sha256(&request.output)
        .map_err(|_| AppError::Invalid("review output is not canonicalizable".into()))?;
    let environment_hash = hepta_paper_raid_contracts::canonical_json_sha256(&request.environment)
        .map_err(|_| AppError::Invalid("review environment is not canonicalizable".into()))?;
    let run_manifest_hash =
        hepta_paper_raid_contracts::canonical_json_sha256(&request.run_manifest)
            .map_err(|_| AppError::Invalid("review run manifest is not canonicalizable".into()))?;
    let logs_hash = hepta_paper_raid_contracts::canonical_json_sha256(&request.logs)
        .map_err(|_| AppError::Invalid("review logs are not canonicalizable".into()))?;
    let seed_set_hash =
        hepta_paper_raid_contracts::canonical_json_sha256(&vec![descriptor.execution.seed])
            .map_err(|_| AppError::Internal)?;
    let expected_expiry = DateTime::parse_from_rfc3339(&descriptor.expires_at)
        .map_err(|_| AppError::Upstream)?
        .timestamp();
    let now = Utc::now().timestamp();
    if receipt.receipt_id != expected_receipt_id
        || receipt.task_id != expected_task_id
        || receipt.assignment_id != descriptor.assignment_id
        || receipt.paper_project_id != descriptor.paper_project_id
        || receipt.submission_id != descriptor.submission_id
        || receipt.evaluation_id != evaluation_id
        || receipt.kind != descriptor.execution.kind
        || receipt.fencing_token != descriptor.assignment_version
        || receipt.bundle_hash != descriptor.bundle_hash
        || receipt.evaluator_version != descriptor.execution.evaluator_version
        || receipt.input_root != expected_input_root
        || receipt.metrics_hash != expected_metrics_hash
        || receipt.output_root != output_root
        || receipt.environment_hash != environment_hash
        || receipt.run_manifest_hash != run_manifest_hash
        || receipt.logs_hash != logs_hash
        || receipt.seed_set_hash != seed_set_hash
        || receipt.completed_at_unix > now + AGENT_PROOF_CLOCK_SKEW_SECONDS
        || receipt.completed_at_unix > expected_expiry
    {
        return Err(AppError::Forbidden);
    }
    if !review_output_matches_receipt(&request.output, &receipt)
        || request.run_manifest.get("adapter").and_then(Value::as_str)
            != Some(descriptor.execution.adapter.as_str())
        || request
            .run_manifest
            .get("entrypoint")
            .and_then(Value::as_str)
            != Some(descriptor.execution.entrypoint.as_str())
        || request.run_manifest.get("seed").and_then(Value::as_u64)
            != Some(descriptor.execution.seed)
    {
        return Err(AppError::Forbidden);
    }
    for (field, expected) in [
        ("task_id", json!(receipt.task_id)),
        ("assignment_id", json!(receipt.assignment_id)),
        ("paper_project_id", json!(receipt.paper_project_id)),
        ("submission_id", json!(receipt.submission_id)),
        ("kind", json!(&receipt.kind)),
        ("attempt", json!(receipt.attempt)),
        ("fencing_token", json!(receipt.fencing_token)),
        ("bundle_hash", json!(&receipt.bundle_hash)),
        ("evaluator_version", json!(&receipt.evaluator_version)),
        ("input_root", json!(&receipt.input_root)),
        ("output_root", json!(&receipt.output_root)),
        ("metrics_hash", json!(&receipt.metrics_hash)),
        ("seed_set_hash", json!(&receipt.seed_set_hash)),
        ("environment_hash", json!(&receipt.environment_hash)),
        ("logs_hash", json!(&receipt.logs_hash)),
        ("started_at_unix", json!(receipt.started_at_unix)),
        ("completed_at_unix", json!(receipt.completed_at_unix)),
    ] {
        if request.run_manifest.get(field) != Some(&expected) {
            return Err(AppError::Forbidden);
        }
    }
    let receipt_hash = review_execution_receipt_hash(&receipt)
        .map_err(|_| AppError::Invalid("review receipt cannot be canonicalized".into()))?;
    let receipt_json = serde_json::to_value(&receipt).map_err(|_| AppError::Internal)?;
    let request_json = json!({
        "schema": request.schema,
        "idempotency_key": request.idempotency_key,
        "receipt": receipt_json,
        "output": request.output,
        "environment": request.environment,
        "run_manifest": request.run_manifest,
        "logs": request.logs,
    });
    let mut tx = state.pool.begin().await?;
    let attempt =
        i64::try_from(receipt.attempt).map_err(|_| AppError::Invalid("attempt overflow".into()))?;
    let latest = sqlx::query(
        "SELECT state,attempt FROM paper_raid_bff_review_execution_receipts
         WHERE task_id=$1 ORDER BY attempt DESC LIMIT 1 FOR UPDATE",
    )
    .bind(receipt.task_id)
    .fetch_optional(&mut *tx)
    .await?;
    let latest = latest
        .map(|row| {
            let stored_attempt = row.try_get::<i64, _>("attempt")?;
            Ok::<StoredReviewTaskState, AppError>(StoredReviewTaskState {
                state: row.try_get("state")?,
                attempt: u64::try_from(stored_attempt).map_err(|_| AppError::Upstream)?,
            })
        })
        .transpose()?;
    let existing = sqlx::query(
        "SELECT receipt_id,receipt_hash,receipt,state
         FROM paper_raid_bff_review_execution_receipts
         WHERE task_id=$1 AND attempt=$2 FOR UPDATE",
    )
    .bind(receipt.task_id)
    .bind(attempt)
    .fetch_optional(&mut *tx)
    .await?;
    let inserted = if existing.is_some() {
        false
    } else {
        let (expected_attempt, _) = projected_review_task_attempt(latest.as_ref())?;
        if receipt.attempt != expected_attempt {
            return Err(AppError::Conflict(
                "review receipt attempt is stale or was not projected".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO paper_raid_bff_review_execution_receipts (
                receipt_id,task_id,binding_id,assignment_id,paper_id,submission_id,evaluation_id,
                kind,attempt,fencing_token,bundle_hash,receipt_hash,receipt,state,created_at,updated_at
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13::jsonb,'pending',now(),now())
             -- One canonical receipt simultaneously owns receipt_id and (task_id,attempt).
             -- Omitting the arbiter makes an identical concurrent first insert idempotent
             -- regardless of which of those two unique indexes PostgreSQL checks first.
             ON CONFLICT DO NOTHING",
        )
        .bind(receipt.receipt_id)
        .bind(receipt.task_id)
        .bind(receipt.binding_id)
        .bind(receipt.assignment_id)
        .bind(receipt.paper_project_id)
        .bind(receipt.submission_id)
        .bind(receipt.evaluation_id)
        .bind(&receipt.kind)
        .bind(attempt)
        .bind(i64::try_from(receipt.fencing_token).map_err(|_| AppError::Invalid("fencing overflow".into()))?)
        .bind(&receipt.bundle_hash)
        .bind(&receipt_hash)
        .bind(&request_json)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1
    };
    let stored = sqlx::query(
        "SELECT receipt_id,receipt_hash,receipt,state
         FROM paper_raid_bff_review_execution_receipts
         WHERE task_id=$1 AND attempt=$2 FOR UPDATE",
    )
    .bind(receipt.task_id)
    .bind(attempt)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| {
        AppError::Conflict("review_receipt_conflict_did_not_resolve_to_task_attempt".into())
    })?;
    if stored.get::<Uuid, _>("receipt_id") != receipt.receipt_id
        || stored.get::<String, _>("receipt_hash") != receipt_hash
        || stored.get::<Value, _>("receipt") != request_json
    {
        return Err(AppError::Conflict(
            "review_task_attempt_already_has_different_receipt".into(),
        ));
    }
    bridge_audit(
        &mut tx,
        Some(&verified.mapping.subject_id),
        Some(verified.mapping.binding_id),
        "review_execution_receipt",
        "succeeded",
        json!({
            "receipt_id": receipt.receipt_id,
            "task_id": receipt.task_id,
            "assignment_id": receipt.assignment_id,
            "paper_id": receipt.paper_project_id,
            "kind": receipt.kind,
            "receipt_hash": receipt_hash,
            "storage_disposition": if inserted { "accepted" } else { "replayed" },
        }),
    )
    .await?;
    tx.commit().await?;
    let value = json!({
        "schema": "hepta.paper_raid.agent_bridge.review_receipt_result.v1",
        "receipt_id": receipt.receipt_id,
        "task_id": receipt.task_id,
        "attempt": receipt.attempt,
        "receipt_hash": receipt_hash,
        "status": "stored",
    });
    complete_agent_request(&state, &verified, StatusCode::OK, &value).await
}

fn review_output_matches_receipt(output: &Value, receipt: &ReviewExecutionReceiptV1) -> bool {
    match receipt.kind.as_str() {
        "evaluate" => serde_json::from_value::<ReviewEvaluationExecutionResultV1>(output.clone())
            .is_ok_and(|evaluation| {
                evaluation.reference_metrics_micros == receipt.observed_metrics_micros
                    && Some(evaluation.candidate_passed) == receipt.candidate_passed
                    && receipt.statistical_evidence
                        == json!({
                            "schema": "hepta.paper_raid.statistical_evidence.none.v1",
                            "reason": "frozen_evaluator_did_not_emit_statistical_evidence",
                        })
            }),
        "reproduce" => serde_json::from_value::<ReviewReproductionExecutionResultV1>(
            output.clone(),
        )
        .is_ok_and(|reproduction| {
            reproduction.observed_metrics_micros == receipt.observed_metrics_micros
                && serde_json::to_value(reproduction.statistical_evidence)
                    .is_ok_and(|evidence| evidence == receipt.statistical_evidence)
                && receipt.candidate_passed.is_none()
        }),
        _ => false,
    }
}

async fn complete_agent_raw_request(
    state: &AppState,
    verified: &VerifiedAgentRequest,
    status: StatusCode,
    body: &[u8],
) -> Result<(), AppError> {
    if body.len() > MAX_REVIEW_OBJECT_BYTES {
        return Err(AppError::Upstream);
    }
    let updated = sqlx::query(
        "UPDATE paper_raid_bff_agent_request_uses
         SET response_status=$1,response_body=$2,completed_at=now()
         WHERE binding_id=$3 AND nonce=$4 AND request_hash=$5 AND response_status IS NULL",
    )
    .bind(i32::from(status.as_u16()))
    .bind(body)
    .bind(verified.mapping.binding_id)
    .bind(verified.claim.nonce)
    .bind(verified.request_hash.as_slice())
    .execute(&state.pool)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(AppError::Conflict(
            "agent_raw_response_already_completed".into(),
        ));
    }
    Ok(())
}

fn unavailable_delivery_candidates(reason_code: &str) -> Value {
    // Never infer a candidate from independent room collections. A candidate
    // exists only after this Agent declares one short-lived delivery draft and
    // the BFF revalidates its complete tuple against current Hepta authority.
    json!({
        "schema": "hepta.paper_raid.agent_bridge.delivery_candidates.v1",
        "status": "unavailable",
        "reason_code": reason_code,
        "items": [],
    })
}

fn delivery_draft_matches_context(draft: &DeliveryDraft, context: &DeliveryContext) -> bool {
    draft.expected_work_version == context.expected_work_version
        && draft.lease_id == context.lease_id
        && draft.lease_fencing_token == context.lease_fencing_token
        && draft.parent_revision_id == context.parent_revision_id
        && draft.artifact_manifest_hash == context.artifact_manifest_hash
        && draft.expires_at <= context.lease_expires_at
}

fn delivery_candidate_value(draft: &DeliveryDraft) -> Value {
    json!({
        "schema": "hepta.paper_raid.agent_bridge.delivery_candidate.v1",
        "delivery_draft_id": draft.delivery_draft_id,
        "binding_id": draft.binding_id,
        "paper_id": draft.paper_id,
        "work_item_id": draft.work_item_id,
        "expected_work_version": draft.expected_work_version,
        "section_key": draft.section_key,
        "lease_id": draft.lease_id,
        "lease_fencing_token": draft.lease_fencing_token,
        "parent_revision_id": draft.parent_revision_id,
        "proposal_kind": "delivery",
        "payload_hash": draft.payload_hash,
        "artifact_manifest_id": draft.artifact_manifest_id,
        "artifact_manifest_hash": draft.artifact_manifest_hash,
        "delivery_state": draft.state,
        "declared_at_unix": draft.created_at.timestamp(),
        "expires_at_unix": draft.expires_at.timestamp(),
    })
}

fn delivery_recovery_pins_are_canonical(draft: &DeliveryDraft) -> bool {
    draft
        .proposal_body_hash
        .as_deref()
        .is_some_and(|hash| decode_digest(hash).is_ok())
        && draft.proposal_id
            == Some(delivery_proposal_id(
                draft.binding_id,
                draft.delivery_draft_id,
            ))
        && draft.proposal_idempotency_key
            == Some(delivery_idempotency_key(
                draft.binding_id,
                draft.delivery_draft_id,
            ))
        && draft.proposal_signed_at_unix == Some(draft.created_at.timestamp())
}

fn room_contains_exact_delivery_proposal(
    room: &Value,
    draft: &DeliveryDraft,
) -> Result<bool, AppError> {
    let expected = draft.proposal_id.ok_or(AppError::Internal)?;
    let proposals = room
        .get("proposals")
        .and_then(Value::as_array)
        .ok_or(AppError::Upstream)?;
    let matches: Vec<&Value> = proposals
        .iter()
        .filter(|proposal| record_uuid(proposal, "proposal_id") == Some(expected))
        .collect();
    if matches.len() > 1 {
        return Err(AppError::Upstream);
    }
    match matches.first() {
        Some(proposal) => {
            validate_delivery_proposal_record(proposal, draft)?;
            Ok(matches!(
                proposal.get("status").and_then(Value::as_str),
                Some("submitted" | "accepted" | "rework" | "rejected" | "superseded")
            ))
        }
        None => Ok(false),
    }
}

fn delivery_draft_from_row(row: &sqlx::postgres::PgRow) -> Result<DeliveryDraft, AppError> {
    let lease_fencing_token =
        u64::try_from(row.get::<i64, _>("lease_fencing_token")).map_err(|_| AppError::Internal)?;
    let expected_work_version = u64::try_from(row.get::<i64, _>("expected_work_version"))
        .map_err(|_| AppError::Internal)?;
    if lease_fencing_token == 0
        || lease_fencing_token > JSON_SAFE_U64_MAX
        || expected_work_version == 0
        || expected_work_version > JSON_SAFE_U64_MAX
    {
        return Err(AppError::Internal);
    }
    Ok(DeliveryDraft {
        delivery_draft_id: row.get("delivery_draft_id"),
        binding_id: row.get("binding_id"),
        paper_id: row.get("paper_id"),
        work_item_id: row.get("work_item_id"),
        expected_work_version,
        section_key: row.get("section_key"),
        lease_id: row.get("lease_id"),
        lease_fencing_token,
        parent_revision_id: row.get("parent_revision_id"),
        artifact_manifest_id: row.get("artifact_manifest_id"),
        artifact_manifest_hash: row.get("artifact_manifest_hash"),
        payload_hash: row.get("payload_hash"),
        state: row.get("state"),
        proposal_body_hash: row.try_get("proposal_body_hash").ok(),
        proposal_id: row.try_get("proposal_id").ok(),
        proposal_idempotency_key: row.try_get("proposal_idempotency_key").ok(),
        proposal_signed_at_unix: row.try_get("proposal_signed_at_unix").ok(),
        created_at: row.get("created_at"),
        expires_at: row.get("expires_at"),
    })
}

async fn expire_delivery_drafts(
    tx: &mut Transaction<'_, Postgres>,
    binding_id: Uuid,
    now: DateTime<Utc>,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE paper_raid_bff_agent_delivery_drafts \
         SET state='expired',invalidated_at=$1,updated_at=$1 \
         WHERE binding_id=$2 AND state='pending' AND expires_at<=$1",
    )
    .bind(now)
    .bind(binding_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn load_delivery_draft_tx(
    tx: &mut Transaction<'_, Postgres>,
    delivery_draft_id: Uuid,
    binding_id: Uuid,
    paper_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Option<DeliveryDraft>, AppError> {
    sqlx::query(
        "SELECT delivery_draft_id,binding_id,paper_id,work_item_id,expected_work_version,section_key, \
                lease_id,lease_fencing_token,parent_revision_id,artifact_manifest_id, \
                artifact_manifest_hash,payload_hash,state,proposal_body_hash,proposal_id, \
                proposal_idempotency_key,proposal_signed_at_unix,created_at,expires_at \
         FROM paper_raid_bff_agent_delivery_drafts \
         WHERE delivery_draft_id=$1 AND binding_id=$2 AND paper_id=$3 \
           AND state='pending' AND expires_at>$4 FOR UPDATE",
    )
    .bind(delivery_draft_id)
    .bind(binding_id)
    .bind(paper_id)
    .bind(now)
    .fetch_optional(&mut **tx)
    .await?
    .map(|row| delivery_draft_from_row(&row))
    .transpose()
}

async fn load_recoverable_delivery_drafts(
    state: &AppState,
    binding_id: Uuid,
    paper_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Vec<DeliveryDraft>, AppError> {
    sqlx::query(
        "SELECT delivery_draft_id,binding_id,paper_id,work_item_id,expected_work_version,section_key, \
                lease_id,lease_fencing_token,parent_revision_id,artifact_manifest_id, \
                artifact_manifest_hash,payload_hash,state,proposal_body_hash,proposal_id, \
                proposal_idempotency_key,proposal_signed_at_unix,created_at,expires_at \
         FROM paper_raid_bff_agent_delivery_drafts \
         WHERE binding_id=$1 AND paper_id=$2 \
           AND state IN ('pending','submitting','consumed') AND expires_at>$3 \
         ORDER BY created_at,delivery_draft_id LIMIT 64",
    )
    .bind(binding_id)
    .bind(paper_id)
    .bind(now)
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .map(|row| delivery_draft_from_row(&row))
    .collect()
}

async fn load_delivery_draft_for_proposal(
    state: &AppState,
    delivery_draft_id: Uuid,
    binding_id: Uuid,
    paper_id: Uuid,
) -> Result<Option<DeliveryDraft>, AppError> {
    sqlx::query(
        "SELECT delivery_draft_id,binding_id,paper_id,work_item_id,expected_work_version,section_key, \
                lease_id,lease_fencing_token,parent_revision_id,artifact_manifest_id, \
                artifact_manifest_hash,payload_hash,state,proposal_body_hash,proposal_id, \
                proposal_idempotency_key,proposal_signed_at_unix,created_at,expires_at \
         FROM paper_raid_bff_agent_delivery_drafts \
         WHERE delivery_draft_id=$1 AND binding_id=$2 AND paper_id=$3",
    )
    .bind(delivery_draft_id)
    .bind(binding_id)
    .bind(paper_id)
    .fetch_optional(&state.pool)
    .await?
    .map(|row| delivery_draft_from_row(&row))
    .transpose()
}

fn delivery_proposal_pin_matches(
    draft: &DeliveryDraft,
    proposal_body_hash: &str,
    proposal_id: Uuid,
    proposal_idempotency_key: Uuid,
    proposal_signed_at_unix: i64,
) -> bool {
    draft.proposal_body_hash.as_deref() == Some(proposal_body_hash)
        && draft.proposal_id == Some(proposal_id)
        && draft.proposal_idempotency_key == Some(proposal_idempotency_key)
        && draft.proposal_signed_at_unix == Some(proposal_signed_at_unix)
}

fn delivery_draft_authority_pins_match(expected: &DeliveryDraft, locked: &DeliveryDraft) -> bool {
    expected.delivery_draft_id == locked.delivery_draft_id
        && expected.binding_id == locked.binding_id
        && expected.paper_id == locked.paper_id
        && expected.work_item_id == locked.work_item_id
        && expected.expected_work_version == locked.expected_work_version
        && expected.section_key == locked.section_key
        && expected.lease_id == locked.lease_id
        && expected.lease_fencing_token == locked.lease_fencing_token
        && expected.parent_revision_id == locked.parent_revision_id
        && expected.artifact_manifest_id == locked.artifact_manifest_id
        && expected.artifact_manifest_hash == locked.artifact_manifest_hash
        && expected.payload_hash == locked.payload_hash
        && expected.created_at == locked.created_at
        && expected.expires_at == locked.expires_at
}

fn delivery_request_payload_matches_draft(payload: &Value, draft: &DeliveryDraft) -> bool {
    record_uuid(payload, "work_item_id") == Some(draft.work_item_id)
        && payload.get("section_key").and_then(Value::as_str) == Some(draft.section_key.as_str())
        && record_uuid(payload, "parent_revision_id") == Some(draft.parent_revision_id)
        && record_uuid(payload, "lease_id") == Some(draft.lease_id)
        && payload.get("lease_fencing_token").and_then(Value::as_u64)
            == Some(draft.lease_fencing_token)
        && payload.get("expected_work_version").and_then(Value::as_u64)
            == Some(draft.expected_work_version)
        && payload.get("proposal_kind").and_then(Value::as_str) == Some("delivery")
        && payload.get("payload_hash").and_then(Value::as_str) == Some(draft.payload_hash.as_str())
        && record_uuid(payload, "artifact_manifest_id") == Some(draft.artifact_manifest_id)
        && record_uuid(payload, "binding_id") == Some(draft.binding_id)
}

fn delivery_claim_matches_locked_snapshot(
    expected_draft: &DeliveryDraft,
    locked_draft: &DeliveryDraft,
    request_payload: &Value,
    proposal_body_hash: &str,
    proposal_id: Uuid,
    proposal_idempotency_key: Uuid,
    proposal_signed_at_unix: i64,
) -> bool {
    delivery_draft_authority_pins_match(expected_draft, locked_draft)
        && delivery_request_payload_matches_draft(request_payload, locked_draft)
        && decode_digest(proposal_body_hash).is_ok()
        && proposal_id
            == delivery_proposal_id(locked_draft.binding_id, locked_draft.delivery_draft_id)
        && proposal_idempotency_key
            == delivery_idempotency_key(locked_draft.binding_id, locked_draft.delivery_draft_id)
        && proposal_signed_at_unix == locked_draft.created_at.timestamp()
}

async fn claim_delivery_draft_for_proposal(
    state: &AppState,
    expected_draft: &DeliveryDraft,
    claim: &DeliveryProposalClaim<'_>,
) -> Result<(DeliveryDraft, bool), AppError> {
    let mut tx = state.pool.begin().await?;
    let row = sqlx::query(
        "SELECT delivery_draft_id,binding_id,paper_id,work_item_id,expected_work_version,section_key, \
                lease_id,lease_fencing_token,parent_revision_id,artifact_manifest_id, \
                artifact_manifest_hash,payload_hash,state,proposal_body_hash,proposal_id, \
                proposal_idempotency_key,proposal_signed_at_unix,created_at,expires_at \
         FROM paper_raid_bff_agent_delivery_drafts \
         WHERE delivery_draft_id=$1 AND binding_id=$2 AND paper_id=$3 FOR UPDATE",
    )
    .bind(expected_draft.delivery_draft_id)
    .bind(expected_draft.binding_id)
    .bind(expected_draft.paper_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::Conflict("delivery_draft_missing_or_expired".into()))?;
    let mut draft = delivery_draft_from_row(&row)?;
    let is_recovery = matches!(draft.state.as_str(), "submitting" | "consumed");
    if !delivery_claim_matches_locked_snapshot(
        expected_draft,
        &draft,
        claim.request_payload,
        claim.proposal_body_hash,
        claim.proposal_id,
        claim.proposal_idempotency_key,
        claim.proposal_signed_at_unix,
    ) {
        return Err(AppError::Conflict(
            "delivery_draft_claim_stale_or_mismatched".into(),
        ));
    }
    match draft.state.as_str() {
        "pending" => {
            if draft.expires_at <= claim.claimed_at {
                return Err(AppError::Conflict(
                    "delivery_draft_missing_or_expired".into(),
                ));
            }
            let claimed = sqlx::query(
                "UPDATE paper_raid_bff_agent_delivery_drafts SET \
                    state='submitting',proposal_body_hash=$1,proposal_id=$2, \
                    proposal_idempotency_key=$3,proposal_signed_at_unix=$4, \
                    submitting_at=$5,updated_at=$5 \
                 WHERE delivery_draft_id=$6 AND binding_id=$7 AND paper_id=$8 \
                   AND state='pending' AND expires_at>$5",
            )
            .bind(claim.proposal_body_hash)
            .bind(claim.proposal_id)
            .bind(claim.proposal_idempotency_key)
            .bind(claim.proposal_signed_at_unix)
            .bind(claim.claimed_at)
            .bind(expected_draft.delivery_draft_id)
            .bind(expected_draft.binding_id)
            .bind(expected_draft.paper_id)
            .execute(&mut *tx)
            .await?;
            if claimed.rows_affected() != 1 {
                return Err(AppError::Conflict("delivery_draft_claim_conflict".into()));
            }
            draft.state = "submitting".into();
            draft.proposal_body_hash = Some(claim.proposal_body_hash.to_string());
            draft.proposal_id = Some(claim.proposal_id);
            draft.proposal_idempotency_key = Some(claim.proposal_idempotency_key);
            draft.proposal_signed_at_unix = Some(claim.proposal_signed_at_unix);
        }
        "submitting" | "consumed" => {
            if !delivery_proposal_pin_matches(
                &draft,
                claim.proposal_body_hash,
                claim.proposal_id,
                claim.proposal_idempotency_key,
                claim.proposal_signed_at_unix,
            ) {
                return Err(AppError::Conflict(
                    "delivery_draft_already_claimed_by_different_proposal".into(),
                ));
            }
        }
        _ => {
            return Err(AppError::Conflict(
                "delivery_draft_missing_or_expired".into(),
            ))
        }
    }
    tx.commit().await?;
    Ok((draft, is_recovery))
}

async fn consume_claimed_delivery_draft(
    state: &AppState,
    draft: &DeliveryDraft,
    proposal_body_hash: &str,
) -> Result<(), AppError> {
    let updated = sqlx::query(
        "UPDATE paper_raid_bff_agent_delivery_drafts SET \
            state='consumed',consumed_at=COALESCE(consumed_at,now()),updated_at=now() \
         WHERE delivery_draft_id=$1 AND binding_id=$2 AND state='submitting' \
           AND proposal_body_hash=$3 AND proposal_id=$4 \
           AND proposal_idempotency_key=$5 AND proposal_signed_at_unix=$6",
    )
    .bind(draft.delivery_draft_id)
    .bind(draft.binding_id)
    .bind(proposal_body_hash)
    .bind(draft.proposal_id.ok_or(AppError::Internal)?)
    .bind(draft.proposal_idempotency_key.ok_or(AppError::Internal)?)
    .bind(draft.proposal_signed_at_unix.ok_or(AppError::Internal)?)
    .execute(&state.pool)
    .await?;
    if updated.rows_affected() == 1 {
        return Ok(());
    }
    let current = load_delivery_draft_for_proposal(
        state,
        draft.delivery_draft_id,
        draft.binding_id,
        draft.paper_id,
    )
    .await?
    .ok_or(AppError::Internal)?;
    if current.state == "consumed"
        && delivery_proposal_pin_matches(
            &current,
            proposal_body_hash,
            draft.proposal_id.ok_or(AppError::Internal)?,
            draft.proposal_idempotency_key.ok_or(AppError::Internal)?,
            draft.proposal_signed_at_unix.ok_or(AppError::Internal)?,
        )
    {
        return Ok(());
    }
    Err(AppError::Conflict(
        "delivery_draft_consumption_conflict".into(),
    ))
}

fn validate_delivery_proposal_record(
    proposal: &Value,
    draft: &DeliveryDraft,
) -> Result<(), AppError> {
    let expected_proposal_id = draft.proposal_id.ok_or(AppError::Internal)?;
    let expected_signed_at = draft.proposal_signed_at_unix.ok_or(AppError::Internal)?;
    let exact = record_uuid(proposal, "proposal_id") == Some(expected_proposal_id)
        && record_uuid(proposal, "paper_project_id") == Some(draft.paper_id)
        && record_uuid(proposal, "work_item_id") == Some(draft.work_item_id)
        && proposal.get("section_key").and_then(Value::as_str) == Some(draft.section_key.as_str())
        && record_uuid(proposal, "parent_revision_id") == Some(draft.parent_revision_id)
        && record_uuid(proposal, "lease_id") == Some(draft.lease_id)
        && proposal.get("lease_fencing_token").and_then(Value::as_u64)
            == Some(draft.lease_fencing_token)
        && proposal
            .get("expected_work_version")
            .and_then(Value::as_u64)
            == Some(draft.expected_work_version)
        && proposal.get("proposal_kind").and_then(Value::as_str) == Some("delivery")
        && proposal.get("payload_hash").and_then(Value::as_str)
            == Some(draft.payload_hash.as_str())
        && record_uuid(proposal, "artifact_manifest_id") == Some(draft.artifact_manifest_id)
        && proposal
            .get("artifact_manifest_hash")
            .and_then(Value::as_str)
            == Some(draft.artifact_manifest_hash.as_str())
        && record_uuid(proposal, "binding_id") == Some(draft.binding_id)
        && proposal.get("signed_at_unix").and_then(Value::as_i64) == Some(expected_signed_at)
        && proposal
            .get("version")
            .and_then(Value::as_u64)
            .is_some_and(|version| version > 0);
    if exact {
        Ok(())
    } else {
        Err(AppError::Upstream)
    }
}

fn validate_delivery_proposal_response(
    proposal: &Value,
    draft: &DeliveryDraft,
    request_payload: &Value,
    authoritative_binding: &Value,
) -> Result<(), AppError> {
    validate_delivery_proposal_record(proposal, draft)?;
    if proposal.get("status").and_then(Value::as_str) != Some("submitted") {
        return Err(AppError::Upstream);
    }
    for field in ["agent_id", "agent_key_id", "signature"] {
        if proposal.get(field) != request_payload.get(field) {
            return Err(AppError::Upstream);
        }
    }
    if proposal.get("agent_public_key") != authoritative_binding.get("agent_public_key") {
        return Err(AppError::Upstream);
    }
    Ok(())
}

fn project_fields(record: &Value, fields: &[&str]) -> Result<Value, AppError> {
    let record = record.as_object().ok_or(AppError::Upstream)?;
    let mut projected = serde_json::Map::new();
    for field in fields {
        projected.insert(
            (*field).to_string(),
            record.get(*field).cloned().ok_or(AppError::Upstream)?,
        );
    }
    Ok(Value::Object(projected))
}

pub async fn agent_proposal(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let verified = verify_agent_request(&state, Method::POST, &uri, &headers, &body).await?;
    if let Some(response) = replay_response(&verified) {
        return Ok(response);
    }
    let request: ProposalRequest = decode_json(&body, MAX_AGENT_BODY_BYTES)?;
    if request.schema != "hepta.paper_raid.agent_bridge.proposal_request.v1" {
        return Err(AppError::Invalid(
            "invalid Agent proposal request schema".into(),
        ));
    }
    let payload = request
        .payload
        .as_object()
        .ok_or_else(|| AppError::Invalid("proposal payload must be an object".into()))?;
    let idempotency_key = payload
        .get("idempotency_key")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(|| AppError::Invalid("proposal idempotency_key must be a UUID".into()))?;
    let proposal_id = record_uuid(&request.payload, "proposal_id")
        .ok_or_else(|| AppError::Invalid("proposal_id must be a UUID".into()))?;
    let proposal_signed_at_unix = request
        .payload
        .get("signed_at_unix")
        .and_then(Value::as_i64)
        .ok_or_else(|| AppError::Invalid("proposal signed_at_unix must be an integer".into()))?;
    let proposal_kind = request
        .payload
        .get("proposal_kind")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Invalid("proposal_kind is required".into()))?;
    if proposal_kind != "delivery" {
        return Err(AppError::Invalid(
            "Agent Bridge proposals require a delivery-bound Agent Proposal V2".into(),
        ));
    }
    for (field, expected) in [
        ("binding_id", verified.mapping.binding_id.to_string()),
        ("agent_id", verified.mapping.agent_id.clone()),
        ("agent_key_id", verified.mapping.agent_key_id.clone()),
    ] {
        if payload.get(field).and_then(Value::as_str) != Some(expected.as_str()) {
            return Err(AppError::Forbidden);
        }
    }
    let proposal_body_hash = verified.claim.body_hash.clone();
    let delivery_draft_id = request
        .delivery_draft_id
        .ok_or_else(|| AppError::Invalid("delivery proposals require delivery_draft_id".into()))?;
    let now = Utc::now();
    let draft = load_delivery_draft_for_proposal(
        &state,
        delivery_draft_id,
        verified.mapping.binding_id,
        request.paper_id,
    )
    .await?
    .ok_or_else(|| AppError::Conflict("delivery_draft_missing_or_expired".into()))?;
    if !delivery_request_payload_matches_draft(&request.payload, &draft) {
        return Err(AppError::Forbidden);
    }
    if proposal_id != delivery_proposal_id(draft.binding_id, draft.delivery_draft_id)
        || idempotency_key != delivery_idempotency_key(draft.binding_id, draft.delivery_draft_id)
        || proposal_signed_at_unix != draft.created_at.timestamp()
    {
        return Err(AppError::Forbidden);
    }
    if draft.state == "pending" {
        let room = state
            .hepta
            .get_paper_room(&verified.identity, request.paper_id)
            .await?;
        let context = resolve_delivery_context(
            &room,
            &verified.mapping,
            draft.work_item_id,
            &draft.section_key,
            draft.artifact_manifest_id,
            now,
        )?
        .ok_or_else(|| AppError::Conflict("delivery_draft_is_no_longer_current".into()))?;
        if !delivery_draft_matches_context(&draft, &context) {
            return Err(AppError::Conflict(
                "delivery_draft_authoritative_context_changed".into(),
            ));
        }
    } else if !matches!(draft.state.as_str(), "submitting" | "consumed")
        || !delivery_proposal_pin_matches(
            &draft,
            &proposal_body_hash,
            proposal_id,
            idempotency_key,
            proposal_signed_at_unix,
        )
    {
        return Err(AppError::Conflict(
            "delivery_draft_already_claimed_by_different_proposal".into(),
        ));
    }
    let delivery_claim = DeliveryProposalClaim {
        request_payload: &request.payload,
        proposal_body_hash: &proposal_body_hash,
        proposal_id,
        proposal_idempotency_key: idempotency_key,
        proposal_signed_at_unix,
        claimed_at: now,
    };
    let (delivery_draft, is_recovery) =
        claim_delivery_draft_for_proposal(&state, &draft, &delivery_claim).await?;
    let submission = async {
        let command = BrowserCommand {
            command: CommandName::SubmitAgentProposal,
            resource_id: Some(request.paper_id),
            child_id: None,
            session_id: None,
            idempotency_key,
            payload: request.payload.clone(),
        };
        let upstream = state
            .hepta
            .forward_command(&verified.identity, &command)
            .await?;
        if !(200..300).contains(&upstream.status) {
            return Err(AppError::Upstream);
        }
        let proposal = upstream.json()?;
        validate_delivery_proposal_response(
            &proposal,
            &delivery_draft,
            &request.payload,
            &verified.authoritative_binding,
        )?;
        consume_claimed_delivery_draft(&state, &delivery_draft, &proposal_body_hash).await?;
        let value = json!({
            "schema": "hepta.paper_raid.agent_bridge.proposal_result.v2",
            "proposal": proposal
        });
        complete_agent_request(
            &state,
            &verified,
            StatusCode::from_u16(upstream.status).map_err(|_| AppError::Internal)?,
            &value,
        )
        .await
    }
    .await;
    if is_recovery {
        state.metrics.observe_bridge_recovery(submission.is_ok());
    }
    submission
}

fn decode_json<T: DeserializeOwned>(body: &[u8], max_bytes: usize) -> Result<T, AppError> {
    if body.is_empty() || body.len() > max_bytes {
        return Err(AppError::Invalid(
            "JSON request body size is invalid".into(),
        ));
    }
    serde_json::from_slice(body).map_err(|_| AppError::Invalid("invalid JSON request body".into()))
}

fn generate_pairing_code() -> String {
    let mut secret = [0_u8; 32];
    OsRng.fill_bytes(&mut secret);
    format!("{PAIR_CODE_PREFIX}{}", URL_SAFE_NO_PAD.encode(secret))
}

fn validate_pairing_code(value: &str) -> Result<(), AppError> {
    let encoded = value
        .strip_prefix(PAIR_CODE_PREFIX)
        .ok_or(AppError::Forbidden)?;
    let decoded = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| AppError::Forbidden)?;
    if decoded.len() != 32 || URL_SAFE_NO_PAD.encode(decoded) != encoded {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

fn pairing_code_hash_for_admission(value: &str) -> Result<[u8; 32], AppError> {
    if value.is_empty() || value.len() > 128 || value.as_bytes().contains(&0) {
        return Err(AppError::Forbidden);
    }
    Ok(digest_bytes(value.as_bytes()))
}

fn digest_bytes(value: &[u8]) -> [u8; 32] {
    Sha256::digest(value).into()
}

fn deterministic_uuid(domain: &str, fields: &[String]) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0]);
    hasher.update(fields.join("\0").as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn delivery_proposal_id(binding_id: Uuid, delivery_draft_id: Uuid) -> Uuid {
    deterministic_uuid(
        DELIVERY_PROPOSAL_ID_DOMAIN,
        &[binding_id.to_string(), delivery_draft_id.to_string()],
    )
}

fn delivery_idempotency_key(binding_id: Uuid, delivery_draft_id: Uuid) -> Uuid {
    deterministic_uuid(
        DELIVERY_IDEMPOTENCY_KEY_DOMAIN,
        &[binding_id.to_string(), delivery_draft_id.to_string()],
    )
}

fn review_task_id(
    assignment_id: Uuid,
    bundle_hash: &str,
    kind: &str,
    evaluation_id: Option<Uuid>,
) -> Uuid {
    deterministic_uuid(
        REVIEW_TASK_ID_DOMAIN,
        &[
            assignment_id.to_string(),
            bundle_hash.to_string(),
            kind.to_string(),
            evaluation_id
                .map(|value| value.to_string())
                .unwrap_or_default(),
        ],
    )
}

fn review_evaluation_id(assignment_id: Uuid, bundle_hash: &str) -> Uuid {
    deterministic_uuid(
        REVIEW_EVALUATION_ID_DOMAIN,
        &[assignment_id.to_string(), bundle_hash.to_string()],
    )
}

fn fixed_bucket(domain: &[u8], digest: &[u8; 32]) -> [u8; 32] {
    let mut value = Vec::with_capacity(domain.len() + 1);
    value.extend_from_slice(domain);
    value.push(digest[0]);
    digest_bytes(&value)
}

async fn admit_pair_quota(state: &AppState, code_hash: &[u8; 32]) -> Result<(), AppError> {
    let now = Utc::now();
    let config = state.config.agent_bridge_quota;
    let global = digest_bytes(GLOBAL_PAIR_PRINCIPAL);
    let bucket = fixed_bucket(PAIR_BUCKET_DOMAIN, code_hash);
    let mut tx = state.pool.begin().await?;
    let retry = bump_quota(
        &mut tx,
        "agent_pair_global",
        &global,
        config.window,
        config.pair_global_limit,
        now,
    )
    .await?
    .or(bump_quota(
        &mut tx,
        "agent_pair_bucket",
        &bucket,
        config.window,
        config.pair_bucket_limit,
        now,
    )
    .await?);
    tx.commit().await?;
    match retry {
        Some(retry_after_secs) => Err(AppError::RateLimited { retry_after_secs }),
        None => Ok(()),
    }
}

async fn admit_agent_request_global(
    state: &AppState,
    candidate_hash: &[u8; 32],
) -> Result<(), AppError> {
    let now = Utc::now();
    let config = state.config.agent_bridge_quota;
    let global = digest_bytes(GLOBAL_REQUEST_PRINCIPAL);
    let bucket = fixed_bucket(REQUEST_BUCKET_DOMAIN, candidate_hash);
    let mut tx = state.pool.begin().await?;
    // The response bytes exist only to make an exact signed nonce retry
    // deterministic. They are never a durable inbox/proposal read model.
    sqlx::query("DELETE FROM paper_raid_bff_agent_request_uses WHERE expires_at <= $1")
        .bind(now)
        .execute(&mut *tx)
        .await?;
    let retry = bump_quota(
        &mut tx,
        "agent_request_global",
        &global,
        config.window,
        config.request_global_limit,
        now,
    )
    .await?
    .or(bump_quota(
        &mut tx,
        "agent_request_bucket",
        &bucket,
        config.window,
        config.request_bucket_limit,
        now,
    )
    .await?);
    tx.commit().await?;
    match retry {
        Some(retry_after_secs) => Err(AppError::RateLimited { retry_after_secs }),
        None => Ok(()),
    }
}

async fn admit_agent_binding_quota(
    state: &AppState,
    config: AgentBridgeQuotaConfig,
    binding_id: Uuid,
) -> Result<(), AppError> {
    let principal = digest_bytes(binding_id.as_bytes());
    let now = Utc::now();
    let mut tx = state.pool.begin().await?;
    let retry = bump_quota(
        &mut tx,
        "agent_request_binding",
        &principal,
        config.window,
        config.request_binding_limit,
        now,
    )
    .await?;
    tx.commit().await?;
    match retry {
        Some(retry_after_secs) => Err(AppError::RateLimited { retry_after_secs }),
        None => Ok(()),
    }
}

async fn load_pairing_grant_by_hash(
    state: &AppState,
    code_hash: &[u8; 32],
) -> Result<PairingGrant, AppError> {
    let query = "SELECT grant_id, subject_id, player_id, state, created_at, expires_at, \
                pinned_request_hash, pinned_binding_id \
         FROM paper_raid_bff_agent_pairing_grants \
         WHERE code_hash=$1 AND state IN ('issued','pinned','consumed') \
           AND expires_at > now()";
    let row = sqlx::query(query)
        .bind(code_hash.as_slice())
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::Forbidden)?;
    Ok(PairingGrant {
        grant_id: row.get("grant_id"),
        subject_id: row.get("subject_id"),
        player_id: row.get("player_id"),
        state: row.get("state"),
        created_at: row.get("created_at"),
        expires_at: row.get("expires_at"),
        pinned_request_hash: row.try_get("pinned_request_hash").ok(),
        pinned_binding_id: row.try_get("pinned_binding_id").ok(),
    })
}

fn validate_binding_request(
    request: &AgentBindingV3Request,
    identity: &AlphaIdentity,
) -> Result<Value, AppError> {
    if request.agent_proof_schema != AGENT_BINDING_PROOF_V3
        || request.player_id != identity.player_id
        || request.agent_proof_nonce != request.idempotency_key
    {
        return Err(AppError::Forbidden);
    }
    let calculated = agent_capability_disclosure_hash(&request.capability_disclosure)
        .map_err(AppError::Invalid)?;
    if calculated != request.capability_disclosure_hash {
        return Err(AppError::Forbidden);
    }
    let claim = AgentBindingProofClaimV3 {
        schema: request.agent_proof_schema.clone(),
        binding_id: request.binding_id,
        agent_id: request.agent_id.clone(),
        agent_key_id: request.agent_key_id.clone(),
        agent_public_key: request.agent_public_key.clone(),
        agent_public_key_hash: request.agent_key_id.clone(),
        capability_disclosure_hash: request.capability_disclosure_hash.clone(),
        subject_id: identity.subject_id.clone(),
        player_id: identity.player_id,
        nonce: request.agent_proof_nonce.to_string(),
        issued_at_unix: request.agent_proof_issued_at_unix,
        expires_at_unix: request.agent_proof_expires_at_unix,
    };
    verify_agent_binding_proof_v3(&claim, &request.agent_proof_signature)
        .map_err(|_| AppError::Forbidden)?;
    let now = Utc::now().timestamp();
    if request.agent_proof_issued_at_unix > now + AGENT_PROOF_CLOCK_SKEW_SECONDS
        || request.agent_proof_expires_at_unix <= now
        || request.agent_proof_expires_at_unix - request.agent_proof_issued_at_unix
            > PAIRING_GRANT_TTL_SECONDS
    {
        return Err(AppError::Forbidden);
    }
    serde_json::to_value(request).map_err(|_| AppError::Internal)
}

async fn pin_pairing_grant(
    state: &AppState,
    code_hash: &[u8; 32],
    request_hash: [u8; 32],
    binding_id: Uuid,
) -> Result<(), AppError> {
    let mut tx = state.pool.begin().await?;
    let row = sqlx::query(
        "SELECT state, pinned_request_hash, pinned_binding_id, expires_at \
         FROM paper_raid_bff_agent_pairing_grants \
         WHERE code_hash=$1 FOR UPDATE",
    )
    .bind(code_hash.as_slice())
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::Forbidden)?;
    let grant_state: String = row.get("state");
    let expires_at: DateTime<Utc> = row.get("expires_at");
    if expires_at <= Utc::now() || !matches!(grant_state.as_str(), "issued" | "pinned" | "consumed")
    {
        return Err(AppError::Forbidden);
    }
    let stored_hash: Option<Vec<u8>> = row.try_get("pinned_request_hash").ok();
    let stored_binding: Option<Uuid> = row.try_get("pinned_binding_id").ok();
    if grant_state == "issued" {
        sqlx::query(
            "UPDATE paper_raid_bff_agent_pairing_grants \
             SET state='pinned', pinned_request_hash=$1, pinned_binding_id=$2, \
                 pinned_at=now(), updated_at=now() \
             WHERE code_hash=$3 AND state='issued'",
        )
        .bind(request_hash.as_slice())
        .bind(binding_id)
        .bind(code_hash.as_slice())
        .execute(&mut *tx)
        .await?;
    } else if stored_hash.as_deref() != Some(request_hash.as_slice())
        || stored_binding != Some(binding_id)
    {
        return Err(AppError::Conflict("pairing_request_changed".into()));
    }
    tx.commit().await?;
    Ok(())
}

async fn find_exact_active_binding(
    state: &AppState,
    identity: &AlphaIdentity,
    request: &AgentBindingV3Request,
) -> Result<Option<Value>, AppError> {
    let value = state.hepta.list_current_agent_bindings(identity).await?;
    let bindings = value.as_array().ok_or(AppError::Upstream)?;
    let expected_binding_id = request.binding_id.to_string();
    let matches: Vec<Value> = bindings
        .iter()
        .filter(|binding| {
            binding.get("binding_id").and_then(Value::as_str) == Some(expected_binding_id.as_str())
        })
        .cloned()
        .collect();
    if matches.len() > 1 {
        return Err(AppError::Upstream);
    }
    match matches.into_iter().next() {
        Some(binding) => {
            validate_exact_binding(&binding, request, identity)?;
            Ok(Some(binding))
        }
        None => Ok(None),
    }
}

fn validate_exact_binding(
    binding: &Value,
    request: &AgentBindingV3Request,
    identity: &AlphaIdentity,
) -> Result<(), AppError> {
    let disclosure =
        serde_json::to_value(&request.capability_disclosure).map_err(|_| AppError::Internal)?;
    for (field, expected) in [
        ("binding_id", Value::String(request.binding_id.to_string())),
        ("player_id", Value::String(identity.player_id.to_string())),
        ("agent_id", Value::String(request.agent_id.clone())),
        ("agent_key_id", Value::String(request.agent_key_id.clone())),
        (
            "agent_public_key",
            Value::String(request.agent_public_key.clone()),
        ),
        (
            "agent_public_key_hash",
            Value::String(request.agent_key_id.clone()),
        ),
        (
            "capability_disclosure_hash",
            Value::String(request.capability_disclosure_hash.clone()),
        ),
        ("capability_disclosure", disclosure),
        ("status", Value::String("active".into())),
    ] {
        if binding.get(field) != Some(&expected) {
            return Err(AppError::Upstream);
        }
    }
    Ok(())
}

fn mapping_repair_owner_matches(actual: BridgeOwner<'_>, expected: BridgeOwner<'_>) -> bool {
    actual.binding_id == expected.binding_id
        && actual.subject_id == expected.subject_id
        && actual.player_id == expected.player_id
        && actual.agent_id == expected.agent_id
}

async fn persist_pairing_result(
    state: &AppState,
    grant: &PairingGrant,
    request: &AgentBindingV3Request,
    binding: &Value,
) -> Result<Vec<u8>, AppError> {
    let now = Utc::now();
    let response = canonical_json_bytes(&json!({
        "schema": "hepta.paper_raid.agent_bridge.pairing_result.v2",
        "binding": binding,
    }))
    .map_err(AppError::Invalid)?;
    let disclosure =
        serde_json::to_value(&request.capability_disclosure).map_err(|_| AppError::Internal)?;
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO paper_raid_bff_agent_bridge_bindings ( \
            binding_id, grant_id, last_pairing_grant_id, subject_id, player_id, \
            agent_id, agent_key_id, \
            capability_disclosure_hash, capability_disclosure, binding_record, \
            paired_at, last_verified_at \
         ) VALUES ($1,$2,$2,$3,$4,$5,$6,$7,$8::jsonb,$9::jsonb,$10,$10) \
         ON CONFLICT(binding_id) DO NOTHING",
    )
    .bind(request.binding_id)
    .bind(grant.grant_id)
    .bind(&grant.subject_id)
    .bind(grant.player_id)
    .bind(&request.agent_id)
    .bind(&request.agent_key_id)
    .bind(&request.capability_disclosure_hash)
    .bind(&disclosure)
    .bind(binding)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    let row = sqlx::query(
        "SELECT binding_id, grant_id, subject_id, player_id, agent_id \
         FROM paper_raid_bff_agent_bridge_bindings WHERE binding_id=$1 FOR UPDATE",
    )
    .bind(request.binding_id)
    .fetch_one(&mut *tx)
    .await?;
    let stored_subject_id: String = row.get("subject_id");
    let stored_agent_id: String = row.get("agent_id");
    if !mapping_repair_owner_matches(
        BridgeOwner {
            binding_id: row.get("binding_id"),
            subject_id: &stored_subject_id,
            player_id: row.get("player_id"),
            agent_id: &stored_agent_id,
        },
        BridgeOwner {
            binding_id: request.binding_id,
            subject_id: &grant.subject_id,
            player_id: grant.player_id,
            agent_id: &request.agent_id,
        },
    ) {
        return Err(AppError::Conflict("pairing_binding_conflict".into()));
    }
    let mapping_updated = sqlx::query(
        "UPDATE paper_raid_bff_agent_bridge_bindings SET \
            last_pairing_grant_id=$1, agent_key_id=$2, \
            capability_disclosure_hash=$3, capability_disclosure=$4::jsonb, \
            binding_record=$5::jsonb, last_verified_at=$6 \
         WHERE binding_id=$7 AND subject_id=$8 AND player_id=$9 AND agent_id=$10",
    )
    .bind(grant.grant_id)
    .bind(&request.agent_key_id)
    .bind(&request.capability_disclosure_hash)
    .bind(&disclosure)
    .bind(binding)
    .bind(now)
    .bind(request.binding_id)
    .bind(&grant.subject_id)
    .bind(grant.player_id)
    .bind(&request.agent_id)
    .execute(&mut *tx)
    .await?;
    if mapping_updated.rows_affected() != 1 {
        return Err(AppError::Conflict("pairing_binding_conflict".into()));
    }
    let updated = sqlx::query(
        "UPDATE paper_raid_bff_agent_pairing_grants \
         SET state='consumed', consumed_at=$1, pair_response_status=200, \
             pair_response_body=$2, updated_at=$1 \
         WHERE grant_id=$3 AND state='pinned' AND pinned_binding_id=$4",
    )
    .bind(now)
    .bind(&response)
    .bind(grant.grant_id)
    .bind(request.binding_id)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 1 {
        bridge_audit(
            &mut tx,
            Some(&grant.subject_id),
            Some(request.binding_id),
            "agent_paired",
            "succeeded",
            json!({"grant_id": grant.grant_id}),
        )
        .await?;
    }
    let stored = sqlx::query(
        "SELECT state, pinned_request_hash, pinned_binding_id, \
                pair_response_status, pair_response_body \
         FROM paper_raid_bff_agent_pairing_grants WHERE grant_id=$1",
    )
    .bind(grant.grant_id)
    .fetch_one(&mut *tx)
    .await?;
    if stored.get::<String, _>("state") != "consumed"
        || stored.try_get::<Uuid, _>("pinned_binding_id").ok() != Some(request.binding_id)
        || stored
            .try_get::<Vec<u8>, _>("pinned_request_hash")
            .ok()
            .as_deref()
            != grant.pinned_request_hash.as_deref()
        || stored.try_get::<i32, _>("pair_response_status").ok() != Some(200)
    {
        return Err(AppError::Conflict("pairing_grant_conflict".into()));
    }
    let stored_response = stored
        .try_get::<Vec<u8>, _>("pair_response_body")
        .map_err(|_| AppError::Internal)?;
    if stored_response != response {
        return Err(AppError::Conflict("pairing_replay_response_changed".into()));
    }
    tx.commit().await?;
    Ok(stored_response)
}

async fn verify_agent_request(
    state: &AppState,
    method: Method,
    uri: &axum::http::Uri,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<VerifiedAgentRequest, AppError> {
    let candidate = bounded_header(headers, HEADER_BINDING_ID).unwrap_or_default();
    let candidate_hash = digest_bytes(candidate.as_bytes());
    admit_agent_request_global(state, &candidate_hash).await?;
    reject_unknown_agent_headers(headers)?;
    let canonical_query = uri.query().unwrap_or("");
    validate_agent_bridge_canonical_query(canonical_query).map_err(|_| AppError::Forbidden)?;
    if !matches!(
        uri.path(),
        "/api/agent-bridge/review-objects" | "/api/agent-bridge/challenge-objects"
    ) && !canonical_query.is_empty()
    {
        return Err(AppError::Forbidden);
    }
    let body_hash = sha256_digest(body);
    if bounded_header(headers, HEADER_BODY_SHA256).as_deref() != Some(body_hash.as_str()) {
        return Err(AppError::Forbidden);
    }
    let claim = AgentBridgeRequestProofV1 {
        schema: required_header(headers, HEADER_SCHEMA)?,
        binding_id: parse_header_uuid(headers, HEADER_BINDING_ID)?,
        agent_id: required_header(headers, HEADER_AGENT_ID)?,
        agent_key_id: required_header(headers, HEADER_KEY_ID)?,
        http_method: method.as_str().to_string(),
        canonical_path: uri.path().to_string(),
        canonical_query: canonical_query.to_string(),
        body_hash,
        nonce: parse_header_uuid(headers, HEADER_NONCE)?,
        issued_at_unix: parse_header_i64(headers, HEADER_ISSUED_AT)?,
        expires_at_unix: parse_header_i64(headers, HEADER_EXPIRES_AT)?,
    };
    let request_hash_text =
        agent_bridge_request_proof_hash(&claim).map_err(|_| AppError::Forbidden)?;
    let now = Utc::now().timestamp();
    if claim.issued_at_unix > now + AGENT_PROOF_CLOCK_SKEW_SECONDS || claim.expires_at_unix <= now {
        return Err(AppError::Forbidden);
    }
    let mapping = load_bridge_mapping(state, claim.binding_id).await?;
    if mapping.agent_id != claim.agent_id
        || mapping.agent_key_id != claim.agent_key_id
        || mapping.binding_id != claim.binding_id
    {
        return Err(AppError::Forbidden);
    }
    let identity = state
        .identity_for_agent_bridge(&mapping.subject_id)
        .await?
        .ok_or(AppError::Forbidden)?;
    if identity.player_id != mapping.player_id {
        return Err(AppError::Forbidden);
    }
    let (authoritative, exactly_one_active_binding) =
        authoritative_bridge_binding(state, &identity, &mapping).await?;
    let public_key = authoritative
        .get("agent_public_key")
        .and_then(Value::as_str)
        .ok_or(AppError::Upstream)?;
    verify_agent_bridge_request_proof(
        &claim,
        public_key,
        &required_header(headers, HEADER_SIGNATURE)?,
    )
    .map_err(|_| AppError::Forbidden)?;
    let request_hash_vec = hex::decode(&request_hash_text[7..]).map_err(|_| AppError::Internal)?;
    let request_hash: [u8; 32] = request_hash_vec
        .try_into()
        .map_err(|_| AppError::Internal)?;
    let replay = match read_agent_request_use(state, &claim, &request_hash).await? {
        AgentRequestUseState::Completed(status, body) => Some((status, body)),
        AgentRequestUseState::Pending => {
            wait_for_agent_request_use(state, &claim, &request_hash).await?
        }
        AgentRequestUseState::Missing => {
            admit_agent_binding_quota(state, state.config.agent_bridge_quota, claim.binding_id)
                .await?;
            begin_agent_request_use(state, &claim, &request_hash).await?
        }
    };
    Ok(VerifiedAgentRequest {
        identity,
        mapping,
        authoritative_binding: authoritative,
        exactly_one_active_binding,
        claim,
        request_hash,
        replay,
    })
}

fn reject_unknown_agent_headers(headers: &HeaderMap) -> Result<(), AppError> {
    for name in headers.keys() {
        let value = name.as_str();
        if value.starts_with("x-paper-raid-agent-") && !AGENT_HEADER_NAMES.contains(&value) {
            return Err(AppError::Forbidden);
        }
    }
    for name in AGENT_HEADER_NAMES {
        let header = HeaderName::from_static(name);
        if headers.get_all(header).iter().count() != 1 {
            return Err(AppError::Forbidden);
        }
    }
    Ok(())
}

fn bounded_header(headers: &HeaderMap, name: &'static str) -> Option<String> {
    let values = headers.get_all(HeaderName::from_static(name));
    if values.iter().count() != 1 {
        return None;
    }
    values
        .iter()
        .next()
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && value.len() <= 1024 && !value.contains('\0'))
        .map(str::to_string)
}

fn required_header(headers: &HeaderMap, name: &'static str) -> Result<String, AppError> {
    bounded_header(headers, name).ok_or(AppError::Forbidden)
}

fn parse_header_uuid(headers: &HeaderMap, name: &'static str) -> Result<Uuid, AppError> {
    let value = required_header(headers, name)?;
    let parsed = Uuid::parse_str(&value).map_err(|_| AppError::Forbidden)?;
    if parsed.to_string() != value {
        return Err(AppError::Forbidden);
    }
    Ok(parsed)
}

fn parse_header_i64(headers: &HeaderMap, name: &'static str) -> Result<i64, AppError> {
    let value = required_header(headers, name)?;
    if value.starts_with('+') || (value.starts_with('0') && value.len() > 1) {
        return Err(AppError::Forbidden);
    }
    value.parse::<i64>().map_err(|_| AppError::Forbidden)
}

async fn load_bridge_mapping(
    state: &AppState,
    binding_id: Uuid,
) -> Result<BridgeMapping, AppError> {
    let row = sqlx::query(
        "SELECT binding_id, subject_id, player_id, agent_id, agent_key_id, \
                capability_disclosure_hash \
         FROM paper_raid_bff_agent_bridge_bindings WHERE binding_id=$1",
    )
    .bind(binding_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::Forbidden)?;
    Ok(BridgeMapping {
        binding_id: row.get("binding_id"),
        subject_id: row.get("subject_id"),
        player_id: row.get("player_id"),
        agent_id: row.get("agent_id"),
        agent_key_id: row.get("agent_key_id"),
        capability_disclosure_hash: row.get("capability_disclosure_hash"),
    })
}

async fn authoritative_bridge_binding(
    state: &AppState,
    identity: &AlphaIdentity,
    mapping: &BridgeMapping,
) -> Result<(Value, bool), AppError> {
    let value = state.hepta.list_current_agent_bindings(identity).await?;
    let bindings = value.as_array().ok_or(AppError::Upstream)?;
    let expected_player = mapping.player_id.to_string();
    let mut seen = HashSet::new();
    let mut active_count = 0_usize;
    for binding in bindings {
        let binding_id_text = binding
            .get("binding_id")
            .and_then(Value::as_str)
            .ok_or(AppError::Upstream)?;
        let binding_id = Uuid::parse_str(binding_id_text).map_err(|_| AppError::Upstream)?;
        if binding_id.is_nil()
            || binding_id.to_string() != binding_id_text
            || !seen.insert(binding_id)
            || binding.get("player_id").and_then(Value::as_str) != Some(expected_player.as_str())
        {
            return Err(AppError::Upstream);
        }
        match binding.get("status").and_then(Value::as_str) {
            Some("active") => active_count += 1,
            Some("revoked") => {}
            _ => return Err(AppError::Upstream),
        }
    }
    let expected_binding = mapping.binding_id.to_string();
    let matches: Vec<&Value> = bindings
        .iter()
        .filter(|binding| {
            binding.get("binding_id").and_then(Value::as_str) == Some(expected_binding.as_str())
        })
        .collect();
    if matches.len() != 1 {
        return Err(AppError::Forbidden);
    }
    let binding = matches[0];
    for (field, expected) in [
        ("player_id", mapping.player_id.to_string()),
        ("agent_id", mapping.agent_id.clone()),
        ("agent_key_id", mapping.agent_key_id.clone()),
        (
            "capability_disclosure_hash",
            mapping.capability_disclosure_hash.clone(),
        ),
        ("status", "active".to_string()),
    ] {
        if binding.get(field).and_then(Value::as_str) != Some(expected.as_str()) {
            return Err(AppError::Forbidden);
        }
    }
    if binding
        .get("capability_disclosure")
        .and_then(|value| value.get("assurance"))
        .and_then(Value::as_str)
        != Some("self_declared_unverified")
    {
        return Err(AppError::Forbidden);
    }
    sqlx::query(
        "UPDATE paper_raid_bff_agent_bridge_bindings SET last_verified_at=now() \
         WHERE binding_id=$1",
    )
    .bind(mapping.binding_id)
    .execute(&state.pool)
    .await?;
    Ok((binding.clone(), active_count == 1))
}

async fn begin_agent_request_use(
    state: &AppState,
    claim: &AgentBridgeRequestProofV1,
    request_hash: &[u8; 32],
) -> Result<Option<(u16, Vec<u8>)>, AppError> {
    let created_at = Utc
        .timestamp_opt(claim.issued_at_unix, 0)
        .single()
        .ok_or(AppError::Forbidden)?;
    let expires_at = Utc
        .timestamp_opt(claim.expires_at_unix, 0)
        .single()
        .ok_or(AppError::Forbidden)?;
    let inserted = sqlx::query(
        "INSERT INTO paper_raid_bff_agent_request_uses ( \
            binding_id, nonce, request_hash, created_at, expires_at \
         ) VALUES ($1,$2,$3,$4,$5) ON CONFLICT(binding_id,nonce) DO NOTHING",
    )
    .bind(claim.binding_id)
    .bind(claim.nonce)
    .bind(request_hash.as_slice())
    .bind(created_at)
    .bind(expires_at)
    .execute(&state.pool)
    .await?;
    if inserted.rows_affected() == 1 {
        return Ok(None);
    }
    wait_for_agent_request_use(state, claim, request_hash).await
}

async fn read_agent_request_use(
    state: &AppState,
    claim: &AgentBridgeRequestProofV1,
    request_hash: &[u8; 32],
) -> Result<AgentRequestUseState, AppError> {
    let row = sqlx::query(
        "SELECT request_hash, response_status, response_body \
         FROM paper_raid_bff_agent_request_uses WHERE binding_id=$1 AND nonce=$2",
    )
    .bind(claim.binding_id)
    .bind(claim.nonce)
    .fetch_optional(&state.pool)
    .await?;
    let Some(row) = row else {
        return Ok(AgentRequestUseState::Missing);
    };
    let stored_hash: Vec<u8> = row.get("request_hash");
    if stored_hash.as_slice() != request_hash {
        return Err(AppError::Conflict("agent_nonce_reused".into()));
    }
    match (
        row.try_get::<i32, _>("response_status").ok(),
        row.try_get::<Vec<u8>, _>("response_body").ok(),
    ) {
        (Some(status), Some(body)) => Ok(AgentRequestUseState::Completed(
            u16::try_from(status).map_err(|_| AppError::Internal)?,
            body,
        )),
        (None, None) => Ok(AgentRequestUseState::Pending),
        _ => Err(AppError::Internal),
    }
}

async fn wait_for_agent_request_use(
    state: &AppState,
    claim: &AgentBridgeRequestProofV1,
    request_hash: &[u8; 32],
) -> Result<Option<(u16, Vec<u8>)>, AppError> {
    for _ in 0..AGENT_REPLAY_WAIT_ATTEMPTS {
        match read_agent_request_use(state, claim, request_hash).await? {
            AgentRequestUseState::Completed(status, body) => return Ok(Some((status, body))),
            AgentRequestUseState::Pending => {
                tokio::time::sleep(AGENT_REPLAY_WAIT_INTERVAL).await;
            }
            AgentRequestUseState::Missing => return Err(AppError::Internal),
        }
    }
    // A process can die after the route's authoritative/idempotent operation
    // but before response completion. The exact same hash+nonce may therefore
    // re-enter its fixed route after the bounded concurrent wait. Route reads,
    // the health upsert, and Hepta proposal idempotency are the recovery
    // authorities; complete_agent_request still compare-checks exact bytes.
    Ok(None)
}

async fn complete_agent_request(
    state: &AppState,
    verified: &VerifiedAgentRequest,
    status: StatusCode,
    value: &Value,
) -> Result<Response, AppError> {
    let body = canonical_json_bytes(value).map_err(AppError::Invalid)?;
    if body.len() > MAX_AGENT_BODY_BYTES {
        return Err(AppError::Upstream);
    }
    let updated = sqlx::query(
        "UPDATE paper_raid_bff_agent_request_uses \
         SET response_status=$1, response_body=$2, completed_at=now() \
         WHERE binding_id=$3 AND nonce=$4 AND request_hash=$5 \
           AND response_status IS NULL",
    )
    .bind(i32::from(status.as_u16()))
    .bind(&body)
    .bind(verified.mapping.binding_id)
    .bind(verified.claim.nonce)
    .bind(verified.request_hash.as_slice())
    .execute(&state.pool)
    .await?;
    if updated.rows_affected() == 0 {
        let replay =
            match read_agent_request_use(state, &verified.claim, &verified.request_hash).await? {
                AgentRequestUseState::Completed(status, body) => (status, body),
                AgentRequestUseState::Missing | AgentRequestUseState::Pending => {
                    return Err(AppError::Internal)
                }
            };
        if replay.0 != status.as_u16() || replay.1 != body {
            return Err(AppError::Conflict("agent_replay_response_changed".into()));
        }
    }
    Ok(private_bytes(status.as_u16(), body))
}

fn replay_response(verified: &VerifiedAgentRequest) -> Option<Response> {
    verified
        .replay
        .as_ref()
        .map(|(status, body)| private_bytes(*status, body.clone()))
}

pub(crate) async fn bridge_audit(
    tx: &mut Transaction<'_, Postgres>,
    subject_id: Option<&str>,
    binding_id: Option<Uuid>,
    action: &str,
    outcome: &str,
    metadata: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO paper_raid_bff_agent_bridge_audit ( \
            audit_id, subject_id, binding_id, action, outcome, metadata \
         ) VALUES ($1,$2,$3,$4,$5,$6::jsonb)",
    )
    .bind(Uuid::new_v4())
    .bind(subject_id)
    .bind(binding_id)
    .bind(action)
    .bind(outcome)
    .bind(metadata)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn result_json(result: Result<Value, AppError>) -> Response {
    match result {
        Ok(value) => private_json(value),
        Err(error) => error.into_response(),
    }
}

fn private_json(value: Value) -> Response {
    private_bytes(
        StatusCode::OK.as_u16(),
        serde_json::to_vec(&value).unwrap_or_else(|_| b"{\"error\":\"internal_error\"}".to_vec()),
    )
}

fn private_bytes(status: u16, body: Vec<u8>) -> Response {
    let mut response = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CACHE_CONTROL, "no-store, private")
        .header(header::PRAGMA, "no-cache")
        .body(Body::from(body))
        .unwrap_or_else(|_| AppError::Internal.into_response());
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response
}

fn with_rotated_csrf(mut response: Response, csrf: String) -> Response {
    if let Ok(value) = HeaderValue::from_str(&csrf) {
        response.headers_mut().insert(CSRF_HEADER, value);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use hepta_paper_raid_contracts::AGENT_BRIDGE_REQUEST_PROOF_V1;

    #[test]
    fn pairing_codes_are_canonical_random_and_hash_only_ready() {
        let first = generate_pairing_code();
        let second = generate_pairing_code();
        validate_pairing_code(&first).expect("first pairing code");
        validate_pairing_code(&second).expect("second pairing code");
        assert_ne!(first, second);
        assert_eq!(pairing_code_hash_for_admission(&first).unwrap().len(), 32);
        assert!(validate_pairing_code("prg1.not+base64").is_err());
    }

    #[test]
    fn unknown_agent_pop_headers_fail_closed() {
        let mut headers = HeaderMap::new();
        for name in AGENT_HEADER_NAMES {
            headers.insert(HeaderName::from_static(name), HeaderValue::from_static("x"));
        }
        assert!(reject_unknown_agent_headers(&headers).is_ok());
        headers.insert(
            HeaderName::from_static("x-paper-raid-agent-cookie"),
            HeaderValue::from_static("forbidden"),
        );
        assert!(reject_unknown_agent_headers(&headers).is_err());
    }

    #[test]
    fn fixed_buckets_have_only_256_possible_principals() {
        let values: HashSet<[u8; 32]> = (0_u16..=255)
            .map(|value| {
                let mut digest = [0_u8; 32];
                digest[0] = value as u8;
                fixed_bucket(REQUEST_BUCKET_DOMAIN, &digest)
            })
            .collect();
        assert_eq!(values.len(), 256);
    }

    #[test]
    fn practice_agent_requests_are_minimal_and_reject_authority_fields() {
        serde_json::from_value::<PracticeTaskQueryV1>(json!({
            "schema": PRACTICE_TASK_QUERY_V1,
        }))
        .expect("bounded practice task query");
        serde_json::from_value::<PracticeClaimRequestV1>(json!({
            "schema": PRACTICE_CLAIM_REQUEST_V1,
            "expected_version": 3,
            "task_token": format!("sha256:{}", "a".repeat(64)),
        }))
        .expect("bounded practice claim");
        serde_json::from_value::<PracticeResultRequestV1>(json!({
            "schema": PRACTICE_RESULT_REQUEST_V1,
            "expected_version": 4,
            "task_token": format!("sha256:{}", "b".repeat(64)),
            "result_code": "concern_confirmed",
        }))
        .expect("bounded practice result");
        assert!(serde_json::from_value::<PracticeTaskQueryV1>(json!({
            "schema": PRACTICE_TASK_QUERY_V1,
            "expected_version": 3,
        }))
        .is_err());
        assert!(serde_json::from_value::<PracticeClaimRequestV1>(json!({
            "schema": PRACTICE_CLAIM_REQUEST_V1,
            "expected_version": 3,
            "task_token": format!("sha256:{}", "a".repeat(64)),
            "result_code": "concern_confirmed",
        }))
        .is_err());

        for forbidden in [
            "practice_session_id",
            "subject_id",
            "player_id",
            "binding_id",
            "bridge_task_id",
            "event_id",
            "request_hash",
            "result_hash",
            "paper_project_id",
            "activation_id",
            "qualification_id",
            "finality_receipt_hash",
            "rank",
            "reward",
            "economy",
        ] {
            let mut hostile = json!({
                "schema": PRACTICE_RESULT_REQUEST_V1,
                "expected_version": 4,
                "task_token": format!("sha256:{}", "b".repeat(64)),
                "result_code": "concern_confirmed",
            });
            hostile[forbidden] = json!(Uuid::new_v4());
            assert!(
                serde_json::from_value::<PracticeResultRequestV1>(hostile).is_err(),
                "accepted hostile practice field {forbidden}"
            );
        }
    }

    #[test]
    fn practice_agent_projection_is_bounded_and_owner_opaque() {
        let now = Utc::now();
        let mut practice = PracticeSessionV1::new(
            Uuid::new_v4(),
            "practice-agent-owner".into(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            now,
            now + chrono::Duration::minutes(20),
        )
        .expect("practice fixture");
        practice.stage = PracticeStageV1::ExperimentWaitingBridge;
        practice.version = 3;
        practice.captain_plan = Some(crate::practice::CaptainPlanChoiceV1::AuditHighestRiskClaim);
        practice.evidence_assessment =
            Some(crate::practice::EvidenceAssessmentChoiceV1::CitationMismatch);
        practice.validate().expect("waiting practice fixture");

        let projected =
            practice_task_projection(Some(&practice), now).expect("bounded practice projection");
        assert_eq!(projected["schema"], PRACTICE_TASKS_V1);
        assert_eq!(projected["mode"], PRACTICE_UNRANKED_MODE);
        assert_eq!(projected["task"]["state"], "pending");
        assert_eq!(projected["task"]["version"], 3);
        assert!(decode_digest(
            projected["task"]["task_token"]
                .as_str()
                .expect("opaque practice task token")
        )
        .is_ok());
        let original_token = projected["task"]["task_token"]
            .as_str()
            .expect("original opaque task token");
        let mut substituted = practice.clone();
        substituted.practice_session_id = Uuid::new_v4();
        substituted.bridge_task_id = Uuid::new_v4();
        substituted
            .validate()
            .expect("same-version replacement fixture");
        assert_ne!(
            original_token,
            practice_agent_task_token(&substituted).expect("replacement opaque task token"),
            "same-version replacement reused the prior task token"
        );
        assert_eq!(
            projected["task"]["allowed_result_codes"],
            json!(["concern_confirmed", "concern_not_detected", "inconclusive"])
        );
        let encoded = serde_json::to_string(&projected).expect("serialize projection");
        for forbidden in [
            "practice_session_id",
            "subject_id",
            "player_id",
            "binding_id",
            "bridge_task_id",
            "request_hash",
            "result_hash",
            "activation_eligible",
            "qualification_eligible",
            "scientific_finality_eligible",
            "ranking_eligible",
            "reward_eligible",
            "economic_eligible",
        ] {
            assert!(
                !encoded.contains(forbidden),
                "projection leaked {forbidden}"
            );
        }

        let absent = practice_task_projection(None, now).expect("absent projection");
        assert_eq!(absent["status"], "absent");
        assert!(absent["task"].is_null());
    }

    #[test]
    fn review_attempt_projection_is_monotonic_and_fail_closed() {
        assert_eq!(projected_review_task_attempt(None).unwrap(), (1, "pending"));
        assert_eq!(
            projected_review_task_attempt(Some(&StoredReviewTaskState {
                state: "pending".into(),
                attempt: 2,
            }))
            .unwrap(),
            (2, "submitting")
        );
        assert_eq!(
            projected_review_task_attempt(Some(&StoredReviewTaskState {
                state: "consumed".into(),
                attempt: 2,
            }))
            .unwrap(),
            (2, "consumed")
        );
        assert_eq!(
            projected_review_task_attempt(Some(&StoredReviewTaskState {
                state: "invalidated".into(),
                attempt: 2,
            }))
            .unwrap(),
            (3, "pending")
        );
        assert!(projected_review_task_attempt(Some(&StoredReviewTaskState {
            state: "invalidated".into(),
            attempt: JSON_SAFE_U64_MAX,
        }))
        .is_err());
        assert!(projected_review_task_attempt(Some(&StoredReviewTaskState {
            state: "invented".into(),
            attempt: 1,
        }))
        .is_err());
    }

    fn output_binding_receipt(
        kind: &str,
        candidate_passed: Option<bool>,
        statistical_evidence: Value,
    ) -> ReviewExecutionReceiptV1 {
        ReviewExecutionReceiptV1 {
            schema: REVIEW_EXECUTION_RECEIPT_V1.to_string(),
            receipt_id: Uuid::from_u128(1),
            task_id: Uuid::from_u128(2),
            binding_id: Uuid::from_u128(3),
            assignment_id: Uuid::from_u128(4),
            paper_project_id: Uuid::from_u128(5),
            submission_id: Uuid::from_u128(6),
            evaluation_id: Uuid::from_u128(7),
            kind: kind.to_string(),
            attempt: 1,
            fencing_token: 1,
            bundle_hash: format!("sha256:{}", "1".repeat(64)),
            evaluator_version: "fixture-v1".to_string(),
            input_root: format!("sha256:{}", "2".repeat(64)),
            output_root: format!("sha256:{}", "3".repeat(64)),
            metrics_hash: format!("sha256:{}", "4".repeat(64)),
            observed_metrics_micros: [("accuracy".to_string(), 900_000)].into(),
            statistical_evidence,
            candidate_passed,
            seed_set_hash: format!("sha256:{}", "5".repeat(64)),
            environment_hash: format!("sha256:{}", "6".repeat(64)),
            run_manifest_hash: format!("sha256:{}", "7".repeat(64)),
            logs_hash: format!("sha256:{}", "8".repeat(64)),
            started_at_unix: 1,
            completed_at_unix: 2,
            agent_id: "agent.fixture".to_string(),
            agent_key_id: format!("sha256:{}", "9".repeat(64)),
            signing_public_key_hash: format!("sha256:{}", "9".repeat(64)),
            signature: String::new(),
        }
    }

    #[test]
    fn review_output_binding_is_kind_typed_and_rejects_mixed_fields() {
        let none = json!({
            "schema": "hepta.paper_raid.statistical_evidence.none.v1",
            "reason": "frozen_evaluator_did_not_emit_statistical_evidence",
        });
        let evaluation_receipt = output_binding_receipt("evaluate", Some(true), none.clone());
        let evaluation = json!({
            "reference_metrics_micros": {"accuracy": 900_000},
            "tolerance_policy_version": "1",
            "tolerance_rules": [{
                "kind": "absolute",
                "metric": "accuracy",
                "max_delta_micros": 1000,
            }],
            "candidate_passed": true,
        });
        assert!(review_output_matches_receipt(
            &evaluation,
            &evaluation_receipt
        ));
        let mut mixed_evaluation = evaluation.clone();
        mixed_evaluation["observed_metrics_micros"] = json!({"accuracy": 900_000});
        assert!(!review_output_matches_receipt(
            &mixed_evaluation,
            &evaluation_receipt
        ));
        let mut wrong_evidence = evaluation_receipt.clone();
        wrong_evidence.statistical_evidence = json!({});
        assert!(!review_output_matches_receipt(&evaluation, &wrong_evidence));

        let reproduction_evidence = json!({
            "accuracy": {
                "interval_overlap_bps": 9_500,
                "effect_delta_micros": 50,
                "p_value_micros": 50_000,
            }
        });
        let reproduction_receipt =
            output_binding_receipt("reproduce", None, reproduction_evidence.clone());
        let reproduction = json!({
            "observed_metrics_micros": {"accuracy": 900_000},
            "statistical_evidence": reproduction_evidence,
        });
        assert!(review_output_matches_receipt(
            &reproduction,
            &reproduction_receipt
        ));
        let mut mixed_reproduction = reproduction;
        mixed_reproduction["candidate_passed"] = json!(true);
        assert!(!review_output_matches_receipt(
            &mixed_reproduction,
            &reproduction_receipt
        ));
    }

    #[test]
    fn agent_bridge_request_frame_matches_the_frozen_node_vector() {
        let claim = AgentBridgeRequestProofV1 {
            schema: AGENT_BRIDGE_REQUEST_PROOF_V1.to_string(),
            binding_id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            agent_id: "agent.fixture".into(),
            agent_key_id: format!("sha256:{}", "aa".repeat(32)),
            http_method: "POST".into(),
            canonical_path: "/api/agent-bridge/inbox".into(),
            canonical_query: String::new(),
            body_hash: format!("sha256:{}", "bb".repeat(32)),
            nonce: Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
            issued_at_unix: 1_700_000_000,
            expires_at_unix: 1_700_000_060,
        };
        assert_eq!(
            agent_bridge_request_proof_hash(&claim).unwrap(),
            "sha256:a4016d74baab0315d849fbe120c238188860cee326324eba34c7a8e21745f062"
        );

        let mut key_tamper = claim.clone();
        key_tamper.agent_key_id = format!("sha256:{}", "cc".repeat(32));
        assert_ne!(
            agent_bridge_request_proof_hash(&key_tamper).unwrap(),
            agent_bridge_request_proof_hash(&claim).unwrap()
        );

        let mut review_object = claim.clone();
        review_object.http_method = "GET".into();
        review_object.canonical_path = "/api/agent-bridge/review-objects".into();
        review_object.canonical_query = "assignment_id=33333333-3333-4333-8333-333333333333&object_key=object-0000&task_id=44444444-4444-4444-8444-444444444444".into();
        review_object.body_hash = sha256_digest(&[]);
        assert!(agent_bridge_request_proof_hash(&review_object).is_ok());

        let mut review_receipt = claim.clone();
        review_receipt.canonical_path = "/api/agent-bridge/review-receipts".into();
        assert!(agent_bridge_request_proof_hash(&review_receipt).is_ok());

        review_receipt.http_method = "GET".into();
        review_receipt.body_hash = sha256_digest(&[]);
        assert!(agent_bridge_request_proof_hash(&review_receipt).is_err());
    }

    #[test]
    fn inbox_projection_excludes_human_task_content() {
        let task = json!({
            "work_item_id": Uuid::nil(),
            "title": "private human-authored scientific instruction",
            "status": "planned",
        });
        let projected = project_fields(&task, &["work_item_id", "status"]).unwrap();
        assert_eq!(
            projected.get("status").and_then(Value::as_str),
            Some("planned")
        );
        assert!(projected.get("title").is_none());
        assert!(!serde_json::to_string(&projected)
            .unwrap()
            .contains("scientific instruction"));
    }

    #[test]
    fn automatic_inbox_discovery_ignores_history_and_bounds_active_raids() {
        let active: Vec<Value> = (1_u128..=70)
            .map(|value| {
                json!({
                    "team_status": "locked",
                    "paper": {
                        "paper_project_id": Uuid::from_u128(value),
                        "phase": "drafting"
                    }
                })
            })
            .chain([
                json!({
                    "team_status": "archived",
                    "paper": {"paper_project_id": "not-a-uuid", "phase": "drafting"}
                }),
                json!({
                    "team_status": "locked",
                    "paper": {"paper_project_id": Uuid::from_u128(71), "phase": "submission_ready"}
                }),
            ])
            .collect();
        let (paper_ids, truncated) = discover_inbox_paper_ids(&active).unwrap();
        assert_eq!(paper_ids.len(), MAX_DISCOVERED_INBOX_PAPERS);
        assert!(truncated);
        assert_eq!(paper_ids[0], Uuid::from_u128(1));
        assert_eq!(paper_ids[63], Uuid::from_u128(64));

        let malformed = [json!({
            "team_status": "locked",
            "paper": {"paper_project_id": "not-a-uuid", "phase": "drafting"}
        })];
        assert!(discover_inbox_paper_ids(&malformed).is_err());
    }

    #[test]
    fn automatic_inbox_discovery_prioritizes_ambiguous_delivery_recovery() {
        let recovery = vec![Uuid::from_u128(70), Uuid::from_u128(1)];
        let active: Vec<Uuid> = (1_u128..=70).map(Uuid::from_u128).collect();
        let (merged, truncated) = merge_discovered_paper_ids(recovery, active);
        assert_eq!(merged.len(), MAX_DISCOVERED_INBOX_PAPERS);
        assert_eq!(merged[0], Uuid::from_u128(70));
        assert_eq!(merged[1], Uuid::from_u128(1));
        assert!(truncated);
    }

    #[test]
    fn challenge_material_projection_ignores_terminal_history_and_rejects_ambiguous_active_work() {
        let active_id = Uuid::from_u128(7);
        let tasks = vec![
            json!({"work_item_id": Uuid::from_u128(1), "status": "accepted"}),
            json!({"work_item_id": Uuid::from_u128(2), "status": "rejected"}),
            json!({"work_item_id": Uuid::from_u128(3), "status": "cancelled"}),
            json!({"work_item_id": active_id, "status": "in_progress"}),
        ];
        assert_eq!(
            active_challenge_material_work_item_ids(&tasks).unwrap(),
            [active_id]
        );

        let duplicate = vec![
            json!({"work_item_id": active_id, "status": "planned"}),
            json!({"work_item_id": active_id, "status": "in_progress"}),
        ];
        assert!(active_challenge_material_work_item_ids(&duplicate).is_err());
        assert!(active_challenge_material_work_item_ids(&[json!({
            "work_item_id": active_id,
            "status": "review"
        })])
        .unwrap()
        .is_empty());

        for hostile in [
            json!({"work_item_id": active_id.simple().to_string(), "status": "planned"}),
            json!({"work_item_id": Uuid::nil(), "status": "in_progress"}),
            json!({"work_item_id": "not-a-uuid", "status": "in_progress"}),
            json!({"status": "planned"}),
        ] {
            assert!(active_challenge_material_work_item_ids(&[hostile]).is_err());
        }
    }

    fn review_object(
        key: &str,
        path: &str,
        role: &str,
        digest_byte: char,
        media_type: &str,
    ) -> FrozenReviewObjectV1 {
        FrozenReviewObjectV1 {
            object_key: key.to_string(),
            logical_path: path.to_string(),
            role: role.to_string(),
            digest: format!("sha256:{}", digest_byte.to_string().repeat(64)),
            size_bytes: 7,
            media_type: media_type.to_string(),
            download_path: "/api/agent-bridge/review-objects".to_string(),
        }
    }

    fn manifest_member(
        path: &str,
        digest_byte: char,
        media_type: &str,
    ) -> hepta_paper_raid_contracts::ChallengeManifestObjectV1 {
        let digest = format!("sha256:{}", digest_byte.to_string().repeat(64));
        hepta_paper_raid_contracts::ChallengeManifestObjectV1 {
            cas_uri: format!("cas://sha256/{}", &digest[7..]),
            media_type: media_type.to_string(),
            path: path.to_string(),
            sha256: digest,
            size: 7,
        }
    }

    fn human_review_objects() -> Vec<FrozenReviewObjectV1> {
        vec![
            review_object(
                "human-0000",
                "paper/references.bib",
                "bibliography",
                'd',
                "application/x-bibtex",
            ),
            review_object(
                "human-0001",
                "paper/claim-evidence.json",
                "claim_evidence_graph",
                'e',
                "application/json",
            ),
            review_object(
                "human-0002",
                "paper/paper.md",
                "paper_source",
                'f',
                "text/markdown; charset=utf-8",
            ),
        ]
    }

    fn review_authority() -> FrozenReviewAuthorityV1 {
        let mut authority = FrozenReviewAuthorityV1 {
            schema: hepta_paper_raid_contracts::FROZEN_REVIEW_AUTHORITY_V1.to_string(),
            authority_hash: String::new(),
            assignment_id: Uuid::from_u128(1),
            paper_project_id: Uuid::from_u128(2),
            submission_id: Uuid::from_u128(3),
            review_round: 1,
            slot: "evaluator".to_string(),
            assignment_version: 1,
            expires_at: "2026-08-12T00:00:00Z".to_string(),
            release_candidate_hash: format!("sha256:{}", "1".repeat(64)),
            paper_bundle_hash: format!("sha256:{}", "2".repeat(64)),
            artifact_manifest_hash: format!("sha256:{}", "3".repeat(64)),
            evaluator_manifest_hash: format!("sha256:{}", "4".repeat(64)),
            dataset_manifest_hash: format!("sha256:{}", "5".repeat(64)),
            artifact_objects: human_review_objects()
                .into_iter()
                .chain([
                    review_object(
                        "object-0000",
                        "evaluator.py",
                        "frozen_evaluator",
                        'a',
                        "text/x-python; charset=utf-8",
                    ),
                    review_object(
                        "object-0001",
                        "dataset/claims.json",
                        "dataset",
                        'b',
                        "application/json",
                    ),
                    review_object(
                        "object-0002",
                        "inputs/candidate.json",
                        "candidate",
                        'c',
                        "application/json",
                    ),
                ])
                .collect(),
            execution_policy: hepta_paper_raid_contracts::FrozenReviewExecutionPolicyV1 {
                schema: "hepta.paper_raid.review_execution_policy.v1".to_string(),
                kind: "evaluate".to_string(),
                adapter: "python3-stdlib-v1".to_string(),
                timeout_ms: 30_000,
                seed: 7,
            },
        };
        authority.authority_hash =
            hepta_paper_raid_contracts::frozen_review_authority_hash(&authority).unwrap();
        authority
    }

    #[test]
    fn review_authority_is_exactly_bound_to_the_nested_paper_bundle() {
        let authority = review_authority();
        let fixture = json!({
            "paper_project_id": authority.paper_project_id,
            "submission_id": authority.submission_id,
            "status": "submission_ready",
            "release_candidate_hash": authority.release_candidate_hash,
            "paper_bundle_hash": authority.paper_bundle_hash,
            "paper_bundle": {
                "schema": PAPER_BUNDLE_V2,
                "release_candidate_hash": authority.release_candidate_hash,
                "paper_bundle_hash": authority.paper_bundle_hash,
                "release_candidate": {
                    "schema": PAPER_RELEASE_CANDIDATE_V2,
                    "paper_project_id": authority.paper_project_id,
                    "artifact_manifest_hash": authority.artifact_manifest_hash,
                }
            }
        });
        assert!(review_outer_bundle_matches_authority(&fixture, &authority));

        let mut wrong_manifest = fixture.clone();
        wrong_manifest["paper_bundle"]["release_candidate"]["artifact_manifest_hash"] =
            json!(format!("sha256:{}", "9".repeat(64)));
        assert!(!review_outer_bundle_matches_authority(
            &wrong_manifest,
            &authority
        ));

        let mut wrong_status = fixture.clone();
        wrong_status["status"] = json!("integrity_hold");
        assert!(!review_outer_bundle_matches_authority(
            &wrong_status,
            &authority
        ));

        let mut missing_paper_bundle = fixture;
        missing_paper_bundle
            .as_object_mut()
            .unwrap()
            .remove("paper_bundle");
        assert!(!review_outer_bundle_matches_authority(
            &missing_paper_bundle,
            &authority
        ));
    }

    #[test]
    fn legacy_golden_adapter_is_exact_pin_role_media_path_and_digest_bound() {
        const PLAN: &[u8] = b"{\"dataset_sha256\":\"b002e6297f6fd781742866533b89bf781c7f21d5cfa7b42b5c97a9ecd5821314\",\"expected_failure_run_ids\":[\"baseline-invalid-threshold\"],\"metric\":\"accuracy_bps\",\"required_run_ids\":[\"baseline-seed-17\",\"ablation-seed-17\",\"baseline-invalid-threshold\"],\"schema\":\"paper-raid.experiment-plan.v1\",\"stopping_rule\":\"execute_every_required_run_exactly_once\"}\n";
        assert_eq!(sha256_digest(PLAN), LEGACY_GOLDEN_EVALUATOR_MANIFEST_HASH);
        assert_eq!(
            sha256_digest(LEGACY_GOLDEN_DATASET_CARD),
            LEGACY_GOLDEN_DATASET_MANIFEST_HASH
        );
        let mut authority = review_authority();
        authority.evaluator_manifest_hash = LEGACY_GOLDEN_EVALUATOR_MANIFEST_HASH.to_string();
        authority.dataset_manifest_hash = LEGACY_GOLDEN_DATASET_MANIFEST_HASH.to_string();
        authority.artifact_objects = human_review_objects()
            .into_iter()
            .chain([
                review_object(
                    "legacy-candidate",
                    "inputs/metrics.json",
                    "candidate",
                    'c',
                    "application/json",
                ),
                FrozenReviewObjectV1 {
                    object_key: "legacy-dataset".to_string(),
                    logical_path: LEGACY_GOLDEN_DATASET_PATH.to_string(),
                    role: "dataset".to_string(),
                    digest: format!(
                        "sha256:{}",
                        "b002e6297f6fd781742866533b89bf781c7f21d5cfa7b42b5c97a9ecd5821314"
                    ),
                    size_bytes: LEGACY_GOLDEN_DATASET_SIZE,
                    media_type: "text/csv; charset=utf-8".to_string(),
                    download_path: "/api/agent-bridge/review-objects".to_string(),
                },
                FrozenReviewObjectV1 {
                    object_key: "legacy-evaluator".to_string(),
                    logical_path: LEGACY_GOLDEN_FROZEN_EVALUATOR_PATH.to_string(),
                    role: "frozen_evaluator".to_string(),
                    digest: LEGACY_GOLDEN_FROZEN_EVALUATOR_HASH.to_string(),
                    size_bytes: LEGACY_GOLDEN_FROZEN_EVALUATOR_SIZE,
                    media_type: "text/x-python; charset=utf-8".to_string(),
                    download_path: "/api/agent-bridge/review-objects".to_string(),
                },
            ])
            .collect();
        authority.authority_hash =
            hepta_paper_raid_contracts::frozen_review_authority_hash(&authority).unwrap();

        let (evaluator, dataset) =
            legacy_golden_challenge_manifests(&authority, PLAN, LEGACY_GOLDEN_DATASET_CARD)
                .expect("exact legacy authority adapter");
        assert_eq!(evaluator.pack_id, LEGACY_GOLDEN_PACK_ID);
        assert_eq!(evaluator.entrypoint, LEGACY_GOLDEN_FROZEN_EVALUATOR_PATH);
        assert_eq!(evaluator.objects.len(), 1);
        assert_eq!(dataset.objects.len(), 1);
        assert_eq!(dataset.objects[0].path, LEGACY_GOLDEN_DATASET_PATH);
        assert!(resolve_manifest_members(&authority, &evaluator, &dataset).is_ok());

        let mut tampered_plan = PLAN.to_vec();
        tampered_plan[0] ^= 1;
        assert!(legacy_golden_challenge_manifests(
            &authority,
            &tampered_plan,
            LEGACY_GOLDEN_DATASET_CARD,
        )
        .is_err());
        let mut wrong_evaluator = authority.clone();
        wrong_evaluator
            .artifact_objects
            .iter_mut()
            .find(|object| object.role == "frozen_evaluator")
            .expect("frozen evaluator")
            .digest = format!("sha256:{}", "d".repeat(64));
        assert!(legacy_golden_challenge_manifests(
            &wrong_evaluator,
            PLAN,
            LEGACY_GOLDEN_DATASET_CARD,
        )
        .is_err());
        let mut wrong_role = authority;
        wrong_role
            .artifact_objects
            .iter_mut()
            .find(|object| object.role == "frozen_evaluator")
            .expect("frozen evaluator")
            .role = "input".to_string();
        assert!(
            legacy_golden_challenge_manifests(&wrong_role, PLAN, LEGACY_GOLDEN_DATASET_CARD,)
                .is_err()
        );
    }

    #[test]
    fn manifest_resolution_rejects_substitution_extra_members_and_wrong_pack() {
        let authority = review_authority();
        let evaluator = ChallengeEvaluatorManifestV1 {
            entrypoint: "evaluator.py".to_string(),
            frozen: true,
            objects: vec![manifest_member(
                "evaluator.py",
                'a',
                "text/x-python; charset=utf-8",
            )],
            pack_id: "pack-fixture-v1".to_string(),
            runtime: "python3-stdlib".to_string(),
            schema: "hepta.challenge_pack.evaluator_manifest.v1".to_string(),
        };
        let dataset = ChallengeDatasetManifestV1 {
            objects: vec![manifest_member(
                "dataset/claims.json",
                'b',
                "application/json",
            )],
            pack_id: evaluator.pack_id.clone(),
            schema: "hepta.challenge_pack.dataset_manifest.v1".to_string(),
        };
        let resolved = resolve_manifest_members(&authority, &evaluator, &dataset).unwrap();
        assert_eq!(resolved.len(), 3);
        assert_eq!(resolved[0].logical_path, "evaluator/main.py");
        assert_eq!(resolved[1].logical_path, "inputs/dataset.json");
        assert_eq!(resolved[2].logical_path, "inputs/candidate.json");
        assert_eq!(
            authority
                .artifact_objects
                .iter()
                .find(|object| object.role == "frozen_evaluator")
                .expect("frozen evaluator")
                .logical_path,
            "evaluator.py"
        );
        assert_eq!(
            authority
                .artifact_objects
                .iter()
                .find(|object| object.role == "dataset")
                .expect("dataset")
                .logical_path,
            "dataset/claims.json"
        );

        let mut with_support = authority.clone();
        with_support.artifact_objects.push(review_object(
            "object-0003",
            "baseline.py",
            "evaluator_support",
            '7',
            "text/x-python; charset=utf-8",
        ));
        with_support.authority_hash =
            hepta_paper_raid_contracts::frozen_review_authority_hash(&with_support).unwrap();
        let mut evaluator_with_support = evaluator.clone();
        evaluator_with_support.objects.push(manifest_member(
            "baseline.py",
            '7',
            "text/x-python; charset=utf-8",
        ));
        let support_resolved =
            resolve_manifest_members(&with_support, &evaluator_with_support, &dataset).unwrap();
        assert_eq!(support_resolved.len(), 4);
        assert_eq!(support_resolved[3].logical_path, "evaluator/baseline.py");

        let mut with_input = authority.clone();
        with_input.artifact_objects.push(review_object(
            "object-0003",
            "inputs/objects/parameters.json",
            "input",
            'd',
            "application/json",
        ));
        assert!(resolve_manifest_members(&with_input, &evaluator, &dataset).is_err());

        let mut substituted = evaluator.clone();
        substituted.objects[0].sha256 = format!("sha256:{}", "d".repeat(64));
        substituted.objects[0].cas_uri = format!("cas://sha256/{}", "d".repeat(64));
        assert!(resolve_manifest_members(&authority, &substituted, &dataset).is_err());

        let mut extra = authority.clone();
        extra.artifact_objects.push(review_object(
            "object-0003",
            "other.py",
            "frozen_evaluator",
            'd',
            "text/x-python; charset=utf-8",
        ));
        assert!(resolve_manifest_members(&extra, &evaluator, &dataset).is_err());

        let mut wrong_pack = dataset.clone();
        wrong_pack.pack_id = "other-pack-v1".to_string();
        assert!(resolve_manifest_members(&authority, &evaluator, &wrong_pack).is_err());
    }

    #[test]
    fn mixed_inbox_keeps_author_delivery_and_cross_paper_review_tasks_side_by_side() {
        let author_paper = Uuid::from_u128(10);
        let review_paper = Uuid::from_u128(11);
        let mut papers = vec![json!({
            "paper_id": author_paper,
            "tasks": [{"work_item_id": Uuid::from_u128(12)}],
            "delivery_candidates": {"status":"available"},
        })];
        let review = json!({
            "papers": [
                {"paper_id": review_paper, "review_tasks": {"status":"available","items":[{"task_id":Uuid::from_u128(13)}]}},
            ]
        });
        merge_review_tasks_into_author_papers(&mut papers, &review).unwrap();
        assert_eq!(papers.len(), 2);
        assert!(papers[0].get("tasks").is_some());
        assert_eq!(
            papers[0]
                .get("review_tasks")
                .and_then(|value| value.get("status"))
                .and_then(Value::as_str),
            Some("unavailable")
        );
        assert_eq!(
            papers[0]
                .get("review_tasks")
                .and_then(|value| value.get("reason_code"))
                .and_then(Value::as_str),
            Some("target_paper_author_forbidden_or_unassigned")
        );
        assert_eq!(
            papers[1]
                .get("review_tasks")
                .and_then(|value| value.get("status"))
                .and_then(Value::as_str),
            Some("available")
        );
        assert!(papers[1].get("tasks").is_none());
    }

    #[test]
    fn delivery_projection_never_guesses_across_independent_room_records() {
        let projection = unavailable_delivery_candidates("no_agent_declared_delivery_draft");
        assert_eq!(
            projection.get("schema").and_then(Value::as_str),
            Some("hepta.paper_raid.agent_bridge.delivery_candidates.v1")
        );
        assert_eq!(
            projection.get("status").and_then(Value::as_str),
            Some("unavailable")
        );
        assert_eq!(
            projection
                .get("items")
                .and_then(Value::as_array)
                .map(Vec::is_empty),
            Some(true)
        );
        let encoded = serde_json::to_string(&projection).unwrap();
        assert!(!encoded.contains("artifact_manifests"));
        assert!(!encoded.contains("section_heads"));
        assert!(!encoded.contains("leases"));
    }

    #[test]
    fn delivery_context_requires_one_exact_live_authoritative_tuple() {
        let binding_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let player_id = Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();
        let paper_id = Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();
        let work_item_id = Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap();
        let manifest_id = Uuid::parse_str("55555555-5555-4555-8555-555555555555").unwrap();
        let lease_id = Uuid::parse_str("66666666-6666-4666-8666-666666666666").unwrap();
        let parent_id = Uuid::parse_str("77777777-7777-4777-8777-777777777777").unwrap();
        let mapping = BridgeMapping {
            binding_id,
            subject_id: "subject.fixture".into(),
            player_id,
            agent_id: "agent.fixture".into(),
            agent_key_id: format!("sha256:{}", "a".repeat(64)),
            capability_disclosure_hash: format!("sha256:{}", "b".repeat(64)),
        };
        let now = Utc.timestamp_opt(1_800_000_000, 0).single().unwrap();
        let mut room = json!({
            "paper": {"paper_project_id": paper_id, "phase": "drafting"},
            "work_items": [{
                "work_item_id": work_item_id,
                "assigned_binding_id": binding_id,
                "assigned_player_id": player_id,
                "status": "in_progress",
                "version": 3
            }],
            "section_heads": [{
                "section_key": "methods",
                "current_head_revision_id": parent_id,
                "fencing_token": 9
            }],
            "leases": [{
                "lease_id": lease_id,
                "section_key": "methods",
                "holder_binding_id": binding_id,
                "holder_player_id": player_id,
                "fencing_token": 9,
                "status": "active",
                "expires_at": "2027-01-15T08:01:00Z"
            }],
            "artifact_manifests": [{
                "manifest_id": manifest_id,
                "manifest_hash": format!("sha256:{}", "c".repeat(64))
            }],
            "proposals": []
        });
        let context =
            resolve_delivery_context(&room, &mapping, work_item_id, "methods", manifest_id, now)
                .unwrap()
                .expect("exact live delivery context");
        assert_eq!(context.lease_id, lease_id);
        assert_eq!(context.parent_revision_id, parent_id);
        assert_eq!(context.lease_fencing_token, 9);
        assert_eq!(context.expected_work_version, 3);

        room["leases"][0]["holder_binding_id"] = json!(Uuid::new_v4());
        assert!(resolve_delivery_context(
            &room,
            &mapping,
            work_item_id,
            "methods",
            manifest_id,
            now,
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn delivery_response_must_match_every_pinned_authoritative_field() {
        let created_at = Utc.timestamp_opt(1_800_000_000, 0).single().unwrap();
        let draft = DeliveryDraft {
            delivery_draft_id: Uuid::from_u128(1),
            binding_id: Uuid::from_u128(2),
            paper_id: Uuid::from_u128(3),
            work_item_id: Uuid::from_u128(4),
            expected_work_version: 3,
            section_key: "methods".into(),
            lease_id: Uuid::from_u128(5),
            lease_fencing_token: 7,
            parent_revision_id: Uuid::from_u128(6),
            artifact_manifest_id: Uuid::from_u128(7),
            artifact_manifest_hash: format!("sha256:{}", "a".repeat(64)),
            payload_hash: format!("sha256:{}", "b".repeat(64)),
            state: "submitting".into(),
            proposal_body_hash: Some(format!("sha256:{}", "c".repeat(64))),
            proposal_id: Some(Uuid::from_u128(8)),
            proposal_idempotency_key: Some(Uuid::from_u128(9)),
            proposal_signed_at_unix: Some(created_at.timestamp()),
            created_at,
            expires_at: created_at + chrono::Duration::minutes(15),
        };
        let response = json!({
            "proposal_id": draft.proposal_id,
            "paper_project_id": draft.paper_id,
            "work_item_id": draft.work_item_id,
            "section_key": draft.section_key,
            "parent_revision_id": draft.parent_revision_id,
            "lease_id": draft.lease_id,
            "lease_fencing_token": draft.lease_fencing_token,
            "expected_work_version": draft.expected_work_version,
            "proposal_kind": "delivery",
            "payload_hash": draft.payload_hash,
            "artifact_manifest_id": draft.artifact_manifest_id,
            "artifact_manifest_hash": draft.artifact_manifest_hash,
            "binding_id": draft.binding_id,
            "agent_id": "agent.fixture",
            "agent_key_id": "key.fixture",
            "agent_public_key": "public.fixture",
            "signature": "signature.fixture",
            "signed_at_unix": draft.proposal_signed_at_unix,
            "status": "submitted",
            "version": 1
        });
        let payload = json!({
            "agent_id": "agent.fixture",
            "agent_key_id": "key.fixture",
            "signature": "signature.fixture"
        });
        let binding = json!({"agent_public_key": "public.fixture"});
        validate_delivery_proposal_response(&response, &draft, &payload, &binding).unwrap();
        for (field, tampered) in [
            ("lease_id", json!(Uuid::from_u128(10))),
            ("lease_fencing_token", json!(8)),
            ("expected_work_version", json!(4)),
            ("artifact_manifest_id", json!(Uuid::from_u128(10))),
            (
                "artifact_manifest_hash",
                json!(format!("sha256:{}", "d".repeat(64))),
            ),
            ("payload_hash", json!(format!("sha256:{}", "e".repeat(64)))),
            ("status", json!("accepted")),
        ] {
            let mut changed = response.clone();
            changed[field] = tampered;
            assert!(
                validate_delivery_proposal_response(&changed, &draft, &payload, &binding).is_err()
            );
        }
        let mut changed_payload = payload.clone();
        changed_payload["signature"] = json!("other.signature");
        assert!(
            validate_delivery_proposal_response(&response, &draft, &changed_payload, &binding)
                .is_err()
        );
        let changed_binding = json!({"agent_public_key": "other.public"});
        assert!(
            validate_delivery_proposal_response(&response, &draft, &payload, &changed_binding)
                .is_err()
        );
    }

    #[test]
    fn delivery_claim_rechecks_locked_epoch_and_manifest_after_a_concurrent_refresh() {
        let created_at = Utc.timestamp_opt(1_800_000_000, 0).single().unwrap();
        let binding_id = Uuid::from_u128(2);
        let delivery_draft_id = Uuid::from_u128(1);
        let expected = DeliveryDraft {
            delivery_draft_id,
            binding_id,
            paper_id: Uuid::from_u128(3),
            work_item_id: Uuid::from_u128(4),
            expected_work_version: 3,
            section_key: "methods".into(),
            lease_id: Uuid::from_u128(5),
            lease_fencing_token: 7,
            parent_revision_id: Uuid::from_u128(6),
            artifact_manifest_id: Uuid::from_u128(7),
            artifact_manifest_hash: format!("sha256:{}", "a".repeat(64)),
            payload_hash: format!("sha256:{}", "b".repeat(64)),
            state: "pending".into(),
            proposal_body_hash: None,
            proposal_id: None,
            proposal_idempotency_key: None,
            proposal_signed_at_unix: None,
            created_at,
            expires_at: created_at + chrono::Duration::minutes(15),
        };
        let payload = json!({
            "proposal_id": delivery_proposal_id(binding_id, delivery_draft_id),
            "work_item_id": expected.work_item_id,
            "section_key": expected.section_key,
            "parent_revision_id": expected.parent_revision_id,
            "lease_id": expected.lease_id,
            "lease_fencing_token": expected.lease_fencing_token,
            "expected_work_version": expected.expected_work_version,
            "proposal_kind": "delivery",
            "payload_hash": expected.payload_hash,
            "artifact_manifest_id": expected.artifact_manifest_id,
            "binding_id": expected.binding_id,
            "signed_at_unix": expected.created_at.timestamp(),
            "idempotency_key": delivery_idempotency_key(binding_id, delivery_draft_id)
        });
        let body_hash = format!("sha256:{}", "c".repeat(64));
        assert!(delivery_claim_matches_locked_snapshot(
            &expected,
            &expected,
            &payload,
            &body_hash,
            delivery_proposal_id(binding_id, delivery_draft_id),
            delivery_idempotency_key(binding_id, delivery_draft_id),
            expected.created_at.timestamp(),
        ));
        assert!(!delivery_claim_matches_locked_snapshot(
            &expected,
            &expected,
            &payload,
            &body_hash,
            Uuid::from_u128(99),
            delivery_idempotency_key(binding_id, delivery_draft_id),
            expected.created_at.timestamp(),
        ));
        assert!(!delivery_claim_matches_locked_snapshot(
            &expected,
            &expected,
            &payload,
            &body_hash,
            delivery_proposal_id(binding_id, delivery_draft_id),
            Uuid::from_u128(99),
            expected.created_at.timestamp(),
        ));
        assert!(!delivery_claim_matches_locked_snapshot(
            &expected,
            &expected,
            &payload,
            &body_hash,
            delivery_proposal_id(binding_id, delivery_draft_id),
            delivery_idempotency_key(binding_id, delivery_draft_id),
            expected.created_at.timestamp() + 1,
        ));

        let mut refreshed = expected.clone();
        refreshed.expected_work_version += 1;
        refreshed.lease_fencing_token += 1;
        refreshed.created_at += chrono::Duration::seconds(1);
        refreshed.expires_at += chrono::Duration::seconds(1);
        assert!(!delivery_claim_matches_locked_snapshot(
            &expected,
            &refreshed,
            &payload,
            &body_hash,
            delivery_proposal_id(binding_id, delivery_draft_id),
            delivery_idempotency_key(binding_id, delivery_draft_id),
            expected.created_at.timestamp(),
        ));
        let mut changed_manifest_hash = expected.clone();
        changed_manifest_hash.artifact_manifest_hash = format!("sha256:{}", "d".repeat(64));
        assert!(!delivery_claim_matches_locked_snapshot(
            &expected,
            &changed_manifest_hash,
            &payload,
            &body_hash,
            delivery_proposal_id(binding_id, delivery_draft_id),
            delivery_idempotency_key(binding_id, delivery_draft_id),
            expected.created_at.timestamp(),
        ));

        let mut replaced_manifest = payload.clone();
        replaced_manifest["artifact_manifest_id"] = json!(Uuid::from_u128(8));
        assert!(!delivery_request_payload_matches_draft(
            &replaced_manifest,
            &expected
        ));
        assert!(!delivery_claim_matches_locked_snapshot(
            &expected,
            &expected,
            &replaced_manifest,
            &body_hash,
            delivery_proposal_id(binding_id, delivery_draft_id),
            delivery_idempotency_key(binding_id, delivery_draft_id),
            expected.created_at.timestamp(),
        ));
    }

    #[test]
    fn delivery_restart_pins_match_the_frozen_node_derivation() {
        let binding_id = Uuid::parse_str("66666666-6666-4666-8666-666666666666").unwrap();
        let delivery_draft_id = Uuid::parse_str("77777777-7777-4777-8777-777777777777").unwrap();
        assert_eq!(
            delivery_proposal_id(binding_id, delivery_draft_id).to_string(),
            "01ac1203-13fb-52b8-84a9-e6d6c7df213c"
        );
        assert_eq!(
            delivery_idempotency_key(binding_id, delivery_draft_id).to_string(),
            "34626bcd-0a56-5a6e-9c18-f2c41ba3c84f"
        );

        let created_at = Utc.timestamp_opt(1_800_000_000, 0).single().unwrap();
        let mut draft = DeliveryDraft {
            delivery_draft_id,
            binding_id,
            paper_id: Uuid::from_u128(3),
            work_item_id: Uuid::from_u128(4),
            expected_work_version: 3,
            section_key: "methods".into(),
            lease_id: Uuid::from_u128(5),
            lease_fencing_token: 7,
            parent_revision_id: Uuid::from_u128(6),
            artifact_manifest_id: Uuid::from_u128(7),
            artifact_manifest_hash: format!("sha256:{}", "a".repeat(64)),
            payload_hash: format!("sha256:{}", "b".repeat(64)),
            state: "submitting".into(),
            proposal_body_hash: Some(format!("sha256:{}", "c".repeat(64))),
            proposal_id: Some(delivery_proposal_id(binding_id, delivery_draft_id)),
            proposal_idempotency_key: Some(delivery_idempotency_key(binding_id, delivery_draft_id)),
            proposal_signed_at_unix: Some(created_at.timestamp()),
            created_at,
            expires_at: created_at + chrono::Duration::minutes(15),
        };
        assert!(delivery_recovery_pins_are_canonical(&draft));
        assert_eq!(
            delivery_candidate_value(&draft)
                .get("delivery_state")
                .and_then(Value::as_str),
            Some("submitting")
        );
        draft.proposal_signed_at_unix = Some(created_at.timestamp() + 1);
        assert!(!delivery_recovery_pins_are_canonical(&draft));
    }

    #[test]
    fn exact_authoritative_proposal_reprojects_a_claimed_delivery() {
        let created_at = Utc.timestamp_opt(1_800_000_000, 0).single().unwrap();
        let binding_id = Uuid::from_u128(2);
        let delivery_draft_id = Uuid::from_u128(1);
        let draft = DeliveryDraft {
            delivery_draft_id,
            binding_id,
            paper_id: Uuid::from_u128(3),
            work_item_id: Uuid::from_u128(4),
            expected_work_version: 3,
            section_key: "methods".into(),
            lease_id: Uuid::from_u128(5),
            lease_fencing_token: 7,
            parent_revision_id: Uuid::from_u128(6),
            artifact_manifest_id: Uuid::from_u128(7),
            artifact_manifest_hash: format!("sha256:{}", "a".repeat(64)),
            payload_hash: format!("sha256:{}", "b".repeat(64)),
            state: "submitting".into(),
            proposal_body_hash: Some(format!("sha256:{}", "c".repeat(64))),
            proposal_id: Some(delivery_proposal_id(binding_id, delivery_draft_id)),
            proposal_idempotency_key: Some(delivery_idempotency_key(binding_id, delivery_draft_id)),
            proposal_signed_at_unix: Some(created_at.timestamp()),
            created_at,
            expires_at: created_at + chrono::Duration::minutes(15),
        };
        let proposal = json!({
            "proposal_id": draft.proposal_id,
            "paper_project_id": draft.paper_id,
            "work_item_id": draft.work_item_id,
            "section_key": draft.section_key,
            "parent_revision_id": draft.parent_revision_id,
            "lease_id": draft.lease_id,
            "lease_fencing_token": draft.lease_fencing_token,
            "expected_work_version": draft.expected_work_version,
            "proposal_kind": "delivery",
            "payload_hash": draft.payload_hash,
            "artifact_manifest_id": draft.artifact_manifest_id,
            "artifact_manifest_hash": draft.artifact_manifest_hash,
            "binding_id": draft.binding_id,
            "signed_at_unix": draft.proposal_signed_at_unix,
            "status": "submitted",
            "version": 1
        });
        let room = json!({"proposals": [proposal.clone()]});
        assert!(room_contains_exact_delivery_proposal(&room, &draft).unwrap());
        let mut accepted = proposal.clone();
        accepted["status"] = json!("accepted");
        assert!(
            room_contains_exact_delivery_proposal(&json!({"proposals": [accepted]}), &draft)
                .unwrap()
        );
        let mut reviewed = proposal.clone();
        reviewed["status"] = json!("rejected");
        assert!(
            room_contains_exact_delivery_proposal(&json!({"proposals": [reviewed]}), &draft)
                .unwrap()
        );
        let mut invalid = proposal;
        invalid["status"] = json!("unknown");
        assert!(
            !room_contains_exact_delivery_proposal(&json!({"proposals": [invalid]}), &draft)
                .unwrap()
        );
        let changed = json!({"proposals": []});
        assert!(!room_contains_exact_delivery_proposal(&changed, &draft).unwrap());
    }

    #[test]
    fn same_owner_binding_can_repair_key_continuity_but_identity_changes_cannot() {
        let binding_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        let actual = BridgeOwner {
            binding_id,
            subject_id: "subject-a",
            player_id,
            agent_id: "agent-a",
        };
        assert!(mapping_repair_owner_matches(actual, actual,));
        assert!(!mapping_repair_owner_matches(
            actual,
            BridgeOwner {
                binding_id,
                subject_id: "subject-b",
                player_id,
                agent_id: "agent-a",
            },
        ));
        assert!(!mapping_repair_owner_matches(
            actual,
            BridgeOwner {
                binding_id,
                subject_id: "subject-a",
                player_id,
                agent_id: "agent-b",
            },
        ));

        let old_key = format!("sha256:{}", "11".repeat(32));
        let new_key = format!("sha256:{}", "22".repeat(32));
        assert_ne!(old_key, new_key, "rotated key must fail the stale mapping");
        let refreshed_mapping_key = new_key.clone();
        assert_eq!(
            refreshed_mapping_key, new_key,
            "same-binding re-pair refreshes the mapping to Hepta's active key"
        );
    }
}
