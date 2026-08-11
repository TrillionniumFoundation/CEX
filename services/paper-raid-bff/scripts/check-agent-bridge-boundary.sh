#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
migration="$root/migrations/0004_agent_pairing_bridge.sql"
delivery_migration="$root/migrations/0005_agent_delivery_drafts.sql"
review_migration="$root/migrations/0008_review_execution_receipts.sql"
review_upgrade_fixture="$root/migrations/fixtures/0008_review_receipt_true_upgrade.sql"
review_partial_fixture="$root/migrations/fixtures/0008_review_receipt_partial_catalog.sql"
review_tamper_fixture="$root/migrations/fixtures/0008_review_receipt_catalog_tamper.sql"
postgres_gate="$root/scripts/check-postgres.sh"
bridge="$root/src/agent_bridge.rs"
review_receipts="$root/src/review_receipts.rs"
browser="$root/src/browser.js"
db="$root/src/db.rs"
ctl="$root/src/bin/paper-raid-accessctl.rs"
readme="$root/README.md"
tool_root=$(CDPATH= cd -- "$root/../.." && pwd)/tools/paper-raid-agent-bridge
contracts=$(CDPATH= cd -- "$root/../.." && pwd)/crates/hepta-paper-raid-contracts/src/lib.rs

for file in "$migration" "$delivery_migration" "$review_migration" \
  "$review_upgrade_fixture" "$review_partial_fixture" "$review_tamper_fixture" \
  "$postgres_gate" "$bridge" \
  "$review_receipts" "$browser" "$db" "$ctl" "$readme" "$contracts" \
  "$tool_root/src/operations.mjs" "$tool_root/src/state.mjs" \
  "$tool_root/src/canonical.mjs" "$tool_root/src/review.mjs" "$tool_root/src/work.mjs" \
  "$tool_root/test/canonical.test.mjs" "$tool_root/test/review.test.mjs" \
  "$tool_root/test/work.test.mjs"; do
  test -s "$file"
done

grep -Fq "expires_at <= created_at + interval '60 seconds'" "$migration"
grep -Fq 'At-most-60-second exact replay cache' "$migration"
grep -Fq 'DELETE FROM paper_raid_bff_agent_request_uses WHERE expires_at <= $1' "$bridge"
grep -Fq 'DELETE FROM paper_raid_bff_agent_request_uses WHERE expires_at <= now()' "$ctl"
grep -Fq 'DELETE FROM paper_raid_bff_agent_delivery_drafts WHERE expires_at <= now()' "$ctl"
grep -Fq 'Route reads,' "$bridge"
grep -Fq 'complete_agent_request still compare-checks exact bytes' "$bridge"
grep -Fq 'authoritative_bridge_binding(state, &identity, &mapping).await?' "$bridge"
grep -Fq 'AgentRequestUseState::Completed(status, body)' "$bridge"
grep -Fq "expires_at <= created_at + interval '15 minutes'" "$delivery_migration"
grep -Fq 'lease_fencing_token > 0' "$delivery_migration"
grep -Fq 'lease_fencing_token <= 9007199254740991' "$delivery_migration"
grep -Fq 'expected_work_version > 0' "$delivery_migration"
grep -Fq 'expected_work_version <= 9007199254740991' "$delivery_migration"
grep -Fq 'agent_bridge_delivery_drafts_v1' "$delivery_migration"
grep -Fq 'resolve_delivery_context' "$bridge"
grep -Fq 'delivery_draft_matches_context' "$bridge"
grep -Fq 'delivery_draft_id' "$tool_root/src/work.mjs"
grep -Fq 'evaluation_id UUID NOT NULL' "$review_migration"
grep -Fq "receipt -> 'receipt' ->> 'evaluation_id' IS NOT DISTINCT FROM evaluation_id::text" "$review_migration"
grep -Fq 'parse_challenge_evaluator_manifest' "$contracts"
grep -Fq 'parse_challenge_dataset_manifest' "$contracts"
grep -Fq 'validate_resolved_review_object_mapping(' "$contracts"
grep -Fq 'verify_frozen_review_authority(&authority)' "$bridge"
grep -Fq '.get(&authority.evaluator_manifest_hash, "application/json")' "$bridge"
grep -Fq '.get(&authority.dataset_manifest_hash, "application/json")' "$bridge"
grep -Fq 'resolve_manifest_members(&authority, &evaluator, &dataset)' "$bridge"
grep -Fq 'resolved_transport_path' "$bridge"
grep -Fq '"frozen_evaluator" | "evaluator_support" | "dataset" | "input" | "candidate"' "$bridge"
grep -Fq '"resolved_frozen_review_bundle".to_string()' "$bridge"
if grep -Eq '"(execution_contract_status|result_consumable)"' "$bridge"; then
  echo "Agent review inbox still publishes source-only/non-consumable task fields" >&2
  exit 1
