#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

STATE_DIR="${OPENCLAW_STATE_DIR:-$CEX_PROJECT_ROOT/run/openclaw-cex}"
CONFIG_PATH="${OPENCLAW_CONFIG_PATH:-$STATE_DIR/openclaw.json}"
AGENT_ID="${OPENCLAW_CEX_AGENT_ID:-cex}"
AGENT_DIR="${OPENCLAW_AGENT_DIR:-$STATE_DIR/agents/$AGENT_ID/agent}"
WORKSPACE_DIR="${OPENCLAW_CEX_WORKSPACE:-$CEX_PROJECT_ROOT}"
SOURCE_STATE_DIR="${OPENCLAW_SOURCE_STATE_DIR:-$HOME/.openclaw}"
SOURCE_CONFIG_PATH="${OPENCLAW_SOURCE_CONFIG_PATH:-$SOURCE_STATE_DIR/openclaw.json}"
SOURCE_AGENT_DIR="${OPENCLAW_SOURCE_AGENT_DIR:-$SOURCE_STATE_DIR/agents/main/agent}"
PRIMARY_MODEL="${OPENCLAW_CEX_PRIMARY_MODEL:-}"
EXTRA_MODELS="${OPENCLAW_CEX_EXTRA_MODELS:-}"

if [[ -n "${GEMINI_API_KEY:-}" ]]; then
  if [[ -n "$EXTRA_MODELS" ]]; then
    EXTRA_MODELS="$EXTRA_MODELS,google/gemini-2.5-flash"
  else
    EXTRA_MODELS="google/gemini-2.5-flash"
  fi
fi

usage() {
  cat <<EOF
Usage: ./scripts/bootstrap-openclaw-cex.sh [options]

Create a repo-local isolated OpenClaw state for CEX under run/openclaw-cex/.
It copies local auth/model state from an existing OpenClaw install, writes an
isolated config, and leaves runtime-manager-linux.sh able to auto-detect it.

Options:
  --state-dir <dir>          Target isolated OpenClaw state dir (default: $STATE_DIR)
  --config-path <path>       Target config path (default: $CONFIG_PATH)
  --agent-id <id>            Dedicated agent id inside isolated config (default: $AGENT_ID)
  --agent-dir <dir>          Dedicated agent dir (default: $AGENT_DIR)
  --workspace <dir>          Agent workspace (default: $WORKSPACE_DIR)
  --source-config <path>     Source OpenClaw config (default: $SOURCE_CONFIG_PATH)
  --source-agent-dir <dir>   Source OpenClaw agent dir (default: $SOURCE_AGENT_DIR)
  --primary-model <ref>      Override primary model in isolated config
  --extra-models <csv>       Additional provider/model refs to allow (default: OPENCLAW_CEX_EXTRA_MODELS plus google/gemini-2.5-flash when GEMINI_API_KEY exists)
  -h, --help                 Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --state-dir)
      STATE_DIR="$2"
      shift 2
      ;;
    --config-path)
      CONFIG_PATH="$2"
      shift 2
      ;;
    --agent-id)
      AGENT_ID="$2"
      shift 2
      ;;
    --agent-dir)
      AGENT_DIR="$2"
      shift 2
      ;;
    --workspace)
      WORKSPACE_DIR="$2"
      shift 2
      ;;
    --source-config)
      SOURCE_CONFIG_PATH="$2"
      shift 2
      ;;
    --source-agent-dir)
      SOURCE_AGENT_DIR="$2"
      shift 2
      ;;
    --primary-model)
      PRIMARY_MODEL="$2"
      shift 2
      ;;
    --extra-models)
      EXTRA_MODELS="$2"
      shift 2
      ;;
    -h|--help|help)
      usage
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      usage >&2
      exit 64
      ;;
  esac
done

mkdir -p "$STATE_DIR" "$AGENT_DIR"

python3 - <<'PY' "$SOURCE_CONFIG_PATH" "$SOURCE_AGENT_DIR" "$CONFIG_PATH" "$AGENT_DIR" "$AGENT_ID" "$WORKSPACE_DIR" "$PRIMARY_MODEL" "$EXTRA_MODELS"
import json
import shutil
import sys
from pathlib import Path

