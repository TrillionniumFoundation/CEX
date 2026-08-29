#!/usr/bin/env python3
"""Fail-closed static gate for P0-N6 Gateway exact registration and reserve."""

from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROBLEMS: list[str] = []


def read(relative: str) -> str:
    path = ROOT / relative
    if not path.is_file():
        PROBLEMS.append(f"missing required file: {relative}")
        return ""
    try:
        return path.read_text(encoding="utf-8")
    except UnicodeDecodeError as error:
        PROBLEMS.append(f"required UTF-8 file cannot be decoded: {relative}: {error}")
        return ""


def require(relative: str, *markers: str) -> str:
    content = read(relative)
    for marker in markers:
        if marker not in content:
            PROBLEMS.append(f"{relative} lacks required marker: {marker}")
    return content


migration = require(
    "migrations/0073_add_gateway_exact_reserve_commands.sql",
    "cex_gateway_ledger_reserve_commands_v1",
    "cex_prepare_gateway_exact_reserve_v1",
    "cex_claim_gateway_exact_reserves_v1",
    "cex_validate_gateway_exact_reserve_receipt_v1",
    "cex_finish_gateway_exact_reserve_v1",
    "claim_lease_expired_after_final_attempt_unknown_outcome",
    "retry_budget_exhausted_unknown_outcome",
    "for update skip locked",
    "cex_acknowledge_gateway_exact_reserve_v1",
    "cex_requeue_gateway_exact_reserve_v1",
    "cex_gateway_ledger_reserve_status_v1",
    "cex_enqueue_audit_outbox_v1",
)
if migration and not migration.strip().lower().startswith("begin;"):
    PROBLEMS.append("0073 migration must begin with BEGIN")
if migration and not migration.strip().lower().endswith("commit;"):
    PROBLEMS.append("0073 migration must end with COMMIT")
for marker in (
    "requested_amount",
    "requested_reserve_amount",
    "rejects dual exact/legacy money input",
    "execution_mode = 'active'",
    "contract_row.status is distinct from 'reserved'",
):
    if marker not in migration:
        PROBLEMS.append(f"0073 lacks exact/legacy or durable-contract guard: {marker}")

api = require(
    "services/gateway-service/src/bin/gateway-exact-reserve-api.rs",
    "#[serde(deny_unknown_fields)]",
    "amount_minor: String",
    "parse_positive_minor_units",
    "CEX_GATEWAY_EXACT_INGRESS_TOKEN",
    "CEX_GATEWAY_EXACT_RESERVE_ALLOW_ACTIVE",
    "cex_prepare_gateway_exact_reserve_v1",
    "DefaultBodyLimit::max",
    "constant_time_eq",
)
worker = require(
    "services/gateway-service/src/bin/gateway-exact-reserve-worker.rs",
    "cex_claim_gateway_exact_reserves_v1",
    "/v2/ledger/effects",
    "Policy::none()",
    "MAX_RESPONSE_BYTES",
    "cex_finish_gateway_exact_reserve_v1",
    "ledger_transport_unknown_outcome",
    "claim is intentionally left for lease recovery",
    "CEX_GATEWAY_LEDGER_MODE=dual or require_v2",
    "validate_serial_lease_budget",
)
canonical_entry = require(
    "services/gateway-service/src/application/invocation_service_entry.rs",
    "req.has_legacy_reserve()",
    "legacy_reserve_break_glass",
    "rejected_legacy_reserve_record",
    "legacy::create_invocation",
)
canonical_http = require(
    "services/gateway-service/src/interfaces/http.rs",
    "body.has_legacy_reserve()",
    "state.legacy_reserve_break_glass",
    "legacy_reserve_requires_exact_ingress_response",
    "resolve_api_key",
    "invocation_service::create_invocation",
)
invocation_domain = require(
    "services/gateway-service/src/domain/invocation.rs",
    'skip_serializing_if = "Option::is_none"',
    "LEGACY_RESERVE_REJECTION_CODE",
    "LEGACY_RESERVE_REJECTION_MESSAGE",
)
gateway_state = require(
    "services/gateway-service/src/infrastructure/state.rs",
    "CEX_GATEWAY_LEGACY_RESERVE_BREAK_GLASS",
    "legacy_reserve_break_glass_from_env",
    "env_flag(LEGACY_RESERVE_BREAK_GLASS_ENV, false)",
    "production-like profile",
)
legacy_clients = require(
    "services/gateway-service/src/infrastructure/clients.rs",
    "reserve_credits_legacy_v1",
    "refund_credits_legacy_v1",
)

for relative, content in (
    ("gateway exact reserve API", api),
    ("gateway exact reserve worker", worker),
):
    for pattern in (
        r"\bf32\b",
        r"\bf64\b",
        r"/v1/ledger/",
        r"\.round\s*\(",
        r"\.trunc\s*\(",
        r"\bas\s+i64\b",
        r"sqlx::Transaction",
        r"\.begin\s*\(\s*\)\s*\.await",
    ):
        if re.search(pattern, content):
            PROBLEMS.append(
                f"{relative} contains forbidden legacy-money/long-transaction marker: {pattern}"
            )

entry_guard_offset = canonical_entry.find("if req.has_legacy_reserve()")
entry_legacy_call_offset = canonical_entry.find("legacy::create_invocation")
if min(entry_guard_offset, entry_legacy_call_offset) < 0 or not (
    entry_guard_offset < entry_legacy_call_offset
):
    PROBLEMS.append(
        "canonical Invocation entry must fail closed before reaching the legacy implementation"
    )

