use super::*;

pub(super) fn identity_binding_metadata_json(metadata: &IdentityBindingMetadata) -> Value {
    json!({
        "format": metadata.format.clone(),
        "version": metadata.version,
        "revision": metadata.revision.clone(),
        "source_path": metadata.source_path.clone(),
        "source_modified_epoch": metadata.source_modified_epoch,
        "loaded_at_epoch": metadata.loaded_at_epoch,
        "load_status": metadata.load_status.clone(),
        "load_error": metadata.load_error.clone(),
    })
}

pub(super) fn identity_binding_audit_json(audit_state: &IdentityBindingAuditState) -> Value {
    json!({
        "path": audit_state.path.clone(),
        "last_event_kind": audit_state.last_event_kind.clone(),
        "last_event_epoch": audit_state.last_event_epoch,
        "last_status": audit_state.last_status.clone(),
        "last_error": audit_state.last_error.clone(),
        "last_policy_decision": audit_state.last_policy_decision.clone(),
        "last_policy_reason": audit_state.last_policy_reason.clone(),
    })
}

pub(super) fn identity_source_of_truth_json(store: &IdentityBindingStore) -> Value {
    json!({
        "mode": identity_source_of_truth_mode(store),
        "product_users": store.product_users.len(),
        "missing_product_user_refs": count_missing_product_user_refs(store),
    })
}

pub(super) fn identity_reload_policy_json(config: &ConsumerEntryConfig) -> Value {
    json!({
        "require_revision": config.identity_binding_reload_require_revision,
        "reject_same_revision": config.identity_binding_reload_reject_same_revision,
        "allow_legacy_format": config.identity_binding_reload_allow_legacy_format,
        "require_approved_revision": config.identity_binding_reload_require_approved_revision,
        "allow_rollback": config.identity_binding_reload_allow_rollback,
        "require_actor": config.identity_binding_reload_require_actor,
        "actor_header": config.identity_binding_reload_actor_header,
        "allowed_actors_count": config.identity_binding_reload_allowed_actors.len(),
    })
}

pub(super) fn identity_actor_checks_json(config: &ConsumerEntryConfig) -> Value {
    let actor_header_valid = !config
        .identity_binding_reload_actor_header
        .trim()
        .is_empty();
    let status = if !config.identity_binding_reload_require_actor {
        "disabled"
    } else if !actor_header_valid {
        "actor_header_missing"
    } else if config.identity_binding_reload_allowed_actors.is_empty() {
        "no_allowed_actors_configured"
    } else {
        "ok"
    };
    let valid = matches!(status, "disabled" | "ok");

    json!({
        "status": status,
        "valid": valid,
        "require_actor": config.identity_binding_reload_require_actor,
        "actor_header": config.identity_binding_reload_actor_header,
        "actor_header_valid": actor_header_valid,
        "allowed_actor_count": config.identity_binding_reload_allowed_actors.len(),
        "allowed_actors": config.identity_binding_reload_allowed_actors,
    })
}

pub(super) fn current_effective_identity_revision(
    config: &ConsumerEntryConfig,
    store: &IdentityBindingStore,
) -> Option<String> {
    effective_identity_revision(
        &store.metadata,
        &store.registry_metadata,
        config.identity_registry_path.is_some(),
    )
}

pub(super) fn identity_approval_checks_json(
    config: &ConsumerEntryConfig,
    store: &IdentityBindingStore,
    approval_state: &IdentityBindingRevisionApprovalState,
) -> Value {
    let current_effective_revision = current_effective_identity_revision(config, store);
    let current_effective_revision_index =
        current_effective_revision.as_ref().and_then(|revision| {
            approval_state
                .approved_revisions
                .iter()
                .position(|approved| approved == revision)
        });
    let latest_approved_revision = approval_state.approved_revisions.last().cloned();
    let latest_approved_revision_index = approval_state.approved_revisions.len().checked_sub(1);
    let current_effective_revision_approved = current_effective_revision.as_ref().map(|revision| {
        approval_state
            .approved_revisions
            .iter()
            .any(|approved| approved == revision)
    });
    let current_matches_latest_approved = current_effective_revision
        .as_ref()
        .zip(latest_approved_revision.as_ref())
        .map(|(current, latest)| current == latest);
    let status = if config.identity_binding_approved_revisions_path.is_none() {
        "approval_not_configured"
    } else if approval_state.load_status != "loaded" {
        "approval_state_not_loaded"
    } else if current_effective_revision.is_none() {
        "current_effective_revision_missing"
    } else if current_effective_revision_approved != Some(true) {
        "current_effective_revision_not_approved"
    } else {
        "ok"
    };

    json!({
        "configured": config.identity_binding_approved_revisions_path.is_some(),
        "status": status,
        "current_effective_revision": current_effective_revision,
        "current_effective_revision_approved": current_effective_revision_approved,
        "current_effective_revision_index": current_effective_revision_index,
        "latest_approved_revision": latest_approved_revision,
        "latest_approved_revision_index": latest_approved_revision_index,
        "current_matches_latest_approved": current_matches_latest_approved,
        "approved_revision_count": approval_state.approved_revisions.len(),
        "approval_state_loaded": approval_state.load_status == "loaded",
        "rollback_order_available": current_effective_revision_index.is_some() && !approval_state.approved_revisions.is_empty(),
    })
}

pub(super) fn identity_approval_source_json(
    config: &ConsumerEntryConfig,
    store: &IdentityBindingStore,
    approval_state: &IdentityBindingRevisionApprovalState,
    limit: usize,
) -> Value {
    let current_effective_revision = current_effective_identity_revision(config, store);
    let current_effective_revision_index =
        current_effective_revision.as_ref().and_then(|revision| {
            approval_state
                .approved_revisions
                .iter()
                .position(|approved| approved == revision)
        });
    let latest_approved_revision = approval_state.approved_revisions.last().cloned();
    let latest_approved_revision_index = approval_state.approved_revisions.len().checked_sub(1);
    let revisions = approval_state
        .approved_revisions
        .iter()
        .enumerate()
        .rev()
        .take(limit)
        .map(|(index, revision)| {
            json!({
                "index": index,
                "revision": revision,
                "is_latest": Some(index) == latest_approved_revision_index,
                "is_current_effective": current_effective_revision
                    .as_ref()
                    .map(|current| current == revision)
                    .unwrap_or(false),
            })
        })
        .collect::<Vec<_>>();
    let status = if config.identity_binding_approved_revisions_path.is_none() {
        "approval_not_configured"
    } else if approval_state.load_status != "loaded" {
        "approval_state_not_loaded"
    } else if approval_state.approved_revisions.is_empty() {
        "approved_revision_set_empty"
    } else {
        "ok"
    };

    json!({
        "configured": config.identity_binding_approved_revisions_path.is_some(),
        "status": status,
        "valid": status == "ok",
        "source_path": approval_state.source_path,
        "source_modified_epoch": approval_state.source_modified_epoch,
        "loaded_at_epoch": approval_state.loaded_at_epoch,
        "load_status": approval_state.load_status,
        "load_error": approval_state.load_error,
        "version": approval_state.version,
        "revision": approval_state.revision,
        "limit": limit,
        "returned_order": "latest_first",
        "approved_revision_count": approval_state.approved_revisions.len(),
        "returned_revision_count": revisions.len(),
        "latest_approved_revision": latest_approved_revision,
        "latest_approved_revision_index": latest_approved_revision_index,
        "current_effective_revision": current_effective_revision,
        "current_effective_revision_index": current_effective_revision_index,
        "current_effective_revision_approved": current_effective_revision_index.is_some(),
        "revisions": revisions,
    })
}

