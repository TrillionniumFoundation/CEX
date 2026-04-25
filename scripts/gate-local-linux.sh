#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

SERVICE_LOCAL_ONLY=0
SKIP_DB_BOOTSTRAP=0
SKIP_WORKSPACE=0

usage() {
  cat <<'EOF'
Usage: scripts/gate-local-linux.sh [--service-local-only] [--skip-db-bootstrap] [--skip-workspace]

Linux equivalent of the Windows full gate.

Options:
  --service-local-only  run cargo test --workspace only
  --skip-db-bootstrap   skip migrations + local-dev seed (use when DB is already provisioned)
  --skip-workspace      skip cargo test --workspace and run runtime suites only
  -h, --help            show this help
EOF
}

while (($#)); do
  case "$1" in
    --service-local-only)
      SERVICE_LOCAL_ONLY=1
      ;;
    --skip-db-bootstrap)
      SKIP_DB_BOOTSTRAP=1
      ;;
    --skip-workspace)
      SKIP_WORKSPACE=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown arg: $1" >&2
      usage >&2
      exit 64
      ;;
  esac
  shift
done

PROJECT_ROOT="$CEX_PROJECT_ROOT"
GATE_DIR="${CEX_LINUX_GATE_DIR:-$PROJECT_ROOT/run/linux-gate}"
FAKEBIN_DIR="$GATE_DIR/fakebin"
mkdir -p "$FAKEBIN_DIR"

export PATH="$FAKEBIN_DIR:$PATH"
export CEX_LINUX_SKIP_SEED_LOCAL_DEV=1

cat > "$FAKEBIN_DIR/powershell" <<EOF
#!/usr/bin/env bash
set -euo pipefail
file=''
for ((i=1; i<=\$#; i++)); do
  if [[ "\${!i}" == "-File" ]]; then
    j=\$((i+1))
    file="\${!j:-}"
    break
  fi
done
base="\$(basename "\${file:-}")"
case "\$base" in
  start-local-runtime-detached.ps1)
    exec bash "$SCRIPT_DIR/runtime-manager-linux.sh" restart
    ;;
  stop-local-runtime.ps1)
    exec bash "$SCRIPT_DIR/runtime-manager-linux.sh" stop
    ;;
  status-local-runtime.ps1)
    exec bash "$SCRIPT_DIR/runtime-manager-linux.sh" status
    ;;
  seed-local-dev.ps1)
    if [[ "\${CEX_LINUX_SKIP_SEED_LOCAL_DEV:-0}" == "1" ]]; then
      exit 0
    fi
    exec bash "$SCRIPT_DIR/seed-local-dev.sh"
    ;;
  *)
    echo "fake powershell cannot handle script: \${file:-<none>}" >&2
    exit 64
    ;;
esac
EOF
chmod +x "$FAKEBIN_DIR/powershell"

cleanup() {
  rm -f "$FAKEBIN_DIR/powershell"
}
trap cleanup EXIT

cex_require_cmd cargo curl

if [[ "$SKIP_DB_BOOTSTRAP" -eq 0 ]]; then
  cex_wait_postgres
  cex_apply_migrations
  cex_seed_local_dev
fi

if [[ "$SKIP_WORKSPACE" -eq 0 ]]; then
  echo '==> cargo test --workspace'
  (cd "$PROJECT_ROOT" && cargo test --workspace)
fi

if [[ "$SERVICE_LOCAL_ONLY" -eq 1 ]]; then
  echo '==> service-local-only complete'
  exit 0
fi

echo '==> restart runtime (linux)'
bash "$SCRIPT_DIR/runtime-manager-linux.sh" restart

echo '==> cargo test -p audit-service --test runtime_blackbox -- --ignored --test-threads=1'
(cd "$PROJECT_ROOT" && cargo test -p audit-service --test runtime_blackbox -- --ignored --test-threads=1)

echo '==> cargo test -p identity-service --test runtime_blackbox -- --ignored --test-threads=1'
(cd "$PROJECT_ROOT" && cargo test -p identity-service --test runtime_blackbox -- --ignored --test-threads=1)

echo '==> cargo test -p gateway-service --test runtime_blackbox -- --ignored --test-threads=1'
(cd "$PROJECT_ROOT" && cargo test -p gateway-service --test runtime_blackbox -- --ignored --test-threads=1)

echo '==> cargo test -p gateway-service --test runtime_approval_probe -- --ignored --test-threads=1'
(cd "$PROJECT_ROOT" && cargo test -p gateway-service --test runtime_approval_probe -- --ignored --test-threads=1)

echo '==> runtime status after linux gate'
bash "$SCRIPT_DIR/runtime-manager-linux.sh" status

echo '==> linux gate complete'
