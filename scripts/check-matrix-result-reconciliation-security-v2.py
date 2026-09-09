#!/usr/bin/env python3
"""Fail closed on Matrix result-reconciliation security contract v2."""
from __future__ import annotations

import json
from pathlib import Path
import stat
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
FILES = {
    "consumer": "services/consumer-entry-api/src/matrix_result_lookup.rs",
    "adapter": "services/matrix-entry-adapter/src/result_reconciliation.rs",
    "migration": (
        "services/matrix-entry-adapter/operator-migrations/"
        "0005_adapter_result_causal_binding.sql"
    ),
    "regression": "scripts/test-matrix-result-causal-binding-postgres.sql",
    "runner": "scripts/matrix_operator_postgres_regression.py",
    "command": "scripts/reconcile-matrix-adapter-result.py",
    "workflow": ".github/workflows/matrix-review-repair-regression.yml",
    "design": "docs/matrix-result-reconciliation-v2.md",
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
            timeout=30,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return ["runtime command: self-test could not execute"]
    if result.returncode != 0:
        return ["runtime command: self-test failed"]
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError:
        return ["runtime command: self-test output is not JSON"]
    if (
        payload.get("schema") != "cex.matrix.adapter-result-reconciler.v1"
        or payload.get("status") != "ok"
        or payload.get("self_test") is not True
        or payload.get("security_contract") != "v2"
        or payload.get("production_authorization") != "not_granted"
    ):
        return ["runtime command: self-test output contract mismatch"]
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
    shared_binding = (
        'DELIVERY_FINGERPRINT_DOMAIN: &str = '
        '"cex.matrix.adapter-result-delivery.v1"',
        "delivery_id: String",
        "payload_sha256: String",
        "request_fingerprint: String",
        "request.request_fingerprint != expected",
        "hasher.update((value.len() as u64).to_be_bytes())",
    )
    failures += require(
        sources["consumer"],
        shared_binding
        + (
            '"delivery_id": request.delivery_id',
            '"payload_sha256": request.payload_sha256',
            '"request_fingerprint": request.request_fingerprint',
            "claims.request_fingerprint.as_deref()",
        ),
        "consumer delivery-bound lookup",
    )
    failures += require(
        sources["adapter"],
        shared_binding
        + (
            '"delivery_id": &request.delivery_id',
            '"payload_sha256": &request.payload_sha256',
            '"request_fingerprint": &request.request_fingerprint',
            '"schema": "cex.matrix.adapter-result-reconciliation.v2"',
            '"causal_binding": "delivery_payload_fingerprint"',
            "request_fingerprint: Some(fingerprint)",
        ),
        "adapter signed delivery binding",
    )

    failures += require(
        sources["command"],
        (
            'SECURITY_CONTRACT = "v2"',
            'DELIVERY_FINGERPRINT_DOMAIN = '
            '"cex.matrix.adapter-result-delivery.v1"',
            'EVIDENCE_SCHEMA = '
            '"cex.matrix.adapter-result-reconciliation-evidence.v2"',
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
            "security_contract",
        ),
        "operator command",
    )
    failures += forbid(
        sources["command"],
        (
            "environment = os.environ.copy()",
            'shutil.which("psql")',
            '"PGSSLMODE": "prefer"',
            "sslmode=prefer",
            "sslmode=require",
            "verify=False",
            "shell=True",
            'host.lower() == "localhost"',
        ),
        "operator command fail-closed transport",
    )

    failures += require(
        sources["migration"],
        (
            "matrix_transport_adapter_result_observations",
            "request_fingerprint",
            "cex_matrix_reconcile_adapter_result_v2",
            "cex.matrix.adapter-result-delivery.v1",
            "int8send(octet_length(p_delivery_id::text)::bigint)",
            "matrix_adapter_result_request_fingerprint_mismatch",
            "matrix_adapter_result_observation_outside_window",
            "interval '15 minutes'",
            "on conflict do nothing",
            "matrix_adapter_result_reconciliation_collision_v2",
            "from cex_matrix_reconciler_runtime",
            "to cex_matrix_reconciler_runtime",
            "adapter_result_reconciled_v2",
        ),
        "operator migration v2",
    )
    failures += forbid(
        sources["migration"],
        (
            "existing.evidence_context is distinct from p_evidence_context",
            "grant all",
            " to public;",
        ),
        "operator migration v2",
    )

    failures += require(
        sources["regression"],
        (
            "matrix_causal_honest_retry_not_idempotent",
            "matrix_causal_observation_append_count_mismatch",
            "matrix_causal_changed_fingerprint_not_rejected",
            "matrix_causal_stale_observation_not_rejected",
            "matrix_causal_changed_task_not_rejected",
            "matrix_causal_v1_runtime_execute_not_revoked",
            "matrix_causal_v2_runtime_execute_missing",
        ),
        "causal-binding PostgreSQL regression",
    )
    failures += require(
        sources["runner"],
        (
            '"0005_adapter_result_causal_binding.sql"',
            '"scripts/test-matrix-result-causal-binding-postgres.sql"',
            'SCHEMA = "cex.matrix-operator-postgres-regression.v4"',
            "BASE_MIGRATIONS",
            "BASE_REGRESSIONS",
            "for pass_number in (1, 2):",
        ),
        "operator migration runner",
    )
    failures += require(
        sources["workflow"],
        (
            "python3 scripts/check-matrix-result-reconciliation-security-v2.py",
            "python3 scripts/matrix_operator_postgres_regression.py",
        ),
        "hosted Matrix gate wiring",
    )
    failures += require(
        sources["design"],
        (
            "# Matrix result reconciliation security contract v2",
            "delivery_id",
            "payload_sha256",
            "request_fingerprint",
            "verify-full",
            "append-only observation",
            "production_authorization=not_granted",
        ),
        "security design",
    )
    failures += reconciler_self_test()

    if failures:
        print(
            "Matrix result reconciliation security contract v2 failed:",
            file=sys.stderr,
        )
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1

    print(
        json.dumps(
            {
                "schema": (
                    "cex.matrix.result-reconciliation-security-source-check.v1"
                ),
                "status": "ok",
                "causal_delivery_binding": True,
                "honest_retry_idempotent": True,
                "append_only_observations": True,
                "remote_database_tls": "verify-full",
                "closed_psql_environment": True,
                "pinned_psql_executable": True,
                "operator_migration_head": "0005-preserved-before-v3",
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
