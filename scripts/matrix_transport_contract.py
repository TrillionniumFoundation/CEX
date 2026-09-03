#!/usr/bin/env python3
"""Static contract for durable Matrix cursor, inbox, outbox and poison state."""

from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any

SCHEMA = "cex.matrix-transport-durability-check.v1"
PRODUCTION_AUTHORIZATION = "not_granted"
MIGRATION = "services/matrix-entry-adapter/migrations/0001_transport_durability.sql"
DOCUMENT = "docs/matrix-transport-durability-v1.md"

REQUIRED_TABLES = (
    "matrix_transport_cursors",
    "matrix_transport_inbox",
    "matrix_transport_outbox",
    "matrix_transport_delivery_history",
    "matrix_transport_poison_events",
)
REQUIRED_FUNCTIONS = (
    "cex_matrix_reject_immutable_mutation_v1",
    "cex_matrix_guard_outbox_identity_v1",
    "cex_matrix_acquire_cursor_lease_v1",
    "cex_matrix_advance_cursor_v1",
    "cex_matrix_accept_source_event_v1",
    "cex_matrix_enqueue_delivery_v1",
    "cex_matrix_register_delivery_and_advance_v1",
    "cex_matrix_claim_delivery_v1",
    "cex_matrix_finish_delivery_v1",
    "cex_matrix_lookup_delivery_v1",
    "cex_matrix_record_poison_event_v1",
    "cex_matrix_acknowledge_poison_event_v1",
)
REQUIRED_MARKERS = (
    "for update skip locked",
    "selected.previous_status",
    "matrix_delivery_identity_mutation_rejected",
    "matrix_immutable_history_mutation_rejected",
    "matrix_cursor_lease_or_revision_mismatch",
    "matrix_source_event_identity_collision",
    "matrix_delivery_identity_collision",
    "matrix_delivery_claim_mismatch",
    "matrix_poison_event_identity_collision",
    "payload_sha256 = p_payload_sha256",
    "lease_fence = p_lease_fence",
    "lease_expires_at > clock_timestamp()",
    "pg_column_size(payload) <= 1048576",
)
FORBIDDEN_MARKERS = (
    "truncate public.matrix_transport",
    "drop table public.matrix_transport",
    "on conflict do update set payload",
    "delete from public.matrix_transport_delivery_history",
    "delete from public.matrix_transport_inbox",
)


def _normalise_sql(text: str) -> str:
    stripped = re.sub(r"--[^\n]*|/\*.*?\*/", " ", text, flags=re.S)
    return re.sub(r"\s+", " ", stripped).strip().lower()


def _function_bodies(text: str) -> dict[str, str]:
    pattern = re.compile(
        r"create\s+or\s+replace\s+function\s+public\.([a-zA-Z0-9_]+)\s*\("
        r".*?\)\s*returns\s+.*?\blanguage\s+([a-zA-Z0-9_]+)\s+"
        r"(.*?)\bas\s+\$\$(.*?)\$\$\s*;",
        re.I | re.S,
    )
    return {
        match.group(1).lower(): match.group(0)
        for match in pattern.finditer(text)
    }


def validate_sql(text: str) -> list[str]:
    problems: list[str] = []
    normal = _normalise_sql(text)
    if not normal.startswith("begin;") or not normal.endswith("commit;"):
        problems.append("migration must be one explicit begin/commit transaction")

    for table in REQUIRED_TABLES:
        if f"create table if not exists public.{table}" not in normal:
            problems.append(f"missing durable Matrix table: {table}")

    functions = _function_bodies(text)
    for function in REQUIRED_FUNCTIONS:
        body = functions.get(function)
        if body is None:
            problems.append(f"missing Matrix function: {function}")
            continue
        if "set search_path = pg_catalog, public" not in _normalise_sql(body):
            problems.append(f"Matrix function lacks pinned search_path: {function}")
        if f"revoke all on function public.{function}" not in normal:
            problems.append(f"Matrix function is not revoked from public: {function}")

    for marker in REQUIRED_MARKERS:
        if marker not in normal:
            problems.append(f"missing Matrix durability marker: {marker}")
    for marker in FORBIDDEN_MARKERS:
        if marker in normal:
            problems.append(f"forbidden Matrix durability marker: {marker}")

    if normal.count("before update or delete on public.matrix_transport_inbox") != 1:
        problems.append("Matrix inbox does not have exactly one immutable trigger")
    if normal.count(
        "before update or delete on public.matrix_transport_delivery_history"
    ) != 1:
        problems.append("Matrix delivery history does not have exactly one immutable trigger")
    if normal.count("before update on public.matrix_transport_outbox") != 1:
        problems.append("Matrix outbox identity guard is missing or duplicated")

    register = _normalise_sql(
        functions.get("cex_matrix_register_delivery_and_advance_v1", "")
    )
    for marker in (
        "cex_matrix_accept_source_event_v1(",
        "cex_matrix_enqueue_delivery_v1(",
        "cex_matrix_advance_cursor_v1(",
        "for update",
    ):
        if marker not in register:
            problems.append(f"atomic Matrix registration lacks: {marker}")

    claim = _normalise_sql(functions.get("cex_matrix_claim_delivery_v1", ""))
    if "outbox.status as previous_status" not in claim:
        problems.append("Matrix claim does not freeze the pre-claim status")
    if "selected.previous_status" not in claim:
        problems.append("Matrix claim history does not record the frozen pre-claim status")

    lookup = _normalise_sql(functions.get("cex_matrix_lookup_delivery_v1", ""))
    if "delivery_id = p_delivery_id" not in lookup or "payload_sha256 = p_payload_sha256" not in lookup:
        problems.append("Matrix response-loss lookup is not bound to delivery ID and payload hash")

    return problems


def validate_document(text: str) -> list[str]:
    problems: list[str] = []
    for marker in (
        "Status: active Matrix transport durability contract",
        "Production authorization: `not_granted`",
        MIGRATION,
        "cursor compare-and-set",
        "lease fencing",
        "poison-event",
        "response loss",
        "python3 scripts/check-matrix-transport-durability.py",
        "bash scripts/check-matrix-transport-postgres.sh",
    ):
        if marker not in text:
            problems.append(f"Matrix durability document lacks marker: {marker}")
    return problems


def validate_repository(root: Path) -> dict[str, Any]:
    root = root.resolve()
    problems: list[str] = []
    migration_path = root / MIGRATION
    document_path = root / DOCUMENT
    try:
        migration = migration_path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as error:
        migration = ""
        problems.append(f"cannot read {MIGRATION}: {error}")
    try:
        document = document_path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as error:
        document = ""
        problems.append(f"cannot read {DOCUMENT}: {error}")
    if migration:
        problems.extend(validate_sql(migration))
    if document:
        problems.extend(validate_document(document))
    return {
        "schema": SCHEMA,
        "status": "failed" if problems else "ok",
        "migration": MIGRATION,
        "document": DOCUMENT,
        "required_tables": len(REQUIRED_TABLES),
        "required_functions": len(REQUIRED_FUNCTIONS),
        "checker_may_grant_production_authorization": False,
        "production_authorization": PRODUCTION_AUTHORIZATION,
        "problems": problems,
    }


def dumps_result(result: dict[str, Any]) -> str:
    return json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True)
