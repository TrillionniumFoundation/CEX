#!/usr/bin/env python3
"""Run the v3 Matrix result reconciler with durable embedded delivery binding."""
from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path
import sys
from types import ModuleType
from typing import Any, Callable

ROOT = Path(__file__).resolve().parents[1]
CORE_PATH = ROOT / "scripts/reconcile-matrix-adapter-result.py"
SCHEMA = "cex.matrix.adapter-result-reconciler.v3"
SECURITY_CONTRACT = "v3"
BINDING_FIELD = "cex_delivery_binding"
BINDING_SCHEMA = "cex.matrix.delivery-binding.v1"
BINDING_SOURCE = "matrix-bot-relay-headers-v1"
V2_FUNCTION = "public.cex_matrix_reconcile_adapter_result_v2("
V3_FUNCTION = "public.cex_matrix_reconcile_adapter_result_v3("


class V3BindingError(RuntimeError):
    pass


def load_core() -> ModuleType:
    spec = importlib.util.spec_from_file_location(
        "cex_matrix_adapter_result_reconciler_v2_core",
        CORE_PATH,
    )
    if spec is None or spec.loader is None:
        raise V3BindingError("reconciler_v2_core_unavailable")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def validate_persisted_binding(
    forwarded: dict[str, Any],
    expected: dict[str, str],
) -> dict[str, str]:
    task_id = forwarded.get("task_id")
    raw = forwarded.get("raw")
    if (
        not isinstance(task_id, str)
        or not 1 <= len(task_id.encode("utf-8")) <= 128
        or not isinstance(raw, dict)
        or raw.get("invocation_id") != task_id
    ):
        raise V3BindingError("lookup_forwarded_task_invocation_mismatch_v3")

    source = forwarded.get("source")
    if not isinstance(source, dict):
        raise V3BindingError("lookup_forwarded_source_invalid_v3")
    transport_metadata = source.get("metadata")
    if not isinstance(transport_metadata, dict):
        raise V3BindingError("lookup_forwarded_transport_metadata_missing_v3")
    event_metadata = transport_metadata.get("metadata")
    if not isinstance(event_metadata, dict):
        raise V3BindingError("lookup_forwarded_event_metadata_missing_v3")
    binding = event_metadata.get(BINDING_FIELD)
    if not isinstance(binding, dict):
        raise V3BindingError("lookup_forwarded_delivery_binding_missing_v3")

    required = {
        "schema",
        "source",
        "delivery_id",
        "payload_sha256",
        "event_id",
        "room_id",
        "matrix_user_id",
        "request_fingerprint",
    }
    if set(binding) != required:
        raise V3BindingError("lookup_forwarded_delivery_binding_shape_invalid_v3")
    expected_values = {
        "schema": BINDING_SCHEMA,
        "source": BINDING_SOURCE,
        "delivery_id": expected["delivery_id"],
        "payload_sha256": expected["payload_sha256"],
        "event_id": expected["event_id"],
        "room_id": expected["room_id"],
        "matrix_user_id": expected["sender"],
        "request_fingerprint": expected["request_fingerprint"],
    }
    if binding != expected_values:
        raise V3BindingError("lookup_forwarded_delivery_binding_mismatch_v3")
    return expected_values


def patch_core(core: ModuleType) -> None:
    original_parse: Callable = core.parse_lookup_response
    original_build_sql: Callable = core.build_sql

    def parse_lookup_response(
        raw: bytes,
        expected: dict[str, str],
    ) -> dict[str, Any]:
        forwarded = original_parse(raw, expected)
        validate_persisted_binding(forwarded, expected)
        return forwarded

    def build_sql(*args: Any, **kwargs: Any) -> str:
        sql = original_build_sql(*args, **kwargs)
        if sql.count(V2_FUNCTION) != 1:
            raise V3BindingError("reconciler_v2_function_marker_invalid")
        rewritten = sql.replace(V2_FUNCTION, V3_FUNCTION, 1)
        if V2_FUNCTION in rewritten or rewritten.count(V3_FUNCTION) != 1:
            raise V3BindingError("reconciler_v3_function_rewrite_failed")
        return rewritten

    core.parse_lookup_response = parse_lookup_response
    core.build_sql = build_sql
    core.SCHEMA = SCHEMA
    core.SECURITY_CONTRACT = SECURITY_CONTRACT


