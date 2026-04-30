#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
BASE_URL="${CONSUMER_ENTRY_BASE_URL:-http://127.0.0.1:8090}"
SUMMARY_DIR="$ROOT_DIR/run/league-web"
mkdir -p "$SUMMARY_DIR"

python3 - <<'PY'
import json, os, pathlib, re, time, urllib.parse, urllib.request

base = os.environ.get('CONSUMER_ENTRY_BASE_URL', 'http://127.0.0.1:8090').rstrip('/')
root = pathlib.Path.cwd()
summary_dir = root / 'run/league-web'
summary_dir.mkdir(parents=True, exist_ok=True)

def get(path):
    with urllib.request.urlopen(base + path, timeout=20) as resp:
        body = resp.read().decode('utf-8', errors='replace')
        return resp.status, body

def post_form(path, headers=None, **fields):
    data = urllib.parse.urlencode(fields).encode()
    request_headers = {'content-type': 'application/x-www-form-urlencoded'}
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

status, html = get('/league')
assert status == 200, status
for needle in ['Trillionnium League', 'Trillionnium World', 'Web Battle Console', 'Guild Halls', 'Battle Timeline', 'Submit Result', 'Progression Systems', '/skills', '/tools', '/skins']:
    assert needle in html, needle

world_status, world_html = get('/world')
assert world_status == 200, world_status
for needle in ['Trillionnium World', 'World Action Console', 'Reality Mirror Sandbox', 'Global Real-world Map Engine', 'world-real-map', 'createRealWorldMapAdapter', 'leaflet_renderer_adapter_v1', 'maplibre_gl_v1', 'const mapRuntime', 'supports_future_engine_swap', 'gating_contract', 'renderRouteLine', 'renderTileFrame', 'renderEventPulse', 'onViewportChange', 'getCenter', 'getZoom', 'global_real_world_tiles', 'cn-shanghai-core', 'street_level_world_nodes', 'osm-z15', 'primary_actions', 'trillionnium-map-action', 'Move here', 'world-map-move-target', 'world-action-body', 'world-action-console-status', 'Draft world action', 'Draft task follow-up', 'Route next opportunity', 'Opportunity lane', 'Focused event brief:', 'Linked task route:', 'Recommended next step:', 'Task-linked Route Graph', 'world-route-task-graph-live', '/v1/world/map/@alice:local.dev/viewport', '/world/web/map-viewport', 'world-map-camera-summary', 'world-map-route-flow-status', 'world-map-route-next-step-status', 'world-map-route-link-status', 'world-work-deliver-id', 'world-work-deliver-body', 'world-buy-body', 'world-contract-completion-id', 'world-contract-completion-body', 'world-company-asset-id', 'world-listing-company-id', 'world-tile-shards-live', 'Player Assets', 'Upgrade Asset', 'Companies / Shops', 'Launch Company', 'Shops / Listings', 'Publish Listing', 'Commerce / Work Orders', 'Buy / Hire Listing', 'Deliver Work Order', 'Accept Work Order', 'Faction Reputation Map', 'World Contracts', 'Complete Contract', 'findLiveEventByFocus', 'buildEventFocus', 'focusRouteEvent', 'filterLiveEventStream', 'stream lens', 'data-focus-kind="event"', 'data-event-id=', 'data-task-id=', 'world-event-timeline-item-']:
    assert needle in world_html, needle