pub(super) fn identity_governance_overview_json(
    config: &ConsumerEntryConfig,
    store: &IdentityBindingStore,
    approval_state: &IdentityBindingRevisionApprovalState,
    audit_state: &IdentityBindingAuditState,
    approval_limit: usize,
) -> Value {
    let binding_loaded = store.metadata.load_status == "loaded";
    let registry_configured = config.identity_registry_path.is_some();
    let registry_loaded = !registry_configured || store.registry_metadata.load_status == "loaded";
    let missing_product_user_refs = count_missing_product_user_refs(store);
    let actor_checks = identity_actor_checks_json(config);
    let actor_valid = actor_checks
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let approval_checks = identity_approval_checks_json(config, store, approval_state);
    let approval_coverage_status = approval_checks
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("approval_state_not_loaded");
    let approval_coverage_valid = approval_coverage_status == "ok";
    let approval_source =
        identity_approval_source_json(config, store, approval_state, approval_limit);
    let approval_source_valid = approval_source
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let actor_status = actor_checks
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("actor_header_missing");
    let approval_source_status = approval_source
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("approval_state_not_loaded");
    let status = if !binding_loaded {
        "identity_bindings_not_loaded"
    } else if !registry_loaded {
        "identity_registry_not_loaded"
    } else if missing_product_user_refs > 0 {
        "missing_product_user_refs"
    } else if !actor_valid {
        actor_status
    } else if !approval_source_valid {
        approval_source_status
    } else if !approval_coverage_valid {
        approval_coverage_status
    } else {
        "ok"
    };

    json!({
        "status": status,
        "valid": status == "ok",
        "effective_revision": current_effective_identity_revision(config, store),
        "registry_configured": registry_configured,
        "missing_product_user_refs": missing_product_user_refs,
        "checks": {
            "binding_loaded": binding_loaded,
            "registry_loaded": registry_loaded,
            "ref_integrity_ok": missing_product_user_refs == 0,
            "actor_gate_valid": actor_valid,
            "approval_source_valid": approval_source_valid,
            "approval_coverage_valid": approval_coverage_valid,
        },
        "identity_binding_metadata": identity_binding_metadata_json(&store.metadata),
        "identity_registry_metadata": identity_binding_metadata_json(&store.registry_metadata),
        "identity_binding_counts": identity_binding_counts_json(store),
        "identity_source_of_truth": identity_source_of_truth_json(store),
        "identity_binding_reload_policy": identity_reload_policy_json(config),
        "identity_binding_audit": identity_binding_audit_json(audit_state),
        "identity_actor_checks": actor_checks,
        "identity_binding_revision_approval": approval_state,
        "identity_approval_checks": approval_checks,
        "identity_approval_source": approval_source,
    })
}

pub(super) fn identity_admin_snapshot_json(
    config: &ConsumerEntryConfig,
    store: &IdentityBindingStore,
    approval_state: &IdentityBindingRevisionApprovalState,
) -> Value {
    json!({
        "registry_configured": config.identity_registry_path.is_some(),
        "effective_revision": current_effective_identity_revision(config, store),
        "missing_product_user_refs": count_missing_product_user_refs(store),
        "identity_binding_metadata": identity_binding_metadata_json(&store.metadata),
        "identity_registry_metadata": identity_binding_metadata_json(&store.registry_metadata),
        "identity_binding_counts": identity_binding_counts_json(store),
        "identity_source_of_truth": identity_source_of_truth_json(store),
        "identity_binding_reload_policy": identity_reload_policy_json(config),
        "identity_actor_checks": identity_actor_checks_json(config),
        "identity_binding_revision_approval": approval_state,
        "identity_approval_checks": identity_approval_checks_json(config, store, approval_state),
    })
}

pub(super) fn session_auth_issuer_registry_active_key_rows(
    registry: &HashMap<String, SessionAuthIssuerRegistryIssuer>,
) -> Vec<Value> {
    let mut rows = registry
        .iter()
        .map(|(issuer, entry)| {
            let mut key_ids = entry.keys.keys().cloned().collect::<Vec<_>>();
            key_ids.sort();
            let active_key_id = entry
                .active_key_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string);
            let active_key_present = active_key_id
                .as_ref()
                .map(|key_id| entry.keys.contains_key(key_id))
                .unwrap_or(false);
            json!({
                "issuer": issuer,
                "active_key_id": active_key_id,
                "active_key_present": active_key_present,
                "key_count": key_ids.len(),
                "key_ids": key_ids,
            })
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.get("issuer")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .cmp(
                right
                    .get("issuer")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
    });
    rows
}

