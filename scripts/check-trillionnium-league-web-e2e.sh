#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$ROOT_DIR/scripts/_dev-helpers.sh"
cex_load_env
cd "$ROOT_DIR"
BASE_URL="${CONSUMER_ENTRY_BASE_URL:-http://127.0.0.1:8090}"
SUMMARY_DIR="$ROOT_DIR/run/league-web"
mkdir -p "$SUMMARY_DIR"

# The web E2E mutates world/company/listing state and exercises normalized
# repository mirrors. Runtime restarts do not apply migrations, so keep the
# schema current before probing command paths.
cex_wait_postgres 60 1 >/dev/null
cex_apply_migrations >/dev/null 2>&1

python3 - <<'PY'
import base64, hashlib, hmac, json, os, pathlib, re, time, urllib.parse, urllib.request

base = os.environ.get('CONSUMER_ENTRY_BASE_URL', 'http://127.0.0.1:8090').rstrip('/')
root = pathlib.Path.cwd()
summary_dir = root / 'run/league-web'
summary_dir.mkdir(parents=True, exist_ok=True)

def entry_headers():
    token = os.environ.get('CONSUMER_ENTRY_INGRESS_TOKEN', '').strip()
    return {'x-entry-token': token} if token else {}

def get(path, headers=None, timeout=20):
    request_headers = entry_headers()
    if headers:
        request_headers.update(headers)
    req = urllib.request.Request(base + path, headers=request_headers)
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        body = resp.read().decode('utf-8', errors='replace')
        return resp.status, body

def post_form(path, headers=None, **fields):
    data = urllib.parse.urlencode(fields).encode()
    request_headers = {'content-type': 'application/x-www-form-urlencoded'}
    request_headers.update(entry_headers())
    if headers:
        request_headers.update(headers)
    req = urllib.request.Request(
        base + path,
        data=data,
        method='POST',
        headers=request_headers,
    )
    with urllib.request.urlopen(req, timeout=20) as resp:
        body = resp.read().decode('utf-8', errors='replace')
        return resp.status, resp.geturl(), body

def post_action(headers=None, **fields):
    return post_form('/league/web/action', headers=headers, **fields)

def post_json(path, payload, headers=None):
    request_headers = {'content-type': 'application/json'}
    request_headers.update(entry_headers())
    if headers:
        request_headers.update(headers)
    req = urllib.request.Request(
        base + path,
        data=json.dumps(payload).encode(),
        method='POST',
        headers=request_headers,
    )
    with urllib.request.urlopen(req, timeout=20) as resp:
        body = resp.read().decode('utf-8', errors='replace')
        return resp.status, dict(resp.headers), json.loads(body)

def b64url(raw: bytes) -> str:
    return base64.urlsafe_b64encode(raw).decode().rstrip('=')

def signed_session_headers(matrix_user_id, room_id, session_id):
    secret = os.environ.get('CONSUMER_ENTRY_SESSION_AUTH_SECRET') or os.environ.get('MATRIX_ENTRY_CONSUMER_SESSION_AUTH_SECRET')
    if not secret:
        return {}
    issuer = os.environ.get('MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER') or 'matrix-entry-adapter'
    audience = os.environ.get('CONSUMER_ENTRY_SESSION_AUTH_EXPECTED_AUDIENCE') or os.environ.get('MATRIX_ENTRY_CONSUMER_SESSION_AUTH_AUDIENCE') or 'consumer-entry-api'
    now = int(time.time())
    fingerprint = f'league-web-session:{matrix_user_id}:{room_id or ""}:{session_id or ""}'
    claims = {
        'version': 1,
        'issuer': issuer,
        'key_id': None,
        'subject': matrix_user_id,
        'source_kind': 'league_web_session',
        'audience': audience,
        'request_fingerprint': fingerprint,
        'room_id': room_id,
        'session_id': session_id,
        'org_id': None,
        'account_id': None,
        'issued_at_epoch': now,
        'expires_at_epoch': now + 300,
    }
    assertion = b64url(json.dumps(claims, separators=(',', ':')).encode())
    signature = b64url(hmac.new(secret.encode(), assertion.encode(), hashlib.sha256).digest())
    return {
        'x-cex-user-session': assertion,
        'x-cex-user-session-signature': signature,
    }


def route_runner_next_route_status_ok(status):
    return status in ('next_route_preview_locked_until_reward_claim', 'next_route_ready_after_reward_claim')


def extract_listing_id(html, marker_text):
    match = re.search(
        r'<article[^>]*class="[^"]*mini listing[^"]*"[^>]*>.*?'
        + re.escape(marker_text)
        + r'.*?<code>(world-listing-[^<]+)</code>',
        html,
        re.S,
    )
    assert match, ('created_listing_id_not_found', marker_text)
    return match.group(1)


def extract_work_order_id(html, marker_text):
    match = re.search(
        r'<article[^>]*class="[^"]*mini work[^"]*"[^>]*data-work-order-id="(world-work-[^"]+)"[^>]*>.*?'
        + re.escape(marker_text),
        html,
        re.S,
    )
    if not match:
        match = re.search(
            re.escape(marker_text) + r'.*?<code>(world-work-[^<]+)</code>',
            html,
            re.S,
        )
    assert match, ('created_work_order_id_not_found', marker_text)
    return match.group(1)


def feed_route_runner_handoff_ok(feed):
    handoff = feed.get('route_runner_handoff') or {}
    runner_count = int(handoff.get('runner_count') or 0)
    reward_claim_count = int(handoff.get('reward_claim_action_count') or 0)
    next_route_count = int(handoff.get('next_route_action_count') or 0)
    return (
        int(feed.get('source_count') or 0) >= 7
        and 'route_runner_handoff' in (feed.get('sources') or [])
        and handoff.get('contract_version') == 'trillionnium_route_runner_handoff_v1'
        and handoff.get('supports_route_runner_reward_claim_actions') is True
        and handoff.get('supports_route_runner_next_route_actions') is True
        and handoff.get('supports_checkpoint_reward_history') is True
        and handoff.get('supports_route_runner_lifecycle') is True
        and handoff.get('lifecycle_contract_version') == 'trillionnium_route_runner_lifecycle_v1'
        and handoff.get('supports_route_mastery_progression') is True
        and handoff.get('route_mastery_contract_version') == 'trillionnium_route_mastery_v1'
        and int(handoff.get('route_mastery_runner_count') or 0) >= 1
        and runner_count >= 1
        and reward_claim_count >= 1
        and next_route_count >= 1
        and bool(handoff.get('first_task_id'))
        and bool(handoff.get('first_progress_label'))
        and int(handoff.get('first_route_mastery_xp') or 0) >= 1
        and bool(handoff.get('first_route_mastery_tier'))
        and bool(handoff.get('first_route_mastery_next_goal'))
        and bool(handoff.get('first_reward_claim_status'))
        and route_runner_next_route_status_ok(handoff.get('first_next_route_status'))
        and bool(handoff.get('first_lifecycle_source'))
        and bool(handoff.get('first_lifecycle_stage'))
        and bool(handoff.get('first_lifecycle_status'))
        and bool(handoff.get('first_next_route_action_body'))
        and bool(handoff.get('first_next_route_sequence_summary'))
        and bool(handoff.get('handoff_prompt'))
    )

status, html = get('/league')
assert status == 200, status
for needle in ['Trillionnium League', 'Trillionnium World', 'Global-first Beta', '海外市场首发', 'Playable Now', '当前可玩版本', 'Web Battle Console', '网页战斗台', 'Guild Halls', '公会大厅', 'Battle Timeline', '战斗时间线', 'Submit Result', '提交战果', 'Progression System', '角色成长系统', '/skills', '/tools', '/skins', 'league-language-switcher', 'trillionnium-league-language-select']:
    assert needle in html, needle

