#!/usr/bin/env python3
"""Runner orchestration tests use a fake psql; they never claim SQL execution."""
from __future__ import annotations

import importlib.util
import os
import re
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("matrix_pg_runner", ROOT / "scripts/matrix_postgres_regression.py")
assert SPEC and SPEC.loader
R = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(R)
BASE_SQL = "DO $test$ begin raise exception 'matrix_source_event_identity_collision'; end; $test$;\n"


def config(**updates):
    env = {"MATRIX_TEST_DATABASE_URL": "postgres://cex:test-secret@localhost/matrix_review_ci",
           "MATRIX_TEST_ALLOW_SCHEMA_RESET": "1", "PATH": os.defpath}
    env.update(updates)
    return env


def transcript(script):
    """Explicit fake psql transcript: tests orchestration, not SQL semantics."""
    lines = []
    for line in script.splitlines():
        if line.startswith("\\echo "):
            lines.append(line[len("\\echo "):])
        elif line.startswith("select 'CEX_MATRIX_"):
            lines.append(re.search(r"'(CEX_MATRIX_[a-f0-9]+:version:)'", line).group(1) + "160015")
    return "\n".join(lines) + "\n"


class ConfigurationTests(unittest.TestCase):
    def test_requires_explicit_consent(self):
        for value in ["", "0", "yes", "true"]:
            with self.assertRaises(R.RegressionError):
                R.database_environment(config(MATRIX_TEST_ALLOW_SCHEMA_RESET=value))

    def test_rejects_production_and_system_databases(self):
        for name in ["postgres", "template1", "cex", "production", "matrix_prod", "../matrix_review_ci"]:
            with self.assertRaises(R.RegressionError):
                R.database_environment(config(MATRIX_TEST_DATABASE_URL=f"postgres://u:p@h/{name}"))

    def test_missing_invalid_or_non_postgres_url_fails(self):
        for url in ["", "https://u:p@h/matrix_review_ci", "postgres://h/matrix_review_ci", "postgres://u:p@h:no/matrix_review_ci",
                    "postgres://u:p@h:0/matrix_review_ci", "postgres://u:p@h:65536/matrix_review_ci", "postgres://u:p@/matrix_review_ci"]:
            with self.subTest(url=url), self.assertRaises(R.RegressionError):
                R.database_environment(config(MATRIX_TEST_DATABASE_URL=url))

    def test_query_options_are_whitelisted_and_unique(self):
        for query in ["options=--anything", "service=production", "sslmode=require&sslmode=disable", "connect_timeout=0", "connect_timeout=999", "sslmode=magic"]:
            with self.assertRaises(R.RegressionError):
                R.database_environment(config(MATRIX_TEST_DATABASE_URL="postgres://u:p@h/matrix_review_ci?" + query))

    def test_percent_encoding_is_strict(self):
        for password in ["%zz", "%00", "%0a", "%ff"]:
            with self.assertRaises(R.RegressionError):
                R.database_environment(config(MATRIX_TEST_DATABASE_URL=f"postgres://u:{password}@h/matrix_review_ci"))

    def test_shell_metacharacters_are_values_not_commands(self):
        env = R.database_environment(config(MATRIX_TEST_DATABASE_URL="postgres://u:%24%28touch%20x%29@h/matrix_review_ci"))
        self.assertEqual(env["PGPASSWORD"], "$(touch x)")

    def test_no_sensitive_process_environment_inheritance(self):
        env = R.database_environment(config(PGSERVICE="production", PGSERVICEFILE="/secret", PGOPTIONS="unsafe", LD_PRELOAD="/secret.so",
                                             BASH_ENV="/evil", DATABASE_URL="postgres://live", AWS_SECRET_ACCESS_KEY="secret"))
        for key in ["PGSERVICE", "PGSERVICEFILE", "LD_PRELOAD", "BASH_ENV", "DATABASE_URL", "AWS_SECRET_ACCESS_KEY"]:
            self.assertNotIn(key, env)
        self.assertIn("statement_timeout", env["PGOPTIONS"])
        self.assertEqual(env["PGPASSFILE"], os.devnull)

    def test_errors_do_not_echo_passwords(self):
        with self.assertRaises(R.RegressionError) as failure:
            R.database_environment(config(MATRIX_TEST_DATABASE_URL="postgres://u:PRIVATE-PASSWORD@h:no/matrix_review_ci"))
        self.assertNotIn("PRIVATE-PASSWORD", str(failure.exception))


class InputTests(unittest.TestCase):
    def test_preserves_verbatim_sql_without_running_shell(self):
        self.assertEqual(R.baseline_sql(BASE_SQL), BASE_SQL)

    def test_sql_body_must_be_unique_and_complete(self):
        for sql in ["no regression", BASE_SQL + BASE_SQL, BASE_SQL[:-3]]:
            with self.assertRaises(R.RegressionError): R.baseline_sql(sql)

    def test_empty_assertion_body_and_shell_wrapper_rejected(self):
        for sql in ["select 1;", "#!/bin/bash\n" + BASE_SQL]:
            with self.assertRaises(R.RegressionError): R.baseline_sql(sql)


class ExecutionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for name in R.MIGRATIONS:
            self.write(f"services/matrix-entry-adapter/migrations/{name}", f"begin;\n-- {name}\ncommit;\n")
        self.write(R.BASELINE, BASE_SQL)
        self.write(R.RUNNER_SOURCE, Path(R.__file__).read_text())
        for path in R.ENTRYPOINTS:
            self.write(path, R.wrapper_source())
        for path in R.REGRESSIONS:
            self.write(path, "begin; select 1; rollback;\n")
        self.calls = []

    def write(self, path, text):
        dest = self.root / path
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_text(text)

    def invoke(self, argv, **kwargs):
        self.calls.append((argv, kwargs))
        return SimpleNamespace(returncode=0, stdout=transcript(kwargs["input"]), stderr="")

    def execute(self, invoke=None):
        with patch.object(R.shutil, "which", return_value="/fake/psql"):
            return R.execute(self.root, config(), invoke or self.invoke)

    def test_full_chain_precedes_all_original_assertions(self):
        result = self.execute()
        self.assertEqual(result["status"], "ok")
        self.assertEqual(len(self.calls), 1)  # same connection, not one per stage
        self.assertEqual(len(result["stages"]), 3 + 2 * len(R.MIGRATIONS) + 1 + len(R.REGRESSIONS))
        sql = self.calls[0][1]["input"]
        position = -1
        for iteration in (1, 2):
            for name in R.MIGRATIONS:
                position = sql.index(f":start:migration-{iteration}-{name}", position + 1)
        self.assertGreater(sql.index(BASE_SQL), position)
        self.assertEqual(result["production_authorization"], "not_granted")

    def test_credentials_never_enter_argv_or_report(self):
        result = self.execute()
        for argv, kw in self.calls:
            self.assertNotIn("test-secret", repr(argv))
            self.assertNotIn("postgres://", repr(argv))
            self.assertEqual(kw["env"]["PGPASSWORD"], "test-secret")
            self.assertFalse(kw.get("shell", False))
        self.assertNotIn("test-secret", repr(result))

    def test_wrong_server_version_cannot_produce_success(self):
        def old(argv, **kwargs):
            answer = self.invoke(argv, **kwargs)
            answer.stdout = answer.stdout.replace(":version:160015", ":version:150013")
            return answer
        result = self.execute(old)
        self.assertEqual(result["status"], "failed")
        self.assertEqual(len(self.calls), 1)
        self.assertIn("not between 160000 and 169999", self.calls[0][1]["input"])

    def test_unrelated_objects_guard_precedes_reset_in_same_session(self):
        def nonempty(argv, **kwargs):
            answer = self.invoke(argv, **kwargs)
            answer.stdout = answer.stdout.split(":ok:foreign-table-guard")[0].rsplit("\n", 1)[0]
            answer.returncode = 3
            return answer
        result = self.execute(nonempty)
        self.assertEqual(result["status"], "failed")
        self.assertFalse(any(r["name"] == "reset-transport-test-rows" for r in result["stages"]))
        self.assertEqual(len(self.calls), 1)

    def test_failure_stops_confirmed_stages_and_does_not_leak_diagnostics(self):
        def fails(argv, **kwargs):
            answer = self.invoke(argv, **kwargs)
            answer.returncode, answer.stderr = 3, "test-secret private data"
            marker = ":ok:migration-1-" + R.MIGRATIONS[0]
            answer.stdout = answer.stdout.split(marker)[0].rsplit("\n", 1)[0]
            return answer
        result = self.execute(fails)
        self.assertEqual(result["status"], "failed")
        self.assertEqual(len(self.calls), 1)
        self.assertEqual(result["stages"][-1]["status"], "not_completed")
        self.assertNotIn("test-secret", repr(result))

    def test_timeout_is_not_pass_or_skip(self):
        def expires(*args, **kwargs):
            raise subprocess.TimeoutExpired("psql", 180)
        result = self.execute(expires)
        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["stages"], [])

    def test_missing_migration_never_opens_database(self):
        (self.root / "services/matrix-entry-adapter/migrations" / R.MIGRATIONS[1]).unlink()
        with self.assertRaises(R.RegressionError):
            self.execute()
        self.assertFalse(self.calls)

    def test_extra_unreviewed_migration_is_not_silently_ignored(self):
        self.write("services/matrix-entry-adapter/migrations/0004_unregistered.sql", "begin; commit;")
        with self.assertRaises(R.RegressionError):
            self.execute()
        self.assertFalse(self.calls)

    def test_symlink_inputs_rejected(self):
        target = self.root / R.REGRESSIONS[0]
        target.unlink()
        target.symlink_to(self.root / R.REGRESSIONS[1])
        with self.assertRaises(R.RegressionError):
            self.execute()
        self.assertFalse(self.calls)

    def test_missing_client_is_not_database_success(self):
        with patch.object(R.shutil, "which", return_value=None), self.assertRaises(R.RegressionError):
            R.execute(self.root, config(), self.invoke)
        self.assertFalse(self.calls)


if __name__ == "__main__":
    unittest.main(verbosity=2)
