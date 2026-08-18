use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use hepta_paper_raid_contracts::{
    assigned_challenge_material_bundle_hash, canonical_json_sha256,
    parse_challenge_dataset_manifest, parse_challenge_evaluator_manifest,
    parse_challenge_pack_manifest, resolve_challenge_material_objects,
    verify_assigned_challenge_material_bundle, verify_frozen_challenge_material_authority,
    AssignedChallengeMaterialBundleV1, AssignedChallengeMaterialObjectV1,
    FrozenChallengeMaterialAuthorityV1, ASSIGNED_CHALLENGE_MATERIAL_BUNDLE_V1, JSON_SAFE_U64_MAX,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{app::AppState, config::AlphaIdentityScope, error::AppError};

const BROWSER_CHALLENGE_MATERIALS_V1: &str = "hepta.paper_raid.bff.challenge_materials.v1";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/papers/:paper_id/challenge-materials",
            get(browser_challenge_materials),
        )
        .route(
            "/api/papers/:paper_id/challenge-materials/:object_key",
            get(browser_challenge_material_object),
        )
        .route(
            "/api/agent-bridge/challenge-objects",
            get(crate::agent_bridge::agent_challenge_object),
        )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserChallengeMaterialObjectQuery {
    digest: String,
    projection_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AuthorWorkItemScope {
    paper_project_id: Uuid,
    work_item_id: Uuid,
    work_item_version: u64,
}

fn parse_uuid(record: &Value, field: &str) -> Result<Uuid, AppError> {
    let text = record
        .get(field)
        .and_then(Value::as_str)
        .ok_or(AppError::Upstream)?;
    let value = Uuid::parse_str(text).map_err(|_| AppError::Upstream)?;
    if value.is_nil() || value.to_string() != text {
        return Err(AppError::Upstream);
    }
    Ok(value)
}

fn frozen_authority_from_room(
    room: &Value,
) -> Result<(Uuid, String, FrozenChallengeMaterialAuthorityV1), AppError> {
    let paper = room
        .get("paper")
        .filter(|value| value.is_object())
        .ok_or(AppError::Upstream)?;
    let paper_project_id = parse_uuid(paper, "paper_project_id")?;
    let challenge_id = parse_uuid(paper, "challenge_id")?;
    let challenge_ruleset_snapshot_hash = paper
        .get("challenge_ruleset_snapshot_hash")
        .and_then(Value::as_str)
        .ok_or(AppError::Upstream)?
        .to_string();
    let snapshot = paper
        .get("challenge_ruleset_snapshot")
        .filter(|value| value.is_object())
        .ok_or(AppError::Upstream)?;
    let authority: FrozenChallengeMaterialAuthorityV1 =
        serde_json::from_value(snapshot.get("material_authority").cloned().ok_or_else(|| {
            AppError::Conflict(
                "challenge materials are unavailable without a frozen activation snapshot".into(),
            )
        })?)
        .map_err(|_| AppError::Upstream)?;
    verify_frozen_challenge_material_authority(&authority).map_err(|_| AppError::Upstream)?;
    if authority.challenge_id != challenge_id
        || snapshot
            .get("challenge_snapshot_hash")
            .and_then(Value::as_str)
            != Some(authority.challenge_snapshot_hash.as_str())
        || snapshot.get("ruleset_version").and_then(Value::as_str)
            != Some(authority.ruleset_version.as_str())
        || snapshot.get("ruleset_hash").and_then(Value::as_str)
            != Some(authority.ruleset_hash.as_str())
    {
        return Err(AppError::Upstream);
    }
    Ok((paper_project_id, challenge_ruleset_snapshot_hash, authority))
}

fn author_work_item_scope(
    room: &Value,
    paper_project_id: Uuid,
    binding_id: Uuid,
    player_id: Uuid,
    work_item_id: Uuid,
) -> Result<AuthorWorkItemScope, AppError> {
    if room
        .get("paper")
        .and_then(|paper| paper.get("outcome"))
        .and_then(Value::as_str)
        != Some("in_progress")
    {
        return Err(AppError::Forbidden);
    }
    let matches = room
        .get("work_items")
        .and_then(Value::as_array)
        .ok_or(AppError::Upstream)?
        .iter()
        .filter(|item| parse_uuid(item, "work_item_id").ok() == Some(work_item_id))
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(AppError::Forbidden);
    }
    let item = matches[0];
    let work_item_version = item
        .get("version")
        .and_then(Value::as_u64)
        .filter(|version| *version > 0 && *version <= JSON_SAFE_U64_MAX)
        .ok_or(AppError::Upstream)?;
    if parse_uuid(item, "paper_project_id")? != paper_project_id
        || parse_uuid(item, "assigned_binding_id")? != binding_id
        || parse_uuid(item, "assigned_player_id")? != player_id
        || !matches!(
            item.get("status").and_then(Value::as_str),
            Some("planned" | "in_progress" | "review")
        )
    {
        return Err(AppError::Forbidden);
    }
    Ok(AuthorWorkItemScope {
        paper_project_id,
        work_item_id,
        work_item_version,
    })
}

