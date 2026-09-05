#!/usr/bin/env python3
"""Run every Matrix SQL regression against one fully migrated disposable DB.

No shell evaluation, application data, host credentials or deployment activation.
Unit tests of this runner are orchestration tests, not PostgreSQL execution proof.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
from typing import Callable
from urllib.parse import parse_qsl, unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = (
    "0001_transport_durability.sql",
    "0002_source_observation_replay.sql",
    "0003_sync_recovery_and_send_receipts.sql",
)
BASELINE = "scripts/check-matrix-transport-postgres.sh"
REGRESSIONS = (
    "scripts/test-matrix-source-observation-replay.sql",
    "scripts/test-matrix-sync-recovery-postgres.sql",
)
MAX_INPUT = 2 * 1024 * 1024
BASELINE_MARKER = "psql -X -q -v ON_ERROR_STOP=1 <<'SQL'\n"


class RegressionError(RuntimeError):
    pass


def database_environment(environment: dict[str, str]) -> dict[str, str]:
    if environment.get("MATRIX_TEST_ALLOW_SCHEMA_RESET") != "1":
        raise RegressionError("explicit disposable database reset consent is required")
    raw = environment.get("MATRIX_TEST_DATABASE_URL", "")
    try:
        if not raw or any(ord(c) < 32 for c in raw) or re.search(r"%(?![0-9a-fA-F]{2})", raw):
            raise ValueError()
        url = urlsplit(raw)
        database = unquote(url.path.removeprefix("/"), errors="strict")
        username = unquote(url.username or "", errors="strict")
        password = unquote(url.password or "", errors="strict")
        if (url.scheme not in {"postgres", "postgresql"} or not url.hostname
                or url.fragment or not username or not password
                or not re.fullmatch(r"(?:matrix_[a-z0-9_]*_ci|cex_matrix_test_[a-z0-9_]+)", database)):
            raise ValueError()
        port = 5432 if url.port is None else url.port
        if not 1 <= port <= 65535:
            raise ValueError()
        options: dict[str, str] = {}
        for key, value in parse_qsl(url.query, keep_blank_values=True, strict_parsing=True):
            if key not in {"sslmode", "connect_timeout"} or key in options:
                raise ValueError()
            options[key] = value
        sslmode = options.get("sslmode", "prefer")
        connect_timeout = options.get("connect_timeout", "5")
        if sslmode not in {"disable", "prefer", "require", "verify-ca", "verify-full"}:
            raise ValueError()
        if not connect_timeout.isdigit() or not 1 <= int(connect_timeout) <= 10:
            raise ValueError()
        if any("\x00" in value or "\n" in value or "\r" in value for value in [username, password, url.hostname, database]):
            raise ValueError()
    except (ValueError, UnicodeError):
        raise RegressionError("invalid MATRIX_TEST_DATABASE_URL or non-disposable database name") from None
    # Deliberately do not inherit PGSERVICE, PGOPTIONS, LD_PRELOAD, shell hooks,
    # .pgpass or a production DATABASE_URL. Credentials never enter argv.
    return {
        "PATH": environment.get("PATH", os.defpath),
        "LANG": "C.UTF-8",
        "PGHOST": url.hostname,
        "PGPORT": str(port),
        "PGDATABASE": database,
        "PGUSER": username,
        "PGPASSWORD": password,
        "PGSSLMODE": sslmode,
        "PGCONNECT_TIMEOUT": connect_timeout,
        "PGPASSFILE": os.devnull,
        "PSQLRC": os.devnull,
        "PGOPTIONS": "-c statement_timeout=30000 -c lock_timeout=5000 -c idle_in_transaction_session_timeout=30000",
    }


def read_input(root: Path, relative: str) -> str:
    path = root / relative
    if path.is_symlink() or not path.is_file() or path.stat().st_size > MAX_INPUT:
        raise RegressionError(f"missing, linked or oversized regression input: {relative}")
    try:
        path.resolve().relative_to(root.resolve())
        return path.read_bytes().decode("utf-8")
    except (ValueError, UnicodeError, OSError):
        raise RegressionError(f"invalid regression input: {relative}") from None


def baseline_sql(script: str) -> str:
    # Retain the existing SQL assertion body verbatim. Never execute its shell
    # URL/eval/bootstrap wrapper, which would reinstall obsolete functions.
    if script.count(BASELINE_MARKER) != 1:
        raise RegressionError("original Matrix SQL regression delimiter is missing or ambiguous")
    body = script.split(BASELINE_MARKER, 1)[1]
    if "\nSQL\n" not in body:
        raise RegressionError("original Matrix SQL regression is unterminated")
    sql = body.split("\nSQL\n", 1)[0] + "\n"
    if "DO $test$" not in sql or "matrix_source_event_identity_collision" not in sql:
        raise RegressionError("original Matrix behavioral assertions are missing")
    return sql


def input_stages(root: Path) -> tuple[list[tuple[str, str]], dict[str, str]]:
    directory = root / "services/matrix-entry-adapter/migrations"
    present = sorted(path.name for path in directory.glob("[0-9][0-9][0-9][0-9]_*.sql"))
    if present != list(MIGRATIONS):
        raise RegressionError("Matrix migration manifest does not match the source directory")
    inputs: dict[str, str] = {}
    for name in MIGRATIONS:
        path = f"services/matrix-entry-adapter/migrations/{name}"
        inputs[path] = read_input(root, path)
        if not inputs[path].lstrip().startswith("begin;") or not inputs[path].rstrip().endswith("commit;"):
            raise RegressionError(f"nontransactional migration: {name}")
    inputs[BASELINE] = read_input(root, BASELINE)
    for path in REGRESSIONS:
        inputs[path] = read_input(root, path)
    stages: list[tuple[str, str]] = []
    # Reapply the COMPLETE chain, not 0001 after a newer migration and then run
    # tests in that downgraded interval. No application operations occur here.
    for iteration in (1, 2):
        for name in MIGRATIONS:
            stages.append((f"migration-{iteration}-{name}", inputs[f"services/matrix-entry-adapter/migrations/{name}"]))
    stages.append(("original-transport-regression", baseline_sql(inputs[BASELINE])))
    stages.extend((Path(path).stem, inputs[path]) for path in REGRESSIONS)
    hashes = {path: hashlib.sha256(text.encode()).hexdigest() for path, text in inputs.items()}
    return stages, hashes


# Remove only transport test rows; reject a database containing unrelated tables
# before this executes. Unknown tables are never treated as disposable CEX data.
RESET_SQL = """do $reset$
declare item record;
begin
  for item in select tablename from pg_catalog.pg_tables
    where schemaname = 'public' and tablename like 'matrix\\_transport\\_%' escape '\\' loop
    execute format('truncate table public.%I restart identity', item.tablename);
  end loop;