def self_test(core: ModuleType) -> None:
    # Retain every v2 transport, parser, TLS, executable-custody and closed-environment test.
    core.self_test()
    patch_core(core)

    expected = {
        "delivery_id": "65000000-0000-4000-8000-000000000001",
        "event_id": "$delivery-bound-event",
        "room_id": "!delivery-room:example",
        "sender": "@delivery-user:example",
        "payload_sha256": "sha256:" + "8" * 64,
    }
    expected["request_fingerprint"] = core.delivery_request_fingerprint(
        expected["delivery_id"],
        expected["payload_sha256"],
        expected["event_id"],
        expected["sender"],
        expected["room_id"],
    )
    binding = {
        "schema": BINDING_SCHEMA,
        "source": BINDING_SOURCE,
        "delivery_id": expected["delivery_id"],
        "payload_sha256": expected["payload_sha256"],
        "event_id": expected["event_id"],
        "room_id": expected["room_id"],
        "matrix_user_id": expected["sender"],
        "request_fingerprint": expected["request_fingerprint"],
    }
    forwarded = {
        "task_id": "task-delivery-bound",
        "source": {
            "kind": "matrix_message",
            "event_id": expected["event_id"],
            "room_id": expected["room_id"],
            "matrix_user_id": expected["sender"],
            "identity_scope": {
                "user_id": expected["sender"],
                "room_id": expected["room_id"],
            },
            "metadata": {
                "event_type": "m.room.message",
                "timestamp_ms": 1_789_000_000_000,
                "metadata": {BINDING_FIELD: binding},
                "content": {
                    "msgtype": "m.text",
                    "body": "/task preserve delivery identity",
                },
            },
        },
        "raw": {"invocation_id": "task-delivery-bound"},
    }
    envelope = {
        "accepted": True,
        "action": core.LOOKUP_ACTION,
        **expected,
        "forwarded": forwarded,
        "projected_reply": None,
        "reconciliation": {
            "schema": core.LOOKUP_SCHEMA,
            "source": "consumer_entry_durable_replay",
            "read_only": True,
            "causal_binding": "delivery_payload_fingerprint",
        },
        "generated_at": "2026-09-09T00:00:00Z",
    }
    raw = json.dumps(envelope, separators=(",", ":"), sort_keys=True).encode()
    assert core.parse_lookup_response(raw, expected) == forwarded

    changed = json.loads(raw)
    changed["forwarded"]["source"]["metadata"]["metadata"][BINDING_FIELD][
        "payload_sha256"
    ] = "sha256:" + "9" * 64
    try:
        core.parse_lookup_response(
            json.dumps(changed, separators=(",", ":"), sort_keys=True).encode(),
            expected,
        )
    except (V3BindingError, core.ReconciliationError):
        pass
    else:
        raise AssertionError("changed persisted delivery binding accepted")

    for raw_value in ({}, {"invocation_id": "other-task"}):
        changed = json.loads(raw)
        changed["forwarded"]["raw"] = raw_value
        try:
            core.parse_lookup_response(
                json.dumps(changed, separators=(",", ":"), sort_keys=True).encode(),
                expected,
            )
        except (V3BindingError, core.ReconciliationError):
            pass
        else:
            raise AssertionError("missing or changed task invocation accepted")

    args = argparse.Namespace(
        delivery_id=expected["delivery_id"],
        event_id=expected["event_id"],
        room_id=expected["room_id"],
        sender=expected["sender"],
        payload_sha256=expected["payload_sha256"],
        candidate_sha="a" * 40,
    )
    sql = core.build_sql(
        args,
        forwarded,
        "sha256:" + "3" * 64,
        "2026-09-09T00:00:00+00:00",
        expected["request_fingerprint"],
    )
    assert V3_FUNCTION in sql
    assert V2_FUNCTION not in sql


def main() -> int:
    try:
        core = load_core()
        if "--self-test" in sys.argv[1:]:
            if sys.argv[1:] != ["--self-test"]:
                raise V3BindingError("self_test_accepts_no_other_arguments")
            self_test(core)
            print(
                json.dumps(
                    {
                        "schema": SCHEMA,
                        "status": "ok",
                        "self_test": True,
                        "security_contract": SECURITY_CONTRACT,
                        "production_authorization": "not_granted",
                    },
                    sort_keys=True,
                )
            )
            return 0
        patch_core(core)
        return int(core.main())
    except Exception:
        print("matrix_adapter_result_reconciler_v3_failed", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