matrix_user_id = '@alice:local.dev'
marker = f'web-e2e-{int(time.time())}'
actions = []
session_status, session_headers, session_body = post_json('/league/web/session', {
    'matrix_user_id': matrix_user_id,
    'room_id': '!web-local:local.dev',
    'session_id': 'web-e2e-session',
})
assert session_status == 200, session_status
session_cookie = session_headers.get('Set-Cookie') or session_headers.get('set-cookie')
assert session_cookie and 'cex_league_session=' in session_cookie, session_headers
csrf = session_body.get('csrf')
assert csrf, session_body
cookie_header = {'Cookie': session_cookie.split(';', 1)[0]}
status, html_with_session = get('/league')
assert status == 200, status
app_status, app_html = get('/app')
assert app_status == 200 and 'Trillionnium Client App' in app_html, app_status
for needle in ['real-world-map', 'leaflet_openstreetmap_v1', 'createRealWorldMapAdapter', 'leaflet_renderer_adapter_v1', 'maplibre_gl_v1', 'const mapRuntime', 'supports_future_engine_swap', 'gating_contract', 'renderRouteLine', 'renderTileFrame', 'renderEventPulse', 'onViewportChange', 'getCenter', 'getZoom', 'OpenStreetMap', 'Leaflet', 'tile.openstreetmap.org', 'global_real_world_tiles', 'gather_hero_tale_lod', 'cn-shanghai-core', 'primary_actions', 'trillionnium-map-action', 'Move here', '/v1/world/map/{matrix_user_id}/viewport', '/world/web/map-viewport', '/v1/client/feed/@alice:local.dev', 'app-global-search', 'app-bottom-tabs', 'app-tab-messages', 'app-tab-map', 'app-tab-feed', 'app-tab-me', '消息', '世界', '动态', '我', 'app-first-playable-onboarding', 'app-first-playable-checks', 'app-first-playable-steps', 'First playable onboarding', 'trillionnium_first_playable_onboarding_v1', 'first_playable_loop_100', 'data-onboarding-step="commerce_delivery"', 'route_task_graph_next_action_visible', 'app-map-camera-summary', 'app-map-route-status', 'Recommended world handoff:', 'Focused event brief:', 'Linked task route:', 'Draft task follow-up', 'Route next opportunity', 'Opportunity lane', 'Filter route by focus', 'Show full route', 'app-route-preview-live', 'World Route Preview', 'app-route-task-graph-live', 'Task-linked Route Graph', 'Route linked contract', 'app-feed-api-status', 'app-feed-filter-actions', 'app-feed-summary', 'app-feed-items-live', 'Unified Feed Timeline', 'trillionnium-app-feed-filter', 'trillionnium-app-feed-action', 'loadFeedSurface', 'app-tile-shards-live', 'Tile Shards', 'Region Shards', 'POI Hotspots', 'Progression', '/progression', '/skills /tools /skins', 'findLiveEventByFocus', 'buildEventFocus', 'filterLiveEventStream', 'stream lens', 'web_event_id', 'data-focus-kind="event"', 'data-event-id=', 'data-task-id=']:
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
    'body': f'{world_marker}: 我要在镜像城市开一家 AI 设计公司，招募 Agent，服务真实客户。',
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
    'body': f'Web asset upgrade {marker}: offer, evidence package, risk control checklist, operating cadence, acceptance standard, next customer path, and self-review.',
})
assert code == 200 and 'Trillionnium World' in body, ('world_asset', code, url)
actions.append({'action': 'world_asset', 'status': code, 'url': url})

code, url, body = post_form('/world/web/company', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'asset_id': 'latest',
    'body': f'Web company launch {marker}: offer, target customer, revenue path, operating loop, evidence, risk controls, acceptance standard, and next sale.',
})
assert code == 200 and 'Trillionnium World' in body, ('world_company', code, url)
actions.append({'action': 'world_company', 'status': code, 'url': url})

code, url, body = post_form('/world/web/listing', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'company_id': 'latest',
    'body': f'Web listing {marker}: AI service package with deliverable, price logic, evidence package, customer promise, risk controls, self-review, acceptance standard, revision policy, and next action.',
})
assert code == 200 and 'Trillionnium World' in body, ('world_listing', code, url)
actions.append({'action': 'world_listing', 'status': code, 'url': url})

code, url, body = post_form('/world/web/buy', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'listing_id': 'latest',
    'body': f'Web purchase {marker}: buy the listing, open work order, define deliverable, evidence, acceptance standard, risk controls, and next action.',
})
assert code == 200 and 'Trillionnium World' in body, ('world_buy', code, url)
actions.append({'action': 'world_buy', 'status': code, 'url': url})

code, url, body = post_form('/world/web/work-deliver', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'work_order_id': 'latest',
    'body': f'Web work delivery {marker}: final deliverable, evidence package, acceptance checklist, risk review, next action, and self-review confirming completion.',
})
assert code == 200 and 'Trillionnium World' in body, ('world_work_deliver', code, url)
actions.append({'action': 'world_work_deliver', 'status': code, 'url': url})

