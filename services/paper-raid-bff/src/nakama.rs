use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use reqwest::{header, Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;
use uuid::Uuid;

use crate::error::AppError;

const MAX_ARCHIVE_BYTES: usize = 2 * 1024 * 1024;
const NAKAMA_ARCHIVE_PAGE_LIMIT: u32 = 128;
const JSON_SAFE_U64_MAX: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberSessionAccess {
    pub logical_session_id: String,
    pub authorization_id: Uuid,
    pub roster_version: u64,
    pub status: String,
    pub nakama_completion_received: bool,
}

#[derive(Debug, Deserialize)]
struct HeptaMemberResearchSession {
    logical_session_id: String,
    authorization_id: Uuid,
    roster_version: u64,
    status: String,
    nakama_completion_received: bool,
}

#[derive(Debug, Serialize)]
struct ArchiveRequest<'a> {
    schema: &'static str,
    logical_session_id: &'a str,
    after_sequence: u64,
    limit: u32,
    authorization_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RpcEnvelope {
    payload: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResearchArchiveResponse {
    schema: String,
    logical_session_id: String,
    external_match_id: Option<String>,
    runtime_generation: u64,
    status: String,
    session_version: u64,
    roster_version: u64,
    roster_root: String,
    event_count: u64,
    after_sequence: u64,
    next_after_sequence: u64,
    has_more: bool,
    events: Vec<ResearchArchiveEvent>,
    roster: Vec<ResearchRosterEntry>,
    participants: Vec<ResearchParticipant>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResearchArchiveEvent {
    schema: String,
    event_id: String,
    event_type: String,
    session_id: String,
    team_id: String,
    paper_project_id: String,
    challenge_id: String,
    roster_version: u64,
    sequence: u64,
    causation_id: String,
    occurred_at_unix: i64,
    participant_slot: u64,
    session_version: u64,
    action_type: String,
    payload_type: String,
    payload: String,
    payload_hash: String,
    reference_hash: String,
    event_hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResearchRosterEntry {
    participant_slot: u64,
    authorization_id: String,
    subject_user_id: String,
    agent_id: String,
    agent_did: String,
    agent_key_id: String,
    agent_key_hash: String,
    role: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResearchParticipant {
    participant_slot: u64,
    authorization_id: String,
    subject_user_id: String,
    agent_id: String,
    role: String,
    joined: bool,
    connected: bool,
    ready: bool,
    last_action_sequence: u64,
    acknowledgement_reference_hash: Option<String>,
}

#[derive(Clone)]
pub struct NakamaArchiveClient {
    client: Client,
    base: Url,
    http_key: String,
}

impl NakamaArchiveClient {
    pub fn new(base: Url, http_key: String) -> Result<Self, String> {
        if base.path() != "/" && !base.path().is_empty() {
            return Err("Nakama base URL must not contain a path".to_string());
        }
        if http_key.len() < 8 || http_key.len() > 256 {
            return Err("Nakama HTTP key length is invalid".to_string());
        }
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(8))
            .user_agent("paper-raid-bff/0.1")
            .build()
            .map_err(|error| format!("cannot build Nakama client: {error}"))?;
        Ok(Self {
            client,
            base,
            http_key,
        })
    }

    pub async fn ready(&self) -> bool {
        let mut url = self.base.clone();
        url.set_path("/healthcheck");
        url.set_query(None);
        url.set_fragment(None);
        let Ok(response) = self.client.get(url).send().await else {
            return false;
        };
        if response.status() != StatusCode::OK || !strict_json(response.headers()) {
            return false;
        }
        limited_body(response, 64 * 1024).await.is_ok()
    }

    pub async fn archive(
        &self,
        access: &MemberSessionAccess,
        paper_id: Uuid,
        after_sequence: u64,
    ) -> Result<Value, AppError> {
        validate_session_id(&access.logical_session_id)?;
        let mut url = self.base.clone();
        url.set_path("/v2/rpc/trnm_research_session_archive_v1");
        url.query_pairs_mut()
            .clear()
            .append_pair("http_key", &self.http_key);
        url.set_fragment(None);
        let runtime_payload = serde_json::to_string(&ArchiveRequest {
            schema: "trnm.nakama.research-session.get-archive.v1",
            logical_session_id: &access.logical_session_id,
            after_sequence,
            limit: NAKAMA_ARCHIVE_PAGE_LIMIT,
            authorization_id: access.authorization_id,
        })
        .map_err(|_| AppError::Internal)?;
        let response = self
            .client
            .post(url)
            .header(header::ACCEPT, "application/json")
            .header(header::CONTENT_TYPE, "application/json")
            .json(&runtime_payload)
            .send()
            .await
            .map_err(|_| AppError::Unavailable("nakama"))?;
        if response.status().is_redirection() || !strict_json(response.headers()) {
            return Err(AppError::Upstream);
        }
        if response.status() == StatusCode::UNAUTHORIZED
            || response.status() == StatusCode::FORBIDDEN
        {
            return Err(AppError::Forbidden);
        }
        if !response.status().is_success() {
            return Err(AppError::Unavailable("nakama"));
        }
        let bytes = limited_body(response, MAX_ARCHIVE_BYTES).await?;
        let envelope: RpcEnvelope =
            serde_json::from_slice(&bytes).map_err(|_| AppError::Upstream)?;
        if envelope.payload.len() > MAX_ARCHIVE_BYTES {
            return Err(AppError::Upstream);
        }
        let archive: Value =
            serde_json::from_str(&envelope.payload).map_err(|_| AppError::Upstream)?;
        validate_archive_response(&archive, access, paper_id, after_sequence)?;
        Ok(archive)
    }
}

/// Extracts only the player-scoped authorization records returned by Hepta's
/// P3 Paper Room. Hepta already verified the Consumer assertion and removed
/// every other roster member. An invented `research_session.members` shape is
/// never accepted as authority.
pub fn member_session_accesses(
    paper_read_model: &Value,
) -> Result<Vec<MemberSessionAccess>, AppError> {
    let records = paper_read_model
        .get("member_research_sessions")
        .and_then(Value::as_array)
        .ok_or(AppError::Upstream)?;
    let mut accesses = Vec::with_capacity(records.len());
    for record in records {
        let record: HeptaMemberResearchSession =
            serde_json::from_value(record.clone()).map_err(|_| AppError::Upstream)?;
        validate_session_id(&record.logical_session_id)?;
        if record.roster_version == 0
            || record.roster_version > JSON_SAFE_U64_MAX
            || !matches!(
                record.status.as_str(),
                "issued" | "consumed" | "completed" | "superseded" | "expired"
            )
            || accesses.iter().any(|existing: &MemberSessionAccess| {
                existing.logical_session_id == record.logical_session_id
                    && existing.roster_version == record.roster_version
            })
        {
            return Err(AppError::Upstream);
        }
        accesses.push(MemberSessionAccess {
            logical_session_id: record.logical_session_id,
            authorization_id: record.authorization_id,
            roster_version: record.roster_version,
            status: record.status,
            nakama_completion_received: record.nakama_completion_received,
        });
    }
    accesses.sort_by(|left, right| {
        (&left.logical_session_id, left.roster_version)
            .cmp(&(&right.logical_session_id, right.roster_version))
    });
    Ok(accesses)
}

fn validate_session_id(value: &str) -> Result<(), AppError> {
    if value.is_empty()
        || value.len() > 128
        || value.bytes().any(|byte| {
            !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
        })
    {
        return Err(AppError::Upstream);
    }
    Ok(())
}

fn validate_archive_response(
    value: &Value,
    access: &MemberSessionAccess,
    paper_id: Uuid,
    requested_after_sequence: u64,
) -> Result<(), AppError> {
    let archive: ResearchArchiveResponse =
        serde_json::from_value(value.clone()).map_err(|_| AppError::Upstream)?;
    if value.get("external_match_id").is_some_and(Value::is_null)
        || value
            .get("participants")
            .and_then(Value::as_array)
            .is_some_and(|participants| {
                participants.iter().any(|participant| {
                    participant
                        .get("acknowledgement_reference_hash")
                        .is_some_and(Value::is_null)
                })
            })
    {
        return Err(AppError::Upstream);
    }
    if archive.schema != "trnm.nakama.research-session.archive.v1"
        || archive.logical_session_id != access.logical_session_id
        || archive.roster_version != access.roster_version
        || archive.after_sequence != requested_after_sequence
        || archive.runtime_generation == 0
        || archive.runtime_generation > JSON_SAFE_U64_MAX
        || archive.session_version == 0
        || archive.session_version > JSON_SAFE_U64_MAX
        || archive.roster_version == 0
        || archive.roster_version > JSON_SAFE_U64_MAX
        || archive.event_count > JSON_SAFE_U64_MAX
        || archive.session_version
            != archive
                .event_count
                .checked_add(1)
                .ok_or(AppError::Upstream)?
        || archive.after_sequence > JSON_SAFE_U64_MAX
        || archive.next_after_sequence > JSON_SAFE_U64_MAX
        || archive.next_after_sequence < archive.after_sequence
        || !valid_digest(&archive.roster_root)
        || !matches!(
            archive.status.as_str(),
            "created" | "waiting" | "ready" | "active" | "paused" | "completed"
        )
        || archive
            .external_match_id
            .as_deref()
            .is_some_and(|value| !valid_text(value, 512))
        || !(3..=5).contains(&archive.roster.len())
        || archive.participants.len() != archive.roster.len()
        || archive.events.len() > NAKAMA_ARCHIVE_PAGE_LIMIT as usize
    {
        return Err(AppError::Upstream);
    }

    let mut roster_by_slot = BTreeMap::new();
    let mut authorization_ids = BTreeSet::new();
    let mut subject_ids = BTreeSet::new();
    let mut agent_ids = BTreeSet::new();
    let mut agent_dids = BTreeSet::new();
    let mut agent_key_ids = BTreeSet::new();
    let mut agent_key_hashes = BTreeSet::new();
    let mut player_authorization_present = false;
    for entry in &archive.roster {
        if !(1..=5).contains(&entry.participant_slot)
            || !valid_uuid(&entry.authorization_id)
            || !valid_uuid(&entry.subject_user_id)
            || !valid_text(&entry.agent_id, 512)
            || !valid_text(&entry.agent_did, 512)
            || !valid_text(&entry.agent_key_id, 512)
            || !valid_digest(&entry.agent_key_hash)
            || !valid_text(&entry.role, 512)
            || !authorization_ids.insert(entry.authorization_id.as_str())
            || !subject_ids.insert(entry.subject_user_id.as_str())
            || !agent_ids.insert(entry.agent_id.as_str())
            || !agent_dids.insert(entry.agent_did.as_str())
            || !agent_key_ids.insert(entry.agent_key_id.as_str())
            || !agent_key_hashes.insert(entry.agent_key_hash.as_str())
            || roster_by_slot
                .insert(
                    entry.participant_slot,
                    (
                        entry.authorization_id.as_str(),
                        entry.subject_user_id.as_str(),
                        entry.agent_id.as_str(),
                        entry.role.as_str(),
                    ),
                )
                .is_some()
        {
            return Err(AppError::Upstream);
        }
        player_authorization_present |=
            entry.authorization_id == access.authorization_id.to_string();
    }
    if !player_authorization_present {
        return Err(AppError::Upstream);
    }
    if roster_by_slot
        .keys()
        .copied()
        .ne(1..=archive.roster.len() as u64)
    {
        return Err(AppError::Upstream);
    }

    let mut participant_slots = BTreeSet::new();
    for participant in &archive.participants {
        if !participant_slots.insert(participant.participant_slot)
            || participant.last_action_sequence > JSON_SAFE_U64_MAX
            || participant
                .acknowledgement_reference_hash
                .as_deref()
                .is_some_and(|value| !valid_digest(value))
            || roster_by_slot.get(&participant.participant_slot).copied()
                != Some((
                    participant.authorization_id.as_str(),
                    participant.subject_user_id.as_str(),
                    participant.agent_id.as_str(),
                    participant.role.as_str(),
                ))
        {
            return Err(AppError::Upstream);
        }
        let _ = (participant.joined, participant.connected, participant.ready);
    }

    let mut expected_sequence = archive
        .after_sequence
        .checked_add(1)
        .ok_or(AppError::Upstream)?;
    let mut page_identity: Option<(&str, &str, &str)> = None;
    let mut previous_time = None;
    let mut event_ids = BTreeSet::new();
    let mut event_hashes = BTreeSet::new();
    for (index, event) in archive.events.iter().enumerate() {
        let identity = (
            event.team_id.as_str(),
            event.paper_project_id.as_str(),
            event.challenge_id.as_str(),
        );
        if event.schema != "trnm.research-session.event.v1"
            || event.session_id != archive.logical_session_id
            || event.sequence != expected_sequence
            || event.sequence > JSON_SAFE_U64_MAX
            || event.roster_version == 0
            || event.roster_version > archive.roster_version
            || event.session_version == 0
            || event.session_version > archive.session_version
            || event.session_version != event.sequence.checked_add(1).ok_or(AppError::Upstream)?
            || event.occurred_at_unix < 0
            || event.participant_slot > 5
            || !valid_digest(&event.event_id)
            || !valid_uuid(&event.team_id)
            || !valid_uuid(&event.paper_project_id)
            || event.paper_project_id != paper_id.to_string()
            || !valid_uuid(&event.challenge_id)
            || !matches!(
                event.event_type.as_str(),
                "participant_joined"
                    | "participant_disconnected"
                    | "participant_reconnected"
                    | "research_action_applied"
                    | "roster_replaced"
                    | "research_session_completed"
            )
            || !valid_text(&event.causation_id, 512)
            || !valid_text(&event.action_type, 512)
            || !valid_text(&event.payload_type, 512)
            || !valid_base64_payload(&event.payload)
            || !valid_digest(&event.payload_hash)
            || !valid_digest(&event.reference_hash)
            || !valid_digest(&event.event_hash)
            || page_identity.is_some_and(|expected| expected != identity)
            || previous_time.is_some_and(|previous| event.occurred_at_unix < previous)
            || !event_ids.insert(event.event_id.as_str())
            || !event_hashes.insert(event.event_hash.as_str())
            || (event.event_type == "research_session_completed"
                && (event.participant_slot != 0
                    || index + 1 != archive.events.len()
                    || archive.has_more
                    || archive.status != "completed"))
            || (event.event_type != "research_session_completed" && event.participant_slot == 0)
        {
            return Err(AppError::Upstream);
        }
        page_identity = Some(identity);
        previous_time = Some(event.occurred_at_unix);
        expected_sequence = expected_sequence.checked_add(1).ok_or(AppError::Upstream)?;
    }

    let returned_last = archive
        .events
        .last()
        .map(|event| event.sequence)
        .unwrap_or(archive.after_sequence);
    if archive.next_after_sequence != returned_last
        || archive.event_count < archive.next_after_sequence
        || archive.has_more != (archive.next_after_sequence < archive.event_count)
        || (archive.has_more && archive.events.is_empty())
        || (archive.has_more && archive.events.len() != NAKAMA_ARCHIVE_PAGE_LIMIT as usize)
    {
        return Err(AppError::Upstream);
    }
    Ok(())
}

fn valid_uuid(value: &str) -> bool {
    Uuid::parse_str(value)
        .ok()
        .is_some_and(|parsed| parsed.hyphenated().to_string() == value)
}

fn valid_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.chars().count() <= max
        && !value.chars().any(|character| character == '\0')
}

fn valid_base64_payload(value: &str) -> bool {
    if !(4..=87_384).contains(&value.len()) || !value.len().is_multiple_of(4) {
        return false;
    }
    BASE64_STANDARD
        .decode(value)
        .ok()
        .filter(|decoded| !decoded.is_empty() && decoded.len() <= 65_536)
        .is_some_and(|decoded| BASE64_STANDARD.encode(decoded) == value)
}

fn strict_json(headers: &header::HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            matches!(
                value,
                "application/json" | "application/json; charset=utf-8"
            )
        })
}

async fn limited_body(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>, AppError> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(AppError::Upstream);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| AppError::Upstream)? {
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(AppError::Upstream);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        extract::{Query, State},
        http::HeaderMap,
        response::{IntoResponse, Redirect, Response},
        routing::post,
        Json, Router,
    };
    use std::{collections::HashMap, sync::Arc};
    use tokio::sync::Mutex;

    #[test]
    fn access_uses_only_player_scoped_p3_member_sessions() {
        let first_authorization = Uuid::new_v4();
        let second_authorization = Uuid::new_v4();
        let model = serde_json::json!({
            "research_session": {"members":[{"authorization_id":Uuid::new_v4()}]},
            "member_research_sessions": [
                {
                    "logical_session_id":"paper.raid:one",
                    "authorization_id":first_authorization,
                    "roster_version":1,
                    "nakama_completion_received":false,
                    "status":"issued",
                    "issued_at":"2026-08-05T00:00:00Z",
                    "expires_at":"2026-08-06T00:00:00Z",
                    "consumed_at":null,
                    "authorization_set_id":Uuid::new_v4()
                },
                {
                    "logical_session_id":"paper.raid:one",
                    "authorization_id":second_authorization,
                    "roster_version":2,
                    "nakama_completion_received":true,
                    "status":"consumed",
                    "issued_at":"2026-08-05T01:00:00Z",
                    "expires_at":"2026-08-06T01:00:00Z",
                    "consumed_at":"2026-08-05T02:00:00Z",
                    "authorization_set_id":Uuid::new_v4()
                }
            ]
        });
        let accesses = member_session_accesses(&model).expect("member accesses");
        assert_eq!(accesses.len(), 2);
        assert_eq!(accesses[0].authorization_id, first_authorization);
        assert_eq!(accesses[1].authorization_id, second_authorization);
        assert!(accesses[1].nakama_completion_received);

        assert!(member_session_accesses(&serde_json::json!({
            "research_session":{"members":[]}
        }))
        .is_err());
    }

    #[derive(Default)]
    struct CapturedWire {
        query_key: String,
        body: String,
        content_type: String,
    }

    fn digest(character: char) -> String {
        format!("sha256:{}", character.to_string().repeat(64))
    }

    fn valid_archive(
        logical_session_id: &str,
        paper_project_id: &str,
        player_authorization_id: &str,
        roster_version: u64,
        after_sequence: u64,
        event_sequences: &[u64],
        event_count: u64,
    ) -> Value {
        let authorizations = [
            player_authorization_id.to_string(),
            "22222222-2222-4222-8222-222222222222".into(),
            "33333333-3333-4333-8333-333333333333".into(),
        ];
        let subjects = [
            "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
            "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
        ];
        let roles = ["captain", "evidence", "experiment"];
        let roster: Vec<_> = (0..3)
            .map(|index| {
                serde_json::json!({
                    "participant_slot": index + 1,
                    "authorization_id": authorizations[index].as_str(),
                    "subject_user_id": subjects[index],
                    "agent_id": format!("agent-{}", index + 1),
                    "agent_did": format!("did:example:agent-{}", index + 1),
                    "agent_key_id": format!("agent-key-{}", index + 1),
                    "agent_key_hash": digest((b'a' + index as u8) as char),
                    "role": roles[index],
                })
            })
            .collect();
        let participants: Vec<_> = (0..3)
            .map(|index| {
                serde_json::json!({
                    "participant_slot": index + 1,
                    "authorization_id": authorizations[index].as_str(),
                    "subject_user_id": subjects[index],
                    "agent_id": format!("agent-{}", index + 1),
                    "role": roles[index],
                    "joined": true,
                    "connected": true,
                    "ready": true,
                    "last_action_sequence": 0,
                })
            })
            .collect();
        let events: Vec<_> = event_sequences
            .iter()
            .map(|sequence| {
                serde_json::json!({
                    "schema": "trnm.research-session.event.v1",
                    "event_id": format!("sha256:{sequence:064x}"),
                    "event_type": "research_action_applied",
                    "session_id": logical_session_id,
                    "team_id": "44444444-4444-4444-8444-444444444444",
                    "paper_project_id": paper_project_id,
                    "challenge_id": "66666666-6666-4666-8666-666666666666",
                    "roster_version": roster_version,
                    "sequence": sequence,
                    "causation_id": format!("action-{sequence}"),
                    "occurred_at_unix": 1_786_344_000,
                    "participant_slot": 1,
                    "session_version": sequence + 1,
                    "action_type": "experiment.record",
                    "payload_type": "application/json",
                    "payload": "e30=",
                    "payload_hash": digest('e'),
                    "reference_hash": digest('f'),
                    "event_hash": format!("sha256:{:064x}", sequence + 1_000),
                })
            })
            .collect();
        let next_after_sequence = event_sequences.last().copied().unwrap_or(after_sequence);
        serde_json::json!({
            "schema": "trnm.nakama.research-session.archive.v1",
            "logical_session_id": logical_session_id,
            "runtime_generation": 1,
            "status": "active",
            "session_version": event_count + 1,
            "roster_version": roster_version,
            "roster_root": digest('2'),
            "event_count": event_count,
            "after_sequence": after_sequence,
            "next_after_sequence": next_after_sequence,
            "has_more": next_after_sequence < event_count,
            "events": events,
            "roster": roster,
            "participants": participants,
        })
    }

    async fn exact_wire_handler(
        State(captured): State<Arc<Mutex<CapturedWire>>>,
        Query(query): Query<HashMap<String, String>>,
        headers: HeaderMap,
        body: String,
    ) -> Response {
        let inner: String = serde_json::from_str(&body).expect("outer request JSON");
        let request: Value = serde_json::from_str(&inner).expect("inner request JSON");
        let logical_session_id = request["logical_session_id"]
            .as_str()
            .expect("logical session");
        let authorization_id = request["authorization_id"].as_str().expect("authorization");
        let after_sequence = request["after_sequence"].as_u64().expect("after sequence");
        let archive = valid_archive(
            logical_session_id,
            "55555555-5555-4555-8555-555555555555",
            authorization_id,
            3,
            after_sequence,
            &[after_sequence + 1],
            after_sequence + 1,
        );
        let mut captured = captured.lock().await;
        captured.query_key = query.get("http_key").cloned().unwrap_or_default();
        captured.content_type = headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        captured.body = body;
        Json(serde_json::json!({
            "payload": archive.to_string()
        }))
        .into_response()
    }

    async fn spawn(router: Router) -> Url {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock");
        let address = listener.local_addr().expect("mock address");
        tokio::spawn(async move {
            axum::serve(listener, router).await.expect("mock server");
        });
        Url::parse(&format!("http://{address}")).expect("mock URL")
    }

    #[tokio::test]
    async fn exact_rest_rpc_wire_is_double_json_and_query_encoded() {
        let captured = Arc::new(Mutex::new(CapturedWire::default()));
        let base = spawn(
            Router::new()
                .route(
                    "/v2/rpc/trnm_research_session_archive_v1",
                    post(exact_wire_handler),
                )
                .with_state(captured.clone()),
        )
        .await;
        let http_key = "alpha key+/with?symbols&";
        let client = NakamaArchiveClient::new(base, http_key.into()).expect("client");
        let authorization_id = Uuid::new_v4();
        let paper_id = Uuid::parse_str("55555555-5555-4555-8555-555555555555").expect("paper UUID");
        let result = client
            .archive(
                &MemberSessionAccess {
                    logical_session_id: "paper.raid:wire".into(),
                    authorization_id,
                    roster_version: 3,
                    status: "consumed".into(),
                    nakama_completion_received: false,
                },
                paper_id,
                7,
            )
            .await
            .expect("archive");
        assert_eq!(result["events"][0]["sequence"], 8);
        let captured = captured.lock().await;
        assert_eq!(captured.query_key, http_key);
        assert_eq!(captured.content_type, "application/json");
        let inner: String = serde_json::from_str(&captured.body).expect("outer JSON string");
        let payload: Value = serde_json::from_str(&inner).expect("runtime payload");
        assert_eq!(
            payload["schema"],
            "trnm.nakama.research-session.get-archive.v1"
        );
        assert_eq!(payload["authorization_id"], authorization_id.to_string());
        assert_eq!(payload["after_sequence"], 7);
        assert_eq!(payload["limit"], NAKAMA_ARCHIVE_PAGE_LIMIT);
        assert!(payload.get("operator_credential").is_none());
    }

    #[test]
    fn archive_response_binds_epoch_cursor_and_contiguous_page() {
        let authorization_id = Uuid::new_v4();
        let paper_id = Uuid::parse_str("55555555-5555-4555-8555-555555555555").expect("paper UUID");
        let access = MemberSessionAccess {
            logical_session_id: "paper.raid:validated".into(),
            authorization_id,
            roster_version: 2,
            status: "consumed".into(),
            nakama_completion_received: false,
        };
        let valid = valid_archive(
            &access.logical_session_id,
            &paper_id.to_string(),
            &authorization_id.to_string(),
            access.roster_version,
            7,
            &[8, 9],
            9,
        );
        validate_archive_response(&valid, &access, paper_id, 7).expect("valid bounded page");

        let full_page: Vec<_> = (8..136).collect();
        let paged = valid_archive(
            &access.logical_session_id,
            &paper_id.to_string(),
            &authorization_id.to_string(),
            access.roster_version,
            7,
            &full_page,
            136,
        );
        validate_archive_response(&paged, &access, paper_id, 7).expect("valid full catch-up page");

        let mut malformed = Vec::new();
        let mut wrong_schema = valid.clone();
        wrong_schema["schema"] = Value::String("invented.archive.v1".into());
        malformed.push(wrong_schema);
        let mut wrong_session = valid.clone();
        wrong_session["logical_session_id"] = Value::String("paper.raid:other".into());
        malformed.push(wrong_session);
        let mut wrong_roster = valid.clone();
        wrong_roster["roster_version"] = Value::from(1);
        malformed.push(wrong_roster);
        let mut wrong_echo = valid.clone();
        wrong_echo["after_sequence"] = Value::from(6);
        malformed.push(wrong_echo);
        let mut sequence_gap = valid.clone();
        sequence_gap["events"][1]["sequence"] = Value::from(10);
        malformed.push(sequence_gap);
        let mut unknown_field = valid.clone();
        unknown_field["unexpected"] = Value::Bool(true);
        malformed.push(unknown_field);
        malformed.push(valid_archive(
            &access.logical_session_id,
            &paper_id.to_string(),
            &authorization_id.to_string(),
            access.roster_version,
            7,
            &[],
            8,
        ));

        for archive in malformed {
            assert!(matches!(
                validate_archive_response(&archive, &access, paper_id, 7),
                Err(AppError::Upstream)
            ));
        }
    }

    async fn redirect_handler() -> Redirect {
        Redirect::temporary("/somewhere-else")
    }

    async fn wrong_type_handler() -> Response {
        (StatusCode::OK, [(header::CONTENT_TYPE, "text/plain")], "{}").into_response()
    }

    async fn bad_envelope_handler() -> Json<Value> {
        Json(serde_json::json!({"payload":"{}", "unexpected":true}))
    }

    async fn oversized_handler() -> Json<Value> {
        Json(serde_json::json!({"payload":"x".repeat(MAX_ARCHIVE_BYTES + 1)}))
    }

    #[tokio::test]
    async fn rejects_redirect_content_type_bad_envelope_and_body_cap() {
        let access = MemberSessionAccess {
            logical_session_id: "paper.raid:negative".into(),
            authorization_id: Uuid::new_v4(),
            roster_version: 1,
            status: "issued".into(),
            nakama_completion_received: false,
        };
        for handler in [
            Router::new().route(
                "/v2/rpc/trnm_research_session_archive_v1",
                post(redirect_handler),
            ),
            Router::new().route(
                "/v2/rpc/trnm_research_session_archive_v1",
                post(wrong_type_handler),
            ),
            Router::new().route(
                "/v2/rpc/trnm_research_session_archive_v1",
                post(bad_envelope_handler),
            ),
            Router::new().route(
                "/v2/rpc/trnm_research_session_archive_v1",
                post(oversized_handler),
            ),
        ] {
            let base = spawn(handler).await;
            let client = NakamaArchiveClient::new(base, "alpha-http-key".into()).expect("client");
            assert!(client.archive(&access, Uuid::new_v4(), 0).await.is_err());
        }
    }
}
