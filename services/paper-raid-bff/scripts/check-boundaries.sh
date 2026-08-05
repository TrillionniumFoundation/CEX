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

browser="$service_root/src/browser.js"
html="$service_root/src/html.rs"
if rg -n 'localStorage|sessionStorage|indexedDB|\.style' "$browser" || \
  rg -n '<input[^>]+name=\\?"agent_(private_key|seed|mnemonic)' "$html"
then
  echo "browser persistence or external Agent secret input detected" >&2
  exit 1
fi

for required in \
  'human-key-create-form' \
  'human-key-register-form' \
  'forget_current_in_memory_key_before_generating_another' \
  'Registration may already be committed' \
  'window.location.assign("/league/start")' \
  'agent_proof_nonce_must_equal_idempotency_key' \
  'sendCommand("create_agent_binding", null, null, payload)'
do
  if ! rg -q --fixed-strings "$required" "$browser" "$html"; then
    echo "required browser onboarding boundary is missing: $required" >&2
    exit 1
  fi
done

node "$service_root/scripts/check-browser-crypto.mjs"

echo "paper-raid-bff boundary scan: ok"