fi
grep -Fq 'confirmation_context_signature TEXT' "$review_migration"
grep -Fq 'receipt_context_signing_bytes' "$review_migration"
grep -Fq 'receipt_context_signature: String' "$review_receipts"
grep -Fq 'verify_confirmation_context_signature' "$review_receipts"
grep -Fq 'confirmation_context_signature=$2' "$review_receipts"
grep -Fq 'receipt_context_signature: signed.receiptContextSignature' "$browser"
grep -Fq 'signReviewConfirmationFrame' "$browser"
grep -Fq 'target_paper_author_forbidden_or_unassigned' "$bridge"
grep -Fq 'paper_raid_bff_one_live_review_receipt_per_task' "$review_migration"
grep -Fq 'review_execution_receipt_id(' "$contracts"
grep -Fq 'let expected_receipt_id = review_execution_receipt_id(' "$bridge"
grep -Fq 'projected_review_task_attempt' "$bridge"
grep -Fq 'reviewExecutionReceiptId(' "$tool_root/src/review.mjs"
grep -Fq 'validateReviewReceiptResult' "$tool_root/src/operations.mjs"
python3 - "$review_upgrade_fixture" "$review_partial_fixture" \
  "$review_tamper_fixture" "$postgres_gate" <<'PY'
from pathlib import Path
import sys


class FixtureContractError(ValueError):
    pass


def require(source: str, marker: str, label: str) -> None:
    if marker not in source:
        raise FixtureContractError(f"{label} is missing {marker}")


def validate_upgrade(source: str) -> None:
    previous = -1
    for revision, name in [
        ("0001", "bff_state"),
        ("0002", "product_telemetry"),
        ("0003", "invite_alpha_access"),
        ("0004", "agent_pairing_bridge"),
        ("0005", "agent_delivery_drafts"),
        ("0006", "accessctl_operator_audit"),
        ("0007", "invite_activation_authority"),
    ]:
        marker = f"\\ir ../{revision}_{name}.sql"
        if source.count(marker) != 1:
            raise FixtureContractError(f"true upgrade must apply {marker} exactly once")
        current = source.index(marker)
        if current <= previous:
            raise FixtureContractError("pre-0008 migrations are not ordered")
        previous = current
    seed = source.index("DO $pre_upgrade_seed$")
    snapshot = source.index("CREATE TEMP TABLE fixture_0008_legacy_rows_before")
    first_0008 = source.index("\\ir ../0008_review_execution_receipts.sql")
    if not previous < seed < snapshot < first_0008:
        raise FixtureContractError("legacy rows are not seeded and snapshotted before 0008")
    if source.count("\\ir ../0008_review_execution_receipts.sql") != 2:
        raise FixtureContractError("true upgrade must prove one install and one replay")
    for marker in [
        "0008 rewrote or lost valid pre-0008 rows",
        "0008 rewrote or removed a pre-0008 capability",
        "0008 replay rewrote or lost its capability",
        "0008 replay rewrote or lost valid pre-0008 rows",
        "GRANT SELECT, INSERT, UPDATE ON TABLE paper_raid_bff_review_execution_receipts",
        "'DELETE','TRUNCATE','REFERENCES','TRIGGER','MAINTAIN'",
        "BFF persisted a Frozen Review Bundle authority table",
    ]:
        require(source, marker, "true 0008 upgrade fixture")


