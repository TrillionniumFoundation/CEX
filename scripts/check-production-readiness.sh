#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

EXECUTION_BASE_URL="${EXECUTION_BASE_URL:-http://127.0.0.1:7003}"
EXECUTION_ADMIN_TOKEN="${EXECUTION_ADMIN_TOKEN:-${CEX_EXECUTION_ADMIN_TOKEN:-${LOCAL_DEV_ADMIN_TOKEN:-local-dev-admin-token}}}"
CONSUMER_ENTRY_BASE_URL="${CONSUMER_ENTRY_BASE_URL:-http://127.0.0.1:8090}"
MATRIX_ENTRY_BASE_URL="${MATRIX_ENTRY_BASE_URL:-http://127.0.0.1:8091}"
CEX_PROVIDER_PROBE_REQUIRED="${CEX_PROVIDER_PROBE_REQUIRED:-1}"
CEX_PROVIDER_PROBE_MODEL="${CEX_PROVIDER_PROBE_MODEL:-}"
CEX_PROVIDER_PROBE_SUCCESS_MAX_AGE_SECONDS="${CEX_PROVIDER_PROBE_SUCCESS_MAX_AGE_SECONDS:-21600}"
CEX_READINESS_MODE="${CEX_READINESS_MODE:-production}"
CEX_DB_BACKUP_RESTORE_DRILL_REQUIRED="${CEX_DB_BACKUP_RESTORE_DRILL_REQUIRED:-}"
CEX_DB_BACKUP_RESTORE_DRILL_SUMMARY_PATH="${CEX_DB_BACKUP_RESTORE_DRILL_SUMMARY_PATH:-}"
CEX_DB_BACKUP_RESTORE_DRILL_MAX_AGE_SECONDS="${CEX_DB_BACKUP_RESTORE_DRILL_MAX_AGE_SECONDS:-86400}"
CEX_REQUIRED_BLOCK_CAPABILITY_PREFIXES="${CEX_REQUIRED_BLOCK_CAPABILITY_PREFIXES:-}"
CEX_MONITORING_DEPLOY_VERIFY_REQUIRED="${CEX_MONITORING_DEPLOY_VERIFY_REQUIRED:-}"
CEX_MONITORING_DEPLOY_METADATA_PATH="${CEX_MONITORING_DEPLOY_METADATA_PATH:-$CEX_PROJECT_ROOT/run/monitoring-live-target/metadata/monitoring-deploy-metadata.yml}"
CEX_MONITORING_DEPLOY_MAX_AGE_SECONDS="${CEX_MONITORING_DEPLOY_MAX_AGE_SECONDS:-86400}"
LEDGER_FAIL_FAST="${LEDGER_FAIL_FAST:-true}"

case "$CEX_READINESS_MODE" in
  local|production) ;;
  *)
    echo "invalid CEX_READINESS_MODE: $CEX_READINESS_MODE (expected local|production)" >&2
    exit 64
    ;;
esac

if [[ -z "$CEX_DB_BACKUP_RESTORE_DRILL_REQUIRED" ]]; then
  if [[ "$CEX_READINESS_MODE" == "production" ]]; then
    CEX_DB_BACKUP_RESTORE_DRILL_REQUIRED="1"
  else
    CEX_DB_BACKUP_RESTORE_DRILL_REQUIRED="0"
  fi
fi
if [[ -z "$CEX_MONITORING_DEPLOY_VERIFY_REQUIRED" ]]; then
  if [[ "$CEX_READINESS_MODE" == "production" ]]; then
    CEX_MONITORING_DEPLOY_VERIFY_REQUIRED="1"
  else
    CEX_MONITORING_DEPLOY_VERIFY_REQUIRED="0"
  fi
fi

cex_require_cmd bash curl jq

failures=0

section() {
  printf '\n==> %s\n' "$*"
}

fail() {
  failures=$((failures + 1))
  printf 'FAIL %s\n' "$*" >&2
}

pass() {
  printf 'OK %s\n' "$*"
}

secret_is_default_or_empty() {
  local value="$1"
  [[ -z "$value" || "$value" == "local-dev-key" || "$value" == "local-dev-admin-token" ]]
}

required_bool_true() {
  local value="$1"
  [[ "${value,,}" == "true" || "$value" == "1" ]]
}

