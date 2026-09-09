#!/usr/bin/env python3
"""Execute the exact Matrix operator migration chain and reconciliation/role SQL."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import secrets
import shutil
import subprocess
import sys
from typing import Callable

import matrix_postgres_regression as base

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = (
    "0001_adapter_result_reconciliation.sql",
    "0002_runtime_roles.sql",
    "0003_adapter_result_evidence_binding.sql",
    "0004_adapter_result_runtime_reconciliation.sql",
    "0005_adapter_result_causal_binding.sql",
)
REGRESSIONS = (
    "scripts/test-matrix-result-reconciliation-postgres.sql",
    "scripts/test-matrix-result-evidence-hardening-postgres.sql",
    "scripts/test-matrix-result-runtime-reconciliation-postgres.sql",
    "scripts/test-matrix-result-causal-binding-postgres.sql",
)
SCHEMA = "cex.matrix-operator-postgres-regression.v3"
MAX_RUN_SECONDS = 900


class OperatorRegressionError(RuntimeError):
    pass


def acquired_inputs(root: Path) -> tuple[list[tuple[str, str]], dict[str, str]]:
    root = base.plain_path(root)
    directory = base.plain_path(
        root / "services/matrix-entry-adapter/operator-migrations"
    )
    present = sorted(
        path.name for path in directory.glob("[0-9][0-9][0-9][0-9]_*.sql")
    )
    if present != list(MIGRATIONS):
        raise OperatorRegressionError(
            "Matrix operator migration manifest does not match source"
        )

    inputs: dict[str, str] = {}
    stages: list[tuple[str, str]] = []
    for pass_number in (1, 2):
        for name in MIGRATIONS:
            relative = (
                f"services/matrix-entry-adapter/operator-migrations/{name}"
            )
            if relative not in inputs:
                inputs[relative] = base.read_input(root, relative)
            sql = inputs[relative]
            if (
                not sql.lstrip().startswith("begin;")
                or not sql.rstrip().endswith("commit;")
            ):
                raise OperatorRegressionError(
                    f"nontransactional operator migration: {name}"
                )
            stages.append((f"operator-migration-{pass_number}-{name}", sql))

    for relative in REGRESSIONS:
        inputs[relative] = base.read_input(root, relative)
        stages.append((Path(relative).stem, inputs[relative]))

    for _, sql in stages:
        base.validate_sql_source(sql)
    hashes = {
        relative: hashlib.sha256(text.encode("utf-8")).hexdigest()
        for relative, text in sorted(inputs.items())
    }
    return stages, hashes


def session_sql(
    database: str,
    stages: list[tuple[str, str]],
    nonce: str,
) -> tuple[str, list[str]]:
    if not database or not nonce or len(nonce) != 32:
        raise OperatorRegressionError(
            "invalid operator regression session identity"
        )
    # Reuse the established parser's exact closed marker prefix; operator stage
    # names and evidence schema provide the namespace distinction.
    prefix = "CEX_MATRIX_" + nonce
    safety = f"""do $matrix_operator_identity$
begin
  if current_database() <> '{database}'
      or current_setting('server_version_num')::integer not between 160000 and 169999 then
    raise exception 'matrix_operator_test_database_or_version_mismatch';
  end if;
  if not pg_try_advisory_lock(1128618057, 1296127058) then
    raise exception 'matrix_test_runner_busy';
  end if;
end;
$matrix_operator_identity$;
select '{prefix}:version:' || current_setting('server_version_num');
"""
    base_schema = """do $matrix_operator_base$
begin
  if to_regprocedure('public.cex_matrix_acquire_cursor_lease_v1(text,text,integer)') is null
      or to_regprocedure('public.cex_matrix_finish_delivery_v1(uuid,text,bigint,text,text)') is null
      or to_regclass('public.matrix_transport_outbox') is null
      or to_regclass('public.matrix_transport_delivery_history') is null then
    raise exception 'matrix_operator_base_schema_missing';
  end if;
