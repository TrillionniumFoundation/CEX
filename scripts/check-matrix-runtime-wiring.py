#!/usr/bin/env python3
"""Validate source wiring, not runtime execution or production qualification."""
from __future__ import annotations

from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    target = ROOT / path
    if not target.is_file():
        raise AssertionError(f"required file is absent: {path}")
    return target.read_text(encoding="utf-8")


def require(text: str, path: str, markers: list[str]) -> None:
    missing = [marker for marker in markers if marker not in text]
    if missing:
        raise AssertionError(f"{path} is missing required runtime markers: {missing}")


def forbid(text: str, path: str, markers: list[str]) -> None:
    present = [marker for marker in markers if marker in text]
    if present:
        raise AssertionError(f"{path} contains forbidden non-durable markers: {present}")


def function_source(text: str, name: str) -> str:
    # Function-local ordering avoids satisfying a transaction check with a
    # similarly named helper elsewhere. This remains a static source check.
    start = re.search(r"(?m)^(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+" + re.escape(name) + r"\s*\(", text)
    if start is None:
        raise AssertionError(f"missing function definition: {name}")
    tail = text[start.end():]
    next_function = re.search(r"(?m)^(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+\w+\s*\(", tail)
    return tail[:next_function.start()] if next_function else tail


def require_order(text: str, name: str, markers: list[str]) -> None:
    body = function_source(text, name)
    previous = -1
    for marker in markers:
        position = body.find(marker, previous + 1)
        if position < 0:
            raise AssertionError(f"{name} is missing ordered source operation: {marker}")
        previous = position


def main() -> int:
    poller_path = "apps/matrix-bot-poller/src/main.rs"
    relay_path = "apps/matrix-bot-relay/src/main.rs"
    poller = read(poller_path)
    relay = read(relay_path)
    require(poller, poller_path, [
        "cex_matrix_acquire_cursor_lease_v1", "cex_matrix_accept_source_event_v1",
        "cex_matrix_enqueue_delivery_v1", "cex_matrix_advance_cursor_v1",
        "MATRIX_TRANSPORT_DATABASE_URL", "MATRIX_POLL_CURSOR_LEASE_SECONDS",
        "MATRIX_POLL_PARTITION_ID", "MATRIX_POLL_WORKER_ID",
        "matrix_cursor_lease_or_revision_mismatch", "cex_matrix_record_poison_event_v1",
        "Policy::none()", "deterministic_uuid", "persist_batch",
    ])
    forbid(poller, poller_path, [
        "MATRIX_SYNC_STATE_FILE", "MATRIX_BOT_STATE_FILE", "/tmp/matrix",
        "File::create", "write_all(",
    ])
    require_order(poller, "persist_batch", [
        "pool.begin().await?", "cex_matrix_accept_source_event_v1",
        "cex_matrix_enqueue_delivery_v1", "cex_matrix_advance_cursor_v1", "tx.commit().await?",
    ])
    forbid(function_source(poller, "persist_batch"), "persist_batch", [".send()", ".send().await"])
    require(relay, relay_path, [
        "cex_matrix_accept_source_event_v1", "cex_matrix_enqueue_delivery_v1",
        "cex_matrix_claim_delivery_v1", "cex_matrix_finish_delivery_v1",
        "cex_matrix_record_poison_event_v1", "MATRIX_TRANSPORT_DATABASE_URL",
        "MATRIX_RELAY_WORKER_ID", "matrix-relay-adapter-v1", "matrix-homeserver-v1",
        "x-cex-delivery-id", "x-cex-payload-sha256", "idempotency-key",
        "matrix_send_url", "complete_adapter_success", "Policy::none()",
    ])
    forbid(relay, relay_path, [
        "MATRIX_BOT_RELAY_QUEUE_PATH", "matrix-bot-relay-queue.json", "/tmp/matrix",
        "VecDeque", "File::create",
    ])
    require_order(relay, "complete_adapter_success", [
        "state.pool.begin().await?", "cex_matrix_enqueue_delivery_v1",
        "cex_matrix_finish_delivery_v1", "tx.commit().await?",
    ])
    forbid(function_source(relay, "complete_adapter_success"), "complete_adapter_success", [".send()"])
    matrix_call = function_source(relay, "call_matrix_homeserver")
    if not re.search(r"matrix_send_url\s*\([^;]*\bdelivery\.delivery_id\s*,?\s*\)", matrix_call):
        raise AssertionError("Matrix send URL must use the immutable delivery UUID")
    for path in ["apps/matrix-bot-poller/Cargo.toml", "apps/matrix-bot-relay/Cargo.toml"]:
        manifest = read(path)
        require(manifest, path, ["anyhow.workspace = true", 'sha2 = "0.10"', "sqlx.workspace = true"])
        forbid(manifest, path, ["chrono.workspace = true"])
    migration_path = "services/matrix-entry-adapter/migrations/0001_transport_durability.sql"
    require(read(migration_path), migration_path, [
        "create table if not exists public.matrix_transport_cursors",
        "create table if not exists public.matrix_transport_inbox",
        "create table if not exists public.matrix_transport_outbox",
        "create table if not exists public.matrix_transport_delivery_history",
        "create table if not exists public.matrix_transport_poison_events",
        "cex_matrix_register_delivery_and_advance_v1", "cex_matrix_lookup_delivery_v1",
        "cex_matrix_acknowledge_poison_event_v1", "matrix_transport_inbox_immutable_v1",
        "matrix_transport_delivery_history_immutable_v1", "matrix_transport_outbox_identity_guard_v1",
        "for update skip locked", "lease_fence = outbox.lease_fence + 1",
    ])
    print("Matrix durable source wiring: OK; runtime qualification remains required")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except AssertionError as error:
        print(f"Matrix durable runtime wiring: FAILED: {error}", file=sys.stderr)
        raise SystemExit(1)
