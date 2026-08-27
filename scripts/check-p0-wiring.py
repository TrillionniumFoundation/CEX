#!/usr/bin/env python3
"""Fast static wiring checks for the CEX P0 production-baseline branch.

This gate intentionally uses only the Python standard library. It catches
high-value integration mistakes before Rust compilation or PostgreSQL service
startup: source-shared modules that are not included, alternate library entry
points that are not selected, production variables that are absent, route
aliases that diverge, and release evidence that points at an old migration.
"""

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
            PROBLEMS.append(f"{relative_path} is not wired to required marker: {needle}")


def latest_migration_head() -> str:
    migrations = sorted(
        path
        for path in (ROOT / "migrations").glob("[0-9][0-9][0-9][0-9]_*.sql")
        if path.is_file()
    )
    if not migrations:
        PROBLEMS.append("no numbered SQL migrations found")
        return ""
    match = re.match(r"^(\d{4})_", migrations[-1].name)
    if match is None:
        PROBLEMS.append(f"latest migration has invalid name: {migrations[-1].name}")
        return ""
    return match.group(1)


def verify_release_template(migration_head: str) -> None:
    relative_path = "docs/templates/cex-release-baseline-manifest-v1.json"
    raw = read_text(relative_path)
    if not raw:
        return
    try:
        document = json.loads(raw)
    except json.JSONDecodeError as error:
        PROBLEMS.append(f"invalid release manifest template JSON: {error}")
        return
    recorded = document.get("source", {}).get("migration_head")
    if recorded != migration_head:
        PROBLEMS.append(
            f"release manifest migration_head={recorded!r} does not match latest {migration_head!r}"
        )


def verify_core_startup_wiring() -> None:
    core_mains = [
        "services/gateway-service/src/main.rs",
        "services/identity-service/src/main.rs",
        "services/ledger-service/src/main.rs",
        "services/execution-service/src/main.rs",
        "services/audit-service/src/main.rs",
    ]
    for relative_path in core_mains:
        require_text(
            relative_path,
            "crates/shared-config/src/runtime_guard.rs",
            "runtime_guard::enforce",
        )

    require_text(
        "services/identity-service/Cargo.toml",
        '[lib]',
        'path = "src/lib_entry.rs"',
    )
    require_text(
        "services/identity-service/src/lib_entry.rs",
        'include!("lib.rs")',
        "harden_runtime_state",
        "install_internal_http_client",
    )


def verify_workload_identity_wiring() -> None:
    require_text(
        "crates/shared-config/src/service_auth.rs",
        "execution_create_from_env",
        "audit_write_from_env",
        "identity_resolve_from_env",
        "AuthenticatedService",
    )
    require_text(
        "crates/shared-config/src/service_client.rs",
        "build_internal_http_client",
        "x-cex-service-id",
        "x-cex-service-token",
    )
    require_text(
        "services/gateway-service/src/main.rs",
        "service_client.rs",
        'build_internal_http_client("gateway-service"',
    )
    require_text(
        "services/identity-service/src/main.rs",
        "service_client.rs",
        "require_identity_resolve_auth",
        "harden_runtime_state",
    )
    require_text(
        "services/execution-service/src/main.rs",
        "service_client.rs",
        'build_internal_http_client("execution-service"',
        "validate_internal_service_auth",
    )
    require_text(
        "services/execution-service/src/lib.rs",
        'route("/v1/executions"',
        "require_service_auth",
    )
    require_text(
        "services/audit-service/src/main.rs",
        "validate_internal_service_auth",
    )


def verify_audit_v2_routes() -> None:
    require_text(
        "services/audit-service/src/lib.rs",
        'route("/v2/audit/events"',
        'route("/v1/audit/events/v2"',
        '"/v2/audit/events/trace/:trace_id"',
        '"/v1/audit/events/v2/trace/:trace_id"',
        'route("/v2/audit/metrics"',
        'route("/metrics/audit-v2"',
    )
    require_text(
        "services/audit-service/src/v2.rs",
        "authenticated_writer_required",
        "cex_append_audit_event_v2",
        "writer.service_id",
    )
    require_text(
        "docs/audit-integrity-v2.md",
        "POST /v2/audit/events",
        "GET /v2/audit/events/trace/:trace_id",
        "GET /v2/audit/metrics",
    )


def verify_production_environment_contract() -> None:
    require_text(
        ".env.production.example",
        "GATEWAY_FAIL_FAST=true",
        "IDENTITY_FAIL_FAST=true",
        "LEDGER_FAIL_FAST=true",
        "EXECUTION_FAIL_FAST=true",
        "AUDIT_FAIL_FAST=true",
        "CEX_INTERNAL_SERVICE_AUTH_MODE=enforce",
        '"gateway-service"',
        '"identity-service"',
        '"execution-service"',
    )


def verify_gate_wiring() -> None:
    require_text(
        ".github/workflows/rust-service-gate.yml",
        "scripts/check-p0-wiring.py",
        "scripts/check-release-baseline-manifest.py",
        "cargo fmt --all --check",
    )
    require_text(
        ".github/workflows/p0-migration-gate.yml",
        "scripts/check-p0-migrations-postgres.sh",
    )
    require_text(
        "scripts/check-p0-migrations.py",
        "0055",
        "0059",
    )
    require_text(
        "scripts/check-p0-migrations-postgres.sh",
        "cex_append_audit_event_v2",
        "cex_claim_saga_commands_v1",
    )


def main() -> int:
    migration_head = latest_migration_head()
    verify_release_template(migration_head)
    verify_core_startup_wiring()
    verify_workload_identity_wiring()
    verify_audit_v2_routes()
    verify_production_environment_contract()
    verify_gate_wiring()

    result = {
        "status": "failed" if PROBLEMS else "ok",
        "migration_head": migration_head,
        "checks": 6,
        "problems": PROBLEMS,
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 1 if PROBLEMS else 0


if __name__ == "__main__":
    sys.exit(main())
