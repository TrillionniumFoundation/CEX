#!/usr/bin/env python3
"""Regression tests for the deterministic repository contract inventory."""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from repository_contract_facts import (  # noqa: E402
    INVENTORY_SCHEMA,
    build_inventory,
    extract_configuration,
    extract_metrics,
    extract_routes,
)


class ExtractionTests(unittest.TestCase):
    def test_routes_capture_chained_methods_and_ignore_comments(self) -> None:
        text = '''
// .route("/fake", get(fake))
Router::new().route(
    "/v1/items/:id",
    get(api::read).post(api::create).delete(api::remove),
)
'''
        routes, unresolved = extract_routes(text, "src/lib.rs")
        self.assertEqual(unresolved, 0)
        self.assertEqual(
            [(item["method"], item["handler"]) for item in routes],
            [("DELETE", "api::remove"), ("GET", "api::read"), ("POST", "api::create")],
        )

    def test_non_literal_route_is_reported_unresolved(self) -> None:
        routes, unresolved = extract_routes(
            "Router::new().route(PATH, get(handler))", "src/lib.rs"
        )
        self.assertEqual(routes, [])
        self.assertEqual(unresolved, 1)

    def test_configuration_ignores_comments(self) -> None:
        text = (
            'let a = env::var("REAL_KEY"); // env::var("FAKE_KEY")\n'
            'let b = required_env("SECOND_KEY")?;'
        )
        keys = [item["key"] for item in extract_configuration(text, "src/lib.rs")]
        self.assertEqual(keys, ["REAL_KEY", "SECOND_KEY"])

    def test_metrics_are_source_located(self) -> None:
        values = extract_metrics(
            'let x = "cex_queue_age_seconds"; // cex_fake\n', "src/lib.rs"
        )
        self.assertEqual(
            values,
            [{"name": "cex_queue_age_seconds", "source": "src/lib.rs", "line": 1}],
        )


class InventoryTests(unittest.TestCase):
    def make_repo(self, *, include_lib_in_catalog: bool = True) -> Path:
        root = Path(tempfile.mkdtemp())
        (root / "svc/src/bin").mkdir(parents=True)
        (root / "svc/tests").mkdir(parents=True)
        (root / "docs").mkdir()
        (root / "migrations").mkdir()
        (root / "Cargo.toml").write_text(
            '[workspace]\nmembers=["svc"]\n', encoding="utf-8"
        )
        (root / "svc/Cargo.toml").write_text(
            '[package]\nname="svc"\nversion="0.1.0"\nedition="2021"\n\n'
            '[[test]]\nname="contract"\npath="tests/contract.rs"\n',
            encoding="utf-8",
        )
        (root / "svc/src/lib.rs").write_text(
            'pub fn router() { let _ = Router::new().route("/health", get(health)); '
            'let _ = std::env::var("SVC_TOKEN"); let _ = "cex_svc_up"; }\n',
            encoding="utf-8",
        )
        (root / "svc/src/bin/worker.rs").write_text(
            "fn main() {}\n", encoding="utf-8"
        )
        (root / "svc/tests/contract.rs").write_text(
            "#[test] fn works() {}\n", encoding="utf-8"
        )
        (root / "migrations/0001.sql").write_text(
            "create table public.items(id bigint);\n"
            "create or replace function public.claim_item() returns void "
            "language sql as $$ select 1 $$;\n",
            encoding="utf-8",
        )
        entrypoints = ["svc/src/bin/worker.rs", "svc/tests/contract.rs"]
        if include_lib_in_catalog:
            entrypoints.append("svc/src/lib.rs")
        (root / "docs/module-catalog-v1.json").write_text(
            json.dumps(
                {
                    "modules": [
                        {
                            "workspace_member": "svc",
                            "package": "svc",
                            "source_entrypoints": entrypoints,
                        }
                    ]
                }
            ),
            encoding="utf-8",
        )
        return root

    def test_inventory_contains_targets_routes_env_metrics_and_sql(self) -> None:
        inventory = build_inventory(self.make_repo())
        self.assertEqual(inventory["schema"], INVENTORY_SCHEMA)
        self.assertEqual(inventory["status"], "ok")
        member = inventory["members"][0]
        self.assertEqual(
            {item["path"] for item in member["targets"]},
            {"svc/src/lib.rs", "svc/src/bin/worker.rs", "svc/tests/contract.rs"},
        )
        self.assertEqual(member["routes"][0]["path"], "/health")
        self.assertEqual(member["configuration"][0]["key"], "SVC_TOKEN")
        self.assertEqual(member["metrics"][0]["name"], "cex_svc_up")
        self.assertEqual(
            {item["kind"] for item in inventory["sql_objects"]},
            {"function", "table"},
        )
        self.assertFalse(inventory["checker_may_grant_production_authorization"])
        self.assertEqual(inventory["production_authorization"], "not_granted")

    def test_missing_required_target_fails(self) -> None:
        inventory = build_inventory(self.make_repo(include_lib_in_catalog=False))
        self.assertEqual(inventory["status"], "failed")
        self.assertTrue(
            any("omits Cargo targets" in value for value in inventory["problems"])
        )

    def test_output_is_deterministic(self) -> None:
        root = self.make_repo()
        first = json.dumps(build_inventory(root), sort_keys=True)
        second = json.dumps(build_inventory(root), sort_keys=True)
        self.assertEqual(first, second)

    def test_workspace_escape_fails_closed(self) -> None:
        root = self.make_repo()
        (root / "Cargo.toml").write_text(
            '[workspace]\nmembers=["../escape"]\n', encoding="utf-8"
        )
        inventory = build_inventory(root)
        self.assertEqual(inventory["status"], "failed")
        self.assertTrue(
            any("escapes repository" in value for value in inventory["problems"])
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