world_status, world_html = get('/world')
assert world_status == 200, world_status
for needle in ['Trillionnium World', 'Global-first open world', '面向海外首发', 'World Action Console', '世界行动台', 'Open-source tactics RPG', '开源战棋 RPG', '三国魔改界面', 'trillionnium_open_source_tactics_world_shell_v1', 'trillionnium_world_tactics_board_v1', 'trillionnium_world_tactics_unit_v1', 'trillionnium_world_tactics_command_v1', 'trillionnium_character_v1', 'trillionnium_skill_v1', 'trillionnium_training_command_v1', 'trillionnium_sect_v1', 'trillionnium_npc_v1', 'trillionnium_sect_osm_binding_v1', 'trillionnium_npc_spawn_anchor_v1', 'trillionnium_npc_command_descriptor_v1', 'trillionnium_mentor_training_task_v1', 'trillionnium_task_archetype_v1', 'trillionnium_task_completion_v1', 'trillionnium_reward_gate_v1', 'trillionnium_battle_log_style_v1', 'trillionnium_combat_log_v1', 'trillionnium_npc_relationship_v1', 'trillionnium_osm_objective_v1', 'trillionnium_tactics_combat_resolution_v1', 'trillionnium_tactics_game_session_v1', 'trillionnium_tactics_simulation_tick_v1', 'trillionnium_tactics_reward_settlement_v1', 'data-tactics-reward-settlement-contract', 'trillionnium_map_overlay_identity_v1', 'trillionnium_world_objective_travel_v1', 'trillionnium_world_skill_practice_loop_v1', 'data-skill-practice-contract-version="trillionnium_world_skill_practice_loop_v1"', 'world-local-skill-practice', 'data-practice-command="train_skill"', 'trillionnium_world_combat_encounter_loop_v1', 'data-combat-encounter-contract-version="trillionnium_world_combat_encounter_loop_v1"', 'trillionnium_hero_tan_full_content_alignment_v1', 'trillionnium-full-content-alignment', 'data-full-content-alignment-contract=', 'data-thresholds-green="true"', 'rust_trillionnium_full_content_volume_alignment_gate', 'data-content-domain="items_and_equipment"', 'data-domain-status="rust_runtime_backed"', 'trillionnium-equipment', 'data-item-equipment-runtime-contract="trillionnium_world_item_equipment_runtime_v1"', 'rust_trillionnium_item_equipment_runtime_state', 'trillionnium-equipment-form', 'name="command" value="equip_item"', 'name="target_slot"', 'data-content-domain="survival_time_resource_pressure"', 'data-resource-pressure-runtime-contract="trillionnium_world_resource_pressure_runtime_v1"', 'trillionnium-resource-pressure-panel', 'rust_trillionnium_resource_pressure_runtime_state', 'world_state.world_trillionnium_characters.resource_pressure_state', 'data-runtime-status="rust_owned_time_stamina_injury_evidence_live"', 'world-local-combat-encounter', 'world-local-combat-encounter-form', 'rust_world_combat_encounter_projection', 'rust_world_combat_encounter_validator', 'rust_world_combat_encounter_return_state', 'world-local-combat-return', 'data-world-objective-travel-contract="trillionnium_world_objective_travel_v1"', 'world-objective-travel', 'rust_world_graph_objective_travel', 'data-objective-travel-role=', 'world-objective-party-member', 'trillionnium-tactics-session-state', 'world-tactics-player-hud', 'trillionnium_tactics_player_visible_surface_v1', 'world-tactics-objective-card', 'world-tactics-current-session-card', 'world-tactics-command-draft-panel', 'world-tactics-command-draft-form', 'world-tactics-reward-history-handoff', 'world-tactics-repeat-farming-copy', 'trillionnium_tactics_board_cell_interaction_v1', 'trillionnium_tactics_unit_selection_v1', 'trillionnium_tactics_command_intent_draft_v1', 'trillionnium_tactics_accessibility_v1', 'data-keyboard-traversal="roving_grid_focus"', 'data-low-motion-support="prefers_reduced_motion"', 'world-tactics-keyboard-help', 'role="gridcell"', 'data-roving-tabindex="tactics_board"', 'aria-live="polite"', 'focusAdjacentTile', 'data-draft-target-tile=', 'data-draft-unit-id=', 'data-draft-command=', 'initializeTacticsIntentDraft', 'window.trillionniumTacticsIntentDraft', 'data-reward-history-contract="trillionnium_tactics_reward_history_v1"', 'data-anti-cheese-contract="trillionnium_tactics_repeat_farming_anti_cheese_v1"', 'Repeat-farming guard', 'data-objective-progress', 'data-victory-state', 'data-reward-status', 'rust_trillionnium_osm_objective_generator', 'talk_npc', 'offer_task', 'complete_task', 'courier_letter', 'trillionnium-task-candidates', 'ledger_settlement_review_hold_anti_cheese', 'rust_trillionnium_task_completion_handler', '/world/web/tactics-command', '/v1/world/tactics/command', 'rust_mentor_training_validator', 'rust_mentor_training_command_model', 'rust_trillionnium_sect_model', 'rust_trillionnium_npc_model', 'rust_trillionnium_game_state', 'rust_tactics_board_projection', 'rust_tactics_command_model', 'rust_trillionnium_character', 'rust_tactics_combat_handler', 'basic_inner_power', 'rust_command_handler_ledger_progression', 'trillionnium_native_combat_task_templates_v1', 'native_templates_only_no_verbatim_source_reference_strings', '镜城风从巷口压低', '发起攻击', '结束回合', 'turn_based_strategy_rpg', 'tranchikhang/MedievalWar', '战棋指令菜单', 'openclawstreetmap_underlay', 'OpenClawStreetMap', 'supporting_engine_diagnostics', '支撑层，不是主界面', 'world-openstreetmap-geodata', 'openstreetmap_geodata_v1', 'OpenStreetMapDataProvider', 'fixture_openstreetmap_data_provider_v1', 'stable_fixture_table', 'openstreetmap_fixture_layers_v1', 'mentor_training_anchor', 'rust_openstreetmap_data_provider', 'visualization_input_only', 'osm_id', 'osm_type', 'game_overlay_id', 'odbl_database_obligations', 'no_live_overpass', 'world-openstreetmap-provider-readiness', 'openstreetmap_provider_readiness_v1', 'fixture_ready_live_fail_closed', 'data-live-modes-fail-closed="true"', 'data-live-network-ingestion-enabled="false"', 'overpass_bbox_cache', 'geofabrik_extract_import', 'vendor_tile_cache', 'world-openstreetmap-geodata-freshness', 'openstreetmap_geodata_freshness_v1', 'fixture_static_fresh_live_stale_blocked', 'data-wall-clock-freshness-applies="false"', 'data-live-data-freshness-applies="false"', 'data-staleness-gate-green="true"', 'world-openstreetmap-attribution', 'openstreetmap_attribution_presence_v1', '© OpenStreetMap contributors', 'ODbL-1.0', 'data-attribution-visible="true"', 'world-mobile-first-screen', 'world-hero-mobile-actions', 'world-first-human-loop', 'trillionnium_first_human_session_v1', 'trillionnium_world_first_screen_four_questions_v1', 'trillionnium_world_transition_semantics_v1', 'rust_world_map_transition_rules', 'data-transition-contract-version="trillionnium_world_transition_semantics_v1"', 'data-transition-source-of-truth="rust_world_map_transition_rules"', 'data-transition-kind="blocked_terrain"', 'data-transition-kind="zone_transition"', 'data-transition-result="open_exit"', 'world-keypad-adventure-shell', 'world-keypad-numpad', 'world-keypad-move-form', 'data-first-human-question="who"', 'data-first-human-question="where"', 'data-first-human-question="click"', 'data-first-human-question="reward"', 'world-language-switcher', 'trillionnium-world-language-select', 'world-pulse-strip', 'world-stats-compact-more', 'trillionnium_secondary_dashboard_panels_v1', 'data-secondary-dashboard-role="secondary_detail_panel"', 'data-main-experience="false"', 'data-default-state="collapsed_on_mobile"', 'world-mobile-primary-cta', 'Continue route: enter tactics board', '继续路线：进入战棋棋盘', 'Pick route', 'Submit proof', 'Claim reward', 'world-route-archetype-catalog', 'trillionnium_world_route_archetypes_v1', 'world-map-readability-lod', 'world-map-performance-budget', 'world-map-transport-delta', 'world-map-shadow-renderer', 'world-map-rum-slo', 'world-map-weak-network', 'world-map-location-privacy', 'trillionnium_world_map_readability_lod_v1', 'trillionnium_world_map_runtime_performance_budget_v1', 'trillionnium_world_map_transport_delta_v1', 'trillionnium_world_map_renderer_shadow_v1', 'trillionnium_world_map_rum_slo_v1', 'trillionnium_world_map_weak_network_resilience_v1', 'trillionnium_world_map_location_privacy_v1', 'bounty_delivery', 'trillionnium-active-route-line', 'trillionnium-map-pin', 'More world counters', '更多世界统计', 'Global Real-world Map Engine', 'world-real-map', 'createRealWorldMapAdapter', 'leaflet_renderer_adapter_v1', 'maplibre_gl_v1', 'const mapRuntime', 'supports_future_engine_swap', 'gating_contract', 'renderRouteLine', 'renderTileFrame', 'renderEventPulse', 'buildMapLibreShadowParityProbe', 'trillionnium-world-map:last-good-viewport:v1', 'weak_network_cached_snapshot', 'location_privacy_contract_visible', 'onViewportChange', 'getCenter', 'getZoom', 'global_real_world_tiles', 'cn-shanghai-core', 'street_level_world_nodes', 'osm-z15', 'primary_actions', 'trillionnium-map-action', 'Move Here', '移动到这里', 'world-map-move-target', 'world-action-body', 'world-action-console-status', '起草世界行动', '起草任务后续', '推进下一条支线', '下一条支线', 'Event brief', '事件简报：', 'Linked task route', '关联任务路线', 'Recommended next step', '推荐下一步', 'Quest Route Graph', '任务路线图', 'world-route-task-graph-live', '/world/web/map-viewport', '/world/web/map-viewport', 'world-map-camera-summary', 'world-map-route-flow-status', 'world-map-route-next-step-status', 'world-map-route-link-status', 'world-work-deliver-id', 'world-work-deliver-body', 'world-buy-body', 'world-contract-completion-id', 'world-contract-completion-body', 'world-company-asset-id', 'world-listing-company-id', 'world-tile-shards-live', 'Character Items', '角色道具', 'Upgrade Item', '升级道具', 'Studios and Hubs', '工坊与据点', 'Launch Studio', '建立工坊', 'Hubs and Quest Cards', '据点与任务牌', 'Publish Quest Card', '发布任务牌', 'Bounties and Adventure Commissions', '悬赏与冒险委托', 'Accept Quest Card', '接取任务牌', 'Submit Result', '提交成果', 'Pass Rating', '评级通过', 'Faction Reputation Map', '阵营声望图', 'World Contracts', '世界契约', 'Complete Contract', '完成契约', 'findLiveEventByFocus', 'buildEventFocus', 'focusRouteEvent', 'filterLiveEventStream', '条事件镜头', 'data-focus-kind="event"', 'data-event-id=', 'data-task-id=', 'world-event-timeline-item-']:
    assert needle in world_html, needle