pub(super) fn session_auth_issuer_registry_active_key_diff_json(
    current: &HashMap<String, SessionAuthIssuerRegistryIssuer>,
    candidate: &HashMap<String, SessionAuthIssuerRegistryIssuer>,
) -> Value {
    let mut issuers = current
        .keys()
        .chain(candidate.keys())
        .cloned()
        .collect::<Vec<_>>();
    issuers.sort();
    issuers.dedup();

    let mut changes = Vec::new();
    for issuer in issuers {
        let current_active_key_id = current
            .get(&issuer)
            .and_then(|entry| entry.active_key_id.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        let candidate_active_key_id = candidate
            .get(&issuer)
            .and_then(|entry| entry.active_key_id.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        if current_active_key_id == candidate_active_key_id {
            continue;
        }
        let status = match (
            current_active_key_id.as_ref(),
            candidate_active_key_id.as_ref(),
        ) {
            (None, Some(_)) => "added",
            (Some(_), None) => "removed",
            (Some(_), Some(_)) => "changed",
            (None, None) => continue,
        };
        changes.push(json!({
            "issuer": issuer,
            "status": status,
            "current_active_key_id": current_active_key_id,
            "candidate_active_key_id": candidate_active_key_id,
        }));
    }

    json!({
        "changed_active_key_count": changes.len(),
        "matches": changes.is_empty(),
        "changes": changes,
    })
}

pub(super) fn session_auth_issuer_registry_status_json(
    config: &ConsumerEntryConfig,
    metadata: &SessionAuthIssuerRegistryMetadata,
    registry: &HashMap<String, SessionAuthIssuerRegistryIssuer>,
) -> Value {
    let mut issuers = registry.keys().cloned().collect::<Vec<_>>();
    issuers.sort();

    let mut issuers_without_active_key = registry
        .iter()
        .filter_map(|(issuer, entry)| {
            entry
                .active_key_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
                .then_some(issuer.clone())
        })
        .collect::<Vec<_>>();
    issuers_without_active_key.sort();

    let mut allowed_issuers_in_registry = config
        .session_auth_allowed_issuers
        .iter()
        .filter(|issuer| registry.contains_key(issuer.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    allowed_issuers_in_registry.sort();

    let mut allowed_issuers_missing = config
        .session_auth_allowed_issuers
        .iter()
        .filter(|issuer| !registry.contains_key(issuer.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    allowed_issuers_missing.sort();

    let issuer_active_keys = session_auth_issuer_registry_active_key_rows(registry);
    let status = if config.session_auth_issuer_registry_path.is_none() {
        "disabled".to_string()
    } else if metadata.load_status != "loaded" {
        metadata.load_status.clone()
    } else if metadata.issuer_count == 0 {
        "empty".to_string()
    } else {
        "ok".to_string()
    };
    let valid = status == "ok";

    json!({
        "status": status,
        "valid": valid,
        "configured": config.session_auth_issuer_registry_path.is_some(),
        "loaded": metadata.load_status == "loaded",
        "metadata": metadata,
        "issuer_count": metadata.issuer_count,
        "key_count": metadata.key_count,
        "issuers": issuers,
        "active_key_issuer_count": metadata.issuer_count.saturating_sub(issuers_without_active_key.len()),
        "issuers_without_active_key": issuers_without_active_key,
        "issuer_active_keys": issuer_active_keys,
        "allowed_issuers": config.session_auth_allowed_issuers,
        "allowed_issuer_count": config.session_auth_allowed_issuers.len(),
        "allowed_issuers_in_registry": allowed_issuers_in_registry,
        "allowed_issuers_missing": allowed_issuers_missing,
        "expected_audience": config.session_auth_expected_audience,
    })
}

pub(super) fn session_auth_issuer_registry_actor_checks_json(
    config: &ConsumerEntryConfig,
) -> Value {
    let actor_header_valid = !config
        .session_auth_issuer_registry_actor_header
        .trim()
        .is_empty();
    let status = if !config.session_auth_issuer_registry_require_actor {
        "disabled"
    } else if !actor_header_valid {
        "actor_header_missing"
    } else if config
        .session_auth_issuer_registry_allowed_actors
        .is_empty()
    {
        "no_allowed_actors_configured"
    } else {
        "ok"
    };
    let valid = matches!(status, "disabled" | "ok");

    json!({
        "status": status,
        "valid": valid,
        "require_actor": config.session_auth_issuer_registry_require_actor,
        "actor_header": config.session_auth_issuer_registry_actor_header,
        "actor_header_valid": actor_header_valid,
        "allowed_actor_count": config.session_auth_issuer_registry_allowed_actors.len(),
        "allowed_actors": config.session_auth_issuer_registry_allowed_actors,
    })
}

pub(super) fn session_auth_issuer_registry_actor_request_json(
    config: &ConsumerEntryConfig,
    requesting_actor: Option<&str>,
) -> Value {
    let actor_checks = session_auth_issuer_registry_actor_checks_json(config);
    let normalized_actor = requesting_actor
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let actor_header = actor_checks
        .get("actor_header")
        .and_then(Value::as_str)
        .unwrap_or(config.session_auth_issuer_registry_actor_header.as_str());
    let actor_checks_status = actor_checks
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("actor_header_missing");

    let (authorized, status, reason) = if !config.session_auth_issuer_registry_require_actor {
        (None, "disabled".to_string(), None)
    } else if actor_checks_status != "ok" {
        (
            Some(false),
            actor_checks_status.to_string(),
            Some(actor_checks_status.to_string()),
        )
    } else if let Some(actor) = normalized_actor.as_deref() {
        if config
            .session_auth_issuer_registry_allowed_actors
            .iter()
            .any(|allowed| allowed == actor)
        {
            (Some(true), "ok".to_string(), None)
        } else {
            (
                Some(false),
                "actor_not_allowed".to_string(),
                Some("actor_not_allowed".to_string()),
            )
        }
    } else {
        (
            Some(false),
            "actor_missing".to_string(),
            Some("actor_missing".to_string()),
        )
    };

    json!({
        "required": config.session_auth_issuer_registry_require_actor,
        "actor_header": actor_header,
        "request_actor": normalized_actor,
        "authorized": authorized,
        "status": status,
        "reason": reason,
    })
}

pub(super) fn session_auth_issuer_registry_current_revision(
    metadata: &SessionAuthIssuerRegistryMetadata,
) -> Option<String> {
    metadata
        .revision
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub(super) fn session_auth_issuer_registry_approval_checks_json(
    config: &ConsumerEntryConfig,
    metadata: &SessionAuthIssuerRegistryMetadata,
    approval_state: &SessionAuthIssuerRegistryRevisionApprovalState,
) -> Value {
    let current_revision = session_auth_issuer_registry_current_revision(metadata);
    let current_revision_index = current_revision.as_ref().and_then(|revision| {
        approval_state
            .approved_revisions
            .iter()
            .position(|approved| approved == revision)
    });
    let latest_approved_revision = approval_state.approved_revisions.last().cloned();
    let latest_approved_revision_index = approval_state.approved_revisions.len().checked_sub(1);
    let current_revision_approved = current_revision.as_ref().map(|revision| {
        approval_state
            .approved_revisions
            .iter()
            .any(|approved| approved == revision)
    });
    let current_matches_latest_approved = current_revision
        .as_ref()
        .zip(latest_approved_revision.as_ref())
        .map(|(current, latest)| current == latest);
    let status = if config
        .session_auth_issuer_registry_approved_revisions_path
        .is_none()
    {
        "approval_not_configured"
    } else if approval_state.load_status != "loaded" {
        "approval_state_not_loaded"
    } else if current_revision.is_none() {
        "current_revision_missing"
    } else if current_revision_approved != Some(true) {
        "current_revision_not_approved"
    } else {
        "ok"
    };

    json!({
        "configured": config
            .session_auth_issuer_registry_approved_revisions_path
            .is_some(),
        "required": config.session_auth_issuer_registry_require_approved_revision,
        "status": status,
        "valid": status == "ok",
        "current_revision": current_revision,
        "current_revision_approved": current_revision_approved,
        "current_revision_index": current_revision_index,
        "latest_approved_revision": latest_approved_revision,
        "latest_approved_revision_index": latest_approved_revision_index,
        "current_matches_latest_approved": current_matches_latest_approved,
        "approved_revision_count": approval_state.approved_revisions.len(),
        "approval_state_loaded": approval_state.load_status == "loaded",
    })
}

pub(super) fn session_auth_issuer_registry_approval_source_json(
    config: &ConsumerEntryConfig,
    metadata: &SessionAuthIssuerRegistryMetadata,
    approval_state: &SessionAuthIssuerRegistryRevisionApprovalState,
    limit: usize,
) -> Value {
    let current_revision = session_auth_issuer_registry_current_revision(metadata);
    let current_revision_index = current_revision.as_ref().and_then(|revision| {
        approval_state
            .approved_revisions
            .iter()
            .position(|approved| approved == revision)
    });
    let latest_approved_revision = approval_state.approved_revisions.last().cloned();
    let latest_approved_revision_index = approval_state.approved_revisions.len().checked_sub(1);
    let revisions = approval_state
        .approved_revisions
        .iter()
        .enumerate()
        .rev()
        .take(limit)
        .map(|(index, revision)| {
            json!({
                "index": index,
                "revision": revision,
                "is_latest": Some(index) == latest_approved_revision_index,
                "is_current": current_revision
                    .as_ref()
                    .map(|current| current == revision)
                    .unwrap_or(false),
            })
        })
        .collect::<Vec<_>>();
    let status = if config
        .session_auth_issuer_registry_approved_revisions_path
        .is_none()
    {
        "approval_not_configured"
    } else if approval_state.load_status != "loaded" {
        "approval_state_not_loaded"
    } else if approval_state.approved_revisions.is_empty() {
        "approved_revision_set_empty"
    } else {
        "ok"
    };

    json!({
        "configured": config
            .session_auth_issuer_registry_approved_revisions_path
            .is_some(),
        "required": config.session_auth_issuer_registry_require_approved_revision,
        "status": status,
        "valid": status == "ok",
        "source_path": approval_state.source_path,
        "source_modified_epoch": approval_state.source_modified_epoch,
        "loaded_at_epoch": approval_state.loaded_at_epoch,
        "load_status": approval_state.load_status,
        "load_error": approval_state.load_error,
        "version": approval_state.version,
        "revision": approval_state.revision,
        "limit": limit,
        "returned_order": "latest_first",
        "approved_revision_count": approval_state.approved_revisions.len(),
        "returned_revision_count": revisions.len(),
        "latest_approved_revision": latest_approved_revision,
        "latest_approved_revision_index": latest_approved_revision_index,
        "current_revision": current_revision,
        "current_revision_index": current_revision_index,
        "current_revision_approved": current_revision_index.is_some(),
        "revisions": revisions,
    })
}

pub(super) fn session_auth_issuer_registry_governance_overview_json(
    config: &ConsumerEntryConfig,
    metadata: &SessionAuthIssuerRegistryMetadata,
    approval_state: &SessionAuthIssuerRegistryRevisionApprovalState,
    approval_limit: usize,
) -> Value {
    let registry_configured = config.session_auth_issuer_registry_path.is_some();
    let approval_source_configured = config
        .session_auth_issuer_registry_approved_revisions_path
        .is_some();
    let registry_loaded = !registry_configured || metadata.load_status == "loaded";
    let revision_present = !registry_configured || metadata.revision.is_some();
    let actor_checks = session_auth_issuer_registry_actor_checks_json(config);
    let actor_gate_valid = if !registry_configured {
        true
    } else {
        actor_checks
            .get("valid")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    let actor_gate_status = actor_checks
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("actor_header_missing");
    let approval_checks =
        session_auth_issuer_registry_approval_checks_json(config, metadata, approval_state);
    let approval_source = session_auth_issuer_registry_approval_source_json(
        config,
        metadata,
        approval_state,
        approval_limit,
    );
    let approval_source_valid = if !registry_configured {
        true
    } else if !approval_source_configured {
        !config.session_auth_issuer_registry_require_approved_revision
    } else {
        approval_source
            .get("valid")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    let approval_coverage_valid =
        if !registry_configured || !config.session_auth_issuer_registry_require_approved_revision {
            true
        } else {
            approval_checks
                .get("valid")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        };
    let approval_source_status = approval_source
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("approval_state_not_loaded");
    let approval_coverage_status = approval_checks
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("approval_state_not_loaded");
    let status = if !config.require_session_auth {
        "session_auth_disabled"
    } else if !registry_configured {
        "issuer_registry_not_configured"
    } else if !registry_loaded {
        "issuer_registry_not_loaded"
    } else if !revision_present {
        "issuer_registry_revision_missing"
    } else if !actor_gate_valid {
        actor_gate_status
    } else if !approval_source_valid {
        approval_source_status
    } else if !approval_coverage_valid {
        approval_coverage_status
    } else {
        "ok"
    };

    json!({
        "status": status,
        "valid": status == "ok" || status == "issuer_registry_not_configured" || status == "session_auth_disabled",
        "session_auth_required": config.require_session_auth,
        "configured": registry_configured,
        "approval_required": config.session_auth_issuer_registry_require_approved_revision,
        "current_revision": session_auth_issuer_registry_current_revision(metadata),
        "checks": {
            "registry_loaded": registry_loaded,
            "revision_present": revision_present,
            "actor_gate_valid": actor_gate_valid,
            "approval_source_valid": approval_source_valid,
            "approval_coverage_valid": approval_coverage_valid,
        },
        "issuer_registry_metadata": metadata,
        "issuer_registry_actor_checks": actor_checks,
        "issuer_registry_approval": approval_state,
        "issuer_registry_approval_checks": approval_checks,
        "issuer_registry_approval_source": approval_source,
    })
}

pub(super) fn normalize_identity_approval_limit(limit: Option<usize>) -> usize {
    limit.unwrap_or(20).clamp(1, 100)
}

pub(super) fn normalize_identity_audit_limit(limit: Option<usize>) -> usize {
    limit.unwrap_or(20).clamp(1, 100)
}

pub(super) fn is_registry_audit_event_kind(kind: &str) -> bool {
    matches!(kind, "registry_reload" | "registry_reload_rejected")
}

pub(super) fn read_registry_audit_events(
    path: &str,
    limit: usize,
) -> Result<(Vec<Value>, usize), String> {
    let raw = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
    let mut events = Vec::new();
    let mut parse_error_count = 0;

    for line in raw.lines().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value = match serde_json::from_str::<Value>(trimmed) {
            Ok(value) => value,
            Err(_) => {
                parse_error_count += 1;
                continue;
            }
        };
        let is_registry_event = value
            .get("event_kind")
            .and_then(Value::as_str)
            .map(is_registry_audit_event_kind)
            .unwrap_or(false);
        if !is_registry_event {
            continue;
        }
        events.push(value);
        if events.len() >= limit {
            break;
        }
    }

    Ok((events, parse_error_count))
}

pub(super) fn identity_reload_response_json(
    ok: bool,
    reloaded: bool,
    store: &IdentityBindingStore,
    audit_state: &IdentityBindingAuditState,
    governance: &IdentityBindingReloadGovernance,
    approval_state: &IdentityBindingRevisionApprovalState,
) -> Value {
    json!({
        "ok": ok,
        "reloaded": reloaded,
        "identity_binding_metadata": identity_binding_metadata_json(&store.metadata),
        "identity_registry_metadata": identity_binding_metadata_json(&store.registry_metadata),
        "identity_binding_counts": identity_binding_counts_json(store),
        "identity_source_of_truth": identity_source_of_truth_json(store),
        "identity_binding_audit": identity_binding_audit_json(audit_state),
        "identity_binding_reload_governance": governance,
        "identity_binding_revision_approval": approval_state,
    })
}

pub(super) async fn apply_identity_store_reload(
    state: &AppState,
    headers: &HeaderMap,
    reloaded_store: IdentityBindingStore,
    accepted_event_kind: &str,
    rejected_event_kind: &str,
) -> Response {
    if let Err(response) = authorize_ingress(headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    state.inner.metrics.inc_identity_binding_reload_requests();
    let current_store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let requesting_actor = headers
        .get(state.config().identity_binding_reload_actor_header.as_str())
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let governance = evaluate_identity_binding_reload_governance(
        state.config(),
        &current_store,
        &reloaded_store,
        &approval_state,
        requesting_actor.as_deref(),
    );
    let event_kind = if governance.accepted {
        accepted_event_kind
    } else {
        rejected_event_kind
    };
    let audit_state = append_identity_binding_audit_event(
        state.config(),
        event_kind,
        &reloaded_store,
        Some(&governance),
    );
    if audit_state.last_status != "written"
        && state.config().identity_binding_audit_log_path.is_some()
    {
        state.inner.metrics.inc_identity_binding_audit_failures();
    }

    let response_body = if governance.accepted {
        identity_reload_response_json(
            true,
            true,
            &reloaded_store,
            &audit_state,
            &governance,
            &approval_state,
        )
    } else {
        identity_reload_response_json(
            false,
            false,
            &reloaded_store,
            &audit_state,
            &governance,
            &approval_state,
        )
    };

    {
        let mut audit_state_guard = state.inner.identity_binding_audit_state.write().await;
        *audit_state_guard = audit_state;
    }

    if !governance.accepted {
        state.inner.metrics.inc_identity_binding_reload_rejections();
        if governance.actor_authorized == Some(false) {
            state
                .inner
                .metrics
                .inc_identity_binding_reload_actor_rejections();
            return (StatusCode::FORBIDDEN, Json(response_body)).into_response();
        }
        return (StatusCode::CONFLICT, Json(response_body)).into_response();
    }

    state.inner.metrics.inc_identity_binding_reload_successes();
    {
        let mut store = state.inner.identity_binding_store.write().await;
        *store = reloaded_store;
    }

    (StatusCode::OK, Json(response_body)).into_response()
}

pub(super) async fn reload_identity_bindings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let reloaded_store = load_identity_binding_store(state.config());
    apply_identity_store_reload(
        &state,
        &headers,
        reloaded_store,
        "reload",
        "reload_rejected",
    )
    .await
}

pub(super) async fn reload_identity_registry(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let Some(_) = state.config().identity_registry_path.as_deref() else {
        if let Err(response) = authorize_ingress(&headers, state.config()) {
            state.inner.metrics.inc_ingress_auth_failures();
            return response;
        }
        let current_store = {
            let store = state.inner.identity_binding_store.read().await;
            store.clone()
        };
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "ok": false,
                "reloaded": false,
                "error": "identity_registry_not_configured",
                "identity_binding_metadata": identity_binding_metadata_json(&current_store.metadata),
                "identity_registry_metadata": identity_binding_metadata_json(&current_store.registry_metadata),
                "identity_binding_counts": identity_binding_counts_json(&current_store),
                "identity_source_of_truth": identity_source_of_truth_json(&current_store),
            })),
        )
            .into_response();
    };

    let current_store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let (registry_metadata, product_users) = load_product_user_registry(
        state.config().identity_registry_path.as_deref(),
        current_store.product_users.clone(),
        &current_store.metadata,
    );
    let reloaded_store = IdentityBindingStore {
        registry_metadata,
        product_users,
        ..current_store
    };

    apply_identity_store_reload(
        &state,
        &headers,
        reloaded_store,
        "registry_reload",
        "registry_reload_rejected",
    )
    .await
}

pub(super) async fn validate_identity_registry(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let Some(_) = state.config().identity_registry_path.as_deref() else {
        let current_store = {
            let store = state.inner.identity_binding_store.read().await;
            store.clone()
        };
        let approval_state = load_identity_binding_revision_approval_state(state.config());
        let mut body =
            identity_admin_snapshot_json(state.config(), &current_store, &approval_state);
        let object = body
            .as_object_mut()
            .expect("identity admin snapshot should be json object");
        object.insert("ok".to_string(), json!(false));
        object.insert("validated".to_string(), json!(true));
        object.insert("valid".to_string(), json!(false));
        object.insert("would_reload".to_string(), json!(false));
        object.insert(
            "error".to_string(),
            json!("identity_registry_not_configured"),
        );
        return (StatusCode::CONFLICT, Json(body)).into_response();
    };

    let current_store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let (registry_metadata, product_users) = load_product_user_registry(
        state.config().identity_registry_path.as_deref(),
        current_store.product_users.clone(),
        &current_store.metadata,
    );
    let candidate_store = IdentityBindingStore {
        registry_metadata,
        product_users,
        ..current_store.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let requesting_actor = headers
        .get(state.config().identity_binding_reload_actor_header.as_str())
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let governance = evaluate_identity_binding_reload_governance(
        state.config(),
        &current_store,
        &candidate_store,
        &approval_state,
        requesting_actor.as_deref(),
    );
    let mut body = identity_admin_snapshot_json(state.config(), &candidate_store, &approval_state);
    let object = body
        .as_object_mut()
        .expect("identity admin snapshot should be json object");
    object.insert("ok".to_string(), json!(governance.accepted));
    object.insert("validated".to_string(), json!(true));
    object.insert("valid".to_string(), json!(governance.accepted));
    object.insert("would_reload".to_string(), json!(governance.accepted));
    object.insert("checked_only".to_string(), json!(true));
    object.insert(
        "identity_binding_reload_governance".to_string(),
        serde_json::to_value(&governance).expect("serialize governance"),
    );

    if !governance.accepted {
        if governance.actor_authorized == Some(false) {
            return (StatusCode::FORBIDDEN, Json(body)).into_response();
        }
        return (StatusCode::CONFLICT, Json(body)).into_response();
    }

    (StatusCode::OK, Json(body)).into_response()
}

pub(super) async fn get_identity_registry_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let audit = {
        let audit = state.inner.identity_binding_audit_state.read().await;
        audit.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
    let object = body
        .as_object_mut()
        .expect("identity admin snapshot should be json object");
    object.insert("ok".to_string(), json!(true));
    object.insert("status".to_string(), json!("ok"));
    object.insert(
        "identity_binding_audit".to_string(),
        identity_binding_audit_json(&audit),
    );

    (StatusCode::OK, Json(body)).into_response()
}

pub(super) async fn get_identity_registry_audit(
    State(state): State<AppState>,
    Query(query): Query<IdentityAuditQuery>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let audit = {
        let audit = state.inner.identity_binding_audit_state.read().await;
        audit.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let Some(path) = state.config().identity_binding_audit_log_path.as_deref() else {
        let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
        let object = body
            .as_object_mut()
            .expect("identity admin snapshot should be json object");
        object.insert("ok".to_string(), json!(false));
        object.insert("status".to_string(), json!("error"));
        object.insert("error".to_string(), json!("identity_audit_not_configured"));
        object.insert(
            "identity_binding_audit".to_string(),
            identity_binding_audit_json(&audit),
        );
        return (StatusCode::CONFLICT, Json(body)).into_response();
    };

    let limit = normalize_identity_audit_limit(query.limit);
    let (events, parse_error_count) = match read_registry_audit_events(path, limit) {
        Ok(result) => result,
        Err(err) => {
            let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
            let object = body
                .as_object_mut()
                .expect("identity admin snapshot should be json object");
            object.insert("ok".to_string(), json!(false));
            object.insert("status".to_string(), json!("error"));
            object.insert("error".to_string(), json!("identity_audit_read_error"));
            object.insert("error_detail".to_string(), json!(err));
            object.insert(
                "identity_binding_audit".to_string(),
                identity_binding_audit_json(&audit),
            );
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response();
        }
    };

    let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
    let object = body
        .as_object_mut()
        .expect("identity admin snapshot should be json object");
    object.insert("ok".to_string(), json!(true));
    object.insert("status".to_string(), json!("ok"));
    object.insert("audit_path".to_string(), json!(path));
    object.insert("limit".to_string(), json!(limit));
    object.insert("returned_event_count".to_string(), json!(events.len()));
    object.insert("parse_error_count".to_string(), json!(parse_error_count));
    object.insert("events".to_string(), json!(events));
    object.insert(
        "identity_binding_audit".to_string(),
        identity_binding_audit_json(&audit),
    );

    (StatusCode::OK, Json(body)).into_response()
}

pub(super) async fn get_session_auth_issuer_registry_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let current_state = session_auth_issuer_registry_runtime_state(&state);
    let status = session_auth_issuer_registry_status_json(
        state.config(),
        &current_state.metadata,
        &current_state.registry,
    );
    let approval_state = load_session_auth_issuer_registry_revision_approval_state(state.config());
    let actor_checks = session_auth_issuer_registry_actor_checks_json(state.config());
    let approval_checks = session_auth_issuer_registry_approval_checks_json(
        state.config(),
        &current_state.metadata,
        &approval_state,
    );
    let approval_source = session_auth_issuer_registry_approval_source_json(
        state.config(),
        &current_state.metadata,
        &approval_state,
        20,
    );

    let body = json!({
        "ok": true,
        "status": "ok",
        "require_session_auth": state.config().require_session_auth,
        "session_auth_issuer_registry": status,
        "session_auth_issuer_registry_actor_checks": actor_checks,
        "session_auth_issuer_registry_approval": approval_checks,
        "session_auth_issuer_registry_approval_source": approval_source,
    });

    (StatusCode::OK, Json(body)).into_response()
}

pub(super) async fn validate_session_auth_issuer_registry(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let current_state = session_auth_issuer_registry_runtime_state(&state);
    let current_status = session_auth_issuer_registry_status_json(
        state.config(),
        &current_state.metadata,
        &current_state.registry,
    );
    let (candidate_metadata, candidate_registry) = load_session_auth_issuer_registry(
        state.config().session_auth_issuer_registry_path.as_deref(),
    );
    let candidate_status = session_auth_issuer_registry_status_json(
        state.config(),
        &candidate_metadata,
        &candidate_registry,
    );
    let approval_state = load_session_auth_issuer_registry_revision_approval_state(state.config());
    let actor_checks = session_auth_issuer_registry_actor_checks_json(state.config());
    let requesting_actor = headers
        .get(
            state
                .config()
                .session_auth_issuer_registry_actor_header
                .as_str(),
        )
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let actor_request =
        session_auth_issuer_registry_actor_request_json(state.config(), requesting_actor);
    let current_approval_checks = session_auth_issuer_registry_approval_checks_json(
        state.config(),
        &current_state.metadata,
        &approval_state,
    );
    let current_approval_source = session_auth_issuer_registry_approval_source_json(
        state.config(),
        &current_state.metadata,
        &approval_state,
        20,
    );
    let candidate_approval_checks = session_auth_issuer_registry_approval_checks_json(
        state.config(),
        &candidate_metadata,
        &approval_state,
    );
    let candidate_approval_source = session_auth_issuer_registry_approval_source_json(
        state.config(),
        &candidate_metadata,
        &approval_state,
        20,
    );
    let candidate_registry_valid = candidate_status
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let actor_gate_valid = if state.config().session_auth_issuer_registry_require_actor {
        actor_request
            .get("authorized")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    } else {
        true
    };
    let candidate_approval_valid = candidate_approval_checks
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let validation_status = if !candidate_registry_valid {
        candidate_status
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("error")
            .to_string()
    } else if state.config().session_auth_issuer_registry_require_actor && !actor_gate_valid {
        actor_request
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("actor_missing")
            .to_string()
    } else if state
        .config()
        .session_auth_issuer_registry_require_approved_revision
        && !candidate_approval_valid
    {
        candidate_approval_checks
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("approval_state_not_loaded")
            .to_string()
    } else {
        "ok".to_string()
    };
    let is_valid = candidate_registry_valid
        && (!state.config().session_auth_issuer_registry_require_actor || actor_gate_valid)
        && (!state
            .config()
            .session_auth_issuer_registry_require_approved_revision
            || candidate_approval_valid);
    let active_key_diff = session_auth_issuer_registry_active_key_diff_json(
        &current_state.registry,
        &candidate_registry,
    );
    let matches_loaded_revision = candidate_metadata.revision == current_state.metadata.revision;
    let matches_loaded_key_count = candidate_metadata.key_count == current_state.metadata.key_count;
    let matches_loaded_issuer_count =
        candidate_metadata.issuer_count == current_state.metadata.issuer_count;
    let matches_loaded_status =
        candidate_metadata.load_status == current_state.metadata.load_status;
    let matches_loaded_active_keys = active_key_diff
        .get("matches")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let body = json!({
        "ok": is_valid,
        "validated": true,
        "valid": is_valid,
        "status": validation_status,
        "require_session_auth": state.config().require_session_auth,
        "session_auth_issuer_registry": current_status,
        "session_auth_issuer_registry_source": candidate_status,
        "session_auth_issuer_registry_actor_checks": actor_checks,
        "session_auth_issuer_registry_actor_request": actor_request,
        "session_auth_issuer_registry_approval": current_approval_checks,
        "session_auth_issuer_registry_approval_source": current_approval_source,
        "session_auth_issuer_registry_source_approval": candidate_approval_checks,
        "session_auth_issuer_registry_source_approval_source": candidate_approval_source,
        "session_auth_issuer_registry_active_key_diff": active_key_diff,
        "matches_loaded_status": matches_loaded_status,
        "matches_loaded_revision": matches_loaded_revision,
        "matches_loaded_issuer_count": matches_loaded_issuer_count,
        "matches_loaded_key_count": matches_loaded_key_count,
        "matches_loaded_active_keys": matches_loaded_active_keys,
    });

    if is_valid {
        return (StatusCode::OK, Json(body)).into_response();
    }

    (StatusCode::CONFLICT, Json(body)).into_response()
}

pub(super) async fn reload_session_auth_issuer_registry(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let current_state = session_auth_issuer_registry_runtime_state(&state);
    let current_status = session_auth_issuer_registry_status_json(
        state.config(),
        &current_state.metadata,
        &current_state.registry,
    );
    let actor_checks = session_auth_issuer_registry_actor_checks_json(state.config());

    if state.config().session_auth_issuer_registry_path.is_none() {
        let approval_state =
            load_session_auth_issuer_registry_revision_approval_state(state.config());
        let approval_checks = session_auth_issuer_registry_approval_checks_json(
            state.config(),
            &current_state.metadata,
            &approval_state,
        );
        let approval_source = session_auth_issuer_registry_approval_source_json(
            state.config(),
            &current_state.metadata,
            &approval_state,
            20,
        );
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "ok": false,
                "reloaded": false,
                "error": "session_auth_issuer_registry_not_configured",
                "require_session_auth": state.config().require_session_auth,
                "session_auth_issuer_registry": current_status,
                "session_auth_issuer_registry_actor_checks": actor_checks,
                "session_auth_issuer_registry_approval": approval_checks,
                "session_auth_issuer_registry_approval_source": approval_source,
            })),
        )
            .into_response();
    }

    let (candidate_metadata, candidate_registry) = load_session_auth_issuer_registry(
        state.config().session_auth_issuer_registry_path.as_deref(),
    );
    let candidate_status = session_auth_issuer_registry_status_json(
        state.config(),
        &candidate_metadata,
        &candidate_registry,
    );
    let approval_state = load_session_auth_issuer_registry_revision_approval_state(state.config());
    let requesting_actor = headers
        .get(
            state
                .config()
                .session_auth_issuer_registry_actor_header
                .as_str(),
        )
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let actor_request =
        session_auth_issuer_registry_actor_request_json(state.config(), requesting_actor);
    let current_approval_checks = session_auth_issuer_registry_approval_checks_json(
        state.config(),
        &current_state.metadata,
        &approval_state,
    );
    let current_approval_source = session_auth_issuer_registry_approval_source_json(
        state.config(),
        &current_state.metadata,
        &approval_state,
        20,
    );
    let candidate_approval_checks = session_auth_issuer_registry_approval_checks_json(
        state.config(),
        &candidate_metadata,
        &approval_state,
    );
    let candidate_approval_source = session_auth_issuer_registry_approval_source_json(
        state.config(),
        &candidate_metadata,
        &approval_state,
        20,
    );
    let candidate_registry_valid = candidate_status
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let actor_authorized = if state.config().session_auth_issuer_registry_require_actor {
        actor_request
            .get("authorized")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    } else {
        true
    };
    let candidate_approval_valid = candidate_approval_checks
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let (accepted, reason) = if !candidate_registry_valid {
        (
            false,
            candidate_status
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("error")
                .to_string(),
        )
    } else if state.config().session_auth_issuer_registry_require_actor && !actor_authorized {
        (
            false,
            actor_request
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("actor_missing")
                .to_string(),
        )
    } else if state
        .config()
        .session_auth_issuer_registry_require_approved_revision
        && !candidate_approval_valid
    {
        (
            false,
            candidate_approval_checks
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("approval_state_not_loaded")
                .to_string(),
        )
    } else {
        (true, "ok".to_string())
    };
    let active_key_diff = session_auth_issuer_registry_active_key_diff_json(
        &current_state.registry,
        &candidate_registry,
    );
    let matches_loaded_active_keys = active_key_diff
        .get("matches")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if accepted {
        let mut live_state = state
            .inner
            .session_auth_issuer_registry_state
            .write()
            .expect("session auth issuer registry state lock poisoned");
        *live_state = SessionAuthIssuerRegistryRuntimeState {
            metadata: candidate_metadata.clone(),
            registry: candidate_registry.clone(),
        };
    }

    let live_state = session_auth_issuer_registry_runtime_state(&state);
    let live_status = session_auth_issuer_registry_status_json(
        state.config(),
        &live_state.metadata,
        &live_state.registry,
    );
    let governance = json!({
        "accepted": accepted,
        "status": if accepted { "accepted" } else { "rejected" },
        "reason": reason,
        "actor_authorized": actor_request.get("authorized").cloned().unwrap_or(Value::Null),
        "actor_reason": actor_request.get("reason").cloned().unwrap_or(Value::Null),
        "current_revision": session_auth_issuer_registry_current_revision(&current_state.metadata),
        "candidate_revision": session_auth_issuer_registry_current_revision(&candidate_metadata),
        "candidate_loaded": candidate_metadata.load_status == "loaded",
        "candidate_revision_approved": candidate_approval_checks
            .get("current_revision_approved")
            .cloned()
            .unwrap_or(Value::Null),
        "matches_loaded_active_keys": matches_loaded_active_keys,
    });

    let body = json!({
        "ok": accepted,
        "reloaded": accepted,
        "status": if accepted { "ok" } else { "rejected" },
        "require_session_auth": state.config().require_session_auth,
        "session_auth_issuer_registry": live_status,
        "session_auth_issuer_registry_previous": current_status,
        "session_auth_issuer_registry_source": candidate_status,
        "session_auth_issuer_registry_actor_checks": actor_checks,
        "session_auth_issuer_registry_actor_request": actor_request,
        "session_auth_issuer_registry_approval": current_approval_checks,
        "session_auth_issuer_registry_approval_source": current_approval_source,
        "session_auth_issuer_registry_source_approval": candidate_approval_checks,
        "session_auth_issuer_registry_source_approval_source": candidate_approval_source,
        "session_auth_issuer_registry_active_key_diff": active_key_diff,
        "session_auth_issuer_registry_reload_governance": governance,
    });

    if accepted {
        return (StatusCode::OK, Json(body)).into_response();
    }

    if state.config().session_auth_issuer_registry_require_actor && !actor_authorized {
        return (StatusCode::FORBIDDEN, Json(body)).into_response();
    }

    (StatusCode::CONFLICT, Json(body)).into_response()
}

pub(super) async fn get_session_auth_issuer_registry_actor_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let checks = session_auth_issuer_registry_actor_checks_json(state.config());
    let is_valid = checks
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let current_state = session_auth_issuer_registry_runtime_state(&state);

    let body = json!({
        "ok": true,
        "status": "ok",
        "require_session_auth": state.config().require_session_auth,
        "session_auth_issuer_registry": session_auth_issuer_registry_status_json(
            state.config(),
            &current_state.metadata,
            &current_state.registry,
        ),
        "session_auth_issuer_registry_actor_checks": checks,
        "actor_valid": is_valid,
    });

    (StatusCode::OK, Json(body)).into_response()
}

