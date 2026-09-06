#!/usr/bin/env python3
"""Fail closed if an authoritative money path regresses to floating point."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "docs/compatibility/authoritative-money-isolation-v1.json"
PROBLEMS: list[str] = []
FLOAT = re.compile(r"\bf(?:32|64)\b")
LOSSY = (
    re.compile(r"\.round\s*\("),
    re.compile(r"\.trunc\s*\("),
    re.compile(r"\bas\s+i(?:32|64|128)\b"),
)


def read(relative: str) -> str:
    path = ROOT / relative
    if not path.is_file():
        PROBLEMS.append(f"missing required file: {relative}")
        return ""
    return path.read_text(encoding="utf-8")


try:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
except (FileNotFoundError, UnicodeDecodeError, json.JSONDecodeError) as error:
    print(json.dumps({"status": "failed", "problems": [str(error)]}, indent=2))
    raise SystemExit(1)

if manifest.get("schema") != "cex.authoritative-money-isolation.v1":
    PROBLEMS.append("money isolation schema is invalid")
if manifest.get("status") != "active":
    PROBLEMS.append("money isolation status must be active")
if manifest.get("production_authorization") != "not_granted":
    PROBLEMS.append("money isolation manifest must deny production authorization")

exact_paths = manifest.get("authoritative_exact_files")
if not isinstance(exact_paths, list) or not exact_paths:
    PROBLEMS.append("authoritative_exact_files must be a nonempty list")
    exact_paths = []

for relative in exact_paths:
    if not isinstance(relative, str):
        PROBLEMS.append("authoritative exact path must be a string")
        continue
    text = read(relative)
    if FLOAT.search(text):
        PROBLEMS.append(f"authoritative money file contains binary float: {relative}")
    for line_number, line in enumerate(text.splitlines(), start=1):
        if not re.search(r"(?i)amount|balance|reserve|credit|price|cost|reward|money", line):
            continue
        for pattern in LOSSY:
            if pattern.search(line):
                PROBLEMS.append(
                    f"authoritative money file contains potentially lossy conversion "
                    f"{pattern.pattern}: {relative}:{line_number}"
                )

legacy = manifest.get("legacy_value_float_allowlist")
if not isinstance(legacy, list) or not legacy:
    PROBLEMS.append("legacy_value_float_allowlist must be a nonempty list")
    legacy = []
legacy_paths: set[str] = set()
for index, item in enumerate(legacy):
    if not isinstance(item, dict):
        PROBLEMS.append(f"legacy allowlist entry {index} must be an object")
        continue
    relative = item.get("path")
    classification = item.get("classification")
    if not isinstance(relative, str) or not relative:
        PROBLEMS.append(f"legacy allowlist entry {index} lacks path")
        continue
    if relative in legacy_paths:
        PROBLEMS.append(f"duplicate legacy money path: {relative}")
    legacy_paths.add(relative)
    if relative in exact_paths:
        PROBLEMS.append(f"path cannot be exact and legacy: {relative}")
    if not isinstance(classification, str) or not classification:
        PROBLEMS.append(f"legacy money path lacks classification: {relative}")
    text = read(relative)
    if text and not FLOAT.search(text):
        PROBLEMS.append(
            f"legacy allowlist entry no longer contains a binary float and should be removed: {relative}"
        )

shared = read("crates/shared-types/src/ledger_v2.rs")
gateway = read("services/gateway-service/src/infrastructure/ledger_v2_client.rs")
execution = read("services/execution-service/src/ledger_settlement.rs")
runtime_guard = read("crates/shared-config/src/runtime_guard.rs")
ledger_main = read("services/ledger-service/src/main.rs")
compatibility = read("docs/protocol/version-compatibility-matrix-v1.md")
module = read("services/ledger-service/MODULE.md")

for marker in ("amount_minor", "currency_scale", "i64_string", "LedgerOperationKind"):
    if marker not in shared:
        PROBLEMS.append(f"Ledger v2 contract lacks exact marker: {marker}")
for marker in ("MoneyAmount", "apply_ledger_effect_v2", "CEX_GATEWAY_LEDGER_MODE"):
    if marker not in gateway:
        PROBLEMS.append(f"Gateway exact reserve path lacks marker: {marker}")
for marker in ("LedgerEffectRequestV1", "ReconcileRequired", "CEX_EXECUTION_LEDGER_MODE"):
    if marker not in execution:
        PROBLEMS.append(f"Execution exact settlement path lacks marker: {marker}")
for marker in ('Self::Ledger => "LEDGER_FAIL_FAST"', "require_explicit_true(service.fail_fast_env())"):
    if marker not in runtime_guard:
        PROBLEMS.append(f"runtime guard lacks production Ledger fail-fast marker: {marker}")
for marker in ("Err(err) if fail_fast", "PostgresLedgerRepository::new_placeholder()"):
    if marker not in ledger_main:
        PROBLEMS.append(f"Ledger startup containment marker missing: {marker}")
for marker in (
    "exact v2 account/effect functions only",
    "Read compatibility never grants write authority",
):
    if marker not in compatibility:
        PROBLEMS.append(f"compatibility contract lacks money authority marker: {marker}")
for marker in ("Legacy in-memory/f64 structures are compatibility/test-only", "TRNM/finality"):
    if marker not in module:
        PROBLEMS.append(f"Ledger module contract lacks isolation marker: {marker}")

result = {
    "schema": "cex.authoritative-money-isolation-check.v1",
    "status": "failed" if PROBLEMS else "ok",
    "authoritative_exact_files": len(exact_paths),
    "legacy_allowlist_files": len(legacy_paths),
    "production_authorization": "not_granted",
    "problems": PROBLEMS,
}
print(json.dumps(result, indent=2, sort_keys=True))
sys.exit(1 if PROBLEMS else 0)
