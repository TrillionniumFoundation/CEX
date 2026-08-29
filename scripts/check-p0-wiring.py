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


def forbid_text(relative_path: str, *needles: str) -> None:
    content = read_text(relative_path)
    for needle in needles:
        if needle in content:
            PROBLEMS.append(f"{relative_path} contains forbidden marker: {needle}")


def require_regex(relative_path: str, pattern: str, description: str) -> None:
    content = read_text(relative_path)
    if content and re.search(pattern, content, flags=re.MULTILINE | re.DOTALL) is None:
        PROBLEMS.append(f"{relative_path} lacks required pattern: {description}")


def latest_migration() -> tuple[str, str]:
    migrations = sorted(
        path
        for path in (ROOT / "migrations").glob("[0-9][0-9][0-9][0-9]_*.sql")
        if path.is_file()
    )
    if not migrations:
        PROBLEMS.append("no numbered SQL migrations found")
        return "", ""
    match = re.match(r"^(?P<number>\d{4})_", migrations[-1].name)
    if match is None:
        PROBLEMS.append(f"latest migration has invalid name: {migrations[-1].name}")
        return "", migrations[-1].name
    return match.group("number"), migrations[-1].name


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
            f"release manifest database.migration_head={recorded!r}, "
            f"expected {expected_filename!r}"
        )


def verify_core_startup_wiring() -> None:
    for relative_path in (
        "services/gateway-service/src/main.rs",
        "services/identity-service/src/main.rs",
        "services/ledger-service/src/main.rs",
        "services/execution-service/src/main.rs",
        "services/audit-service/src/main.rs",
    ):
        require_text(
            relative_path,
            "crates/shared-config/src/runtime_guard.rs",
            "runtime_guard::enforce",
        )

    require_text(
        "services/identity-service/Cargo.toml",
        "[lib]",
        'path = "src/lib_entry.rs"',
    )
    require_text(
        "services/identity-service/src/lib_entry.rs",
        'include!("lib.rs")',
        "harden_runtime_state",
        "state.http = internal_http",
        "state.api_keys = Arc::new(HashMap::new())",
    )


def verify_workload_identity() -> None:
    require_text(
        "crates/shared-config/src/service_auth.rs",
        "execution_create_from_env",
        "audit_write_from_env",
        "identity_resolve_from_env",
        "audit-outbox-dispatcher",
        "AuthenticatedService",
    )
    require_text(
        "crates/shared-config/src/service_client.rs",
        "build_internal_http_client",
        "x-cex-service-id",
        "x-cex-service-token",
        "Policy::none()",
    )
    require_regex(
        "services/gateway-service/src/main.rs",
        r'build_internal_http_client\s*\(\s*"gateway-service"',
        'build_internal_http_client("gateway-service", ...)',
    )
    require_regex(
        "services/execution-service/src/main.rs",
        r'build_internal_http_client\s*\(\s*"execution-service"',
        'build_internal_http_client("execution-service", ...)',
    )


def verify_audit_dispatcher() -> None:
    require_text(
        "services/audit-service/Cargo.toml",
        "reqwest.workspace = true",
    )
    require_text(
        "services/audit-service/src/lib.rs",
        "pub mod outbox_dispatcher;",
    )
    require_text(
        "services/audit-service/src/outbox_dispatcher.rs",
        "cex_claim_audit_outbox_v1",
        "cex_mark_audit_outbox_delivered_v1",
        "cex_fail_audit_outbox_delivery_v1",
        "DISPATCHER_SERVICE_ID",
        "/v2/audit/events",
        "invalid_success_receipt",
        "retry_delay_seconds",
        "read_bounded_body",
    )
    require_text(
        "services/audit-service/src/bin/audit-outbox-dispatcher.rs",
        "runtime_guard::enforce",
        "build_internal_http_client",
        "dispatch_once",
        "CEX_AUDIT_OUTBOX_RUN_ONCE",
    )
    require_text(
        "deploy/systemd/cex-audit-outbox-dispatcher.service",
        "audit-outbox-dispatcher",
        "EnvironmentFile=/etc/cex/cex-production.env",
        "NoNewPrivileges=true",
    )
    require_text(
        ".env.production.example",
        '"audit-outbox-dispatcher"',
        "CEX_AUDIT_OUTBOX_LEASE_SECONDS=60",
        "CEX_AUDIT_OUTBOX_REQUEST_TIMEOUT_SECONDS=20",
    )


