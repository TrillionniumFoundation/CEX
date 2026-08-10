#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
migration="$root/migrations/0004_agent_pairing_bridge.sql"
bridge="$root/src/agent_bridge.rs"
db="$root/src/db.rs"
ctl="$root/src/bin/paper-raid-accessctl.rs"
readme="$root/README.md"
tool_root=$(CDPATH= cd -- "$root/../.." && pwd)/tools/paper-raid-agent-bridge

for file in "$migration" "$bridge" "$db" "$ctl" "$readme" \
  "$tool_root/src/operations.mjs" "$tool_root/src/state.mjs"; do
  test -s "$file"
done

grep -Fq "expires_at <= created_at + interval '60 seconds'" "$migration"
grep -Fq 'At-most-60-second exact replay cache' "$migration"
grep -Fq 'DELETE FROM paper_raid_bff_agent_request_uses WHERE expires_at <= $1' "$bridge"
grep -Fq 'DELETE FROM paper_raid_bff_agent_request_uses WHERE expires_at <= now()' "$ctl"
grep -Fq 'Route reads,' "$bridge"
grep -Fq 'complete_agent_request still compare-checks exact bytes' "$bridge"
grep -Fq 'authoritative_bridge_binding(state, &identity, &mapping).await?' "$bridge"
grep -Fq 'AgentRequestUseState::Completed(status, body)' "$bridge"

grep -Fq 'last_pairing_grant_id UUID NOT NULL UNIQUE' "$migration"
grep -Fq 'last_pairing_grant_id=$1, agent_key_id=$2' "$bridge"
grep -Fq 'mapping_repair_owner_matches' "$bridge"
grep -Fq 'stableBindingId' "$tool_root/src/operations.mjs"
grep -Fq 'prepareBridgeStateForPairing' "$tool_root/src/operations.mjs"
grep -Fq 'issuedAtUnix: context.issued_at_unix' "$tool_root/src/operations.mjs"

if grep -Eqi '(login[_-]?key|cookie|csrf|authorization|bearer)[[:space:]]*:' \
  "$tool_root/example.config.json"; then
  echo "Agent Bridge config contains a player/session credential field" >&2
  exit 1
fi

echo "agent-bridge static boundary: PASS"
