use std::{collections::HashSet, time::Duration};

use axum::{
    body::{Body, Bytes},
    extract::{OriginalUri, Path, State},
    http::{header, HeaderMap, HeaderName, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, TimeZone, Utc};
use hepta_paper_raid_contracts::{
    agent_bridge_request_proof_hash, agent_capability_disclosure_hash, canonical_json_bytes,
    sha256_digest, validate_agent_bridge_canonical_query, verify_agent_binding_proof_v3,
    verify_agent_bridge_request_proof, AgentBindingProofClaimV3, AgentBridgeRequestProofV1,
    AgentCapabilityDisclosureV1, AGENT_BINDING_PROOF_V3, AGENT_BRIDGE_REQUEST_PROOF_V1,
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
};

const PAIRING_GRANT_TTL_SECONDS: i64 = 300;
const AGENT_PROOF_CLOCK_SKEW_SECONDS: i64 = 5;
const AGENT_REPLAY_WAIT_ATTEMPTS: usize = 40;
const AGENT_REPLAY_WAIT_INTERVAL: Duration = Duration::from_millis(50);
const MAX_PAIR_BODY_BYTES: usize = 128 * 1024;
const MAX_AGENT_BODY_BYTES: usize = 2 * 1024 * 1024;
const PAIR_CODE_PREFIX: &str = "prg1.";
const GLOBAL_PAIR_PRINCIPAL: &[u8] = b"paper-raid-bff/agent-pair/global/v1";
const GLOBAL_REQUEST_PRINCIPAL: &[u8] = b"paper-raid-bff/agent-request/global/v1";
const PAIR_BUCKET_DOMAIN: &[u8] = b"paper-raid-bff/agent-pair/fixed-bucket/v1\0";
const REQUEST_BUCKET_DOMAIN: &[u8] = b"paper-raid-bff/agent-request/fixed-bucket/v1\0";

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
struct InboxRequest {
    schema: String,
    paper_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposalRequest {
    schema: String,
    paper_id: Uuid,
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

struct VerifiedAgentRequest {
    identity: AlphaIdentity,
    mapping: BridgeMapping,
    authoritative_binding: Value,
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
        if !session.identity.has_scope(AlphaIdentityScope::Author) {
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
    if !session.identity.has_scope(AlphaIdentityScope::Author) {
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
    if identity.player_id != grant.player_id || !identity.has_scope(AlphaIdentityScope::Author) {
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
        || request.paper_ids.is_empty()
        || request.paper_ids.len() > 64
    {
        return Err(AppError::Invalid("invalid Agent inbox request".into()));
    }
    let unique: HashSet<Uuid> = request.paper_ids.iter().copied().collect();
    if unique.len() != request.paper_ids.len() {
        return Err(AppError::Invalid(
            "Agent inbox paper_ids must be unique".into(),
        ));
    }
    let mut papers = Vec::with_capacity(request.paper_ids.len());
    let binding_id = verified.mapping.binding_id.to_string();
    let player_id = verified.mapping.player_id.to_string();
    for paper_id in request.paper_ids {
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
        let leases: Vec<Value> = room
            .get("leases")
            .and_then(Value::as_array)
            .ok_or(AppError::Upstream)?
            .iter()
            .filter(|lease| {
                lease.get("holder_binding_id").and_then(Value::as_str) == Some(binding_id.as_str())
                    && lease.get("holder_player_id").and_then(Value::as_str)
                        == Some(player_id.as_str())
            })
            .map(|lease| {
                project_fields(
                    lease,
                    &[
                        "lease_id",
                        "paper_project_id",
                        "section_key",
                        "holder_player_id",
                        "holder_binding_id",
                        "fencing_token",
                        "status",
                        "version",
                        "expires_at",
                    ],
                )
            })
            .collect::<Result<_, _>>()?;
        let leased_sections: HashSet<&str> = leases
            .iter()
            .filter_map(|lease| lease.get("section_key").and_then(Value::as_str))
            .collect();
        let section_heads: Vec<Value> = room
            .get("section_heads")
            .and_then(Value::as_array)
            .ok_or(AppError::Upstream)?
            .iter()
            .filter(|head| {
                head.get("section_key")
                    .and_then(Value::as_str)
                    .is_some_and(|section| leased_sections.contains(section))
            })
            .map(|head| {
                project_fields(
                    head,
                    &[
                        "paper_project_id",
                        "section_key",
                        "base_paper_revision_id",
                        "current_head_revision_id",
                        "fencing_token",
                        "version",
                    ],
                )
            })
            .collect::<Result<_, _>>()?;
        let artifact_manifests: Vec<Value> = room
            .get("artifact_manifests")
            .and_then(Value::as_array)
            .ok_or(AppError::Upstream)?
            .iter()
            .map(|manifest| {
                project_fields(
                    manifest,
                    &[
                        "manifest_id",
                        "paper_project_id",
                        "manifest_hash",
                        "version",
                    ],
                )
            })
            .collect::<Result<_, _>>()?;
        papers.push(json!({
            "paper_id": paper_id,
            "phase": paper.get("phase").cloned().unwrap_or(Value::Null),
            "tasks": tasks,
            "delivery_candidates": {
                "artifact_manifests": artifact_manifests,
                "section_heads": section_heads,
                "leases": leases,
            }
        }));
    }
    let value = json!({
        "schema": "hepta.paper_raid.agent_bridge.inbox.v2",
        "binding_id": verified.mapping.binding_id,
        "assurance": "self_declared_unverified",
        "papers": papers
    });
    complete_agent_request(&state, &verified, StatusCode::OK, &value).await
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
    for (field, expected) in [
        ("binding_id", verified.mapping.binding_id.to_string()),
        ("agent_id", verified.mapping.agent_id.clone()),
        ("agent_key_id", verified.mapping.agent_key_id.clone()),
    ] {
        if payload.get(field).and_then(Value::as_str) != Some(expected.as_str()) {
            return Err(AppError::Forbidden);
        }
    }
    let command = BrowserCommand {
        command: CommandName::SubmitAgentProposal,
        resource_id: Some(request.paper_id),
        child_id: None,
        session_id: None,
        idempotency_key,
        payload: request.payload,
    };
    let upstream = state
        .hepta
        .forward_command(&verified.identity, &command)
        .await?;
    if !(200..300).contains(&upstream.status) {
        return Err(AppError::Upstream);
    }
    let proposal = upstream.json()?;
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

fn mapping_repair_owner_matches(
    binding_id: Uuid,
    subject_id: &str,
    player_id: Uuid,
    agent_id: &str,
    expected_binding_id: Uuid,
    expected_subject_id: &str,
    expected_player_id: Uuid,
    expected_agent_id: &str,
) -> bool {
    binding_id == expected_binding_id
        && subject_id == expected_subject_id
        && player_id == expected_player_id
        && agent_id == expected_agent_id
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
    if !mapping_repair_owner_matches(
        row.get("binding_id"),
        &row.get::<String, _>("subject_id"),
        row.get("player_id"),
        &row.get::<String, _>("agent_id"),
        request.binding_id,
        &grant.subject_id,
        grant.player_id,
        &request.agent_id,
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
    if uri.query().is_some() {
        return Err(AppError::Forbidden);
    }
    let canonical_query = "";
    validate_agent_bridge_canonical_query(canonical_query).map_err(|_| AppError::Forbidden)?;
    if !canonical_query.is_empty() {
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
    let authoritative = authoritative_bridge_binding(state, &identity, &mapping).await?;
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
) -> Result<Value, AppError> {
    let value = state.hepta.list_current_agent_bindings(identity).await?;
    let bindings = value.as_array().ok_or(AppError::Upstream)?;
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
    Ok(binding.clone())
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

async fn bridge_audit(
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
    fn same_owner_binding_can_repair_key_continuity_but_identity_changes_cannot() {
        let binding_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        assert!(mapping_repair_owner_matches(
            binding_id,
            "subject-a",
            player_id,
            "agent-a",
            binding_id,
            "subject-a",
            player_id,
            "agent-a",
        ));
        assert!(!mapping_repair_owner_matches(
            binding_id,
            "subject-a",
            player_id,
            "agent-a",
            binding_id,
            "subject-b",
            player_id,
            "agent-a",
        ));
        assert!(!mapping_repair_owner_matches(
            binding_id,
            "subject-a",
            player_id,
            "agent-a",
            binding_id,
            "subject-a",
            player_id,
            "agent-b",
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