end;
$reset$;
"""
FOREIGN_TABLES_SQL = """select count(*) from pg_catalog.pg_tables
where schemaname not in ('pg_catalog','information_schema')
  and schemaname not like 'pg_toast%'
  and (schemaname <> 'public' or tablename not like 'matrix\\_transport\\_%' escape '\\');
"""


def execute(root: Path, environment: dict[str, str], invoke: Callable = subprocess.run) -> dict:
    pg = database_environment(environment)
    stages, hashes = input_stages(root)
    client = shutil.which("psql", path=pg["PATH"])
    if not client:
        raise RegressionError("psql is required; PostgreSQL regressions were not executed")
    result = {
        "schema": "cex.matrix-postgres-regression.v3",
        "status": "failed",
        "source_sha256": hashes,
        "stages": [],
        "production_authorization": "not_granted",
        "scope": "disposable-database-regression-only",
    }

    def run(name: str, sql: str) -> str:
        try:
            response = invoke([client, "-X", "--no-password", "-q", "-A", "-t", "-v", "ON_ERROR_STOP=1", "-f", "-"],
                              input=sql, env=pg, text=True, capture_output=True, timeout=180, check=False)
        except (OSError, subprocess.TimeoutExpired):
            result["stages"].append({"name": name, "exit_code": None, "status": "not_completed"})
            raise RegressionError(f"{name}: PostgreSQL command could not complete") from None
        result["stages"].append({"name": name, "exit_code": response.returncode,
                                 "status": "passed" if response.returncode == 0 else "failed"})
        if response.returncode != 0:
            # The inputs are fixed regression fixtures, but connection messages
            # can contain secrets. Do not print raw server/client diagnostics.
            raise RegressionError(f"{name}: PostgreSQL regression failed (exit {response.returncode})")
        return response.stdout.strip()

    try:
        identity = run("server-identity", "show server_version_num; select current_database();\n").splitlines()
        if len(identity) != 2 or not identity[0].isdigit() or not 160000 <= int(identity[0]) < 170000 or identity[1] != pg["PGDATABASE"]:
            raise RegressionError("PostgreSQL 16 and the exact disposable database are required")
        result["server_version_num"] = int(identity[0])
        result["database"] = identity[1]
        if run("foreign-table-guard", FOREIGN_TABLES_SQL) != "0":
            raise RegressionError("refusing to reset a database with unrelated tables")
        run("reset-transport-test-rows", RESET_SQL)
        for name, sql in stages:
            run(name, sql)
        result["status"] = "ok"
    except RegressionError as error:
        result["error"] = str(error)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", type=Path)
    args = parser.parse_args()
    try:
        result = execute(ROOT, dict(os.environ))
    except RegressionError as error:
        result = {"schema": "cex.matrix-postgres-regression.v3", "status": "failed",
                  "error": str(error), "stages": [], "production_authorization": "not_granted"}
    result["observed_at"] = datetime.now(timezone.utc).isoformat()
    text = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if args.evidence:
        destination = args.evidence if args.evidence.is_absolute() else ROOT / args.evidence
        if destination.is_symlink() or destination.suffix != ".json":
            raise SystemExit("invalid evidence output")
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(text, encoding="utf-8")
    print(text, end="")
    return 0 if result["status"] == "ok" else 1


if __name__ == "__main__":
    raise SystemExit(main())