pub(super) async fn validate_session_auth_issuer_registry_actors(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let checks = session_auth_issuer_registry_actor_checks_json(state.config());
    let status = checks
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("actor_header_missing");
    let is_valid = checks
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let current_state = session_auth_issuer_registry_runtime_state(&state);

    let body = json!({
        "ok": is_valid,
        "validated": true,
        "valid": is_valid,
        "status": status,
        "require_session_auth": state.config().require_session_auth,
        "session_auth_issuer_registry": session_auth_issuer_registry_status_json(
            state.config(),
            &current_state.metadata,
            &current_state.registry,
        ),
        "session_auth_issuer_registry_actor_checks": checks,
    });

    if is_valid {
        return (StatusCode::OK, Json(body)).into_response();
    }

    (StatusCode::CONFLICT, Json(body)).into_response()
}

pub(super) async fn get_session_auth_issuer_registry_approval_status(
    State(state): State<AppState>,
    Query(query): Query<IdentityApprovalQuery>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let current_state = session_auth_issuer_registry_runtime_state(&state);
    let approval_state = load_session_auth_issuer_registry_revision_approval_state(state.config());
    let checks = session_auth_issuer_registry_approval_checks_json(
        state.config(),
        &current_state.metadata,
        &approval_state,
    );
    let source = session_auth_issuer_registry_approval_source_json(
        state.config(),
        &current_state.metadata,
        &approval_state,
        normalize_identity_approval_limit(query.limit),
    );

    let body = json!({
        "ok": true,
        "status": "ok",
        "require_session_auth": state.config().require_session_auth,
        "session_auth_issuer_registry": session_auth_issuer_registry_status_json(
            state.config(),
            &current_state.metadata,
            &current_state.registry,
        ),
        "session_auth_issuer_registry_approval": checks,
        "session_auth_issuer_registry_approval_source": source,
    });

    (StatusCode::OK, Json(body)).into_response()
}