def verify_audit_v2_routes() -> None:
    require_text(
        "services/audit-service/src/lib.rs",
        'route("/v2/audit/events"',
        'route("/v1/audit/events/v2"',
        '"/v2/audit/events/trace/:trace_id"',
        'route("/v2/audit/metrics"',
    )
    require_text(
        "services/audit-service/src/v2.rs",
        "authenticated_writer_required",
        "cex_append_audit_event_v2",
        "writer.service_id",
    )


def verify_delivery_migrations() -> None:
    require_text(
        "migrations/0060_close_audit_outbox_delivery_lifecycle.sql",
        "cex_enqueue_audit_outbox_v1",
        "cex_mark_audit_outbox_delivered_v1",
        "cex_fail_audit_outbox_delivery_v1",
        "delivery_receipt",
        "dead_lettered_at",
    )
    require_text(
        "migrations/0061_add_execution_transactional_audit_outbox.sql",
        "returns trigger",
        "cex_enqueue_execution_audit_v1()",
        "execution.persisted.status_changed",
        "result_payload_sha256",
        "cex_enqueue_audit_outbox_v1",
        "execute function public.cex_enqueue_execution_audit_v1();",
    )
    require_text(
        "migrations/0062_add_identity_transactional_audit_outbox.sql",
        "returns trigger",
        "cex_enqueue_api_key_audit_v1()",
        "identity.api_key.persisted.issued",
        "identity.api_key.persisted.revoked",
        "_cex_audit_source_service",
        "cex.audit.actor_id",
        "execute function public.cex_enqueue_api_key_audit_v1();",
    )
    forbid_text(
        "migrations/0061_add_execution_transactional_audit_outbox.sql",
        "case when tg_op",
        "execute function public.cex_enqueue_execution_audit_v1(\n",
    )
    forbid_text(
        "migrations/0062_add_identity_transactional_audit_outbox.sql",
        "case when tg_op",
        "execute function public.cex_enqueue_api_key_audit_v1(\n",
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
        "scripts/check-p0-migrations-postgres.sh",
        "cex_mark_audit_outbox_delivered_v1",
        "cex_fail_audit_outbox_delivery_v1",
        "execution.persisted.status_changed",
        "identity.api_key.persisted.revoked",
        "last_used_at-only",
    )


def verify_documentation() -> None:
    require_text(
        "docs/audit-outbox-delivery-v1.md",
        "ACK",
        "retry_wait",
        "dead_letter",
        "remote append succeeds",
    )
    require_text(
        "docs/audit-source-transactional-enqueue-v1.md",
        "Execution",
        "Identity",
        "same PostgreSQL transaction",
        "last_used_at",
    )
    require_text(
        "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v3.md",
        "0062_add_identity_transactional_audit_outbox.sql",
        "Next locked slice",
        "zero-argument trigger functions",
    )


def main() -> int:
    migration_number, migration_filename = latest_migration()
    verify_release_template(migration_filename)
    verify_core_startup_wiring()
    verify_workload_identity()
    verify_audit_dispatcher()
    verify_audit_v2_routes()
    verify_delivery_migrations()
    verify_gate_wiring()
    verify_documentation()

    result = {
        "status": "failed" if PROBLEMS else "ok",
        "migration_number": migration_number,
        "migration_head": migration_filename,
        "checks": 8,
        "problems": PROBLEMS,
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 1 if PROBLEMS else 0


if __name__ == "__main__":
    sys.exit(main())
