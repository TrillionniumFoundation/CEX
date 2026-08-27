#!/usr/bin/env python3
"""Fast static wiring checks for the CEX P0 production-baseline branch."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROBLEMS: list[str] = []


def read_text(relative_path: str) -> str:
    path = ROOT / relative_path
    if not path.is_file():
        PROBLEMS.append(f"missing required file: {relative_path}")
        return ""
    try:
        return path.read_text(encoding="utf-8")
    except UnicodeDecodeError as error:
        PROBLEMS.append(f"required UTF-8 file cannot be decoded: {relative_path}: {error}")
        return ""


def require_text(relative_path: str, *needles: str) -> None:
    content = read_text(relative_path)
    for needle in needles:
        if needle not in content:
            PROBLEMS.append(f"{relative_path} lacks required marker: {needle}")


def forbid_path(relative_path: str) -> None:
    if (ROOT / relative_path).exists():
        PROBLEMS.append(f"obsolete/conflicting path must not exist: {relative_path}")


def latest_migration() -> tuple[str, str]:
    migrations = sorted(
        path
        for path in (ROOT / "migrations").glob("[0-9][0-9][0-9][0-9]_*.sql")
        if path.is_file()
    )
    if not migrations:
        PROBLEMS.append("no numbered SQL migrations found")
        return "", ""
    numbers: dict[str, list[str]] = {}
    for path in migrations:
        match = re.match(r"^(?P<number>\d{4})_", path.name)
        if match is None:
            PROBLEMS.append(f"invalid numbered migration filename: {path.name}")
            continue
        numbers.setdefault(match.group("number"), []).append(path.name)
    for number, names in sorted(numbers.items()):
        if len(names) > 1:
            PROBLEMS.append(
                f"duplicate migration number {number}: {', '.join(sorted(names))}"
            )
    latest = migrations[-1]
    match = re.match(r"^(?P<number>\d{4})_", latest.name)
    return (match.group("number") if match else "", latest.name)


def verify_release_template(expected_filename: str) -> None:
    relative_path = "docs/templates/cex-release-baseline-manifest-v1.json"
    raw = read_text(relative_path)
    if not raw:
        return
    try:
        document = json.loads(raw)
    except json.JSONDecodeError as error:
        PROBLEMS.append(f"invalid release manifest template JSON: {error}")
        return
    recorded = document.get("database", {}).get("migration_head")
    if recorded != expected_filename:
        PROBLEMS.append(
            f"release manifest database.migration_head={recorded!r}, expected {expected_filename!r}"
        )


def verify_core_startup_wiring() -> None:
    for relative_path in (
        "services/gateway-service/src/main.rs",
        "services/identity-service/src/main.rs",
        "services/ledger-service/src/main.rs",
        "services/execution-service/src/main.rs",
        "services/audit-service/src/main.rs",
    ):
        require_text(relative_path, "runtime_guard::enforce")


def verify_canonical_audit_chain() -> None:
    for obsolete in (
        "migrations/0060_add_audit_outbox_delivery_schema.sql",
        "migrations/0061_add_execution_transactional_audit_outbox.sql",
        "migrations/0062_add_identity_transactional_audit_outbox.sql",
    ):
        forbid_path(obsolete)
    require_text(
        "migrations/0064_add_audit_source_baseline_backfill.sql",
        "cex_backfill_audit_source_baseline_v1",
        "execution.persisted.baseline",
        "identity.api_key.persisted.baseline",
    )


def verify_ledger_operation_identity() -> None:
    require_text(
        "migrations/0065_add_ledger_operation_identity.sql",
        "cex_apply_ledger_effect_v1",
        "idx_ledger_entries_scoped_idempotency_v1",
        "ledger.effect.persisted",
        "cex_ledger_operation_identity_status_v1",
        "ledger_entries is append-only",
    )
    require_text(
        "scripts/check-ledger-operation-identity-postgres.sh",
        "exact replay did not return original effect",
        "same key in a different scope was rejected",
        "ledger entry delete was not rejected",
    )
    require_text(
        "services/ledger-service/src/ledger_effects.rs",
        "LedgerEffectRequestV1",
        "cex_apply_ledger_effect_v1",
        "deterministic_operation_id",
        "LEDGER_V2_REQUIRE_EXPLICIT_TRACE",
        "ledger_operation_collision",
        "TRACE_RESULT_LIMIT",
    )
    require_text(
        "services/ledger-service/src/lib.rs",
        '"/v2/ledger/effects"',
        '"/v2/ledger/effects/:operation_id"',
        '"/v2/ledger/traces/:trace_id"',
    )
    require_text(
        "services/ledger-service/src/main.rs",
        "new_with_operation_pool",
        "repo.pool.clone()",
    )
    require_text(
        "services/ledger-service/src/state.rs",
        "operation_pool: Option<PgPool>",
        "require_explicit_ledger_trace",
    )
    require_text(
        "config/ledger-operation-v1.production.env.example",
        "LEDGER_V2_REQUIRE_EXPLICIT_TRACE=true",
    )
    require_text(
        "docs/ledger-operation-api-v1.md",
        "POST /v2/ledger/effects",
        "exact replay",
        "Stable error categories",
        "Rollout boundary",
    )


def verify_gate_wiring() -> None:
    require_text(
        ".github/workflows/rust-service-gate.yml",
        "scripts/check-p0-wiring.py",
        "cargo fmt --all --check",
    )
    require_text(
        ".github/workflows/p0-migration-gate.yml",
        "scripts/check-p0-migrations-postgres.sh",
        "scripts/check-audit-source-baseline-postgres.sh",
        "scripts/check-ledger-operation-identity-postgres.sh",
    )


def verify_plan() -> None:
    require_text(
        "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v7.md",
        "0065_add_ledger_operation_identity.sql",
        "P0-N1 caller migration and cutover controls",
        "P0-N2 Genesis-as-entry",
    )


def main() -> int:
    migration_number, migration_filename = latest_migration()
    verify_release_template(migration_filename)
    verify_core_startup_wiring()
    verify_canonical_audit_chain()
    verify_ledger_operation_identity()
    verify_gate_wiring()
    verify_plan()
    result = {
        "status": "failed" if PROBLEMS else "ok",
        "migration_number": migration_number,
        "migration_head": migration_filename,
        "checks": 6,
        "problems": PROBLEMS,
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 1 if PROBLEMS else 0


if __name__ == "__main__":
    sys.exit(main())
