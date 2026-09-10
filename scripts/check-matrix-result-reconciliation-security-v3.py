#!/usr/bin/env python3
"""Fail closed on Matrix result-reconciliation security contract v3."""
from __future__ import annotations

import json
from pathlib import Path
import stat
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
FILES = {
    "adapter_facade": "services/matrix-entry-adapter/src/lib.rs",
    "adapter_binding": "services/matrix-entry-adapter/src/delivery_binding.rs",
    "adapter_response_binding": (
        "services/matrix-entry-adapter/src/reconciliation_response_binding.rs"
    ),
    "consumer_main": "services/consumer-entry-api/src/main.rs",
    "consumer_lookup": "services/consumer-entry-api/src/matrix_result_lookup.rs",
    "consumer_response_binding": (
        "services/consumer-entry-api/src/matrix_result_response_binding.rs"
    ),
    "migration": (
        "services/matrix-entry-adapter/operator-migrations/"
        "0006_adapter_result_embedded_delivery_binding.sql"
    ),
    "regression": "scripts/test-matrix-result-embedded-binding-postgres.sql",
    "task_regression": (
        "scripts/test-matrix-result-task-invocation-binding-postgres.sql"
    ),
    "runner": "scripts/matrix_operator_postgres_regression.py",
    "command": "scripts/reconcile-matrix-adapter-result-v3.py",
    "workflow": ".github/workflows/matrix-review-repair-regression.yml",
    "design": "docs/matrix-result-reconciliation-v3.md",
}


def read_regular(relative: str) -> str:
    path = ROOT / relative
    try:
        metadata = path.lstat()
    except OSError as error:
        raise RuntimeError(f"required source unavailable: {relative}: {error}") from None
    if (
        not stat.S_ISREG(metadata.st_mode)
        or path.is_symlink()
        or metadata.st_size > 2_000_000
    ):
        raise RuntimeError(f"source is not a bounded regular file: {relative}")
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        raise RuntimeError(f"source is not readable UTF-8: {relative}: {error}") from None


def require(source: str, tokens: tuple[str, ...], label: str) -> list[str]:
    return [
        f"{label}: missing {token!r}"
        for token in tokens
        if token not in source
    ]


def forbid(source: str, tokens: tuple[str, ...], label: str) -> list[str]:
    return [
        f"{label}: forbidden {token!r}"
        for token in tokens
        if token in source
    ]


