#!/usr/bin/env python3
"""Run the complete Matrix SQL chain in one guarded disposable-DB session.

A local report is an execution observation, not independent hosted qualification.
The test harness uses explicit fake clients; those results are not SQL evidence.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import ipaddress
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
from typing import Callable
from urllib.parse import parse_qsl, unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = (
    "0001_transport_durability.sql",
    "0002_source_observation_replay.sql",
    "0003_sync_recovery_and_send_receipts.sql",
    "0004_stream_scope_binding.sql",
    "0005_filter_definition_pins.sql",
)
ENTRYPOINTS = (
    "scripts/check-matrix-transport-postgres.sh",
    "scripts/check-matrix-source-observation-postgres.sh",
)
RUNNER_SOURCE = "scripts/matrix_postgres_regression.py"
BASELINE = "scripts/test-matrix-transport-baseline.sql"
REGRESSIONS = (
    "scripts/test-matrix-source-observation-replay.sql",
    "scripts/test-matrix-sync-recovery-postgres.sql",
    "scripts/test-matrix-stream-scope-postgres.sql",
    "scripts/test-matrix-filter-definition-postgres.sql",
)
# A prefix is not an ownership contract: unknown similarly named tables fail.
TRANSPORT_TABLES = (
    "matrix_transport_cursors", "matrix_transport_inbox", "matrix_transport_outbox",
    "matrix_transport_delivery_history", "matrix_transport_poison_events",
    "matrix_transport_source_observations", "matrix_transport_poison_payloads",
    "matrix_transport_cursor_history", "matrix_transport_send_bindings",
    "matrix_transport_send_receipts", "matrix_transport_stream_scopes",
    "matrix_transport_filter_definitions",
)
MAX_INPUT = 2 * 1024 * 1024
MAX_OUTPUT = 1024 * 1024
MAX_RUN_SECONDS = 900
SCHEMA = "cex.matrix-postgres-regression.v4"


class RegressionError(RuntimeError):
    pass


def single_host(host: str) -> str:
    if not host or len(host) > 253 or any(c in host for c in ",/%\\"):
        raise ValueError("invalid single host")
    try:
        ipaddress.ip_address(host)
    except ValueError:
        labels = host.rstrip(".").split(".")
        if not all(re.fullmatch(r"[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?", label) for label in labels):
            raise ValueError("invalid single host") from None
    return host


def database_environment(environment: dict[str, str]) -> dict[str, str]:
    if environment.get("MATRIX_TEST_ALLOW_SCHEMA_RESET") != "1":
        raise RegressionError("explicit disposable database reset consent is required")
    raw = environment.get("MATRIX_TEST_DATABASE_URL", "")
    try:
        if not raw or len(raw) > 8192 or any(ord(c) < 32 or ord(c) == 127 for c in raw) or re.search(r"%(?![0-9a-fA-F]{2})", raw):
            raise ValueError()
        url = urlsplit(raw)
        database = unquote(url.path.removeprefix("/"), errors="strict")
        username = unquote(url.username or "", errors="strict")
        password = unquote(url.password or "", errors="strict")
        host = single_host(url.hostname or "")
        if (url.scheme not in {"postgres", "postgresql"} or url.fragment
                or not username or not password or len(username.encode()) > 63
                or len(password.encode()) > 4096 or len(database) > 63
                or not re.fullmatch(r"(?:matrix_[a-z0-9_]*_ci|cex_matrix_test_[a-z0-9_]+)", database)):
            raise ValueError()
        port = 5432 if url.port is None else url.port
        if not 1 <= port <= 65535:
            raise ValueError()
        options: dict[str, str] = {}
        for key, value in parse_qsl(url.query, keep_blank_values=True, strict_parsing=True, max_num_fields=2):
            if key not in {"sslmode", "connect_timeout"} or key in options:
                raise ValueError()
            options[key] = value
        sslmode = options.get("sslmode", "prefer")
        connect_timeout = options.get("connect_timeout", "5")
        if sslmode not in {"disable", "prefer", "require", "verify-ca", "verify-full"}:
            raise ValueError()
        if not re.fullmatch(r"(?:[1-9]|10)", connect_timeout):
            raise ValueError()
        if any(any(ord(c) < 32 or ord(c) == 127 for c in value) for value in (username, password, database)):
            raise ValueError()
    except (ValueError, UnicodeError):
        raise RegressionError("invalid MATRIX_TEST_DATABASE_URL or non-disposable database name") from None
    # No inherited service/options/credential file, loader or shell hooks.
    return {
        "PATH": environment.get("PATH", os.defpath), "LANG": "C.UTF-8",
        "PGHOST": host, "PGPORT": str(port), "PGDATABASE": database,
        "PGUSER": username, "PGPASSWORD": password, "PGSSLMODE": sslmode,
        "PGCONNECT_TIMEOUT": connect_timeout, "PGPASSFILE": os.devnull, "PSQLRC": os.devnull,
        "PGOPTIONS": "-c statement_timeout=30000 -c lock_timeout=5000 -c idle_in_transaction_session_timeout=30000",
    }


def plain_path(path: Path) -> Path:
    path = path.absolute()
    if ".." in path.parts or any(ord(c) < 32 or ord(c) == 127 for c in str(path)):
        raise RegressionError("invalid local regression path")
    if any(parent.is_symlink() for parent in (path, *path.parents)):
        raise RegressionError("linked regression path is forbidden")
    return path


def read_input(root: Path, relative: str) -> str:
    root = plain_path(root)
    path = plain_path(root / relative)
    try:
        path.relative_to(root)
        fd = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0))
        with os.fdopen(fd, "rb") as stream:
            before = os.fstat(stream.fileno())
            if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1 or before.st_size > MAX_INPUT:
                raise RegressionError("nonregular, linked or oversized regression input")
            data = stream.read(MAX_INPUT + 1)
            after = os.fstat(stream.fileno())
        fingerprint = lambda st: (st.st_dev, st.st_ino, st.st_size, st.st_mtime_ns, st.st_ctime_ns)
        if len(data) > MAX_INPUT or fingerprint(before) != fingerprint(after):
            raise RegressionError("regression input changed during acquisition")
        return data.decode("utf-8")
    except (ValueError, UnicodeError, OSError):
        raise RegressionError("required regular UTF-8 regression input is unavailable") from None


def baseline_sql(sql: str) -> str:
    # The former shell heredoc was moved byte-for-byte, not reimplemented.
    if (sql.count("DO $test$") != 1 or "matrix_source_event_identity_collision" not in sql
            or sql.startswith("#!") or not sql.rstrip().endswith("$test$;")):
        raise RegressionError("original Matrix behavioral assertions are missing or ambiguous")
    return sql


def input_stages(root: Path) -> tuple[list[tuple[str, str]], dict[str, str]]:
    root = plain_path(root)
    directory = plain_path(root / "services/matrix-entry-adapter/migrations")
    present = sorted(path.name for path in directory.glob("[0-9][0-9][0-9][0-9]_*.sql"))
    if present != list(MIGRATIONS):
        raise RegressionError("Matrix migration manifest does not match the source directory")
    inputs: dict[str, str] = {}
    for name in MIGRATIONS:
        path = f"services/matrix-entry-adapter/migrations/{name}"
        inputs[path] = read_input(root, path)
        if not inputs[path].lstrip().startswith("begin;") or not inputs[path].rstrip().endswith("commit;"):
            raise RegressionError(f"nontransactional migration: {name}")
    inputs[RUNNER_SOURCE] = read_input(root, RUNNER_SOURCE)
    inputs[BASELINE] = read_input(root, BASELINE)
    for path in REGRESSIONS:
        inputs[path] = read_input(root, path)
    for path in ENTRYPOINTS:
        inputs[path] = read_input(root, path)
        if inputs[path] != wrapper_source():
            raise RegressionError("Matrix entrypoints must use the same guarded runner")
    # Recheck these acquired bytes after the session. This is not Git provenance.
    stages: list[tuple[str, str]] = []
    for iteration in (1, 2):
        for name in MIGRATIONS:
            stages.append((f"migration-{iteration}-{name}", inputs[f"services/matrix-entry-adapter/migrations/{name}"]))
    stages.append(("original-transport-regression", baseline_sql(inputs[BASELINE])))
    stages.extend((Path(path).stem, inputs[path]) for path in REGRESSIONS)
    # Fixed repository SQL only: no client command may reconnect or turn off errors.
    for _, sql in stages:
        if re.search(r"(?m)^\s*\\", sql):
            raise RegressionError("psql metacommands are forbidden in regression inputs")
    hashes = {path: hashlib.sha256(text.encode()).hexdigest() for path, text in inputs.items()}
    return stages, hashes


def wrapper_source() -> str:
    return '''#!/usr/bin/env bash
set -euo pipefail
set +x
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Both public entrypoints run the full guarded chain; never a 0001-only reset.
exec python3 "$ROOT/scripts/matrix_postgres_regression.py" "$@"
'''


_TABLE_SQL = ",".join("'" + name + "'" for name in TRANSPORT_TABLES)
FOREIGN_TABLES_SQL = f"""select count(*) from pg_catalog.pg_class c
join pg_catalog.pg_namespace n on n.oid = c.relnamespace
where n.nspname not in ('pg_catalog','information_schema')
  and n.nspname not like 'pg_toast%' and n.nspname not like 'pg_temp_%'
  and c.relkind in ('r','p','v','m','f')
  and not (n.nspname = 'public' and c.relkind = 'r' and c.relname in ({_TABLE_SQL}));