/// Resolve the four player-facing bytes from the Paper's immutable activation-derived authority.
///
/// This function is shared by Browser rendering and Agent Bridge projection.  It never accepts a
/// digest, manifest or path supplied by a player: all three manifests originate in the Paper
/// snapshot, and every selected object must be closed by the pack plus dataset/evaluator manifests.
pub(crate) async fn resolve_frozen_challenge_materials(
    state: &AppState,
    room: &Value,
) -> Result<
    (
        Uuid,
        String,
        FrozenChallengeMaterialAuthorityV1,
        Vec<AssignedChallengeMaterialObjectV1>,
    ),
    AppError,
> {
    let (paper_project_id, snapshot_hash, authority) = frozen_authority_from_room(room)?;
    let objects = resolve_materials_from_authority(state, &authority).await?;
    Ok((paper_project_id, snapshot_hash, authority, objects))
}

async fn resolve_materials_from_authority(
    state: &AppState,
    authority: &FrozenChallengeMaterialAuthorityV1,
) -> Result<Vec<AssignedChallengeMaterialObjectV1>, AppError> {
    let pack_bytes = state
        .cas
        .get(&authority.pack_manifest_hash, "application/json")
        .await?;
    let pack = parse_challenge_pack_manifest(&pack_bytes, &authority.pack_manifest_hash)
        .map_err(|_| AppError::Upstream)?;
    if pack.pack_id != authority.pack_id
        || pack.template != authority.template
        || pack.ruleset_version != authority.ruleset_version
        || pack.dataset_manifest_sha256 != authority.dataset_manifest_hash
        || pack.evaluator_manifest_sha256 != authority.evaluator_manifest_hash
    {
        return Err(AppError::Upstream);
    }
    let evaluator_bytes = state
        .cas
        .get(&authority.evaluator_manifest_hash, "application/json")
        .await?;
    let dataset_bytes = state
        .cas
        .get(&authority.dataset_manifest_hash, "application/json")
        .await?;
    let evaluator =
        parse_challenge_evaluator_manifest(&evaluator_bytes, &authority.evaluator_manifest_hash)
            .map_err(|_| AppError::Upstream)?;
    let dataset =
        parse_challenge_dataset_manifest(&dataset_bytes, &authority.dataset_manifest_hash)
            .map_err(|_| AppError::Upstream)?;
    let objects = resolve_challenge_material_objects(&pack, &evaluator, &dataset)
        .map_err(|_| AppError::Upstream)?;
    for object in &objects {
        state.cas.validate_media_type(&object.media_type)?;
        let bytes = state.cas.get(&object.digest, &object.media_type).await?;
        if bytes.len() as u64 != object.size_bytes {
            return Err(AppError::Upstream);
        }
    }
    Ok(objects)
}

