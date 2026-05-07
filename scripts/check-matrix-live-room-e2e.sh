#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
TOKENS_FILE="${MATRIX_LIVE_TOKENS_FILE:-$ROOT_DIR/run/matrix-live/tokens.json}"
SUMMARY_DIR="$ROOT_DIR/run/matrix-live"
mkdir -p "$SUMMARY_DIR"

if [[ ! -f "$TOKENS_FILE" ]]; then
  echo "missing Matrix live tokens file: $TOKENS_FILE" >&2
  echo "run ./scripts/start-matrix-live-stack.sh first" >&2
  exit 2
fi

python3 - <<'PY'
import json, pathlib, time, urllib.error, urllib.parse, urllib.request, uuid, sys
root = pathlib.Path.cwd()
tokens_path = pathlib.Path('run/matrix-live/tokens.json')
j = json.load(open(tokens_path))
base = j['homeserver']
room = j['room_id']
alice = j['alice']['access_token']
bot_user = j['cex_bot']['user_id']
room_enc = urllib.parse.quote(room, safe='')

def req(method, path, token, body=None):
    data = None if body is None else json.dumps(body).encode()
    request = urllib.request.Request(
        base + path,
        data=data,
        method=method,
        headers={'Authorization': 'Bearer ' + token, 'content-type': 'application/json'},
    )
    for attempt in range(10):
        try:
            with urllib.request.urlopen(request, timeout=20) as resp:
                return json.load(resp)
        except urllib.error.HTTPError as err:
            if err.code != 429 or attempt == 9:
                raise
            retry_ms = 1500
            try:
                payload = json.loads(err.read().decode() or '{}')
                retry_ms = int(payload.get('retry_after_ms') or retry_ms)
            except Exception:
                pass
            time.sleep((retry_ms + 500) / 1000.0)
    raise RuntimeError('matrix request retry exhausted')

def send_and_wait(text, card_type, match=None):
    print(f'==> {text} [{card_type}]', file=sys.stderr, flush=True)
    sent = req('PUT', f'/_matrix/client/v3/rooms/{room_enc}/send/m.room.message/{uuid.uuid4()}', alice, {
        'msgtype': 'm.text',
        'body': text,
    })
    request_ts = None
    found = None
    for _ in range(90):
        time.sleep(1)
        msgs = req('GET', f'/_matrix/client/v3/rooms/{room_enc}/messages?dir=b&limit=100', alice)
        for ev in msgs.get('chunk', []):
            if ev.get('event_id') == sent['event_id']:
                request_ts = ev.get('origin_server_ts')
                break
        if request_ts is None:
            continue
        candidates = []
        for ev in msgs.get('chunk', []):
            if ev.get('type') != 'm.room.message' or ev.get('sender') != bot_user:
                continue
            if (ev.get('origin_server_ts') or 0) <= request_ts:
                continue
            content = ev.get('content') or {}
            card = content.get('cex_card') or {}
            if card.get('type') != card_type:
                continue
            if match and not match(content, card):
                continue
            candidates.append(ev)
        if candidates:
            ev = sorted(candidates, key=lambda item: item.get('origin_server_ts') or 0)[-1]
            content = ev.get('content') or {}
            found = {
                'event_id': ev.get('event_id'),
                'sender': ev.get('sender'),
                'body': content.get('body'),
                'formatted_body': content.get('formatted_body'),
                'card': content.get('cex_card'),
            }
            break
    if not found:
        raise RuntimeError(f'no bot reply for {text!r} with card_type={card_type}')
    print(f'    ok {text} -> {found["event_id"]}', file=sys.stderr, flush=True)
    time.sleep(2.2)
    return {'sent': sent, 'reply': found}

def route_story_ok(card, command_prefix=None, require_live_task=False):
    task_id = str(card.get('route_next_task_id') or '').strip()
    if require_live_task:
        if task_id in ('', 'none'):
            return False
    elif not task_id:
        return False
    required = [
        'route_next_action_label',
        'route_next_panel_id',
        'route_next_command_hint',
        'route_next_stage_summary',
        'route_next_outcome_summary',
        'route_next_feedback_focus',
        'route_next_node_id',
        'route_next_opportunity_kind',
        'route_next_opportunity_hint',
        'route_next_opportunity_playbook',
        'route_next_opportunity_command',
        'route_next_opportunity_node_id',
    ]
    if not all(bool(card.get(key)) for key in required):
        return False
    if command_prefix and not str(card.get('route_next_opportunity_command') or '').startswith(command_prefix):
        return False
    return True


def route_opportunity_target_ok(card, expected_panel='world-action-console', expected_textarea='world-action-body', expected_action_label=None, expected_input_id=None, expected_input_value=None):
    return (
        bool(card.get('route_next_opportunity_action_label'))
        and (expected_action_label is None or card.get('route_next_opportunity_action_label') == expected_action_label)
        and card.get('route_next_opportunity_panel_id') == expected_panel
        and (expected_input_id is None or card.get('route_next_opportunity_input_id') == expected_input_id)
        and (expected_input_value is None or card.get('route_next_opportunity_input_value') == expected_input_value)
        and card.get('route_next_opportunity_textarea_id') == expected_textarea
        and bool(card.get('route_next_opportunity_body'))
        and bool(card.get('route_next_opportunity_target_node_id'))
    )

def map_renderer_adapter_ok(card):
    return (
        card.get('map_renderer_adapter_id') == 'leaflet_renderer_adapter_v1'
        and int(card.get('map_renderer_adapter_version') or 0) >= 1
        and card.get('map_runtime_handle_name') == 'mapRuntime'
        and card.get('map_renderer_future_engine_candidate') == 'maplibre_gl_v1'
        and card.get('map_renderer_supports_future_engine_swap') is True
        and card.get('map_planned_upgrade_engine_id') == 'maplibre_gl_v1'
        and bool(card.get('map_planned_upgrade_gating_contract'))
    )

def route_focus_panel_default_node_id(panel_id):
    return {
        'world-assets-panel': 'asset-yard',
        'world-companies-panel': 'starter-studio',
        'world-listings-panel': 'client-board',
        'world-commerce-panel': 'delivery-dock',
        'world-contracts-panel': 'ledger-office',
    }.get(panel_id, '')

def route_opportunity_target_from_command(command, opportunity_node_id=''):
    trimmed = (command or '').strip()
    target = {
        'action_label': 'Open world action lane',
        'panel_id': 'world-action-console',
        'input_id': '',
        'input_value': '',
        'textarea_id': 'world-action-body',
        'body': '',
        'node_id': (opportunity_node_id or '').strip(),
    }

    def strip_body(prefix):
        if trimmed.startswith(prefix):
            return trimmed[len(prefix):].strip()
        return None

    if (body := strip_body('/upgrade latest')) is not None:
        target.update(action_label='Open asset upgrade lane', panel_id='world-assets-panel', input_id='world-asset-id', input_value='latest', textarea_id='world-asset-body', body=body)
    elif (body := strip_body('/company latest')) is not None:
        target.update(action_label='Open company lane', panel_id='world-companies-panel', input_id='world-company-asset-id', input_value='latest', textarea_id='world-company-body', body=body)
    elif (body := strip_body('/sell latest')) is not None:
        target.update(action_label='Open listing lane', panel_id='world-listings-panel', input_id='world-listing-company-id', input_value='latest', textarea_id='world-listing-body', body=body)
    elif (body := strip_body('/buy latest')) is not None:
        target.update(action_label='Open purchase lane', panel_id='world-commerce-panel', input_id='world-buy-listing-id', input_value='latest', textarea_id='world-buy-body', body=body)
    elif (body := strip_body('/work deliver latest')) is not None:
        target.update(action_label='Open delivery lane', panel_id='world-commerce-panel', input_id='world-work-deliver-id', input_value='latest', textarea_id='world-work-deliver-body', body=body)
    elif (body := strip_body('/work accept latest')) is not None:
        target.update(action_label='Open acceptance lane', panel_id='world-commerce-panel', input_id='world-work-accept-id', input_value='latest', textarea_id='world-work-accept-body', body=body)
    elif (body := strip_body('/work reject latest')) is not None:
        target.update(action_label='Open rejection lane', panel_id='world-commerce-panel', input_id='world-work-reject-id', input_value='latest', textarea_id='world-work-reject-body', body=body)
    elif (body := strip_body('/work reopen latest')) is not None:
        target.update(action_label='Open reopen lane', panel_id='world-commerce-panel', input_id='world-work-reopen-id', input_value='latest', textarea_id='world-work-reopen-body', body=body)
    elif (body := strip_body('/work cancel latest')) is not None:
        target.update(action_label='Open cancellation lane', panel_id='world-commerce-panel', input_id='world-work-cancel-id', input_value='latest', textarea_id='world-work-cancel-body', body=body)
    elif (body := strip_body('/world action')) is not None:
        target.update(action_label='Open world action lane', body=body)
    elif (body := strip_body('/contract')) is not None:
        target.update(action_label='Open contract capture lane', body=body)
    elif trimmed.startswith('/complete '):
        rest = trimmed[len('/complete '):].strip()
        if rest:
            parts = rest.split(' ', 1)
            contract_id = parts[0].strip()
            body = parts[1].strip() if len(parts) > 1 else ''
        else:
            contract_id = ''
            body = ''
        target.update(action_label='Open contract completion lane', panel_id='world-contracts-panel', input_id='world-contract-completion-id', input_value=contract_id, textarea_id='world-contract-completion-body', body=body)

    if not target['body'].strip():
        target['body'] = trimmed
    if not target['node_id']:
        target['node_id'] = route_focus_panel_default_node_id(target['panel_id'])
    return target

def route_opportunity_target_matches_command(card):
    command = card.get('route_next_opportunity_command') or ''
    expected = route_opportunity_target_from_command(command, card.get('route_next_opportunity_node_id') or '')
    return (
        bool(command)
        and card.get('route_next_opportunity_action_label') == expected['action_label']
        and card.get('route_next_opportunity_panel_id') == expected['panel_id']
        and card.get('route_next_opportunity_input_id') == expected['input_id']
        and card.get('route_next_opportunity_input_value') == expected['input_value']
        and card.get('route_next_opportunity_textarea_id') == expected['textarea_id']
        and card.get('route_next_opportunity_body') == expected['body']
        and card.get('route_next_opportunity_target_node_id') == expected['node_id']
    )


def route_story_kind_ok(card, expected_kind, command_prefix=None, require_live_task=False, copy_any_tokens=()):
    if not route_story_ok(card, command_prefix=command_prefix, require_live_task=require_live_task):
        return False
    if str(card.get('route_next_opportunity_kind') or '').strip() != expected_kind:
        return False
    if copy_any_tokens:
        haystack = ' '.join(
            str(card.get(key) or '')
            for key in (
                'route_next_outcome_summary',
                'route_next_feedback_focus',
                'route_next_opportunity_hint',
                'route_next_opportunity_playbook',
                'route_next_opportunity_command',
            )
        ).lower()
        if not any(str(token).lower() in haystack for token in copy_any_tokens):
            return False
    return True


def route_runner_handoff_ok(card):
    handoff = card.get('route_runner_handoff') or {}
    runner_count = int(handoff.get('runner_count') or 0)
    reward_claim_count = int(handoff.get('reward_claim_action_count') or 0)
    next_route_count = int(handoff.get('next_route_action_count') or 0)
    return (
        handoff.get('contract_version') == 'trillionnium_route_runner_handoff_v1'
        and handoff.get('supports_route_runner_reward_claim_actions') is True
        and handoff.get('supports_route_runner_next_route_actions') is True
        and handoff.get('supports_checkpoint_reward_history') is True
        and runner_count >= 1
        and reward_claim_count >= 1
        and next_route_count >= 1
        and int(card.get('avatar_route_runner_count') or 0) == runner_count
        and int(card.get('route_runner_reward_claim_action_count') or 0) == reward_claim_count
        and int(card.get('route_runner_next_route_action_count') or 0) == next_route_count
        and bool(handoff.get('first_task_id'))
        and bool(handoff.get('first_progress_label'))
        and bool(handoff.get('first_reward_claim_status'))
        and bool(handoff.get('first_next_route_status'))
        and bool(handoff.get('first_next_route_action_body'))
        and bool(handoff.get('first_next_route_sequence_summary'))
        and bool(handoff.get('handoff_prompt'))
        and card.get('route_runner_first_task_id') == handoff.get('first_task_id')
        and card.get('route_runner_next_route_status') == handoff.get('first_next_route_status')
        and card.get('route_runner_next_route_action_body') == handoff.get('first_next_route_action_body')
        and card.get('route_runner_next_route_sequence_summary') == handoff.get('first_next_route_sequence_summary')
        and card.get('route_runner_reward_claim_status') == handoff.get('first_reward_claim_status')
    )