def reconciler_self_test() -> list[str]:
    path = ROOT / FILES["command"]
    try:
        result = subprocess.run(
            [sys.executable, str(path), "--self-test"],
            cwd=ROOT,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=45,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return ["runtime command v3: self-test could not execute"]
    if result.returncode != 0:
        return ["runtime command v3: self-test failed"]
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError:
        return ["runtime command v3: self-test output is not JSON"]
    if (
        payload.get("schema") != "cex.matrix.adapter-result-reconciler.v3"
        or payload.get("status") != "ok"
        or payload.get("self_test") is not True
        or payload.get("security_contract") != "v3"
        or payload.get("production_authorization") != "not_granted"
    ):
        return ["runtime command v3: self-test output contract mismatch"]
    return []


def main() -> int:
    try:
        sources = {
            name: read_regular(path)
            for name, path in FILES.items()
        }
    except RuntimeError as error:
        print(error, file=sys.stderr)
        return 1

    failures: list[str] = []
    failures += require(
        sources["adapter_facade"],
        (
            "mod delivery_binding;",
            "mod reconciliation_response_binding;",
            "DeliveryBindingPolicy::from_config",
            "middleware::from_fn_with_state",
            "delivery_binding::enforce_delivery_binding",
            "reconciliation_response_binding::enforce_reconciliation_response_binding",
        ),
        "adapter facade delivery/result-binding layers",
    )
    failures += require(
        sources["adapter_binding"],
        (
            'const DELIVERY_BINDING_FIELD: &str = "cex_delivery_binding";',
            'const DELIVERY_BINDING_SCHEMA: &str = "cex.matrix.delivery-binding.v1";',
            'const DELIVERY_BINDING_SOURCE: &str = "matrix-bot-relay-headers-v1";',
            '"x-cex-delivery-id"',
            '"x-cex-payload-sha256"',
            '"x-idempotency-key"',
            '"idempotency-key"',
            "RuntimeProfile::Beta | RuntimeProfile::Production",
            "sha256_prefixed(&canonical) != payload_sha256",
            "matrix_delivery_payload_hash_mismatch",
            "matrix_delivery_binding_reserved_field_present",
            "delivery_request_fingerprint(",
            '"request_fingerprint": request_fingerprint',
        ),
        "adapter relay-header binding",
    )
    failures += forbid(
        sources["adapter_binding"],
        (
            "unwrap_or_default()",
            "metadata.insert(DELIVERY_BINDING_FIELD.to_string(), Value::Null)",
        ),
        "adapter relay-header binding",
    )
    failures += require(
        sources["adapter_response_binding"],
        (
            'const RECONCILIATION_PATH: &str = "/v1/matrix/results/lookup";',
            "enforce_reconciliation_response_binding",
            'Some("task_result_reconciled")',
            "RECONCILIATION_SCHEMA",
            "RECONCILIATION_SOURCE",
            "RECONCILIATION_CAUSAL_BINDING",
            '.and_then(|raw| raw.get("invocation_id"))',
            "if invocation_id != task_id",
            ".and_then(|metadata| metadata.get(DELIVERY_BINDING_FIELD))",
            "binding.len() != 8",
            "task_invocation_and_response_envelope_are_mandatory",
            "changed_or_extended_binding_is_rejected",
        ),
        "adapter success response binding",
    )
    failures += forbid(
        sources["adapter_response_binding"],
        ("unwrap_or_default()", "shell=True", "unsafe {"),
        "adapter success response binding",
    )

    failures += require(
        sources["consumer_main"],
        (
            "mod matrix_result_lookup;",
            "mod matrix_result_response_binding;",
            "matrix_result_lookup::router",
            "matrix_result_response_binding::enforce_matrix_result_response_binding",
        ),
        "consumer canonical lookup router",
    )
    failures += forbid(
        sources["consumer_main"],
        ("mod matrix_result_lookup_v2;", "matrix_result_lookup_v2::router"),
        "consumer temporary lookup router",
    )
    failures += require(
        sources["consumer_lookup"],
        (
            'const DELIVERY_BINDING_FIELD: &str = "cex_delivery_binding";',
            'const DELIVERY_BINDING_SCHEMA: &str = "cex.matrix.delivery-binding.v1";',
            'const DELIVERY_BINDING_SOURCE: &str = "matrix-bot-relay-headers-v1";',
            '"result_delivery_binding": result_delivery_binding',
            '.and_then(|metadata| metadata.get("metadata"))',
            ".and_then(|metadata| metadata.get(DELIVERY_BINDING_FIELD))",
            "binding.len() != 8",
            'binding.get("delivery_id")',
            'binding.get("payload_sha256")',
            'binding.get("request_fingerprint")',
            "request.request_fingerprint != lookup_request_fingerprint(request)",
            "cached_result_must_carry_the_exact_persisted_delivery_binding",
            "persisted_binding_rejects_extra_or_wrong_authority_fields",
        ),
        "consumer persisted result binding",
    )
    failures += require(
        sources["consumer_response_binding"],
        (
            'const LOOKUP_PATH: &str = "/v1/matrix/messages/result";',
            "enforce_matrix_result_response_binding",
            'Some("cex.matrix.result-lookup.v1")',
            'value.get("result_delivery_binding")',
            '.and_then(|raw| raw.get("invocation_id"))',
            "if invocation_id != task_id",
            "if nested_binding != top_binding",
            "missing_or_changed_invocation_fails_closed",
            "changed_top_or_nested_delivery_binding_fails_closed",
        ),
        "consumer lookup success response binding",
    )
    failures += forbid(
        sources["consumer_response_binding"],
        ("unwrap_or_default()", "shell=True", "unsafe {"),
        "consumer lookup success response binding",
    )

    failures += require(
        sources["migration"],
        (
            "cex_matrix_reconcile_adapter_result_v3",
            "matrix_adapter_result_task_invocation_mismatch_v3",
            "p_result_payload -> 'raw' ->> 'invocation_id'",
            "{source,metadata,metadata,cex_delivery_binding}",
            "embedded_binding_key_count bigint;",
            "from pg_catalog.jsonb_object_keys(embedded_binding)",
            "embedded_binding_key_count <> 8",
            "matrix-bot-relay-headers-v1",
            "matrix_adapter_result_embedded_binding_invalid_v3",
            "matrix_adapter_result_request_fingerprint_mismatch_v3",
            "return public.cex_matrix_reconcile_adapter_result_v2(",
            "from public, cex_matrix_reconciler_runtime",
            "to cex_matrix_reconciler_runtime",
        ),
        "operator migration v3",
    )
    failures += forbid(
        sources["migration"],
        ("grant all", " to public;"),
        "operator migration v3",
    )

    failures += require(
        sources["regression"],
        (
            "matrix_embedded_binding_honest_retry_not_idempotent",
            "matrix_embedded_binding_v2_runtime_execute_not_revoked",
            "matrix_embedded_binding_v3_runtime_execute_missing",
            "matrix_embedded_binding_missing_binding_accepted",
            "matrix_embedded_binding_changed_payload_accepted",
            "matrix_embedded_binding_extra_field_accepted",
            "matrix_embedded_binding_hostile_input_wrote_observation",
        ),
        "embedded-binding PostgreSQL regression",
    )
    failures += require(
        sources["task_regression"],
        (
            "matrix_task_invocation_missing_accepted",
            "matrix_task_invocation_mismatch_accepted",
            "matrix_adapter_result_task_invocation_mismatch_v3",
            "matrix_missing_delivery_binding_accepted",
            "matrix_adapter_result_embedded_binding_invalid_v3",
        ),
        "task invocation PostgreSQL regression",
    )
    failures += require(
        sources["runner"],
        (
            '"0006_adapter_result_embedded_delivery_binding.sql"',
            '"scripts/test-matrix-result-embedded-binding-postgres.sql"',
            '"scripts/test-matrix-result-task-invocation-binding-postgres.sql"',
            'SCHEMA = "cex.matrix-operator-postgres-regression.v4"',
            "BASE_MIGRATIONS",
            "SECURITY_MIGRATIONS",
            "BASE_REGRESSIONS",
            "SECURITY_REGRESSIONS",
        ),
        "operator migration runner v4",
    )

    failures += require(
        sources["command"],
        (
            'SCHEMA = "cex.matrix.adapter-result-reconciler.v3"',
            'SECURITY_CONTRACT = "v3"',
            'BINDING_FIELD = "cex_delivery_binding"',
            'BINDING_SCHEMA = "cex.matrix.delivery-binding.v1"',
            'BINDING_SOURCE = "matrix-bot-relay-headers-v1"',
            "validate_persisted_binding(",
            'raw.get("invocation_id") != task_id',
            "lookup_forwarded_task_invocation_mismatch_v3",
            "if set(binding) != required",
            "V2_FUNCTION",
            "V3_FUNCTION",
            "rewritten.count(V3_FUNCTION) != 1",
            "changed persisted delivery binding accepted",
            "missing or changed task invocation accepted",
        ),
        "operator command v3",
    )
    failures += forbid(
        sources["command"],
        ("shell=True", "eval(", "exec("),
        "operator command v3",
    )

    failures += require(
        sources["workflow"],
        (
            "python3 scripts/check-matrix-result-reconciliation-security-v3.py",
            "python3 scripts/matrix_operator_postgres_regression.py",
            "services/consumer-entry-api/src/matrix_result_lookup.rs",
            "docs/matrix-result-reconciliation-v3.md",
        ),
        "hosted Matrix v3 gate wiring",
    )
    failures += require(
        sources["design"],
        (
            "# Matrix result reconciliation security contract v3",
            "source.metadata.metadata.cex_delivery_binding",
            "x-cex-delivery-id",
            "x-cex-payload-sha256",
            "unbound cache entry fails closed",
            "reconciliation_response_binding.rs",
            "cex_matrix_reconcile_adapter_result_v3",
            "production_authorization=not_granted",
        ),
        "security design v3",
    )
    failures += reconciler_self_test()

    if failures:
        print(
            "Matrix result reconciliation security contract v3 failed:",
            file=sys.stderr,
        )
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1

    print(
        json.dumps(
            {
                "schema": (
                    "cex.matrix.result-reconciliation-security-source-check.v3"
                ),
                "status": "ok",
                "relay_payload_hash_verified": True,
                "reserved_binding_injected": True,
                "persisted_result_binding_required": True,
                "consumer_success_boundary_revalidated": True,
                "adapter_success_boundary_revalidated": True,
                "task_invocation_binding_required": True,
                "database_embedded_binding_required": True,
                "runtime_v2_execute_revoked": True,
                "runtime_v3_execute_granted": True,
                "operator_migration_head": "0006",
                "runtime_self_test": True,
                "problems": [],
                "checker_max_grant_production_authorization": False,
                "production_authorization": "not_granted",
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
