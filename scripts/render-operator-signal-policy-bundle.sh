#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

LIST_ONLY="false"
BUNDLES=()
PROFILES=()

usage() {
  cat <<'EOF'
Usage: scripts/render-operator-signal-policy-bundle.sh [--bundle <name> ...] [--profile <name> ...] [--list]

Render one or more repo-local operator-signal policy bundles as a single JSON array.

Supported bundle names:
  entry-identity      Consumer-entry identity governance routes
  monitoring-deploy   Monitoring deploy verdict / metadata routes
  baseline            Current repo-local starter bundle (entry-identity + monitoring-deploy)

Supported profile names:
  default             Alias for the current recommended starter bundle
  identity            Alias for entry-identity only
  deploy              Alias for monitoring-deploy only

Options:
  --bundle <name>     Add a bundle (repeatable). If omitted, defaults to profile=default.
  --profile <name>    Add a higher-level profile alias (repeatable)
  --list              Print supported bundles and profiles with backing files
  -h, --help          Show this help

Examples:
  ./scripts/render-operator-signal-policy-bundle.sh
  ./scripts/render-operator-signal-policy-bundle.sh --bundle baseline
  ./scripts/render-operator-signal-policy-bundle.sh --profile default
  ./scripts/render-operator-signal-policy-bundle.sh --bundle entry-identity --bundle monitoring-deploy
  OPERATOR_SIGNAL_NOTIFY_POLICY_JSON="$(./scripts/render-operator-signal-policy-bundle.sh --bundle baseline)" ./scripts/run-operator-signal-check.sh --compact
EOF
}

bundle_files() {
  local bundle="$1"
  case "$bundle" in
    entry-identity)
      printf '%s\n' "$SCRIPT_DIR/operator-signal-policy-entry-identity.example.json"
      ;;
    monitoring-deploy)
      printf '%s\n' "$SCRIPT_DIR/operator-signal-policy-monitoring-deploy.example.json"
      ;;
    baseline)
      printf '%s\n' \
        "$SCRIPT_DIR/operator-signal-policy-entry-identity.example.json" \
        "$SCRIPT_DIR/operator-signal-policy-monitoring-deploy.example.json"
      ;;
    *)
      echo "Error: unsupported bundle: $bundle (supported: entry-identity, monitoring-deploy, baseline)" >&2
      exit 2
      ;;
  esac
}

profile_bundles() {
  local profile="$1"
  case "$profile" in
    default)
      printf '%s\n' baseline
      ;;
    identity)
      printf '%s\n' entry-identity
      ;;
    deploy)
      printf '%s\n' monitoring-deploy
      ;;
    *)
      echo "Error: unsupported profile: $profile (supported: default, identity, deploy)" >&2
      exit 2
      ;;
  esac
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bundle)
      [[ $# -ge 2 ]] || { echo "Error: --bundle requires a name" >&2; exit 2; }
      BUNDLES+=("$2")
      shift 2
      ;;
    --profile)
      [[ $# -ge 2 ]] || { echo "Error: --profile requires a name" >&2; exit 2; }
      PROFILES+=("$2")
      shift 2
      ;;
    --list)
      LIST_ONLY="true"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ "$LIST_ONLY" == "true" ]]; then
  cat <<EOF
bundles:
  entry-identity
    - $SCRIPT_DIR/operator-signal-policy-entry-identity.example.json
  monitoring-deploy
    - $SCRIPT_DIR/operator-signal-policy-monitoring-deploy.example.json
  baseline
    - $SCRIPT_DIR/operator-signal-policy-entry-identity.example.json
    - $SCRIPT_DIR/operator-signal-policy-monitoring-deploy.example.json
profiles:
  default
    - bundle: baseline
  identity
    - bundle: entry-identity
  deploy
    - bundle: monitoring-deploy
EOF
  exit 0
fi

if [[ "${#BUNDLES[@]}" -eq 0 && "${#PROFILES[@]}" -eq 0 ]]; then
  PROFILES=(default)
fi

for profile in "${PROFILES[@]}"; do
  while IFS= read -r bundle; do
    [[ -n "$bundle" ]] || continue
    BUNDLES+=("$bundle")
  done < <(profile_bundles "$profile")
done

declare -A seen=()
files=()
for bundle in "${BUNDLES[@]}"; do
  while IFS= read -r file; do
    [[ -n "$file" ]] || continue
    if [[ ! -f "$file" ]]; then
      echo "Error: policy file not found for bundle '$bundle': $file" >&2
      exit 2
    fi
    if [[ -z "${seen[$file]:-}" ]]; then
      seen[$file]=1
      files+=("$file")
    fi
  done < <(bundle_files "$bundle")
done

if [[ "${#files[@]}" -eq 0 ]]; then
  echo "Error: no policy files resolved" >&2
  exit 2
fi

jq -cs add "${files[@]}"
