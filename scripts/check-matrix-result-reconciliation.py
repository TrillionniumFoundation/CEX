#!/usr/bin/env python3
"""Fail closed unless Matrix result reconciliation is fully cut over to v3."""
from __future__ import annotations

import json
import os
from pathlib import Path
import stat
import subprocess
import sys
from typing import Any

from matrix_source_platform import source_mode, portable_self_test, probe_environment

ROOT = Path(__file__).resolve().parents[1]
FILES = {
    "consumer_main": "services/consumer-entry-api/src/main.rs",
    "consumer_lookup": "services/consumer-entry-api/src/matrix_result_lookup.rs",
    "consumer_response": "services/consumer-entry-api/src/matrix_result_response_binding.rs",
    "replay_snapshot": "services/consumer-entry-api/src/replay_store_snapshot.rs",
    "adapter_facade": "services/matrix-entry-adapter/src/lib.rs",
    "adapter_reconciliation": "services/matrix-entry-adapter/src/result_reconciliation.rs",
    "adapter_response": "services/matrix-entry-adapter/src/reconciliation_response_binding.rs",
    "relay_response": "apps/matrix-bot-relay/src/response_contract.rs",
    "migration_v3": "services/matrix-entry-adapter/operator-migrations/0006_adapter_result_embedded_delivery_binding.sql",
    "runner": "scripts/matrix_operator_postgres_regression.py",
    "historical_loader": "scripts/reconcile-matrix-adapter-result-v2-core.py",
    "historical_implementation": "scripts/reconcile-matrix-adapter-result-v2-internal.py",
    "v3_implementation": "scripts/reconcile-matrix-adapter-result-v3.py",
    "canonical_command": "scripts/reconcile-matrix-adapter-result.py",
    "embedded_regression": "scripts/test-matrix-result-embedded-binding-postgres.sql",
    "task_regression": "scripts/test-matrix-result-task-invocation-binding-postgres.sql",
    "traceability": "docs/traceability/sequence54-matrix-result-reconciliation-v3.json",
    "design": "docs/matrix-result-reconciliation-v3.md",
    "workflow": ".github/workflows/matrix-review-repair-regression.yml",
}


def read_regular(relative: str) -> tuple[str, int]:
    path = ROOT / relative
    try:
        metadata = path.lstat()
    except OSError as error:
        raise RuntimeError(f"required source unavailable: {relative}: {error}") from None
    if not stat.S_ISREG(metadata.st_mode) or path.is_symlink() or metadata.st_size > 2_000_000:
        raise RuntimeError(f"source is not a bounded regular file: {relative}")
    try:
        return path.read_text(encoding="utf-8"), source_mode(ROOT, relative, metadata.st_mode)
    except (OSError, UnicodeError) as error:
        raise RuntimeError(f"source is not readable UTF-8: {relative}: {error}") from None


def require(source: str, tokens: tuple[str, ...], label: str) -> list[str]:
    return [f"{label}: missing {token!r}" for token in tokens if token not in source]


def forbid(source: str, tokens: tuple[str, ...], label: str) -> list[str]:
    return [f"{label}: forbidden {token!r}" for token in tokens if token in source]


def unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        if key in value:
            raise ValueError(f"duplicate JSON member: {key}")
        value[key] = item
    return value


def canonical_self_test() -> list[str]:
    if os.name == "nt":
        return portable_self_test(ROOT)
    path = ROOT / FILES["canonical_command"]
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
        return ["canonical v3 reconciler: self-test could not execute"]
    if result.returncode != 0:
        return ["canonical v3 reconciler: self-test failed"]
    try:
        payload = json.loads(result.stdout, object_pairs_hook=unique_object)
    except (json.JSONDecodeError, ValueError):
        return ["canonical v3 reconciler: self-test output is invalid JSON"]
    if payload != {
        "production_authorization": "not_granted",
        "schema": "cex.matrix.adapter-result-reconciler.v3",
        "security_contract": "v3",
        "self_test": True,
        "status": "ok",
    }:
        return ["canonical v3 reconciler: self-test output contract mismatch"]
    return []


def historical_direct_probe() -> list[str]:
    try:
        result = subprocess.run(
            [sys.executable, str(ROOT / FILES["historical_loader"]), "--delivery-id", "x"],
            cwd=ROOT,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=10,
            check=False,
            env=probe_environment(),
        )
    except (OSError, subprocess.TimeoutExpired):
        return ["historical v2 loader: direct-execution probe failed"]
    if result.returncode == 0:
        return ["historical v2 loader: direct execution unexpectedly succeeded"]
    if result.stdout or result.stderr.strip() != "matrix_adapter_result_v2_historical_core_not_runnable":
        return ["historical v2 loader: direct execution did not fail with bounded code"]
    return []