"""
_GUARD_SQL = """do $matrix_objects$
begin
  if (""" + FOREIGN_TABLES_SQL.rstrip().rstrip(";") + """) <> 0 then
    raise exception 'matrix_test_unrelated_objects';
  end if;
end;
$matrix_objects$;
"""
RESET_SQL = "begin;\n" + _GUARD_SQL + f"""do $reset$
declare item text;
begin
  foreach item in array array[{_TABLE_SQL}] loop
    if to_regclass(format('public.%I', item)) is not null then
      execute format('truncate table public.%I restart identity', item);
    end if;
  end loop;
end;
$reset$;
commit;
"""


def session_sql(database: str, stages: list[tuple[str, str]], nonce: str) -> tuple[str, list[str]]:
    if not re.fullmatch(r"[a-z0-9_]{1,63}", database) or not re.fullmatch(r"[0-9a-f]{32}", nonce):
        raise RegressionError("invalid regression session identity")
    prefix = "CEX_MATRIX_" + nonce
    safety = f"""do $matrix_identity$
begin
  if current_database() <> '{database}'
      or current_setting('server_version_num')::integer not between 160000 and 169999 then
    raise exception 'matrix_test_database_or_version_mismatch';
  end if;
  if not pg_try_advisory_lock(1128618057, 1296127058) then
    raise exception 'matrix_test_runner_busy';
  end if;