def validate_partial(source: str) -> None:
    table = source.index("CREATE TABLE paper_raid_bff_review_execution_receipts")
    migration = source.index("\\ir ../0008_review_execution_receipts.sql")
    if table >= migration:
        raise FixtureContractError("partial table is not installed before 0008")
    partial_definition = source[table:migration]
    for omitted in ("binding_id", "paper_id", "receipt", "confirmation_context_signature"):
        if f"\n    {omitted} " in partial_definition:
            raise FixtureContractError(f"partial table unexpectedly includes {omitted}")
    for marker in [
        "fixture_schema_ready :=",
        "IF fixture_schema_ready THEN",
        "partial 0008 catalog was accepted as ready",
        "deceptive capability/index state",
    ]:
        require(source, marker, "partial 0008 fixture")


def validate_tamper(source: str) -> None:
    for marker in [
        "CREATE FUNCTION pg_temp.fixture_0008_schema_ready()",
        "fresh 0008 catalog did not satisfy fixture readiness",
        "readiness accepted a missing 0008 capability",
        "readiness accepted a pending-only live receipt index",
        "readiness accepted an unvalidated lifecycle constraint",
        "readiness accepted an unreviewed authority column",
        "rollback missing-capability tamper",
        "rollback live-index tamper",
        "rollback lifecycle tamper",
        "rollback column tamper",
    ]:
        require(source, marker, "0008 catalog tamper fixture")


def validate_gate(source: str) -> None:
    expected = {
        "paper_raid_bff_0008_true_upgrade": "0008_review_receipt_true_upgrade.sql",
        "paper_raid_bff_0008_partial_catalog": "0008_review_receipt_partial_catalog.sql",
        "paper_raid_bff_0008_catalog_tamper": "0008_review_receipt_catalog_tamper.sql",
    }
    for database, fixture in expected.items():
        if source.count(database) != 2 or source.count(fixture) != 1:
            raise FixtureContractError(
                f"PostgreSQL gate does not bind {database} to {fixture} exactly"
            )


upgrade, partial, tamper, gate = [
    Path(path).read_text(encoding="utf-8") for path in sys.argv[1:]
]
validate_upgrade(upgrade)
validate_partial(partial)
validate_tamper(tamper)
validate_gate(gate)

mutants = {
    "upgrade without 0007": (
        validate_upgrade,
        upgrade.replace("\\ir ../0007_invite_activation_authority.sql\n", "", 1),
    ),
    "upgrade without pre-0008 snapshot": (
        validate_upgrade,
        upgrade.replace(
            "CREATE TEMP TABLE fixture_0008_legacy_rows_before",
            "CREATE TEMP TABLE fixture_0008_legacy_rows_removed",
            1,
        ),
    ),
    "partial fixture without fail-closed decision": (
        validate_partial,
        partial.replace("IF fixture_schema_ready THEN", "IF NOT fixture_schema_ready THEN", 1),
    ),
    "tamper fixture without index negative": (
        validate_tamper,
        tamper.replace("readiness accepted a pending-only live receipt index", "removed", 1),
    ),
    "PostgreSQL gate without true upgrade": (
        validate_gate,
        gate.replace("0008_review_receipt_true_upgrade.sql", "removed.sql", 1),
    ),
}
for name, (validator, mutant) in mutants.items():
    try:
        validator(mutant)
    except (FixtureContractError, ValueError):
        continue
    raise SystemExit(f"0008 fixture static gate accepted hostile mutant: {name}")
PY
python3 - "$db" <<'PY'
from pathlib import Path
import sys


class DeliveryCatalogError(ValueError):
    pass


def assert_delivery_bigint_deparse(source: str) -> None:
    status = source.split(
        "pub async fn agent_bridge_schema_status", 1
    )[1].split("let integrity_ok", 1)[0]
    for column in ("lease_fencing_token", "expected_work_version"):
        expected = (
            f"CHECK((({column}>0)AND({column}<="
            "''9007199254740991''::bigint)))"
        )
        if status.count(expected) != 1:
            raise DeliveryCatalogError(
                f"Agent Bridge readiness must match PostgreSQL BIGINT deparse for {column}"
            )