http_guard_offset = canonical_http.find("if body.has_legacy_reserve()")
http_auth_offset = canonical_http.find("resolve_api_key")
http_service_offset = canonical_http.find("invocation_service::create_invocation")
if min(http_guard_offset, http_auth_offset, http_service_offset) < 0 or not (
    http_guard_offset < http_auth_offset < http_service_offset
):
    PROBLEMS.append(
        "Gateway HTTP reserve guard must run before auth resolution and Invocation orchestration"
    )

if re.search(r"\.reserve_credits\s*\(", canonical_entry) or re.search(
    r"\.refund_credits\s*\(", canonical_entry
):
    PROBLEMS.append(
        "canonical Invocation entry must not call an unqualified legacy Ledger method"
    )
if "legacy_reserve_break_glass: bool" not in gateway_state:
    PROBLEMS.append("Gateway AppState must carry an explicit legacy reserve break-glass flag")

legacy_break_glass_guarded = (
    "env_flag(LEGACY_RESERVE_BREAK_GLASS_ENV, false)" in gateway_state
    and "legacy_reserve_break_glass: bool" in gateway_state
)

claim_offset = worker.find("cex_claim_gateway_exact_reserves_v1")
network_offset = worker.find("/v2/ledger/effects")
finish_offset = worker.find("cex_finish_gateway_exact_reserve_v1")
if min(claim_offset, network_offset, finish_offset) < 0 or not (
    claim_offset < network_offset < finish_offset
):
    PROBLEMS.append("worker source does not visibly order claim -> Ledger v2 -> outcome persistence")

require(
    "scripts/check-gateway-exact-reserve-postgres.sh",
    "session_replication_role = replica",
    "shadow command was claimed",
    "wrong worker",
    "claim_lease_expired_after_final_attempt_unknown_outcome",
    "fresh acknowledgement",
    "rollback;",
)
require(
    ".github/workflows/p0-gateway-exact-reserve-gate.yml",
    "actions/checkout@11d5960a326750d5838078e36cf38b85af677262",
    "dtolnay/rust-toolchain@4360b52568e2003a75bf9bc1d59f33a8e3fc893c",
    "Swatinem/rust-cache@6323deb102c322ba6fcbdcafc7e3dddab59af2b6",
    "scripts/check-gateway-exact-reserve.py",
    "scripts/check-ledger-caller-cutover.py",
    "cargo test --locked -p gateway-service --all-targets",
    "cargo clippy --locked -p gateway-service --all-targets -- -D warnings",
    "scripts/check-gateway-exact-reserve-postgres.sh",
)
require(
    "config/gateway-exact-reserve.env.example",
    "CEX_GATEWAY_EXACT_RESERVE_ALLOW_ACTIVE=false",
    "CEX_GATEWAY_EXACT_RESERVE_BATCH_SIZE=2",
    "CEX_GATEWAY_EXACT_RESERVE_LEASE_SECONDS=90",
    "CEX_GATEWAY_EXACT_RESERVE_REQUEST_TIMEOUT_SECONDS=20",
)
for unit in (
    "ops/systemd/cex-gateway-exact-reserve-api.service",
    "ops/systemd/cex-gateway-exact-reserve-worker.service",
):
    require(unit, "NoNewPrivileges=true", "ProtectSystem=strict", "CapabilityBoundingSet=")
require(
    "docs/gateway-exact-reserve-v1.md",
    "string minor units",
    "claim transaction commits",
    "reconcile_required",
    "legacy monetary intent is zero",
    "legacy_reserve_fail_closed=true",
    "POST /v2/invocations/:invocation_id/exact-reserve",
)
require(
    "docs/ledger-caller-cutover-v1.md",
    "legacy_reserve_fail_closed=true",
    "CEX_GATEWAY_LEGACY_RESERVE_BREAK_GLASS=false",
    "before API-key resolution",
)
require(
    "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md",
    "P0-N6 delivered by this candidate",
    "Genesis-as-entry",
    "not production-ready",
    "caller migration blocked until exact reserve contract",
)
require(
    "config/gateway-exact-reserve.env.example",
    "CEX_GATEWAY_LEGACY_RESERVE_BREAK_GLASS=false",
)
require(
    ".env.production.example",
    "CEX_GATEWAY_LEGACY_RESERVE_BREAK_GLASS=false",
)

numbered_migrations = sorted(
    ROOT.glob("migrations/[0-9][0-9][0-9][0-9]_*.sql"),
    key=lambda path: path.name,
)
release_migration_head = numbered_migrations[-1].name if numbered_migrations else None
if release_migration_head is None:
    PROBLEMS.append("cannot resolve numbered migration head")

manifest_raw = read("docs/templates/cex-release-baseline-manifest-v1.json")
if manifest_raw:
    try:
        manifest = json.loads(manifest_raw)
        actual = manifest["database"]["migration_head"]
    except (json.JSONDecodeError, KeyError, TypeError) as error:
        PROBLEMS.append(f"cannot decode release manifest template: {error}")
    else:
        if actual != release_migration_head:
            PROBLEMS.append(
                f"release manifest migration_head={actual!r}, "
                f"expected repository head {release_migration_head!r}"
            )

result = {
    "status": "failed" if PROBLEMS else "ok",
    "gateway_contract_migration": "0073_add_gateway_exact_reserve_commands.sql",
    "release_migration_head": release_migration_head,
    "legacy_money_conversion_allowed": False,
    "legacy_reserve_fail_closed": entry_guard_offset >= 0
    and entry_guard_offset < entry_legacy_call_offset
    and http_guard_offset >= 0
    and http_guard_offset < http_auth_offset,
    "legacy_break_glass_default": False,
    "legacy_break_glass_guarded": legacy_break_glass_guarded,
    "active_by_default": False,
    "problems": PROBLEMS,
}
print(json.dumps(result, ensure_ascii=False, indent=2))
raise SystemExit(1 if PROBLEMS else 0)
