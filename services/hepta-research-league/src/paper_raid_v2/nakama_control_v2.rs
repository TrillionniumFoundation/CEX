use axum::{
    extract::State,
    http::{header::CONTENT_TYPE, HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use base64::Engine as _;
use chrono::{DateTime, Timelike, Utc};
use ed25519_dalek::{Signature, Signer, Verifier};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use super::{
    assert_team_actor_memory, assert_team_actor_postgres, decode_record,
    enforce_user_assertion_time, insert_postgres_event, push_memory_event, request_hash,
    require_user_assertion_for_applied_replay, validate_idempotency_key, JointPaperSubmission,
    JointSubmissionStatus, PaperRaidMemory, ResearchSessionAuthorizationSetStatus,
    ResearchSessionAuthorizationSetV1,
};
use crate::{
    paper_raid_contracts::{
        canonical_json_bytes, research_control_complete_business_v2,
        research_control_create_business_v2, research_control_replace_business_v2,
        research_control_resume_business_v2, research_session_roster_root, sha256_digest,
        sign_research_control_v2, verify_nakama_completion_receipt, verify_research_control_v2,
        verify_research_session_authorization, ResearchControlClaimV2,
        ResearchControlCompleteRequestV2, ResearchControlCreateRequestV2,
        ResearchControlEvidenceResultV1, ResearchControlOperationV2,
        ResearchControlReplaceRequestV2, ResearchControlResultV2, ResearchControlResumeRequestV2,
        ResearchControlRuntimeResultV1, ResearchSessionRosterMemberV1,
        ResearchSessionTerminalFactsV1, SignedResearchControlV2, JSON_SAFE_U64_MAX,
        RESEARCH_CONTROL_AUDIENCE_V2, RESEARCH_CONTROL_CLAIM_V2,
        RESEARCH_CONTROL_COMPLETE_REQUEST_V2, RESEARCH_CONTROL_CREATE_REQUEST_V2,
        RESEARCH_CONTROL_MAXIMUM_LIFETIME_SECONDS, RESEARCH_CONTROL_REPLACE_REQUEST_V2,
        RESEARCH_CONTROL_RESULT_V2, RESEARCH_CONTROL_RESUME_REQUEST_V2,
    },
    ApiError, AppState,
};

const COMMAND_SCHEMA_V2: &str = "hepta.paper_raid.nakama_research_control_command.v2";

fn command_storage_now() -> DateTime<Utc> {
    let now = Utc::now();
    now.with_nanosecond((now.nanosecond() / 1_000) * 1_000)
        .expect("a truncated PostgreSQL timestamp is valid")
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NakamaResearchControlCommandStatusV2 {
    Pending,
    Applied,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NakamaResearchControlCommandV2 {
    pub schema: String,
    pub command_id: Uuid,
    pub operation: ResearchControlOperationV2,
    pub target_rpc: String,
    pub session_id: String,
    pub session_roster_version: u64,
    pub authorization_set_id: Uuid,
    pub payload_hash: String,
    pub idempotency_key: String,
    pub request_hash: String,
    pub request_sha256: String,
    pub status: NakamaResearchControlCommandStatusV2,
    pub response: Option<Value>,
    pub response_sha256: Option<String>,
    pub response_seal_signature: Option<String>,
    pub attempt_count: u64,
    pub last_error_code: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(crate) struct StoredControlCommand {
    pub(crate) record: NakamaResearchControlCommandV2,
    pub(crate) idempotency_key: String,
    pub(crate) request_hash: String,
    pub(crate) request_body: Vec<u8>,
    pub(crate) response_body: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateNakamaResearchSessionControlRequestV2 {
    pub authorization_set_id: Uuid,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResumeNakamaResearchSessionControlRequestV2 {
    pub session_id: String,
    pub roster_version: u64,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceNakamaResearchSessionRosterControlRequestV2 {
    pub authorization_set_id: Uuid,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteNakamaResearchSessionControlRequestV2 {
    pub session_id: String,
    pub roster_version: u64,
    pub idempotency_key: String,
}

#[derive(Clone)]
enum OwnedControlLocator {
    AuthorizationSet(Uuid),
    SessionEpoch {
        session_id: String,
        roster_version: u64,
    },
}

struct ControlInvocation {
    operation: ResearchControlOperationV2,
    canonical_path: &'static str,
    idempotency_key: String,
    request_hash: String,
    locator: OwnedControlLocator,
}

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/v2/hepta/nakama/research-session-controls/create",
            post(create_control),
        )
        .route(
            "/v2/hepta/nakama/research-session-controls/resume",
            post(resume_control),
        )
        .route(
            "/v2/hepta/nakama/research-session-controls/replace-roster",
            post(replace_roster_control),
        )
        .route(
            "/v2/hepta/nakama/research-session-controls/complete",
            post(complete_control),
        )
}

async fn create_control(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateNakamaResearchSessionControlRequestV2>,
) -> Result<Json<NakamaResearchControlCommandV2>, ApiError> {
    let invocation = ControlInvocation {
        operation: ResearchControlOperationV2::Create,
        canonical_path: "/v2/hepta/nakama/research-session-controls/create",
        idempotency_key: request.idempotency_key.clone(),
        request_hash: request_hash(&request)?,
        locator: OwnedControlLocator::AuthorizationSet(request.authorization_set_id),
    };
    execute_control(state, headers, invocation).await.map(Json)
}

async fn resume_control(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ResumeNakamaResearchSessionControlRequestV2>,
) -> Result<Json<NakamaResearchControlCommandV2>, ApiError> {
    let invocation = ControlInvocation {
        operation: ResearchControlOperationV2::Resume,
        canonical_path: "/v2/hepta/nakama/research-session-controls/resume",
        idempotency_key: request.idempotency_key.clone(),
        request_hash: request_hash(&request)?,
        locator: OwnedControlLocator::SessionEpoch {
            session_id: request.session_id,
            roster_version: request.roster_version,
        },
    };
    execute_control(state, headers, invocation).await.map(Json)
}

async fn replace_roster_control(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ReplaceNakamaResearchSessionRosterControlRequestV2>,
) -> Result<Json<NakamaResearchControlCommandV2>, ApiError> {
    let invocation = ControlInvocation {
        operation: ResearchControlOperationV2::ReplaceRoster,
        canonical_path: "/v2/hepta/nakama/research-session-controls/replace-roster",
        idempotency_key: request.idempotency_key.clone(),
        request_hash: request_hash(&request)?,
        locator: OwnedControlLocator::AuthorizationSet(request.authorization_set_id),
    };
    execute_control(state, headers, invocation).await.map(Json)
}

async fn complete_control(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CompleteNakamaResearchSessionControlRequestV2>,
) -> Result<Json<NakamaResearchControlCommandV2>, ApiError> {
    let invocation = ControlInvocation {
        operation: ResearchControlOperationV2::Complete,
        canonical_path: "/v2/hepta/nakama/research-session-controls/complete",
        idempotency_key: request.idempotency_key.clone(),
        request_hash: request_hash(&request)?,
        locator: OwnedControlLocator::SessionEpoch {
            session_id: request.session_id,
            roster_version: request.roster_version,
        },
    };
    execute_control(state, headers, invocation).await.map(Json)
}

async fn execute_control(
    state: AppState,
    headers: HeaderMap,
    invocation: ControlInvocation,
) -> Result<NakamaResearchControlCommandV2, ApiError> {
    validate_idempotency_key(&invocation.idempotency_key)?;
    validate_locator(&invocation.locator)?;
    let assertion = require_user_assertion_for_applied_replay(
        &headers,
        &state,
        invocation.operation_name(),
        "POST",
        invocation.canonical_path,
        &invocation.idempotency_key,
        &invocation.request_hash,
    )?;
    let (stored, set) = if state.pool.is_some() {
        prepare_postgres_command(&state, &invocation, &assertion).await?
    } else {
        prepare_memory_command(&state, &invocation, &assertion).await?
    };
    if stored.record.status == NakamaResearchControlCommandStatusV2::Applied {
        return validate_applied_replay(&state, stored, &set).await;
    }
    match dispatch_control(&state, &stored, &set).await {
        Ok((response, response_body)) => {
            persist_applied(&state, stored, &set, response, response_body).await
        }
        Err(error) => {
            record_dispatch_failure(&state, &stored, error.code).await?;
            Err(error)
        }
    }
}

impl ControlInvocation {
    fn operation_name(&self) -> &'static str {
        match self.operation {
            ResearchControlOperationV2::Create => "create_nakama_research_session_control_v2",
            ResearchControlOperationV2::Resume => "resume_nakama_research_session_control_v2",
            ResearchControlOperationV2::ReplaceRoster => {
                "replace_nakama_research_session_roster_control_v2"
            }
            ResearchControlOperationV2::Complete => "complete_nakama_research_session_control_v2",
        }
    }
}

fn validate_locator(locator: &OwnedControlLocator) -> Result<(), ApiError> {
    if let OwnedControlLocator::SessionEpoch {
        session_id,
        roster_version,
    } = locator
    {
        crate::validate_contract_text_api("session_id", session_id)?;
        if *roster_version == 0 || *roster_version > JSON_SAFE_U64_MAX {
            return Err(ApiError::bad_request(
                "invalid_session_roster_version",
                "session roster version must be a positive JSON-safe integer",
            ));
        }
    }
    Ok(())
}

async fn prepare_memory_command(
    state: &AppState,
    invocation: &ControlInvocation,
    assertion: &crate::paper_raid_contracts::ConsumerUserAssertionClaimV2,
) -> Result<(StoredControlCommand, ResearchSessionAuthorizationSetV1), ApiError> {
    let mut memory = state.paper_raid.write().await;
    let set = locate_memory_set(&memory, &invocation.locator)?;
    ensure_latest_memory_epoch(&memory, &set)?;
    let team = memory
        .teams
        .get(&set.team_id)
        .ok_or_else(|| ApiError::internal("research session team record is missing"))?;
    assert_team_actor_memory(&memory, team, assertion)?;
    let key = (
        invocation.operation.as_str().to_string(),
        invocation.idempotency_key.clone(),
    );
    if let Some(command_id) = memory.nakama_control_idempotency.get(&key) {
        let stored = memory
            .nakama_control_commands
            .get(command_id)
            .cloned()
            .ok_or_else(|| ApiError::internal("Nakama control idempotency index is corrupt"))?;
        ensure_request_hash(&stored, &invocation.request_hash)?;
        validate_stored_signed_request(state, &stored, &set)?;
        if stored.record.status == NakamaResearchControlCommandStatusV2::Pending {
            enforce_user_assertion_time(assertion)?;
        }
        return Ok((stored, set));
    }
    enforce_user_assertion_time(assertion)?;
    validate_control_epoch(state, &set, invocation.operation)?;
    let submission = if invocation.operation == ResearchControlOperationV2::Complete {
        Some(locate_memory_submission(&memory, &set)?)
    } else {
        None
    };
    let stored = build_control_command(state, invocation, &set, submission.as_ref())?;
    memory
        .nakama_control_idempotency
        .insert(key, stored.record.command_id);
    memory
        .nakama_control_commands
        .insert(stored.record.command_id, stored.clone());
    push_memory_event(
        &mut memory,
        invocation.operation_name(),
        &invocation.idempotency_key,
        "hepta.paper_raid.nakama_control.prepared.v2",
        stored.record.command_id,
        1,
        serde_json::to_value(&stored.record)
            .map_err(|error| ApiError::internal(format!("encode control event: {error}")))?,
    )?;
    Ok((stored, set))
}

fn locate_memory_set(
    memory: &PaperRaidMemory,
    locator: &OwnedControlLocator,
) -> Result<ResearchSessionAuthorizationSetV1, ApiError> {
    let found = match locator {
        OwnedControlLocator::AuthorizationSet(authorization_set_id) => memory
            .research_session_authorization_sets
            .values()
            .find(|set| set.authorization_set_id == *authorization_set_id),
        OwnedControlLocator::SessionEpoch {
            session_id,
            roster_version,
        } => memory
            .research_session_authorization_sets
            .get(&(session_id.clone(), *roster_version)),
    };
    found.cloned().ok_or_else(|| {
        ApiError::not_found(
            "authorization_set_not_found",
            "Nakama control command has no matching authorization epoch",
        )
    })
}

fn ensure_latest_memory_epoch(
    memory: &PaperRaidMemory,
    set: &ResearchSessionAuthorizationSetV1,
) -> Result<(), ApiError> {
    let latest = memory
        .research_session_authorization_sets
        .values()
        .filter(|candidate| candidate.session_id == set.session_id)
        .map(|candidate| candidate.roster_version)
        .max()
        .ok_or_else(|| ApiError::internal("authorization epoch history is empty"))?;
    if latest != set.roster_version {
        return Err(ApiError::conflict(
            "control_not_latest_authorization_epoch",
            "Nakama control commands must bind the latest authorization epoch",
        ));
    }
    Ok(())
}

fn locate_memory_submission(
    memory: &PaperRaidMemory,
    set: &ResearchSessionAuthorizationSetV1,
) -> Result<JointPaperSubmission, ApiError> {
    memory
        .submissions
        .values()
        .find(|submission| {
            submission.paper_project_id == set.paper_project_id
                && submission.status == JointSubmissionStatus::SubmissionReady
        })
        .cloned()
        .ok_or_else(|| {
            ApiError::conflict(
                "paper_bundle_not_finalized",
                "completion control requires a finalized canonical PaperBundle",
            )
        })
}

async fn prepare_postgres_command(
    state: &AppState,
    invocation: &ControlInvocation,
    assertion: &crate::paper_raid_contracts::ConsumerUserAssertionClaimV2,
) -> Result<(StoredControlCommand, ResearchSessionAuthorizationSetV1), ApiError> {
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::internal("PostgreSQL state is unavailable"))?;
    let mut tx = pool.begin().await.map_err(ApiError::database)?;
    sqlx::query("select pg_advisory_xact_lock(hashtext($1))")
        .bind(format!(
            "hepta-nakama-control:{}:{}",
            invocation.operation.as_str(),
            invocation.idempotency_key
        ))
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    let set = locate_postgres_set(&mut tx, &invocation.locator).await?;
    ensure_latest_postgres_epoch(&mut tx, &set).await?;
    assert_team_actor_postgres(&mut tx, set.team_id, assertion).await?;
    let existing = sqlx::query(
        "select command_id, operation, target_rpc, idempotency_key, request_hash,
                session_id, session_roster_version, authorization_set_id, payload_hash,
                request_body, request_sha256, status, response_body, response_sha256,
                response_seal_signature, attempt_count, last_error_code, created_at, updated_at,
                record_json
         from hepta_nakama_research_control_commands
         where operation = $1 and idempotency_key = $2",
    )
    .bind(invocation.operation.as_str())
    .bind(&invocation.idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if let Some(row) = existing {
        let stored = decode_stored_command_row(&row)?;
        ensure_request_hash(&stored, &invocation.request_hash)?;
        validate_stored_signed_request(state, &stored, &set)?;
        if stored.record.status == NakamaResearchControlCommandStatusV2::Pending {
            enforce_user_assertion_time(assertion)?;
        }
        tx.commit().await.map_err(ApiError::database)?;
        return Ok((stored, set));
    }
    enforce_user_assertion_time(assertion)?;
    validate_control_epoch(state, &set, invocation.operation)?;
    let submission = if invocation.operation == ResearchControlOperationV2::Complete {
        Some(locate_postgres_submission(&mut tx, &set).await?)
    } else {
        None
    };
    let stored = build_control_command(state, invocation, &set, submission.as_ref())?;
    let record_json = serde_json::to_value(&stored.record)
        .map_err(|error| ApiError::internal(format!("encode control command: {error}")))?;
    sqlx::query(
        "insert into hepta_nakama_research_control_commands (
            command_id, operation, target_rpc, idempotency_key, request_hash,
            session_id, session_roster_version, authorization_set_id, payload_hash,
            request_body, request_sha256, status, response_body, response_sha256,
            response_seal_signature, attempt_count, last_error_code, record_json,
            created_at, updated_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'pending',null,null,null,0,null,$12::jsonb,$13,$13)",
    )
    .bind(stored.record.command_id)
    .bind(stored.record.operation.as_str())
    .bind(&stored.record.target_rpc)
    .bind(&stored.idempotency_key)
    .bind(&stored.request_hash)
    .bind(&stored.record.session_id)
    .bind(i64::try_from(stored.record.session_roster_version).map_err(|_| {
        ApiError::internal("control command roster version does not fit PostgreSQL bigint")
    })?)
    .bind(stored.record.authorization_set_id)
    .bind(&stored.record.payload_hash)
    .bind(&stored.request_body)
    .bind(&stored.record.request_sha256)
    .bind(record_json)
    .bind(stored.record.created_at)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    insert_postgres_event(
        &mut tx,
        invocation.operation_name(),
        &invocation.idempotency_key,
        "hepta.paper_raid.nakama_control.prepared.v2",
        stored.record.command_id,
        1,
        serde_json::to_value(&stored.record)
            .map_err(|error| ApiError::internal(format!("encode control event: {error}")))?,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((stored, set))
}

async fn locate_postgres_set(
    tx: &mut Transaction<'_, Postgres>,
    locator: &OwnedControlLocator,
) -> Result<ResearchSessionAuthorizationSetV1, ApiError> {
    let row = match locator {
        OwnedControlLocator::AuthorizationSet(authorization_set_id) => {
            sqlx::query(
                "select record_json from hepta_research_session_authorization_sets
             where authorization_set_id = $1 for share",
            )
            .bind(authorization_set_id)
            .fetch_optional(&mut **tx)
            .await
        }
        OwnedControlLocator::SessionEpoch {
            session_id,
            roster_version,
        } => {
            sqlx::query(
                "select record_json from hepta_research_session_authorization_sets
             where session_id = $1 and roster_version = $2 for share",
            )
            .bind(session_id)
            .bind(i64::try_from(*roster_version).map_err(|_| {
                ApiError::bad_request(
                    "invalid_session_roster_version",
                    "session roster version does not fit PostgreSQL bigint",
                )
            })?)
            .fetch_optional(&mut **tx)
            .await
        }
    }
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "authorization_set_not_found",
            "Nakama control command has no matching authorization epoch",
        )
    })?;
    decode_record(row.get("record_json"), "research session authorization set")
}

async fn ensure_latest_postgres_epoch(
    tx: &mut Transaction<'_, Postgres>,
    set: &ResearchSessionAuthorizationSetV1,
) -> Result<(), ApiError> {
    let latest: i64 = sqlx::query_scalar(
        "select max(roster_version) from hepta_research_session_authorization_sets
         where session_id = $1",
    )
    .bind(&set.session_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    if u64::try_from(latest).ok() != Some(set.roster_version) {
        return Err(ApiError::conflict(
            "control_not_latest_authorization_epoch",
            "Nakama control commands must bind the latest authorization epoch",
        ));
    }
    Ok(())
}

async fn locate_postgres_submission(
    tx: &mut Transaction<'_, Postgres>,
    set: &ResearchSessionAuthorizationSetV1,
) -> Result<JointPaperSubmission, ApiError> {
    let row = sqlx::query(
        "select record_json from hepta_joint_paper_submissions
         where paper_project_id = $1 and status = 'submission_ready' for share",
    )
    .bind(set.paper_project_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::conflict(
            "paper_bundle_not_finalized",
            "completion control requires a finalized canonical PaperBundle",
        )
    })?;
    decode_record(row.get("record_json"), "joint paper submission")
}

fn ensure_request_hash(
    stored: &StoredControlCommand,
    expected_request_hash: &str,
) -> Result<(), ApiError> {
    if stored.request_hash != expected_request_hash {
        return Err(ApiError::conflict(
            "idempotency_key_conflict",
            "idempotency_key was already used with a different control request",
        ));
    }
    if sha256_digest(&stored.request_body) != stored.record.request_sha256 {
        return Err(ApiError::internal(
            "stored Nakama control request checksum verification failed",
        ));
    }
    Ok(())
}

fn decode_stored_command_row(
    row: &sqlx::postgres::PgRow,
) -> Result<StoredControlCommand, ApiError> {
    let record: NakamaResearchControlCommandV2 =
        decode_record(row.get("record_json"), "Nakama control command")?;
    let row_command_id: Uuid = row.get("command_id");
    let row_operation: String = row.get("operation");
    let row_target_rpc: String = row.get("target_rpc");
    let row_idempotency_key: String = row.get("idempotency_key");
    let row_request_hash: String = row.get("request_hash");
    let row_session_id: String = row.get("session_id");
    let row_session_roster_version: i64 = row.get("session_roster_version");
    let row_authorization_set_id: Uuid = row.get("authorization_set_id");
    let row_payload_hash: String = row.get("payload_hash");
    let row_request_sha256: String = row.get("request_sha256");
    let row_status: String = row.get("status");
    let row_response_sha256: Option<String> = row.get("response_sha256");
    let row_response_seal_signature: Option<String> = row.get("response_seal_signature");
    let row_attempt_count: i64 = row.get("attempt_count");
    let row_last_error_code: Option<String> = row.get("last_error_code");
    let row_created_at: DateTime<Utc> = row.get("created_at");
    let row_updated_at: DateTime<Utc> = row.get("updated_at");
    let expected_status = match record.status {
        NakamaResearchControlCommandStatusV2::Pending => "pending",
        NakamaResearchControlCommandStatusV2::Applied => "applied",
    };
    if record.schema != COMMAND_SCHEMA_V2
        || record.command_id != row_command_id
        || record.operation.as_str() != row_operation
        || record.target_rpc != row_target_rpc
        || record.target_rpc != record.operation.target_rpc()
        || record.idempotency_key != row_idempotency_key
        || record.request_hash != row_request_hash
        || record.session_id != row_session_id
        || i64::try_from(record.session_roster_version).ok() != Some(row_session_roster_version)
        || record.authorization_set_id != row_authorization_set_id
        || record.payload_hash != row_payload_hash
        || record.request_sha256 != row_request_sha256
        || expected_status != row_status
        || record.response_sha256 != row_response_sha256
        || record.response_seal_signature != row_response_seal_signature
        || i64::try_from(record.attempt_count).ok() != Some(row_attempt_count)
        || record.last_error_code != row_last_error_code
        || record.created_at != row_created_at
        || record.updated_at != row_updated_at
    {
        return Err(ApiError::internal(
            "normalized Nakama control columns do not match record_json",
        ));
    }
    let stored = StoredControlCommand {
        record,
        idempotency_key: row_idempotency_key,
        request_hash: row_request_hash,
        request_body: row.get("request_body"),
        response_body: row.get("response_body"),
    };
    ensure_request_hash(&stored, &stored.request_hash)?;
    if let (Some(response_body), Some(response_sha256)) = (
        stored.response_body.as_ref(),
        stored.record.response_sha256.as_ref(),
    ) {
        if sha256_digest(response_body) != *response_sha256 {
            return Err(ApiError::internal(
                "stored Nakama control response checksum verification failed",
            ));
        }
    }
    match stored.record.status {
        NakamaResearchControlCommandStatusV2::Pending
            if stored.response_body.is_none()
                && stored.record.response.is_none()
                && stored.record.response_sha256.is_none()
                && stored.record.response_seal_signature.is_none() => {}
        NakamaResearchControlCommandStatusV2::Applied
            if stored.response_body.is_some()
                && stored.record.response.is_some()
                && stored.record.response_sha256.is_some()
                && stored.record.response_seal_signature.is_some() => {}
        _ => {
            return Err(ApiError::internal(
                "stored Nakama control status and response fields are inconsistent",
            ));
        }
    }
    Ok(stored)
}

fn validate_stored_signed_request(
    state: &AppState,
    stored: &StoredControlCommand,
    set: &ResearchSessionAuthorizationSetV1,
) -> Result<(), ApiError> {
    let mut expected_authorizations = set
        .members
        .iter()
        .map(|member| member.authorization.clone())
        .collect::<Vec<_>>();
    expected_authorizations.sort_by_key(|authorization| authorization.claim.participant_slot);
    let (control, business) = match stored.record.operation {
        ResearchControlOperationV2::Create => {
            let request: ResearchControlCreateRequestV2 =
                serde_json::from_slice(&stored.request_body)
                    .map_err(|_| ApiError::internal("stored create control request is invalid"))?;
            if canonical_json_bytes(&request).map_err(|error| {
                ApiError::internal(format!(
                    "canonicalize stored create control request: {error}"
                ))
            })? != stored.request_body
                || request.authorization_set_id != set.authorization_set_id
                || request.authorizations != expected_authorizations
            {
                return Err(ApiError::internal(
                    "stored create control request differs from its authorization epoch",
                ));
            }
            let business = research_control_create_business_v2(&request).map_err(|message| {
                ApiError::internal(format!("verify create business frame: {message}"))
            })?;
            (request.control, business)
        }
        ResearchControlOperationV2::Resume => {
            let request: ResearchControlResumeRequestV2 =
                serde_json::from_slice(&stored.request_body)
                    .map_err(|_| ApiError::internal("stored resume control request is invalid"))?;
            if canonical_json_bytes(&request).map_err(|error| {
                ApiError::internal(format!(
                    "canonicalize stored resume control request: {error}"
                ))
            })? != stored.request_body
                || request.logical_session_id != set.session_id
                || request.authorization_set_id != set.authorization_set_id
            {
                return Err(ApiError::internal(
                    "stored resume control request differs from its authorization epoch",
                ));
            }
            let business = research_control_resume_business_v2(&request).map_err(|message| {
                ApiError::internal(format!("verify resume business frame: {message}"))
            })?;
            (request.control, business)
        }
        ResearchControlOperationV2::ReplaceRoster => {
            let request: ResearchControlReplaceRequestV2 =
                serde_json::from_slice(&stored.request_body).map_err(|_| {
                    ApiError::internal("stored replacement control request is invalid")
                })?;
            if canonical_json_bytes(&request).map_err(|error| {
                ApiError::internal(format!(
                    "canonicalize stored replacement control request: {error}"
                ))
            })? != stored.request_body
                || request.logical_session_id != set.session_id
                || request.authorization_set_id != set.authorization_set_id
                || request.authorizations != expected_authorizations
            {
                return Err(ApiError::internal(
                    "stored replacement control request differs from its authorization epoch",
                ));
            }
            let business = research_control_replace_business_v2(&request).map_err(|message| {
                ApiError::internal(format!("verify replacement business frame: {message}"))
            })?;
            (request.control, business)
        }
        ResearchControlOperationV2::Complete => {
            let request: ResearchControlCompleteRequestV2 =
                serde_json::from_slice(&stored.request_body).map_err(|_| {
                    ApiError::internal("stored completion control request is invalid")
                })?;
            if canonical_json_bytes(&request).map_err(|error| {
                ApiError::internal(format!(
                    "canonicalize stored completion control request: {error}"
                ))
            })? != stored.request_body
                || request.logical_session_id != set.session_id
                || request.authorization_set_id != set.authorization_set_id
            {
                return Err(ApiError::internal(
                    "stored completion control request differs from its authorization epoch",
                ));
            }
            let business = research_control_complete_business_v2(&request).map_err(|message| {
                ApiError::internal(format!("verify completion business frame: {message}"))
            })?;
            (request.control, business)
        }
    };
    let claim = &control.claim;
    let expected_payload_hash = sha256_digest(&business);
    if claim.command_id != stored.record.command_id
        || claim.operation != stored.record.operation
        || claim.target_rpc != stored.record.target_rpc
        || claim.session_id != stored.record.session_id
        || claim.session_roster_version != stored.record.session_roster_version
        || claim.authorization_set_id != stored.record.authorization_set_id
        || claim.payload_hash != stored.record.payload_hash
        || claim.payload_hash != expected_payload_hash
        || claim.issuer_key_id != state.security.nakama_control_issuer_key_id
    {
        return Err(ApiError::internal(
            "stored signed control claim differs from normalized command fields",
        ));
    }
    verify_research_control_v2(
        &control,
        &state.security.nakama_control_signing_key.verifying_key(),
    )
    .map_err(|message| ApiError::internal(format!("verify stored control signature: {message}")))
}

fn validate_control_epoch(
    state: &AppState,
    set: &ResearchSessionAuthorizationSetV1,
    operation: ResearchControlOperationV2,
) -> Result<(), ApiError> {
    if !(3..=5).contains(&set.members.len())
        || set.roster_version == 0
        || set.roster_version > JSON_SAFE_U64_MAX
        || recompute_roster_root(state, set)? != set.roster_root
    {
        return Err(ApiError::conflict(
            "invalid_control_authorization_epoch",
            "authorization epoch roster is not a canonical 3-5 member snapshot",
        ));
    }
    let issued_or_consumed = matches!(
        set.status,
        ResearchSessionAuthorizationSetStatus::Issued
            | ResearchSessionAuthorizationSetStatus::Consumed
    );
    match operation {
        ResearchControlOperationV2::Create if set.roster_version == 1 && issued_or_consumed => {}
        ResearchControlOperationV2::Resume if issued_or_consumed => {}
        ResearchControlOperationV2::ReplaceRoster
            if set.roster_version > 1
                && set.supersedes_roster_version == Some(set.roster_version - 1)
                && issued_or_consumed => {}
        ResearchControlOperationV2::Complete
            if set.status == ResearchSessionAuthorizationSetStatus::Consumed
                && set.consumed_at.is_some()
                && set
                    .members
                    .iter()
                    .all(|member| member.consumed_at.is_some()) => {}
        _ => {
            return Err(ApiError::conflict(
                "control_authorization_epoch_state_invalid",
                "authorization epoch cannot perform the requested Nakama control operation",
            ));
        }
    }
    Ok(())
}

fn recompute_roster_root(
    state: &AppState,
    set: &ResearchSessionAuthorizationSetV1,
) -> Result<String, ApiError> {
    let authorization_key = state
        .security
        .nakama_authorization_signing_key
        .verifying_key();
    let mut members = set.members.clone();
    members.sort_by_key(|member| member.authorization.claim.participant_slot);
    let mut roster = Vec::with_capacity(members.len());
    for (index, member) in members.iter().enumerate() {
        let claim = &member.authorization.claim;
        let expected_slot = u32::try_from(index + 1).expect("five members fit u32");
        if member.authorization.issuer_key_id != state.security.nakama_authorization_issuer_key_id
            || verify_research_session_authorization(&member.authorization, &authorization_key)
                .is_err()
            || claim.participant_slot != expected_slot
            || claim.session_id != set.session_id
            || claim.team_id != set.team_id.to_string()
            || claim.paper_project_id != set.paper_project_id.to_string()
            || claim.challenge_id != set.challenge_id.to_string()
            || claim.roster_version != set.roster_version
            || claim.roster_root != set.roster_root
        {
            return Err(ApiError::conflict(
                "control_authorization_signature_invalid",
                "stored authorization set signature or epoch binding is invalid",
            ));
        }
        roster.push(ResearchSessionRosterMemberV1 {
            participant_slot: claim.participant_slot,
            authorization_id: claim.authorization_id.clone(),
            subject_user_id: claim.subject_user_id.clone(),
            agent_id: claim.agent_id.clone(),
            agent_did: claim.agent_did.clone(),
            agent_key_id: claim.agent_key_id.clone(),
            agent_key_hash: sha256_digest(
                &base64::engine::general_purpose::STANDARD
                    .decode(&claim.agent_public_key)
                    .map_err(|_| {
                        ApiError::conflict(
                            "control_authorization_key_invalid",
                            "stored authorization Agent public key is invalid base64",
                        )
                    })?,
            ),
            role: claim.role.clone(),
        });
    }
    research_session_roster_root(
        &set.session_id,
        &set.team_id.to_string(),
        &set.paper_project_id.to_string(),
        set.roster_version,
        &roster,
    )
    .map_err(|message| ApiError::conflict("invalid_control_roster_root", message))
}

fn placeholder_control(
    operation: ResearchControlOperationV2,
    set: &ResearchSessionAuthorizationSetV1,
) -> SignedResearchControlV2 {
    SignedResearchControlV2 {
        claim: ResearchControlClaimV2 {
            schema: RESEARCH_CONTROL_CLAIM_V2.to_string(),
            command_id: Uuid::nil(),
            operation,
            target_rpc: operation.target_rpc().to_string(),
            session_id: set.session_id.clone(),
            session_roster_version: set.roster_version,
            authorization_set_id: set.authorization_set_id,
            payload_hash: "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
            audience: RESEARCH_CONTROL_AUDIENCE_V2.to_string(),
            issued_at_unix: 0,
            expires_at_unix: 1,
            issuer_key_id: "placeholder".to_string(),
        },
        signature: String::new(),
    }
}

fn build_control_command(
    state: &AppState,
    invocation: &ControlInvocation,
    set: &ResearchSessionAuthorizationSetV1,
    submission: Option<&JointPaperSubmission>,
) -> Result<StoredControlCommand, ApiError> {
    let authorizations = || {
        let mut members = set.members.clone();
        members.sort_by_key(|member| member.authorization.claim.participant_slot);
        members
            .into_iter()
            .map(|member| member.authorization)
            .collect::<Vec<_>>()
    };
    let placeholder = placeholder_control(invocation.operation, set);
    let (business, facts) = match invocation.operation {
        ResearchControlOperationV2::Create => (
            research_control_create_business_v2(&ResearchControlCreateRequestV2 {
                schema: RESEARCH_CONTROL_CREATE_REQUEST_V2.to_string(),
                authorization_set_id: set.authorization_set_id,
                authorizations: authorizations(),
                control: placeholder,
            }),
            None,
        ),
        ResearchControlOperationV2::Resume => (
            research_control_resume_business_v2(&ResearchControlResumeRequestV2 {
                schema: RESEARCH_CONTROL_RESUME_REQUEST_V2.to_string(),
                logical_session_id: set.session_id.clone(),
                authorization_set_id: set.authorization_set_id,
                control: placeholder,
            }),
            None,
        ),
        ResearchControlOperationV2::ReplaceRoster => (
            research_control_replace_business_v2(&ResearchControlReplaceRequestV2 {
                schema: RESEARCH_CONTROL_REPLACE_REQUEST_V2.to_string(),
                logical_session_id: set.session_id.clone(),
                authorization_set_id: set.authorization_set_id,
                authorizations: authorizations(),
                control: placeholder,
            }),
            None,
        ),
        ResearchControlOperationV2::Complete => {
            let submission = submission.ok_or_else(|| {
                ApiError::internal("completion control is missing its finalized PaperBundle")
            })?;
            let facts = ResearchSessionTerminalFactsV1 {
                result_code: "paper_bundle_ready".to_string(),
                paper_bundle_hash: submission.paper_bundle_hash.clone(),
                paper_release_candidate_hash: submission.release_candidate_hash.clone(),
                contribution_ledger_hash: submission
                    .paper_bundle
                    .release_candidate
                    .contribution_ledger_hash
                    .clone(),
            };
            (
                research_control_complete_business_v2(&ResearchControlCompleteRequestV2 {
                    schema: RESEARCH_CONTROL_COMPLETE_REQUEST_V2.to_string(),
                    logical_session_id: set.session_id.clone(),
                    authorization_set_id: set.authorization_set_id,
                    facts: facts.clone(),
                    control: placeholder,
                }),
                Some(facts),
            )
        }
    };
    let payload_hash = sha256_digest(
        &business
            .map_err(|message| ApiError::conflict("invalid_control_business_payload", message))?,
    );
    let now = command_storage_now();
    let claim = ResearchControlClaimV2 {
        schema: RESEARCH_CONTROL_CLAIM_V2.to_string(),
        command_id: Uuid::new_v4(),
        operation: invocation.operation,
        target_rpc: invocation.operation.target_rpc().to_string(),
        session_id: set.session_id.clone(),
        session_roster_version: set.roster_version,
        authorization_set_id: set.authorization_set_id,
        payload_hash: payload_hash.clone(),
        audience: RESEARCH_CONTROL_AUDIENCE_V2.to_string(),
        issued_at_unix: now.timestamp(),
        expires_at_unix: now.timestamp() + RESEARCH_CONTROL_MAXIMUM_LIFETIME_SECONDS,
        issuer_key_id: state.security.nakama_control_issuer_key_id.clone(),
    };
    let control = sign_research_control_v2(claim, &state.security.nakama_control_signing_key)
        .map_err(|message| ApiError::internal(format!("sign Nakama control command: {message}")))?;
    let request_body = match invocation.operation {
        ResearchControlOperationV2::Create => {
            canonical_json_bytes(&ResearchControlCreateRequestV2 {
                schema: RESEARCH_CONTROL_CREATE_REQUEST_V2.to_string(),
                authorization_set_id: set.authorization_set_id,
                authorizations: authorizations(),
                control,
            })
        }
        ResearchControlOperationV2::Resume => {
            canonical_json_bytes(&ResearchControlResumeRequestV2 {
                schema: RESEARCH_CONTROL_RESUME_REQUEST_V2.to_string(),
                logical_session_id: set.session_id.clone(),
                authorization_set_id: set.authorization_set_id,
                control,
            })
        }
        ResearchControlOperationV2::ReplaceRoster => {
            canonical_json_bytes(&ResearchControlReplaceRequestV2 {
                schema: RESEARCH_CONTROL_REPLACE_REQUEST_V2.to_string(),
                logical_session_id: set.session_id.clone(),
                authorization_set_id: set.authorization_set_id,
                authorizations: authorizations(),
                control,
            })
        }
        ResearchControlOperationV2::Complete => {
            canonical_json_bytes(&ResearchControlCompleteRequestV2 {
                schema: RESEARCH_CONTROL_COMPLETE_REQUEST_V2.to_string(),
                logical_session_id: set.session_id.clone(),
                authorization_set_id: set.authorization_set_id,
                facts: facts.expect("completion facts constructed"),
                control,
            })
        }
    }
    .map_err(|error| ApiError::internal(format!("encode signed control request: {error}")))?;
    let request_sha256 = sha256_digest(&request_body);
    let command_id = extract_command_id(&request_body, invocation.operation)?;
    let record = NakamaResearchControlCommandV2 {
        schema: COMMAND_SCHEMA_V2.to_string(),
        command_id,
        operation: invocation.operation,
        target_rpc: invocation.operation.target_rpc().to_string(),
        session_id: set.session_id.clone(),
        session_roster_version: set.roster_version,
        authorization_set_id: set.authorization_set_id,
        payload_hash,
        idempotency_key: invocation.idempotency_key.clone(),
        request_hash: invocation.request_hash.clone(),
        request_sha256,
        status: NakamaResearchControlCommandStatusV2::Pending,
        response: None,
        response_sha256: None,
        response_seal_signature: None,
        attempt_count: 0,
        last_error_code: None,
        created_at: now,
        updated_at: now,
    };
    Ok(StoredControlCommand {
        record,
        idempotency_key: invocation.idempotency_key.clone(),
        request_hash: invocation.request_hash.clone(),
        request_body,
        response_body: None,
    })
}

fn extract_command_id(
    request_body: &[u8],
    operation: ResearchControlOperationV2,
) -> Result<Uuid, ApiError> {
    let command_id = match operation {
        ResearchControlOperationV2::Create => {
            serde_json::from_slice::<ResearchControlCreateRequestV2>(request_body)
                .map(|request| request.control.claim.command_id)
        }
        ResearchControlOperationV2::Resume => {
            serde_json::from_slice::<ResearchControlResumeRequestV2>(request_body)
                .map(|request| request.control.claim.command_id)
        }
        ResearchControlOperationV2::ReplaceRoster => {
            serde_json::from_slice::<ResearchControlReplaceRequestV2>(request_body)
                .map(|request| request.control.claim.command_id)
        }
        ResearchControlOperationV2::Complete => {
            serde_json::from_slice::<ResearchControlCompleteRequestV2>(request_body)
                .map(|request| request.control.claim.command_id)
        }
    };
    command_id.map_err(|error| {
        ApiError::internal(format!("decode newly signed control request: {error}"))
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NakamaRpcEnvelope {
    payload: String,
}

async fn dispatch_control(
    state: &AppState,
    stored: &StoredControlCommand,
    set: &ResearchSessionAuthorizationSetV1,
) -> Result<(Value, Vec<u8>), ApiError> {
    let config = state
        .nakama_control_http
        .as_ref()
        .ok_or_else(|| ApiError::internal("signed Nakama control HTTP client is not configured"))?;
    let mut url = config.base_url.clone();
    url.set_path(&format!("/v2/rpc/{}", stored.record.target_rpc));
    url.query_pairs_mut()
        .append_pair("http_key", &config.runtime_http_key);
    let inner = String::from_utf8(stored.request_body.clone()).map_err(|_| {
        ApiError::internal("stored signed Nakama control request is not UTF-8 JSON")
    })?;
    let outer = serde_json::to_vec(&inner)
        .map_err(|error| ApiError::internal(format!("encode Nakama RPC envelope: {error}")))?;
    let response = config
        .client
        .post(url)
        .header(CONTENT_TYPE, "application/json")
        .body(outer)
        .send()
        .await
        .map_err(|_| {
            ApiError::bad_gateway(
                "nakama_control_transport_failed",
                "signed Nakama control RPC transport failed; retry reuses the exact stored command",
            )
        })?;
    if response.status() != StatusCode::OK {
        return Err(ApiError::bad_gateway(
            "nakama_control_http_status_invalid",
            format!(
                "signed Nakama control RPC returned HTTP {}",
                response.status().as_u16()
            ),
        ));
    }
    let content_types = response.headers().get_all(CONTENT_TYPE);
    if content_types.iter().count() != 1
        || content_types
            .iter()
            .next()
            .and_then(|value| value.to_str().ok())
            != Some("application/json")
    {
        return Err(ApiError::bad_gateway(
            "nakama_control_content_type_invalid",
            "signed Nakama control RPC must return exactly one application/json Content-Type",
        ));
    }
    let envelope_bytes = response.bytes().await.map_err(|_| {
        ApiError::bad_gateway(
            "nakama_control_response_read_failed",
            "signed Nakama control RPC response body could not be read",
        )
    })?;
    let envelope: NakamaRpcEnvelope = serde_json::from_slice(&envelope_bytes).map_err(|_| {
        ApiError::bad_gateway(
            "nakama_control_envelope_invalid",
            "signed Nakama control RPC returned a non-canonical response envelope",
        )
    })?;
    let response_body = envelope.payload.into_bytes();
    let response_value = validate_control_response(state, stored, set, &response_body).await?;
    Ok((response_value, response_body))
}

async fn validate_control_response(
    state: &AppState,
    stored: &StoredControlCommand,
    set: &ResearchSessionAuthorizationSetV1,
    response_body: &[u8],
) -> Result<Value, ApiError> {
    match stored.record.operation {
        ResearchControlOperationV2::Create
        | ResearchControlOperationV2::Resume
        | ResearchControlOperationV2::ReplaceRoster => {
            let response: ResearchControlResultV2<ResearchControlRuntimeResultV1> =
                serde_json::from_slice(response_body).map_err(|_| {
                    ApiError::bad_gateway(
                        "nakama_control_result_invalid",
                        "signed Nakama control runtime response violates its strict schema",
                    )
                })?;
            validate_wrapper(stored, &response)?;
            let result = &response.result;
            let allowed_status = matches!(
                result.status.as_str(),
                "created" | "waiting" | "ready" | "active" | "paused" | "completed"
            );
            if result.schema != "trnm.nakama.research-session.match-runtime.v1"
                || result.logical_session_id != stored.record.session_id
                || result.external_match_id.is_empty()
                || result.runtime_generation == 0
                || result.runtime_generation > JSON_SAFE_U64_MAX
                || result.session_version == 0
                || result.session_version > JSON_SAFE_U64_MAX
                || result.roster_version != stored.record.session_roster_version
                || result.roster_root != recompute_roster_root(state, set)?
                || !allowed_status
            {
                return Err(ApiError::bad_gateway(
                    "nakama_control_runtime_mismatch",
                    "Nakama runtime response differs from the locally authorized session epoch",
                ));
            }
            serde_json::to_value(response)
                .map_err(|error| ApiError::internal(format!("encode runtime result: {error}")))
        }
        ResearchControlOperationV2::Complete => {
            let response: ResearchControlResultV2<ResearchControlEvidenceResultV1> =
                serde_json::from_slice(response_body).map_err(|_| {
                    ApiError::bad_gateway(
                        "nakama_control_result_invalid",
                        "signed Nakama completion response violates its strict schema",
                    )
                })?;
            validate_wrapper(stored, &response)?;
            let result = &response.result;
            let completion_request: ResearchControlCompleteRequestV2 =
                serde_json::from_slice(&stored.request_body).map_err(|_| {
                    ApiError::internal("stored completion control request cannot be decoded")
                })?;
            let completion = &result.completion;
            if result.schema != "trnm.nakama.research-session.evidence.v1"
                || result.logical_session_id != stored.record.session_id
                || result.external_match_id.is_empty()
                || result.runtime_generation == 0
                || result.runtime_generation > JSON_SAFE_U64_MAX
                || completion.session_id != set.session_id
                || completion.team_id != set.team_id.to_string()
                || completion.paper_project_id != set.paper_project_id.to_string()
                || completion.challenge_id != set.challenge_id.to_string()
                || completion.roster_version != set.roster_version
                || completion.roster_root != recompute_roster_root(state, set)?
                || completion.terminal_facts != completion_request.facts
                || completion.event_count == 0
                || completion.event_count > JSON_SAFE_U64_MAX
            {
                return Err(ApiError::bad_gateway(
                    "nakama_control_evidence_mismatch",
                    "Nakama completion evidence differs from the finalized PaperBundle or roster",
                ));
            }
            state
                .security
                .verify_nakama_research_completion_signature(
                    completion,
                    &result.authority_public_key_base64,
                )
                .map_err(|message| {
                    ApiError::bad_gateway("nakama_control_evidence_signature_invalid", message)
                })?;
            ensure_completion_callback_persisted(state, completion).await?;
            serde_json::to_value(response)
                .map_err(|error| ApiError::internal(format!("encode evidence result: {error}")))
        }
    }
}

fn validate_wrapper<T>(
    stored: &StoredControlCommand,
    response: &ResearchControlResultV2<T>,
) -> Result<(), ApiError> {
    if response.schema != RESEARCH_CONTROL_RESULT_V2
        || response.command_id != stored.record.command_id
        || response.operation != stored.record.operation
        || response.target_rpc != stored.record.target_rpc
    {
        return Err(ApiError::bad_gateway(
            "nakama_control_wrapper_mismatch",
            "Nakama control response command, operation, or target does not match",
        ));
    }
    Ok(())
}

async fn ensure_completion_callback_persisted(
    state: &AppState,
    completion: &crate::paper_raid_contracts::ResearchSessionCompletionV1,
) -> Result<(), ApiError> {
    let receipt = if let Some(pool) = &state.pool {
        let row = sqlx::query(
            "select record_json from hepta_nakama_research_session_completions
             where session_id = $1 and roster_version = $2",
        )
        .bind(&completion.session_id)
        .bind(i64::try_from(completion.roster_version).map_err(|_| {
            ApiError::bad_gateway(
                "nakama_completion_callback_missing",
                "completion roster version does not fit PostgreSQL bigint",
            )
        })?)
        .fetch_optional(pool)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| {
            ApiError::bad_gateway(
                "nakama_completion_callback_missing",
                "Nakama returned completion before Hepta verified and persisted its archive callback",
            )
        })?;
        decode_record::<crate::paper_raid_contracts::SignedNakamaCompletionReceiptV1>(
            row.get("record_json"),
            "Nakama completion receipt",
        )?
    } else {
        state
            .paper_raid
            .read()
            .await
            .research_session_completions
            .get(&(completion.session_id.clone(), completion.roster_version))
            .cloned()
            .ok_or_else(|| {
                ApiError::bad_gateway(
                    "nakama_completion_callback_missing",
                    "Nakama returned completion before Hepta verified and persisted its archive callback",
                )
            })?
    };
    if receipt.commitment_id != completion.commitment_id
        || receipt.session_id != completion.session_id
        || receipt.roster_root != completion.roster_root
        || receipt.event_count != completion.event_count
        || receipt.event_root != completion.event_root
        || receipt.archive_hash != completion.archive_hash
        || receipt.terminal_facts != completion.terminal_facts
    {
        return Err(ApiError::bad_gateway(
            "nakama_completion_callback_mismatch",
            "persisted callback receipt differs from Nakama completion evidence",
        ));
    }
    if receipt.issuer_key_id != state.security.nakama_authorization_issuer_key_id {
        return Err(ApiError::bad_gateway(
            "nakama_completion_callback_issuer_mismatch",
            "persisted callback receipt issuer differs from the pinned Hepta issuer",
        ));
    }
    verify_nakama_completion_receipt(
        &receipt,
        &state
            .security
            .nakama_authorization_signing_key
            .verifying_key(),
    )
    .map_err(|message| {
        ApiError::bad_gateway("nakama_completion_callback_signature_invalid", message)
    })?;
    Ok(())
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct ControlResponseSealV1<'a> {
    schema: &'static str,
    command_id: Uuid,
    request_sha256: &'a str,
    response_sha256: &'a str,
}

fn control_response_seal_bytes(
    record: &NakamaResearchControlCommandV2,
    response_sha256: &str,
) -> Result<Vec<u8>, ApiError> {
    canonical_json_bytes(&ControlResponseSealV1 {
        schema: "hepta.paper_raid.nakama_control_response_seal.v1",
        command_id: record.command_id,
        request_sha256: &record.request_sha256,
        response_sha256,
    })
    .map_err(|error| ApiError::internal(format!("encode control response seal: {error}")))
}

fn sign_control_response_seal(
    state: &AppState,
    record: &NakamaResearchControlCommandV2,
    response_sha256: &str,
) -> Result<String, ApiError> {
    Ok(base64::engine::general_purpose::STANDARD.encode(
        state
            .security
            .nakama_control_signing_key
            .sign(&control_response_seal_bytes(record, response_sha256)?)
            .to_bytes(),
    ))
}

fn verify_control_response_seal(
    state: &AppState,
    stored: &StoredControlCommand,
) -> Result<(), ApiError> {
    let response_sha256 = stored.record.response_sha256.as_deref().ok_or_else(|| {
        ApiError::internal("applied Nakama control command has no response checksum")
    })?;
    let encoded = stored
        .record
        .response_seal_signature
        .as_deref()
        .ok_or_else(|| ApiError::internal("applied Nakama control command has no response seal"))?;
    let signature_bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| ApiError::internal("stored control response seal is invalid base64"))?;
    if base64::engine::general_purpose::STANDARD.encode(&signature_bytes) != encoded {
        return Err(ApiError::internal(
            "stored control response seal is not canonical padded base64",
        ));
    }
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| ApiError::internal("stored control response seal is not 64 bytes"))?;
    state
        .security
        .nakama_control_signing_key
        .verifying_key()
        .verify(
            &control_response_seal_bytes(&stored.record, response_sha256)?,
            &signature,
        )
        .map_err(|_| ApiError::internal("stored Nakama control response seal verification failed"))
}

async fn validate_applied_replay(
    state: &AppState,
    stored: StoredControlCommand,
    set: &ResearchSessionAuthorizationSetV1,
) -> Result<NakamaResearchControlCommandV2, ApiError> {
    validate_stored_signed_request(state, &stored, set)?;
    verify_control_response_seal(state, &stored)?;
    let response_body = stored
        .response_body
        .as_deref()
        .ok_or_else(|| ApiError::internal("applied Nakama control command has no response body"))?;
    let validated = validate_control_response(state, &stored, set, response_body).await?;
    let recorded = stored.record.response.as_ref().ok_or_else(|| {
        ApiError::internal("applied Nakama control command has no decoded response")
    })?;
    if canonical_json_bytes(&validated).map_err(|error| {
        ApiError::internal(format!("canonicalize validated control response: {error}"))
    })? != canonical_json_bytes(recorded).map_err(|error| {
        ApiError::internal(format!("canonicalize recorded control response: {error}"))
    })? {
        return Err(ApiError::internal(
            "stored Nakama response bytes do not match record_json response",
        ));
    }
    Ok(stored.record)
}

async fn persist_applied(
    state: &AppState,
    mut stored: StoredControlCommand,
    set: &ResearchSessionAuthorizationSetV1,
    response: Value,
    response_body: Vec<u8>,
) -> Result<NakamaResearchControlCommandV2, ApiError> {
    let now = command_storage_now();
    stored.record.status = NakamaResearchControlCommandStatusV2::Applied;
    stored.record.response = Some(response);
    let response_sha256 = sha256_digest(&response_body);
    stored.record.response_sha256 = Some(response_sha256.clone());
    stored.record.response_seal_signature = Some(sign_control_response_seal(
        state,
        &stored.record,
        &response_sha256,
    )?);
    stored.record.attempt_count = stored
        .record
        .attempt_count
        .checked_add(1)
        .filter(|value| *value <= JSON_SAFE_U64_MAX)
        .ok_or_else(|| ApiError::internal("Nakama control attempt counter overflow"))?;
    stored.record.last_error_code = None;
    stored.record.updated_at = now;
    stored.response_body = Some(response_body.clone());
    if state.pool.is_none() {
        let mut memory = state.paper_raid.write().await;
        let current = memory
            .nakama_control_commands
            .get(&stored.record.command_id)
            .cloned()
            .ok_or_else(|| ApiError::internal("prepared Nakama control command disappeared"))?;
        if current.record.status == NakamaResearchControlCommandStatusV2::Applied {
            drop(memory);
            return validate_applied_replay(state, current, set).await;
        }
        memory
            .nakama_control_commands
            .insert(stored.record.command_id, stored.clone());
        push_memory_event(
            &mut memory,
            stored.record.operation.as_str(),
            &stored.idempotency_key,
            "hepta.paper_raid.nakama_control.applied.v2",
            stored.record.command_id,
            2,
            serde_json::to_value(&stored.record)
                .map_err(|error| ApiError::internal(format!("encode control event: {error}")))?,
        )?;
        return Ok(stored.record);
    }
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::internal("PostgreSQL state is unavailable"))?;
    let mut tx = pool.begin().await.map_err(ApiError::database)?;
    let record_json = serde_json::to_value(&stored.record)
        .map_err(|error| ApiError::internal(format!("encode applied command: {error}")))?;
    let updated = sqlx::query(
        "update hepta_nakama_research_control_commands
         set status='applied', response_body=$1, response_sha256=$2,
             response_seal_signature=$3, attempt_count=$4, last_error_code=null,
             record_json=$5::jsonb, updated_at=$6
         where command_id=$7 and status='pending'",
    )
    .bind(&response_body)
    .bind(stored.record.response_sha256.as_deref())
    .bind(stored.record.response_seal_signature.as_deref())
    .bind(i64::try_from(stored.record.attempt_count).map_err(|_| {
        ApiError::internal("Nakama control attempt counter does not fit PostgreSQL bigint")
    })?)
    .bind(record_json)
    .bind(now)
    .bind(stored.record.command_id)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if updated.rows_affected() == 1 {
        insert_postgres_event(
            &mut tx,
            stored.record.operation.as_str(),
            &stored.idempotency_key,
            "hepta.paper_raid.nakama_control.applied.v2",
            stored.record.command_id,
            2,
            serde_json::to_value(&stored.record)
                .map_err(|error| ApiError::internal(format!("encode control event: {error}")))?,
        )
        .await?;
        tx.commit().await.map_err(ApiError::database)?;
        return Ok(stored.record);
    }
    tx.rollback().await.map_err(ApiError::database)?;
    let persisted = load_postgres_command(state, stored.record.command_id).await?;
    validate_applied_replay(state, persisted, set).await
}

async fn record_dispatch_failure(
    state: &AppState,
    stored: &StoredControlCommand,
    error_code: &'static str,
) -> Result<(), ApiError> {
    if state.pool.is_none() {
        let mut memory = state.paper_raid.write().await;
        if let Some(current) = memory
            .nakama_control_commands
            .get_mut(&stored.record.command_id)
        {
            if current.record.status == NakamaResearchControlCommandStatusV2::Pending {
                current.record.attempt_count = current
                    .record
                    .attempt_count
                    .checked_add(1)
                    .filter(|value| *value <= JSON_SAFE_U64_MAX)
                    .ok_or_else(|| ApiError::internal("Nakama control attempt counter overflow"))?;
                current.record.last_error_code = Some(error_code.to_string());
                current.record.updated_at = command_storage_now();
            }
        }
        return Ok(());
    }
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::internal("PostgreSQL state is unavailable"))?;
    let mut tx = pool.begin().await.map_err(ApiError::database)?;
    let row = sqlx::query(
        "select record_json from hepta_nakama_research_control_commands
         where command_id=$1 for update",
    )
    .bind(stored.record.command_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if let Some(row) = row {
        let mut record: NakamaResearchControlCommandV2 =
            decode_record(row.get("record_json"), "Nakama control command")?;
        if record.status == NakamaResearchControlCommandStatusV2::Pending {
            record.attempt_count = record
                .attempt_count
                .checked_add(1)
                .filter(|value| *value <= JSON_SAFE_U64_MAX)
                .ok_or_else(|| ApiError::internal("Nakama control attempt counter overflow"))?;
            record.last_error_code = Some(error_code.to_string());
            record.updated_at = command_storage_now();
            sqlx::query(
                "update hepta_nakama_research_control_commands
                 set attempt_count=$1, last_error_code=$2, record_json=$3::jsonb, updated_at=$4
                 where command_id=$5 and status='pending'",
            )
            .bind(i64::try_from(record.attempt_count).map_err(|_| {
                ApiError::internal("Nakama control attempt counter does not fit PostgreSQL bigint")
            })?)
            .bind(error_code)
            .bind(serde_json::to_value(&record).map_err(|error| {
                ApiError::internal(format!("encode failed control command: {error}"))
            })?)
            .bind(record.updated_at)
            .bind(record.command_id)
            .execute(&mut *tx)
            .await
            .map_err(ApiError::database)?;
        }
    }
    tx.commit().await.map_err(ApiError::database)?;
    Ok(())
}

async fn load_postgres_command(
    state: &AppState,
    command_id: Uuid,
) -> Result<StoredControlCommand, ApiError> {
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::internal("PostgreSQL state is unavailable"))?;
    let row = sqlx::query(
        "select command_id, operation, target_rpc, idempotency_key, request_hash,
                session_id, session_roster_version, authorization_set_id, payload_hash,
                request_body, request_sha256, status, response_body, response_sha256,
                response_seal_signature, attempt_count, last_error_code, created_at, updated_at,
                record_json
         from hepta_nakama_research_control_commands where command_id=$1",
    )
    .bind(command_id)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::internal("persisted Nakama control command disappeared"))?;
    decode_stored_command_row(&row)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{
        body::{Body, Bytes},
        http::{header::HeaderValue, Request, Response},
        routing::any,
    };
    use chrono::Duration;
    use ed25519_dalek::SigningKey;
    use sqlx::{Connection, Row};
    use tokio::sync::Mutex;
    use tower::ServiceExt;

    use super::*;
    use crate::{
        app,
        paper_raid_contracts::{
            canonical_json_sha256, sign_consumer_user_assertion,
            sign_research_session_authorization, ConsumerUserAssertionClaimV2,
            ResearchSessionAuthorizationClaimV1, CONSUMER_USER_ASSERTION_V2,
            RESEARCH_SESSION_AUTHORIZATION_V1,
        },
        SecurityConfig, USER_ASSERTION_HEADER,
    };

    #[test]
    fn command_status_strings_are_frozen() {
        assert_eq!(
            serde_json::to_string(&NakamaResearchControlCommandStatusV2::Pending).unwrap(),
            "\"pending\""
        );
        assert_eq!(
            serde_json::to_string(&NakamaResearchControlCommandStatusV2::Applied).unwrap(),
            "\"applied\""
        );
    }

    #[test]
    fn strict_endpoint_requests_reject_unknown_members() {
        let request = serde_json::json!({
            "authorization_set_id": Uuid::nil(),
            "idempotency_key": "control-1",
            "operator_token": "forbidden"
        });
        assert!(
            serde_json::from_value::<CreateNakamaResearchSessionControlRequestV2>(request).is_err()
        );
    }

    fn signed_set(state: &AppState) -> ResearchSessionAuthorizationSetV1 {
        let session_id = "paper-raid-control-test".to_string();
        let team_id = Uuid::from_u128(0x101);
        let paper_project_id = Uuid::from_u128(0x202);
        let challenge_id = Uuid::from_u128(0x303);
        let authorization_set_id = Uuid::from_u128(0x404);
        let issued_at = Utc::now();
        let expires_at = issued_at + Duration::minutes(5);
        let mut roster = Vec::new();
        let mut member_inputs = Vec::new();
        for slot in 1_u32..=3 {
            let agent_key = SigningKey::from_bytes(&[u8::try_from(slot).unwrap(); 32]);
            let public_key = agent_key.verifying_key().to_bytes();
            let authorization_id = Uuid::from_u128(0x500 + u128::from(slot));
            let subject_user_id = Uuid::from_u128(0x600 + u128::from(slot));
            let agent_key_hash = sha256_digest(&public_key);
            roster.push(ResearchSessionRosterMemberV1 {
                participant_slot: slot,
                authorization_id: authorization_id.to_string(),
                subject_user_id: subject_user_id.to_string(),
                agent_id: format!("agent-{slot}"),
                agent_did: format!("did:trnm:agent-{slot}"),
                agent_key_id: agent_key_hash.clone(),
                agent_key_hash: agent_key_hash.clone(),
                role: format!("role-{slot}"),
            });
            member_inputs.push((
                authorization_id,
                subject_user_id,
                public_key,
                agent_key_hash,
            ));
        }
        let roster_root = research_session_roster_root(
            &session_id,
            &team_id.to_string(),
            &paper_project_id.to_string(),
            1,
            &roster,
        )
        .unwrap();
        let ruleset_hash = sha256_digest(b"ruleset");
        let challenge_snapshot_hash = sha256_digest(b"challenge");
        let members = member_inputs
            .into_iter()
            .enumerate()
            .map(
                |(index, (authorization_id, subject_user_id, public_key, agent_key_hash))| {
                    let slot = u32::try_from(index + 1).unwrap();
                    let authorization = sign_research_session_authorization(
                        ResearchSessionAuthorizationClaimV1 {
                            schema: RESEARCH_SESSION_AUTHORIZATION_V1.to_string(),
                            authorization_id: authorization_id.to_string(),
                            session_id: session_id.clone(),
                            team_id: team_id.to_string(),
                            paper_project_id: paper_project_id.to_string(),
                            challenge_id: challenge_id.to_string(),
                            agent_id: format!("agent-{slot}"),
                            agent_did: format!("did:trnm:agent-{slot}"),
                            agent_key_id: agent_key_hash,
                            agent_public_key: base64::engine::general_purpose::STANDARD
                                .encode(public_key),
                            subject_user_id: subject_user_id.to_string(),
                            participant_slot: slot,
                            role: format!("role-{slot}"),
                            roster_version: 1,
                            roster_root: roster_root.clone(),
                            ruleset_hash: ruleset_hash.clone(),
                            challenge_snapshot_hash: challenge_snapshot_hash.clone(),
                            issued_at_unix: issued_at.timestamp(),
                            expires_at_unix: expires_at.timestamp(),
                        },
                        &state.security.nakama_authorization_issuer_key_id,
                        &state.security.nakama_authorization_signing_key,
                    )
                    .unwrap();
                    super::super::ResearchSessionAuthorizationMemberV1 {
                        player_id: Uuid::from_u128(0x700 + u128::from(slot)),
                        binding_id: Uuid::from_u128(0x800 + u128::from(slot)),
                        authorization,
                        consumed_at: None,
                    }
                },
            )
            .collect();
        ResearchSessionAuthorizationSetV1 {
            schema: "hepta.paper_raid.research_session_authorization_set.v1".to_string(),
            authorization_set_id,
            session_id,
            team_id,
            paper_project_id,
            challenge_id,
            team_roster_version: 1,
            roster_version: 1,
            roster_root,
            supersedes_roster_version: None,
            replaced_participant_slot: None,
            status: ResearchSessionAuthorizationSetStatus::Issued,
            members,
            version: 1,
            issued_at,
            expires_at,
            consumed_at: None,
        }
    }

    fn create_stored(
        state: &AppState,
        set: &ResearchSessionAuthorizationSetV1,
    ) -> StoredControlCommand {
        let invocation = ControlInvocation {
            operation: ResearchControlOperationV2::Create,
            canonical_path: "/v2/hepta/nakama/research-session-controls/create",
            idempotency_key: "control-create-test".to_string(),
            request_hash: sha256_digest(b"user-request"),
            locator: OwnedControlLocator::AuthorizationSet(set.authorization_set_id),
        };
        validate_control_epoch(state, set, invocation.operation).unwrap();
        build_control_command(state, &invocation, set, None).unwrap()
    }

    async fn mock_rpc(
        payload: String,
        duplicate_content_type: bool,
    ) -> (String, Arc<Mutex<Vec<Vec<u8>>>>) {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let captured_handler = captured.clone();
        let app = Router::new().fallback(any(move |body: Bytes| {
            let payload = payload.clone();
            let captured = captured_handler.clone();
            async move {
                captured.lock().await.push(body.to_vec());
                let envelope =
                    serde_json::to_vec(&serde_json::json!({"payload": payload})).unwrap();
                let mut response = Response::new(Body::from(envelope));
                response
                    .headers_mut()
                    .append(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                if duplicate_content_type {
                    response
                        .headers_mut()
                        .append(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }
                response
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{address}/"), captured)
    }

    #[tokio::test]
    async fn rpc_transport_replays_exact_stored_bytes_and_rejects_duplicate_content_type() {
        let base_state = AppState::new(SecurityConfig::new("operator", "nakama"));
        let set = signed_set(&base_state);
        let stored = create_stored(&base_state, &set);
        let runtime = ResearchControlResultV2 {
            schema: RESEARCH_CONTROL_RESULT_V2.to_string(),
            command_id: stored.record.command_id,
            operation: stored.record.operation,
            target_rpc: stored.record.target_rpc.clone(),
            result: ResearchControlRuntimeResultV1 {
                schema: "trnm.nakama.research-session.match-runtime.v1".to_string(),
                logical_session_id: set.session_id.clone(),
                external_match_id: "external-match.test".to_string(),
                runtime_generation: 1,
                status: "created".to_string(),
                session_version: 1,
                roster_version: set.roster_version,
                roster_root: set.roster_root.clone(),
            },
        };
        let payload = String::from_utf8(canonical_json_bytes(&runtime).unwrap()).unwrap();
        let (base_url, captured) = mock_rpc(payload.clone(), false).await;
        let state = base_state
            .clone()
            .with_nakama_control_http(&base_url, "runtime-http-key")
            .unwrap();
        dispatch_control(&state, &stored, &set).await.unwrap();
        dispatch_control(&state, &stored, &set).await.unwrap();
        let bodies = captured.lock().await.clone();
        assert_eq!(bodies.len(), 2);
        assert_eq!(bodies[0], bodies[1]);
        let inner: String = serde_json::from_slice(&bodies[0]).unwrap();
        assert_eq!(inner.as_bytes(), stored.request_body);

        let (base_url, _) = mock_rpc(payload, true).await;
        let state = base_state
            .with_nakama_control_http(&base_url, "runtime-http-key")
            .unwrap();
        let error = dispatch_control(&state, &stored, &set).await.unwrap_err();
        assert_eq!(error.code, "nakama_control_content_type_invalid");
    }

    #[tokio::test]
    async fn runtime_response_is_bound_to_local_roster_root() {
        let state = AppState::new(SecurityConfig::new("operator", "nakama"));
        let set = signed_set(&state);
        let stored = create_stored(&state, &set);
        let tampered = ResearchControlResultV2 {
            schema: RESEARCH_CONTROL_RESULT_V2.to_string(),
            command_id: stored.record.command_id,
            operation: stored.record.operation,
            target_rpc: stored.record.target_rpc.clone(),
            result: ResearchControlRuntimeResultV1 {
                schema: "trnm.nakama.research-session.match-runtime.v1".to_string(),
                logical_session_id: set.session_id.clone(),
                external_match_id: "external-match.test".to_string(),
                runtime_generation: 1,
                status: "created".to_string(),
                session_version: 1,
                roster_version: set.roster_version,
                roster_root: sha256_digest(b"tampered"),
            },
        };
        let body = canonical_json_bytes(&tampered).unwrap();
        let error = validate_control_response(&state, &stored, &set, &body)
            .await
            .unwrap_err();
        assert_eq!(error.code, "nakama_control_runtime_mismatch");
    }

    #[tokio::test]
    async fn applied_response_replay_rejects_tampering_after_checksum_recalculation() {
        let state = AppState::new(SecurityConfig::new("operator", "nakama"));
        let set = signed_set(&state);
        let stored = create_stored(&state, &set);
        state
            .paper_raid
            .write()
            .await
            .nakama_control_commands
            .insert(stored.record.command_id, stored.clone());
        let response = ResearchControlResultV2 {
            schema: RESEARCH_CONTROL_RESULT_V2.to_string(),
            command_id: stored.record.command_id,
            operation: stored.record.operation,
            target_rpc: stored.record.target_rpc.clone(),
            result: ResearchControlRuntimeResultV1 {
                schema: "trnm.nakama.research-session.match-runtime.v1".to_string(),
                logical_session_id: set.session_id.clone(),
                external_match_id: "external-match.test".to_string(),
                runtime_generation: 1,
                status: "created".to_string(),
                session_version: 1,
                roster_version: set.roster_version,
                roster_root: set.roster_root.clone(),
            },
        };
        let response_body = canonical_json_bytes(&response).unwrap();
        persist_applied(
            &state,
            stored,
            &set,
            serde_json::to_value(&response).unwrap(),
            response_body,
        )
        .await
        .unwrap();

        let mut tampered = state
            .paper_raid
            .read()
            .await
            .nakama_control_commands
            .values()
            .next()
            .unwrap()
            .clone();
        let mut tampered_response = response;
        tampered_response.result.roster_root = sha256_digest(b"database-tampering");
        let tampered_body = canonical_json_bytes(&tampered_response).unwrap();
        tampered.record.response = Some(serde_json::to_value(&tampered_response).unwrap());
        tampered.record.response_sha256 = Some(sha256_digest(&tampered_body));
        tampered.response_body = Some(tampered_body);

        let error = validate_applied_replay(&state, tampered, &set)
            .await
            .unwrap_err();
        assert_eq!(error.code, "internal_error");
        assert!(error.message.contains("response seal verification failed"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn postgres_applied_response_rejects_tampering_with_recalculated_checksum() {
        let Ok(database_url) = std::env::var("HEPTA_TEST_DATABASE_URL") else {
            eprintln!("HEPTA_TEST_DATABASE_URL unset; control response tamper test skipped");
            return;
        };
        let mut lock = sqlx::PgConnection::connect(&database_url)
            .await
            .expect("PostgreSQL tamper-test lock connection");
        sqlx::query("select pg_advisory_lock(hashtext('hepta-research-league-pg-tests'))")
            .execute(&mut lock)
            .await
            .expect("serialize Hepta PostgreSQL tests");

        let state = AppState::connect(&database_url, super::super::endpoint_tests::security())
            .await
            .expect("PostgreSQL tamper-test state");
        super::super::endpoint_tests::reset_postgres(&database_url).await;
        super::super::endpoint_tests::seed_three_member_postgres_flow_for_control_test(
            state.clone(),
        )
        .await;
        let pool = state.pool.as_ref().expect("PostgreSQL pool");
        let set_row = sqlx::query(
            "select record_json from hepta_research_session_authorization_sets
             where session_id='paper-raid-3-session' and roster_version=1",
        )
        .fetch_one(pool)
        .await
        .expect("three-member authorization epoch");
        let set: ResearchSessionAuthorizationSetV1 = decode_record(
            set_row.get("record_json"),
            "research session authorization set",
        )
        .expect("decode authorization epoch");
        let invocation = ControlInvocation {
            operation: ResearchControlOperationV2::Create,
            canonical_path: "/v2/hepta/nakama/research-session-controls/create",
            idempotency_key: "postgres-control-tamper".to_string(),
            request_hash: sha256_digest(b"postgres-control-tamper-request"),
            locator: OwnedControlLocator::AuthorizationSet(set.authorization_set_id),
        };
        let stored = build_control_command(&state, &invocation, &set, None)
            .expect("build signed PostgreSQL command");
        let record_json = serde_json::to_value(&stored.record).expect("encode pending command");
        sqlx::query(
            "insert into hepta_nakama_research_control_commands (
                command_id, operation, target_rpc, idempotency_key, request_hash,
                session_id, session_roster_version, authorization_set_id, payload_hash,
                request_body, request_sha256, status, response_body, response_sha256,
                response_seal_signature, attempt_count, last_error_code, record_json,
                created_at, updated_at
             ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'pending',null,null,null,0,null,$12::jsonb,$13,$13)",
        )
        .bind(stored.record.command_id)
        .bind(stored.record.operation.as_str())
        .bind(&stored.record.target_rpc)
        .bind(&stored.idempotency_key)
        .bind(&stored.request_hash)
        .bind(&stored.record.session_id)
        .bind(i64::try_from(stored.record.session_roster_version).unwrap())
        .bind(stored.record.authorization_set_id)
        .bind(&stored.record.payload_hash)
        .bind(&stored.request_body)
        .bind(&stored.record.request_sha256)
        .bind(record_json)
        .bind(stored.record.created_at)
        .execute(pool)
        .await
        .expect("persist pending PostgreSQL command");

        let response = ResearchControlResultV2 {
            schema: RESEARCH_CONTROL_RESULT_V2.to_string(),
            command_id: stored.record.command_id,
            operation: stored.record.operation,
            target_rpc: stored.record.target_rpc.clone(),
            result: ResearchControlRuntimeResultV1 {
                schema: "trnm.nakama.research-session.match-runtime.v1".to_string(),
                logical_session_id: set.session_id.clone(),
                external_match_id: "external-match.postgres-test".to_string(),
                runtime_generation: 1,
                status: "created".to_string(),
                session_version: 1,
                roster_version: set.roster_version,
                roster_root: set.roster_root.clone(),
            },
        };
        persist_applied(
            &state,
            stored.clone(),
            &set,
            serde_json::to_value(&response).unwrap(),
            canonical_json_bytes(&response).unwrap(),
        )
        .await
        .expect("persist valid applied response");

        let mut tampered_response = response;
        tampered_response.result.roster_root = sha256_digest(b"postgres-database-tampering");
        let tampered_body = canonical_json_bytes(&tampered_response).unwrap();
        let tampered_sha256 = sha256_digest(&tampered_body);
        let row = sqlx::query(
            "select record_json from hepta_nakama_research_control_commands where command_id=$1",
        )
        .bind(stored.record.command_id)
        .fetch_one(pool)
        .await
        .expect("load applied command record");
        let mut tampered_record: Value = row.get("record_json");
        tampered_record["response"] = serde_json::to_value(&tampered_response).unwrap();
        tampered_record["response_sha256"] = Value::String(tampered_sha256.clone());
        sqlx::query(
            "update hepta_nakama_research_control_commands
             set response_body=$1, response_sha256=$2, record_json=$3::jsonb
             where command_id=$4",
        )
        .bind(&tampered_body)
        .bind(&tampered_sha256)
        .bind(tampered_record)
        .bind(stored.record.command_id)
        .execute(pool)
        .await
        .expect("tamper applied response while recalculating checksum");

        let tampered = load_postgres_command(&state, stored.record.command_id)
            .await
            .expect("normalized row remains internally consistent after tampering");
        let error = validate_applied_replay(&state, tampered, &set)
            .await
            .unwrap_err();
        assert_eq!(error.code, "internal_error");
        assert!(error.message.contains("response seal verification failed"));

        super::super::endpoint_tests::reset_postgres(&database_url).await;
        sqlx::query("select pg_advisory_unlock(hashtext('hepta-research-league-pg-tests'))")
            .execute(&mut lock)
            .await
            .expect("unlock Hepta PostgreSQL tests");
    }

    #[tokio::test]
    async fn completion_callback_receipt_requires_pinned_issuer_signature() {
        let state = AppState::new(SecurityConfig::new("operator", "nakama"));
        let set = signed_set(&state);
        let event_root = sha256_digest(b"receipt-event-root");
        let archive_hash = sha256_digest(b"receipt-archive-hash");
        let completion = crate::paper_raid_contracts::ResearchSessionCompletionV1 {
            schema: crate::paper_raid_contracts::RESEARCH_SESSION_COMPLETION_V1.to_string(),
            commitment_id: crate::paper_raid_contracts::research_session_commitment_id(
                &set.session_id,
                &event_root,
                &archive_hash,
            )
            .unwrap(),
            session_id: set.session_id.clone(),
            team_id: set.team_id.to_string(),
            paper_project_id: set.paper_project_id.to_string(),
            challenge_id: set.challenge_id.to_string(),
            roster_version: set.roster_version,
            roster_root: set.roster_root.clone(),
            terminal_facts: ResearchSessionTerminalFactsV1 {
                result_code: "paper_bundle_ready".to_string(),
                paper_bundle_hash: sha256_digest(b"receipt-paper-bundle"),
                paper_release_candidate_hash: sha256_digest(b"receipt-release"),
                contribution_ledger_hash: sha256_digest(b"receipt-ledger"),
            },
            event_count: 1,
            event_root,
            archive_hash,
            ruleset_hash: set.members[0].authorization.claim.ruleset_hash.clone(),
            challenge_snapshot_hash: set.members[0]
                .authorization
                .claim
                .challenge_snapshot_hash
                .clone(),
            completed_at_unix: Utc::now().timestamp(),
            authority_key_id: "nakama-test-completion-authority".to_string(),
            signature: String::new(),
        };
        let receipt = super::super::completion_receipt(
            &completion,
            set.team_id,
            set.paper_project_id,
            set.challenge_id,
            Utc::now(),
            &state.security.nakama_authorization_issuer_key_id,
            &state.security.nakama_authorization_signing_key,
        )
        .unwrap();
        state
            .paper_raid
            .write()
            .await
            .research_session_completions
            .insert(
                (completion.session_id.clone(), completion.roster_version),
                receipt.clone(),
            );
        ensure_completion_callback_persisted(&state, &completion)
            .await
            .unwrap();

        let mut forged = receipt;
        forged.signature = base64::engine::general_purpose::STANDARD.encode([0_u8; 64]);
        state
            .paper_raid
            .write()
            .await
            .research_session_completions
            .insert(
                (completion.session_id.clone(), completion.roster_version),
                forged,
            );
        let error = ensure_completion_callback_persisted(&state, &completion)
            .await
            .unwrap_err();
        assert_eq!(error.code, "nakama_completion_callback_signature_invalid");
    }

    async fn seed_memory_actor(state: &AppState, set: &ResearchSessionAuthorizationSetV1) {
        let now = Utc::now();
        let members = set
            .members
            .iter()
            .map(|member| super::super::TeamMember {
                participant_slot: member.authorization.claim.participant_slot,
                player_id: member.player_id,
                binding_id: member.binding_id,
                agent_id: member.authorization.claim.agent_id.clone(),
                role: member.authorization.claim.role.clone(),
                joined_at: now,
            })
            .collect::<Vec<_>>();
        let mut memory = state.paper_raid.write().await;
        for member in &set.members {
            let claim = &member.authorization.claim;
            let player = super::super::HumanPlayer {
                player_id: member.player_id,
                subject_id: format!("oidc|control-{}", member.player_id),
                nakama_user_id: Uuid::parse_str(&claim.subject_user_id).unwrap(),
                display_name: format!("Control {}", claim.participant_slot),
                signing_key_id: format!("human-key-{}", claim.participant_slot),
                signing_public_key: base64::engine::general_purpose::STANDARD.encode(
                    SigningKey::from_bytes(
                        &[0x40 + u8::try_from(claim.participant_slot).unwrap(); 32],
                    )
                    .verifying_key()
                    .to_bytes(),
                ),
                signing_public_key_hash: sha256_digest(
                    &SigningKey::from_bytes(
                        &[0x40 + u8::try_from(claim.participant_slot).unwrap(); 32],
                    )
                    .verifying_key()
                    .to_bytes(),
                ),
                status: super::super::HumanPlayerStatus::Active,
                version: 1,
                created_at: now,
                updated_at: now,
            };
            memory.players.insert(player.player_id, player);
        }
        memory.teams.insert(
            set.team_id,
            super::super::ResearchTeam {
                team_id: set.team_id,
                challenge_id: set.challenge_id,
                collaboration_compact_hash: sha256_digest(b"compact"),
                status: super::super::TeamStatus::Locked,
                roster_version: set.team_roster_version,
                members,
                version: 1,
                created_at: now,
                updated_at: now,
            },
        );
        memory
            .research_session_authorization_sets
            .insert((set.session_id.clone(), set.roster_version), set.clone());
    }

    fn user_assertion(
        set: &ResearchSessionAuthorizationSetV1,
        request: &CreateNakamaResearchSessionControlRequestV2,
    ) -> String {
        let first = &set.members[0];
        let now = Utc::now().timestamp();
        let assertion = sign_consumer_user_assertion(
            ConsumerUserAssertionClaimV2 {
                schema: CONSUMER_USER_ASSERTION_V2.to_string(),
                assertion_id: Uuid::new_v4(),
                issuer: "hepta-test-consumer-edge".to_string(),
                audience: "hepta-paper-raid-v2".to_string(),
                subject_id: format!("oidc|control-{}", first.player_id),
                nakama_user_id: Uuid::parse_str(&first.authorization.claim.subject_user_id)
                    .unwrap(),
                player_id: first.player_id,
                operation: "create_nakama_research_session_control_v2".to_string(),
                http_method: "POST".to_string(),
                canonical_path: "/v2/hepta/nakama/research-session-controls/create".to_string(),
                idempotency_key: request.idempotency_key.clone(),
                body_hash: canonical_json_sha256(request).unwrap(),
                issued_at_unix: now - 1,
                expires_at_unix: now + 120,
                nonce: request.idempotency_key.clone(),
            },
            "hepta-test-consumer-edge-key-v2",
            &SigningKey::from_bytes(&[0x6c; 32]),
        )
        .unwrap();
        base64::engine::general_purpose::STANDARD.encode(canonical_json_bytes(&assertion).unwrap())
    }

    async fn post_create_control(
        router: &Router,
        request: &CreateNakamaResearchSessionControlRequestV2,
        assertion: &str,
    ) -> StatusCode {
        router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v2/hepta/nakama/research-session-controls/create")
                    .header(CONTENT_TYPE, "application/json")
                    .header(USER_ASSERTION_HEADER, assertion)
                    .body(Body::from(serde_json::to_vec(request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn transport_failure_keeps_one_pending_command_and_retries_original_signature() {
        let base = AppState::new(SecurityConfig::new("operator", "nakama"));
        let set = signed_set(&base);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let unreachable = listener.local_addr().unwrap();
        drop(listener);
        let state = base
            .with_nakama_control_http(&format!("http://{unreachable}/"), "runtime-http-key")
            .unwrap();
        seed_memory_actor(&state, &set).await;
        let request = CreateNakamaResearchSessionControlRequestV2 {
            authorization_set_id: set.authorization_set_id,
            idempotency_key: "pending-exact-retry".to_string(),
        };
        let assertion = user_assertion(&set, &request);
        let router = app(state.clone());
        assert_eq!(
            post_create_control(&router, &request, &assertion).await,
            StatusCode::BAD_GATEWAY
        );
        let first = {
            let memory = state.paper_raid.read().await;
            assert_eq!(memory.nakama_control_commands.len(), 1);
            memory
                .nakama_control_commands
                .values()
                .next()
                .unwrap()
                .clone()
        };
        assert_eq!(
            post_create_control(&router, &request, &assertion).await,
            StatusCode::BAD_GATEWAY
        );
        let memory = state.paper_raid.read().await;
        assert_eq!(memory.nakama_control_commands.len(), 1);
        let retried = memory.nakama_control_commands.values().next().unwrap();
        assert_eq!(retried.record.command_id, first.record.command_id);
        assert_eq!(retried.request_body, first.request_body);
        assert_eq!(retried.record.request_sha256, first.record.request_sha256);
        assert_eq!(retried.record.attempt_count, 2);
        assert_eq!(
            retried.record.status,
            NakamaResearchControlCommandStatusV2::Pending
        );
    }
}