pub(super) async fn validate_session_auth_issuer_registry_approval(
    State(state): State<AppState>,
    Query(query): Query<IdentityApprovalQuery>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let current_state = session_auth_issuer_registry_runtime_state(&state);
    let approval_state = load_session_auth_issuer_registry_revision_approval_state(state.config());
    let checks = session_auth_issuer_registry_approval_checks_json(
        state.config(),
        &current_state.metadata,
        &approval_state,
    );
    let source = session_auth_issuer_registry_approval_source_json(
        state.config(),
        &current_state.metadata,
        &approval_state,
        normalize_identity_approval_limit(query.limit),
    );
    let status = checks
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("approval_state_not_loaded");
    let is_valid = checks
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let body = json!({
        "ok": is_valid,
        "validated": true,
        "valid": is_valid,
        "status": status,
        "require_session_auth": state.config().require_session_auth,
        "session_auth_issuer_registry": session_auth_issuer_registry_status_json(
            state.config(),
            &current_state.metadata,
            &current_state.registry,
        ),
        "session_auth_issuer_registry_approval": checks,
        "session_auth_issuer_registry_approval_source": source,
    });

    if is_valid {
        return (StatusCode::OK, Json(body)).into_response();
    }

    (StatusCode::CONFLICT, Json(body)).into_response()
}

