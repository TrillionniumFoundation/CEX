#!/usr/bin/env python3
"""Fast fail-closed static wiring checks for the active CEX P0 v12 candidate."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROBLEMS: list[str] = []
ACTIVE_PLAN = "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md"
ACTIVE_ADDENDUM = "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md"
DOC_CHECKER = "scripts/check-development-docs.py"
SHARED_TRIGGER = "docs/release-evidence/p0-candidate-trigger.json"
MIGRATION_HEAD = "0087_add_term_exchange_receipt_event_history.sql"
AUTHORITATIVE_WORKFLOWS = (
    ".github/workflows/p0-migration-gate.yml",
    ".github/workflows/rust-service-gate.yml",
    ".github/workflows/p0-gateway-exact-reserve-gate.yml",
    ".github/workflows/p0-execution-settlement-gate.yml",
    ".github/workflows/p0-provider-reconciliation-gate.yml",
)
RELEASE_WORKFLOW = ".github/workflows/p0-release-candidate-gate.yml"


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


def forbid_text(relative_path: str, *needles: str) -> None:
    content = read_text(relative_path)
    for needle in needles:
        if needle in content:
            PROBLEMS.append(f"{relative_path} contains forbidden marker: {needle}")


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


def verify_candidate_trigger() -> None:
    raw = read_text(SHARED_TRIGGER)
    if not raw:
        return
    try:
        trigger = json.loads(raw)
    except json.JSONDecodeError as error:
        PROBLEMS.append(f"invalid P0 candidate trigger JSON: {error}")
        return
    if trigger.get("schema") != "cex.p0-candidate-trigger.v1":
        PROBLEMS.append("candidate trigger schema is not cex.p0-candidate-trigger.v1")
    if trigger.get("plan") != Path(ACTIVE_PLAN).name:
        PROBLEMS.append("candidate trigger is not bound to the active v12 plan")
    if not isinstance(trigger.get("sequence"), int) or trigger["sequence"] < 1:
        PROBLEMS.append("candidate trigger sequence must be a positive integer")
    if trigger.get("production_authorization") != "not_granted":
        PROBLEMS.append("candidate trigger must explicitly deny production authorization")


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

    require_text(
        "services/gateway-service/src/infrastructure/state.rs",
        '"trnm-economy"',
        '"trnm_economy"',
        "legacy reserve remains fail-closed",
    )

    trnm_launcher = "scripts/run-trnm-economy-service.sh"
    require_text(
        trnm_launcher,
        "TRNM_SECRET_ENV_NAMES=(",
        "require_distinct_secrets",
        "TRNM_ENTITLEMENT_ISSUER_REGISTRY_PATH is required",
        "must be an absolute mounted path",
        "LEDGER_ADMIN_TOKEN",
        "TRNM_VALUE_ENTITLEMENT_SIGNING_SECRET",
        "TRNM_GAME_AUTHORITY_TOKEN",
        "TRNM_PLAYER_SESSION_SIGNING_SECRET",
        "CONSUMER_ENTRY_INGRESS_TOKEN",
        "CONSUMER_ENTRY_SESSION_AUTH_SECRET",
        "CEX_GATEWAY_API_KEY",
        "CONSUMER_ENTRY_LEAGUE_WEB_SESSION_SECRET",
    )
    forbid_text(
        trnm_launcher,
        "trnm-economy-local-production-key",
        ":-$IDENTITY_ADMIN_TOKEN",
        "trnm-entitlement-signing-v1:$IDENTITY_ADMIN_TOKEN",
        "../trillionnium-world/run/online-authority/issuer-registry.json",
    )

    require_text(
        "deploy/systemd/cex-trnm-economy-maintenance.service",
        "WorkingDirectory=%h/.openclaw/workspace/CEX",
        "ExecStart=%h/.openclaw/workspace/CEX/scripts/run-trnm-economy-maintenance.sh",
    )
    forbid_text(
        "deploy/systemd/cex-trnm-economy-maintenance.service",
        "/home/alex/",
    )


def verify_exact_contracts() -> None:
    # The normalized runtime probe must honor an explicitly supplied
    # DATABASE_URL even though it loads the repository's .env for the rest of
    # its local defaults.  Keep this contract visible to the static gate so a
    # future refactor cannot silently redirect a CI/operator run to cex_ai.
    require_text(
        "scripts/check-trillionnium-league-normalized-runtime-dual-write.sh",
        "NORMALIZED_RUNTIME_CALLER_DATABASE_URL=\"${DATABASE_URL-}\"",
        "NORMALIZED_RUNTIME_CALLER_DATABASE_URL_SET=0",
        "cex_load_env",
        'if [[ \"$NORMALIZED_RUNTIME_CALLER_DATABASE_URL_SET\" == \"1\" ]]; then',
        'export DATABASE_URL=\"$NORMALIZED_RUNTIME_CALLER_DATABASE_URL\"',
    )
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
        "scripts/check-consumer-exact-money.py",
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

    runtime_profile_test = ROOT / "scripts/test-runtime-profile-wiring.sh"
    try:
        result = subprocess.run(
            ["bash", str(runtime_profile_test)],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
    except OSError as error:
        PROBLEMS.append(f"cannot execute scripts/test-runtime-profile-wiring.sh: {error}")
    else:
        if result.returncode != 0:
            PROBLEMS.append(
                "scripts/test-runtime-profile-wiring.sh failed: "
                + result.stdout.strip()
            )


def verify_development_documents() -> None:
    result = subprocess.run(
        [sys.executable, str(ROOT / DOC_CHECKER)],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if result.returncode != 0:
        PROBLEMS.append(f"development-document contract failed: {result.stdout.strip()}")


def verify_gates_and_plan() -> None:
    require_text(
        "scripts/check-repository-integrity.py",
        '"commit_sha": commit_sha',
        '"tree_sha": tree_sha',
    )
    require_text(
        ".github/workflows/rust-service-gate.yml",
        "scripts/check-p0-wiring.py",
        "scripts/check-development-docs.py",
        "scripts/check-repository-integrity.py",
        "scripts/check-hepta-postgres-integration.sh",
        "scripts/test-runtime-profile-wiring.sh",
        "repository-integrity:",
        "hepta-postgres-integration:",
        "cargo fmt --all --check",
        "cargo check --locked --workspace --all-targets",
        SHARED_TRIGGER,
    )
    require_text(
        ".github/workflows/p0-migration-gate.yml",
        "scripts/check-invocation-ledger-terminal-postgres.sh",
        "scripts/check-ledger-operation-identity-postgres.sh",
        "scripts/check-trnm-economy-settlement-contract.py",
        "scripts/test-trnm-economy-settlement-status-negative.py",
        "scripts/test-runtime-profile-wiring.sh",
        SHARED_TRIGGER,
    )
    require_text(
        ".github/workflows/p0-gateway-exact-reserve-gate.yml",
        "scripts/check-gateway-exact-reserve-postgres.sh",
        SHARED_TRIGGER,
    )
    require_text(
        ".github/workflows/p0-execution-settlement-gate.yml",
        "scripts/check-execution-settlement-commands-postgres.sh",
        SHARED_TRIGGER,
    )
    require_text(
        ".github/workflows/p0-provider-reconciliation-gate.yml",
        "scripts/check-provider-reconciliation-postgres.sh",
        SHARED_TRIGGER,
    )
    require_text(
        RELEASE_WORKFLOW,
        "scripts/p0-release-evidence.py collect",
        "scripts/check-p0-exact-ledger-soak-postgres.sh",
        "scripts/check-p0-backup-restore-postgres.sh",
        "scripts/check-development-docs.py",
        "scripts/check-repository-integrity.py",
        "scripts/check-hepta-postgres-integration.sh --mode recovery-only",
        "scripts/check-trnm-economy-settlement-contract.py",
        "scripts/test-trnm-economy-settlement-status-negative.py",
        "scripts/test-runtime-profile-wiring.sh",
        SHARED_TRIGGER,
    )
    require_text(
        ACTIVE_PLAN,
        "Status: active all-blocker closure candidate; **not production-ready**.",
        f"Candidate migration head: `{MIGRATION_HEAD}`.",
        "External gates that repository edits cannot self-certify",
        "Definition of repository closure",
    )
    require_text(
        ACTIVE_ADDENDUM,
        "Block H",
        "Block I",
        "REPOSITORY_CLOSED_CANDIDATE",
        "External production gates remain upstream blockers",
    )
    require_text(
        "services/hepta-research-league/tests/postgres_recovery.rs",
        "HEPTA_REQUIRE_POSTGRES_TESTS",
        "strict PostgreSQL integration test",
    )

    for workflow in (*AUTHORITATIVE_WORKFLOWS, RELEASE_WORKFLOW):
        require_text(workflow, SHARED_TRIGGER)


def main() -> int:
    migration_number, migration_filename = latest_migration()
    if migration_filename and migration_filename != MIGRATION_HEAD:
        PROBLEMS.append(
            f"active v12 migration head {MIGRATION_HEAD!r} != repository head {migration_filename!r}"
        )
    verify_release_template(migration_filename)
    verify_candidate_trigger()
    verify_core()
    verify_exact_contracts()
    verify_development_documents()
    verify_gates_and_plan()
    result = {
        "status": "failed" if PROBLEMS else "ok",
        "plan": Path(ACTIVE_PLAN).name,
        "addendum": Path(ACTIVE_ADDENDUM).name,
        "migration_number": migration_number,
        "migration_head": migration_filename,
        "checks": 6,
        "problems": PROBLEMS,
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 1 if PROBLEMS else 0


if __name__ == "__main__":
    sys.exit(main())