end;
$matrix_operator_base$;
"""
    all_stages = [
        ("server-identity", safety),
        ("base-schema", base_schema),
        *stages,
    ]
    script = (
        "\\set ON_ERROR_STOP on\n"
        "set search_path = pg_catalog, public;\n"
        "set standard_conforming_strings = on;\n"
    )
    for name, sql in all_stages:
        if not name or any(
            character
            not in (
                "abcdefghijklmnopqrstuvwxyz"
                "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
                "0123456789_.-"
            )
            for character in name
        ):
            raise OperatorRegressionError(
                "invalid operator regression stage name"
            )
        script += (
            f"\\echo {prefix}:start:{name}\n"
            f"{sql}\n"
            f"\\echo {prefix}:ok:{name}\n"
        )
    return script, [name for name, _ in all_stages]


def execute(
    root: Path,
    environment: dict[str, str],
    invoke: Callable | None = None,
) -> dict:
    pg = base.database_environment(environment)
    stages, hashes = acquired_inputs(root)
    client = shutil.which("psql", path=pg["PATH"])
    if not client:
        raise OperatorRegressionError(
            "psql is required; operator regressions were not executed"
        )

    result: dict = {
        "schema": SCHEMA,
        "status": "failed",
        "source_sha256": hashes,
        "stages": [],
        "production_authorization": "not_granted",
        "scope": "disposable-database-operator-regression-only",
        "execution_model": "single_psql_session_after_transport_schema",
        "session_exit_code": None,
    }
    nonce = secrets.token_hex(16)
    script, names = session_sql(pg["PGDATABASE"], stages, nonce)
    try:
        response = (invoke or base.bounded_client)(
            [
                client,
                "-X",
                "--no-password",
                "-q",
                "-A",
                "-t",
                "-v",
                "ON_ERROR_STOP=1",
                "-f",
                "-",
            ],
            input=script,
            env=pg,
            timeout=MAX_RUN_SECONDS,
        )
        result["session_exit_code"] = response.returncode
        records, version, complete = base.parse_observation(
            response.stdout,
            names,
            nonce,
        )
        result["stages"] = records
        if version is not None:
            result["server_version_num"] = version
        if response.returncode != 0 or not complete:
            raise OperatorRegressionError(
                "Matrix operator SQL session failed or completion markers "
                "were incomplete"
            )
        if acquired_inputs(root)[1] != hashes:
            raise OperatorRegressionError(
                "operator regression source changed during execution"
            )
        result["status"] = "ok"
        result["database"] = pg["PGDATABASE"]
    except (
        OperatorRegressionError,
        base.RegressionError,
        OSError,
        UnicodeError,
        subprocess.TimeoutExpired,
    ) as error:
        if isinstance(
            error,
            (OperatorRegressionError, base.RegressionError),
        ):
            result["error"] = str(error)
        else:
            result["error"] = "Matrix operator SQL client could not complete"
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", type=Path)
    args = parser.parse_args()
    try:
        destination = (
            base.checked_output(ROOT, args.evidence)
            if args.evidence
            else None
        )
    except (base.RegressionError, OSError):
        print(
            "invalid evidence output; no database operation attempted",
            file=sys.stderr,
        )
        return 1
    try:
        result = execute(ROOT, dict(__import__("os").environ))
    except (
        OperatorRegressionError,
        base.RegressionError,
        OSError,
        UnicodeError,
    ) as error:
        result = {
            "schema": SCHEMA,
            "status": "failed",
            "error": (
                str(error)
                if isinstance(
                    error,
                    (OperatorRegressionError, base.RegressionError),
                )
                else "operator regression input unavailable"
            ),
            "stages": [],
            "production_authorization": "not_granted",
        }
    result["observed_at"] = datetime.now(timezone.utc).isoformat()
    text = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if destination:
        try:
            base.write_evidence(ROOT, destination, text)
        except (base.RegressionError, OSError):
            print(
                "operator evidence publication failed; no qualification "
                "granted",
                file=sys.stderr,
            )
            return 1
    print(text, end="")
    return 0 if result["status"] == "ok" else 1


if __name__ == "__main__":
    raise SystemExit(main())