pub(super) async fn get_identity_approval_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
    let checks = identity_approval_checks_json(state.config(), &store, &approval_state);
    let is_valid = checks
        .get("status")
        .and_then(Value::as_str)
        .map(|status| status == "ok")
        .unwrap_or(false);
    let object = body
        .as_object_mut()
        .expect("identity admin snapshot should be json object");
    object.insert("ok".to_string(), json!(true));
    object.insert("status".to_string(), json!("ok"));
    object.insert("approval_valid".to_string(), json!(is_valid));

    (StatusCode::OK, Json(body)).into_response()
}

pub(super) async fn validate_identity_approval(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let checks = identity_approval_checks_json(state.config(), &store, &approval_state);
    let status = checks
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("approval_state_not_loaded");
    let is_valid = status == "ok";
    let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
    let object = body
        .as_object_mut()
        .expect("identity admin snapshot should be json object");
    object.insert("ok".to_string(), json!(is_valid));
    object.insert("validated".to_string(), json!(true));
    object.insert("valid".to_string(), json!(is_valid));
    object.insert("status".to_string(), json!(status));

    if is_valid {
        return (StatusCode::OK, Json(body)).into_response();
    }

    (StatusCode::CONFLICT, Json(body)).into_response()
}

pub(super) async fn get_identity_approval_source(
    State(state): State<AppState>,
    Query(query): Query<IdentityApprovalQuery>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let limit = normalize_identity_approval_limit(query.limit);
    let source = identity_approval_source_json(state.config(), &store, &approval_state, limit);
    let is_valid = source
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
    let object = body
        .as_object_mut()
        .expect("identity admin snapshot should be json object");
    object.insert("ok".to_string(), json!(true));
    object.insert("status".to_string(), json!("ok"));
    object.insert("source_valid".to_string(), json!(is_valid));
    object.insert("identity_approval_source".to_string(), source);

    (StatusCode::OK, Json(body)).into_response()
}

