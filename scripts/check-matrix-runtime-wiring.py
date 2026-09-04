#!/usr/bin/env python3
"""Fail closed when Matrix runtime code drifts from the durable transport contract."""

from __future__ import annotations

from pathlib import Path
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


def main() -> int:
    poller_path = "apps/matrix-bot-poller/src/main.rs"
    relay_path = "apps/matrix-bot-relay/src/main.rs"
    poller_manifest_path = "apps/matrix-bot-poller/Cargo.toml"
    relay_manifest_path = "apps/matrix-bot-relay/Cargo.toml"
    migration_path = "services/matrix-entry-adapter/migrations/0001_transport_durability.sql"

    poller = read(poller_path)
    relay = read(relay_path)
    poller_manifest = read(poller_manifest_path)
    relay_manifest = read(relay_manifest_path)
    migration = read(migration_path)

    require(
        poller,
        poller_path,
        [
            "cex_matrix_acquire_cursor_lease_v1",
            "cex_matrix_accept_source_event_v1",
            "cex_matrix_enqueue_delivery_v1",
            "cex_matrix_advance_cursor_v1",
            "MATRIX_TRANSPORT_DATABASE_URL",
            "MATRIX_CURSOR_LEASE_SECONDS",
            "MATRIX_POLL_PARTITION_ID",
            "MATRIX_POLL_WORKER_ID",
            "matrix_cursor_lease_or_revision_mismatch",
            "record_poison",
            "Policy::none()",
            "deterministic_delivery_id",
            "register_batch_and_advance",
        ],
    )
    forbid(
        poller,
        poller_path,
        [
            "MATRIX_SYNC_STATE_FILE",
            "MATRIX_BOT_STATE_FILE",
            "/tmp/matrix",
            "File::create",
            "write_all(",
        ],
    )

    require(
        relay,
        relay_path,
        [
            "cex_matrix_accept_source_event_v1",
            "cex_matrix_enqueue_delivery_v1",
            "cex_matrix_claim_delivery_v1",
            "cex_matrix_finish_delivery_v1",
            "cex_matrix_record_poison_event_v1",
            "MATRIX_TRANSPORT_DATABASE_URL",
            "MATRIX_RELAY_WORKER_ID",
            "matrix-relay-adapter-v1",
            "matrix-homeserver-v1",
            "x-cex-delivery-id",
            "x-cex-payload-sha256",
            "idempotency-key",
            "transaction_id = claim.delivery_id.to_string()",
            "finish_claim_in_transaction",
            "Policy::none()",
        ],
    )
    forbid(
        relay,
        relay_path,
        [
            "MATRIX_BOT_RELAY_QUEUE_PATH",
            "matrix-bot-relay-queue.json",
            "/tmp/matrix",
            "VecDeque",
            "File::create",
        ],
    )

    for manifest, path in [
        (poller_manifest, poller_manifest_path),
        (relay_manifest, relay_manifest_path),
    ]:
        require(
            manifest,
            path,
            [
                "anyhow.workspace = true",
                'sha2 = "0.10"',
                "sqlx.workspace = true",
            ],
        )
        forbid(manifest, path, ["chrono.workspace = true"])

    require(
        migration,
        migration_path,
        [
            "create table if not exists public.matrix_transport_cursors",
            "create table if not exists public.matrix_transport_inbox",
            "create table if not exists public.matrix_transport_outbox",
            "create table if not exists public.matrix_transport_delivery_history",
            "create table if not exists public.matrix_transport_poison_events",
            "cex_matrix_register_delivery_and_advance_v1",
            "cex_matrix_lookup_delivery_v1",
            "cex_matrix_acknowledge_poison_event_v1",
            "matrix_transport_inbox_immutable_v1",
            "matrix_transport_delivery_history_immutable_v1",
            "matrix_transport_outbox_identity_guard_v1",
            "for update skip locked",
            "lease_fence = outbox.lease_fence + 1",
        ],
    )

    print("Matrix durable runtime wiring: OK")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except AssertionError as error:
        print(f"Matrix durable runtime wiring: FAILED: {error}", file=sys.stderr)
        raise SystemExit(1)
