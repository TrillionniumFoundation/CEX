#!/usr/bin/env bash
set -euo pipefail

service_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)

for pattern in \
  "League""State" \
  "/bat""tle" \
  "forward_to_""cex_task" \
  "Alice"" fallback" \
  "keyword""_score" \
  "NAKAMA_""OPERATOR_TOKEN" \
  "HEPTA_""SERVICE_TOKEN" \
  "NAKAMA_""CONTROL_SIGNING_SEED"
do
  if rg -n --fixed-strings "$pattern" \
    "$service_root/src" \
    "$service_root/migrations" \
    "$service_root/deploy" \
    "$service_root/Dockerfile" \
    "$service_root/Cargo.toml"
  then
    echo "forbidden authority pattern detected" >&2
    exit 1
  fi
done

if rg -n 'std::process::Command|tokio::process::Command' "$service_root/src"; then
  echo "production shell-out is forbidden" >&2
  exit 1
fi

echo "paper-raid-bff boundary scan: ok"