matrix_user_id = '@alice:local.dev'
marker = f'web-e2e-{int(time.time())}'
actions = []
room_id = '!web-local:local.dev'
session_id = 'web-e2e-session'
session_status, session_headers, session_body = post_json('/league/web/session', {
    'matrix_user_id': matrix_user_id,
    'room_id': room_id,
    'session_id': session_id,
}, headers=signed_session_headers(matrix_user_id, room_id, session_id))
assert session_status == 200, session_status
session_cookie = session_headers.get('Set-Cookie') or session_headers.get('set-cookie')
assert session_cookie and 'cex_league_session=' in session_cookie, session_headers
csrf = session_body.get('csrf')
assert csrf, session_body
cookie_header = {'Cookie': session_cookie.split(';', 1)[0]}
status, html_with_session = get('/league', headers=cookie_header)
assert status == 200, status
app_status, app_html = get('/app')
assert app_status == 200 and 'Trillionnium World' in app_html and '移动世界壳 v1' in app_html, app_status
feed_status, feed_body = get('/app/web/feed', headers=cookie_header)
assert feed_status == 200, feed_status
feed_json = json.loads(feed_body)
assert feed_route_runner_handoff_ok(feed_json), feed_json.get('route_runner_handoff')
for needle in ['real-world-map', 'leaflet_openstreetmap_v1', 'createRealWorldMapAdapter', 'leaflet_renderer_adapter_v1', 'maplibre_gl_v1', 'trillionnium_world_future_engine_readiness_v1', 'const mapRuntime', 'supports_future_engine_swap', 'gating_contract', 'renderRouteLine', 'renderTileFrame', 'renderEventPulse', 'buildMapLibreShadowParityProbe', 'trillionnium-world-map:last-good-viewport:v1', 'weak_network_cached_snapshot', 'location_privacy_contract_visible', 'onViewportChange', 'getCenter', 'getZoom', 'OpenStreetMap', 'Leaflet', 'tile.openstreetmap.org', 'app-openstreetmap-attribution', 'openstreetmap_attribution_presence_v1', '© OpenStreetMap contributors', 'ODbL-1.0', 'global_real_world_tiles', 'gather_hero_tale_lod', 'cn-shanghai-core', 'primary_actions', 'trillionnium-map-action', 'Move Here', '移动到这里', '/v1/world/map/{matrix_user_id}/viewport', '/world/web/map-viewport', '/app/web/feed', 'TrillionniumLanguage', 'trillionnium.ui.language', 'data-trillionnium-language-select', 'trillionnium-app-language-select', 'trillionnium-system-language-settings', 'System Settings', '系统设置', 'English', '中文', 'app-global-search', 'app-search-clear', 'app-search-empty-state', 'app-ux-live-status', 'role="tablist"', 'role="tab"', 'role="tabpanel"', 'aria-selected="true"', 'handleAppTabKeydown', 'announceUxStatus', 'trillionnium_mobile_shell_ux_v1', 'trillionnium_mobile_single_primary_cta_v1', 'mobile_bottom_sheet_single_primary_cta_visible', 'app-mobile-action-sheet', 'app-mobile-primary-cta', 'data-primary-cta-count="1"', 'Continue Route', 'app-tactics-player-hud', 'trillionnium_tactics_player_visible_surface_v1', 'app-tactics-objective-card', 'app-tactics-current-session-card', 'app-tactics-intent-draft-card', 'app-tactics-reward-history-handoff', 'app-tactics-repeat-farming-copy', 'trillionnium_tactics_board_cell_interaction_v1', 'trillionnium_tactics_unit_selection_v1', 'trillionnium_tactics_command_intent_draft_v1', 'data-reward-history-contract="trillionnium_tactics_reward_history_v1"', 'data-anti-cheese-contract="trillionnium_tactics_repeat_farming_anti_cheese_v1"', 'Repeat-farming guard', 'trillionnium_mobile_copy_layering_v1', 'mobile_copy_layering_visible', 'trillionnium_world_map_readability_lod_v1', 'trillionnium_world_map_runtime_performance_budget_v1', 'trillionnium_world_map_transport_delta_v1', 'trillionnium_world_map_first_screen_decision_v1', 'trillionnium_world_map_renderer_shadow_v1', 'trillionnium_world_map_rum_slo_v1', 'trillionnium_world_map_weak_network_resilience_v1', 'trillionnium_world_map_location_privacy_v1', 'trillionnium_world_route_recommendation_policy_v1', 'trillionnium_world_map_subsystem_v1', 'trillionnium_world_map_game_layer_semantics_v1', 'map_readability_lod_visible', 'map_rum_slo_quantiles_visible', 'map_weak_network_resilience_visible', 'map_location_privacy_visible', 'map_game_layer_semantics_visible', 'app-map-readability-lod', 'app-map-performance-budget', 'app-map-transport-delta', 'app-map-rum-slo', 'app-map-weak-network', 'app-map-location-privacy', 'app-map-first-screen-decision', 'data-visible-marker-budget="18"', 'One route first', 'app-map-copy-summary', 'app-map-copy-layer-details', 'data-default-state="collapsed"', 'Pick a nearby route', 'Why this map matters', 'keyboard_tab_navigation_visible', 'offline_feed_fallback_status_visible', 'web_session_feed_hydration_visible', '/app/web/feed', 'app-bottom-tabs', 'app-tab-messages', 'app-tab-map', 'app-tab-feed', 'app-tab-me', 'Messages', '消息', 'World', '世界', 'Feed', '动态', 'Me', '我', 'app-first-playable-onboarding', 'app-first-playable-checks', 'app-first-playable-steps', 'Starter Quest', '新手主线', 'global_first_overseas_beta', 'trillionnium_first_playable_onboarding_v1', 'first_playable_loop_100', 'data-onboarding-step="quest_delivery"', 'route_task_graph_next_action_visible', 'app-map-camera-summary', 'app-map-route-status', 'Recommended next step', '推荐下一步', 'Event brief', '事件简报：', 'Linked task route', '关联任务路线', '起草任务后续', '推进下一条支线', '下一条支线', '按焦点筛选路线', '显示完整路线', 'app-route-preview-live', 'Adventure Route Preview', '冒险路线预览', 'app-route-task-graph-live', 'Quest Route Graph', '任务路线图', '打开关联契约', 'app-feed-api-status', 'app-feed-filter-actions', 'app-feed-summary', 'app-feed-items-live', 'app-feed-route-runner-handoff', 'app-route-runner-funnel-telemetry', 'trillionnium_route_runner_funnel_telemetry_v1', 'data-time-to-reward-target-seconds="1800"', 'data-reward-to-next-route-percent=', 'app-route-archetype-catalog', 'app-commercial-operating-dashboard', 'World Activity Timeline', '世界动态时间线', 'trillionnium-app-feed-filter', 'trillionnium-app-feed-action', 'loadFeedSurface', 'route_runner_handoff', 'trillionnium_route_runner_handoff_v1', 'trillionnium_route_runner_lifecycle_v1', 'trillionnium_route_mastery_v1', 'routeRunnerMasteryChipsHtml', 'data-route-mastery-tier', 'data-next-route-status', 'Route runner handoff', 'Route mastery', 'route-runner-handoff', 'app-tile-shards-live', 'Map Tiles', '地图分片', 'Regional Hubs', '区域据点', 'Nearby Hotspots', 'Character Modules', '角色成长', '/progression', '/skills /tools /skins', 'findLiveEventByFocus', 'buildEventFocus', 'filterLiveEventStream', '条事件镜头', 'web_event_id', 'data-focus-kind="event"', 'data-event-id=', 'data-task-id=']:
    assert needle in app_html, ('client_app_real_world_map_engine', needle)
