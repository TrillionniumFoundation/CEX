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
plan = read("docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md")

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

# The adapter is authoritative only from the durable settlement worker. The
# public API must not retain a provider/Ledger network call inside its status
# transaction; provider-backed requests enter through provider_dispatch and the
# 0074 trigger owns exact terminal command creation.
if "settle_invocation(" in api:
    problems.append(
        "Execution api.rs activates the exact adapter before command/receipt transaction separation"
    )

for marker in (
    "not yet called by api.rs",
    "durable settlement worker",
    "unknown remote outcome",
    "legacy_v1",
    "require_v2",
):
    if marker not in doc:
        problems.append(f"Execution settlement documentation lacks {marker}")

for marker in (
    "P0-N5 durable Execution settlement",
    "P0-N6 delivered by this candidate",
    "not production-ready",
    "Definition of repository closure",
):
    if marker not in plan:
        problems.append(f"active v12 plan lacks required settlement marker: {marker}")

for forbidden in ("/v1/ledger/", "call_ledger_action(", "consume_reserved_credits(", "release_reserved_credits("):
    if forbidden in api:
        problems.append(f"Execution API retains retired settlement marker: {forbidden}")

print(json.dumps({
    "status": "failed" if problems else "ok",
    "adapter_exported": "pub mod ledger_settlement;" in lib,
    "adapter_activated_in_api": "settle_invocation(" in api,
    "problems": problems,
}, ensure_ascii=False, indent=2))
raise SystemExit(1 if problems else 0)
