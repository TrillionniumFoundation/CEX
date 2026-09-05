#!/usr/bin/env python3
from __future__ import annotations

from pathlib import Path
import re
import subprocess
import tempfile
import unittest
from unittest import mock

import matrix_operator_postgres_regression as target


class MatrixOperatorPostgresRunnerTests(unittest.TestCase):
    def make_root(self) -> Path:
        root = Path(tempfile.mkdtemp(prefix="cex-matrix-operator-runner-"))
        migrations = root / "services/matrix-entry-adapter/operator-migrations"
        migrations.mkdir(parents=True)
        scripts = root / "scripts"
        scripts.mkdir()
        for name in target.MIGRATIONS:
            (migrations / name).write_text(
                "begin;\nselect 1;\ncommit;\n", encoding="utf-8"
            )
        for name in target.REGRESSIONS:
            (root / name).write_text(
                "begin;\nselect 1;\nrollback;\n", encoding="utf-8"
            )
        return root

    def environment(self) -> dict[str, str]:
        return {
            "PATH": "/usr/bin:/bin",
            "MATRIX_TEST_ALLOW_SCHEMA_RESET": "1",
            "MATRIX_TEST_DATABASE_URL": (
                "postgres://cex:disposable_ci_only@127.0.0.1:5432/matrix_operator_ci"
            ),
        }

    def successful_client(self, argv, *, input, env, timeout):
        del env, timeout
        prefix = re.search(r"CEX_MATRIX_[0-9a-f]{32}", input)
        self.assertIsNotNone(prefix)
        marker_prefix = prefix.group(0)
        output: list[str] = []
        for line in input.splitlines():
            if not line.startswith("\\echo "):
                continue
            marker = line.removeprefix("\\echo ")
            output.append(marker)
            if marker == f"{marker_prefix}:start:server-identity":
                output.append(f"{marker_prefix}:version:160014")
        return subprocess.CompletedProcess(
            argv, 0, "\n".join(output) + "\n", ""
        )

    def test_exact_manifest_runs_twice_then_regressions(self):
        root = self.make_root()
        stages, hashes = target.acquired_inputs(root)
        self.assertEqual(len(hashes), len(target.MIGRATIONS) + len(target.REGRESSIONS))
        self.assertEqual(
            [name for name, _ in stages[: len(target.MIGRATIONS)]],
            [f"operator-migration-1-{name}" for name in target.MIGRATIONS],
        )
        self.assertEqual(
            [
                name
                for name, _ in stages[
                    len(target.MIGRATIONS) : 2 * len(target.MIGRATIONS)
                ]
            ],
            [f"operator-migration-2-{name}" for name in target.MIGRATIONS],
        )

    def test_extra_migration_is_rejected(self):
        root = self.make_root()
        extra = (
            root
            / "services/matrix-entry-adapter/operator-migrations/0004_unreviewed.sql"
        )
        extra.write_text("begin;\ncommit;\n", encoding="utf-8")
        with self.assertRaisesRegex(target.OperatorRegressionError, "manifest"):
            target.acquired_inputs(root)

    def test_nontransactional_migration_is_rejected(self):
        root = self.make_root()
        path = (
            root
            / "services/matrix-entry-adapter/operator-migrations"
            / target.MIGRATIONS[0]
        )
        path.write_text("select 1;\n", encoding="utf-8")
        with self.assertRaisesRegex(target.OperatorRegressionError, "nontransactional"):
            target.acquired_inputs(root)

    def test_psql_control_backslash_is_rejected(self):
        root = self.make_root()
        path = root / target.REGRESSIONS[0]
        path.write_text("begin;\n\\quit\nrollback;\n", encoding="utf-8")
        with self.assertRaisesRegex(target.base.RegressionError, "backslashes"):
            target.acquired_inputs(root)

    def test_marker_prefix_matches_shared_parser_contract(self):
        root = self.make_root()
        stages, _ = target.acquired_inputs(root)
        script, _ = target.session_sql("matrix_operator_ci", stages, "0" * 32)
        self.assertIn("CEX_MATRIX_" + "0" * 32 + ":start:server-identity", script)
        self.assertNotIn("CEX_MATRIX_OPERATOR_", script)

    def test_execute_accepts_only_complete_ordered_markers(self):
        root = self.make_root()
        with mock.patch.object(target.shutil, "which", return_value="/usr/bin/psql"):
            result = target.execute(
                root, self.environment(), invoke=self.successful_client
            )
        self.assertEqual(result["status"], "ok")
        self.assertEqual(result["server_version_num"], 160014)
        self.assertTrue(result["stages"])
        self.assertTrue(
            all(stage["status"] == "passed" for stage in result["stages"])
        )
        self.assertEqual(result["production_authorization"], "not_granted")

    def test_incomplete_transcript_fails_closed(self):
        root = self.make_root()

        def incomplete(argv, *, input, env, timeout):
            completed = self.successful_client(
                argv, input=input, env=env, timeout=timeout
            )
            lines = completed.stdout.splitlines()
            return subprocess.CompletedProcess(
                argv, 0, "\n".join(lines[:-1]) + "\n", ""
            )

        with mock.patch.object(target.shutil, "which", return_value="/usr/bin/psql"):
            result = target.execute(root, self.environment(), invoke=incomplete)
        self.assertEqual(result["status"], "failed")
        self.assertIn("completion markers", result["error"])


if __name__ == "__main__":
    unittest.main()