# The normal unauthenticated local-dev shell remains available; the signed session path is checked below.
code, url, body = post_action(**{
    'action': 'join',
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'match_id': 'daily-dungeon-001',
}, headers=cookie_header)
assert code == 200, ('session_join', code, url)
world_marker = f'world-{marker}'
code, url, body = post_form('/world/web/action', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'location_id': 'zbj-market-gate',
    'body': f'{world_marker}: 我要在镜像城市建立 AI 设计工坊，招募 Agent，完成一条真实委托支线。',
})
assert code == 200, ('world_action', code, url)
assert 'Trillionnium World' in body and world_marker in body, ('world_action_body', url)
actions.append({'action': 'world', 'status': code, 'url': url})
map_target_match = re.search(r'<form method="post" action="/world/web/map-move".*?<option value="([^"]+)"', body, re.S)
map_target = map_target_match.group(1) if map_target_match else 'east'
code, url, body = post_form('/world/web/map-move', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'target': map_target,
})
assert code == 200 and 'Trillionnium World' in body, ('world_map_move', code, url)
actions.append({'action': 'world_map_move', 'status': code, 'url': url})

code, url, body = post_form('/world/web/asset', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'asset_id': 'latest',
    'body': f'Web asset upgrade {marker}: 客户交付方案、道具定位、证据包、风险控制清单、行动节奏、评级标准、下一步计划和自检。',
})
assert code == 200 and 'Trillionnium World' in body, ('world_asset', code, url)
actions.append({'action': 'world_asset', 'status': code, 'url': url})

code, url, body = post_form('/world/web/company', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'asset_id': 'latest',
    'body': f'Web workshop launch {marker}: 工坊定位、客户交付方案、委托目标画像、奖励路径、行动循环、证据、风险控制、评级标准和下一步计划。',
})
assert code == 200 and 'Trillionnium World' in body, ('world_company', code, url)
actions.append({'action': 'world_company', 'status': code, 'url': url})

first_listing_marker = f'Web quest board {marker}'
code, url, body = post_form('/world/web/listing', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'company_id': 'latest',
    'body': f'{first_listing_marker}: AI 工坊委托，写清成果、赏金逻辑、证据包、承诺、风险控制、自检、评级标准、返工规则和下一步。',
})
assert code == 200 and 'Trillionnium World' in body, ('world_listing', code, url)
first_listing_id = extract_listing_id(body, first_listing_marker)
actions.append({'action': 'world_listing', 'status': code, 'url': url, 'listing_id': first_listing_id})

first_buy_marker = f'Web quest accept {marker}'
code, url, body = post_form('/world/web/buy', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'listing_id': first_listing_id,
    'body': f'{first_buy_marker}: 接取任务牌，开启冒险委托，定义成果、证据、评级标准、风险控制和下一步。',
})
assert code == 200 and 'Trillionnium World' in body, ('world_buy', code, url)
first_work_order_id = extract_work_order_id(body, first_buy_marker)
actions.append({'action': 'world_buy', 'status': code, 'url': url, 'work_order_id': first_work_order_id})

code, url, body = post_form('/world/web/work-deliver', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'work_order_id': first_work_order_id,
    'body': f'Web quest result {marker}: 最终成果、证据包、评级清单、风险复盘、下一步和完成自检。',
})
assert code == 200 and 'Trillionnium World' in body, ('world_work_deliver', code, url)
actions.append({'action': 'world_work_deliver', 'status': code, 'url': url, 'work_order_id': first_work_order_id})

code, url, body = post_form('/world/web/work-accept', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'work_order_id': first_work_order_id,
    'body': f'Web quest rating {marker}: 委托方确认质量、证据、评级标准和下一次协作。',
})
assert code == 200 and 'Trillionnium World' in body, ('world_work_accept', code, url)
actions.append({'action': 'world_work_accept', 'status': code, 'url': url, 'work_order_id': first_work_order_id})

revise_listing_marker = f'Web revise-flow board {marker}'
code, url, body = post_form('/world/web/listing', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'company_id': 'latest',
    'body': f'{revise_listing_marker}: AI 工坊返工委托，写清成果、赏金逻辑、证据包、验收标准、返工触发条件、风险控制、自检记录、评级标准和下一步计划。',
})
assert code == 200 and 'Trillionnium World' in body, ('world_reject_listing', code, url)
revise_listing_id = extract_listing_id(body, revise_listing_marker)
actions.append({'action': 'world_reject_listing', 'status': code, 'url': url, 'listing_id': revise_listing_id})

revise_buy_marker = f'Web revise-flow accept {marker}'
code, url, body = post_form('/world/web/buy', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'listing_id': revise_listing_id,
    'body': f'{revise_buy_marker}: 开启第二条冒险委托，用于验证奖励退回、证据缺口、返工要求和下一步。',
})
assert code == 200 and 'Trillionnium World' in body, ('world_reject_buy', code, url)
revise_work_order_id = extract_work_order_id(body, revise_buy_marker)
actions.append({'action': 'world_reject_buy', 'status': code, 'url': url, 'work_order_id': revise_work_order_id})

code, url, body = post_form('/world/web/work-deliver', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'work_order_id': revise_work_order_id,
    'body': f'Web revise-flow result {marker}: 成果、证据包、质量记录、风险复盘和下一步。',
})
assert code == 200 and 'Trillionnium World' in body, ('world_reject_deliver', code, url)
actions.append({'action': 'world_reject_deliver', 'status': code, 'url': url, 'work_order_id': revise_work_order_id})

code, url, body = post_form('/world/web/work-reject', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'work_order_id': revise_work_order_id,
    'body': f'Web quest revision {marker}: 委托方要求返工，退回预留奖励，记录证据缺口、修改要求和下一步。',
})
assert code == 200 and 'Trillionnium World' in body, ('world_work_reject', code, url)
actions.append({'action': 'world_work_reject', 'status': code, 'url': url, 'work_order_id': revise_work_order_id})

code, url, body = post_form('/world/web/work-reopen', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'work_order_id': revise_work_order_id,
    'body': f'Web quest reopen {marker}: 委托方重新锁定奖励，列出返工要求、证据缺口、评级标准和再次提交动作。',
})
assert code == 200 and 'Trillionnium World' in body, ('world_work_reopen', code, url)
actions.append({'action': 'world_work_reopen', 'status': code, 'url': url, 'work_order_id': revise_work_order_id})

code, url, body = post_form('/world/web/work-cancel', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'work_order_id': revise_work_order_id,
    'body': f'Web quest cancel {marker}: 委托方在成果前放弃这条委托，退回预留奖励，记录原因并关闭契约。',
})
assert code == 200 and 'Trillionnium World' in body, ('world_work_cancel', code, url)
actions.append({'action': 'world_work_cancel', 'status': code, 'url': url, 'work_order_id': revise_work_order_id})

for fields in [
    {'action': 'join', 'matrix_user_id': matrix_user_id, 'csrf': csrf, 'match_id': 'daily-dungeon-001'},
    {'action': 'guild', 'matrix_user_id': matrix_user_id, 'csrf': csrf, 'guild_id': 'guild-prompt-forge'},
    {'action': 'team', 'matrix_user_id': matrix_user_id, 'csrf': csrf, 'match_id': 'guild-raid-001', 'role': 'scout', 'heroes': 'oracle_scout'},
    {'action': 'draft', 'matrix_user_id': matrix_user_id, 'csrf': csrf, 'heroes': 'oracle_scout forge_builder mirror_auditor courier_closer'},
    {
        'action': 'raid',
        'matrix_user_id': matrix_user_id,
        'csrf': csrf,
        'match_id': 'guild-raid-001',
        'body': f'Raid contribution {marker}: scout evidence, assign builder, define risk gate, next step.',
    },
    {
        'action': 'submit',
        'matrix_user_id': matrix_user_id,
        'csrf': csrf,
        'match_id': 'daily-dungeon-001',
        'body': f'Web E2E result {marker}: 成果、证据、风险、自检和下一步。',
    },
    {
        'action': 'submit',
        'matrix_user_id': matrix_user_id,
        'csrf': csrf,
        'match_id': 'daily-dungeon-001',
        'body': 'copy copy copy',
    },
]:
    code, url, body = post_action(headers=cookie_header, **fields)
    assert code == 200, (fields['action'], code, url)
    assert 'Trillionnium League' in body, fields['action']
    actions.append({'action': fields['action'], 'status': code, 'url': url})

