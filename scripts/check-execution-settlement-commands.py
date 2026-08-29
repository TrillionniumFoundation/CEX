#!/usr/bin/env python3
"""Static fail-closed gate for P0-N5 durable Execution Ledger settlement."""

from __future__ import annotations

import json
import os
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROBLEMS: list[str] = []


def read(relative: str) -> str:
    path = ROOT / relative
    if not path.is_file():
        PROBLEMS.append(f"missing required file: {relative}")
        return ""
    try:
        return path.read_text(encoding="utf-8")
    except UnicodeDecodeError as error:
        PROBLEMS.append(f"required UTF-8 file cannot be decoded: {relative}: {error}")
        return ""


def require(relative: str, *markers: str) -> str:
    content = read(relative)
    for marker in markers:
        if marker not in content:
            PROBLEMS.append(f"{relative} lacks required marker: {marker}")
    return content


def migration_number(filename: str) -> int | None:
    match = re.match(r"^(\d{4})_", Path(filename).name)
    return int(match.group(1)) if match else None


MIGRATION_PATHS = [
    "migrations/0067_add_execution_ledger_settlement_schema.sql",
    "migrations/0068_add_execution_ledger_settlement_guards.sql",
    "migrations/0069_add_execution_ledger_settlement_enqueue.sql",
    "migrations/0070_add_execution_ledger_settlement_claim.sql",
    "migrations/0071_add_execution_ledger_settlement_finish.sql",
    "migrations/0072_add_execution_ledger_settlement_operator.sql",
]
SETTLEMENT_SLICE_HEAD = Path(MIGRATION_PATHS[-1]).name
migration_parts = [read(relative) for relative in MIGRATION_PATHS]
migration = "\n".join(migration_parts)
for relative, content in zip(MIGRATION_PATHS, migration_parts):
    if content and not content.strip().lower().startswith("begin;"):
        PROBLEMS.append(f"{relative} must begin with BEGIN")
    if content and not content.strip().lower().endswith("commit;"):
        PROBLEMS.append(f"{relative} must end with COMMIT")
for marker in (
    "cex_execution_ledger_settlement_commands_v1",
    "cex_validate_execution_ledger_settlement_insert_v1",
    "cex_enqueue_execution_ledger_settlement_v1",
    "cex_claim_execution_ledger_settlements_v1",
    "cex_finish_execution_ledger_settlement_v1",
    "cex_acknowledge_execution_ledger_settlement_v1",
    "cex_requeue_execution_ledger_settlement_v1",
    "claim_lease_expired_after_final_attempt_unknown_outcome",
    "reconcile_required",
    "execution_mode = 'active'",
    "cex_execution_ledger_settlement_status_v1",
    "cex_enqueue_audit_outbox_v1",
):
    if marker not in migration:
        PROBLEMS.append(f"settlement migration series lacks required marker: {marker}")
if "for update skip locked" not in migration.lower():
    PROBLEMS.append("settlement claim function lacks FOR UPDATE SKIP LOCKED")

WORKER_PARTS = [
    "services/execution-service/src/settlement_worker_config.rs",
    "services/execution-service/src/settlement_worker_runtime.rs",
    "services/execution-service/src/settlement_worker_helpers.rs",
    "services/execution-service/src/settlement_worker_tests.rs",
]
worker = "\n".join(read(relative) for relative in WORKER_PARTS)
for marker in (
    "cex_claim_execution_ledger_settlements_v1",
    "settle_invocation",
    "cex_finish_execution_ledger_settlement_v1",
    "Policy::none()",
    "validate_serial_lease_budget",
    "retry_budget_exhausted_unknown_outcome",
    "claim is intentionally left for lease recovery",
    "CEX_EXECUTION_SETTLEMENT_WORKER_ID",
    "CEX_EXECUTION_LEDGER_MODE=dual or require_v2",
):
    if marker not in worker:
        PROBLEMS.append(f"settlement worker source series lacks required marker: {marker}")
