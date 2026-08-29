#!/usr/bin/env bash
set -euo pipefail

service_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
domain="$service_root/src/quick_raid.rs"
http="$service_root/src/quick_raid_http.rs"
html="$service_root/src/html.rs"
browser="$service_root/src/browser.js"
migration="$service_root/migrations/0011_quick_raid.sql"
library="$service_root/src/lib.rs"
app="$service_root/src/app.rs"
db="$service_root/src/db.rs"

for file in "$domain" "$http" "$html" "$browser" "$migration" "$library" "$app" "$db"; do
  [[ -f "$file" ]] || { echo "Quick Raid boundary file is absent: $file" >&2; exit 1; }
done

for required in \
  'QUICK_RAID_FIXED_SEED: u64 = 17' \
  'QUICK_RAID_DURATION_SECONDS: u32 = 15 * 60' \
  'QUICK_RAID_PACK_ID' \
  'QuickRaidEvidenceCardV1' \
  'QuickRaidExperimentRunV1' \
  'QuickRaidPaperBundleV1' \
  'QuickRaidAuthorityV1' \
  'QuickRaidError::AuthorityEscape' \
  'pub fn apply_action' \
  'pub fn expire'; do
  rg -q --fixed-strings "$required" "$domain" || { echo "Quick Raid domain marker missing: $required" >&2; exit 1; }
done

for flag in activation_eligible qualification_eligible scientific_finality_eligible ranking_eligible reward_eligible score_eligible economic_eligible completion_portable; do
  rg -q --fixed-strings "pub $flag: bool" "$domain" || { echo "Quick Raid Rust lock missing: $flag" >&2; exit 1; }
  rg -q --fixed-strings "$flag BOOLEAN NOT NULL DEFAULT FALSE" "$migration" || { echo "Quick Raid SQL lock missing: $flag" >&2; exit 1; }
  rg -q --fixed-strings "$flag = FALSE" "$migration" || { echo "Quick Raid SQL false-only check missing: $flag" >&2; exit 1; }
done

for required in \
  'paper_raid_bff_quick_raid_sessions' \
  'paper_raid_bff_quick_raid_events' \
  'paper_raid_bff_quick_raid_session_monotonic_v1()' \
  'paper_raid_bff_reject_quick_raid_event_mutation_v1()' \
  'paper_raid_bff_quick_raid_events_no_truncate' \
  "VALUES ('quick_raid_fixed_seed_v1')"; do
  rg -q --fixed-strings "$required" "$migration" || { echo "Quick Raid SQL append-only marker missing: $required" >&2; exit 1; }
done

# The player slice must expose a complete, bounded result and a recoverable
# terminal action.  Keep these markers in the source gate so a future UI
# rewrite cannot silently drop the visible bundle or strand an active session.
for required in \
  'quick-raid-bundle' \
  'data-finality="none"' \
  'data-portable="false"' \
  'quick-raid-metrics' \
  'quick-raid-abandon-form' \
  '"/api/quick-raid/abandon"'; do
  rg -q --fixed-strings "$required" "$html" "$browser" "$http" || { echo "Quick Raid player-slice marker missing: $required" >&2; exit 1; }
done

for required in \
  'pub mod quick_raid;' \
  'pub mod quick_raid_http;' \
  '0011_quick_raid.sql' \
  'pub async fn quick_raid_schema_ready' \
  'merge(crate::quick_raid_http::router())' \
  '/league/quick-raid' \
  '/api/quick-raid/start' \
  '/api/quick-raid/action'; do
  rg -q --fixed-strings "$required" "$library" "$http" "$app" "$domain" "$db" || { echo "Quick Raid route wiring marker missing: $required" >&2; exit 1; }
done

for forbidden in 'ranking_eligible: true' 'reward_eligible: true' 'economic_eligible: true' 'portable: true' 'finality: "scientific"'; do
  if rg -n --fixed-strings "$forbidden" "$domain" "$http" "$html" "$browser" "$migration"; then
    echo "Quick Raid authority boundary crossed: $forbidden" >&2
    exit 1
  fi
done

echo "paper-raid-bff Quick Raid boundary: ok"
