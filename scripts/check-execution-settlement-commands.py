#!/usr/bin/env python3
"""Static fail-closed gate for P0-N5 durable Execution Ledger settlement."""

from __future__ import annotations

import json
import os
import re
import sys
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


migration = require(
    "migrations/0067_add_execution_ledger_settlement_commands.sql",
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
)
if migration and not migration.strip().lower().startswith("begin;"):
    PROBLEMS.append("0067 migration must begin with BEGIN")
if migration and not migration.strip().lower().endswith("commit;"):
    PROBLEMS.append("0067 migration must end with COMMIT")
# The marker is compared case-insensitively because SQL formatting is not normative.
if "for update skip locked" not in migration.lower():
    PROBLEMS.append("0067 claim function lacks FOR UPDATE SKIP LOCKED")

worker = require(
    "services/execution-service/src/settlement_worker.rs",
    "cex_claim_execution_ledger_settlements_v1",
    "settle_invocation",
    "cex_finish_execution_ledger_settlement_v1",
    "Policy::none()",
    "validate_serial_lease_budget",
    "retry_budget_exhausted_unknown_outcome",
    "claim is intentionally left for lease recovery",
    "CEX_EXECUTION_SETTLEMENT_WORKER_ID",
    "CEX_EXECUTION_LEDGER_MODE=dual or require_v2",
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
    "claim transaction commits",
    "reconcile_required",
    "fresh explicit acknowledgement",
)
require(
    "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v11.md",
    "P0-N5 delivered by this candidate",
    "Gateway exact registration and reserve",
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

manifest_raw = read("docs/templates/cex-release-baseline-manifest-v1.json")
if manifest_raw:
    try:
        manifest = json.loads(manifest_raw)
        actual = manifest["database"]["migration_head"]
    except (json.JSONDecodeError, KeyError, TypeError) as error:
        PROBLEMS.append(f"cannot decode release manifest template: {error}")
    else:
        expected = "0067_add_execution_ledger_settlement_commands.sql"
        if actual != expected:
            PROBLEMS.append(f"release manifest migration_head={actual!r}, expected {expected!r}")

result = {
    "status": "failed" if PROBLEMS else "ok",
    "migration_head": "0067_add_execution_ledger_settlement_commands.sql",
    "api_adapter_activated": "settle_invocation(" in api if api else None,
    "problems": PROBLEMS,
}
print(json.dumps(result, ensure_ascii=False, indent=2))
raise SystemExit(1 if PROBLEMS else 0)