require(
    "services/execution-service/src/settlement_worker.rs",
    'include!("settlement_worker_config.rs")',
    'include!("settlement_worker_runtime.rs")',
    'include!("settlement_worker_helpers.rs")',
    'include!("settlement_worker_tests.rs")',
)

for pattern in (
    r"\bf64\b",
    r"reserve_amount",
    r"/v1/ledger/",
    r"\.round\s*\(",
    r"\.trunc\s*\(",
    r"\bas\s+i64\b",
    r"sqlx::Transaction",
    r"\.begin\s*\(\s*\)\s*\.await",
):
    if re.search(pattern, worker):
        PROBLEMS.append(f"settlement worker contains forbidden long-transaction/legacy-money marker: {pattern}")

claim_offset = worker.find("cex_claim_execution_ledger_settlements_v1")
network_offset = worker.find("settle_invocation")
finish_offset = worker.find("cex_finish_execution_ledger_settlement_v1")
if min(claim_offset, network_offset, finish_offset) < 0 or not (
    claim_offset < network_offset < finish_offset
):
    PROBLEMS.append("worker source does not visibly order claim -> network -> outcome persistence")

require(
    "services/execution-service/src/bin/execution-settlement-worker.rs",
    "settlement_worker::run_from_env",
    "init_tracing",
)
require("services/execution-service/src/lib.rs", "pub mod settlement_worker;")

api_path = ROOT / "services/execution-service/src/api.rs"
if api_path.is_file():
    api = api_path.read_text(encoding="utf-8")
elif os.environ.get("CEX_P0_N5_ALLOW_SYNTHETIC_BASELINE") == "1":
    api = ""
else:
    PROBLEMS.append("missing required file: services/execution-service/src/api.rs")
    api = ""
if api:
    if "settle_invocation(" in api:
        PROBLEMS.append(
            "api.rs directly activates exact settlement before caller transaction separation"
        )
    if "cex_claim_execution_ledger_settlements_v1" in api:
        PROBLEMS.append("api.rs must not act as the settlement worker")

    # Monetary value writes were retired in v12.  The API may still expose the
    # historical reservation flags for compatibility/read-only diagnostics, but
    # it must never call the old HTTP Ledger routes or carry the old floating
    # point request payload through a terminal transition.
    for marker in (
        "/v1/ledger/",
        "call_ledger_action(",
        "consume_reserved_credits(",
        "release_reserved_credits(",
    ):
        if marker in api:
            PROBLEMS.append(f"api.rs contains retired Ledger settlement marker: {marker}")
    if re.search(r"\bledger_(?:reserved|refunded)\s*=", api):
        PROBLEMS.append(
            "api.rs writes compatibility-only Invocation Ledger flags after v12 cutover"
        )

    for marker in (
        "fn ensure_no_legacy_settlement(",
        "legacy Ledger",
        "exact durable settlement command required",
        "provider-backed execution requires a durable dispatch command",
    ):
        if marker not in api:
            PROBLEMS.append(f"api.rs lacks fail-closed v12 marker: {marker}")

    # A provider dispatch must be durably claimed/enqueued before any network
    # I/O.  Keep this assertion scoped to the historical DB helper: the
    # no-pool test-only path intentionally invokes the adapter directly, while
    # production routes are intercepted by provider_dispatch.rs.
    start_match = re.search(
        r"async\s+fn\s+start_execution_in_db\([\s\S]*?(?=\nasync\s+fn\s+reject_execution_in_db)",
        api,
    )
    if not start_match:
        PROBLEMS.append("api.rs start_execution_in_db function cannot be isolated")
    else:
        start_source = start_match.group(0)
        if "state: &AppState" in start_source:
            PROBLEMS.append(
                "start_execution_in_db must not receive AppState while holding a SQL transaction"
            )
        for marker in (
            "dispatch_via_provider",
            "reqwest",
            "ledger_base_url",
            "ledger_manage_token",
            ".send(",
            "state.http",
        ):
            if marker in start_source:
                PROBLEMS.append(
                    "start_execution_in_db contains provider/Ledger network I/O marker: "
                    f"{marker}"
                )

    retry_match = re.search(
        r"async\s+fn\s+retry_execution_in_db\([\s\S]*?(?=\nasync\s+fn\s+requeue_execution_in_db)",
        api,
    )
    if not retry_match:
        PROBLEMS.append("api.rs retry_execution_in_db function cannot be isolated")
    elif "ensure_no_legacy_settlement" not in retry_match.group(0):
        PROBLEMS.append(
            "retry_execution_in_db must fail closed before touching a legacy reservation"
        )