def main() -> int:
    try:
        loaded = {name: read_regular(path) for name, path in FILES.items()}
    except RuntimeError as error:
        print(error, file=sys.stderr)
        return 1
    sources = {name: item[0] for name, item in loaded.items()}
    failures: list[str] = []

    failures += require(sources["consumer_main"], (
        "mod matrix_result_lookup;",
        "mod matrix_result_response_binding;",
        "matrix_result_lookup::router",
        "matrix_result_response_binding::enforce_matrix_result_response_binding",
    ), "consumer lookup router")
    failures += require(sources["consumer_lookup"], (
        'const DELIVERY_BINDING_FIELD: &str = "cex_delivery_binding";',
        'const DELIVERY_BINDING_SCHEMA: &str = "cex.matrix.delivery-binding.v1";',
        'binding.get("delivery_id")',
        'binding.get("payload_sha256")',
        'binding.get("request_fingerprint")',
        "request.request_fingerprint != lookup_request_fingerprint(request)",
        "cached_result_must_carry_the_exact_persisted_delivery_binding",
    ), "consumer persisted delivery binding")
    failures += forbid(sources["consumer_lookup"], (
        "forward_to_cex_task",
        "create_matrix_message_task",
        ".post(url)",
    ), "consumer lookup read-only boundary")
    failures += require(sources["consumer_response"], (
        'const LOOKUP_PATH: &str = "/v1/matrix/messages/result";',
        ".and_then(|raw| raw.get(\"invocation_id\"))",
        "if invocation_id != task_id",
        "if nested_binding != top_binding",
        "missing_or_changed_invocation_fails_closed",
    ), "consumer response boundary")
    failures += require(sources["replay_snapshot"], (
        "read_stable_regular_file(",
        "identity(&before) != identity(&after)",
        "metadata.nlink() != 1",
        "rejects_symlinked_ancestors",
    ), "stable replay snapshot")

    failures += require(sources["adapter_facade"], (
        "mod delivery_binding;",
        "mod reconciliation_response_binding;",
        "delivery_binding::enforce_delivery_binding",
        "reconciliation_response_binding::enforce_reconciliation_response_binding",
    ), "adapter binding middleware")
    failures += require(sources["adapter_reconciliation"], (
        'const RECONCILIATION_PATH: &str = "/v1/matrix/results/lookup";',
        "sign_lookup_assertion",
        "validate_lookup_response",
        '"action": "task_result_reconciled"',
        '"read_only": true',
    ), "adapter read-only reconciliation")
    failures += require(sources["adapter_response"], (
        'const RECONCILIATION_PATH: &str = "/v1/matrix/results/lookup";',
        '.and_then(|raw| raw.get(\"invocation_id\"))',
        "if invocation_id != task_id",
        ".and_then(|metadata| metadata.get(DELIVERY_BINDING_FIELD))",
        "binding.len() != 8",
    ), "adapter response boundary")
    failures += require(sources["relay_response"], (
        'if action == "duplicate_event"',
        'return Err("adapter_duplicate_outcome_unknown")',
    ), "relay unknown-outcome boundary")

    failures += require(sources["migration_v3"], (
        "create or replace function public.cex_matrix_reconcile_adapter_result_v3(",
        "matrix_adapter_result_task_invocation_mismatch_v3",
        "{source,metadata,metadata,cex_delivery_binding}",
        "embedded_binding_key_count <> 8",
        "matrix_adapter_result_request_fingerprint_mismatch_v3",
        "revoke all on function public.cex_matrix_reconcile_adapter_result_v2(",
        "from public, cex_matrix_reconciler_runtime",
        "grant execute on function public.cex_matrix_reconcile_adapter_result_v3(",
        "to cex_matrix_reconciler_runtime",
    ), "operator migration v3")
    failures += forbid(sources["migration_v3"], ("grant all", " to public;"), "operator migration v3")
    failures += require(sources["runner"], (
        '"0006_adapter_result_embedded_delivery_binding.sql"',
        '"scripts/test-matrix-result-embedded-binding-postgres.sql"',
        '"scripts/test-matrix-result-task-invocation-binding-postgres.sql"',
        'SCHEMA = "cex.matrix-operator-postgres-regression.v4"',
        "SECURITY_MIGRATIONS",
        "SECURITY_REGRESSIONS",
    ), "operator runner v4")
    failures += require(sources["embedded_regression"], (
        "matrix_embedded_binding_v2_runtime_execute_not_revoked",
        "matrix_embedded_binding_v3_runtime_execute_missing",
        "matrix_embedded_binding_extra_field_accepted",
    ), "embedded binding regression")
    failures += require(sources["task_regression"], (
        "matrix_task_invocation_missing_accepted",
        "matrix_task_invocation_mismatch_accepted",
        "matrix_adapter_result_task_invocation_mismatch_v3",
    ), "task invocation regression")

    failures += require(sources["historical_loader"], (
        'IMPLEMENTATION_PATH = ROOT / "scripts/reconcile-matrix-adapter-result-v2-internal.py"',
        'if __name__ == "__main__" and sys.argv[1:] != DIRECT_SELF_TEST',
        "matrix_adapter_result_v2_historical_core_not_runnable",
        "metadata.st_mode & 0o111",
        "_IMPLEMENTATION.parse_lookup_response = parse_lookup_response",
        "_IMPLEMENTATION.build_sql = build_sql",
    ), "historical v2 import-only loader")
    failures += require(sources["historical_implementation"], (
        'SECURITY_CONTRACT = "v2"',
        "cex_matrix_reconcile_adapter_result_v2",
        '"PGSSLMODE"] = "verify-full"',
        '"PGCHANNELBINDING"] = "require"',
        "pinned_psql_path_required",
    ), "historical v2 implementation")
    for name in ("historical_loader", "historical_implementation"):
        if loaded[name][1] & 0o111:
            failures.append(f"{name}: executable bit must remain cleared")

    failures += require(sources["v3_implementation"], (
        'CORE_PATH = ROOT / "scripts/reconcile-matrix-adapter-result-v2-core.py"',
        'SCHEMA = "cex.matrix.adapter-result-reconciler.v3"',
        'SECURITY_CONTRACT = "v3"',
        "validate_persisted_binding(",
        'raw.get("invocation_id") != task_id',
        "if set(binding) != required",
        "V2_FUNCTION",
        "V3_FUNCTION",
        "rewritten.count(V3_FUNCTION) != 1",
        "metadata.st_mode & 0o111",
    ), "v3 reconciler implementation")
    failures += forbid(sources["v3_implementation"], (
        'CORE_PATH = ROOT / "scripts/reconcile-matrix-adapter-result.py"',
        "shell=True",
        "eval(",
        "exec(",
    ), "v3 reconciler implementation")

    failures += require(sources["canonical_command"], (
        'V3_PATH = ROOT / "scripts/reconcile-matrix-adapter-result-v3.py"',
        'SCHEMA = "cex.matrix.adapter-result-reconciler.v3"',
        'SECURITY_CONTRACT = "v3"',
        "def load_v3()",
        "reconciler_v3_identity_mismatch",
    ), "canonical v3 command")
    failures += forbid(sources["canonical_command"], (
        "cex_matrix_reconcile_adapter_result_v2",
        "reconcile-matrix-adapter-result-v2-core.py",
        "reconcile-matrix-adapter-result-v2-internal.py",
        "shell=True",
        "eval(",
        "exec(",
    ), "canonical v3 command")

    try:
        traceability = json.loads(sources["traceability"], object_pairs_hook=unique_object)
    except (json.JSONDecodeError, ValueError):
        failures.append("Matrix v3 traceability: invalid JSON")
    else:
        expected = {
            "schema": "cex.sequence54-matrix-result-reconciliation-traceability.v3",
            "runtime_command": "scripts/reconcile-matrix-adapter-result.py",
            "runtime_implementation": "scripts/reconcile-matrix-adapter-result-v3.py",
            "historical_v2_loader": "scripts/reconcile-matrix-adapter-result-v2-core.py",
            "historical_v2_implementation": "scripts/reconcile-matrix-adapter-result-v2-internal.py",
            "operator_migration_head": "services/matrix-entry-adapter/operator-migrations/0006_adapter_result_embedded_delivery_binding.sql",
            "production_authorization": "not_granted",
        }
        for field, value in expected.items():
            if traceability.get(field) != value:
                failures.append(f"Matrix v3 traceability: {field} mismatch")
    failures += require(sources["design"], (
        "# Matrix result reconciliation security contract v3",
        "source.metadata.metadata.cex_delivery_binding",
        "scripts/reconcile-matrix-adapter-result.py",
        "scripts/reconcile-matrix-adapter-result-v3.py",
        "scripts/reconcile-matrix-adapter-result-v2-core.py",
        "scripts/reconcile-matrix-adapter-result-v2-internal.py",
        "cex_matrix_reconcile_adapter_result_v3",
        "production_authorization=not_granted",
    ), "Matrix v3 design")
    failures += require(sources["workflow"], (
        "python3 scripts/check-matrix-result-reconciliation.py",
        "python3 scripts/check-matrix-result-reconciliation-security-v2.py",
        "python3 scripts/check-matrix-result-reconciliation-security-v3.py",
        "python3 scripts/check-matrix-result-reconciliation-traceability-v3.py",
        "python3 scripts/matrix_operator_postgres_regression.py",
    ), "hosted Matrix gate")
    failures += historical_direct_probe()
    failures += canonical_self_test()

    if failures:
        print("Matrix result reconciliation v3 cutover contract failed:", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1

    print(json.dumps({
        "schema": "cex.matrix.result-reconciliation-source-check.v3",
        "status": "ok",
        "consumer_lookup": "/v1/matrix/messages/result",
        "adapter_lookup": "/v1/matrix/results/lookup",
        "read_only_lookup": True,
        "principal_bound": True,
        "stable_replay_snapshot": True,
        "embedded_delivery_binding": True,
        "task_invocation_binding": True,
        "historical_v2_preserved": True,
        "historical_v2_direct_invocation": "rejected",
        "runtime_entrypoint": "cex_matrix_reconcile_adapter_result_v3",
        "canonical_runtime_command": "scripts/reconcile-matrix-adapter-result.py",
        "runtime_reconciler_self_test": os.name == "posix",
        "portable_contract_self_test": os.name == "nt",
        "operator_postgres_runner_present": True,
        "problems": [],
        "checker_max_grant_production_authorization": False,
        "production_authorization": "not_granted",
    }, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
