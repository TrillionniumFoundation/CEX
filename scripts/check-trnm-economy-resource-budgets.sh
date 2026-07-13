#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"

require_line() {
  local file="$1" line="$2"
  if ! grep -Fqx -- "$line" "$file"; then
    echo "missing resource budget in ${file#$ROOT_DIR/}: $line" >&2
    exit 1
  fi
}

ledger_unit="$ROOT_DIR/deploy/systemd/cex-trnm-ledger.service"
consumer_unit="$ROOT_DIR/deploy/systemd/cex-trnm-consumer.service"
for setting in CPUAccounting=true CPUWeight=200 CPUQuota=100% \
  MemoryAccounting=true MemoryHigh=256M MemoryMax=384M MemorySwapMax=128M \
  IOAccounting=true IOWeight=200 TasksAccounting=true TasksMax=256; do
  require_line "$ledger_unit" "$setting"
done
for setting in CPUAccounting=true CPUWeight=100 CPUQuota=100% \
  MemoryAccounting=true MemoryHigh=384M MemoryMax=512M MemorySwapMax=128M \
  IOAccounting=true IOWeight=100 TasksAccounting=true TasksMax=256; do
  require_line "$consumer_unit" "$setting"
done

compose_json="$(docker compose -f "$ROOT_DIR/docker-compose.yml" config --format json)"
jq -e '
  .services.postgres.cpus == 2
  and .services.postgres.mem_reservation == "536870912"
  and .services.postgres.mem_limit == "1610612736"
  and .services.postgres.memswap_limit == "2147483648"
  and .services.postgres.pids_limit == 256
  and (.services.postgres.command | index("max_connections=50")) != null
' >/dev/null <<<"$compose_json"

rg -q 'LEDGER_DATABASE_MAX_CONNECTIONS' \
  "$ROOT_DIR/scripts/run-trnm-economy-service.sh" \
  "$ROOT_DIR/services/ledger-service/src/repository/postgres.rs"

installed=false
if [[ "${TRNM_REQUIRE_INSTALLED_RESOURCE_BUDGETS:-0}" == 1 ]]; then
  installed=true
  [[ "$(systemctl --user show cex-trnm-ledger.service -p CPUQuotaPerSecUSec --value)" == 1s ]]
  [[ "$(systemctl --user show cex-trnm-ledger.service -p MemoryHigh --value)" == 268435456 ]]
  [[ "$(systemctl --user show cex-trnm-ledger.service -p MemoryMax --value)" == 402653184 ]]
  [[ "$(systemctl --user show cex-trnm-consumer.service -p CPUQuotaPerSecUSec --value)" == 1s ]]
  [[ "$(systemctl --user show cex-trnm-consumer.service -p MemoryHigh --value)" == 402653184 ]]
  [[ "$(systemctl --user show cex-trnm-consumer.service -p MemoryMax --value)" == 536870912 ]]
  postgres_container_id="$(
    sudo docker compose --project-directory "$ROOT_DIR" \
      -f "$ROOT_DIR/docker-compose.yml" ps -q postgres
  )"
  [[ -n "$postgres_container_id" ]]
  inspect="$(sudo docker inspect "$postgres_container_id")"
  jq -e '.[0].HostConfig.Memory == 1610612736
    and .[0].HostConfig.MemoryReservation == 536870912
    and .[0].HostConfig.MemorySwap == 2147483648
    and .[0].HostConfig.NanoCpus == 2000000000
    and .[0].HostConfig.PidsLimit == 256' >/dev/null <<<"$inspect"
fi

jq -n --argjson installed "$installed" \
  '{status:"passed",ledger_pool_max:8,postgres_max_connections:50,
    postgres_memory_max_mib:1536,postgres_cpu_cores:2,
    systemd_unit_budgets:true,installed_runtime_verified:$installed}'
