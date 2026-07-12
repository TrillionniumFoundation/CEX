#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"

for binary in ledger-service consumer-entry-api; do
  if [[ ! -x "$CEX_PROJECT_ROOT/target/release/$binary" ]]; then
    echo "missing release binary: target/release/$binary" >&2
    echo "run: cargo build --release -p ledger-service -p consumer-entry-api" >&2
    exit 1
  fi
done

if cex_can_use_docker_postgres; then
  cex_docker update --restart unless-stopped "$CEX_POSTGRES_CONTAINER_NAME" >/dev/null
fi

mkdir -p "$HOME/.config/systemd/user"
install -m 600 \
  "$CEX_PROJECT_ROOT/deploy/systemd/cex-trnm-ledger.service" \
  "$HOME/.config/systemd/user/cex-trnm-ledger.service"
install -m 600 \
  "$CEX_PROJECT_ROOT/deploy/systemd/cex-trnm-consumer.service" \
  "$HOME/.config/systemd/user/cex-trnm-consumer.service"
install -m 600 \
  "$CEX_PROJECT_ROOT/deploy/systemd/cex-trnm-economy-maintenance.service" \
  "$HOME/.config/systemd/user/cex-trnm-economy-maintenance.service"
install -m 600 \
  "$CEX_PROJECT_ROOT/deploy/systemd/cex-trnm-economy-maintenance.timer" \
  "$HOME/.config/systemd/user/cex-trnm-economy-maintenance.timer"

systemctl --user daemon-reload
systemctl --user enable cex-trnm-ledger.service cex-trnm-consumer.service
systemctl --user enable --now cex-trnm-economy-maintenance.timer
systemctl --user restart cex-trnm-ledger.service cex-trnm-consumer.service
echo "installed persistent TRNM economy services (ledger 7002, consumer 8090, maintenance timer)"