code, url, body = post_form('/world/web/work-accept', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'work_order_id': 'latest',
    'body': f'Web work acceptance {marker}: buyer confirms quality, evidence, acceptance standard, and next collaboration.',
})
assert code == 200 and 'Trillionnium World' in body, ('world_work_accept', code, url)
actions.append({'action': 'world_work_accept', 'status': code, 'url': url})

code, url, body = post_form('/world/web/buy', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'listing_id': 'latest',
    'body': f'Web reject-flow purchase {marker}: open a second work order to validate buyer refund, evidence gaps, revision requirements, and next action.',
})
assert code == 200 and 'Trillionnium World' in body, ('world_reject_buy', code, url)
actions.append({'action': 'world_reject_buy', 'status': code, 'url': url})

code, url, body = post_form('/world/web/work-deliver', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'work_order_id': 'latest',
    'body': f'Web reject-flow delivery {marker}: deliverable, evidence package, quality notes, risk review, and next action.',
})
assert code == 200 and 'Trillionnium World' in body, ('world_reject_deliver', code, url)
actions.append({'action': 'world_reject_deliver', 'status': code, 'url': url})

code, url, body = post_form('/world/web/work-reject', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'work_order_id': 'latest',
    'body': f'Web work rejection {marker}: buyer rejects delivery, refunds reserved funds, records evidence gaps, revision requirements, and next action.',
})
assert code == 200 and 'Trillionnium World' in body, ('world_work_reject', code, url)
actions.append({'action': 'world_work_reject', 'status': code, 'url': url})

code, url, body = post_form('/world/web/work-reopen', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'work_order_id': 'latest',
    'body': f'Web work reopen {marker}: buyer reserves funds again, lists revision requirements, evidence gaps, acceptance standard, and next redelivery action.',
})
assert code == 200 and 'Trillionnium World' in body, ('world_work_reopen', code, url)
actions.append({'action': 'world_work_reopen', 'status': code, 'url': url})

code, url, body = post_form('/world/web/work-cancel', headers=cookie_header, **{
    'matrix_user_id': matrix_user_id,
    'csrf': csrf,
    'work_order_id': 'latest',
    'body': f'Web work cancel {marker}: buyer cancels this open work before delivery, refunds reserved funds, records reason, and closes the order.',
})
assert code == 200 and 'Trillionnium World' in body, ('world_work_cancel', code, url)
actions.append({'action': 'world_work_cancel', 'status': code, 'url': url})

for fields in [
    {'action': 'join', 'matrix_user_id': matrix_user_id, 'match_id': 'daily-dungeon-001'},
    {'action': 'guild', 'matrix_user_id': matrix_user_id, 'guild_id': 'guild-prompt-forge'},
    {'action': 'team', 'matrix_user_id': matrix_user_id, 'match_id': 'guild-raid-001', 'role': 'scout', 'heroes': 'oracle_scout'},
    {'action': 'draft', 'matrix_user_id': matrix_user_id, 'heroes': 'oracle_scout forge_builder mirror_auditor courier_closer'},
    {
        'action': 'raid',
        'matrix_user_id': matrix_user_id,
        'match_id': 'guild-raid-001',
        'body': f'Raid contribution {marker}: scout evidence, assign builder, define risk gate, next step.',
    },
    {
        'action': 'submit',
        'matrix_user_id': matrix_user_id,
        'match_id': 'daily-dungeon-001',
        'body': f'Web E2E result {marker}: deliverable, evidence, risk, self-review, next action.',
    },
    {
        'action': 'submit',
        'matrix_user_id': matrix_user_id,
        'match_id': 'daily-dungeon-001',
        'body': 'copy copy copy',
    },
]:
    code, url, body = post_action(**fields)
    assert code == 200, (fields['action'], code, url)
    assert 'Trillionnium League' in body, fields['action']
    actions.append({'action': fields['action'], 'status': code, 'url': url})

status, html_after = get('/league')
assert status == 200, status
for needle in ['Oracle Scout', 'Forge Builder', 'Mirror Auditor', 'Top loot', 'settled', 'held_review', 'rubric_hidden', 'Progression Systems', '成功任务']:
    assert needle in html_after, needle