require(
    "scripts/check-execution-settlement-commands-postgres.sh",
    "claim_lease_expired_after_final_attempt_unknown_outcome",
    "reconcile_required",
    "wrong worker",
    "fresh acknowledgement",
    "rollback;",
)
require(
    ".github/workflows/p0-execution-settlement-gate.yml",
    "actions/checkout@11d5960a326750d5838078e36cf38b85af677262",
    "dtolnay/rust-toolchain@4360b52568e2003a75bf9bc1d59f33a8e3fc893c",
    "Swatinem/rust-cache@6323deb102c322ba6fcbdcafc7e3dddab59af2b6",
    "scripts/check-execution-settlement-commands.py",
    "cargo test --locked -p execution-service --all-targets",
    "cargo check --locked --workspace --all-targets",
    "scripts/check-execution-settlement-commands-postgres.sh",
)
require(
    "docs/execution-ledger-settlement-commands-v1.md",
    "short worker claim transaction",
    "Ledger HTTP outside every business transaction",
    "reconcile_required",
    "fresh explicit acknowledgement",
)
require(
    "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md",
    "P0-N5 durable Execution settlement",
    "P0-N6 delivered by this candidate",
    "not production-ready",
)
require(
    "config/execution-settlement-worker.env.example",
    "CEX_EXECUTION_SETTLEMENT_BATCH_SIZE=2",
    "CEX_EXECUTION_SETTLEMENT_LEASE_SECONDS=90",
    "CEX_EXECUTION_SETTLEMENT_REQUEST_TIMEOUT_SECONDS=20",
)
require(
    "ops/systemd/cex-execution-settlement-worker.service",
    "NoNewPrivileges=true",
    "ProtectSystem=strict",
)

canonical_migration_head: str | None = None
manifest_raw = read("docs/templates/cex-release-baseline-manifest-v1.json")
if manifest_raw:
    try:
        manifest = json.loads(manifest_raw)
        canonical_migration_head = str(manifest["database"]["migration_head"])
    except (json.JSONDecodeError, KeyError, TypeError) as error:
        PROBLEMS.append(f"cannot decode release manifest template: {error}")
    else:
        actual_number = migration_number(canonical_migration_head)
        slice_number = migration_number(SETTLEMENT_SLICE_HEAD)
        if actual_number is None:
            PROBLEMS.append(
                f"release manifest migration_head={canonical_migration_head!r} is not a numbered migration"
            )
        elif slice_number is None or actual_number < slice_number:
            PROBLEMS.append(
                "release manifest canonical migration head predates the complete settlement slice: "
                f"{canonical_migration_head!r} < {SETTLEMENT_SLICE_HEAD!r}"
            )
        elif not (ROOT / "migrations" / Path(canonical_migration_head).name).is_file():
            PROBLEMS.append(
                f"release manifest canonical migration file does not exist: {canonical_migration_head}"
            )

result = {
    "status": "failed" if PROBLEMS else "ok",
    "settlement_slice_head": SETTLEMENT_SLICE_HEAD,
    "canonical_migration_head": canonical_migration_head,
    "api_adapter_activated": "settle_invocation(" in api if api else None,
    "problems": PROBLEMS,
}
print(json.dumps(result, ensure_ascii=False, indent=2))
raise SystemExit(1 if PROBLEMS else 0)
