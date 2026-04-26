#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

python3 - <<'PY'
import ast
import json
import os
import pathlib
import re
import urllib.request

root = pathlib.Path.cwd()
env_path = pathlib.Path(os.environ.get('CEX_ENV_FILE', 'run/local-production/.env'))
base = os.environ.get('CONSUMER_ENTRY_BASE_URL', 'http://127.0.0.1:8090').rstrip('/')

def env_value(name, default=None):
    if os.environ.get(name):
        return os.environ[name]
    path = root / env_path if not env_path.is_absolute() else env_path
    if path.exists():
        for raw in path.read_text().splitlines():
            line = raw.strip()
            if not line or line.startswith('#') or '=' not in line:
                continue
            key, value = line.split('=', 1)
            if key.strip() == name:
                return value.strip().strip('"').strip("'")
    return default

snapshot_path = pathlib.Path(env_value(
    'CONSUMER_ENTRY_LEAGUE_SQL_SNAPSHOT_PATH',
    'run/linux-runtime/entry-config/league-state-snapshot.sql',
))
if not snapshot_path.is_absolute():
    snapshot_path = root / snapshot_path
assert snapshot_path.exists(), f'missing SQL snapshot: {snapshot_path}'
raw = snapshot_path.read_text()
assert 'league_state_snapshots' in raw, 'snapshot table insert missing'
assert 'consumer_entry_json_v1' in raw, 'snapshot kind missing'
assert 'sha256:' in raw, 'state hash missing'
match = re.search(r"values \('consumer_entry_json_v1', '([^']+)', '(.*)'::jsonb\);", raw, re.S)
assert match, 'snapshot insert format changed'
state_hash, sql_json = match.groups()
json_text = sql_json.replace("''", "'")
state = json.loads(json_text)
for key in ['matches', 'players_by_matrix_user', 'entries', 'submissions', 'rewards']:
    assert key in state, f'missing state key {key}'
assert state['matches'], 'expected seeded league matches'

token = env_value('CONSUMER_ENTRY_INGRESS_TOKEN')
endpoint = None
if token:
    req = urllib.request.Request(base + '/v1/league/state/snapshot', headers={'x-entry-token': token})
    with urllib.request.urlopen(req, timeout=20) as resp:
        endpoint = json.loads(resp.read().decode())
    assert endpoint.get('kind') == 'league_state_snapshot', endpoint
    assert endpoint.get('state_hash', '').startswith('sha256:'), endpoint
    assert endpoint.get('sql_snapshot_path_configured') is True, endpoint

summary = {
    'ok': True,
    'snapshot_path': str(snapshot_path),
    'state_hash': state_hash,
    'matches': len(state.get('matches') or {}),
    'players': len(state.get('players_by_matrix_user') or {}),
    'submissions': len(state.get('submissions') or {}),
    'rewards': len(state.get('rewards') or []),
    'endpoint_checked': endpoint is not None,
}
print(json.dumps(summary, ensure_ascii=False, indent=2))
PY