world_status, world_html_after = get('/world')
assert world_status == 200, world_status
assert world_marker in world_html_after, world_marker
health_status, health_body = get('/health')
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
assert health.get('league_repository_runtime', {}).get('effective_repository') == 'normalized_sql_dual_write', health.get('league_repository_runtime')
assert health.get('league_repository_runtime', {}).get('repository_cutover_status') == 'normalized_sql_dual_write_read_switch_active', health.get('league_repository_runtime')

summary = {
    'ok': True,
    'checked_at_epoch': int(time.time()),
    'base_url': base,
    'actions': actions,
    'has_title': 'Trillionnium League' in html_after,
    'has_web_console': 'Web Battle Console' in html_after,
    'has_web_session': bool(session_cookie and csrf),
    'has_client_app_shell': 'Trillionnium Client App' in app_html,
    'has_client_app_mobile_shell': 'app-global-search' in app_html and 'app-bottom-tabs' in app_html and 'app-tab-map' in app_html,
    'has_client_app_mobile_tabs': all(label in app_html for label in ['消息', '世界', '动态', '我']),
    'has_client_app_first_playable_onboarding': 'app-first-playable-onboarding' in app_html and 'First playable onboarding' in app_html and 'first_playable_loop_100' in app_html and 'trillionnium_first_playable_onboarding_v1' in app_html and 'data-onboarding-step="commerce_delivery"' in app_html and 'route_task_graph_next_action_visible' in app_html,
    'has_client_app_world_map': 'World Map' in app_html,
    'has_client_app_feed_api': '/v1/client/feed/@alice:local.dev' in app_html and 'Feed API' in app_html,
    'has_client_app_feed_surface': 'app-feed-api-status' in app_html and 'app-feed-summary' in app_html and 'app-feed-items-live' in app_html and 'Unified Feed Timeline' in app_html,
    'has_client_app_feed_filters': 'app-feed-filter-actions' in app_html and 'trillionnium-app-feed-filter' in app_html and '推荐' in app_html and '委托' in app_html and '成交' in app_html,
    'has_client_app_feed_api_hydration': 'loadFeedSurface' in app_html and 'Feed API synced' in app_html and 'trillionnium-app-feed-action' in app_html,
    'has_client_app_real_world_map_engine': 'real-world-map' in app_html and 'leaflet_openstreetmap_v1' in app_html and 'OpenStreetMap' in app_html,
    'has_client_app_map_renderer_adapter': 'createRealWorldMapAdapter' in app_html and 'leaflet_renderer_adapter_v1' in app_html and 'maplibre_gl_v1' in app_html and 'const mapRuntime' in app_html and 'supports_future_engine_swap' in app_html and 'gating_contract' in app_html and 'leafletMap' not in app_html and 'renderRouteLine' in app_html and 'renderTileFrame' in app_html and 'renderEventPulse' in app_html and 'onViewportChange' in app_html and 'getCenter' in app_html and 'getZoom' in app_html,
    'has_client_app_live_viewport_hydration': '/world/web/map-viewport' in app_html and 'app-map-camera-summary' in app_html,
    'has_client_app_stream_hud': 'app-map-stream-hud' in app_html and 'app-map-overlay-legend' in app_html,
    'has_client_app_overlay_controls': 'app-map-overlay-controls' in app_html and 'trillionnium-overlay-toggle' in app_html and 'trillionnium-map-camera-action' in app_html,
    'has_client_app_focus_action_rail': 'app-map-focus-summary' in app_html and 'app-map-action-rail' in app_html and 'Map focus action rail' in app_html,
    'has_client_app_world_handoff_bridge': 'trillionnium-world-handoff' in app_html and 'World handoff ready' in app_html,
    'has_client_app_route_cockpit': 'app-map-route-status' in app_html and 'Recommended world handoff:' in app_html and 'Linked task route:' in app_html and 'Draft task follow-up' in app_html,
    'has_client_app_route_filter_controls': 'app-map-route-filter-actions' in app_html and 'Filter route by focus' in app_html and 'Show full route' in app_html,
    'has_client_app_event_route_brief': 'app-map-route-event-brief-status' in app_html and 'Focused event brief:' in app_html,
    'has_client_app_linked_event_handoff': 'web_event_id' in app_html and 'Open linked event' in app_html,
    'has_client_app_route_preview': 'app-route-preview-live' in app_html and 'World Route Preview' in app_html,
    'has_client_app_route_task_graph': 'app-route-task-graph-live' in app_html and 'Task-linked Route Graph' in app_html and ('Route linked contract' in app_html or 'Draft task follow-up' in app_html),
    'has_client_app_route_opportunity_lane': 'app-route-task-graph-live' in app_html and 'Opportunity lane' in app_html and 'Route next opportunity' in app_html,
    'has_client_app_route_node_handoff': 'resolveRouteTargetNodeId' in app_html and 'data-target-node-id=' in app_html and 'next_opportunity_node_id' in app_html,
    'has_client_app_tile_shards': 'app-tile-shards-live' in app_html and 'osm-z15' in app_html,
    'has_client_app_prefetch_queue': 'app-prefetch-queue-live' in app_html and 'Prefetch Queue' in app_html,
    'has_client_app_live_event_stream': 'app-live-events-live' in app_html and 'Live Event Stream' in app_html,
    'has_client_app_event_route_focus': 'findLiveEventByFocus' in app_html and 'data-focus-kind="event"' in app_html and 'data-task-id=' in app_html,
    'has_client_app_event_overlay_focus': 'buildEventFocus' in app_html,
    'has_client_app_stream_lens': 'filterLiveEventStream' in app_html and 'stream lens' in app_html,
    'has_client_app_map_marker_actions': 'primary_actions' in app_html and 'trillionnium-map-action' in app_html,
    'has_client_app_map_focus_controls': 'trillionnium-map-focus' in app_html and 'Focus POI' in app_html,
    'has_client_app_global_map_mirror': 'global_real_world_tiles' in app_html and 'gather_hero_tale_lod' in app_html,
    'has_client_app_face_duel': 'Face Duel' in app_html,
    'has_client_app_social': 'Social' in app_html,
    'has_client_app_wallet': 'Wallet' in app_html,
    'has_client_app_progression': 'Progression' in app_html and '/progression' in app_html,
    'has_world_shell': 'World Action Console' in world_html_after,
    'has_world_action': world_marker in world_html_after,
    'has_world_real_world_map_engine': 'world-real-map' in world_html_after and 'global_real_world_tiles' in world_html_after,
    'has_world_map_renderer_adapter': 'createRealWorldMapAdapter' in world_html_after and 'leaflet_renderer_adapter_v1' in world_html_after and 'maplibre_gl_v1' in world_html_after and 'const mapRuntime' in world_html_after and 'supports_future_engine_swap' in world_html_after and 'gating_contract' in world_html_after and 'leafletMap' not in world_html_after and 'renderRouteLine' in world_html_after and 'renderTileFrame' in world_html_after and 'renderEventPulse' in world_html_after and 'onViewportChange' in world_html_after and 'getCenter' in world_html_after and 'getZoom' in world_html_after,
    'has_world_live_viewport_hydration': '/world/web/map-viewport' in world_html_after and 'world-map-camera-summary' in world_html_after,
    'has_world_stream_hud': 'world-map-stream-hud' in world_html_after and 'world-map-overlay-legend' in world_html_after,
    'has_world_overlay_controls': 'world-map-overlay-controls' in world_html_after and 'trillionnium-overlay-toggle' in world_html_after and 'trillionnium-map-camera-action' in world_html_after,
    'has_world_focus_action_rail': 'world-map-focus-summary' in world_html_after and 'world-map-action-rail' in world_html_after and 'Map focus action rail' in world_html_after,
    'has_world_handoff_bootstrap': 'trillionnium-world-handoff' in world_html_after and 'Prepared from /app:' in world_html_after,
    'has_world_route_filter_controls': 'world-map-route-filter-status' in world_html_after and 'trillionnium-route-filter-action' in world_html_after and 'Focused route filter:' in world_html_after,
    'has_world_event_route_brief': 'world-map-route-event-brief-status' in world_html_after and 'Focused event brief:' in world_html_after,
    'has_world_timeline_event_focus': 'focusRouteEvent' in world_html_after and 'world-event-timeline-item-' in world_html_after and 'Open linked event' in world_html_after,
    'has_world_route_flow_routing': 'world-map-route-flow-status' in world_html_after and 'world-map-route-flow-actions' in world_html_after and 'world-work-deliver-id' in world_html_after and 'world-contract-completion-id' in world_html_after,
    'has_world_route_action_console': 'world-action-console-status' in world_html_after and 'Draft world action' in world_html_after and 'Focused world action:' in world_html_after,
    'has_world_route_next_step': 'world-map-route-next-step-status' in world_html_after and 'Recommended next step:' in world_html_after and ('Route acceptance' in world_html_after or 'Route reopen' in world_html_after or 'Route redelivery' in world_html_after or 'Route delivery' in world_html_after or 'Draft follow-up action' in world_html_after or 'Route contract' in world_html_after or 'Route listing' in world_html_after),
    'has_world_route_task_linkage': 'world-map-route-link-status' in world_html_after and 'Linked task route:' in world_html_after and ('Draft task follow-up' in world_html_after or 'Route linked contract' in world_html_after or 'Open linked event' in world_html_after),
    'has_world_route_task_graph': 'world-route-task-graph-live' in world_html_after and 'Task-linked Route Graph' in world_html_after and ('Draft task follow-up' in world_html_after or 'Route linked contract' in world_html_after),
    'has_world_route_opportunity_lane': 'world-route-task-graph-live' in world_html_after and 'Opportunity lane' in world_html_after and 'Route next opportunity' in world_html_after,
    'has_world_route_node_focus': 'focusRouteTarget' in world_html_after and 'data-target-node-id=' in world_html_after and 'next_opportunity_node_id' in world_html_after,
    'has_world_tile_shards': 'world-tile-shards-live' in world_html_after and 'osm-z15' in world_html_after,
    'has_world_prefetch_queue': 'world-prefetch-queue-live' in world_html_after,
    'has_world_live_event_stream': 'world-live-events-live' in world_html_after,
    'has_world_event_route_focus': 'findLiveEventByFocus' in world_html_after and 'data-focus-kind="event"' in world_html_after and 'data-task-id=' in world_html_after,
    'has_world_event_overlay_focus': 'buildEventFocus' in world_html_after,
    'has_world_stream_lens': 'filterLiveEventStream' in world_html_after and 'stream lens' in world_html_after,
    'has_world_map_marker_actions': 'primary_actions' in world_html_after and 'trillionnium-map-action' in world_html_after and 'world-map-move-target' in world_html_after and 'world-action-body' in world_html_after,
    'has_world_map_focus_controls': 'trillionnium-map-focus' in world_html_after and 'Focus region' in world_html_after,
    'has_world_map_viewport_contract': '/v1/world/map/@alice:local.dev/viewport' in world_html_after and 'street_level_world_nodes' in world_html_after,
    'has_world_map_panel': 'Detailed World Map' in world_html_after,
    'has_world_map_move_form': 'Move on Map' in world_html_after,
    'has_world_contracts_panel': 'World Contracts' in world_html_after,
    'has_world_contract_completion_form': 'Complete Contract' in world_html_after,
    'has_world_asset_upgrade_form': 'Upgrade Asset' in world_html_after,
    'has_world_company_form': 'Launch Company' in world_html_after,
    'has_world_listing_form': 'Publish Listing' in world_html_after,
    'has_world_buy_form': 'Buy / Hire Listing' in world_html_after,
    'has_world_work_delivery_form': 'Deliver Work Order' in world_html_after,
    'has_world_work_acceptance_form': 'Accept Work Order' in world_html_after,
    'has_world_work_rejection_form': 'Reject / Refund Work Order' in world_html_after,
    'has_world_work_reopen_form': 'Reopen / Reserve Again' in world_html_after,
    'has_world_work_cancel_form': 'Cancel / Refund Work Order' in world_html_after,
    'has_world_factions_panel': 'Faction Reputation Map' in world_html_after,
    'has_world_work_orders': 'Work Orders' in world_html_after,
    'has_timeline': 'Battle Timeline' in html_after,
    'has_progression_systems': 'Progression Systems' in html_after and '/skills' in html_after and '/tools' in html_after and '/skins' in html_after,
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
    'marker': marker,
}
path = summary_dir / f'web-e2e-summary-{int(time.time())}.json'
path.write_text(json.dumps(summary, ensure_ascii=False, indent=2))
print(json.dumps({'ok': True, 'summary': str(path), **summary}, ensure_ascii=False, indent=2))
PY
