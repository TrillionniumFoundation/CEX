#!/usr/bin/env python3
"""Mutation tests for the Consumer World/League projection boundary guard."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "consumer_projection_guard",
    Path(__file__).with_name("check-consumer-projection-boundary.py"),
)
assert SPEC and SPEC.loader
C = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(C)


class ConsumerProjectionBoundaryTests(unittest.TestCase):
    def check_source(self, source: str) -> list[str]:
        path: Path | None = None
        try:
            with tempfile.NamedTemporaryFile(
                mode="w",
                encoding="utf-8",
                suffix=".rs",
                prefix="world_projection_guard_fixture_",
                dir=C.SOURCE_ROOT,
                delete=False,
            ) as stream:
                stream.write(source)
                path = Path(stream.name)
            return C.check_file(path)
        finally:
            if path is not None:
                path.unlink(missing_ok=True)

    def assert_rejected(self, source: str, marker: str) -> None:
        problems = self.check_source(source)
        self.assertTrue(
            any(marker in problem for problem in problems),
            f"expected {marker!r} in {problems!r}",
        )

    def test_current_sources_and_contracts(self) -> None:
        files = C.projection_files()
        self.assertGreater(len(files), 0)
        problems = [problem for path in files for problem in C.check_file(path)]
        problems.extend(C.check_contract_files())
        self.assertEqual(problems, [])

    def test_authoritative_table_mutation_is_rejected(self) -> None:
        self.assert_rejected(
            'fn fixture() { let _ = sqlx::query("insert into ledger_entries(id) values (1)"); }',
            "direct mutation of authoritative table",
        )

    def test_unapproved_projection_namespace_is_rejected(self) -> None:
        self.assert_rejected(
            'fn fixture() { let _ = sqlx::query("update miscellaneous set value = 1"); }',
            "lacks an approved projection namespace",
        )

    def test_runtime_ddl_is_rejected(self) -> None:
        self.assert_rejected(
            'fn fixture() { let _ = sqlx::query("create table world_shadow(id bigint)"); }',
            "runtime DDL is forbidden",
        )

    def test_internal_authority_endpoint_is_rejected(self) -> None:
        self.assert_rejected(
            'fn fixture() { let endpoint = "https://internal/v1/ledger/settle"; use_it(endpoint); }',
            "internal authoritative endpoint",
        )

    def test_authoritative_outcome_literal_is_rejected(self) -> None:
        self.assert_rejected(
            'fn fixture() { let value = serde_json::json!({"authoritative": true}); use_it(value); }',
            "declares an authoritative outcome",
        )

    def test_approved_projection_mutation_is_allowed(self) -> None:
        problems = self.check_source(
            'fn fixture() { let _ = sqlx::query("insert into consumer_world_projection(id) values (1)"); }'
        )
        self.assertEqual(problems, [])

    def test_sql_comments_and_values_do_not_manufacture_mutation(self) -> None:
        problems = self.check_source(
            'fn fixture() { let _ = sqlx::query("select 1 -- insert into ledger_entries(id) values (1)\\n"); }'
        )
        self.assertEqual(problems, [])

    def test_dynamic_authoritative_route_is_fail_closed(self) -> None:
        self.assert_rejected(
            """
fn fixture() {
    let path = route_path();
    let _ = axum::Router::new().route(path, axum::routing::get(handler));
}
""",
            "dynamic projection route path requires explicit reviewed resolution",
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
