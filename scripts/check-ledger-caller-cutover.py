#!/usr/bin/env python3
"""Fail closed when a caller claims Ledger v2 while still bridging from f64."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
problems: list[str] = []


def read(relative: str) -> str:
    path = ROOT / relative
    if not path.is_file():
        problems.append(f"missing required file: {relative}")
        return ""
    return path.read_text(encoding="utf-8")


shared = read("crates/shared-types/src/ledger_v2.rs")
client = read("services/gateway-service/src/infrastructure/ledger_v2_client.rs")
legacy_clients = read("services/gateway-service/src/infrastructure/clients.rs")
invocation = read("services/gateway-service/src/application/invocation_service.rs")
canonical_entry = read("services/gateway-service/src/application/invocation_service_entry.rs")
canonical_http = read("services/gateway-service/src/interfaces/http.rs")
gateway_state = read("services/gateway-service/src/infrastructure/state.rs")
plan = read("docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md")

for marker in (
    "LedgerEffectRequestV1",
    "amount_minor",
    "i64_string",
    "ExplicitTraceRequired",
):
    if marker not in shared:
        problems.append(f"shared Ledger v2 contract lacks {marker}")

for marker in (
    "apply_ledger_effect_v2",
    "invocation_ledger_effect",
    "CEX_GATEWAY_LEDGER_MODE",
    "MoneyAmount",
):
    if marker not in client:
        problems.append(f"Gateway Ledger v2 client lacks {marker}")

for forbidden in (
    r"amount\s*\*\s*1_?000_?000",
    r"amount\s+as\s+i64",
    r"round\s*\(",
):
    if re.search(forbidden, client):
        problems.append(f"Gateway Ledger v2 client contains forbidden f64 bridge: {forbidden}")

legacy_f64_present = "pub amount: f64" in legacy_clients
canonical_called_from_invocation = "apply_ledger_effect_v2" in invocation
if canonical_called_from_invocation and legacy_f64_present:
    problems.append(
        "Invocation orchestration calls Ledger v2 while the authoritative invocation reserve contract is still f64"
    )

entry_guard_offset = canonical_entry.find("if req.has_legacy_reserve()")
entry_legacy_call_offset = canonical_entry.find("legacy::create_invocation")
http_guard_offset = canonical_http.find("if body.has_legacy_reserve()")
http_auth_offset = canonical_http.find("resolve_api_key")
http_service_offset = canonical_http.find("invocation_service::create_invocation")
legacy_reserve_fail_closed = (
    entry_guard_offset >= 0
    and entry_legacy_call_offset > entry_guard_offset
    and http_guard_offset >= 0
    and http_auth_offset > http_guard_offset
    and http_service_offset > http_auth_offset
)
if not legacy_reserve_fail_closed:
    problems.append(
        "canonical Invocation reserve input is not fail-closed before legacy orchestration/upstream calls"
    )

legacy_break_glass_guarded = (
    "env_flag(LEGACY_RESERVE_BREAK_GLASS_ENV, false)" in gateway_state
    and "legacy_reserve_break_glass: bool" in gateway_state
)
if not legacy_break_glass_guarded:
    problems.append(
        "legacy reserve compatibility must be behind an explicit false-by-default break-glass flag"
    )
if "production-like profile" not in gateway_state:
    problems.append(
        "legacy reserve break-glass must be disabled for production-like profiles"
    )

if "caller migration blocked until exact reserve contract" not in plan:
    problems.append("active plan does not record the exact-money caller cutover blocker")

result = {
    "status": "failed" if problems else "ok",
    "legacy_f64_contract_present": legacy_f64_present,
    "canonical_invocation_call_enabled": canonical_called_from_invocation,
    "cutover_ready": not legacy_f64_present and canonical_called_from_invocation,
    "canonical_legacy_reserve_fail_closed": legacy_reserve_fail_closed,
    "legacy_break_glass_default": False,
    "legacy_break_glass_guarded": legacy_break_glass_guarded,
    "problems": problems,
}
print(json.dumps(result, ensure_ascii=False, indent=2))
raise SystemExit(1 if problems else 0)
