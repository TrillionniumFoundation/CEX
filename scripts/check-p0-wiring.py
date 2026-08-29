#!/usr/bin/env python3
"""Fast static wiring checks for the CEX P0 production-baseline branch."""

from __future__ import annotations

import json
import re
import subprocess
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
    raw = read_text("docs/templates/cex-release-baseline-manifest-v1.json")
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


def verify_core() -> None:
    for relative_path in (
        "services/gateway-service/src/main.rs",
        "services/identity-service/src/main.rs",
        "services/ledger-service/src/main.rs",
        "services/execution-service/src/main.rs",
        "services/audit-service/src/main.rs",
    ):
        require_text(relative_path, "runtime_guard::enforce")

    for obsolete in (
        "migrations/0060_add_audit_outbox_delivery_schema.sql",
        "migrations/0061_add_execution_transactional_audit_outbox.sql",
        "migrations/0062_add_identity_transactional_audit_outbox.sql",
    ):
        forbid_path(obsolete)

    require_text(
        "migrations/0066_add_invocation_ledger_contract.sql",
        "cex_invocation_ledger_contracts_v1",
        "cex_register_invocation_ledger_contract_v1",
        "cex_invocation_ledger_effect_request_v1",
        "cex_bind_invocation_ledger_effect_v1",
        "missing_effect_evidence",
    )


def verify_exact_contracts() -> None:
    require_text(
        "crates/shared-types/src/ledger_v2.rs",
        "LedgerEffectRequestV1",
        "LedgerOperationKind",
        "i64_string",
        "ExplicitTraceRequired",
    )
    require_text(
        "services/ledger-service/src/ledger_effects.rs",
        "shared_types::ledger_v2",
        "ledger_currency_mismatch",
        "cex_apply_ledger_effect_v1",
    )
    require_text(
        "services/gateway-service/src/infrastructure/ledger_v2_client.rs",
        "CEX_GATEWAY_LEDGER_MODE",
        "apply_ledger_effect_v2",
        "MoneyAmount",
    )
    require_text(
        "services/execution-service/src/ledger_settlement.rs",
        "ExecutionLedgerMode",
        "cex_invocation_ledger_effect_request_v1",
        "RetryableExactReplay",
        "ReconcileRequired",
        "validate_success_receipt",
    )

    for script in (
        "scripts/check-ledger-caller-cutover.py",
        "scripts/check-invocation-ledger-contract-static.py",
        "scripts/check-execution-ledger-settlement.py",
    ):
        try:
            result = subprocess.run(
                [sys.executable, str(ROOT / script)],
                cwd=ROOT,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                check=False,
            )
        except OSError as error:
            PROBLEMS.append(f"cannot execute {script}: {error}")
        else:
            if result.returncode != 0:
                PROBLEMS.append(f"{script} failed: {result.stdout.strip()}")


def verify_gates_and_plan() -> None:
    require_text(
        ".github/workflows/rust-service-gate.yml",
        "scripts/check-p0-wiring.py",
        "cargo fmt --all --check",
    )
    require_text(
        ".github/workflows/p0-migration-gate.yml",
        "scripts/check-invocation-ledger-terminal-postgres.sh",
        "scripts/check-ledger-operation-identity-postgres.sh",
    )
    require_text(
        "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v10.md",
        "P0-N5 transaction separation remains the activation blocker",
        "settlement command and receipt schema",
    )


def main() -> int:
    migration_number, migration_filename = latest_migration()
    verify_release_template(migration_filename)
    verify_core()
    verify_exact_contracts()
    verify_gates_and_plan()
    result = {
        "status": "failed" if PROBLEMS else "ok",
        "migration_number": migration_number,
        "migration_head": migration_filename,
        "checks": 4,
        "problems": PROBLEMS,
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 1 if PROBLEMS else 0


if __name__ == "__main__":
    sys.exit(main())
