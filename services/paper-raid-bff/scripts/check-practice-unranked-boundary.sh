#!/usr/bin/env bash
set -euo pipefail

service_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
practice="$service_root/src/practice.rs"
database="$service_root/src/db.rs"
library="$service_root/src/lib.rs"
migration="$service_root/migrations/0009_practice_unranked.sql"
scratch=$(mktemp)

cleanup() {
  rm -f -- "$scratch"
}
trap cleanup EXIT INT TERM

for file in "$practice" "$database" "$library" "$migration"; do
  [[ -f "$file" ]] || {
    echo "practice_unranked boundary file is absent: $file" >&2
    exit 1
  }
done

# Hostile tests intentionally name forbidden cross-domain keys.  The production
# contract above cfg(test) must not know any of them.
awk '/^#\[cfg\(test\)\]/{exit} {print}' "$practice" >"$scratch"
for forbidden in \
  'paper_project_id' \
  'challenge_id' \
  'activation_id' \
  'qualification_id' \
  'submission_id' \
  'release_candidate_hash' \
  'paper_bundle_hash' \
  'finality_receipt_hash' \
  'HeptaClient' \
  'Nakama' \
  'CasClient' \
  'crate::hepta' \
  'crate::nakama' \
  'crate::cas' \
  'LegacyGoldenQualification' \
  'LegacyUnranked' \
  'PaperProject' \
  'PaperBundle' \
  'finalize_paper' \
  'axum::' \
  'sqlx::'
do
  if rg -n --fixed-strings "$forbidden" "$scratch"; then
    echo "practice_unranked production contract crossed an authority boundary: $forbidden" >&2
    exit 1
  fi
done

for required in \
  'pub const PRACTICE_UNRANKED_MODE: &str = "practice_unranked"' \
  'pub const PRACTICE_AUTHORITY_NONE: &str = "none"' \
  '#[serde(deny_unknown_fields)]' \
  'PracticeStageV1' \
  'PracticeBridgeTaskStateV1' \
  'apply_browser_action' \
  'claim_bridge_task' \
  'apply_bridge_result' \
  'PracticeEligibilityV1::locked()' \
  'PracticeError::AuthorityEscape'
do
  rg -q --fixed-strings "$required" "$scratch" || {
    echo "practice_unranked pure state-machine boundary drifted: $required" >&2
    exit 1
  }
done

for flag in \
  activation_eligible \
  qualification_eligible \
  scientific_finality_eligible \
  ranking_eligible \
  reward_eligible \
  score_eligible \
  economic_eligible \
  completion_portable
do
  rg -q --fixed-strings "pub $flag: bool" "$scratch" || {
    echo "practice_unranked Rust eligibility lock is absent: $flag" >&2
    exit 1
  }
  rg -q --fixed-strings "$flag BOOLEAN NOT NULL DEFAULT FALSE" "$migration" || {
    echo "practice_unranked PostgreSQL eligibility lock is absent: $flag" >&2
    exit 1
  }
  rg -q --fixed-strings "$flag = FALSE" "$migration" || {
    echo "practice_unranked PostgreSQL false-only constraint is absent: $flag" >&2
    exit 1
  }
done

for required in \
  'paper_raid_bff_practice_sessions' \
  'paper_raid_bff_practice_events' \
  'paper_raid_bff_practice_binding_owner_fk' \
  'paper_raid_bff_practice_no_authority_ck' \
  'paper_raid_bff_practice_session_monotonic_v1()' \
  'paper_raid_bff_reject_practice_event_mutation_v1()' \
  'paper_raid_bff_practice_events_no_truncate' \
  "VALUES ('practice_unranked_v1')"
do
  rg -q --fixed-strings "$required" "$migration" || {
    echo "practice_unranked database boundary drifted: $required" >&2
    exit 1
  }
done

rg -q --fixed-strings 'pub mod practice;' "$library" || {
  echo "practice_unranked module is not exported" >&2
  exit 1
}
for required in \
  '../migrations/0009_practice_unranked.sql' \
  'pub async fn practice_unranked_schema_ready' \
  '&& practice_unranked_schema_ready(pool).await' \
  'expected_practice_unranked_sources()'
do
  rg -q --fixed-strings "$required" "$database" || {
    echo "practice_unranked fail-closed readiness wiring drifted: $required" >&2
    exit 1
  }
done

python3 - "$database" <<'PY'
from pathlib import Path
import sys


class PracticeCatalogError(ValueError):
    pass


def assert_catalog_contract(source: str) -> None:
    query = source.split(
        'const PRACTICE_UNRANKED_SCHEMA_READY_SQL: &str = r#"', 1
    )[1].split('"#;', 1)[0]
    markers = [
        "count(*) = 30 AND bool_and(COALESCE(",
        "count(*) = 30 FROM pg_attribute",
        "count(*) = 10 AND bool_and(COALESCE(",
        "count(*) = 10 FROM pg_attribute",
        "count(*) = 23 AND bool_and(COALESCE(",
        "count(*) = 23 FROM pg_constraint",
        "count(*) = 9 AND bool_and(COALESCE(",
        "count(*) = 9 FROM pg_constraint",
        "constraint_row.conkey::text = '{4,2,3}'",
        "constraint_row.confkey::text = '{1,4,5}'",
        "activation_eligible=false%qualification_eligible=false%",
        "scientific_finality_eligible=false%ranking_eligible=false%",
        "reward_eligible=false%score_eligible=false%economic_eligible=false%",
        "completion_portable=false%",
        "trigger_row.tgtype = 19",
        "trigger_row.tgtype = 27",
        "trigger_row.tgtype = 34",
        "capability = 'practice_unranked_v1'",
    ]
    for marker in markers:
        if marker not in query:
            raise PracticeCatalogError(
                f"practice_unranked readiness query is missing {marker}"
            )
    if query.count("bool_and(COALESCE(") != 4:
        raise PracticeCatalogError(
            "practice_unranked catalog aggregates must fail closed on NULL exactly four times"
        )
    for marker in [
        "normalize_sql(&monotonic) == normalize_sql(expected.0)",
        "normalize_sql(&append_only) == normalize_sql(expected.1)",
        "expected_practice_unranked_sources()",
    ]:
        if marker not in source:
            raise PracticeCatalogError(
                f"practice_unranked exact function-source gate is missing {marker}"
            )


source = Path(sys.argv[1]).read_text(encoding="utf-8")
assert_catalog_contract(source)
mutants = {
    "partial session columns": source.replace(
        "count(*) = 30 AND bool_and(COALESCE(",
        "count(*) = 29 AND bool_and(COALESCE(",
        1,
    ),
    "partial constraints": source.replace(
        "count(*) = 23 FROM pg_constraint",
        "count(*) = 22 FROM pg_constraint",
        1,
    ),
    "cross-owner key drift": source.replace("'{4,2,3}'", "'{4,3}'", 1),
    "ranking escape": source.replace("ranking_eligible=false%", "", 1),
    "append-only trigger drift": source.replace(
        "trigger_row.tgtype = 27", "trigger_row.tgtype = 19", 1
    ),
    "missing capability": source.replace(
        "capability = 'practice_unranked_v1'",
        "capability = 'practice_unranked_v0'",
        1,
    ),
}
for name, mutant in mutants.items():
    if mutant == source:
        raise SystemExit(f"practice catalog hostile mutant was not constructed: {name}")
    try:
        assert_catalog_contract(mutant)
    except PracticeCatalogError:
        continue
    raise SystemExit(f"practice catalog gate accepted hostile mutant: {name}")
PY

echo "paper-raid-bff practice_unranked boundary: ok"