def route_runner_handoff_summary(prefix, card):
    handoff = card.get('route_runner_handoff') or {}
    return {
        f'{prefix}_route_runner_handoff_contract_version': handoff.get('contract_version'),
        f'{prefix}_avatar_route_runner_count': card.get('avatar_route_runner_count'),
        f'{prefix}_route_runner_handoff_runner_count': handoff.get('runner_count'),
        f'{prefix}_route_runner_reward_claim_action_count': card.get('route_runner_reward_claim_action_count'),
        f'{prefix}_route_runner_next_route_action_count': card.get('route_runner_next_route_action_count'),
        f'{prefix}_route_runner_next_route_ready_count': card.get('route_runner_next_route_ready_count'),
        f'{prefix}_route_runner_first_task_id': card.get('route_runner_first_task_id'),
        f'{prefix}_route_runner_first_progress_label': card.get('route_runner_first_progress_label'),
        f'{prefix}_route_runner_reward_claim_status': card.get('route_runner_reward_claim_status'),
        f'{prefix}_route_runner_next_route_status': card.get('route_runner_next_route_status'),
        f'{prefix}_route_runner_next_route_action_body': card.get('route_runner_next_route_action_body'),
        f'{prefix}_route_runner_next_route_sequence_summary': card.get('route_runner_next_route_sequence_summary'),
        f'{prefix}_route_runner_handoff_summary': handoff.get('summary'),
        f'{prefix}_route_runner_handoff_prompt': handoff.get('handoff_prompt'),
    }

marker = f'真房间闭环 E2E {int(time.time())}'
task_result = send_and_wait('/task ' + marker, 'task_status', lambda content, card: marker in (content.get('formatted_body') or ''))
task_id = task_result['reply']['card']['task_id']
status_result = send_and_wait('/status ' + task_id, 'task_status', lambda content, card: card.get('task_id') == task_id)
balance_result = send_and_wait('/balance', 'wallet_summary')
plans_result = send_and_wait('/plans', 'package_summary')
client_app_result = send_and_wait('/app', 'trillionnium_client_app', lambda content, card: int(card.get('module_count') or 0) >= 5 and card.get('has_world_map') is True and card.get('has_real_world_map_engine') is True and card.get('map_engine_id') == 'leaflet_openstreetmap_v1' and card.get('tile_provider') == 'OpenStreetMap' and card.get('primary_entry_module_id') == 'world_map' and map_renderer_adapter_ok(card) and bool(card.get('active_region_id')) and int(card.get('tile_shard_count') or 0) >= 1 and int(card.get('nearby_poi_count') or 0) >= 1 and int(card.get('prefetch_count') or 0) >= 1 and int(card.get('live_event_count') or 0) >= 1 and bool(card.get('player_density_mode')) and card.get('has_first_playable_onboarding') is True and card.get('onboarding_contract_version') == 'trillionnium_first_playable_onboarding_v1' and card.get('onboarding_completion_target') == 'first_playable_loop_100' and int(card.get('onboarding_step_count') or 0) >= 5 and ('route_preview_item_count' in card) and ('route_task_graph_count' in card) and route_story_ok(card, command_prefix='/world action') and route_opportunity_target_ok(card, expected_action_label='Open world action lane') and route_runner_handoff_ok(card) and card.get('has_face_duel') is True and card.get('has_social') is True and card.get('has_wallet') is True and card.get('has_progression') is True and int(card.get('progression_level') or 0) >= 1)
client_feed_result = send_and_wait('/feed', 'trillionnium_client_feed', lambda content, card: int(card.get('item_count') or 0) >= 1 and int(card.get('source_count') or 0) >= 7 and bool(card.get('active_region_id')) and bool(card.get('top_title')) and bool(card.get('top_action_label')) and bool(card.get('top_action_panel_id')) and route_story_ok(card, command_prefix='/world action') and route_opportunity_target_ok(card, expected_action_label='Open world action lane') and route_runner_handoff_ok(card))
client_feed_commerce_result = send_and_wait('/feed commerce', 'trillionnium_client_feed', lambda content, card: card.get('feed_filter') == 'commerce' and card.get('feed_filter_label') == '成交' and int(card.get('visible_item_count') or 0) >= 1 and card.get('top_feed_group') == 'commerce' and bool(card.get('top_action_label')) and bool(card.get('top_action_panel_id')))
client_social_result = send_and_wait('/social', 'trillionnium_client_social', lambda content, card: int(card.get('contact_count') or 0) >= 1)
client_duel_result = send_and_wait('/duel nearby Face duel opening move: choose Oracle Scout, scout opponent intent, use Forge Builder follow-up, and record fair-play evidence.', 'trillionnium_client_duel', lambda content, card: card.get('match_id') == 'face-duel-001' and bool(card.get('task_id')))
league_result = send_and_wait('/league', 'league_home')
world_result = send_and_wait('/world', 'trillionnium_world', lambda content, card: int(card.get('zone_count') or 0) >= 4 and map_renderer_adapter_ok(card) and ('route_preview_item_count' in card) and ('route_task_graph_count' in card) and route_story_ok(card, command_prefix='/world action') and route_opportunity_target_ok(card, expected_action_label='Open world action lane') and route_runner_handoff_ok(card))
world_map_result = send_and_wait('/map', 'trillionnium_world_map', lambda content, card: int(card.get('node_count') or 0) >= 8 and bool(card.get('current_node_id')) and card.get('has_real_world_map_engine') is True and card.get('map_engine_id') == 'leaflet_openstreetmap_v1' and card.get('tile_provider') == 'OpenStreetMap' and card.get('mirror_scope') == 'global_real_world_tiles' and map_renderer_adapter_ok(card) and bool(card.get('active_region_id')) and ('route_preview_item_count' in card) and ('route_task_graph_count' in card) and route_story_ok(card, command_prefix='/world action') and route_opportunity_target_ok(card, expected_action_label='Open world action lane') and route_runner_handoff_ok(card))
world_map_exit = str(world_map_result['reply']['card'].get('exits') or 'east').split('→', 1)[0].strip().split(' / ', 1)[0] or 'east'
world_map_move_result = send_and_wait('/go ' + world_map_exit, 'trillionnium_world_map_move', lambda content, card: bool(card.get('to_node_id')) and bool(card.get('location_id')))
world_action_result = send_and_wait('/world action 我要在镜像城市开一家 AI 设计公司，招募 Agent，服务真实客户。', 'trillionnium_world_action', lambda content, card: card.get('event_kind') in ('venture', 'craft', 'market', 'explore', 'recruit'))
craft_action_result = send_and_wait('/craft 建一个自动交付工坊，把客户需求转成可复用资产。', 'trillionnium_craft_action', lambda content, card: card.get('event_kind') == 'craft')
world_contract_result = send_and_wait('/contract 帮客户整理一个 AI 店铺启动方案，包含目标、证据、风险和验收标准。', 'trillionnium_world_contract', lambda content, card: card.get('event_kind') == 'contract' and bool(card.get('task_id')) and bool(card.get('contract_id')))
world_contract_id = world_contract_result['reply']['card'].get('contract_id')
world_contract_completion_result = send_and_wait('/complete ' + world_contract_id + ' 交付方案：包含 deliverable、evidence、risk review、acceptance standard、next step 和自检记录。', 'trillionnium_world_contract_completion', lambda content, card: card.get('contract_id') == world_contract_id and card.get('ledger_status') == 'settled' and 'hidden' in str(card.get('judge_status')))
world_route_result = send_and_wait('/world', 'trillionnium_world', lambda content, card: int(card.get('route_task_graph_count') or 0) >= 1 and route_story_ok(card, command_prefix='/world action', require_live_task=True) and route_opportunity_target_ok(card, expected_action_label='Open world action lane'))
client_app_route_result = send_and_wait('/app', 'trillionnium_client_app', lambda content, card: int(card.get('route_preview_item_count') or 0) >= 1 and int(card.get('route_task_graph_count') or 0) >= 1 and route_story_ok(card, command_prefix='/world action', require_live_task=True) and route_opportunity_target_ok(card, expected_action_label='Open world action lane'))
world_map_route_result = send_and_wait('/map', 'trillionnium_world_map', lambda content, card: int(card.get('route_preview_item_count') or 0) >= 1 and int(card.get('route_task_graph_count') or 0) >= 1 and route_story_ok(card, command_prefix='/world action', require_live_task=True) and route_opportunity_target_ok(card, expected_action_label='Open world action lane'))
world_assets_result = send_and_wait('/assets', 'trillionnium_world_assets', lambda content, card: int(card.get('asset_count') or 0) >= 1 and int(card.get('route_task_graph_count') or 0) >= 1 and route_story_ok(card, command_prefix='/upgrade latest') and route_opportunity_target_ok(card, expected_panel='world-assets-panel', expected_textarea='world-asset-body', expected_action_label='Open asset upgrade lane', expected_input_id='world-asset-id', expected_input_value='latest'))
world_asset_upgrade_result = send_and_wait('/upgrade latest Asset upgrade deliverable: service package, evidence template, risk control checklist, operating cadence, acceptance standard, next customer path, and self-review notes.', 'trillionnium_world_asset_upgrade', lambda content, card: int(card.get('asset_level') or 0) >= 1 and int(card.get('value_delta') or 0) >= 1 and int(card.get('route_task_graph_count') or 0) >= 1 and route_story_ok(card, command_prefix='/') and route_opportunity_target_matches_command(card))
world_companies_result = send_and_wait('/companies', 'trillionnium_world_companies', lambda content, card: int(card.get('company_count') or 0) >= 1 and int(card.get('route_task_graph_count') or 0) >= 1 and route_story_ok(card, command_prefix='/company latest') and route_opportunity_target_ok(card, expected_panel='world-companies-panel', expected_textarea='world-company-body', expected_action_label='Open company lane', expected_input_id='world-company-asset-id', expected_input_value='latest'))
world_company_result = send_and_wait('/company latest Company launch plan: offer, target customer, revenue path, operating loop, evidence, risk controls, acceptance standard, and next sale.', 'trillionnium_world_company_created', lambda content, card: int(card.get('company_level') or 0) >= 1 and int(card.get('revenue_score') or 0) >= 1 and int(card.get('route_task_graph_count') or 0) >= 1 and route_story_ok(card, command_prefix='/') and route_opportunity_target_matches_command(card))
world_shops_result = send_and_wait('/shops', 'trillionnium_world_shops', lambda content, card: int(card.get('shop_count') or 0) >= 1 and int(card.get('listing_count') or 0) >= 1 and int(card.get('route_task_graph_count') or 0) >= 1 and route_story_ok(card, command_prefix='/sell latest') and route_opportunity_target_ok(card, expected_panel='world-listings-panel', expected_textarea='world-listing-body', expected_action_label='Open listing lane', expected_input_id='world-listing-company-id', expected_input_value='latest'))
world_listing_result = send_and_wait('/sell latest Service listing: AI design delivery package with scope, price logic, evidence package, customer deliverable, risk controls, self-review, acceptance standard, revision policy, and next action.', 'trillionnium_world_listing_created', lambda content, card: int(card.get('price_credits') or 0) >= 1 and int(card.get('quality_score') or 0) >= 1 and card.get('status') == 'listed' and int(card.get('route_task_graph_count') or 0) >= 1 and route_story_ok(card, command_prefix='/sell latest') and route_opportunity_target_ok(card, expected_panel='world-listings-panel', expected_textarea='world-listing-body', expected_action_label='Open listing lane', expected_input_id='world-listing-company-id', expected_input_value='latest'))
world_purchase_result = send_and_wait('/buy latest Purchase brief: hire this AI design package, define deliverable, evidence package, acceptance standard, risk controls, and next action.', 'trillionnium_world_listing_purchase', lambda content, card: int(card.get('price_credits') or 0) >= 1 and bool(card.get('work_order_id')) and card.get('ledger_status') == 'settled' and card.get('buyer_ledger_status') == 'reserved' and int(card.get('route_task_graph_count') or 0) >= 1 and route_story_ok(card, command_prefix='/world action') and route_opportunity_target_ok(card, expected_action_label='Open world action lane'))
world_work_delivery_result = send_and_wait('/work deliver latest Work delivery package: final deliverable, evidence package, acceptance checklist, risk review, next action, and self-review confirming the buyer brief is complete.', 'trillionnium_world_work_delivery', lambda content, card: card.get('status') == 'delivered' and float(card.get('score') or 0) >= 1 and int(card.get('route_task_graph_count') or 0) >= 1 and route_story_ok(card, command_prefix='/') and route_opportunity_target_matches_command(card))
world_work_acceptance_result = send_and_wait('/work accept latest Buyer acceptance: delivered package reviewed, evidence confirmed, quality accepted, next collaboration opened, and reputation granted.', 'trillionnium_world_work_acceptance', lambda content, card: card.get('status') == 'accepted' and int(card.get('reputation_delta') or 0) >= 1 and card.get('buyer_consume_status') == 'consumed' and int(card.get('route_task_graph_count') or 0) >= 1 and route_story_ok(card, command_prefix='/') and route_opportunity_target_matches_command(card))
world_reject_purchase_result = send_and_wait('/buy latest Reject-flow purchase brief: hire this service again so the buyer can validate refund handling, evidence gaps, revision requirements, and next action.', 'trillionnium_world_listing_purchase', lambda content, card: int(card.get('price_credits') or 0) >= 1 and bool(card.get('work_order_id')) and card.get('buyer_ledger_status') == 'reserved')
world_reject_delivery_result = send_and_wait('/work deliver latest Reject-flow delivery package: intentionally reviewable deliverable with evidence package, quality notes, risk review, and next action.', 'trillionnium_world_work_delivery', lambda content, card: card.get('status') == 'delivered' and float(card.get('score') or 0) >= 1)
world_work_rejection_result = send_and_wait('/work reject latest Buyer rejection: delivery is not accepted, refund reserved buyer funds, record evidence gaps, revision requirements, and next action.', 'trillionnium_world_work_rejection', lambda content, card: card.get('status') == 'rejected_refunded' and card.get('buyer_refund_status') == 'refunded' and route_story_kind_ok(card, 'revision_reopen', command_prefix='/work reopen latest', copy_any_tokens=('reopen', 'revision', '重开', '修订')) and route_opportunity_target_ok(card, expected_panel='world-commerce-panel', expected_textarea='world-work-reopen-body', expected_action_label='Open reopen lane', expected_input_id='world-work-reopen-id', expected_input_value='latest'))
world_work_reopen_result = send_and_wait('/work reopen latest Buyer reopen: reserve funds again, list revision requirements, evidence gaps, acceptance standard, and next redelivery action.', 'trillionnium_world_work_reopen', lambda content, card: card.get('status') == 'reopened' and card.get('buyer_reopen_reserve_status') == 'reserved' and card.get('work_status') == 'open' and route_story_kind_ok(card, 'reopen_recovery', command_prefix='/work deliver latest', copy_any_tokens=('reopen', 'redelivery', 'revision', '重开')) and route_opportunity_target_ok(card, expected_panel='world-commerce-panel', expected_textarea='world-work-deliver-body', expected_action_label='Open delivery lane', expected_input_id='world-work-deliver-id', expected_input_value='latest'))
world_work_redelivery_result = send_and_wait('/work deliver latest Redelivery package after reopen: revised deliverable, added evidence, fixed gaps, acceptance checklist, risk review, and next action.', 'trillionnium_world_work_delivery', lambda content, card: card.get('status') == 'delivered' and float(card.get('score') or 0) >= 1)
world_work_reacceptance_result = send_and_wait('/work accept latest Buyer acceptance after reopen: revised delivery reviewed, evidence gaps closed, quality accepted, and reserve consumed.', 'trillionnium_world_work_acceptance', lambda content, card: card.get('status') == 'accepted' and card.get('buyer_consume_status') == 'consumed')
world_cancel_purchase_result = send_and_wait('/buy latest Cancel-flow purchase brief: open a work order that will be cancelled before delivery to validate refund and closure handling.', 'trillionnium_world_listing_purchase', lambda content, card: int(card.get('price_credits') or 0) >= 1 and bool(card.get('work_order_id')) and card.get('buyer_ledger_status') == 'reserved')
world_work_cancellation_result = send_and_wait('/work cancel latest Buyer cancellation: cancel this open work before delivery, refund reserved funds, record reason, and close the order.', 'trillionnium_world_work_cancellation', lambda content, card: card.get('status') == 'cancelled_refunded' and card.get('buyer_cancel_refund_status') == 'refunded' and route_story_kind_ok(card, 'smaller_scope_requalification', command_prefix='/sell latest', copy_any_tokens=('smaller', 'requal', 'scope', '试单')) and route_opportunity_target_ok(card, expected_panel='world-listings-panel', expected_textarea='world-listing-body', expected_action_label='Open listing lane', expected_input_id='world-listing-company-id', expected_input_value='latest'))
world_work_result = send_and_wait('/work', 'trillionnium_world_commerce', lambda content, card: int(card.get('work_order_count') or 0) >= 1 and int(card.get('delivery_count') or 0) >= 1 and int(card.get('acceptance_count') or 0) >= 1 and int(card.get('rejection_count') or 0) >= 1 and int(card.get('reopen_count') or 0) >= 1 and int(card.get('cancellation_count') or 0) >= 1 and int(card.get('route_task_graph_count') or 0) >= 1 and route_story_ok(card, command_prefix='/world action') and route_opportunity_target_ok(card, expected_action_label='Open world action lane'))
world_factions_result = send_and_wait('/factions', 'trillionnium_world_factions', lambda content, card: int(card.get('faction_count') or 0) >= 4 and int(card.get('standing_count') or 0) >= 1)
season_result = send_and_wait('/season', 'league_season', lambda content, card: bool(card.get('top_guild')))
arena_result = send_and_wait('/arena', 'league_arena')
guilds_result = send_and_wait('/guild', 'league_guilds', lambda content, card: bool(card.get('top_guild')))
guild_join_result = send_and_wait('/guild guild-prompt-forge', 'league_guild_joined', lambda content, card: card.get('guild_id') == 'guild-prompt-forge')
raids_result = send_and_wait('/raid', 'league_raids', lambda content, card: card.get('match_id') == 'guild-raid-001')
team_result = send_and_wait('/team guild-raid-001 scout', 'league_raid_roster', lambda content, card: card.get('match_id') == 'guild-raid-001' and int(card.get('slot_count') or 0) >= 1)
raid_result = send_and_wait('/raid guild-raid-001 团本贡献：scout evidence，assign builder，define risk gate，next step。', 'league_raid_contribution', lambda content, card: card.get('match_id') == 'guild-raid-001')
draft_result = send_and_wait('/draft oracle_scout forge_builder mirror_auditor courier_closer', 'league_draft')
join_result = send_and_wait('/join daily-dungeon-001', 'league_joined', lambda content, card: card.get('match_id') == 'daily-dungeon-001')
league_battle_marker = f'Trillionnium League E2E {int(time.time())}'
league_battle_result = send_and_wait('/battle daily-dungeon-001 ' + league_battle_marker, 'league_battle', lambda content, card: card.get('match_id') == 'daily-dungeon-001')
submit_marker = f'ledger-settle-{int(time.time())}'
submit_result = send_and_wait('/submit daily-dungeon-001 最终方案：客户交付 deliver，包含 evidence 依据、risk 风险、自评、结算标记 ' + submit_marker + ' 和下一步执行计划。', 'league_submission', lambda content, card: card.get('match_id') == 'daily-dungeon-001' and card.get('ledger_status') == 'settled' and 'hidden' in str(card.get('judge_status')) and int(card.get('score_event_count') or 0) >= 7)
after_submit_balance_result = send_and_wait('/balance', 'wallet_summary')
profile_result = send_and_wait('/profile', 'league_profile')
progression_result = send_and_wait('/progression', 'league_progression', lambda content, card: int(card.get('level') or 0) >= 1 and int(card.get('successful_task_count') or 0) >= 1)
skills_result = send_and_wait('/skills', 'league_skills', lambda content, card: int(card.get('skill_count') or 0) >= 3 and int(card.get('unlocked_skill_count') or 0) >= 1)
tools_result = send_and_wait('/tools', 'league_tools', lambda content, card: int(card.get('tool_count') or 0) >= 3 and int(card.get('unlocked_tool_count') or 0) >= 1)
skins_result = send_and_wait('/skins', 'league_skins', lambda content, card: int(card.get('skin_count') or 0) >= 3 and int(card.get('unlocked_skin_count') or 0) >= 1)
rewards_result = send_and_wait('/rewards', 'league_rewards')
inventory_result = send_and_wait('/inventory', 'league_inventory', lambda content, card: int(card.get('item_count') or 0) >= 1)
history_result = send_and_wait('/history', 'league_history')
rank_result = send_and_wait('/rank', 'league_rank')
loadout_result = send_and_wait('/loadout', 'league_loadout')

