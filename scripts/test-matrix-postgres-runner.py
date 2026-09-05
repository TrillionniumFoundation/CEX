#!/usr/bin/env python3
"""Runner orchestration tests use a fake psql; they never claim SQL execution."""
from __future__ import annotations

import importlib.util
import os
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
    def test_extracts_verbatim_sql_without_running_shell(self):
        script = "#!/bin/bash\nexit 99\n" + R.BASELINE_MARKER + BASE_SQL + "SQL\necho forbidden\n"
        self.assertEqual(R.baseline_sql(script), BASE_SQL)

    def test_delimiter_must_be_unique_and_closed(self):
        for script in ["no regression", R.BASELINE_MARKER + BASE_SQL, (R.BASELINE_MARKER + BASE_SQL + "SQL\n") * 2]:
            with self.assertRaises(R.RegressionError):
                R.baseline_sql(script)

    def test_empty_assertion_body_rejected(self):
        with self.assertRaises(R.RegressionError):
            R.baseline_sql(R.BASELINE_MARKER + "select 1;\nSQL\n")


class ExecutionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for name in R.MIGRATIONS:
            self.write(f"services/matrix-entry-adapter/migrations/{name}", f"begin;\n-- {name}\ncommit;\n")
        self.write(R.BASELINE, R.BASELINE_MARKER + BASE_SQL + "SQL\n")
        for path in R.REGRESSIONS:
            self.write(path, "begin; select 1; rollback;\n")
        self.calls = []

    def write(self, path, text):
        dest = self.root / path
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_text(text)

    def invoke(self, argv, **kwargs):
        self.calls.append((argv, kwargs))
        sql = kwargs["input"]
        stdout = "160015\nmatrix_review_ci\n" if sql.startswith("show server") else ("0\n" if sql == R.FOREIGN_TABLES_SQL else "")
        return SimpleNamespace(returncode=0, stdout=stdout, stderr="")

    def execute(self, invoke=None):
        with patch.object(R.shutil, "which", return_value="/fake/psql"):
            return R.execute(self.root, config(), invoke or self.invoke)

    def test_full_chain_precedes_all_original_assertions(self):
        result = self.execute()
        self.assertEqual(result["status"], "ok")
        self.assertEqual(len(result["stages"]), 3 + 2 * len(R.MIGRATIONS) + 1 + len(R.REGRESSIONS))
        self.assertEqual(self.calls[3 + 2 * len(R.MIGRATIONS)][1]["input"], BASE_SQL)
        for offset in (3, 3 + len(R.MIGRATIONS)):
            for index, name in enumerate(R.MIGRATIONS):
                self.assertIn(name, self.calls[offset + index][1]["input"])
        self.assertEqual(result["production_authorization"], "not_granted")

    def test_credentials_never_enter_argv_or_report(self):
        result = self.execute()
        for argv, kw in self.calls:
            self.assertNotIn("test-secret", repr(argv))
            self.assertNotIn("postgres://", repr(argv))
            self.assertEqual(kw["env"]["PGPASSWORD"], "test-secret")
            self.assertFalse(kw.get("shell", False))
        self.assertNotIn("test-secret", repr(result))

    def test_wrong_server_version_stops_before_any_migration(self):
        def old(argv, **kwargs):
            self.calls.append((argv, kwargs))
            return SimpleNamespace(returncode=0, stdout="150013\nmatrix_review_ci\n", stderr="")
        result = self.execute(old)
        self.assertEqual(result["status"], "failed")
        self.assertEqual(len(self.calls), 1)

    def test_unrelated_tables_prevent_reset(self):
        def nonempty(argv, **kwargs):
            response = self.invoke(argv, **kwargs)
            if kwargs["input"] == R.FOREIGN_TABLES_SQL:
                response.stdout = "1\n"
            return response
        result = self.execute(nonempty)
        self.assertEqual(result["status"], "failed")
        self.assertEqual(len(self.calls), 2)

    def test_failure_stops_later_stages_and_does_not_leak_diagnostics(self):
        def fails(argv, **kwargs):
            response = self.invoke(argv, **kwargs)
            if len(self.calls) == 4:
                response.returncode, response.stderr = 1, "test-secret private data"
            return response
        result = self.execute(fails)
        self.assertEqual(result["status"], "failed")
        self.assertEqual(len(self.calls), 4)
        self.assertEqual(result["stages"][-1]["status"], "failed")
        self.assertNotIn("test-secret", repr(result))

    def test_timeout_is_not_pass_or_skip(self):
        def expires(*args, **kwargs):
            raise subprocess.TimeoutExpired("psql", 180)
        result = self.execute(expires)
        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["stages"][0]["status"], "not_completed")

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
