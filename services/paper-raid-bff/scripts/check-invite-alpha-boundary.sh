#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
migration="$root/migrations/0003_invite_alpha_access.sql"
access="$root/src/access.rs"
app="$root/src/app.rs"
config="$root/src/config.rs"
ctl="$root/src/bin/paper-raid-accessctl.rs"
env_example="$root/deploy/alpha.env.example"

for file in "$migration" "$access" "$app" "$config" "$ctl"; do
  test -s "$file"
done

for table in schema_capabilities accounts account_scopes account_author_roles invite_batches invites login_credentials access_audit quota_windows retention_runs; do
  grep -Fq "paper_raid_bff_${table}" "$migration"
done

grep -Fq 'None | Some("fixed_alpha") => Ok(IdentityMode::FixedAlpha)' "$config"
grep -Fq 'invite_alpha forbids PAPER_RAID_BFF_ALPHA_IDENTITIES_JSON' "$config"
grep -Fq 'identity_for_subject(&subject_id)' "$app"
grep -Fq 'authenticate_or_redeem(&request.login_key)' "$app"
grep -Fq 'issue_at_generation(&identity, generation)' "$app"
grep -Fq 'session_generation_tx(&mut tx, &identity.subject_id)' "$access"
grep -Fq '"access_directory_reachable"' "$app"
grep -Fq '"access_directory_within_capacity"' "$app"
grep -Fq '"access_audit_append_only"' "$app"
grep -Fq '"access_topology_ready"' "$app"
grep -Fq 'author_topology_ready' "$access"
grep -Fq 'review_topology_ready' "$access"
grep -Fq 'PAPER_RAID_BFF_IDENTITY_MODE=fixed_alpha' "$env_example"
grep -Fq '# PAPER_RAID_BFF_LOGIN_QUOTA_BUCKET_LIMIT=' "$env_example"
grep -Fq 'enforce_mutation_quota' "$root/src/auth.rs"
grep -Fq 'StatusCode::TOO_MANY_REQUESTS' "$root/src/error.rs"
grep -Fq 'header::RETRY_AFTER' "$root/src/error.rs"
grep -Fq 'LOGIN_BUCKET_DOMAIN' "$access"
grep -Fq 'login_bucket_principal(&candidate_hash)' "$access"
if grep -Fq 'known_candidate' "$access"; then
  echo "quota bucket depends on credential existence" >&2
  exit 1
fi
grep -Fq 'paper_raid_bff_access_audit is append-only' "$migration"
grep -Fq 'BEFORE UPDATE OR DELETE ON paper_raid_bff_access_audit' "$migration"

if grep -Eq 'route\([^)]*(accessctl|invite|batch|account-(suspend|close)|credential-rotate)' "$app"; then
  echo "operator control leaked into the network router" >&2
  exit 1
fi

if grep -Eqi 'hepta_league_state|consumer-entry-api|LeagueState::|paper_score|league_reward' "$migration" "$access" "$ctl"; then
  echo "invite alpha crossed the Paper Raid authority boundary" >&2
  exit 1
fi

if grep -Eq '(invite_secret|login_credential|login_key)[[:space:]]+(TEXT|BYTEA)' "$migration"; then
  echo "cleartext credential column detected" >&2
  exit 1
fi

grep -Fq 'secret_hash BYTEA' "$migration"
grep -Fq 'displayed_once_not_stored' "$ctl"
grep -Fq 'PAPER_RAID_BFF_IDENTITY_MODE must be invite_alpha' "$ctl"
grep -Fq 'PAPER_RAID_ACCESS_DATABASE_URL' "$ctl"
if grep -Fq 'required_env("PAPER_RAID_BFF_DATABASE_URL")' "$ctl"; then
  echo "operator CLI reads the resident BFF database credential" >&2
  exit 1
fi
grep -Fq 'invite_schema_ready(&pool)' "$app"
grep -Fq 'revoke_sessions(&mut tx, subject)' "$ctl"

echo "invite-alpha static boundary: PASS"