pub(crate) async fn resolve_assigned_challenge_material_bundle(
    state: &AppState,
    room: &Value,
    binding_id: Uuid,
    player_id: Uuid,
    work_item_id: Uuid,
) -> Result<AssignedChallengeMaterialBundleV1, AppError> {
    // Prove the exact live Author assignment before any CAS bytes are read.  This keeps a stale,
    // foreign, or terminal work item from becoming an object-existence oracle.
    let (paper_project_id, snapshot_hash, authority) = frozen_authority_from_room(room)?;
    let scope =
        author_work_item_scope(room, paper_project_id, binding_id, player_id, work_item_id)?;
    let objects = resolve_materials_from_authority(state, &authority).await?;
    let mut bundle = AssignedChallengeMaterialBundleV1 {
        schema: ASSIGNED_CHALLENGE_MATERIAL_BUNDLE_V1.to_string(),
        bundle_hash: String::new(),
        authority_hash: authority.authority_hash.clone(),
        authority,
        paper_project_id: scope.paper_project_id,
        challenge_ruleset_snapshot_hash: snapshot_hash,
        binding_id,
        player_id,
        work_item_id: scope.work_item_id,
        work_item_version: scope.work_item_version,
        objects,
    };
    bundle.bundle_hash =
        assigned_challenge_material_bundle_hash(&bundle).map_err(|_| AppError::Upstream)?;
    verify_assigned_challenge_material_bundle(&bundle).map_err(|_| AppError::Upstream)?;
    Ok(bundle)
}

pub(crate) fn authorized_challenge_material_media(
    bundle: &AssignedChallengeMaterialBundleV1,
    expected_binding_id: Uuid,
    expected_player_id: Uuid,
    work_item_id: Uuid,
    bundle_hash: &str,
    object_key: &str,
    digest: &str,
) -> Result<String, AppError> {
    verify_assigned_challenge_material_bundle(bundle).map_err(|_| AppError::Upstream)?;
    if bundle.binding_id != expected_binding_id
        || bundle.player_id != expected_player_id
        || bundle.work_item_id != work_item_id
        || bundle.bundle_hash != bundle_hash
    {
        return Err(AppError::Forbidden);
    }
    let matches = bundle
        .objects
        .iter()
        .filter(|object| object.object_key == object_key && object.digest == digest)
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(AppError::Forbidden);
    }
    Ok(matches[0].media_type.clone())
}

fn browser_material_projection(
    paper_project_id: Uuid,
    challenge_ruleset_snapshot_hash: &str,
    authority: &FrozenChallengeMaterialAuthorityV1,
    objects: &[AssignedChallengeMaterialObjectV1],
) -> Result<Value, AppError> {
    verify_frozen_challenge_material_authority(authority).map_err(|_| AppError::Upstream)?;
    if objects.len() != 4 {
        return Err(AppError::Upstream);
    }
    let projected_objects = objects
        .iter()
        .map(|object| {
            json!({
                "object_key": object.object_key,
                "logical_path": object.logical_path,
                "role": object.role,
                "digest": object.digest,
                "size_bytes": object.size_bytes,
                "media_type": object.media_type,
                "download_path": format!(
                    "/api/papers/{paper_project_id}/challenge-materials/{}",
                    object.object_key
                ),
            })
        })
        .collect::<Vec<_>>();
    let frame = json!({
        "schema": BROWSER_CHALLENGE_MATERIALS_V1,
        "paper_project_id": paper_project_id,
        "challenge_ruleset_snapshot_hash": challenge_ruleset_snapshot_hash,
        "material_authority": authority,
        "objects": projected_objects,
    });
    let projection_hash = canonical_json_sha256(&frame).map_err(|_| AppError::Upstream)?;
    let mut projection = frame;
    projection
        .as_object_mut()
        .ok_or(AppError::Internal)?
        .insert("projection_hash".into(), json!(projection_hash));
    Ok(projection)
}

async fn browser_challenge_materials(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    if !session.identity.has_scope(AlphaIdentityScope::Author) {
        return Err(AppError::Forbidden);
    }
    let room = state
        .hepta
        .get_paper_room(&session.identity, paper_id)
        .await?;
    let (resolved_paper_id, snapshot_hash, authority, objects) =
        resolve_frozen_challenge_materials(&state, room.value()).await?;
    if resolved_paper_id != paper_id {
        return Err(AppError::Upstream);
    }
    let projection = browser_material_projection(paper_id, &snapshot_hash, &authority, &objects)?;
    Ok(crate::app::private_no_store(
        Json(projection).into_response(),
    ))
}