summary = {
    'ok': True,
    'checked_at_epoch': int(time.time()),
    'homeserver': base,
    'room_id': room,
    'bot_user_id': bot_user,
    'task_id': task_id,
    'task_reply': task_result['reply'],
    'status_reply': status_result['reply'],
    'balance_reply': balance_result['reply'],
    'plans_reply': plans_result['reply'],
    'client_app_reply': client_app_result['reply'],
    'client_feed_reply': client_feed_result['reply'],
    'client_feed_commerce_reply': client_feed_commerce_result['reply'],
    'client_social_reply': client_social_result['reply'],
    'client_duel_reply': client_duel_result['reply'],
    'league_reply': league_result['reply'],
    'world_reply': world_result['reply'],
    'world_route_reply': world_route_result['reply'],
    'world_map_reply': world_map_result['reply'],
    'world_map_move_reply': world_map_move_result['reply'],
    'world_action_reply': world_action_result['reply'],
    'craft_action_reply': craft_action_result['reply'],
    'world_contract_reply': world_contract_result['reply'],
    'world_contract_completion_reply': world_contract_completion_result['reply'],
    'client_app_route_reply': client_app_route_result['reply'],
    'world_map_route_reply': world_map_route_result['reply'],
    'world_assets_reply': world_assets_result['reply'],
    'world_asset_upgrade_reply': world_asset_upgrade_result['reply'],
    'world_companies_reply': world_companies_result['reply'],
    'world_company_reply': world_company_result['reply'],
    'world_shops_reply': world_shops_result['reply'],
    'world_listing_reply': world_listing_result['reply'],
    'world_purchase_reply': world_purchase_result['reply'],
    'world_work_delivery_reply': world_work_delivery_result['reply'],
    'world_work_acceptance_reply': world_work_acceptance_result['reply'],
    'world_reject_purchase_reply': world_reject_purchase_result['reply'],
    'world_reject_delivery_reply': world_reject_delivery_result['reply'],
    'world_work_rejection_reply': world_work_rejection_result['reply'],
    'world_work_reopen_reply': world_work_reopen_result['reply'],
    'world_work_redelivery_reply': world_work_redelivery_result['reply'],
    'world_work_reacceptance_reply': world_work_reacceptance_result['reply'],
    'world_cancel_purchase_reply': world_cancel_purchase_result['reply'],
    'world_work_cancellation_reply': world_work_cancellation_result['reply'],
    'world_work_reply': world_work_result['reply'],
    'world_factions_reply': world_factions_result['reply'],
    'season_reply': season_result['reply'],
    'arena_reply': arena_result['reply'],
    'guilds_reply': guilds_result['reply'],
    'guild_join_reply': guild_join_result['reply'],
    'raids_reply': raids_result['reply'],
    'team_reply': team_result['reply'],
    'raid_reply': raid_result['reply'],
    'draft_reply': draft_result['reply'],
    'join_reply': join_result['reply'],
    'league_battle_reply': league_battle_result['reply'],
    'submit_reply': submit_result['reply'],
    'after_submit_balance_reply': after_submit_balance_result['reply'],
    'profile_reply': profile_result['reply'],
    'progression_reply': progression_result['reply'],
    'skills_reply': skills_result['reply'],
    'tools_reply': tools_result['reply'],
    'skins_reply': skins_result['reply'],
    'rewards_reply': rewards_result['reply'],
    'inventory_reply': inventory_result['reply'],
    'history_reply': history_result['reply'],
    'rank_reply': rank_result['reply'],
    'loadout_reply': loadout_result['reply'],
}
summary.update({
    'client_feed_item_count': client_feed_result['reply']['card'].get('item_count'),
    'client_feed_visible_item_count': client_feed_result['reply']['card'].get('visible_item_count'),
    'client_feed_source_count': client_feed_result['reply']['card'].get('source_count'),
    'client_feed_top_feed_kind': client_feed_result['reply']['card'].get('top_feed_kind'),
    'client_feed_top_feed_group': client_feed_result['reply']['card'].get('top_feed_group'),
    'client_feed_top_title': client_feed_result['reply']['card'].get('top_title'),
    'client_feed_top_action_label': client_feed_result['reply']['card'].get('top_action_label'),
    'client_feed_top_action_panel_id': client_feed_result['reply']['card'].get('top_action_panel_id'),
    'client_feed_commerce_visible_item_count': client_feed_commerce_result['reply']['card'].get('visible_item_count'),
    'client_feed_commerce_top_title': client_feed_commerce_result['reply']['card'].get('top_title'),
    'client_feed_commerce_top_action_label': client_feed_commerce_result['reply']['card'].get('top_action_label'),
    'client_feed_commerce_top_action_panel_id': client_feed_commerce_result['reply']['card'].get('top_action_panel_id'),
    'client_feed_route_task_graph_count': client_feed_result['reply']['card'].get('route_task_graph_count'),
    'client_feed_route_next_task_id': client_feed_result['reply']['card'].get('route_next_task_id'),
    'client_feed_route_next_action_label': client_feed_result['reply']['card'].get('route_next_action_label'),
    'client_feed_route_next_panel_id': client_feed_result['reply']['card'].get('route_next_panel_id'),
    'client_feed_route_next_stage_summary': client_feed_result['reply']['card'].get('route_next_stage_summary'),
    'client_feed_route_next_outcome_summary': client_feed_result['reply']['card'].get('route_next_outcome_summary'),
    'client_feed_route_next_feedback_focus': client_feed_result['reply']['card'].get('route_next_feedback_focus'),
    'client_feed_route_next_opportunity_hint': client_feed_result['reply']['card'].get('route_next_opportunity_hint'),
    'client_feed_route_next_opportunity_kind': client_feed_result['reply']['card'].get('route_next_opportunity_kind'),
    'client_feed_route_next_opportunity_command': client_feed_result['reply']['card'].get('route_next_opportunity_command'),
    'client_feed_route_next_opportunity_action_label': client_feed_result['reply']['card'].get('route_next_opportunity_action_label'),
    'client_feed_route_next_opportunity_panel_id': client_feed_result['reply']['card'].get('route_next_opportunity_panel_id'),
    'client_feed_route_next_opportunity_target_node_id': client_feed_result['reply']['card'].get('route_next_opportunity_target_node_id'),
    'client_app_route_preview_item_count': client_app_result['reply']['card'].get('route_preview_item_count'),
    'client_app_route_task_linked_count': client_app_result['reply']['card'].get('route_task_linked_count'),
    'client_app_route_task_graph_count': client_app_result['reply']['card'].get('route_task_graph_count'),
    'client_app_route_next_task_id': client_app_result['reply']['card'].get('route_next_task_id'),
    'client_app_route_next_action_label': client_app_result['reply']['card'].get('route_next_action_label'),
    'client_app_route_next_panel_id': client_app_result['reply']['card'].get('route_next_panel_id'),
    'client_app_route_next_location_id': client_app_result['reply']['card'].get('route_next_location_id'),
    'client_app_route_next_stage_summary': client_app_result['reply']['card'].get('route_next_stage_summary'),
    'client_app_route_next_node_id': client_app_result['reply']['card'].get('route_next_node_id'),
    'client_app_route_next_command_hint': client_app_result['reply']['card'].get('route_next_command_hint'),
    'client_app_route_next_outcome_summary': client_app_result['reply']['card'].get('route_next_outcome_summary'),
    'client_app_route_next_feedback_focus': client_app_result['reply']['card'].get('route_next_feedback_focus'),
    'client_app_route_next_opportunity_hint': client_app_result['reply']['card'].get('route_next_opportunity_hint'),
    'client_app_route_next_opportunity_kind': client_app_result['reply']['card'].get('route_next_opportunity_kind'),
    'client_app_route_next_opportunity_playbook': client_app_result['reply']['card'].get('route_next_opportunity_playbook'),
    'client_app_route_next_opportunity_command': client_app_result['reply']['card'].get('route_next_opportunity_command'),
    'client_app_route_next_opportunity_node_id': client_app_result['reply']['card'].get('route_next_opportunity_node_id'),
    'client_app_route_next_opportunity_action_label': client_app_result['reply']['card'].get('route_next_opportunity_action_label'),
    'client_app_route_next_opportunity_panel_id': client_app_result['reply']['card'].get('route_next_opportunity_panel_id'),
    'client_app_route_next_opportunity_textarea_id': client_app_result['reply']['card'].get('route_next_opportunity_textarea_id'),
    'client_app_route_next_opportunity_target_node_id': client_app_result['reply']['card'].get('route_next_opportunity_target_node_id'),
    'world_route_preview_item_count': world_result['reply']['card'].get('route_preview_item_count'),
    'world_route_task_linked_count': world_result['reply']['card'].get('route_task_linked_count'),
    'world_route_task_graph_count': world_result['reply']['card'].get('route_task_graph_count'),
    'world_route_next_task_id': world_result['reply']['card'].get('route_next_task_id'),
    'world_route_next_action_label': world_result['reply']['card'].get('route_next_action_label'),
    'world_route_next_panel_id': world_result['reply']['card'].get('route_next_panel_id'),
    'world_route_next_location_id': world_result['reply']['card'].get('route_next_location_id'),
    'world_route_next_stage_summary': world_result['reply']['card'].get('route_next_stage_summary'),
    'world_route_next_node_id': world_result['reply']['card'].get('route_next_node_id'),
    'world_route_next_command_hint': world_result['reply']['card'].get('route_next_command_hint'),
    'world_route_next_outcome_summary': world_result['reply']['card'].get('route_next_outcome_summary'),
    'world_route_next_feedback_focus': world_result['reply']['card'].get('route_next_feedback_focus'),
    'world_route_next_opportunity_hint': world_result['reply']['card'].get('route_next_opportunity_hint'),
    'world_route_next_opportunity_kind': world_result['reply']['card'].get('route_next_opportunity_kind'),
    'world_route_next_opportunity_playbook': world_result['reply']['card'].get('route_next_opportunity_playbook'),
    'world_route_next_opportunity_command': world_result['reply']['card'].get('route_next_opportunity_command'),
    'world_route_next_opportunity_node_id': world_result['reply']['card'].get('route_next_opportunity_node_id'),
    'world_route_next_opportunity_action_label': world_result['reply']['card'].get('route_next_opportunity_action_label'),
    'world_route_next_opportunity_panel_id': world_result['reply']['card'].get('route_next_opportunity_panel_id'),
    'world_route_next_opportunity_textarea_id': world_result['reply']['card'].get('route_next_opportunity_textarea_id'),
    'world_route_next_opportunity_target_node_id': world_result['reply']['card'].get('route_next_opportunity_target_node_id'),
    'world_route_task_graph_count_after_contract': world_route_result['reply']['card'].get('route_task_graph_count'),
    'world_route_next_task_id_after_contract': world_route_result['reply']['card'].get('route_next_task_id'),
    'world_route_next_action_label_after_contract': world_route_result['reply']['card'].get('route_next_action_label'),
    'world_route_next_panel_id_after_contract': world_route_result['reply']['card'].get('route_next_panel_id'),
    'world_route_next_location_id_after_contract': world_route_result['reply']['card'].get('route_next_location_id'),
    'world_route_next_stage_summary_after_contract': world_route_result['reply']['card'].get('route_next_stage_summary'),
    'world_route_next_node_id_after_contract': world_route_result['reply']['card'].get('route_next_node_id'),
    'world_route_next_command_hint_after_contract': world_route_result['reply']['card'].get('route_next_command_hint'),
    'world_route_next_outcome_summary_after_contract': world_route_result['reply']['card'].get('route_next_outcome_summary'),
    'world_route_next_feedback_focus_after_contract': world_route_result['reply']['card'].get('route_next_feedback_focus'),
    'world_route_next_opportunity_hint_after_contract': world_route_result['reply']['card'].get('route_next_opportunity_hint'),
    'world_route_next_opportunity_kind_after_contract': world_route_result['reply']['card'].get('route_next_opportunity_kind'),
    'world_route_next_opportunity_playbook_after_contract': world_route_result['reply']['card'].get('route_next_opportunity_playbook'),
    'world_route_next_opportunity_command_after_contract': world_route_result['reply']['card'].get('route_next_opportunity_command'),
    'world_route_next_opportunity_node_id_after_contract': world_route_result['reply']['card'].get('route_next_opportunity_node_id'),
    'world_route_next_opportunity_action_label_after_contract': world_route_result['reply']['card'].get('route_next_opportunity_action_label'),
    'world_route_next_opportunity_panel_id_after_contract': world_route_result['reply']['card'].get('route_next_opportunity_panel_id'),
    'world_route_next_opportunity_textarea_id_after_contract': world_route_result['reply']['card'].get('route_next_opportunity_textarea_id'),
    'world_route_next_opportunity_target_node_id_after_contract': world_route_result['reply']['card'].get('route_next_opportunity_target_node_id'),
    'world_map_route_preview_item_count': world_map_result['reply']['card'].get('route_preview_item_count'),
    'world_map_route_task_graph_count': world_map_result['reply']['card'].get('route_task_graph_count'),
    'world_map_route_next_task_id': world_map_result['reply']['card'].get('route_next_task_id'),
    'world_map_route_next_action_label': world_map_result['reply']['card'].get('route_next_action_label'),
    'world_map_route_next_panel_id': world_map_result['reply']['card'].get('route_next_panel_id'),
    'world_map_route_next_location_id': world_map_result['reply']['card'].get('route_next_location_id'),
    'world_map_route_next_stage_summary': world_map_result['reply']['card'].get('route_next_stage_summary'),
    'world_map_route_next_node_id': world_map_result['reply']['card'].get('route_next_node_id'),
    'world_map_route_next_command_hint': world_map_result['reply']['card'].get('route_next_command_hint'),
    'world_map_route_next_outcome_summary': world_map_result['reply']['card'].get('route_next_outcome_summary'),
    'world_map_route_next_feedback_focus': world_map_result['reply']['card'].get('route_next_feedback_focus'),
    'world_map_route_next_opportunity_hint': world_map_result['reply']['card'].get('route_next_opportunity_hint'),
    'world_map_route_next_opportunity_kind': world_map_result['reply']['card'].get('route_next_opportunity_kind'),
    'world_map_route_next_opportunity_playbook': world_map_result['reply']['card'].get('route_next_opportunity_playbook'),
    'world_map_route_next_opportunity_command': world_map_result['reply']['card'].get('route_next_opportunity_command'),
    'world_map_route_next_opportunity_node_id': world_map_result['reply']['card'].get('route_next_opportunity_node_id'),
    'world_map_route_next_opportunity_action_label': world_map_result['reply']['card'].get('route_next_opportunity_action_label'),
    'world_map_route_next_opportunity_panel_id': world_map_result['reply']['card'].get('route_next_opportunity_panel_id'),
    'world_map_route_next_opportunity_textarea_id': world_map_result['reply']['card'].get('route_next_opportunity_textarea_id'),
    'world_map_route_next_opportunity_target_node_id': world_map_result['reply']['card'].get('route_next_opportunity_target_node_id'),
    'client_app_route_task_graph_count_after_contract': client_app_route_result['reply']['card'].get('route_task_graph_count'),
    'client_app_route_next_task_id_after_contract': client_app_route_result['reply']['card'].get('route_next_task_id'),
    'client_app_route_next_action_label_after_contract': client_app_route_result['reply']['card'].get('route_next_action_label'),
    'client_app_route_next_panel_id_after_contract': client_app_route_result['reply']['card'].get('route_next_panel_id'),
    'client_app_route_next_location_id_after_contract': client_app_route_result['reply']['card'].get('route_next_location_id'),
    'client_app_route_next_stage_summary_after_contract': client_app_route_result['reply']['card'].get('route_next_stage_summary'),
    'client_app_route_next_node_id_after_contract': client_app_route_result['reply']['card'].get('route_next_node_id'),
    'client_app_route_next_command_hint_after_contract': client_app_route_result['reply']['card'].get('route_next_command_hint'),
    'client_app_route_next_outcome_summary_after_contract': client_app_route_result['reply']['card'].get('route_next_outcome_summary'),
    'client_app_route_next_feedback_focus_after_contract': client_app_route_result['reply']['card'].get('route_next_feedback_focus'),
    'client_app_route_next_opportunity_hint_after_contract': client_app_route_result['reply']['card'].get('route_next_opportunity_hint'),
    'client_app_route_next_opportunity_kind_after_contract': client_app_route_result['reply']['card'].get('route_next_opportunity_kind'),
    'client_app_route_next_opportunity_playbook_after_contract': client_app_route_result['reply']['card'].get('route_next_opportunity_playbook'),
    'client_app_route_next_opportunity_command_after_contract': client_app_route_result['reply']['card'].get('route_next_opportunity_command'),
    'client_app_route_next_opportunity_node_id_after_contract': client_app_route_result['reply']['card'].get('route_next_opportunity_node_id'),
    'client_app_route_next_opportunity_action_label_after_contract': client_app_route_result['reply']['card'].get('route_next_opportunity_action_label'),
    'client_app_route_next_opportunity_panel_id_after_contract': client_app_route_result['reply']['card'].get('route_next_opportunity_panel_id'),
    'client_app_route_next_opportunity_textarea_id_after_contract': client_app_route_result['reply']['card'].get('route_next_opportunity_textarea_id'),
    'client_app_route_next_opportunity_target_node_id_after_contract': client_app_route_result['reply']['card'].get('route_next_opportunity_target_node_id'),
    'world_map_route_task_graph_count_after_contract': world_map_route_result['reply']['card'].get('route_task_graph_count'),
    'world_map_route_next_task_id_after_contract': world_map_route_result['reply']['card'].get('route_next_task_id'),
    'world_map_route_next_action_label_after_contract': world_map_route_result['reply']['card'].get('route_next_action_label'),
    'world_map_route_next_panel_id_after_contract': world_map_route_result['reply']['card'].get('route_next_panel_id'),
    'world_map_route_next_location_id_after_contract': world_map_route_result['reply']['card'].get('route_next_location_id'),
    'world_map_route_next_stage_summary_after_contract': world_map_route_result['reply']['card'].get('route_next_stage_summary'),
    'world_map_route_next_node_id_after_contract': world_map_route_result['reply']['card'].get('route_next_node_id'),
    'world_map_route_next_command_hint_after_contract': world_map_route_result['reply']['card'].get('route_next_command_hint'),
    'world_map_route_next_outcome_summary_after_contract': world_map_route_result['reply']['card'].get('route_next_outcome_summary'),
    'world_map_route_next_feedback_focus_after_contract': world_map_route_result['reply']['card'].get('route_next_feedback_focus'),
    'world_map_route_next_opportunity_hint_after_contract': world_map_route_result['reply']['card'].get('route_next_opportunity_hint'),
    'world_map_route_next_opportunity_kind_after_contract': world_map_route_result['reply']['card'].get('route_next_opportunity_kind'),
    'world_map_route_next_opportunity_playbook_after_contract': world_map_route_result['reply']['card'].get('route_next_opportunity_playbook'),
    'world_map_route_next_opportunity_command_after_contract': world_map_route_result['reply']['card'].get('route_next_opportunity_command'),
    'world_map_route_next_opportunity_node_id_after_contract': world_map_route_result['reply']['card'].get('route_next_opportunity_node_id'),
    'world_map_route_next_opportunity_action_label_after_contract': world_map_route_result['reply']['card'].get('route_next_opportunity_action_label'),
    'world_map_route_next_opportunity_panel_id_after_contract': world_map_route_result['reply']['card'].get('route_next_opportunity_panel_id'),
    'world_map_route_next_opportunity_textarea_id_after_contract': world_map_route_result['reply']['card'].get('route_next_opportunity_textarea_id'),
    'world_map_route_next_opportunity_target_node_id_after_contract': world_map_route_result['reply']['card'].get('route_next_opportunity_target_node_id'),
    'world_assets_route_task_graph_count': world_assets_result['reply']['card'].get('route_task_graph_count'),
    'world_assets_route_next_action_label': world_assets_result['reply']['card'].get('route_next_action_label'),
    'world_assets_route_next_panel_id': world_assets_result['reply']['card'].get('route_next_panel_id'),
    'world_assets_route_next_stage_summary': world_assets_result['reply']['card'].get('route_next_stage_summary'),
    'world_assets_route_next_node_id': world_assets_result['reply']['card'].get('route_next_node_id'),
    'world_assets_route_next_outcome_summary': world_assets_result['reply']['card'].get('route_next_outcome_summary'),
    'world_assets_route_next_feedback_focus': world_assets_result['reply']['card'].get('route_next_feedback_focus'),
    'world_assets_route_next_opportunity_hint': world_assets_result['reply']['card'].get('route_next_opportunity_hint'),
    'world_assets_route_next_opportunity_kind': world_assets_result['reply']['card'].get('route_next_opportunity_kind'),
    'world_assets_route_next_opportunity_playbook': world_assets_result['reply']['card'].get('route_next_opportunity_playbook'),
    'world_assets_route_next_opportunity_command': world_assets_result['reply']['card'].get('route_next_opportunity_command'),
    'world_assets_route_next_opportunity_node_id': world_assets_result['reply']['card'].get('route_next_opportunity_node_id'),
    'world_companies_route_task_graph_count': world_companies_result['reply']['card'].get('route_task_graph_count'),
    'world_companies_route_next_action_label': world_companies_result['reply']['card'].get('route_next_action_label'),
    'world_companies_route_next_panel_id': world_companies_result['reply']['card'].get('route_next_panel_id'),
    'world_companies_route_next_stage_summary': world_companies_result['reply']['card'].get('route_next_stage_summary'),
    'world_companies_route_next_node_id': world_companies_result['reply']['card'].get('route_next_node_id'),
    'world_companies_route_next_outcome_summary': world_companies_result['reply']['card'].get('route_next_outcome_summary'),
    'world_companies_route_next_feedback_focus': world_companies_result['reply']['card'].get('route_next_feedback_focus'),
    'world_companies_route_next_opportunity_hint': world_companies_result['reply']['card'].get('route_next_opportunity_hint'),
    'world_companies_route_next_opportunity_kind': world_companies_result['reply']['card'].get('route_next_opportunity_kind'),
    'world_companies_route_next_opportunity_playbook': world_companies_result['reply']['card'].get('route_next_opportunity_playbook'),
    'world_companies_route_next_opportunity_command': world_companies_result['reply']['card'].get('route_next_opportunity_command'),
    'world_companies_route_next_opportunity_node_id': world_companies_result['reply']['card'].get('route_next_opportunity_node_id'),
    'world_shops_route_task_graph_count': world_shops_result['reply']['card'].get('route_task_graph_count'),
    'world_shops_route_next_action_label': world_shops_result['reply']['card'].get('route_next_action_label'),
    'world_shops_route_next_panel_id': world_shops_result['reply']['card'].get('route_next_panel_id'),
    'world_shops_route_next_stage_summary': world_shops_result['reply']['card'].get('route_next_stage_summary'),
    'world_shops_route_next_node_id': world_shops_result['reply']['card'].get('route_next_node_id'),
    'world_shops_route_next_outcome_summary': world_shops_result['reply']['card'].get('route_next_outcome_summary'),
    'world_shops_route_next_feedback_focus': world_shops_result['reply']['card'].get('route_next_feedback_focus'),
    'world_shops_route_next_opportunity_hint': world_shops_result['reply']['card'].get('route_next_opportunity_hint'),
    'world_shops_route_next_opportunity_kind': world_shops_result['reply']['card'].get('route_next_opportunity_kind'),
    'world_shops_route_next_opportunity_playbook': world_shops_result['reply']['card'].get('route_next_opportunity_playbook'),
    'world_shops_route_next_opportunity_command': world_shops_result['reply']['card'].get('route_next_opportunity_command'),
    'world_shops_route_next_opportunity_node_id': world_shops_result['reply']['card'].get('route_next_opportunity_node_id'),
    'world_listing_route_task_graph_count': world_listing_result['reply']['card'].get('route_task_graph_count'),
    'world_listing_route_next_action_label': world_listing_result['reply']['card'].get('route_next_action_label'),
    'world_listing_route_next_panel_id': world_listing_result['reply']['card'].get('route_next_panel_id'),
    'world_listing_route_next_stage_summary': world_listing_result['reply']['card'].get('route_next_stage_summary'),
    'world_listing_route_next_node_id': world_listing_result['reply']['card'].get('route_next_node_id'),
    'world_listing_route_next_outcome_summary': world_listing_result['reply']['card'].get('route_next_outcome_summary'),
    'world_listing_route_next_feedback_focus': world_listing_result['reply']['card'].get('route_next_feedback_focus'),
    'world_listing_route_next_opportunity_hint': world_listing_result['reply']['card'].get('route_next_opportunity_hint'),
    'world_listing_route_next_opportunity_kind': world_listing_result['reply']['card'].get('route_next_opportunity_kind'),
    'world_listing_route_next_opportunity_playbook': world_listing_result['reply']['card'].get('route_next_opportunity_playbook'),
    'world_listing_route_next_opportunity_command': world_listing_result['reply']['card'].get('route_next_opportunity_command'),
    'world_listing_route_next_opportunity_node_id': world_listing_result['reply']['card'].get('route_next_opportunity_node_id'),
    'world_purchase_route_task_graph_count': world_purchase_result['reply']['card'].get('route_task_graph_count'),
    'world_purchase_route_next_action_label': world_purchase_result['reply']['card'].get('route_next_action_label'),
    'world_purchase_route_next_panel_id': world_purchase_result['reply']['card'].get('route_next_panel_id'),
    'world_purchase_route_next_stage_summary': world_purchase_result['reply']['card'].get('route_next_stage_summary'),
    'world_purchase_route_next_node_id': world_purchase_result['reply']['card'].get('route_next_node_id'),
    'world_purchase_route_next_outcome_summary': world_purchase_result['reply']['card'].get('route_next_outcome_summary'),
    'world_purchase_route_next_feedback_focus': world_purchase_result['reply']['card'].get('route_next_feedback_focus'),
    'world_purchase_route_next_opportunity_hint': world_purchase_result['reply']['card'].get('route_next_opportunity_hint'),
    'world_purchase_route_next_opportunity_kind': world_purchase_result['reply']['card'].get('route_next_opportunity_kind'),
    'world_purchase_route_next_opportunity_playbook': world_purchase_result['reply']['card'].get('route_next_opportunity_playbook'),
    'world_purchase_route_next_opportunity_command': world_purchase_result['reply']['card'].get('route_next_opportunity_command'),
    'world_purchase_route_next_opportunity_node_id': world_purchase_result['reply']['card'].get('route_next_opportunity_node_id'),
    'world_work_route_task_graph_count': world_work_result['reply']['card'].get('route_task_graph_count'),
    'world_work_route_next_action_label': world_work_result['reply']['card'].get('route_next_action_label'),
    'world_work_route_next_panel_id': world_work_result['reply']['card'].get('route_next_panel_id'),
    'world_work_route_next_stage_summary': world_work_result['reply']['card'].get('route_next_stage_summary'),
    'world_work_route_next_node_id': world_work_result['reply']['card'].get('route_next_node_id'),
    'world_work_route_next_outcome_summary': world_work_result['reply']['card'].get('route_next_outcome_summary'),
    'world_work_route_next_feedback_focus': world_work_result['reply']['card'].get('route_next_feedback_focus'),
    'world_work_route_next_opportunity_hint': world_work_result['reply']['card'].get('route_next_opportunity_hint'),
    'world_work_route_next_opportunity_kind': world_work_result['reply']['card'].get('route_next_opportunity_kind'),
    'world_work_route_next_opportunity_playbook': world_work_result['reply']['card'].get('route_next_opportunity_playbook'),
    'world_work_route_next_opportunity_command': world_work_result['reply']['card'].get('route_next_opportunity_command'),
    'world_work_route_next_opportunity_node_id': world_work_result['reply']['card'].get('route_next_opportunity_node_id'),
    'world_work_rejection_route_task_graph_count': world_work_rejection_result['reply']['card'].get('route_task_graph_count'),
    'world_work_rejection_route_next_task_id': world_work_rejection_result['reply']['card'].get('route_next_task_id'),
    'world_work_rejection_route_next_action_label': world_work_rejection_result['reply']['card'].get('route_next_action_label'),
    'world_work_rejection_route_next_panel_id': world_work_rejection_result['reply']['card'].get('route_next_panel_id'),
    'world_work_rejection_route_next_location_id': world_work_rejection_result['reply']['card'].get('route_next_location_id'),
    'world_work_rejection_route_next_stage_summary': world_work_rejection_result['reply']['card'].get('route_next_stage_summary'),
    'world_work_rejection_route_next_node_id': world_work_rejection_result['reply']['card'].get('route_next_node_id'),
    'world_work_rejection_route_next_command_hint': world_work_rejection_result['reply']['card'].get('route_next_command_hint'),
    'world_work_rejection_route_next_outcome_summary': world_work_rejection_result['reply']['card'].get('route_next_outcome_summary'),
    'world_work_rejection_route_next_feedback_focus': world_work_rejection_result['reply']['card'].get('route_next_feedback_focus'),
    'world_work_rejection_route_next_opportunity_hint': world_work_rejection_result['reply']['card'].get('route_next_opportunity_hint'),
    'world_work_rejection_route_next_opportunity_kind': world_work_rejection_result['reply']['card'].get('route_next_opportunity_kind'),
    'world_work_rejection_route_next_opportunity_playbook': world_work_rejection_result['reply']['card'].get('route_next_opportunity_playbook'),
    'world_work_rejection_route_next_opportunity_command': world_work_rejection_result['reply']['card'].get('route_next_opportunity_command'),
    'world_work_rejection_route_next_opportunity_node_id': world_work_rejection_result['reply']['card'].get('route_next_opportunity_node_id'),
    'world_work_reopen_route_task_graph_count': world_work_reopen_result['reply']['card'].get('route_task_graph_count'),
    'world_work_reopen_route_next_task_id': world_work_reopen_result['reply']['card'].get('route_next_task_id'),
    'world_work_reopen_route_next_action_label': world_work_reopen_result['reply']['card'].get('route_next_action_label'),
    'world_work_reopen_route_next_panel_id': world_work_reopen_result['reply']['card'].get('route_next_panel_id'),
    'world_work_reopen_route_next_location_id': world_work_reopen_result['reply']['card'].get('route_next_location_id'),
    'world_work_reopen_route_next_stage_summary': world_work_reopen_result['reply']['card'].get('route_next_stage_summary'),
    'world_work_reopen_route_next_node_id': world_work_reopen_result['reply']['card'].get('route_next_node_id'),
    'world_work_reopen_route_next_command_hint': world_work_reopen_result['reply']['card'].get('route_next_command_hint'),
    'world_work_reopen_route_next_outcome_summary': world_work_reopen_result['reply']['card'].get('route_next_outcome_summary'),
    'world_work_reopen_route_next_feedback_focus': world_work_reopen_result['reply']['card'].get('route_next_feedback_focus'),
    'world_work_reopen_route_next_opportunity_hint': world_work_reopen_result['reply']['card'].get('route_next_opportunity_hint'),
    'world_work_reopen_route_next_opportunity_kind': world_work_reopen_result['reply']['card'].get('route_next_opportunity_kind'),
    'world_work_reopen_route_next_opportunity_playbook': world_work_reopen_result['reply']['card'].get('route_next_opportunity_playbook'),
    'world_work_reopen_route_next_opportunity_command': world_work_reopen_result['reply']['card'].get('route_next_opportunity_command'),
    'world_work_reopen_route_next_opportunity_node_id': world_work_reopen_result['reply']['card'].get('route_next_opportunity_node_id'),
    'world_work_cancellation_route_task_graph_count': world_work_cancellation_result['reply']['card'].get('route_task_graph_count'),
    'world_work_cancellation_route_next_task_id': world_work_cancellation_result['reply']['card'].get('route_next_task_id'),
    'world_work_cancellation_route_next_action_label': world_work_cancellation_result['reply']['card'].get('route_next_action_label'),
    'world_work_cancellation_route_next_panel_id': world_work_cancellation_result['reply']['card'].get('route_next_panel_id'),
    'world_work_cancellation_route_next_location_id': world_work_cancellation_result['reply']['card'].get('route_next_location_id'),
    'world_work_cancellation_route_next_stage_summary': world_work_cancellation_result['reply']['card'].get('route_next_stage_summary'),
    'world_work_cancellation_route_next_node_id': world_work_cancellation_result['reply']['card'].get('route_next_node_id'),
    'world_work_cancellation_route_next_command_hint': world_work_cancellation_result['reply']['card'].get('route_next_command_hint'),
    'world_work_cancellation_route_next_outcome_summary': world_work_cancellation_result['reply']['card'].get('route_next_outcome_summary'),
    'world_work_cancellation_route_next_feedback_focus': world_work_cancellation_result['reply']['card'].get('route_next_feedback_focus'),
    'world_work_cancellation_route_next_opportunity_hint': world_work_cancellation_result['reply']['card'].get('route_next_opportunity_hint'),
    'world_work_cancellation_route_next_opportunity_kind': world_work_cancellation_result['reply']['card'].get('route_next_opportunity_kind'),
    'world_work_cancellation_route_next_opportunity_playbook': world_work_cancellation_result['reply']['card'].get('route_next_opportunity_playbook'),
    'world_work_cancellation_route_next_opportunity_command': world_work_cancellation_result['reply']['card'].get('route_next_opportunity_command'),
    'world_work_cancellation_route_next_opportunity_node_id': world_work_cancellation_result['reply']['card'].get('route_next_opportunity_node_id'),
})
for prefix, result in [
    ('client_app', client_app_result),
    ('client_feed', client_feed_result),
    ('world', world_result),
    ('world_map', world_map_result),
]:
    summary.update(route_runner_handoff_summary(prefix, result['reply']['card']))