status, html_after = get('/league', headers=cookie_header)
assert status == 200, status
for needle in ['Oracle 侦察手', '锻造建造者', '镜像审稿人', '最强掉落', '已结算', '人工复核中', '隐藏规则评分', '角色成长系统', '成功任务']:
    assert needle in html_after, needle
world_status, world_html_after = get('/world', headers=cookie_header)
assert world_status == 200, world_status
assert world_marker in world_html_after, world_marker
health_status, health_body = get('/health', timeout=180)
assert health_status == 200, health_status
health = json.loads(health_body)
maturity = health.get('trillionnium_world_maturity') or {}
axes = maturity.get('axes') or {}
closed_beta = health.get('trillionnium_world_closed_beta_prototype') or {}
closed_beta_axes = closed_beta.get('axes') or {}
real_user_beta = health.get('trillionnium_world_real_user_beta') or {}
real_user_beta_axes = real_user_beta.get('axes') or {}
public_commercial = health.get('trillionnium_world_public_commercial_product') or {}
public_commercial_axes = public_commercial.get('axes') or {}
assert maturity.get('contract_version') == 'trillionnium_world_maturity_axes_v1', maturity
assert maturity.get('target') == 'all_4_axes_100_percent', maturity
assert maturity.get('overall_percent') == 100, maturity
for axis_id in ['first_playable', 'technical_alpha', 'beta_readiness', 'full_vision']:
    axis = axes.get(axis_id) or {}
    assert axis.get('percent') == 100, (axis_id, axis)
    assert axis.get('status') == 'converged', (axis_id, axis)
assert closed_beta.get('contract_version') == 'trillionnium_world_closed_beta_prototype_v1', closed_beta
assert closed_beta.get('target') == 'closed_beta_prototype_100_percent', closed_beta
assert closed_beta.get('overall_percent') == 100, closed_beta
for axis_id in ['product_loop', 'access_governance', 'persistence_runtime', 'world_depth', 'commerce_recovery']:
    axis = closed_beta_axes.get(axis_id) or {}
    assert axis.get('percent') == 100, (axis_id, axis)
    assert axis.get('status') == 'converged', (axis_id, axis)
assert real_user_beta.get('contract_version') == 'trillionnium_world_real_user_beta_v1', real_user_beta
assert real_user_beta.get('target') == 'real_user_long_term_beta_100_percent', real_user_beta
assert real_user_beta.get('overall_percent') == 100, real_user_beta
for axis_id in ['product_retention', 'access_safety', 'durable_persistence', 'economy_recovery', 'world_capacity', 'ops_runtime']:
    axis = real_user_beta_axes.get(axis_id) or {}
    assert axis.get('percent') == 100, (axis_id, axis)
    assert axis.get('status') == 'converged', (axis_id, axis)
assert public_commercial.get('contract_version') == 'trillionnium_world_public_commercial_product_v1', public_commercial
assert public_commercial.get('target') == 'public_commercial_product_100_percent', public_commercial
assert public_commercial.get('overall_percent') == 100, public_commercial
for axis_id in ['public_launch_surface', 'commercial_engine', 'trust_safety', 'durable_scale_ops', 'growth_network', 'public_world_depth']:
    axis = public_commercial_axes.get(axis_id) or {}
    assert axis.get('percent') == 100, (axis_id, axis)
    assert axis.get('status') == 'converged', (axis_id, axis)
assert health.get('league_repository_runtime', {}).get('effective_repository') == 'normalized_sql_direct_write_final', health.get('league_repository_runtime')
assert health.get('league_repository_runtime', {}).get('repository_cutover_status') == 'normalized_sql_direct_write_final_cutover_active', health.get('league_repository_runtime')
osm_provider_readiness = health.get('trillionnium_openstreetmap_provider_readiness_gate') or {}
assert osm_provider_readiness.get('contract_version') == 'trillionnium_openstreetmap_provider_readiness_gate_v1', osm_provider_readiness
assert osm_provider_readiness.get('readiness_contract_version') == 'openstreetmap_provider_readiness_v1', osm_provider_readiness
assert osm_provider_readiness.get('provider_mode') == 'fixture', osm_provider_readiness
assert osm_provider_readiness.get('fixture_mode_green') is True, osm_provider_readiness
assert osm_provider_readiness.get('live_modes_fail_closed') is True, osm_provider_readiness
assert osm_provider_readiness.get('network_ingestion_disabled') is True, osm_provider_readiness
assert osm_provider_readiness.get('production_ingestion_disabled') is True, osm_provider_readiness
assert osm_provider_readiness.get('overpass_bbox_cache_fail_closed') is True, osm_provider_readiness
assert osm_provider_readiness.get('geofabrik_extract_import_fail_closed') is True, osm_provider_readiness
assert osm_provider_readiness.get('vendor_tile_cache_fail_closed') is True, osm_provider_readiness
osm_geodata_freshness = health.get('trillionnium_openstreetmap_geodata_freshness_gate') or {}
assert osm_geodata_freshness.get('contract_version') == 'trillionnium_openstreetmap_geodata_freshness_gate_v1', osm_geodata_freshness
assert osm_geodata_freshness.get('freshness_contract_version') == 'openstreetmap_geodata_freshness_v1', osm_geodata_freshness
assert osm_geodata_freshness.get('freshness_status') == 'fixture_static_fresh_live_stale_blocked', osm_geodata_freshness
assert osm_geodata_freshness.get('fixture_static_snapshot') is True, osm_geodata_freshness
assert osm_geodata_freshness.get('wall_clock_freshness_applies') is False, osm_geodata_freshness
assert osm_geodata_freshness.get('live_data_freshness_applies') is False, osm_geodata_freshness
assert osm_geodata_freshness.get('fixture_snapshot_age_seconds') == 0, osm_geodata_freshness
assert osm_geodata_freshness.get('live_ingestion_disabled') is True, osm_geodata_freshness
assert osm_geodata_freshness.get('stale_live_ingestion_blocked') is True, osm_geodata_freshness
assert osm_geodata_freshness.get('freshness_green') is True, osm_geodata_freshness
osm_attribution_presence = health.get('trillionnium_openstreetmap_attribution_presence_gate') or {}
assert osm_attribution_presence.get('contract_version') == 'trillionnium_openstreetmap_attribution_presence_gate_v1', osm_attribution_presence
assert osm_attribution_presence.get('attribution_presence_contract_version') == 'openstreetmap_attribution_presence_v1', osm_attribution_presence
assert osm_attribution_presence.get('attribution') == '© OpenStreetMap contributors', osm_attribution_presence
assert osm_attribution_presence.get('database_license') == 'ODbL-1.0', osm_attribution_presence
assert osm_attribution_presence.get('attribution_required') is True, osm_attribution_presence
assert osm_attribution_presence.get('attribution_visible_required') is True, osm_attribution_presence
assert osm_attribution_presence.get('derived_database_tracking_required') is True, osm_attribution_presence
assert osm_attribution_presence.get('odbl_database_obligations_visible') is True, osm_attribution_presence
assert osm_attribution_presence.get('attribution_presence_green') is True, osm_attribution_presence