end;
$matrix_identity$;
select '{prefix}:version:' || current_setting('server_version_num');
"""
    all_stages = [("server-identity", safety), ("foreign-table-guard", _GUARD_SQL),
                  ("reset-transport-test-rows", RESET_SQL), *stages]
    script = "\\set ON_ERROR_STOP on\nset search_path = pg_catalog, public;\nset standard_conforming_strings = on;\n"
    for name, sql in all_stages:
        if not re.fullmatch(r"[a-zA-Z0-9_.-]+", name):
            raise RegressionError("invalid regression stage name")
        script += f"\\echo {prefix}:start:{name}\n{sql}\n\\echo {prefix}:ok:{name}\n"
    # Session lock releases on disconnect, including process/SQL failure.
    return script, [name for name, _ in all_stages]


def bounded_client(argv: list[str], *, input: str, env: dict[str, str], timeout: float = MAX_RUN_SECONDS):
    """Use real child execution with private spool files and bounded readback.

    A trusted psql binary is required. Output growth is polled and can briefly
    exceed the threshold; this is not a hostile-process filesystem sandbox.
    """
    with tempfile.TemporaryDirectory(prefix="cex-matrix-client-") as directory:
        root = Path(directory)
        source = root / "input.sql"
        source.write_bytes(input.encode())
        source.chmod(0o600)
        with source.open("rb") as stdin, (root / "stdout").open("w+b") as stdout, (root / "stderr").open("w+b") as stderr:
            process = subprocess.Popen(argv, stdin=stdin, stdout=stdout, stderr=stderr, env=env,
                                       start_new_session=(os.name == "posix"))
            started = time.monotonic()
            failure = None
            try:
                while process.poll() is None:
                    if os.fstat(stdout.fileno()).st_size + os.fstat(stderr.fileno()).st_size > MAX_OUTPUT:
                        failure = "matrix_test_client_output_limit"
                        break
                    if time.monotonic() - started > timeout:
                        failure = "matrix_test_client_timeout"
                        break
                    time.sleep(0.02)
            finally:
                if process.poll() is None:
                    if os.name == "posix":
                        try:
                            os.killpg(process.pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                    else:
                        process.kill()
                process.wait()
            if os.fstat(stdout.fileno()).st_size + os.fstat(stderr.fileno()).st_size > MAX_OUTPUT:
                failure = "matrix_test_client_output_limit"
            if failure:
                raise RegressionError(failure)
            stdout.seek(0)
            # stderr is intentionally not exposed; SQL diagnostics can contain data.
            return subprocess.CompletedProcess(argv, process.returncode, stdout.read(MAX_OUTPUT + 1).decode("utf-8", errors="strict"), "")


def parse_observation(output: str, names: list[str], nonce: str) -> tuple[list[dict], int | None, bool]:
    prefix = "CEX_MATRIX_" + nonce + ":"
    records, version = [], None
    expected = [(kind, name) for name in names for kind in ("start", "ok")]
    position = 0
    for line in output.splitlines():
        if not line.startswith(prefix):
            continue
        value = line[len(prefix):]
        if value.startswith("version:"):
            raw = value[len("version:"):]
            if position != 1 or version is not None or not re.fullmatch(r"16[0-9]{4}", raw):
                return records, version, False
            version = int(raw)
            continue
        if position >= len(expected) or value != ":".join(expected[position]):
            return records, version, False
        kind, name = expected[position]
        if kind == "start":
            records.append({"name": name, "status": "not_completed", "confirmation": "no_post_sql_marker"})
        else:
            records[-1].update(status="passed", confirmation="post_sql_marker")
        position += 1
    return records, version, position == len(expected) and version is not None


def execute(root: Path, environment: dict[str, str], invoke: Callable | None = None) -> dict:
    pg = database_environment(environment)
    stages, hashes = input_stages(root)
    client = shutil.which("psql", path=pg["PATH"])
    if not client:
        raise RegressionError("psql is required; PostgreSQL regressions were not executed")
    result = {"schema": SCHEMA, "status": "failed", "source_sha256": hashes, "stages": [],
              "production_authorization": "not_granted", "scope": "disposable-database-regression-only",
              "execution_model": "single_psql_session", "session_exit_code": None}
    nonce = secrets.token_hex(16)
    script, names = session_sql(pg["PGDATABASE"], stages, nonce)
    try:
        response = (invoke or bounded_client)(
            [client, "-X", "--no-password", "-q", "-A", "-t", "-v", "ON_ERROR_STOP=1", "-f", "-"],
            input=script, env=pg, timeout=MAX_RUN_SECONDS)
        result["session_exit_code"] = response.returncode
        records, version, complete = parse_observation(response.stdout, names, nonce)
        result["stages"] = records
        if version is not None:
            result["server_version_num"] = version
        if response.returncode != 0 or not complete:
            raise RegressionError("Matrix SQL session failed or completion markers were incomplete")
        # Re-enumerate and hash the exact inputs after all SQL. A changed source
        # cannot obtain a successful report for the earlier byte snapshot.
        if input_stages(root)[1] != hashes:
            raise RegressionError("regression source changed during execution")
        result["status"] = "ok"
        result["database"] = pg["PGDATABASE"]
    except (RegressionError, OSError, UnicodeError, subprocess.TimeoutExpired) as error:
        result["error"] = str(error) if isinstance(error, RegressionError) else "Matrix SQL client could not complete"
    return result


def checked_output(root: Path, destination: Path) -> Path:
    root = plain_path(root)
    target = plain_path(destination if destination.is_absolute() else root / destination)
    try:
        target.relative_to(root / "run")
    except ValueError:
        raise RegressionError("evidence must be a JSON file under the checkout run directory") from None
    if target.suffix != ".json" or (target.exists() and (not target.is_file() or target.stat().st_nlink != 1)):
        raise RegressionError("invalid evidence output")
    return target


def write_evidence(root: Path, destination: Path, text: str) -> None:
    destination = checked_output(root, destination)
    destination.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=".matrix-evidence-", suffix=".tmp", dir=destination.parent)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as output:
            output.write(text)
            output.flush()
            os.fsync(output.fileno())
        checked_output(root, destination)
        os.replace(temporary, destination)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", type=Path)
    args = parser.parse_args()
    try:
        destination = checked_output(ROOT, args.evidence) if args.evidence else None
    except (RegressionError, OSError):
        print("invalid evidence output; no database operation attempted", file=sys.stderr)
        return 1
    try:
        result = execute(ROOT, dict(os.environ))
    except (RegressionError, OSError, UnicodeError) as error:
        result = {"schema": SCHEMA, "status": "failed", "error": str(error) if isinstance(error, RegressionError) else "regression input unavailable",
                  "stages": [], "production_authorization": "not_granted"}
    result["observed_at"] = datetime.now(timezone.utc).isoformat()
    text = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if destination:
        try:
            write_evidence(ROOT, destination, text)
        except (RegressionError, OSError):
            print("evidence publication failed; no qualification granted", file=sys.stderr)
            return 1
    print(text, end="")
    return 0 if result["status"] == "ok" else 1


if __name__ == "__main__":
    raise SystemExit(main())
