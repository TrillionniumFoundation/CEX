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
    sent = req('PUT', f'/_matrix/client/v3/rooms/{room_enc}/send/m.room.message/{uuid.uuid4()}', alice, {
        'msgtype': 'm.text',
        'body': text,
    })
    request_ts = None
    found = None
    for _ in range(60):
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
    time.sleep(1.5)
    return {'sent': sent, 'reply': found}

marker = f'真房间闭环 E2E {int(time.time())}'
task_result = send_and_wait('/task ' + marker, 'task_status', lambda content, card: marker in (content.get('formatted_body') or ''))
task_id = task_result['reply']['card']['task_id']
status_result = send_and_wait('/status ' + task_id, 'task_status', lambda content, card: card.get('task_id') == task_id)
balance_result = send_and_wait('/balance', 'wallet_summary')
plans_result = send_and_wait('/plans', 'package_summary')
league_result = send_and_wait('/league', 'league_home')
world_result = send_and_wait('/world', 'trillionnium_world', lambda content, card: int(card.get('zone_count') or 0) >= 4)
world_action_result = send_and_wait('/world action 我要在镜像城市开一家 AI 设计公司，招募 Agent，服务真实客户。', 'trillionnium_world_action', lambda content, card: card.get('event_kind') in ('venture', 'craft', 'market', 'explore', 'recruit'))
craft_action_result = send_and_wait('/craft 建一个自动交付工坊，把客户需求转成可复用资产。', 'trillionnium_craft_action', lambda content, card: card.get('event_kind') == 'craft')
world_contract_result = send_and_wait('/contract 帮客户整理一个 AI 店铺启动方案，包含目标、证据、风险和验收标准。', 'trillionnium_world_contract', lambda content, card: card.get('event_kind') == 'contract' and bool(card.get('task_id')) and bool(card.get('contract_id')))
world_contract_id = world_contract_result['reply']['card'].get('contract_id')
world_contract_completion_result = send_and_wait('/complete ' + world_contract_id + ' 交付方案：包含 deliverable、evidence、risk review、acceptance standard、next step 和自检记录。', 'trillionnium_world_contract_completion', lambda content, card: card.get('contract_id') == world_contract_id and card.get('ledger_status') == 'settled' and 'hidden' in str(card.get('judge_status')))
world_assets_result = send_and_wait('/assets', 'trillionnium_world_assets', lambda content, card: int(card.get('asset_count') or 0) >= 1)
world_asset_upgrade_result = send_and_wait('/upgrade latest Asset upgrade deliverable: service package, evidence template, risk control checklist, operating cadence, acceptance standard, next customer path, and self-review notes.', 'trillionnium_world_asset_upgrade', lambda content, card: int(card.get('asset_level') or 0) >= 1 and int(card.get('value_delta') or 0) >= 1)
world_companies_result = send_and_wait('/companies', 'trillionnium_world_companies')
world_company_result = send_and_wait('/company latest Company launch plan: offer, target customer, revenue path, operating loop, evidence, risk controls, acceptance standard, and next sale.', 'trillionnium_world_company_created', lambda content, card: int(card.get('company_level') or 0) >= 1 and int(card.get('revenue_score') or 0) >= 1)
world_shops_result = send_and_wait('/shops', 'trillionnium_world_shops', lambda content, card: int(card.get('shop_count') or 0) >= 1 and int(card.get('listing_count') or 0) >= 1)
world_listing_result = send_and_wait('/sell latest Service listing: AI design delivery package with scope, price logic, evidence package, customer deliverable, risk controls, self-review, acceptance standard, revision policy, and next action.', 'trillionnium_world_listing_created', lambda content, card: int(card.get('price_credits') or 0) >= 1 and int(card.get('quality_score') or 0) >= 1 and card.get('status') == 'listed')
world_purchase_result = send_and_wait('/buy latest Purchase brief: hire this AI design package, define deliverable, evidence package, acceptance standard, risk controls, and next action.', 'trillionnium_world_listing_purchase', lambda content, card: int(card.get('price_credits') or 0) >= 1 and bool(card.get('work_order_id')) and card.get('ledger_status') == 'settled')
world_work_result = send_and_wait('/work', 'trillionnium_world_commerce', lambda content, card: int(card.get('work_order_count') or 0) >= 1)
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
    'league_reply': league_result['reply'],
    'world_reply': world_result['reply'],
    'world_action_reply': world_action_result['reply'],
    'craft_action_reply': craft_action_result['reply'],
    'world_contract_reply': world_contract_result['reply'],
    'world_contract_completion_reply': world_contract_completion_result['reply'],
    'world_assets_reply': world_assets_result['reply'],
    'world_asset_upgrade_reply': world_asset_upgrade_result['reply'],
    'world_companies_reply': world_companies_result['reply'],
    'world_company_reply': world_company_result['reply'],
    'world_shops_reply': world_shops_result['reply'],
    'world_listing_reply': world_listing_result['reply'],
    'world_purchase_reply': world_purchase_result['reply'],
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
    'rewards_reply': rewards_result['reply'],
    'inventory_reply': inventory_result['reply'],
    'history_reply': history_result['reply'],
    'rank_reply': rank_result['reply'],
    'loadout_reply': loadout_result['reply'],
}
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
    'league_event_id': league_result['reply']['event_id'],
    'world_event_id': world_result['reply']['event_id'],
    'world_action_event_id': world_action_result['reply']['event_id'],
    'world_action_kind': world_action_result['reply']['card'].get('event_kind'),
    'craft_action_event_id': craft_action_result['reply']['event_id'],
    'craft_action_kind': craft_action_result['reply']['card'].get('event_kind'),
    'world_contract_event_id': world_contract_result['reply']['event_id'],
    'world_contract_task_id': world_contract_result['reply']['card'].get('task_id'),
    'world_contract_id': world_contract_id,
    'world_contract_completion_event_id': world_contract_completion_result['reply']['event_id'],
    'world_contract_completion_ledger_status': world_contract_completion_result['reply']['card'].get('ledger_status'),
    'world_assets_event_id': world_assets_result['reply']['event_id'],
    'world_asset_upgrade_event_id': world_asset_upgrade_result['reply']['event_id'],
    'world_asset_upgrade_delta': world_asset_upgrade_result['reply']['card'].get('value_delta'),
    'world_companies_event_id': world_companies_result['reply']['event_id'],
    'world_company_event_id': world_company_result['reply']['event_id'],
    'world_company_id': world_company_result['reply']['card'].get('company_id'),
    'world_shops_event_id': world_shops_result['reply']['event_id'],
    'world_listing_event_id': world_listing_result['reply']['event_id'],
    'world_listing_id': world_listing_result['reply']['card'].get('listing_id'),
    'world_purchase_event_id': world_purchase_result['reply']['event_id'],
    'world_purchase_id': world_purchase_result['reply']['card'].get('purchase_id'),
    'world_work_order_id': world_purchase_result['reply']['card'].get('work_order_id'),
    'world_purchase_ledger_status': world_purchase_result['reply']['card'].get('ledger_status'),
    'world_work_event_id': world_work_result['reply']['event_id'],
    'world_work_order_count': world_work_result['reply']['card'].get('work_order_count'),
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
    'rewards_event_id': rewards_result['reply']['event_id'],
    'inventory_event_id': inventory_result['reply']['event_id'],
    'inventory_item_count': inventory_result['reply']['card'].get('item_count'),
    'history_event_id': history_result['reply']['event_id'],
    'rank_event_id': rank_result['reply']['event_id'],
    'loadout_event_id': loadout_result['reply']['event_id'],
}, ensure_ascii=False, indent=2))
PY
