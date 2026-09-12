#!/usr/bin/env python3
"""Preserve Matrix reconciliation v2 history while proving runtime cutover to v3."""
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
    "consumer": "services/consumer-entry-api/src/matrix_result_lookup.rs",
    "adapter": "services/matrix-entry-adapter/src/result_reconciliation.rs",
    "migration_v2": "services/matrix-entry-adapter/operator-migrations/0005_adapter_result_causal_binding.sql",
    "migration_v3": "services/matrix-entry-adapter/operator-migrations/0006_adapter_result_embedded_delivery_binding.sql",
    "regression": "scripts/test-matrix-result-causal-binding-postgres.sql",
    "runner": "scripts/matrix_operator_postgres_regression.py",
    "historical_loader": "scripts/reconcile-matrix-adapter-result-v2-core.py",
    "historical_implementation": "scripts/reconcile-matrix-adapter-result-v2-internal.py",
    "canonical_command": "scripts/reconcile-matrix-adapter-result.py",
    "workflow": ".github/workflows/matrix-review-repair-regression.yml",
    "design": "docs/matrix-result-reconciliation-v2.md",
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
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON member")
        result[key] = value
    return result


def self_test(path: str, schema: str, contract: str, label: str) -> list[str]:
    if os.name == "nt":
        return portable_self_test(ROOT, contract)
    try:
        result = subprocess.run(
            [sys.executable, str(ROOT / path), "--self-test"],
            cwd=ROOT,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=45,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return [f"{label}: self-test could not execute"]
    if result.returncode != 0:
        return [f"{label}: self-test failed"]
    try:
        payload = json.loads(result.stdout, object_pairs_hook=unique_object)
    except (json.JSONDecodeError, ValueError):
        return [f"{label}: self-test output is invalid JSON"]
    if payload != {
        "production_authorization": "not_granted",
        "schema": schema,
        "security_contract": contract,
        "self_test": True,
        "status": "ok",
    }:
        return [f"{label}: self-test output contract mismatch"]
    return []


def direct_v2_invocation_rejected() -> list[str]:
    try:
        result = subprocess.run(
            [sys.executable, str(ROOT / FILES["historical_loader"])],
            cwd=ROOT,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=10,
            check=False,
            env=probe_environment(),
        )
    except (OSError, subprocess.TimeoutExpired):
        return ["historical v2 loader: negative execution probe failed"]
    if result.returncode == 0:
        return ["historical v2 loader: direct invocation unexpectedly succeeded"]
    if result.stdout or result.stderr.strip() != "matrix_adapter_result_v2_historical_core_not_runnable":
        return ["historical v2 loader: direct invocation did not fail with bounded code"]
    return []


def main() -> int:
    try:
        loaded = {name: read_regular(path) for name, path in FILES.items()}
    except RuntimeError as error:
        print(error, file=sys.stderr)
        return 1
    sources = {name: value[0] for name, value in loaded.items()}
    failures: list[str] = []

    shared_binding = (
        '"cex.matrix.adapter-result-delivery.v1"',
        "delivery_id: String",
        "payload_sha256: String",
        "request_fingerprint: String",
        "hasher.update((value.len() as u64).to_be_bytes())",
    )
    failures += require(sources["consumer"], shared_binding, "consumer v2 causal binding")
    failures += require(sources["adapter"], shared_binding + (
        '"schema": "cex.matrix.adapter-result-reconciliation.v2"',
        '"causal_binding": "delivery_payload_fingerprint"',
    ), "adapter v2 causal binding")

    failures += require(sources["historical_loader"], (
        'IMPLEMENTATION_PATH = ROOT / "scripts/reconcile-matrix-adapter-result-v2-internal.py"',
        'DIRECT_SELF_TEST = ["--self-test"]',
        'if __name__ == "__main__" and sys.argv[1:] != DIRECT_SELF_TEST',
        "matrix_adapter_result_v2_historical_core_not_runnable",
        "IMPLEMENTATION_PATH.is_symlink()",
        "metadata.st_mode & 0o111",
        "historical_v2_implementation_identity_mismatch",
        "_IMPLEMENTATION.parse_lookup_response = parse_lookup_response",
        "_IMPLEMENTATION.build_sql = build_sql",
        "_IMPLEMENTATION.SCHEMA = SCHEMA",
        "_IMPLEMENTATION.SECURITY_CONTRACT = SECURITY_CONTRACT",
    ), "historical v2 import-only loader")
    failures += forbid(sources["historical_loader"], (
        "cex_matrix_reconcile_adapter_result_v2(",
        "shell=True",
        "eval(",
        "exec(",
    ), "historical v2 import-only loader")

    failures += require(sources["historical_implementation"], (
        'SCHEMA = "cex.matrix.adapter-result-reconciler.v1"',
        'SECURITY_CONTRACT = "v2"',
        'EVIDENCE_SCHEMA = "cex.matrix.adapter-result-reconciliation-evidence.v2"',
        "delivery_request_fingerprint(",
        '"PGSSLMODE"] = "verify-full"',
        '"PGSSLROOTCERT"',
        '"PGCHANNELBINDING"] = "require"',
        "pinned_psql_path_required",
        "pinned_psql_unavailable_or_unsafe",
        "resolved = path.resolve(strict=True)",
        "metadata.st_uid not in trusted_owners",
        "parent.st_mode & 0o022",
        "ipaddress.ip_address(host).is_loopback",
        "def psql_environment(",
        '"PGPASSFILE": os.devnull',
        '"PSQLRC": os.devnull',
        "cex_matrix_reconcile_adapter_result_v2",
        "--allow-insecure-database-loopback",
    ), "historical v2 implementation")
    failures += forbid(sources["historical_implementation"], (
        "environment = os.environ.copy()",
        'shutil.which("psql")',
        '"PGSSLMODE": "prefer"',
        "sslmode=prefer",
        "sslmode=require",
        "verify=False",
        "shell=True",
        'host.lower() == "localhost"',
    ), "historical v2 implementation")
    for name in ("historical_loader", "historical_implementation"):
        if loaded[name][1] & 0o111:
            failures.append(f"{name}: executable bit must remain cleared")

    failures += require(sources["canonical_command"], (
        'SCHEMA = "cex.matrix.adapter-result-reconciler.v3"',
        'SECURITY_CONTRACT = "v3"',
        'V3_PATH = ROOT / "scripts/reconcile-matrix-adapter-result-v3.py"',
        "def load_v3()",
        "reconciler_v3_identity_mismatch",
    ), "canonical runtime command")
    failures += forbid(sources["canonical_command"], (
        "cex_matrix_reconcile_adapter_result_v2",
        "reconcile-matrix-adapter-result-v2-core.py",
        "reconcile-matrix-adapter-result-v2-internal.py",
        "shell=True",
        "eval(",
        "exec(",
    ), "canonical runtime command")

    failures += require(sources["migration_v2"], (
        "matrix_transport_adapter_result_observations",
        "request_fingerprint",
        "cex_matrix_reconcile_adapter_result_v2",
        "cex.matrix.adapter-result-delivery.v1",
        "matrix_adapter_result_request_fingerprint_mismatch",
        "matrix_adapter_result_observation_outside_window",
        "interval '15 minutes'",
        "on conflict do nothing",
        "matrix_adapter_result_reconciliation_collision_v2",
        "adapter_result_reconciled_v2",
    ), "historical migration v2")
    failures += require(sources["migration_v3"], (
        "cex_matrix_reconcile_adapter_result_v3",
        "revoke all on function public.cex_matrix_reconcile_adapter_result_v2(",
        "from public, cex_matrix_reconciler_runtime",
        "grant execute on function public.cex_matrix_reconcile_adapter_result_v3(",
        "to cex_matrix_reconciler_runtime",
    ), "runtime cutover migration v3")
    failures += require(sources["regression"], (
        "matrix_causal_honest_retry_not_idempotent",
        "matrix_causal_observation_append_count_mismatch",
        "matrix_causal_changed_fingerprint_not_rejected",
        "matrix_causal_stale_observation_not_rejected",
        "matrix_causal_changed_task_not_rejected",
        "matrix_causal_v1_runtime_execute_not_revoked",
        "matrix_causal_v2_runtime_execute_missing",
    ), "v2 historical regression")
    failures += require(sources["runner"], (
        '"0005_adapter_result_causal_binding.sql"',
        '"0006_adapter_result_embedded_delivery_binding.sql"',
        '"scripts/test-matrix-result-causal-binding-postgres.sql"',
        'SCHEMA = "cex.matrix-operator-postgres-regression.v4"',
        "BASE_MIGRATIONS",
        "SECURITY_MIGRATIONS",
        "BASE_REGRESSIONS",
        "SECURITY_REGRESSIONS",
    ), "operator runner v4")
    failures += require(sources["workflow"], (
        "python3 scripts/check-matrix-result-reconciliation-security-v2.py",
        "python3 scripts/check-matrix-result-reconciliation-security-v3.py",
        "python3 scripts/matrix_operator_postgres_regression.py",
    ), "hosted Matrix gate")
    failures += require(sources["design"], (
        "# Matrix result reconciliation security contract v2",
        "delivery_id",
        "payload_sha256",
        "request_fingerprint",
        "verify-full",
        "append-only observation",
        "production_authorization=not_granted",
    ), "v2 historical design")

    failures += direct_v2_invocation_rejected()
    failures += self_test(
        FILES["historical_loader"],
        "cex.matrix.adapter-result-reconciler.v1",
        "v2",
        "historical v2 self-test",
    )
    failures += self_test(
        FILES["canonical_command"],
        "cex.matrix.adapter-result-reconciler.v3",
        "v3",
        "canonical v3 command",
    )

    if failures:
        print("Matrix result reconciliation v2 preservation/cutover contract failed:", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1
    print(json.dumps({
        "schema": "cex.matrix.result-reconciliation-security-source-check.v2",
        "status": "ok",
        "historical_v2_preserved": True,
        "historical_v2_direct_invocation": "rejected",
        "historical_v2_files_executable": False,
                "runtime_reconciler_self_test": os.name == "posix",
                "portable_contract_self_test": os.name == "nt",
        "runtime_entrypoint": "cex_matrix_reconcile_adapter_result_v3",
        "canonical_runtime_command": "scripts/reconcile-matrix-adapter-result.py",
        "problems": [],
        "checker_may_grant_production_authorization": False,
        "production_authorization": "not_granted",
    }, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
