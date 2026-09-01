#!/usr/bin/env python3
"""Static contract checks for migration 0066 and its rollout evidence."""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
problems: list[str] = []


def require(path: str, *markers: str) -> None:
    file_path = ROOT / path
    if not file_path.is_file():
        problems.append(f"missing required file: {path}")
        return
    content = file_path.read_text(encoding="utf-8")
    for marker in markers:
        if marker not in content:
            problems.append(f"{path} lacks required marker: {marker}")


require(
    "migrations/0066_add_invocation_ledger_contract.sql",
    "cex_invocation_ledger_contracts_v1",
    "cex_register_invocation_ledger_contract_v1",
    "cex_invocation_ledger_effect_request_v1",
    "cex_bind_invocation_ledger_effect_v1",
    "invocation.ledger_contract.registered",
    "amount_minor::text",
    "missing_effect_evidence",
)
require(
    "scripts/check-invocation-ledger-contract-postgres.sh",
    "exact registration replay failed",
    "contract collision was not rejected",
    "reserve did not advance contract state",
    "consume did not terminally settle contract",
    "wrong operation identity did not fail closed",
)
require(
    "scripts/check-invocation-ledger-terminal-postgres.sh",
    "refund-after-consume terminal transition was not rejected",
    "consume-after-refund terminal transition was not rejected",
    "rejection changed durable state",
    "terminal settlement evidence is inconsistent",
    "P0 Invocation Ledger terminal exclusivity gate passed",
)
require(
    "docs/invocation-ledger-contract-v1.md",
    "reserved-value leak",
    "registered -> reserved -> consumed/refunded",
    "independent terminal-exclusivity probe",
    "Execution",
)
require(
    "docs/templates/cex-release-baseline-manifest-v1.json",
    # The v12 manifest template now has one canonical, ordered
    # `migration-and-lifecycle-matrix` evidence record rather than a separate
    # evidence item for every migration.  Migration 0066 remains checked
    # directly above; bind the template assertion to its canonical aggregate
    # record and active head instead of requiring a stale filename literal in
    # the JSON envelope.
    "migration-and-lifecycle-matrix",
    "0088_enforce_provider_terminal_evidence_binding.sql",
)

print(json.dumps({
    "status": "failed" if problems else "ok",
    "problems": problems,
}, ensure_ascii=False, indent=2))
raise SystemExit(1 if problems else 0)
