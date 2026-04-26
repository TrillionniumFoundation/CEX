#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
BASE_URL="${CONSUMER_ENTRY_BASE_URL:-http://127.0.0.1:8090}"
SUMMARY_DIR="$ROOT_DIR/run/league-web"
mkdir -p "$SUMMARY_DIR"

python3 - <<'PY'
import json, os, pathlib, time, urllib.parse, urllib.request

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
for needle in ['Trillionnium League', 'Trillionnium World', 'Web Battle Console', 'Guild Halls', 'Battle Timeline', 'Submit Result']:
    assert needle in html, needle

world_status, world_html = get('/world')
assert world_status == 200, world_status
for needle in ['Trillionnium World', 'World Action Console', 'Reality Mirror Sandbox', 'Player Assets', 'Upgrade Asset', 'Companies / Shops', 'Launch Company', 'Shops / Listings', 'Publish Listing', 'World Contracts', 'Complete Contract']:
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
for needle in ['Oracle Scout', 'Forge Builder', 'Mirror Auditor', 'Top loot', 'settled', 'held_review', 'rubric_hidden']:
    assert needle in html_after, needle
world_status, world_html_after = get('/world')
assert world_status == 200, world_status
assert world_marker in world_html_after, world_marker

summary = {
    'ok': True,
    'checked_at_epoch': int(time.time()),
    'base_url': base,
    'actions': actions,
    'has_title': 'Trillionnium League' in html_after,
    'has_web_console': 'Web Battle Console' in html_after,
    'has_web_session': bool(session_cookie and csrf),
    'has_world_shell': 'World Action Console' in world_html_after,
    'has_world_action': world_marker in world_html_after,
    'has_world_contracts_panel': 'World Contracts' in world_html_after,
    'has_world_contract_completion_form': 'Complete Contract' in world_html_after,
    'has_world_asset_upgrade_form': 'Upgrade Asset' in world_html_after,
    'has_world_company_form': 'Launch Company' in world_html_after,
    'has_world_listing_form': 'Publish Listing' in world_html_after,
    'has_timeline': 'Battle Timeline' in html_after,
    'has_settled_reward': 'settled' in html_after,
    'has_held_review': 'held_review' in html_after,
    'has_hidden_judge': 'rubric_hidden' in html_after,
    'marker': marker,
}
path = summary_dir / f'web-e2e-summary-{int(time.time())}.json'
path.write_text(json.dumps(summary, ensure_ascii=False, indent=2))
print(json.dumps({'ok': True, 'summary': str(path), **summary}, ensure_ascii=False, indent=2))
PY