source = Path(sys.argv[1]).read_text(encoding="utf-8")
assert_delivery_bigint_deparse(source)
hostile_mutants = {
    "untyped fencing upper bound": source.replace(
        "lease_fencing_token<=''9007199254740991''::bigint",
        "lease_fencing_token<=9007199254740991",
        1,
    ),
    "untyped work-version upper bound": source.replace(
        "expected_work_version<=''9007199254740991''::bigint",
        "expected_work_version<=9007199254740991",
        1,
    ),
    "unquoted BIGINT upper bound": source.replace(
        "''9007199254740991''::bigint",
        "9007199254740991::bigint",
        1,
    ),
}
for name, mutant in hostile_mutants.items():
    if mutant == source:
        raise SystemExit(f"delivery catalog hostile mutant was not constructed: {name}")
    try:
        assert_delivery_bigint_deparse(mutant)
    except DeliveryCatalogError:
        continue
    raise SystemExit(f"delivery catalog static gate accepted hostile mutant: {name}")
PY
python3 - "$db" <<'PY'
from pathlib import Path
import sys


class ReviewCatalogError(ValueError):
    pass


def assert_review_catalog_exact(source: str) -> None:
    query = source.split(
        'const REVIEW_RECEIPT_SCHEMA_READY_SQL: &str = r#"', 1
    )[1].split('"#;', 1)[0]
    for marker in [
        "count(*) = 25 FROM expected_columns",
        "count(*) = 25 FROM pg_attribute",
        "count(*) = 17 AND bool_and(",
        "constraint_row.connoinherit = (constraint_row.contype IN ('p','f'))",
        "constraint_row.conkey::text = '{3}'",
        "constraint_row.confkey::text = '{1}'",
        "constraint_row.confdeltype = 'c'",
        "constraint_row.confmatchtype = 's'",
        "count(*) = 4 AND bool_and(",
        "index_row.indkey::text = '2 9'",
        "index_row.indkey::text = '2'",
        "index_row.indkey::text = '4 14 22'",
        "index_row.indoption::text = '0 0 3'",
        "pg_get_indexdef(index_row.indexrelid, 3, TRUE) = 'created_at'",
        "(state=ANY(ARRAY[''pending''::text,''consumed''::text]))",
        "obj_description(relation.oid, 'pg_class')",
        "JOIN review_table relation ON relation.oid = policy_row.polrelid",
        "capability = 'review_execution_receipts_v1'",
    ]:
        if marker not in query:
            raise ReviewCatalogError(f"Review receipt catalog query is missing {marker}")
    if query.count("bool_and(COALESCE(") != 4:
        raise ReviewCatalogError(
            "Review receipt catalog aggregates must fail closed on NULL exactly four times"
        )

    matcher = source.split(
        "fn review_receipt_constraint_definition_is_exact(", 1
    )[1].split("\nasync fn review_receipt_schema_ready(", 1)[0]
    constraints = [
        "paper_raid_bff_review_receipts_pk",
        "paper_raid_bff_review_receipt_binding_fk",
        "paper_raid_bff_review_receipt_kind_ck",
        "paper_raid_bff_review_receipt_attempt_ck",
        "paper_raid_bff_review_receipt_fencing_ck",
        "paper_raid_bff_review_receipt_bundle_hash_ck",
        "paper_raid_bff_review_receipt_hash_ck",
        "paper_raid_bff_review_receipt_state_ck",
        "paper_raid_bff_review_receipt_frame_hash_ck",
        "paper_raid_bff_review_receipt_confirmation_hash_ck",
        "paper_raid_bff_review_receipt_context_signature_ck",
        "paper_raid_bff_review_receipt_response_status_ck",
        "paper_raid_bff_review_receipt_result_shape_ck",
        "paper_raid_bff_review_receipt_frame_pair_ck",
        "paper_raid_bff_review_receipt_frame_binding_ck",
        "paper_raid_bff_review_receipt_lifecycle_ck",
        "paper_raid_bff_review_receipt_json_binding_ck",
    ]
    for name in constraints:
        if matcher.count(f'"{name}"') != 1:
            raise ReviewCatalogError(
                f"Review receipt exact definition matcher must own {name} exactly once"
            )
    for marker in [
        "compact == review_frame_binding_definition(false)",
        "compact == review_frame_binding_definition(true)",
        "compact == review_json_binding_definition(false)",
        "compact == review_json_binding_definition(true)",
        '"receipt->\'receipt\'->>\'evaluation_id\'", "evaluation_id"',
        '"confirmation_frame->\'receipt_context\'->>\'assignment_version\'::bigint"',
        '"confirmation_frame->\'receipt_context\'->\'candidate_passed\'"',
        "definitions.len() == 17",
        "review_receipt_constraint_definition_is_exact(name, definition)",
        "base_schema_ready && review_receipt_schema_ready(pool).await",
    ]:
        if marker not in source:
            raise ReviewCatalogError(f"Review receipt exact definition gate is missing {marker}")