pub(super) async fn validate_identity_approval_source(
    State(state): State<AppState>,
    Query(query): Query<IdentityApprovalQuery>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let limit = normalize_identity_approval_limit(query.limit);
    let source = identity_approval_source_json(state.config(), &store, &approval_state, limit);
    let status = source
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("approval_state_not_loaded");
    let is_valid = source
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
    let object = body
        .as_object_mut()
        .expect("identity admin snapshot should be json object");
    object.insert("ok".to_string(), json!(is_valid));
    object.insert("validated".to_string(), json!(true));
    object.insert("valid".to_string(), json!(is_valid));
    object.insert("status".to_string(), json!(status));
    object.insert("identity_approval_source".to_string(), source);

    if is_valid {
        return (StatusCode::OK, Json(body)).into_response();
    }

    (StatusCode::CONFLICT, Json(body)).into_response()
}

pub(super) async fn get_identity_governance_status(
    State(state): State<AppState>,
    Query(query): Query<IdentityApprovalQuery>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let audit = {
        let audit = state.inner.identity_binding_audit_state.read().await;
        audit.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let limit = normalize_identity_approval_limit(query.limit);
    let overview =
        identity_governance_overview_json(state.config(), &store, &approval_state, &audit, limit);
    let is_valid = overview
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
    let object = body
        .as_object_mut()
        .expect("identity admin snapshot should be json object");
    object.insert("ok".to_string(), json!(true));
    object.insert("status".to_string(), json!("ok"));
    object.insert("governance_valid".to_string(), json!(is_valid));
    object.insert(
        "identity_binding_audit".to_string(),
        identity_binding_audit_json(&audit),
    );
    object.insert("identity_governance_overview".to_string(), overview);

    (StatusCode::OK, Json(body)).into_response()
}

pub(super) async fn validate_identity_governance(
    State(state): State<AppState>,
    Query(query): Query<IdentityApprovalQuery>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let audit = {
        let audit = state.inner.identity_binding_audit_state.read().await;
        audit.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let limit = normalize_identity_approval_limit(query.limit);
    let overview =
        identity_governance_overview_json(state.config(), &store, &approval_state, &audit, limit);
    let status = overview
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("approval_state_not_loaded");
    let is_valid = overview
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
    let object = body
        .as_object_mut()
        .expect("identity admin snapshot should be json object");
    object.insert("ok".to_string(), json!(is_valid));
    object.insert("validated".to_string(), json!(true));
    object.insert("valid".to_string(), json!(is_valid));
    object.insert("status".to_string(), json!(status));
    object.insert(
        "identity_binding_audit".to_string(),
        identity_binding_audit_json(&audit),
    );
    object.insert("identity_governance_overview".to_string(), overview);

    if is_valid {
        return (StatusCode::OK, Json(body)).into_response();
    }

    (StatusCode::CONFLICT, Json(body)).into_response()
}

pub(super) async fn get_identity_actor_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let checks = identity_actor_checks_json(state.config());
    let is_valid = checks
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
    let object = body
        .as_object_mut()
        .expect("identity admin snapshot should be json object");
    object.insert("ok".to_string(), json!(true));
    object.insert("status".to_string(), json!("ok"));
    object.insert("actor_valid".to_string(), json!(is_valid));

    (StatusCode::OK, Json(body)).into_response()
}

pub(super) async fn validate_identity_actors(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let checks = identity_actor_checks_json(state.config());
    let status = checks
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("actor_header_missing");
    let is_valid = checks
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
    let object = body
        .as_object_mut()
        .expect("identity admin snapshot should be json object");
    object.insert("ok".to_string(), json!(is_valid));
    object.insert("validated".to_string(), json!(true));
    object.insert("valid".to_string(), json!(is_valid));
    object.insert("status".to_string(), json!(status));

    if is_valid {
        return (StatusCode::OK, Json(body)).into_response();
    }

    (StatusCode::CONFLICT, Json(body)).into_response()
}

#[derive(Debug, Deserialize)]
pub struct CreateChatTaskRequest {
    pub user_id: Option<String>,
    pub room_id: Option<String>,
    pub session_id: Option<String>,
    pub org_id: Option<String>,
    pub text: String,
    pub capability_id: Option<String>,
    pub account_id: Option<String>,
    pub idempotency_key: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct MatrixMessageRequest {
    pub matrix_user_id: String,
    pub room_id: String,
    pub session_id: Option<String>,
    pub org_id: Option<String>,
    pub message: String,
    pub capability_id: Option<String>,
    pub account_id: Option<String>,
    pub event_id: Option<String>,
    pub idempotency_key: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct ConsumerTaskResponse {
    pub task_id: String,
    pub consumer_status: String,
    pub invocation_status: Option<String>,
    pub execution: Option<Value>,
    pub trace: Option<Value>,
    pub request: Option<Value>,
    pub source: Value,
    pub raw: Value,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: String,
}
