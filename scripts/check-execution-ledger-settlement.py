#!/usr/bin/env python3
"""Static safety gate for the Execution exact Ledger settlement adapter."""

from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
problems: list[str] = []


def read(path: str) -> str:
    file_path = ROOT / path
    if not file_path.is_file():
        problems.append(f"missing required file: {path}")
        return ""
    return file_path.read_text(encoding="utf-8")


adapter = read("services/execution-service/src/ledger_settlement.rs")
lib = read("services/execution-service/src/lib.rs")
api = read("services/execution-service/src/api.rs")
doc = read("docs/execution-ledger-settlement-v1.md")
plan = read("docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v10.md")

for marker in (
    "ExecutionLedgerMode",
    "SettlementOutcome",
    "cex_invocation_ledger_effect_request_v1",
    "/v2/ledger/effects",
    "RetryableExactReplay",
    "ReconcileRequired",
    "ledger_v2_timeout_unknown_outcome",
    "validate_success_receipt",
):
    if marker not in adapter:
        problems.append(f"Execution settlement adapter lacks {marker}")

if "pub mod ledger_settlement;" not in lib:
    problems.append("Execution library does not export ledger_settlement")

for forbidden in (
    r"\bf64\b",
    r"reserve_amount",
    r"\*\s*1_?000_?000",
    r"\bas\s+i64\b",
    r"\.round\s*\(",
):
    if re.search(forbidden, adapter):
        problems.append(f"Execution exact adapter contains forbidden legacy-money bridge: {forbidden}")

# The adapter is intentionally not authoritative while provider/Ledger network calls remain
# inside the monolithic SQL transaction in api.rs.
if "settle_invocation(" in api:
    problems.append(
        "Execution api.rs activates the exact adapter before command/receipt transaction separation"
    )

for marker in (
    "not yet called by api.rs",
    "unknown remote outcome",
    "legacy_v1",
    "require_v2",
):
    if marker not in doc:
        problems.append(f"Execution settlement documentation lacks {marker}")

if "P0-N5 transaction separation" not in plan:
    problems.append("active plan does not preserve P0-N5 transaction separation blocker")

print(json.dumps({
    "status": "failed" if problems else "ok",
    "adapter_exported": "pub mod ledger_settlement;" in lib,
    "adapter_activated_in_api": "settle_invocation(" in api,
    "problems": problems,
}, ensure_ascii=False, indent=2))
raise SystemExit(1 if problems else 0)