source = Path(sys.argv[1]).read_text(encoding="utf-8")
assert_review_catalog_exact(source)
hostile_mutants = {
    "partial column catalog": source.replace(
        "count(*) = 25 FROM expected_columns",
        "count(*) = 24 FROM expected_columns",
        1,
    ),
    "missing constraint authority": source.replace(
        "count(*) = 17 AND bool_and(",
        "count(*) = 16 AND bool_and(",
        1,
    ),
    "wrong task-attempt columns": source.replace(
        "index_row.indkey::text = '2 9'",
        "index_row.indkey::text = '2 8'",
        1,
    ),
    "pending-only live predicate": source.replace(
        "(state=ANY(ARRAY[''pending''::text,''consumed''::text]))",
        "(state=''pending''::text)",
        1,
    ),
    "ascending inbox timestamp": source.replace(
        "index_row.indoption::text = '0 0 3'",
        "index_row.indoption::text = '0 0 0'",
        1,
    ),
    "NULL-skipping catalog aggregate": source.replace(
        "bool_and(COALESCE(",
        "bool_and(",
        1,
    ),
    "incomplete JSON binding": source.replace(
        '"receipt->\'receipt\'->>\'evaluation_id\'", "evaluation_id"',
        '"receipt->\'receipt\'->>\'evaluation_id\'", "submission_id"',
        1,
    ),
    "bypassed exact readiness": source.replace(
        "base_schema_ready && review_receipt_schema_ready(pool).await",
        "base_schema_ready",
        1,
    ),
}
for name, mutant in hostile_mutants.items():
    if mutant == source:
        raise SystemExit(f"Review catalog hostile mutant was not constructed: {name}")
    try:
        assert_review_catalog_exact(mutant)
    except ReviewCatalogError:
        continue
    raise SystemExit(f"Review catalog static gate accepted hostile mutant: {name}")
PY
python3 - "$bridge" <<'PY'
import pathlib
import re
import sys

source = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
claim_start = source.index("async fn claim_delivery_draft_for_proposal(")
claim_end = source.index("\nasync fn consume_claimed_delivery_draft(", claim_start)
claim = source[claim_start:claim_end]
draft_binding = 'let mut draft = delivery_draft_from_row(&row)?;'
recovery_binding = (
    'let is_recovery = matches!(draft.state.as_str(), "submitting" | "consumed");'
)
if claim.index(draft_binding) >= claim.index(recovery_binding):
    raise SystemExit("delivery recovery reads the locked draft before binding it")
if "Result<(DeliveryDraft, bool), AppError>" not in claim \
        or "Ok((draft, is_recovery))" not in claim:
    raise SystemExit("delivery claim does not return its locked recovery truth")
caller_start = source.index("pub async fn agent_proposal(")
caller_end = source.index("\nfn decode_json<", caller_start)
caller = source[caller_start:caller_end]

def delivery_recovery_binding_error(candidate):
    binding = re.search(
        r"let\s+\(\s*delivery_draft\s*,\s*is_recovery\s*\)\s*=\s*"
        r"claim_delivery_draft_for_proposal\s*\(\s*&state\s*,\s*&draft\s*,\s*"
        r"&delivery_claim\s*\)\s*\.await\?;",
        candidate,
    )
    if binding is None:
        return "Agent proposal caller does not consume locked recovery truth"
    submission = candidate.find("let submission = async", binding.end())
    if submission < 0:
        return "Agent proposal caller does not submit from the locked delivery draft"
    metric = re.search(
        r"if\s+is_recovery\s*\{\s*"
        r"state\.metrics\.observe_bridge_recovery\(submission\.is_ok\(\)\);\s*\}",
        candidate[submission:],
    )
    if metric is None:
        return "Agent proposal recovery metrics omit the locked failure outcome"
    locked_flow = candidate[binding.end():]
    if "draft.state" in locked_flow:
        return "Agent proposal caller re-infers recovery from an unlocked draft snapshot"
    if not re.search(
        r"validate_delivery_proposal_response\s*\(\s*&proposal\s*,\s*"
        r"&delivery_draft\s*,",
        locked_flow,
    ):
        return "Agent proposal response validation does not use the locked delivery draft"
    if not re.search(
        r"consume_claimed_delivery_draft\s*\(\s*&state\s*,\s*&delivery_draft\s*,\s*"
        r"&proposal_body_hash\s*\)\s*\.await\?;",
        locked_flow,
    ):
        return "Agent proposal consumption does not use the locked delivery draft"
    return None