for prefix, result in [
    ('world_assets', world_assets_result),
    ('world_asset_upgrade', world_asset_upgrade_result),
    ('world_companies', world_companies_result),
    ('world_company', world_company_result),
    ('world_shops', world_shops_result),
    ('world_listing', world_listing_result),
    ('world_purchase', world_purchase_result),
    ('world_work_delivery', world_work_delivery_result),
    ('world_work_acceptance', world_work_acceptance_result),
    ('world_work', world_work_result),
    ('world_work_rejection', world_work_rejection_result),
    ('world_work_reopen', world_work_reopen_result),
    ('world_work_cancellation', world_work_cancellation_result),
]:
    card = result['reply']['card']
    summary.update({
        f'{prefix}_route_preview_item_count': card.get('route_preview_item_count'),
        f'{prefix}_route_task_linked_count': card.get('route_task_linked_count'),
        f'{prefix}_route_task_graph_count': card.get('route_task_graph_count'),
        f'{prefix}_route_next_task_id': card.get('route_next_task_id'),
        f'{prefix}_route_next_action_label': card.get('route_next_action_label'),
        f'{prefix}_route_next_panel_id': card.get('route_next_panel_id'),
        f'{prefix}_route_next_location_id': card.get('route_next_location_id'),
        f'{prefix}_route_next_node_id': card.get('route_next_node_id'),
        f'{prefix}_route_next_stage_summary': card.get('route_next_stage_summary'),
        f'{prefix}_route_next_command_hint': card.get('route_next_command_hint'),
        f'{prefix}_route_next_outcome_summary': card.get('route_next_outcome_summary'),
        f'{prefix}_route_next_feedback_focus': card.get('route_next_feedback_focus'),
        f'{prefix}_route_next_opportunity_hint': card.get('route_next_opportunity_hint'),
        f'{prefix}_route_next_opportunity_playbook': card.get('route_next_opportunity_playbook'),
        f'{prefix}_route_next_opportunity_kind': card.get('route_next_opportunity_kind'),
        f'{prefix}_route_next_opportunity_command': card.get('route_next_opportunity_command'),
        f'{prefix}_route_next_opportunity_node_id': card.get('route_next_opportunity_node_id'),
        f'{prefix}_route_next_opportunity_action_label': card.get('route_next_opportunity_action_label'),
        f'{prefix}_route_next_opportunity_panel_id': card.get('route_next_opportunity_panel_id'),
        f'{prefix}_route_next_opportunity_input_id': card.get('route_next_opportunity_input_id'),
        f'{prefix}_route_next_opportunity_input_value': card.get('route_next_opportunity_input_value'),
        f'{prefix}_route_next_opportunity_textarea_id': card.get('route_next_opportunity_textarea_id'),
        f'{prefix}_route_next_opportunity_target_node_id': card.get('route_next_opportunity_target_node_id'),
    })
