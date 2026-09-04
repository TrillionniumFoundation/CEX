#!/usr/bin/env python3
"""Fail closed when Execution lifecycle/auth/settlement verification is incomplete."""

from __future__ import annotations

from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
EXECUTION_ROOT = ROOT / "services/execution-service"
WORKFLOW = ROOT / ".github/workflows/execution-lifecycle-gate.yml"
CONTRACT = ROOT / "docs/execution-lifecycle-verification-v1.md"

REQUIRED_SCRIPTS = [
    "scripts/check-execution-default-state-boundary.py",
    "scripts/check-execution-ledger-settlement.py",
    "scripts/check-execution-settlement-commands.py",
    "scripts/check-provider-success-evidence.py",
    "scripts/check-external-agent-runtime-boundary.py",
    "scripts/check-execution-settlement-commands-postgres.sh",
    "scripts/check-provider-reconciliation-postgres.sh",
]
REQUIRED_MIGRATIONS = [
    "migrations/0067_add_execution_ledger_settlement_schema.sql",
    "migrations/0068_add_execution_ledger_settlement_guards.sql",
    "migrations/0069_add_execution_ledger_settlement_enqueue.sql",
    "migrations/0070_add_execution_ledger_settlement_claim.sql",
    "migrations/0071_add_execution_ledger_settlement_finish.sql",
    "migrations/0072_add_execution_ledger_settlement_operator.sql",
    "migrations/0074_activate_exact_execution_terminal_settlement.sql",
    "migrations/0078_add_durable_provider_dispatch.sql",
    "migrations/0082_close_provider_unknown_outcome_reconciliation.sql",
    "migrations/0084_make_provider_reconciliation_replay_terminal_safe.sql",
    "migrations/0088_enforce_provider_terminal_evidence_binding.sql",
]
REQUIRED_WORKFLOW_COMMANDS = [
    "cargo fmt --all -- --check",
    "cargo test --locked -p execution-service --all-targets",
    "cargo clippy --locked -p execution-service --all-targets -- -D warnings",
    "python3 scripts/check-execution-lifecycle-coverage.py",
    "python3 scripts/check-execution-default-state-boundary.py",
    "python3 scripts/check-execution-ledger-settlement.py",
    "python3 scripts/check-execution-settlement-commands.py",
    "python3 scripts/check-provider-success-evidence.py",
    "python3 scripts/check-external-agent-runtime-boundary.py",
    "bash scripts/check-execution-settlement-commands-postgres.sh",
    "bash scripts/check-provider-reconciliation-postgres.sh",
]

VOCABULARY_GROUPS = {
    "authenticated caller": [
        "service_auth",
        "x-cex-service-id",
        "authorization",
        "internal token",
        "service principal",
    ],
    "tenant binding": ["tenant", "org_id", "organization"],
    "initial lifecycle": ["queued", "pending", "accepted"],
    "claim and lease": ["lease_owner", "lease_expires", "worker_id", "claimed"],
    "attempt budget": ["attempt_count", "max_attempt", "attempt_budget", "retry budget"],
    "provider dispatch identity": [
        "provider_dispatch",
        "dispatch_id",
        "provider_request_id",
        "dispatch identity",
    ],
    "provider success evidence": [
        "success_evidence",
        "evidence_hash",
        "provider evidence",
        "terminal evidence",
    ],
    "unknown outcome reconciliation": [
        "unknown_outcome",
        "reconcile",
        "response loss",
        "possible side effect",
    ],
    "exact settlement": ["ledger_settlement", "consume", "refund", "settlement command"],
    "receipt binding": ["receipt", "operation_id", "intent_hash", "receipt hash"],
    "operator recovery": ["operator", "acknowledge", "requeue", "repair"],
    "external Agent boundary": ["external_agent", "external agent", "local model"],
}

HOSTILE_MARKERS = (
    "unauthorized",
    "wrong_owner",
    "wrong owner",
    "wrong_tenant",
    "cross_tenant",
    "stale",
    "expired",
    "tamper",
    "collision",
    "mismatch",
    "response_loss",
    "unknown_outcome",
    "reconcile",
    "retry_exhaust",
    "missing_evidence",
    "invalid_receipt",
)