error = delivery_recovery_binding_error(caller)
if error is not None:
    raise SystemExit(error)

# Prove the static guard rejects the three dangerous regressions: discarding the
# lock-derived truth, re-inferring it from the stale pre-lock row, or validating /
# consuming the upstream result against that stale row.
mutants = {
    "discarded recovery truth": caller.replace(
        "(delivery_draft, is_recovery)", "(delivery_draft, _)", 1
    ),
    "unlocked recovery inference": caller.replace(
        "if is_recovery {",
        'if matches!(draft.state.as_str(), "submitting" | "consumed") {',
        1,
    ),
    "stale response authority": caller.replace(
        "&proposal,\n            &delivery_draft,",
        "&proposal,\n            &draft,",
        1,
    ),
    "stale consumption authority": caller.replace(
        "consume_claimed_delivery_draft(&state, &delivery_draft, &proposal_body_hash)",
        "consume_claimed_delivery_draft(&state, &draft, &proposal_body_hash)",
        1,
    ),
}
for name, mutant in mutants.items():
    if mutant == caller:
        raise SystemExit(f"Agent proposal recovery hostile mutant was not constructed: {name}")
    if delivery_recovery_binding_error(mutant) is None:
        raise SystemExit(f"Agent proposal recovery static guard accepted hostile mutant: {name}")
receipt_start = source.index("pub async fn agent_review_receipt(")
receipt_end = source.index("\nasync fn complete_agent_raw_request(", receipt_start)
receipt = source[receipt_start:receipt_end]
for marker in [
    '"attempt": receipt.attempt',
    '"status": "stored"',
    'ON CONFLICT DO NOTHING',
    'review_receipt_conflict_did_not_resolve_to_task_attempt',
    'review receipt attempt is stale or was not projected',
]:
    if marker not in receipt:
        raise SystemExit(f"Agent review receipt recovery is missing {marker}")
for forbidden in ['"replay":', '"state": "pending_human_confirmation"']:
    if forbidden in receipt:
        raise SystemExit(f"Agent review receipt result exposes mutable/path state: {forbidden}")
if 'ON CONFLICT (task_id,attempt) DO NOTHING' in receipt:
    raise SystemExit(
        "Agent review receipt insert targets only one of its two concurrent unique authorities"
    )
PY
grep -Fq 'stableDeliveryDraftId' "$tool_root/src/operations.mjs"
grep -Fq 'hepta.paper_raid.agent_proposal.v2' "$tool_root/src/canonical.mjs"
grep -Fq '["string", proposal.artifact_manifest_id]' "$tool_root/src/canonical.mjs"
grep -Fq 'Agent Proposal V1 historical frame remains byte frozen' \
  "$tool_root/test/canonical.test.mjs"
grep -Fq 'Agent Proposal V2 frame matches the frozen Rust epoch vector' \
  "$tool_root/test/canonical.test.mjs"
grep -Fq 'hepta.paper_raid.agent_bridge.delivery_candidates.v1' \
  "$tool_root/src/work.mjs"
grep -Fq 'workResult("submitted"' "$tool_root/src/cli.mjs"
if grep -Eq 'for \(const (lease|task|manifest) of' "$tool_root/src/work.mjs"; then
  echo "Agent Bridge work client reconstructs delivery candidates locally" >&2
  exit 1
fi
if grep -Fq 'command === "submit-proposal"' "$tool_root/src/cli.mjs"; then
  echo "Agent Bridge exposes a low-level proposal mutation bypass" >&2
  exit 1
fi

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