summary = {
    'ok': True,
    'checked_at_epoch': int(time.time()),
    'base_url': base,
    'actions': actions,
    'has_title': 'Trillionnium League' in html_after,
    'has_web_console': '网页战斗台' in html_after,
    'has_web_session': bool(session_cookie and csrf),
    'has_client_app_shell': 'Trillionnium World' in app_html and '移动世界壳 v1' in app_html,
    'has_client_app_mobile_shell': 'app-global-search' in app_html and 'app-search-clear' in app_html and 'app-bottom-tabs' in app_html and 'app-tab-map' in app_html,
    'has_client_app_mobile_tabs': all(label in app_html for label in ['Messages', 'World', 'Feed', 'Me', '消息', '世界', '动态', '我']),
    'has_client_app_language_settings': all(token in app_html for token in ['TrillionniumLanguage', 'trillionnium.ui.language', 'data-trillionnium-language-select', 'trillionnium-app-language-select', 'trillionnium-system-language-settings', 'System Settings', '系统设置', 'English', '中文']),
    'has_client_app_mobile_a11y_ux': all(token in app_html for token in ['app-ux-live-status', 'app-search-empty-state', 'role="tablist"', 'role="tab"', 'role="tabpanel"', 'aria-selected="true"', 'handleAppTabKeydown', 'announceUxStatus', 'trillionnium_mobile_shell_ux_v1', 'keyboard_tab_navigation_visible', 'offline_feed_fallback_status_visible', 'web_session_feed_hydration_visible', '/app/web/feed']),
    'has_client_app_mobile_single_primary_cta': all(token in app_html for token in ['trillionnium_mobile_single_primary_cta_v1', 'mobile_bottom_sheet_single_primary_cta_visible', 'app-mobile-action-sheet', 'app-mobile-primary-cta', 'data-primary-cta-count="1"', 'Continue Route', 'data-primary-cta-target="app-map-action-rail"']),
    'has_client_app_tactics_player_surface': all(token in app_html for token in ['app-tactics-player-hud', 'trillionnium_tactics_player_visible_surface_v1', 'app-tactics-objective-card', 'app-tactics-current-session-card', 'app-tactics-intent-draft-card', 'app-tactics-reward-history-handoff', 'app-tactics-repeat-farming-copy', 'trillionnium_tactics_board_cell_interaction_v1', 'trillionnium_tactics_unit_selection_v1', 'trillionnium_tactics_command_intent_draft_v1', 'data-reward-history-contract="trillionnium_tactics_reward_history_v1"', 'data-anti-cheese-contract="trillionnium_tactics_repeat_farming_anti_cheese_v1"', 'Repeat-farming guard']),
    'has_client_app_mobile_copy_layering': all(token in app_html for token in ['trillionnium_mobile_copy_layering_v1', 'mobile_copy_layering_visible', 'app-map-copy-summary', 'app-map-copy-layer-details', 'data-default-state="collapsed"', 'Pick a nearby route', 'Why this map matters']),
    'has_client_app_map_readability_lod': all(token in app_html for token in ['trillionnium_world_map_readability_lod_v1', 'trillionnium_world_map_runtime_performance_budget_v1', 'trillionnium_world_map_transport_delta_v1', 'trillionnium_world_map_first_screen_decision_v1', 'trillionnium_world_map_renderer_shadow_v1', 'trillionnium_world_map_rum_slo_v1', 'trillionnium_world_map_weak_network_resilience_v1', 'trillionnium_world_map_location_privacy_v1', 'trillionnium_world_route_recommendation_policy_v1', 'trillionnium_world_map_subsystem_v1', 'trillionnium_world_map_game_layer_semantics_v1', 'map_readability_lod_visible', 'map_rum_slo_quantiles_visible', 'map_weak_network_resilience_visible', 'map_location_privacy_visible', 'map_game_layer_semantics_visible', 'app-map-readability-lod', 'app-map-performance-budget', 'app-map-transport-delta', 'app-map-rum-slo', 'app-map-weak-network', 'app-map-location-privacy', 'app-map-first-screen-decision', 'data-visible-marker-budget="18"', 'One route first']),
    'has_client_app_route_runner_funnel_telemetry': all(token in app_html for token in ['app-route-runner-funnel-telemetry', 'trillionnium_route_runner_funnel_telemetry_v1', 'data-time-to-reward-target-seconds="1800"', 'data-reward-to-next-route-percent=', 'app-route-archetype-catalog', 'app-commercial-operating-dashboard']),
    'has_client_app_future_engine_readiness': all(token in app_html for token in ['trillionnium_world_future_engine_readiness_v1', 'leaflet_openstreetmap_v1', 'maplibre_gl_v1', 'mapRuntime']),
    'has_client_app_first_playable_onboarding': 'app-first-playable-onboarding' in app_html and '新手主线' in app_html and 'first_playable_loop_100' in app_html and 'trillionnium_first_playable_onboarding_v1' in app_html and 'data-onboarding-step="quest_delivery"' in app_html and 'route_task_graph_next_action_visible' in app_html,
    'has_client_app_world_map': 'real-world-map' in app_html and 'leaflet_openstreetmap_v1' in app_html and 'OpenStreetMap' in app_html,
    'has_client_app_feed_api': '/app/web/feed' in app_html and 'Feed API' in app_html,
    'has_client_app_feed_surface': 'app-feed-api-status' in app_html and 'app-feed-summary' in app_html and 'app-feed-items-live' in app_html and '世界动态时间线' in app_html,
    'has_client_app_feed_filters': 'app-feed-filter-actions' in app_html and 'trillionnium-app-feed-filter' in app_html and '推荐' in app_html and '委托' in app_html and '冒险' in app_html,
    'has_client_app_feed_api_hydration': 'loadFeedSurface' in app_html and '动态已同步' in app_html and 'trillionnium-app-feed-action' in app_html,
    'has_client_app_feed_route_runner_handoff_static': 'app-feed-route-runner-handoff' in app_html and 'route_runner_handoff' in app_html and 'trillionnium_route_runner_handoff_v1' in app_html and 'trillionnium_route_runner_lifecycle_v1' in app_html and 'trillionnium_route_mastery_v1' in app_html and 'data-next-route-status' in app_html and 'data-route-mastery-tier' in app_html,
    'has_client_app_feed_route_runner_handoff_api': feed_route_runner_handoff_ok(feed_json),
    'client_app_feed_source_count': feed_json.get('source_count'),
    'client_app_feed_sources': feed_json.get('sources'),
    'client_app_feed_route_runner_handoff_contract_version': (feed_json.get('route_runner_handoff') or {}).get('contract_version'),
    'client_app_feed_route_runner_count': (feed_json.get('route_runner_handoff') or {}).get('runner_count'),
    'client_app_feed_route_runner_reward_claim_action_count': (feed_json.get('route_runner_handoff') or {}).get('reward_claim_action_count'),
    'client_app_feed_route_runner_next_route_action_count': (feed_json.get('route_runner_handoff') or {}).get('next_route_action_count'),
    'client_app_feed_route_runner_next_route_status': (feed_json.get('route_runner_handoff') or {}).get('first_next_route_status'),
    'client_app_feed_route_runner_next_route_sequence_summary': (feed_json.get('route_runner_handoff') or {}).get('first_next_route_sequence_summary'),
    'client_app_feed_route_mastery_contract_version': (feed_json.get('route_runner_handoff') or {}).get('route_mastery_contract_version'),
    'client_app_feed_route_mastery_xp': (feed_json.get('route_runner_handoff') or {}).get('first_route_mastery_xp'),
    'has_client_app_real_world_map_engine': 'real-world-map' in app_html and 'leaflet_openstreetmap_v1' in app_html and 'OpenStreetMap' in app_html,
    'has_client_app_map_renderer_adapter': 'createRealWorldMapAdapter' in app_html and 'leaflet_renderer_adapter_v1' in app_html and 'maplibre_gl_v1' in app_html and 'const mapRuntime' in app_html and 'supports_future_engine_swap' in app_html and 'gating_contract' in app_html and 'leafletMap' not in app_html and 'renderRouteLine' in app_html and 'renderTileFrame' in app_html and 'renderEventPulse' in app_html and 'onViewportChange' in app_html and 'getCenter' in app_html and 'getZoom' in app_html,
    'has_client_app_live_viewport_hydration': '/world/web/map-viewport' in app_html and 'app-map-camera-summary' in app_html,
    'has_client_app_stream_hud': 'app-map-stream-hud' in app_html and 'app-map-overlay-legend' in app_html,
    'has_client_app_overlay_controls': 'app-map-overlay-controls' in app_html and 'trillionnium-overlay-toggle' in app_html and 'trillionnium-map-camera-action' in app_html,
    'has_client_app_focus_action_rail': 'app-map-focus-summary' in app_html and 'app-map-action-rail' in app_html and '地图行动栏' in app_html,
    'has_client_app_world_handoff_bridge': 'trillionnium-world-handoff' in app_html and '地图行动已准备' in app_html or '冒险行动已准备' in app_html,
    'has_client_app_route_cockpit': 'app-map-route-status' in app_html and '推荐下一步' in app_html and '关联任务路线' in app_html and '起草任务后续' in app_html,
    'has_client_app_route_filter_controls': 'app-map-route-filter-actions' in app_html and '按焦点筛选路线' in app_html and '显示完整路线' in app_html,
    'has_client_app_event_route_brief': 'app-map-route-event-brief-status' in app_html and '事件简报：' in app_html,
    'has_client_app_linked_event_handoff': 'web_event_id' in app_html and '打开关联事件' in app_html,
    'has_client_app_route_preview': 'app-route-preview-live' in app_html and '冒险路线预览' in app_html,
    'has_client_app_route_task_graph': 'app-route-task-graph-live' in app_html and '任务路线图' in app_html and ('打开关联契约' in app_html or '起草任务后续' in app_html),
    'has_client_app_route_opportunity_lane': 'app-route-task-graph-live' in app_html and '下一条支线' in app_html and '推进下一条支线' in app_html,
    'has_client_app_route_node_handoff': 'resolveRouteTargetNodeId' in app_html and 'data-target-node-id=' in app_html and 'next_opportunity_node_id' in app_html,
    'has_client_app_tile_shards': 'app-tile-shards-live' in app_html and 'osm-z15' in app_html,
    'has_client_app_prefetch_queue': 'app-prefetch-queue-live' in app_html and '预热探索圈' in app_html,
    'has_client_app_live_event_stream': 'app-live-events-live' in app_html and '实时事件' in app_html,
    'has_client_app_event_route_focus': 'findLiveEventByFocus' in app_html and 'data-focus-kind="event"' in app_html and 'data-task-id=' in app_html,
    'has_client_app_event_overlay_focus': 'buildEventFocus' in app_html,
    'has_client_app_stream_lens': 'filterLiveEventStream' in app_html and '条事件镜头' in app_html,
    'has_client_app_map_marker_actions': 'primary_actions' in app_html and 'trillionnium-map-action' in app_html,
    'has_client_app_map_focus_controls': 'trillionnium-map-focus' in app_html and '聚焦热点' in app_html,
    'has_client_app_global_map_mirror': 'global_real_world_tiles' in app_html and 'gather_hero_tale_lod' in app_html,
    'has_client_app_openstreetmap_attribution': all(token in app_html for token in ['app-openstreetmap-attribution', 'openstreetmap_attribution_presence_v1', '© OpenStreetMap contributors', 'ODbL-1.0', 'data-attribution-required="true"', 'data-attribution-visible="true"', 'data-derived-database-tracking-required="true"']),
    'has_client_app_face_duel': '面对面切磋' in app_html,
    'has_client_app_social': '队友消息' in app_html,
    'has_client_app_wallet': '奖励钱包' in app_html,
    'has_client_app_progression': '角色成长' in app_html and '/progression' in app_html,
    'has_world_shell': 'Trillionnium World' in world_html_after and '世界行动台' in world_html_after,
    'has_world_action': world_marker in world_html_after,
    'has_world_real_world_map_engine': 'world-real-map' in world_html_after and 'global_real_world_tiles' in world_html_after,
    'has_world_map_renderer_adapter': 'createRealWorldMapAdapter' in world_html_after and 'leaflet_renderer_adapter_v1' in world_html_after and 'maplibre_gl_v1' in world_html_after and 'const mapRuntime' in world_html_after and 'supports_future_engine_swap' in world_html_after and 'gating_contract' in world_html_after and 'leafletMap' not in world_html_after and 'renderRouteLine' in world_html_after and 'renderTileFrame' in world_html_after and 'renderEventPulse' in world_html_after and 'onViewportChange' in world_html_after and 'getCenter' in world_html_after and 'getZoom' in world_html_after,
    'has_world_tactics_player_surface': all(token in world_html_after for token in ['world-tactics-player-hud', 'trillionnium_tactics_player_visible_surface_v1', 'world-tactics-objective-card', 'world-tactics-current-session-card', 'world-tactics-command-draft-panel', 'world-tactics-command-draft-form', 'world-tactics-reward-history-handoff', 'world-tactics-repeat-farming-copy', 'trillionnium_tactics_board_cell_interaction_v1', 'trillionnium_tactics_unit_selection_v1', 'trillionnium_tactics_command_intent_draft_v1', 'trillionnium_tactics_accessibility_v1', 'data-keyboard-traversal="roving_grid_focus"', 'data-low-motion-support="prefers_reduced_motion"', 'world-tactics-keyboard-help', 'role="gridcell"', 'data-roving-tabindex="tactics_board"', 'aria-live="polite"', 'focusAdjacentTile', 'data-draft-target-tile=', 'data-draft-unit-id=', 'data-draft-command=', 'initializeTacticsIntentDraft', 'window.trillionniumTacticsIntentDraft', 'data-reward-history-contract="trillionnium_tactics_reward_history_v1"', 'data-anti-cheese-contract="trillionnium_tactics_repeat_farming_anti_cheese_v1"', 'Repeat-farming guard']),
    'has_world_map_optimization_contracts': all(token in world_html_after for token in ['world-map-readability-lod', 'world-map-performance-budget', 'world-map-transport-delta', 'world-map-shadow-renderer', 'world-map-rum-slo', 'world-map-weak-network', 'world-map-location-privacy', 'trillionnium_world_map_readability_lod_v1', 'trillionnium_world_map_runtime_performance_budget_v1', 'trillionnium_world_map_transport_delta_v1', 'trillionnium_world_map_renderer_shadow_v1', 'trillionnium_world_map_rum_slo_v1', 'trillionnium_world_map_weak_network_resilience_v1', 'trillionnium_world_map_location_privacy_v1', 'data-parity-source="app-map-readability-lod"', 'data-parity-source="app-map-transport-delta"']),
    'has_world_openstreetmap_provider_readiness': all(token in world_html_after for token in ['world-openstreetmap-provider-readiness', 'openstreetmap_provider_readiness_v1', 'fixture_ready_live_fail_closed', 'data-fixture-mode-green="true"', 'data-live-modes-fail-closed="true"', 'data-live-network-ingestion-enabled="false"', 'data-production-ingestion-enabled="false"', 'overpass_bbox_cache', 'geofabrik_extract_import', 'vendor_tile_cache']),
    'has_world_openstreetmap_geodata_freshness': all(token in world_html_after for token in ['world-openstreetmap-geodata-freshness', 'openstreetmap_geodata_freshness_v1', 'fixture_static_fresh_live_stale_blocked', 'data-fixture-static-snapshot="true"', 'data-wall-clock-freshness-applies="false"', 'data-live-data-freshness-applies="false"', 'data-staleness-gate-green="true"', 'data-stale-live-ingestion-blocked="true"', 'data-fixture-snapshot-age-seconds="0"']),
    'has_world_openstreetmap_attribution': all(token in world_html_after for token in ['world-openstreetmap-attribution', 'openstreetmap_attribution_presence_v1', '© OpenStreetMap contributors', 'ODbL-1.0', 'data-attribution-required="true"', 'data-attribution-visible="true"', 'data-derived-database-tracking-required="true"', 'odbl_database_obligations']),
    'has_world_live_viewport_hydration': '/world/web/map-viewport' in world_html_after and 'world-map-camera-summary' in world_html_after,
    'has_world_stream_hud': 'world-map-stream-hud' in world_html_after and 'world-map-overlay-legend' in world_html_after,
    'has_world_overlay_controls': 'world-map-overlay-controls' in world_html_after and 'trillionnium-overlay-toggle' in world_html_after and 'trillionnium-map-camera-action' in world_html_after,
    'has_world_focus_action_rail': 'world-map-focus-summary' in world_html_after and 'world-map-action-rail' in world_html_after and '地图行动栏' in world_html_after,
    'has_world_handoff_bootstrap': 'trillionnium-world-handoff' in world_html_after and '来自 /app 的行动' in world_html_after,
    'has_world_route_filter_controls': 'world-map-route-filter-status' in world_html_after and 'trillionnium-route-filter-action' in world_html_after and '路线筛选：' in world_html_after,
    'has_world_event_route_brief': 'world-map-route-event-brief-status' in world_html_after and '事件简报：' in world_html_after,
    'has_world_timeline_event_focus': 'focusRouteEvent' in world_html_after and 'world-event-timeline-item-' in world_html_after and '打开关联事件' in world_html_after or '打开关联事件' in world_html_after,
    'has_world_route_flow_routing': 'world-map-route-flow-status' in world_html_after and 'world-map-route-flow-actions' in world_html_after and 'world-work-deliver-id' in world_html_after and 'world-contract-completion-id' in world_html_after,
    'has_world_route_action_console': 'world-action-console-status' in world_html_after and '起草世界行动' in world_html_after or '起草世界行动' in world_html_after and '当前世界行动：' in world_html_after,
    'has_world_route_next_step': 'world-map-route-next-step-status' in world_html_after and '推荐下一步' in world_html_after and ('打开评级路线' in world_html_after or '打开重开路线' in world_html_after or '再次提交成果' in world_html_after or '打开成果提交路线' in world_html_after or '起草后续行动' in world_html_after or '打开契约路线' in world_html_after or '打开任务牌路线' in world_html_after or '推进委托' in world_html_after),
    'has_world_route_task_linkage': 'world-map-route-link-status' in world_html_after and '关联任务路线' in world_html_after and ('起草任务后续' in world_html_after or '打开关联契约' in world_html_after or '打开关联事件' in world_html_after or '打开关联事件' in world_html_after),
    'has_world_route_task_graph': 'world-route-task-graph-live' in world_html_after and '任务路线图' in world_html_after and ('起草任务后续' in world_html_after or '打开关联契约' in world_html_after),
    'has_world_route_opportunity_lane': 'world-route-task-graph-live' in world_html_after and '下一条支线' in world_html_after and '推进下一条支线' in world_html_after,
    'has_world_route_node_focus': 'focusRouteTarget' in world_html_after and 'data-target-node-id=' in world_html_after and 'next_opportunity_node_id' in world_html_after,
    'has_world_tile_shards': 'world-tile-shards-live' in world_html_after and 'osm-z15' in world_html_after,
    'has_world_prefetch_queue': 'world-prefetch-queue-live' in world_html_after,
    'has_world_live_event_stream': 'world-live-events-live' in world_html_after,
    'has_world_event_route_focus': 'findLiveEventByFocus' in world_html_after and 'data-focus-kind="event"' in world_html_after and 'data-task-id=' in world_html_after,
    'has_world_event_overlay_focus': 'buildEventFocus' in world_html_after,
    'has_world_stream_lens': 'filterLiveEventStream' in world_html_after and '条事件镜头' in world_html_after,
    'has_world_map_marker_actions': 'primary_actions' in world_html_after and 'trillionnium-map-action' in world_html_after and 'world-map-move-target' in world_html_after and 'world-action-body' in world_html_after,
    'has_world_map_focus_controls': 'trillionnium-map-focus' in world_html_after and '聚焦区域' in world_html_after,
    'has_world_map_viewport_contract': ('/world/web/map-viewport' in world_html_after or '/v1/world/map/' in world_html_after) and 'street_level_world_nodes' in world_html_after,
    'has_world_secondary_dashboard_panels': all(token in world_html_after for token in ['trillionnium_secondary_dashboard_panels_v1', 'data-secondary-dashboard-role="secondary_detail_panel"', 'data-secondary-dashboard-role="secondary_counter_drawer"', 'data-secondary-dashboard-role="supporting_engine_diagnostics"', 'data-main-experience="false"', 'data-default-state="collapsed_on_mobile"', 'data-primary-loop-anchor="trillionnium-tactics-game-shell"']),
    'has_world_map_panel': '详细世界地图' in world_html_after,
    'has_world_map_move_form': '移动到这里' in world_html_after,
    'has_world_contracts_panel': '世界契约' in world_html_after,
    'has_world_contract_completion_form': '完成契约' in world_html_after,
    'has_world_asset_upgrade_form': '升级道具' in world_html_after,
    'has_world_company_form': '建立工坊' in world_html_after,
    'has_world_listing_form': '发布任务牌' in world_html_after,
    'has_world_buy_form': '接取任务牌' in world_html_after,
    'has_world_work_delivery_form': '提交成果' in world_html_after,
    'has_world_work_acceptance_form': '评级通过' in world_html_after,
    'has_world_work_rejection_form': '要求返工' in world_html_after,
    'has_world_work_reopen_form': '重开委托' in world_html_after,
    'has_world_work_cancel_form': '放弃委托' in world_html_after,
    'has_world_factions_panel': '阵营声望图' in world_html_after,
    'has_world_work_orders': '冒险委托' in world_html_after,
    'has_timeline': '战斗时间线' in html_after,
    'has_progression_systems': '角色成长系统' in html_after and '/skills' in html_after and '/tools' in html_after and '/skins' in html_after,
    'has_settled_reward': 'settled' in html_after,
    'has_held_review': 'held_review' in html_after,
    'has_hidden_judge': 'rubric_hidden' in html_after,
    'trillionnium_world_maturity_overall_percent': maturity.get('overall_percent'),
    'trillionnium_world_maturity_first_playable_percent': axes.get('first_playable', {}).get('percent'),
    'trillionnium_world_maturity_technical_alpha_percent': axes.get('technical_alpha', {}).get('percent'),
    'trillionnium_world_maturity_beta_readiness_percent': axes.get('beta_readiness', {}).get('percent'),
    'trillionnium_world_maturity_full_vision_percent': axes.get('full_vision', {}).get('percent'),
    'trillionnium_world_closed_beta_prototype_overall_percent': closed_beta.get('overall_percent'),
    'trillionnium_world_closed_beta_prototype_product_loop_percent': closed_beta_axes.get('product_loop', {}).get('percent'),
    'trillionnium_world_closed_beta_prototype_access_governance_percent': closed_beta_axes.get('access_governance', {}).get('percent'),
    'trillionnium_world_closed_beta_prototype_persistence_runtime_percent': closed_beta_axes.get('persistence_runtime', {}).get('percent'),
    'trillionnium_world_closed_beta_prototype_world_depth_percent': closed_beta_axes.get('world_depth', {}).get('percent'),
    'trillionnium_world_closed_beta_prototype_commerce_recovery_percent': closed_beta_axes.get('commerce_recovery', {}).get('percent'),
    'trillionnium_world_real_user_beta_overall_percent': real_user_beta.get('overall_percent'),
    'trillionnium_world_real_user_beta_product_retention_percent': real_user_beta_axes.get('product_retention', {}).get('percent'),
    'trillionnium_world_real_user_beta_access_safety_percent': real_user_beta_axes.get('access_safety', {}).get('percent'),
    'trillionnium_world_real_user_beta_durable_persistence_percent': real_user_beta_axes.get('durable_persistence', {}).get('percent'),
    'trillionnium_world_real_user_beta_economy_recovery_percent': real_user_beta_axes.get('economy_recovery', {}).get('percent'),
    'trillionnium_world_real_user_beta_world_capacity_percent': real_user_beta_axes.get('world_capacity', {}).get('percent'),
    'trillionnium_world_real_user_beta_ops_runtime_percent': real_user_beta_axes.get('ops_runtime', {}).get('percent'),
    'trillionnium_world_public_commercial_product_overall_percent': public_commercial.get('overall_percent'),
    'trillionnium_world_public_commercial_product_public_launch_surface_percent': public_commercial_axes.get('public_launch_surface', {}).get('percent'),
    'trillionnium_world_public_commercial_product_commercial_engine_percent': public_commercial_axes.get('commercial_engine', {}).get('percent'),
    'trillionnium_world_public_commercial_product_trust_safety_percent': public_commercial_axes.get('trust_safety', {}).get('percent'),
    'trillionnium_world_public_commercial_product_durable_scale_ops_percent': public_commercial_axes.get('durable_scale_ops', {}).get('percent'),
    'trillionnium_world_public_commercial_product_growth_network_percent': public_commercial_axes.get('growth_network', {}).get('percent'),
    'trillionnium_world_public_commercial_product_public_world_depth_percent': public_commercial_axes.get('public_world_depth', {}).get('percent'),
    'repository_effective_repository': health.get('league_repository_runtime', {}).get('effective_repository'),
    'repository_cutover_status': health.get('league_repository_runtime', {}).get('repository_cutover_status'),
    'openstreetmap_provider_readiness_contract_version': osm_provider_readiness.get('readiness_contract_version'),
    'openstreetmap_provider_readiness_status': osm_provider_readiness.get('readiness_status'),
    'openstreetmap_fixture_mode_green': osm_provider_readiness.get('fixture_mode_green'),
    'openstreetmap_live_modes_fail_closed': osm_provider_readiness.get('live_modes_fail_closed'),
    'openstreetmap_geodata_freshness_contract_version': osm_geodata_freshness.get('freshness_contract_version'),
    'openstreetmap_geodata_freshness_status': osm_geodata_freshness.get('freshness_status'),
    'openstreetmap_geodata_fixture_snapshot_age_seconds': osm_geodata_freshness.get('fixture_snapshot_age_seconds'),
    'openstreetmap_geodata_stale_live_ingestion_blocked': osm_geodata_freshness.get('stale_live_ingestion_blocked'),
    'openstreetmap_attribution_presence_contract_version': osm_attribution_presence.get('attribution_presence_contract_version'),
    'openstreetmap_attribution_visible_required': osm_attribution_presence.get('attribution_visible_required'),
    'openstreetmap_attribution_odbl_obligations_visible': osm_attribution_presence.get('odbl_database_obligations_visible'),
    'marker': marker,
}
path = summary_dir / f'web-e2e-summary-{int(time.time())}.json'
path.write_text(json.dumps(summary, ensure_ascii=False, indent=2))
print(json.dumps({'ok': True, 'summary': str(path), **summary}, ensure_ascii=False, indent=2))
PY
