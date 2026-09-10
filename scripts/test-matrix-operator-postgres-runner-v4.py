#!/usr/bin/env python3
"""Test Matrix operator runner v4 orchestration with a fake psql transcript."""
from __future__ import annotations

import importlib.util
import os
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "matrix_operator_pg_runner_v4",
    ROOT / "scripts/matrix_operator_postgres_regression.py",
)
assert SPEC and SPEC.loader
R = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(R)


def configuration() -> dict[str, str]:
    return {
        "MATRIX_TEST_DATABASE_URL": (
            "postgres://cex:test-secret@localhost/matrix_review_ci"
        ),
        "MATRIX_TEST_ALLOW_SCHEMA_RESET": "1",
        "PATH": os.defpath,
    }


def transcript(script: str) -> str:
    lines: list[str] = []
    for line in script.splitlines():
        if line.startswith("\\echo "):
            lines.append(line[len("\\echo "):])
        elif line.startswith("select 'CEX_MATRIX_"):
            prefix = line.split("'", 2)[1]
            lines.append(prefix + "160015")
    return "\n".join(lines) + "\n"


class OperatorRunnerV4Tests(unittest.TestCase):
    def test_manifest_and_stage_order_are_exact(self) -> None:
        stages, hashes = R.acquired_inputs(ROOT)
        names = [name for name, _ in stages]
        expected: list[str] = []
        for migrations, regressions in (
            (R.HISTORICAL_MIGRATIONS, R.HISTORICAL_REGRESSIONS),
            (R.CAUSAL_MIGRATIONS, R.CAUSAL_REGRESSIONS),
            (R.SECURITY_MIGRATIONS, R.SECURITY_REGRESSIONS),
        ):
            for pass_number in (1, 2):
                expected.extend(
                    f"operator-migration-{pass_number}-{name}"
                    for name in migrations
                )
            expected.extend(Path(path).stem for path in regressions)

        self.assertEqual(names, expected)
        self.assertEqual(
            R.MIGRATIONS,
            R.HISTORICAL_MIGRATIONS
            + R.CAUSAL_MIGRATIONS
            + R.SECURITY_MIGRATIONS,
        )
        self.assertEqual(
            R.REGRESSIONS,
            R.HISTORICAL_REGRESSIONS
            + R.CAUSAL_REGRESSIONS
            + R.SECURITY_REGRESSIONS,
        )
        self.assertEqual(set(hashes), {
            *(
                "services/matrix-entry-adapter/operator-migrations/" + name
                for name in R.MIGRATIONS
            ),
            *R.REGRESSIONS,
        })
        self.assertEqual(R.SCHEMA, "cex.matrix-operator-postgres-regression.v4")

    def test_fake_client_observes_every_stage_once(self) -> None:
        calls: list[tuple[list[str], dict]] = []

        def invoke(argv: list[str], **kwargs):
            calls.append((argv, kwargs))
            return SimpleNamespace(
                returncode=0,
                stdout=transcript(kwargs["input"]),
                stderr="",
            )

        with patch.object(R.shutil, "which", return_value="/fake/psql"):
            result = R.execute(ROOT, configuration(), invoke)

        self.assertEqual(result["status"], "ok")
        self.assertEqual(result["production_authorization"], "not_granted")
        self.assertEqual(len(calls), 1)
        self.assertFalse(calls[0][1].get("shell", False))
        self.assertNotIn("test-secret", repr(calls[0][0]))
        self.assertNotIn("test-secret", repr(result))
        self.assertEqual(
            [record["name"] for record in result["stages"]],
            ["server-identity", "base-schema"]
            + [name for name, _ in R.acquired_inputs(ROOT)[0]],
        )
        self.assertTrue(
            all(record["status"] == "passed" for record in result["stages"])
        )

    def test_each_api_cutover_follows_its_historical_regressions(self) -> None:
        stages, _ = R.acquired_inputs(ROOT)
        names = [name for name, _ in stages]
        first_v2 = names.index(
            "operator-migration-1-0005_adapter_result_causal_binding.sql"
        )
        final_v2 = names.index(
            "operator-migration-2-0005_adapter_result_causal_binding.sql"
        )
        first_v3 = names.index(
            "operator-migration-1-0006_adapter_result_embedded_delivery_binding.sql"
        )
        final_v3 = names.index(
            "operator-migration-2-0006_adapter_result_embedded_delivery_binding.sql"
        )
        for path in R.HISTORICAL_REGRESSIONS:
            self.assertLess(names.index(Path(path).stem), first_v2)
        for path in R.CAUSAL_REGRESSIONS:
            self.assertGreater(names.index(Path(path).stem), final_v2)
            self.assertLess(names.index(Path(path).stem), first_v3)
        for path in R.SECURITY_REGRESSIONS:
            self.assertGreater(names.index(Path(path).stem), final_v3)


if __name__ == "__main__":
    unittest.main(verbosity=2)