def read(path: Path, minimum_bytes: int = 1) -> str:
    if not path.is_file():
        raise AssertionError(f"required file is absent: {path.relative_to(ROOT)}")
    data = path.read_text(encoding="utf-8")
    if len(data.encode("utf-8")) < minimum_bytes:
        raise AssertionError(
            f"required file is unexpectedly small: {path.relative_to(ROOT)}"
        )
    return data


def source_bundle() -> tuple[str, list[Path]]:
    paths = sorted((EXECUTION_ROOT / "src").rglob("*.rs"))
    paths += sorted((EXECUTION_ROOT / "tests").rglob("*.rs"))
    if not paths:
        raise AssertionError("Execution Rust source/test surface is absent")
    return "\n".join(read(path) for path in paths).lower(), paths


def verification_bundle() -> tuple[str, list[Path]]:
    paths = [ROOT / path for path in REQUIRED_SCRIPTS + REQUIRED_MIGRATIONS]
    return "\n".join(read(path, 100) for path in paths).lower(), paths


def require_vocabulary(text: str) -> list[str]:
    problems: list[str] = []
    for label, alternatives in VOCABULARY_GROUPS.items():
        if not any(marker.lower() in text for marker in alternatives):
            problems.append(
                f"Execution implementation/evidence lacks {label}: expected one of {alternatives}"
            )
    return problems


def check_test_surface(text: str, paths: list[Path]) -> list[str]:
    problems: list[str] = []
    test_count = len(re.findall(r"#\s*\[\s*(?:tokio::)?test(?:\s*\([^]]*\))?\s*\]", text))
    if test_count < 10:
        problems.append(
            f"Execution source/test surface exposes only {test_count} test attributes; expected at least 10"
        )
    hostile_count = sum(text.count(marker) for marker in HOSTILE_MARKERS)
    if hostile_count < 12:
        problems.append(
            f"Execution hostile/recovery vocabulary count is {hostile_count}; expected at least 12"
        )
    if not any(path.name == "external_agent_boundary.rs" for path in paths):
        problems.append("Execution external_agent_boundary.rs integration test is absent")
    return problems


def check_workflow() -> list[str]:
    workflow = read(WORKFLOW, 500)
    problems: list[str] = []
    for command in REQUIRED_WORKFLOW_COMMANDS:
        if command not in workflow:
            problems.append(f"execution lifecycle workflow omits command: {command}")
    for marker in (
        "pull_request:",
        "workflow_dispatch:",
        "permissions:",
        "contents: read",
        "runs-on: [self-hosted, linux, x64]",
        "timeout-minutes:",
    ):
        if marker not in workflow:
            problems.append(f"execution lifecycle workflow lacks control: {marker}")
    return problems


def check_contract() -> list[str]:
    contract = read(CONTRACT, 2000)
    problems: list[str] = []
    for marker in (
        "Production authorization: `not_granted`",
        "Verification matrix",
        "Required negative cases",
        "Recovery ordering",
        "Gate wiring",
        "Operational evidence still required",
    ):
        if marker not in contract:
            problems.append(f"Execution verification contract lacks section/marker: {marker}")
    for forbidden in ("TODO", "TBD", "production-ready", "production_authorization=granted"):
        if forbidden in contract:
            problems.append(f"Execution verification contract contains forbidden marker: {forbidden}")
    return problems


def main() -> int:
    problems: list[str] = []
    source, rust_paths = source_bundle()
    evidence, evidence_paths = verification_bundle()
    combined = source + "\n" + evidence

    problems.extend(require_vocabulary(combined))
    problems.extend(check_test_surface(source + "\n" + evidence, rust_paths + evidence_paths))
    problems.extend(check_workflow())
    problems.extend(check_contract())

    if problems:
        print("Execution lifecycle verification coverage: FAILED", file=sys.stderr)
        for problem in problems:
            print(f"- {problem}", file=sys.stderr)
        return 1

    print(
        "Execution lifecycle verification coverage: OK "
        f"({len(rust_paths)} Rust files, {len(evidence_paths)} evidence files)"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, FileNotFoundError, UnicodeDecodeError) as error:
        print(f"Execution lifecycle verification coverage: FAILED: {error}", file=sys.stderr)
        raise SystemExit(1)