path = root / 'run/matrix-live' / f'e2e-summary-{int(time.time())}.json'
path.write_text(json.dumps(summary, ensure_ascii=False, indent=2))
print(json.dumps({
    'ok': True,
    'summary': str(path),
    'room_id': room,
    'task_id': task_id,
    'task_event_id': task_result['reply']['event_id'],
    'status_event_id': status_result['reply']['event_id'],
    'balance_event_id': balance_result['reply']['event_id'],
    'plans_event_id': plans_result['reply']['event_id'],
    'client_app_event_id': client_app_result['reply']['event_id'],
    'client_app_module_count': client_app_result['reply']['card'].get('module_count'),
    'client_app_map_engine_id': client_app_result['reply']['card'].get('map_engine_id'),
    'client_app_primary_entry_module_id': client_app_result['reply']['card'].get('primary_entry_module_id'),
    'client_app_active_region_id': client_app_result['reply']['card'].get('active_region_id'),
    'client_app_tile_shard_count': client_app_result['reply']['card'].get('tile_shard_count'),
    'client_app_nearby_poi_count': client_app_result['reply']['card'].get('nearby_poi_count'),
    'client_app_prefetch_count': client_app_result['reply']['card'].get('prefetch_count'),
    'client_app_live_event_count': client_app_result['reply']['card'].get('live_event_count'),
    'client_app_player_density_mode': client_app_result['reply']['card'].get('player_density_mode'),
    'client_app_has_first_playable_onboarding': client_app_result['reply']['card'].get('has_first_playable_onboarding'),
    'client_app_onboarding_contract_version': client_app_result['reply']['card'].get('onboarding_contract_version'),
    'client_app_onboarding_completion_target': client_app_result['reply']['card'].get('onboarding_completion_target'),
    'client_app_onboarding_step_count': client_app_result['reply']['card'].get('onboarding_step_count'),
    'client_app_route_preview_item_count': client_app_result['reply']['card'].get('route_preview_item_count'),
    'client_app_route_task_linked_count': client_app_result['reply']['card'].get('route_task_linked_count'),
    'client_app_route_task_graph_count': client_app_result['reply']['card'].get('route_task_graph_count'),
    'client_app_route_next_task_id': client_app_result['reply']['card'].get('route_next_task_id'),
    'client_app_route_next_action_label': client_app_result['reply']['card'].get('route_next_action_label'),
    'client_app_route_next_panel_id': client_app_result['reply']['card'].get('route_next_panel_id'),
    'client_app_route_next_location_id': client_app_result['reply']['card'].get('route_next_location_id'),
    'client_app_route_next_stage_summary': client_app_result['reply']['card'].get('route_next_stage_summary'),
    'client_app_route_next_node_id': client_app_result['reply']['card'].get('route_next_node_id'),
    'client_app_route_next_command_hint': client_app_result['reply']['card'].get('route_next_command_hint'),
    'client_app_route_next_outcome_summary': client_app_result['reply']['card'].get('route_next_outcome_summary'),
    'client_app_route_next_feedback_focus': client_app_result['reply']['card'].get('route_next_feedback_focus'),
    'client_app_route_next_opportunity_hint': client_app_result['reply']['card'].get('route_next_opportunity_hint'),
    'client_app_route_next_opportunity_kind': client_app_result['reply']['card'].get('route_next_opportunity_kind'),
    'client_app_route_next_opportunity_playbook': client_app_result['reply']['card'].get('route_next_opportunity_playbook'),
    'client_app_route_next_opportunity_command': client_app_result['reply']['card'].get('route_next_opportunity_command'),
    'client_app_route_next_opportunity_node_id': client_app_result['reply']['card'].get('route_next_opportunity_node_id'),
    'client_app_route_next_opportunity_action_label': client_app_result['reply']['card'].get('route_next_opportunity_action_label'),
    'client_app_route_next_opportunity_panel_id': client_app_result['reply']['card'].get('route_next_opportunity_panel_id'),
    'client_app_route_next_opportunity_textarea_id': client_app_result['reply']['card'].get('route_next_opportunity_textarea_id'),
    'client_app_route_next_opportunity_target_node_id': client_app_result['reply']['card'].get('route_next_opportunity_target_node_id'),
    'client_app_tile_provider': client_app_result['reply']['card'].get('tile_provider'),
    'client_app_progression_level': client_app_result['reply']['card'].get('progression_level'),
    'client_social_event_id': client_social_result['reply']['event_id'],
    'client_social_contact_count': client_social_result['reply']['card'].get('contact_count'),
    'client_duel_event_id': client_duel_result['reply']['event_id'],
    'client_duel_task_id': client_duel_result['reply']['card'].get('task_id'),
    'league_event_id': league_result['reply']['event_id'],
    'world_event_id': world_result['reply']['event_id'],
    'world_route_preview_item_count': world_result['reply']['card'].get('route_preview_item_count'),
    'world_route_task_linked_count': world_result['reply']['card'].get('route_task_linked_count'),
    'world_route_task_graph_count': world_result['reply']['card'].get('route_task_graph_count'),
    'world_route_next_task_id': world_result['reply']['card'].get('route_next_task_id'),
    'world_route_next_action_label': world_result['reply']['card'].get('route_next_action_label'),
    'world_route_next_panel_id': world_result['reply']['card'].get('route_next_panel_id'),
    'world_route_next_location_id': world_result['reply']['card'].get('route_next_location_id'),
    'world_route_next_stage_summary': world_result['reply']['card'].get('route_next_stage_summary'),
    'world_route_next_node_id': world_result['reply']['card'].get('route_next_node_id'),
    'world_route_next_command_hint': world_result['reply']['card'].get('route_next_command_hint'),
    'world_route_next_outcome_summary': world_result['reply']['card'].get('route_next_outcome_summary'),
    'world_route_next_feedback_focus': world_result['reply']['card'].get('route_next_feedback_focus'),
    'world_route_next_opportunity_hint': world_result['reply']['card'].get('route_next_opportunity_hint'),
    'world_route_next_opportunity_kind': world_result['reply']['card'].get('route_next_opportunity_kind'),
    'world_route_next_opportunity_playbook': world_result['reply']['card'].get('route_next_opportunity_playbook'),
    'world_route_next_opportunity_command': world_result['reply']['card'].get('route_next_opportunity_command'),
    'world_route_next_opportunity_node_id': world_result['reply']['card'].get('route_next_opportunity_node_id'),
    'world_route_next_opportunity_action_label': world_result['reply']['card'].get('route_next_opportunity_action_label'),
    'world_route_next_opportunity_panel_id': world_result['reply']['card'].get('route_next_opportunity_panel_id'),
    'world_route_next_opportunity_textarea_id': world_result['reply']['card'].get('route_next_opportunity_textarea_id'),
    'world_route_next_opportunity_target_node_id': world_result['reply']['card'].get('route_next_opportunity_target_node_id'),
    'world_route_event_id': world_route_result['reply']['event_id'],
    'world_route_task_graph_count_after_contract': world_route_result['reply']['card'].get('route_task_graph_count'),
    'world_route_next_task_id_after_contract': world_route_result['reply']['card'].get('route_next_task_id'),
    'world_route_next_action_label_after_contract': world_route_result['reply']['card'].get('route_next_action_label'),
    'world_route_next_panel_id_after_contract': world_route_result['reply']['card'].get('route_next_panel_id'),
    'world_route_next_location_id_after_contract': world_route_result['reply']['card'].get('route_next_location_id'),
    'world_route_next_stage_summary_after_contract': world_route_result['reply']['card'].get('route_next_stage_summary'),
    'world_route_next_node_id_after_contract': world_route_result['reply']['card'].get('route_next_node_id'),
    'world_route_next_command_hint_after_contract': world_route_result['reply']['card'].get('route_next_command_hint'),
    'world_route_next_outcome_summary_after_contract': world_route_result['reply']['card'].get('route_next_outcome_summary'),
    'world_route_next_feedback_focus_after_contract': world_route_result['reply']['card'].get('route_next_feedback_focus'),
    'world_route_next_opportunity_hint_after_contract': world_route_result['reply']['card'].get('route_next_opportunity_hint'),
    'world_route_next_opportunity_kind_after_contract': world_route_result['reply']['card'].get('route_next_opportunity_kind'),
    'world_route_next_opportunity_playbook_after_contract': world_route_result['reply']['card'].get('route_next_opportunity_playbook'),
    'world_route_next_opportunity_command_after_contract': world_route_result['reply']['card'].get('route_next_opportunity_command'),
    'world_route_next_opportunity_node_id_after_contract': world_route_result['reply']['card'].get('route_next_opportunity_node_id'),
    'world_route_next_opportunity_action_label_after_contract': world_route_result['reply']['card'].get('route_next_opportunity_action_label'),
    'world_route_next_opportunity_panel_id_after_contract': world_route_result['reply']['card'].get('route_next_opportunity_panel_id'),
    'world_route_next_opportunity_textarea_id_after_contract': world_route_result['reply']['card'].get('route_next_opportunity_textarea_id'),
    'world_route_next_opportunity_target_node_id_after_contract': world_route_result['reply']['card'].get('route_next_opportunity_target_node_id'),
    'world_map_event_id': world_map_result['reply']['event_id'],
    'world_map_current_node_id': world_map_result['reply']['card'].get('current_node_id'),
    'world_map_node_count': world_map_result['reply']['card'].get('node_count'),
    'world_map_engine_id': world_map_result['reply']['card'].get('map_engine_id'),
    'world_map_active_region_id': world_map_result['reply']['card'].get('active_region_id'),
    'world_map_route_preview_item_count': world_map_result['reply']['card'].get('route_preview_item_count'),
    'world_map_route_task_graph_count': world_map_result['reply']['card'].get('route_task_graph_count'),
    'world_map_route_next_task_id': world_map_result['reply']['card'].get('route_next_task_id'),
    'world_map_route_next_action_label': world_map_result['reply']['card'].get('route_next_action_label'),
    'world_map_route_next_panel_id': world_map_result['reply']['card'].get('route_next_panel_id'),
    'world_map_route_next_location_id': world_map_result['reply']['card'].get('route_next_location_id'),
    'world_map_route_next_stage_summary': world_map_result['reply']['card'].get('route_next_stage_summary'),
    'world_map_route_next_node_id': world_map_result['reply']['card'].get('route_next_node_id'),
    'world_map_route_next_command_hint': world_map_result['reply']['card'].get('route_next_command_hint'),
    'world_map_route_next_outcome_summary': world_map_result['reply']['card'].get('route_next_outcome_summary'),
    'world_map_route_next_feedback_focus': world_map_result['reply']['card'].get('route_next_feedback_focus'),
    'world_map_route_next_opportunity_hint': world_map_result['reply']['card'].get('route_next_opportunity_hint'),
    'world_map_route_next_opportunity_kind': world_map_result['reply']['card'].get('route_next_opportunity_kind'),
    'world_map_route_next_opportunity_playbook': world_map_result['reply']['card'].get('route_next_opportunity_playbook'),
    'world_map_route_next_opportunity_command': world_map_result['reply']['card'].get('route_next_opportunity_command'),
    'world_map_route_next_opportunity_node_id': world_map_result['reply']['card'].get('route_next_opportunity_node_id'),
    'world_map_route_next_opportunity_action_label': world_map_result['reply']['card'].get('route_next_opportunity_action_label'),
    'world_map_route_next_opportunity_panel_id': world_map_result['reply']['card'].get('route_next_opportunity_panel_id'),
    'world_map_route_next_opportunity_textarea_id': world_map_result['reply']['card'].get('route_next_opportunity_textarea_id'),
    'world_map_route_next_opportunity_target_node_id': world_map_result['reply']['card'].get('route_next_opportunity_target_node_id'),
    'world_map_move_event_id': world_map_move_result['reply']['event_id'],
    'world_map_move_to_node_id': world_map_move_result['reply']['card'].get('to_node_id'),
    'world_action_event_id': world_action_result['reply']['event_id'],
    'world_action_kind': world_action_result['reply']['card'].get('event_kind'),
    'craft_action_event_id': craft_action_result['reply']['event_id'],
    'craft_action_kind': craft_action_result['reply']['card'].get('event_kind'),
    'world_contract_event_id': world_contract_result['reply']['event_id'],
    'world_contract_task_id': world_contract_result['reply']['card'].get('task_id'),
    'world_contract_id': world_contract_id,
    'world_contract_completion_event_id': world_contract_completion_result['reply']['event_id'],
    'world_contract_completion_ledger_status': world_contract_completion_result['reply']['card'].get('ledger_status'),
    'client_app_route_event_id': client_app_route_result['reply']['event_id'],
    'client_app_route_task_graph_count_after_contract': client_app_route_result['reply']['card'].get('route_task_graph_count'),
    'client_app_route_next_task_id_after_contract': client_app_route_result['reply']['card'].get('route_next_task_id'),
    'client_app_route_next_action_label_after_contract': client_app_route_result['reply']['card'].get('route_next_action_label'),
    'client_app_route_next_panel_id_after_contract': client_app_route_result['reply']['card'].get('route_next_panel_id'),
    'client_app_route_next_location_id_after_contract': client_app_route_result['reply']['card'].get('route_next_location_id'),
    'client_app_route_next_stage_summary_after_contract': client_app_route_result['reply']['card'].get('route_next_stage_summary'),
    'client_app_route_next_node_id_after_contract': client_app_route_result['reply']['card'].get('route_next_node_id'),
    'client_app_route_next_command_hint_after_contract': client_app_route_result['reply']['card'].get('route_next_command_hint'),
    'client_app_route_next_outcome_summary_after_contract': client_app_route_result['reply']['card'].get('route_next_outcome_summary'),
    'client_app_route_next_feedback_focus_after_contract': client_app_route_result['reply']['card'].get('route_next_feedback_focus'),
    'client_app_route_next_opportunity_hint_after_contract': client_app_route_result['reply']['card'].get('route_next_opportunity_hint'),
    'client_app_route_next_opportunity_kind_after_contract': client_app_route_result['reply']['card'].get('route_next_opportunity_kind'),
    'client_app_route_next_opportunity_playbook_after_contract': client_app_route_result['reply']['card'].get('route_next_opportunity_playbook'),
    'client_app_route_next_opportunity_command_after_contract': client_app_route_result['reply']['card'].get('route_next_opportunity_command'),
    'client_app_route_next_opportunity_node_id_after_contract': client_app_route_result['reply']['card'].get('route_next_opportunity_node_id'),
    'client_app_route_next_opportunity_action_label_after_contract': client_app_route_result['reply']['card'].get('route_next_opportunity_action_label'),
    'client_app_route_next_opportunity_panel_id_after_contract': client_app_route_result['reply']['card'].get('route_next_opportunity_panel_id'),
    'client_app_route_next_opportunity_textarea_id_after_contract': client_app_route_result['reply']['card'].get('route_next_opportunity_textarea_id'),
    'client_app_route_next_opportunity_target_node_id_after_contract': client_app_route_result['reply']['card'].get('route_next_opportunity_target_node_id'),
    'world_map_route_event_id': world_map_route_result['reply']['event_id'],
    'world_map_route_task_graph_count_after_contract': world_map_route_result['reply']['card'].get('route_task_graph_count'),
    'world_map_route_next_task_id_after_contract': world_map_route_result['reply']['card'].get('route_next_task_id'),
    'world_map_route_next_action_label_after_contract': world_map_route_result['reply']['card'].get('route_next_action_label'),
    'world_map_route_next_panel_id_after_contract': world_map_route_result['reply']['card'].get('route_next_panel_id'),
    'world_map_route_next_location_id_after_contract': world_map_route_result['reply']['card'].get('route_next_location_id'),
    'world_map_route_next_stage_summary_after_contract': world_map_route_result['reply']['card'].get('route_next_stage_summary'),
    'world_map_route_next_node_id_after_contract': world_map_route_result['reply']['card'].get('route_next_node_id'),
    'world_map_route_next_command_hint_after_contract': world_map_route_result['reply']['card'].get('route_next_command_hint'),
    'world_map_route_next_outcome_summary_after_contract': world_map_route_result['reply']['card'].get('route_next_outcome_summary'),
    'world_map_route_next_feedback_focus_after_contract': world_map_route_result['reply']['card'].get('route_next_feedback_focus'),
    'world_map_route_next_opportunity_hint_after_contract': world_map_route_result['reply']['card'].get('route_next_opportunity_hint'),
    'world_map_route_next_opportunity_kind_after_contract': world_map_route_result['reply']['card'].get('route_next_opportunity_kind'),
    'world_map_route_next_opportunity_playbook_after_contract': world_map_route_result['reply']['card'].get('route_next_opportunity_playbook'),
    'world_map_route_next_opportunity_command_after_contract': world_map_route_result['reply']['card'].get('route_next_opportunity_command'),
    'world_map_route_next_opportunity_node_id_after_contract': world_map_route_result['reply']['card'].get('route_next_opportunity_node_id'),
    'world_map_route_next_opportunity_action_label_after_contract': world_map_route_result['reply']['card'].get('route_next_opportunity_action_label'),
    'world_map_route_next_opportunity_panel_id_after_contract': world_map_route_result['reply']['card'].get('route_next_opportunity_panel_id'),
    'world_map_route_next_opportunity_textarea_id_after_contract': world_map_route_result['reply']['card'].get('route_next_opportunity_textarea_id'),
    'world_map_route_next_opportunity_target_node_id_after_contract': world_map_route_result['reply']['card'].get('route_next_opportunity_target_node_id'),
    'world_assets_event_id': world_assets_result['reply']['event_id'],
    'world_assets_route_task_graph_count': world_assets_result['reply']['card'].get('route_task_graph_count'),
    'world_assets_route_next_action_label': world_assets_result['reply']['card'].get('route_next_action_label'),
    'world_assets_route_next_panel_id': world_assets_result['reply']['card'].get('route_next_panel_id'),
    'world_assets_route_next_stage_summary': world_assets_result['reply']['card'].get('route_next_stage_summary'),
    'world_assets_route_next_node_id': world_assets_result['reply']['card'].get('route_next_node_id'),
    'world_asset_upgrade_event_id': world_asset_upgrade_result['reply']['event_id'],
    'world_asset_upgrade_delta': world_asset_upgrade_result['reply']['card'].get('value_delta'),
    'world_companies_event_id': world_companies_result['reply']['event_id'],
    'world_companies_route_task_graph_count': world_companies_result['reply']['card'].get('route_task_graph_count'),
    'world_companies_route_next_action_label': world_companies_result['reply']['card'].get('route_next_action_label'),
    'world_companies_route_next_panel_id': world_companies_result['reply']['card'].get('route_next_panel_id'),
    'world_companies_route_next_stage_summary': world_companies_result['reply']['card'].get('route_next_stage_summary'),
    'world_companies_route_next_node_id': world_companies_result['reply']['card'].get('route_next_node_id'),
    'world_company_event_id': world_company_result['reply']['event_id'],
    'world_company_id': world_company_result['reply']['card'].get('company_id'),
    'world_shops_event_id': world_shops_result['reply']['event_id'],
    'world_shops_route_task_graph_count': world_shops_result['reply']['card'].get('route_task_graph_count'),
    'world_shops_route_next_action_label': world_shops_result['reply']['card'].get('route_next_action_label'),
    'world_shops_route_next_panel_id': world_shops_result['reply']['card'].get('route_next_panel_id'),
    'world_shops_route_next_stage_summary': world_shops_result['reply']['card'].get('route_next_stage_summary'),
    'world_shops_route_next_node_id': world_shops_result['reply']['card'].get('route_next_node_id'),
    'world_listing_event_id': world_listing_result['reply']['event_id'],
    'world_listing_id': world_listing_result['reply']['card'].get('listing_id'),
    'world_listing_route_task_graph_count': world_listing_result['reply']['card'].get('route_task_graph_count'),
    'world_listing_route_next_action_label': world_listing_result['reply']['card'].get('route_next_action_label'),
    'world_listing_route_next_panel_id': world_listing_result['reply']['card'].get('route_next_panel_id'),
    'world_listing_route_next_stage_summary': world_listing_result['reply']['card'].get('route_next_stage_summary'),
    'world_listing_route_next_node_id': world_listing_result['reply']['card'].get('route_next_node_id'),
    'world_purchase_event_id': world_purchase_result['reply']['event_id'],
    'world_purchase_id': world_purchase_result['reply']['card'].get('purchase_id'),
    'world_work_order_id': world_purchase_result['reply']['card'].get('work_order_id'),
    'world_purchase_ledger_status': world_purchase_result['reply']['card'].get('ledger_status'),
    'world_purchase_buyer_ledger_status': world_purchase_result['reply']['card'].get('buyer_ledger_status'),
    'world_purchase_route_task_graph_count': world_purchase_result['reply']['card'].get('route_task_graph_count'),
    'world_purchase_route_next_action_label': world_purchase_result['reply']['card'].get('route_next_action_label'),
    'world_purchase_route_next_panel_id': world_purchase_result['reply']['card'].get('route_next_panel_id'),
    'world_purchase_route_next_stage_summary': world_purchase_result['reply']['card'].get('route_next_stage_summary'),
    'world_purchase_route_next_node_id': world_purchase_result['reply']['card'].get('route_next_node_id'),
    'world_work_delivery_event_id': world_work_delivery_result['reply']['event_id'],
    'world_work_delivery_id': world_work_delivery_result['reply']['card'].get('delivery_id'),
    'world_work_acceptance_event_id': world_work_acceptance_result['reply']['event_id'],
    'world_work_acceptance_id': world_work_acceptance_result['reply']['card'].get('acceptance_id'),
    'world_work_acceptance_buyer_consume_status': world_work_acceptance_result['reply']['card'].get('buyer_consume_status'),
    'world_work_rejection_event_id': world_work_rejection_result['reply']['event_id'],
    'world_work_rejection_id': world_work_rejection_result['reply']['card'].get('rejection_id'),
    'world_work_rejection_buyer_refund_status': world_work_rejection_result['reply']['card'].get('buyer_refund_status'),
    'world_work_rejection_route_next_opportunity_kind': world_work_rejection_result['reply']['card'].get('route_next_opportunity_kind'),
    'world_work_rejection_route_next_opportunity_command': world_work_rejection_result['reply']['card'].get('route_next_opportunity_command'),
    'world_work_rejection_route_next_opportunity_node_id': world_work_rejection_result['reply']['card'].get('route_next_opportunity_node_id'),
    'world_work_reopen_event_id': world_work_reopen_result['reply']['event_id'],
    'world_work_reopen_id': world_work_reopen_result['reply']['card'].get('reopen_id'),
    'world_work_reopen_buyer_reserve_status': world_work_reopen_result['reply']['card'].get('buyer_reopen_reserve_status'),
    'world_work_reopen_route_next_opportunity_kind': world_work_reopen_result['reply']['card'].get('route_next_opportunity_kind'),
    'world_work_reopen_route_next_opportunity_command': world_work_reopen_result['reply']['card'].get('route_next_opportunity_command'),
    'world_work_reopen_route_next_opportunity_node_id': world_work_reopen_result['reply']['card'].get('route_next_opportunity_node_id'),
    'world_work_reacceptance_buyer_consume_status': world_work_reacceptance_result['reply']['card'].get('buyer_consume_status'),
    'world_work_cancellation_event_id': world_work_cancellation_result['reply']['event_id'],
    'world_work_cancellation_id': world_work_cancellation_result['reply']['card'].get('cancellation_id'),
    'world_work_cancellation_buyer_refund_status': world_work_cancellation_result['reply']['card'].get('buyer_cancel_refund_status'),
    'world_work_cancellation_route_next_opportunity_kind': world_work_cancellation_result['reply']['card'].get('route_next_opportunity_kind'),
    'world_work_cancellation_route_next_opportunity_command': world_work_cancellation_result['reply']['card'].get('route_next_opportunity_command'),
    'world_work_cancellation_route_next_opportunity_node_id': world_work_cancellation_result['reply']['card'].get('route_next_opportunity_node_id'),
    'world_work_event_id': world_work_result['reply']['event_id'],
    'world_work_order_count': world_work_result['reply']['card'].get('work_order_count'),
    'world_work_delivery_count': world_work_result['reply']['card'].get('delivery_count'),
    'world_work_acceptance_count': world_work_result['reply']['card'].get('acceptance_count'),
    'world_work_rejection_count': world_work_result['reply']['card'].get('rejection_count'),
    'world_work_reopen_count': world_work_result['reply']['card'].get('reopen_count'),
    'world_work_cancellation_count': world_work_result['reply']['card'].get('cancellation_count'),
    'world_work_route_task_graph_count': world_work_result['reply']['card'].get('route_task_graph_count'),
    'world_work_route_next_action_label': world_work_result['reply']['card'].get('route_next_action_label'),
    'world_work_route_next_panel_id': world_work_result['reply']['card'].get('route_next_panel_id'),
    'world_work_route_next_stage_summary': world_work_result['reply']['card'].get('route_next_stage_summary'),
    'world_work_route_next_node_id': world_work_result['reply']['card'].get('route_next_node_id'),
    'world_factions_event_id': world_factions_result['reply']['event_id'],
    'world_faction_count': world_factions_result['reply']['card'].get('faction_count'),
    'world_faction_standing_count': world_factions_result['reply']['card'].get('standing_count'),
    'season_event_id': season_result['reply']['event_id'],
    'arena_event_id': arena_result['reply']['event_id'],
    'guilds_event_id': guilds_result['reply']['event_id'],
    'guild_join_event_id': guild_join_result['reply']['event_id'],
    'raids_event_id': raids_result['reply']['event_id'],
    'team_event_id': team_result['reply']['event_id'],
    'team_slot_count': team_result['reply']['card'].get('slot_count'),
    'raid_event_id': raid_result['reply']['event_id'],
    'raid_progress': raid_result['reply']['card'].get('progress_percent'),
    'draft_event_id': draft_result['reply']['event_id'],
    'join_event_id': join_result['reply']['event_id'],
    'league_battle_event_id': league_battle_result['reply']['event_id'],
    'league_battle_task_id': league_battle_result['reply']['card'].get('task_id'),
    'submit_event_id': submit_result['reply']['event_id'],
    'submit_score': submit_result['reply']['card'].get('score'),
    'submit_reward': submit_result['reply']['card'].get('reward_amount'),
    'submit_ledger_status': submit_result['reply']['card'].get('ledger_status'),
    'submit_ledger_entry_id': submit_result['reply']['card'].get('ledger_entry_id'),
    'submit_judge_status': submit_result['reply']['card'].get('judge_status'),
    'submit_score_event_count': submit_result['reply']['card'].get('score_event_count'),
    'after_submit_balance_event_id': after_submit_balance_result['reply']['event_id'],
    'after_submit_balance': after_submit_balance_result['reply']['card'].get('balance'),
    'profile_event_id': profile_result['reply']['event_id'],
    'progression_event_id': progression_result['reply']['event_id'],
    'progression_level': progression_result['reply']['card'].get('level'),
    'progression_successful_task_count': progression_result['reply']['card'].get('successful_task_count'),
    'skills_event_id': skills_result['reply']['event_id'],
    'unlocked_skill_count': skills_result['reply']['card'].get('unlocked_skill_count'),
    'tools_event_id': tools_result['reply']['event_id'],
    'unlocked_tool_count': tools_result['reply']['card'].get('unlocked_tool_count'),
    'skins_event_id': skins_result['reply']['event_id'],
    'unlocked_skin_count': skins_result['reply']['card'].get('unlocked_skin_count'),
    'rewards_event_id': rewards_result['reply']['event_id'],
    'inventory_event_id': inventory_result['reply']['event_id'],
    'inventory_item_count': inventory_result['reply']['card'].get('item_count'),
    'history_event_id': history_result['reply']['event_id'],
    'rank_event_id': rank_result['reply']['event_id'],
    'loadout_event_id': loadout_result['reply']['event_id'],
}, ensure_ascii=False, indent=2))
PY