async fn browser_challenge_material_object(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((paper_id, object_key)): Path<(Uuid, String)>,
    Query(query): Query<BrowserChallengeMaterialObjectQuery>,
) -> Result<Response, AppError> {
    crate::cas::raw_sha256(&query.digest)?;
    crate::cas::raw_sha256(&query.projection_hash)?;
    let session = state.session(&headers).await?;
    if !session.identity.has_scope(AlphaIdentityScope::Author) {
        return Err(AppError::Forbidden);
    }
    let room = state
        .hepta
        .get_paper_room(&session.identity, paper_id)
        .await?;
    let (resolved_paper_id, snapshot_hash, authority, objects) =
        resolve_frozen_challenge_materials(&state, room.value()).await?;
    if resolved_paper_id != paper_id {
        return Err(AppError::Upstream);
    }
    let projection = browser_material_projection(paper_id, &snapshot_hash, &authority, &objects)?;
    if projection.get("projection_hash").and_then(Value::as_str)
        != Some(query.projection_hash.as_str())
    {
        return Err(AppError::Forbidden);
    }
    let matches = objects
        .iter()
        .filter(|object| object.object_key == object_key && object.digest == query.digest)
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(AppError::NotFound);
    }
    let object = matches[0];
    state.cas.validate_media_type(&object.media_type)?;
    let bytes = state.cas.get(&object.digest, &object.media_type).await?;
    if bytes.len() as u64 != object.size_bytes {
        return Err(AppError::Upstream);
    }
    crate::app::review_artifact_response(bytes, &object.media_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hepta_paper_raid_contracts::{
        frozen_challenge_material_authority_hash, FROZEN_CHALLENGE_MATERIAL_AUTHORITY_V1,
    };
    use serde_json::json;

    fn room_fixture() -> (Value, Uuid, Uuid, Uuid) {
        let paper_id = Uuid::from_u128(1);
        let challenge_id = Uuid::from_u128(2);
        let binding_id = Uuid::from_u128(3);
        let player_id = Uuid::from_u128(4);
        let work_item_id = Uuid::from_u128(5);
        let challenge_snapshot_hash = format!("sha256:{}", "1".repeat(64));
        let ruleset_hash = format!("sha256:{}", "2".repeat(64));
        let mut authority = FrozenChallengeMaterialAuthorityV1 {
            schema: FROZEN_CHALLENGE_MATERIAL_AUTHORITY_V1.to_string(),
            authority_hash: String::new(),
            activation_id: Uuid::from_u128(6),
            activation_request_sha256: format!("sha256:{}", "3".repeat(64)),
            challenge_id,
            challenge_snapshot_hash: challenge_snapshot_hash.clone(),
            template: "evidence-audit".to_string(),
            pack_id: "paper-raid-evidence-audit-seeded-v1".to_string(),
            pack_manifest_hash: format!("sha256:{}", "4".repeat(64)),
            ruleset_version: "paper-raid-evidence-audit-v1".to_string(),
            ruleset_hash: ruleset_hash.clone(),
            dataset_manifest_hash: format!("sha256:{}", "5".repeat(64)),
            evaluator_manifest_hash: format!("sha256:{}", "6".repeat(64)),
        };
        authority.authority_hash = frozen_challenge_material_authority_hash(&authority).unwrap();
        let room = json!({
            "paper": {
                "paper_project_id": paper_id,
                "challenge_id": challenge_id,
                "outcome": "in_progress",
                "challenge_ruleset_snapshot_hash": format!("sha256:{}", "7".repeat(64)),
                "challenge_ruleset_snapshot": {
                    "schema": "hepta.paper_raid.challenge_ruleset_snapshot.v1",
                    "challenge_snapshot_hash": challenge_snapshot_hash,
                    "ruleset_version": authority.ruleset_version,
                    "ruleset_hash": ruleset_hash,
                    "enforcement": "authoritative_v1",
                    "ruleset": {},
                    "material_authority": authority,
                }
            },
            "work_items": [{
                "work_item_id": work_item_id,
                "paper_project_id": paper_id,
                "assigned_binding_id": binding_id,
                "assigned_player_id": player_id,
                "status": "in_progress",
                "version": 3
            }]
        });
        (room, binding_id, player_id, work_item_id)
    }

    #[test]
    fn room_authority_and_assignment_are_exactly_scoped() {
        let (room, binding_id, player_id, work_item_id) = room_fixture();
        let (paper_id, _, _) = frozen_authority_from_room(&room).unwrap();
        assert_eq!(paper_id, Uuid::from_u128(1));
        let scope =
            author_work_item_scope(&room, paper_id, binding_id, player_id, work_item_id).unwrap();
        assert_eq!(scope.work_item_version, 3);
        assert!(author_work_item_scope(
            &room,
            paper_id,
            Uuid::from_u128(99),
            player_id,
            work_item_id
        )
        .is_err());
        assert!(author_work_item_scope(
            &room,
            paper_id,
            binding_id,
            Uuid::from_u128(99),
            work_item_id
        )
        .is_err());

        for terminal_status in ["accepted", "rejected", "cancelled"] {
            let (mut terminal, _, _, _) = room_fixture();
            terminal["work_items"][0]["status"] = json!(terminal_status);
            assert!(author_work_item_scope(
                &terminal,
                paper_id,
                binding_id,
                player_id,
                work_item_id
            )
            .is_err());
        }

        let (mut noncanonical, _, _, _) = room_fixture();
        noncanonical["work_items"][0]["assigned_binding_id"] =
            json!(binding_id.simple().to_string());
        assert!(author_work_item_scope(
            &noncanonical,
            paper_id,
            binding_id,
            player_id,
            work_item_id
        )
        .is_err());
    }

    #[test]
    fn legacy_or_tampered_snapshot_never_projects_automatic_materials() {
        let (mut room, _, _, _) = room_fixture();
        room["paper"]["challenge_ruleset_snapshot"]
            .as_object_mut()
            .unwrap()
            .remove("material_authority");
        assert!(matches!(
            frozen_authority_from_room(&room),
            Err(AppError::Conflict(_))
        ));

        let (mut room, _, _, _) = room_fixture();
        room["paper"]["challenge_id"] = json!(Uuid::from_u128(99));
        assert!(frozen_authority_from_room(&room).is_err());
    }

    #[test]
    fn browser_projection_is_hash_bound_to_exact_material_descriptors() {
        let (room, _, _, _) = room_fixture();
        let (paper_id, snapshot_hash, authority) = frozen_authority_from_room(&room).unwrap();
        let objects = [
            (
                "brief",
                "playable_brief",
                "challenge/brief.md",
                "text/markdown; charset=utf-8",
            ),
            (
                "dataset",
                "dataset",
                "challenge/dataset.json",
                "application/json",
            ),
            (
                "baseline",
                "baseline_code",
                "challenge/baseline.py",
                "text/x-python; charset=utf-8",
            ),
            (
                "evaluator",
                "frozen_evaluator",
                "challenge/evaluator.py",
                "text/x-python; charset=utf-8",
            ),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (object_key, role, logical_path, media_type))| {
            AssignedChallengeMaterialObjectV1 {
                object_key: object_key.into(),
                source_path: format!("source/{object_key}"),
                logical_path: logical_path.into(),
                role: role.into(),
                digest: format!("sha256:{}", format!("{index:x}").repeat(64)),
                size_bytes: (index + 1) as u64,
                media_type: media_type.into(),
                download_path: "/api/agent-bridge/challenge-objects".into(),
            }
        })
        .collect::<Vec<_>>();
        let projection =
            browser_material_projection(paper_id, &snapshot_hash, &authority, &objects).unwrap();
        let hash = projection["projection_hash"].as_str().unwrap().to_string();
        let mut frame = projection.clone();
        frame.as_object_mut().unwrap().remove("projection_hash");
        assert_eq!(canonical_json_sha256(&frame).unwrap(), hash);

        frame["objects"][0]["digest"] = json!(format!("sha256:{}", "f".repeat(64)));
        assert_ne!(canonical_json_sha256(&frame).unwrap(), hash);
    }
}