file_has_group_or_other_permissions() {
  local path="$1"
  local mode
  mode="$(stat -c '%a' "$path")"
  (( (8#$mode & 077) != 0 ))
}

csv_items() {
  local raw="$1"
  tr ',' '\n' <<<"$raw" | sed 's/^ *//;s/ *$//' | awk 'length > 0'
}

section 'deployment posture'
printf 'readiness mode=%s\n' "$CEX_READINESS_MODE"
if [[ "$CEX_READINESS_MODE" == "local" ]]; then
  pass 'local readiness posture selected (production secret/profile checks skipped)'
else
  posture_failures=0
  if [[ -n "${CEX_ENV_FILE:-}" && -f "$CEX_ENV_FILE" ]] && file_has_group_or_other_permissions "$CEX_ENV_FILE"; then
    posture_failures=$((posture_failures + 1))
    fail "production posture requires CEX_ENV_FILE to be owner-only readable ($CEX_ENV_FILE)"
  fi
  if secret_is_default_or_empty "${CEX_GATEWAY_API_KEY:-}"; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires non-default CEX_GATEWAY_API_KEY'
  fi
  if secret_is_default_or_empty "${EXECUTION_ADMIN_TOKEN:-}"; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires non-default execution admin token'
  fi
  if ! required_bool_true "${LEDGER_FAIL_FAST:-}"; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires LEDGER_FAIL_FAST=true'
  fi
  if [[ -z "${CONSUMER_ENTRY_INGRESS_TOKEN:-}" ]]; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires CONSUMER_ENTRY_INGRESS_TOKEN'
  fi
  if [[ -z "${MATRIX_ENTRY_INGRESS_TOKEN:-}" ]]; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires MATRIX_ENTRY_INGRESS_TOKEN'
  fi
  if ! required_bool_true "${CONSUMER_ENTRY_REQUIRE_SESSION_AUTH:-}"; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires CONSUMER_ENTRY_REQUIRE_SESSION_AUTH=true'
  fi
  if ! required_bool_true "${CONSUMER_ENTRY_REQUIRE_IDENTITY_BINDING:-}"; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires CONSUMER_ENTRY_REQUIRE_IDENTITY_BINDING=true'
  fi
  if [[ -z "${CONSUMER_ENTRY_REPLAY_STORE_PATH:-}" ]]; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires durable CONSUMER_ENTRY_REPLAY_STORE_PATH'
  fi
  if [[ -z "${CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH:-}" ]]; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires durable CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH'
  fi
  if [[ -z "${MATRIX_ENTRY_RECENT_EVENT_STORE_PATH:-}" ]]; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires durable MATRIX_ENTRY_RECENT_EVENT_STORE_PATH'
  fi

  if [[ "$posture_failures" -eq 0 ]]; then
    pass 'production posture checks clear'
  fi
fi

section 'entry runtime posture'
if [[ "$CEX_READINESS_MODE" == "local" ]]; then
  pass 'local readiness posture selected (entry runtime posture checks skipped)'
else
  consumer_health_file="$(mktemp)"
  matrix_health_file="$(mktemp)"
  consumer_health_ok=0
  matrix_health_ok=0
  curl -fsS "$CONSUMER_ENTRY_BASE_URL/health" >"$consumer_health_file" || consumer_health_ok=$?
  curl -fsS "$MATRIX_ENTRY_BASE_URL/health" >"$matrix_health_file" || matrix_health_ok=$?
  if [[ "$consumer_health_ok" -ne 0 ]]; then
    fail "production posture cannot read consumer-entry health ($CONSUMER_ENTRY_BASE_URL/health)"
  else
    if [[ "$(jq -r '.runtime_profile // "unknown"' "$consumer_health_file")" != "production" ]]; then
      fail 'production runtime requires consumer-entry runtime_profile=production'
    fi
    if [[ "$(jq -r '.ingress_protected // false' "$consumer_health_file")" != "true" ]]; then
      fail 'production runtime requires consumer-entry ingress_protected=true'
    fi
    if [[ "$(jq -r '.require_session_auth // false' "$consumer_health_file")" != "true" ]]; then
      fail 'production runtime requires consumer-entry require_session_auth=true'
    fi
    if [[ "$(jq -r '.require_identity_binding // false' "$consumer_health_file")" != "true" ]]; then
      fail 'production runtime requires consumer-entry require_identity_binding=true'
    fi
    if [[ "$(jq -r '.replay_store_enabled // false' "$consumer_health_file")" != "true" ]]; then
      fail 'production runtime requires consumer-entry replay_store_enabled=true'
    fi
    if [[ "$(jq -r '.rate_limit_store_enabled // false' "$consumer_health_file")" != "true" ]]; then
      fail 'production runtime requires consumer-entry rate_limit_store_enabled=true'
    fi
    if [[ "$(jq -r '.identity_governance_overview.valid // false' "$consumer_health_file")" != "true" ]]; then
      fail 'production runtime requires consumer-entry identity governance valid=true'
    fi
    if ! jq -e '.trillionnium_world_playability_scorecard.user_metric_overall_score == 10 and .trillionnium_world_playability_scorecard.user_metric_overall_percent == 100 and .trillionnium_world_playability_scorecard.user_metric_overall_status == "converged"' "$consumer_health_file" >/dev/null; then
      fail 'production runtime requires Trillionnium five user playability metrics at 10/10'
    fi
    for playability_axis in technical_reliability first_playable_completeness real_player_comprehension_cost long_term_replayability economy_social_strategy_depth; do
      if ! jq -e --arg axis "$playability_axis" '.trillionnium_world_playability_scorecard.user_metric_axes[$axis].score == 10 and .trillionnium_world_playability_scorecard.user_metric_axes[$axis].remaining_checks == []' "$consumer_health_file" >/dev/null; then
        fail "production runtime requires Trillionnium playability axis $playability_axis at 10/10"
      fi
    done
    if ! jq -e '
      def route_runner_handoff_gate_green($gate):
        $gate.contract_version == "trillionnium_playability_route_runner_handoff_gate_v1"
        and $gate.feed_contract_visible == true
        and $gate.map_hub_contract_visible == true
        and $gate.source_count >= 7
        and $gate.sources_include_route_runner_handoff == true
        and $gate.feed_handoff_contract_version == "trillionnium_route_runner_handoff_v1"
        and $gate.map_hub_handoff_contract_version == "trillionnium_route_runner_handoff_v1"
        and $gate.supports_route_mastery_progression == true
        and $gate.route_mastery_contract_version == "trillionnium_route_mastery_v1"
        and $gate.route_mastery_runner_count >= 1
        and $gate.first_route_mastery_xp >= 1
        and (($gate.first_route_mastery_tier // "") != "")
        and (($gate.first_route_mastery_next_goal // "") | ascii_downcase | contains("evidence"))
        and $gate.runner_count >= 1
        and $gate.reward_claim_action_count >= 1
        and $gate.next_route_action_count >= 1
        and (($gate.first_next_route_status // "") != "")
        and (($gate.first_next_route_sequence_summary // "") != "")
        and (($gate.handoff_prompt // "") != "");
      def map_readability_lod_gate_green($gate):
        $gate.contract_version == "trillionnium_world_map_readability_lod_gate_v1"
        and $gate.viewport_contract_version == "trillionnium_world_map_readability_lod_v1"
        and $gate.shell_contract_version == "trillionnium_world_map_readability_lod_v1"
        and $gate.visible_contract_id == "app-map-readability-lod"
        and $gate.first_screen_mode == "route_first_street_detail"
        and $gate.details_default_state == "collapsed"
        and ($gate.visible_markers | type) == "number"
        and ($gate.max_visible_markers | type) == "number"
        and $gate.visible_markers <= $gate.max_visible_markers
        and $gate.max_visible_markers <= 18
        and ($gate.avatar_route_runners | type) == "number"
        and ($gate.max_avatar_route_runners | type) == "number"
        and $gate.avatar_route_runners <= $gate.max_avatar_route_runners
        and $gate.max_avatar_route_runners <= 6
        and $gate.max_primary_cta_count == 1
        and $gate.max_summary_chars <= 150
        and $gate.within_budget == true;
      def route_runner_funnel_telemetry_gate_green($gate):
        $gate.contract_version == "trillionnium_route_runner_funnel_telemetry_gate_v1"
        and $gate.telemetry_contract_version == "trillionnium_route_runner_funnel_telemetry_v1"
        and (($gate.telemetry_stream // "") != "")
        and ($gate.route_started_count | type) == "number"
        and ($gate.evidence_submitted_count | type) == "number"
        and ($gate.reward_claimed_count | type) == "number"
        and ($gate.next_route_opened_count | type) == "number"
        and ($gate.abandoned_or_recovery_count | type) == "number"
        and ($gate.daily_return_resume_count | type) == "number"
        and ($gate.time_to_reward_seconds | type) == "number"
        and $gate.time_to_reward_target_seconds == 1800;
      def future_engine_readiness_gate_green($gate):
        $gate.contract_version == "trillionnium_world_future_engine_readiness_gate_v1"
        and $gate.readiness_contract_version == "trillionnium_world_future_engine_readiness_v1"
        and $gate.planned_upgrade_readiness_contract_version == "trillionnium_world_future_engine_readiness_v1"
        and $gate.active_engine_id == "leaflet_openstreetmap_v1"
        and $gate.adapter_id == "leaflet_renderer_adapter_v1"
        and $gate.runtime_handle_name == "mapRuntime"
        and $gate.candidate_engine_id == "maplibre_gl_v1"
        and $gate.planned_upgrade_status == "planned_not_active"
        and $gate.rollback_plan_visible == true
        and $gate.lod_precondition_visible == true
        and $gate.telemetry_precondition_visible == true
        and $gate.cohort_quality_precondition_visible == true
        and $gate.world_mobile_entry_parity_precondition_visible == true
        and $gate.semantic_map_layers_precondition_visible == true
        and $gate.shadow_renderer_precondition_visible == true
        and $gate.shadow_renderer_contract_version == "trillionnium_world_map_renderer_shadow_v1"
        and $gate.shadow_renderer_status == "shadow_only_not_user_facing"
        and $gate.maplibre_shadow_parity_contract_version == "trillionnium_world_map_maplibre_shadow_parity_v1"
        and $gate.maplibre_shadow_only == true
        and $gate.maplibre_shadow_marker_cluster_popup_focus_parity_visible == true
        and $gate.maplibre_canary_percent == 0
        and $gate.maplibre_canary_starts_at_zero == true
        and $gate.maplibre_max_canary_percent_without_new_signoff <= 1
        and $gate.maplibre_rollback_drill_evidence_required == true
        and $gate.maplibre_canary_rollback_drill_visible == true
        and ($gate.promotion_blocker_count | type) == "number"
        and $gate.promotion_blocker_count >= 3;
      def openstreetmap_provider_readiness_gate_green($gate):
        $gate.contract_version == "trillionnium_openstreetmap_provider_readiness_gate_v1"
        and $gate.geodata_contract_version == "openstreetmap_geodata_v1"
        and $gate.provider_contract == "OpenStreetMapDataProvider"
        and $gate.provider_mode == "fixture"
        and $gate.provider_mode_contract_version == "openstreetmap_provider_mode_v1"
        and $gate.readiness_contract_version == "openstreetmap_provider_readiness_v1"
        and $gate.readiness_status == "fixture_ready_live_fail_closed"
        and $gate.fixture_mode_green == true
        and $gate.fixture_provider_enabled == true
        and $gate.fixture_network_ingestion_enabled == false
        and $gate.stable_fixture_identity_coverage_complete == true
        and ($gate.fixture_node_count | type) == "number"
        and $gate.fixture_node_count > 0
        and ($gate.fixture_layer_feature_count | type) == "number"
        and $gate.fixture_layer_feature_count > 0
        and $gate.live_modes_fail_closed == true
        and $gate.network_ingestion_disabled == true
        and $gate.production_ingestion_disabled == true
        and $gate.provider_modes_observable == true
        and ($gate.fail_closed_mode_count | type) == "number"
        and ($gate.expected_fail_closed_mode_count | type) == "number"
        and $gate.fail_closed_mode_count >= $gate.expected_fail_closed_mode_count
        and $gate.expected_fail_closed_mode_count >= 4
        and $gate.overpass_bbox_cache_fail_closed == true
        and $gate.geofabrik_extract_import_fail_closed == true
        and $gate.vendor_tile_cache_fail_closed == true
        and $gate.unknown_mode_fail_closed == true
        and $gate.public_tile_server_production_traffic_allowed == false
        and $gate.odbl_tracking_required_before_live == true
        and $gate.derived_database_metadata_required_before_live == true
        and $gate.readiness_green == true;
      def world_map_runtime_safety_gate_green($gate):
        $gate.contract_version == "trillionnium_world_map_runtime_safety_gate_v1"
        and $gate.rum_slo_contract_version == "trillionnium_world_map_rum_slo_v1"
        and $gate.rum_slo_quantiles_visible == true
        and $gate.rum_slo_surface_split_visible == true
        and $gate.rum_slo_device_split_visible == true
        and $gate.rum_sample_matrix_contract_version == "trillionnium_world_map_real_user_rum_matrix_v1"
        and $gate.rum_sample_matrix_per_bucket_min_samples >= 1
        and $gate.rum_sample_matrix_cache_network_kinds_visible == true
        and $gate.weak_network_contract_version == "trillionnium_world_map_weak_network_resilience_v1"
        and $gate.weak_network_cached_snapshot_visible == true
        and $gate.weak_network_delta_first_visible == true
        and $gate.weak_network_snapshot_fallback_visible == true
        and $gate.offline_action_queue_contract_version == "trillionnium_world_map_offline_action_queue_v1"
        and $gate.offline_banner_visible == true
        and $gate.pending_action_queue_visible == true
        and $gate.conflict_sync_recovery_visible == true
        and $gate.location_privacy_contract_version == "trillionnium_world_map_location_privacy_v1"
        and $gate.rum_excludes_lat_lng == true
        and $gate.personalized_map_cache_private == true
        and $gate.viewport_api_304_supported == true
        and $gate.entity_delta_cache_contract == "entity_group_versioned_delta_v1"
        and $gate.changed_group_rendering_required == true
        and $gate.visible_marker_delta_required == true
        and $gate.marker_cluster_delta_required == true
        and $gate.viewport_request_abort_visible == true
        and $gate.deferred_card_render_visible == true
        and $gate.marker_cluster_policy_visible == true
        and $gate.density_scalability_contract_version == "trillionnium_world_map_density_scalability_v1"
        and $gate.projection_cache_strategy_visible == true
        and $gate.frontend_virtualization_visible == true
        and $gate.adaptive_density_scheduler_visible == true
        and $gate.gameplay_accessibility_contract_version == "trillionnium_world_map_gameplay_accessibility_i18n_v1"
        and $gate.screen_reader_reduced_motion_touch_targets_visible == true;
      def world_map_rum_slo_gate_observable($gate):
        $gate.contract_version == "trillionnium_world_map_rum_slo_v1"
        and $gate.green == true
        and ($gate.raw_split_green | type) == "boolean"
        and ($gate.sample_count | type) == "number"
        and ($gate.min_enforcement_sample_count | type) == "number"
        and $gate.sample_matrix_contract_version == "trillionnium_world_map_real_user_rum_matrix_v1"
        and $gate.sample_matrix_required_bucket_count == 12
        and $gate.per_bucket_min_samples >= 1
        and ($gate.sample_matrix_coverage_count | type) == "number"
        and ($gate.sample_matrix_missing_bucket_count | type) == "number"
        and (($gate.enforcement_status // "") == "warming_until_min_samples" or ($gate.enforcement_status // "") == "enforced")
        and (if ($gate.enforcement_status // "") == "enforced" then ($gate.raw_split_green == true and $gate.sample_matrix_raw_green == true and $gate.sample_matrix_missing_bucket_count == 0) else true end);
      def world_map_delta_cache_gate_green($gate):
        $gate.contract_version == "trillionnium_world_map_delta_cache_gate_v1"
        and $gate.transport_delta_contract_version == "trillionnium_world_map_transport_delta_v1"
        and $gate.entity_delta_cache_contract == "entity_group_versioned_delta_v1"
        and $gate.failure_rate_within_target == true
        and $gate.noop_and_snapshot_fallback_are_not_failures == true
        and $gate.etag_304_compatible == true;
      def commercial_operating_dashboard_gate_green($gate):
        $gate.contract_version == "trillionnium_world_commercial_operating_dashboard_gate_v1"
        and $gate.dashboard_contract_version == "trillionnium_world_commercial_operating_dashboard_v1"
        and ($gate.route_start_to_paid_task_conversion_percent | type) == "number"
        and ($gate.reward_claim_to_next_commission_percent | type) == "number"
        and ($gate.seller_completion_quality_percent | type) == "number"
        and ($gate.buyer_repeat_order_count | type) == "number"
        and ($gate.dispute_refund_reopen_count | type) == "number"
        and $gate.route_recommendation_policy_contract_version == "trillionnium_world_route_recommendation_policy_v1"
        and $gate.route_recommendation_policy_visible == true
        and $gate.route_recommendation_quality_contract_version == "trillionnium_world_route_recommendation_quality_v1"
        and $gate.route_recommendation_quality_status == "quality_gate_ready"
        and ($gate.route_recommendation_quality_score_percent | type) == "number"
        and ($gate.route_recommendation_quality_score_target_percent | type) == "number"
        and $gate.route_recommendation_quality_score_percent >= $gate.route_recommendation_quality_score_target_percent
        and $gate.route_recommendation_quality_score_ready == true
        and $gate.route_recommendation_reward_lift_visible == true
        and $gate.route_recommendation_abandon_risk_visible == true
        and $gate.route_recommendation_denominator_consistent == true
        and $gate.route_recommendation_raw_counts_preserved == true
        and $gate.route_recommendation_risk_controls_visible == true;
      route_runner_handoff_gate_green(.trillionnium_world_playability_scorecard.route_runner_handoff_gate)
      and route_runner_handoff_gate_green(.trillionnium_world_closed_beta_prototype.route_runner_handoff_gate)
      and route_runner_handoff_gate_green(.trillionnium_world_real_user_beta.route_runner_handoff_gate)
      and route_runner_handoff_gate_green(.trillionnium_world_public_commercial_product.route_runner_handoff_gate)
      and map_readability_lod_gate_green(.trillionnium_world_playability_scorecard.map_readability_lod_gate)
      and route_runner_funnel_telemetry_gate_green(.trillionnium_world_playability_scorecard.route_runner_funnel_telemetry_gate)
      and commercial_operating_dashboard_gate_green(.trillionnium_world_playability_scorecard.commercial_operating_dashboard_gate)
      and future_engine_readiness_gate_green(.trillionnium_world_playability_scorecard.future_engine_readiness_gate)
      and openstreetmap_provider_readiness_gate_green(.trillionnium_openstreetmap_provider_readiness_gate)
      and world_map_runtime_safety_gate_green(.trillionnium_world_map_runtime_safety_gate)
      and world_map_rum_slo_gate_observable(.trillionnium_world_map_rum_slo_gate)
      and world_map_delta_cache_gate_green(.trillionnium_world_map_delta_cache_gate)
    ' "$consumer_health_file" >/dev/null; then
      fail 'production runtime requires route-runner handoff/mastery plus map readability LOD, funnel telemetry, route recommendation quality, future-engine MapLibre shadow readiness, OSM provider fixture/live fail-closed readiness, map runtime safety, RUM SLO/matrix, offline queue, density, gameplay accessibility, and delta-cache gates'
    fi
  fi

  if [[ "$matrix_health_ok" -ne 0 ]]; then
    fail "production posture cannot read matrix-entry health ($MATRIX_ENTRY_BASE_URL/health)"
  else
    if [[ "$(jq -r '.runtime_profile // "unknown"' "$matrix_health_file")" != "production" ]]; then
      fail 'production runtime requires matrix-entry runtime_profile=production'
    fi
    if [[ "$(jq -r '.ingress_protected // false' "$matrix_health_file")" != "true" ]]; then
      fail 'production runtime requires matrix-entry ingress_protected=true'
    fi
    if [[ "$(jq -r '.consumer_entry_protected // false' "$matrix_health_file")" != "true" ]]; then
      fail 'production runtime requires matrix-entry consumer_entry_protected=true'
    fi
    if [[ "$(jq -r '.recent_event_store_enabled // false' "$matrix_health_file")" != "true" ]]; then
      fail 'production runtime requires matrix-entry recent_event_store_enabled=true'
    fi
  fi
  rm -f "$consumer_health_file" "$matrix_health_file"
fi

section 'runtime health/status'
if bash "$SCRIPT_DIR/runtime-manager-linux.sh" status; then
  pass 'runtime status'
else
  fail 'runtime status'
fi

section 'execution policy posture'
if [[ "$CEX_READINESS_MODE" == "local" ]]; then
  pass 'local readiness posture selected (execution policy posture checks skipped)'
else
  policy_info_file="$(mktemp)"
  if curl -fsS "$EXECUTION_BASE_URL/v1/info" >"$policy_info_file"; then
    policy_status="$(jq -r '.policy.policy_bundle_load_status // "unknown"' "$policy_info_file")"
    if [[ "$policy_status" == "loaded" ]]; then
      pass 'execution policy bundle loaded'
    else
      fail "execution policy bundle is not loaded (status=$policy_status)"
    fi
    if [[ -n "$CEX_REQUIRED_BLOCK_CAPABILITY_PREFIXES" ]]; then
      missing_policy_prefixes=0
      while IFS= read -r required_prefix; do
        if jq -e --arg prefix "$required_prefix" '.policy.block_capability_prefixes // [] | index($prefix) != null' "$policy_info_file" >/dev/null; then
          :
        else
          missing_policy_prefixes=$((missing_policy_prefixes + 1))
          fail "execution policy must block non-launch capability prefix: $required_prefix"
        fi
      done < <(csv_items "$CEX_REQUIRED_BLOCK_CAPABILITY_PREFIXES")
      if [[ "$missing_policy_prefixes" -eq 0 ]]; then
        pass 'required non-launch capability prefixes blocked'
      fi
    fi
  else
    fail 'execution info endpoint unreachable for policy posture'
  fi
  rm -f "$policy_info_file"
fi

section 'native metrics smoke'
if bash "$SCRIPT_DIR/smoke-runtime-metrics.sh"; then
  pass 'native metrics smoke'
else
  fail 'native metrics smoke'
fi

section 'operator signals'
operator_json_file="$(mktemp)"
operator_status=0
# Production /health includes full Trillionnium playability and map-readiness evidence;
# keep the operator probe bounded, but do not inherit the shorter ad-hoc 5s default here.
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-${CEX_OPERATOR_SIGNAL_TIMEOUT_SECONDS:-15}}" \
  bash "$SCRIPT_DIR/check-operator-signals.sh" --compact >"$operator_json_file" || operator_status=$?
operator_overall="$(jq -r '.overall // "unknown"' "$operator_json_file")"
operator_warns="$(jq -r '.summary.warn_count // 0' "$operator_json_file")"
operator_criticals="$(jq -r '.summary.critical_count // 0' "$operator_json_file")"
printf 'operator overall=%s warn=%s critical=%s exit=%s\n' \
  "$operator_overall" "$operator_warns" "$operator_criticals" "$operator_status"
if [[ "$operator_status" -eq 0 && "$operator_overall" == "ok" ]]; then
  pass 'operator signals clear'
else
  fail "operator signals not clear (overall=$operator_overall, exit=$operator_status)"
  jq -r '.alerts[]? | "  - \(.severity // "unknown") \(.service):\(.name) value=\(.value // "n/a") threshold=\(.threshold // "n/a")"' "$operator_json_file" >&2
fi
rm -f "$operator_json_file"

section 'provider dead-letter readiness'
dead_letters_json_file="$(mktemp)"
if curl -fsS -H "x-admin-token: $EXECUTION_ADMIN_TOKEN" \
  "$EXECUTION_BASE_URL/v1/executions/provider-dead-letters?limit=200" >"$dead_letters_json_file"; then
  dead_letter_count="$(jq 'length' "$dead_letters_json_file")"
  billing_count="$(jq '[.[] | select(.provider_failure_kind == "billing")] | length' "$dead_letters_json_file")"
  retry_exhausted_count="$(jq '[.[] | select(.retry_budget_exhausted == true)] | length' "$dead_letters_json_file")"
  printf 'provider dead_letters=%s billing=%s retry_budget_exhausted=%s\n' \
    "$dead_letter_count" "$billing_count" "$retry_exhausted_count"
  if [[ "$dead_letter_count" -eq 0 ]]; then
    pass 'provider dead-letter queue clear'
  else
    fail 'provider dead-letter queue is not clear'
    jq -r '.[:10][] | "  - \(.provider_failure_kind) \(.dead_letter_reason) \(.execution_id) \(.provider_target // "unknown") :: \(.error // "")"' "$dead_letters_json_file" >&2
  fi
else
  fail 'provider dead-letter endpoint unreachable'
fi
rm -f "$dead_letters_json_file"

section 'live provider probe'
if [[ "$CEX_PROVIDER_PROBE_REQUIRED" == "0" ]]; then
  pass 'live provider probe not required by environment'
elif [[ -z "$CEX_PROVIDER_PROBE_MODEL" ]]; then
  fail 'live provider probe model is not configured (set CEX_PROVIDER_PROBE_MODEL)'
else
  fresh_provider_probe_log=""
  fresh_provider_probe_age=""
  if [[ "$CEX_PROVIDER_PROBE_SUCCESS_MAX_AGE_SECONDS" =~ ^[0-9]+$ && "$CEX_PROVIDER_PROBE_SUCCESS_MAX_AGE_SECONDS" -gt 0 ]]; then
    now_epoch="$(date +%s)"
    while IFS= read -r candidate; do
      candidate_mtime="${candidate%% *}"
      candidate_path="${candidate#* }"
      candidate_epoch="${candidate_mtime%.*}"
      [[ "$candidate_epoch" =~ ^[0-9]+$ ]] || continue
      candidate_age=$((now_epoch - candidate_epoch))
      if [[ "$candidate_age" -ge 0 && "$candidate_age" -le "$CEX_PROVIDER_PROBE_SUCCESS_MAX_AGE_SECONDS" ]] && \
        grep -Fq "OK live provider probe succeeded ($CEX_PROVIDER_PROBE_MODEL)" "$candidate_path"; then
        fresh_provider_probe_log="$candidate_path"
        fresh_provider_probe_age="$candidate_age"
        break
      fi
    done < <(find "$CEX_PROJECT_ROOT/run" -type f \
      \( -name 'production-readiness-*.log' -o -name 'production-signoff-*.readiness.log' \) \
      -printf '%T@ %p\n' 2>/dev/null | sort -nr)
  fi
  if [[ -n "$fresh_provider_probe_log" ]]; then
    pass "live provider probe fresh evidence ($CEX_PROVIDER_PROBE_MODEL age=${fresh_provider_probe_age}s path=$fresh_provider_probe_log)"
  else
    provider_probe_json_file="$(mktemp)"
    provider_probe_status=0
    bash "$SCRIPT_DIR/probe-openclaw-provider.sh" --model "$CEX_PROVIDER_PROBE_MODEL" --compact \
      >"$provider_probe_json_file" || provider_probe_status=$?
    provider_probe_ok="$(jq -r '.ok // false' "$provider_probe_json_file")"
    provider_probe_status_text="$(jq -r '.status // "unknown"' "$provider_probe_json_file")"
    if [[ "$provider_probe_status" -eq 0 && "$provider_probe_ok" == "true" ]]; then
      pass "live provider probe succeeded ($CEX_PROVIDER_PROBE_MODEL)"
    else
      provider_probe_error="$(jq -r '.error // "unknown provider probe error"' "$provider_probe_json_file")"
      fail "live provider probe failed ($CEX_PROVIDER_PROBE_MODEL status=$provider_probe_status_text): $provider_probe_error"
    fi
    rm -f "$provider_probe_json_file"
  fi
fi

section 'db backup/restore drill evidence'
if [[ "$CEX_DB_BACKUP_RESTORE_DRILL_REQUIRED" == "0" ]]; then
  pass 'db backup/restore drill evidence not required by environment'
else
  drill_summary_path="$CEX_DB_BACKUP_RESTORE_DRILL_SUMMARY_PATH"
  if [[ -z "$drill_summary_path" ]]; then
    drill_summary_path="$(find "$SCRIPT_DIR/../run/drills" -maxdepth 1 -type f -name 'db-backup-restore-*.summary.json' -printf '%T@ %p\n' 2>/dev/null | sort -nr | awk 'NR==1 { $1=""; sub(/^ /, ""); print }')"
  fi
  if [[ -z "$drill_summary_path" || ! -f "$drill_summary_path" ]]; then
    fail 'db backup/restore drill evidence is missing (run scripts/drill-db-backup-restore.sh)'
  else
    drill_ok="$(jq -r '.ok // false' "$drill_summary_path")"
    drill_kind="$(jq -r '.kind // "unknown"' "$drill_summary_path")"
    drill_ended="$(jq -r '.ended_at_epoch // 0' "$drill_summary_path")"
    drill_age=$(( $(date +%s) - drill_ended ))
    if [[ "$drill_ok" != "true" || "$drill_kind" != "db_backup_restore_drill" ]]; then
      fail "db backup/restore drill summary is not successful ($drill_summary_path)"
    elif [[ "$drill_age" -lt 0 || "$drill_age" -gt "$CEX_DB_BACKUP_RESTORE_DRILL_MAX_AGE_SECONDS" ]]; then
      fail "db backup/restore drill summary is stale (age=${drill_age}s max=${CEX_DB_BACKUP_RESTORE_DRILL_MAX_AGE_SECONDS}s path=$drill_summary_path)"
    else
      pass "db backup/restore drill evidence fresh (${drill_age}s old, $drill_summary_path)"
    fi
  fi
fi

section 'monitoring deploy verification evidence'
if [[ "$CEX_MONITORING_DEPLOY_VERIFY_REQUIRED" == "0" ]]; then
  pass 'monitoring deploy verification evidence not required by environment'
else
  monitoring_contract_summary="$(mktemp)"
  if bash "$SCRIPT_DIR/check-trillionnium-route-runner-handoff-monitoring.sh" \
    --metadata "$CEX_MONITORING_DEPLOY_METADATA_PATH" \
    --max-age-seconds "$CEX_MONITORING_DEPLOY_MAX_AGE_SECONDS" \
    --summary-file "$monitoring_contract_summary" \
    --quiet; then
    monitoring_age="$(jq -r '.deployed_age_seconds // "unknown"' "$monitoring_contract_summary")"
    handoff_route_index="$(jq -r '.handoff_route_index // "unknown"' "$monitoring_contract_summary")"
    generic_product_edge_route_index="$(jq -r '.generic_product_edge_route_index // "unknown"' "$monitoring_contract_summary")"
    generic_severity_route_index="$(jq -r '.generic_severity_route_index // "unknown"' "$monitoring_contract_summary")"
    pass "monitoring deploy verification evidence fresh (${monitoring_age}s old, $CEX_MONITORING_DEPLOY_METADATA_PATH; route-runner handoff route order ${handoff_route_index}/${generic_product_edge_route_index}/${generic_severity_route_index})"
  else
    fail "monitoring deploy verification metadata or route-runner handoff monitoring contract is not successful/fresh ($CEX_MONITORING_DEPLOY_METADATA_PATH)"
    jq -r '.failures[]? | "  - " + .' "$monitoring_contract_summary" >&2 || true
  fi
  rm -f "$monitoring_contract_summary"
fi

section 'production readiness verdict'
if [[ "$failures" -eq 0 ]]; then
  echo "READY $CEX_READINESS_MODE readiness smoke passed"
  exit 0
fi

echo "NOT_READY $CEX_READINESS_MODE readiness smoke found $failures blocker(s)" >&2
exit 2