source_config_path = Path(sys.argv[1]).expanduser()
source_agent_dir = Path(sys.argv[2]).expanduser()
config_path = Path(sys.argv[3]).expanduser()
agent_dir = Path(sys.argv[4]).expanduser()
agent_id = sys.argv[5].strip() or 'cex'
workspace_dir = sys.argv[6].strip()
primary_override = sys.argv[7].strip()
extra_models_raw = sys.argv[8].strip()

agent_dir.mkdir(parents=True, exist_ok=True)
config_path.parent.mkdir(parents=True, exist_ok=True)

source_cfg = {}
if source_config_path.exists():
    source_cfg = json.loads(source_config_path.read_text())

source_defaults = ((source_cfg.get('agents') or {}).get('defaults') or {})
source_model_selection = source_defaults.get('model') or {}
if isinstance(source_model_selection, str):
    source_model_selection = {'primary': source_model_selection}
source_catalog = {'providers': {}}
source_models_path = source_agent_dir / 'models.json'
if source_models_path.exists():
    source_catalog = json.loads(source_models_path.read_text())

for filename in ('auth-profiles.json', 'auth-state.json', 'models.json'):
    src = source_agent_dir / filename
    if src.exists():
        shutil.copy2(src, agent_dir / filename)


def canonical_provider(provider: str, api: str | None = None) -> str:
    provider = (provider or '').strip()
    api = (api or '').strip()
    if provider == 'codex':
        return 'openai-codex'
    if provider == 'minimax-cn':
        return 'minimax'
    if api == 'openai-codex-responses' and provider != 'openai-codex':
        return 'openai-codex'
    return provider


def canonical_model_ref(raw: str) -> str | None:
    raw = (raw or '').strip()
    if not raw or '/' not in raw:
        return None
    provider, model = raw.split('/', 1)
    provider = canonical_provider(provider)
    model = model.strip()
    if not provider or not model:
        return None
    return f'{provider}/{model}'


allow_models: list[str] = []
seen: set[str] = set()


def add_allowed(raw: str | None):
    ref = canonical_model_ref(raw or '')
    if not ref or ref in seen:
        return
    seen.add(ref)
    allow_models.append(ref)


primary = primary_override or (source_model_selection.get('primary') or '').strip() or 'openai-codex/gpt-5.5'
add_allowed(primary)

fallbacks: list[str] = []
for raw in source_model_selection.get('fallbacks') or []:
    ref = canonical_model_ref(raw)
    if not ref or ref == primary or ref in fallbacks:
        continue
    fallbacks.append(ref)
    add_allowed(ref)

for raw in ((source_defaults.get('models') or {}).keys()):
    add_allowed(raw)

for source_provider, provider_cfg in ((source_catalog.get('providers') or {}).items()):
    api = (provider_cfg or {}).get('api')
    provider = canonical_provider(source_provider, api)
    for model in (provider_cfg or {}).get('models') or []:
        model_id = (model or {}).get('id')
        if not model_id:
            continue
        add_allowed(f'{provider}/{model_id}')

for raw in extra_models_raw.replace('\n', ',').split(','):
    add_allowed(raw)

isolated_cfg = {
    'models': source_cfg.get('models') or {},
    'agents': {
        'defaults': {
            'workspace': workspace_dir,
            'model': {
                'primary': primary,
                'fallbacks': fallbacks,
            },
            'models': {key: {} for key in allow_models},
            'thinkingDefault': source_defaults.get('thinkingDefault', 'xhigh'),
        },
        'list': [
            {
                'id': agent_id,
                'name': 'cex',
                'default': True,
                'workspace': workspace_dir,
                'agentDir': str(agent_dir),
                'model': primary,
            }
        ],
    },
}

config_path.write_text(json.dumps(isolated_cfg, ensure_ascii=False, indent=2) + '\n')

summary = {
    'config_path': str(config_path),
    'agent_dir': str(agent_dir),
    'primary_model': primary,
    'fallbacks': fallbacks,
    'allowed_count': len(allow_models),
}
print(json.dumps(summary, ensure_ascii=False, indent=2))
PY

cat <<EOF

Isolated OpenClaw scope is ready.

runtime-manager-linux.sh will auto-detect this repo-local scope on the next start/restart.
If you want to export it manually, use:
  export OPENCLAW_STATE_DIR="$STATE_DIR"
  export OPENCLAW_CONFIG_PATH="$CONFIG_PATH"
  export OPENCLAW_AGENT_DIR="$AGENT_DIR"
  export CAPABILITY_OPENCLAW_MODELS_JSON_PATH="$AGENT_DIR/models.json"
EOF
