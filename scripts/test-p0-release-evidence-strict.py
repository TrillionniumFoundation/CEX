#!/usr/bin/env python3
"""Regression tests for single-pass strict P0 evidence collection."""

from __future__ import annotations

import argparse
import contextlib
import importlib.util
import io
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Any
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
STRICT_WRAPPER = ROOT / "scripts/p0-release-evidence-strict.py"


def load_wrapper() -> Any:
    spec = importlib.util.spec_from_file_location(
        "cex_p0_release_evidence_strict_test_target",
        STRICT_WRAPPER,
    )
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load strict release evidence wrapper")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class StrictCollectionRegression(unittest.TestCase):
    def test_collect_selects_hosted_runs_once_and_refreshes_index_in_place(self) -> None:
        module = load_wrapper()
        repository = "TrillionniumFoundation/CEX"
        branch = "candidate/single-pass"
        sha = "1" * 40
        tree = "2" * 40
        selected_gates = {
            "p0-migration-gate": {
                "workflow_path": ".github/workflows/p0-migration-gate.yml",
                "run_id": 101,
                "run_attempt": 1,
            },
            "rust-service-gate": {
                "workflow_path": ".github/workflows/rust-service-gate.yml",
                "run_id": 102,
                "run_attempt": 1,
            },
        }

        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            evidence_dir = base / "evidence"
            context_path = base / "context.json"
            evidence_dir.mkdir()
            governance_path = evidence_dir / "repository-governance.json"
            governance_path.write_text(
                json.dumps(
                    {
                        "schema": "cex.repository-governance-observation.v1",
                        "ok": True,
                        "repository": repository,
                        "commit_sha": sha,
                        "tree_sha": None,
                    }
                )
                + "\n",
                encoding="utf-8",
            )

            initial_context = {
                "repository": repository,
                "branch": branch,
                "commit_sha": sha,
                "tree_sha": tree,
                "workflow_run_id": 7001,
                "workflow_run_attempt": 3,
                "payload_name": f"cex-p0-evidence-{sha}-attempt-3",
                "hosted_gates": selected_gates,
                "files": {},
            }
            legacy_calls: list[list[str]] = []

            def fake_legacy(arguments: list[str]) -> None:
                legacy_calls.append(list(arguments))
                if len(legacy_calls) != 1:
                    raise AssertionError("legacy collector was invoked more than once")
                context_path.write_text(
                    json.dumps(initial_context, indent=2, sort_keys=True) + "\n",
                    encoding="utf-8",
                )
                for relative in (
                    "local-evidence-binding.json",
                    "hosted-gate-execution.json",
                ):
                    (evidence_dir / relative).write_text(
                        json.dumps({"ok": True, "name": relative}) + "\n",
                        encoding="utf-8",
                    )

            def fake_subprocess_run(
                command: list[str], **_: Any
            ) -> subprocess.CompletedProcess:
                self.assertIn("--context", command)
                self.assertIn("--output", command)
                output = Path(command[command.index("--output") + 1])
                output.write_text(
                    json.dumps(
                        {
                            "schema": "cex.hosted-run-execution.v1",
                            "ok": True,
                            "repository": repository,
                            "branch": branch,
                            "commit_sha": sha,
                            "tree_sha": tree,
                            "selected_run_ids": {
                                name: record["run_id"]
                                for name, record in selected_gates.items()
                            },
                        },
                        indent=2,
                        sort_keys=True,
                    )
                    + "\n",
                    encoding="utf-8",
                )
                return subprocess.CompletedProcess(command, 0)

            args = argparse.Namespace(
                repo_root=ROOT,
                evidence_dir=evidence_dir,
                context=context_path,
                repository=repository,
                branch=branch,
                sha=sha,
                tree=tree,
                run_id=7001,
                run_attempt=3,
                server_url="https://github.com",
            )
            with (
                mock.patch.object(module, "run_legacy", side_effect=fake_legacy),
                mock.patch.object(
                    module.subprocess,
                    "run",
                    side_effect=fake_subprocess_run,
                ),
                contextlib.redirect_stdout(io.StringIO()),
            ):
                self.assertEqual(module.collect(args), 0)

            self.assertEqual(len(legacy_calls), 1)
            final_context = json.loads(context_path.read_text(encoding="utf-8"))
            self.assertEqual(final_context["hosted_gates"], selected_gates)
            self.assertEqual(
                set(final_context["payload_only_attestations"]),
                {
                    "repository-governance.json",
                    "hosted-run-execution.json",
                },
            )

            payload_index_path = evidence_dir / "payload-index.json"
            payload_index = json.loads(payload_index_path.read_text(encoding="utf-8"))
            self.assertEqual(payload_index["workflow_run_id"], 7001)
            self.assertEqual(payload_index["workflow_run_attempt"], 3)
            self.assertNotIn("payload-index.json", payload_index["files"])
            self.assertIn("repository-governance.json", payload_index["files"])
            self.assertIn("hosted-run-execution.json", payload_index["files"])
            self.assertEqual(
                final_context["files"]["payload-index.json"],
                module.sha256_file(payload_index_path),
            )
            self.assertEqual(
                final_context["files"]["hosted-run-execution.json"],
                module.sha256_file(evidence_dir / "hosted-run-execution.json"),
            )

            governance = json.loads(governance_path.read_text(encoding="utf-8"))
            self.assertEqual(governance["tree_sha"], tree)


if __name__ == "__main__":
    unittest.main()
