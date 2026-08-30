#!/usr/bin/env python3
"""Regression tests for single-snapshot strict P0 evidence collection."""

from __future__ import annotations

import argparse
import contextlib
import copy
import importlib.util
import io
import json
import subprocess
import sys
import tempfile
import types
import unittest
from pathlib import Path
from typing import Any
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
STRICT_WRAPPER = ROOT / "scripts/p0-release-evidence-strict.py"
LEGACY_WRAPPER = ROOT / "scripts/p0-release-evidence.py"
HOSTED_CHECKER = ROOT / "scripts/check-hosted-gate-execution.py"
GOVERNANCE_OBSERVER = ROOT / "scripts/observe-repository-governance.py"


def load_script(path: Path, module_name: str) -> Any:
    sys.modules.pop(module_name, None)
    spec = importlib.util.spec_from_file_location(module_name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load test target: {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def load_wrapper() -> Any:
    return load_script(
        STRICT_WRAPPER,
        "cex_p0_release_evidence_strict_test_target",
    )


def hosted_attestation(
    module: Any,
    *,
    repository: str,
    branch: str,
    sha: str,
    tree: str,
) -> dict[str, Any]:
    gates: dict[str, Any] = {}
    for index, workflow_path in enumerate(module.AUTHORITATIVE_GATES.values(), start=1):
        gates[workflow_path] = {
            "run_id": 1000 + index,
            "run_attempt": 2,
            "event": "push",
            "head_branch": branch,
            "head_sha": sha,
            "status": "completed",
            "conclusion": "success",
            "created_at": f"2026-08-30T00:00:{index:02d}Z",
            "updated_at": f"2026-08-30T00:01:{index:02d}Z",
            "selection_policy": module.LATEST_RUN_POLICY,
            "jobs": [{"job_id": 2000 + index, "name": f"gate-{index}"}],
        }
    return {
        "schema": "cex.hosted-gate-execution.v1",
        "status": "ok",
        "ok": True,
        "repository": repository,
        "branch": branch,
        "commit_sha": sha,
        "tree_sha": tree,
        "selection_policy": module.LATEST_RUN_POLICY,
        "generated_at": "2026-08-30T00:02:00Z",
        "gates": gates,
    }


class StrictCollectionRegression(unittest.TestCase):
    def test_collect_invokes_legacy_once_and_refreshes_index_in_place(self) -> None:
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


class FrozenHostedRunSelectionRegression(unittest.TestCase):
    def setUp(self) -> None:
        self.module = load_script(
            LEGACY_WRAPPER,
            "cex_p0_release_evidence_legacy_test_target",
        )
        self.repository = "TrillionniumFoundation/CEX"
        self.branch = "candidate/frozen-runs"
        self.sha = "a" * 40
        self.tree = "b" * 40

    def write_attestation(
        self,
        directory: Path,
        payload: dict[str, Any] | None = None,
    ) -> Path:
        path = directory / "hosted-gate-execution.json"
        path.write_text(
            json.dumps(
                payload
                or hosted_attestation(
                    self.module,
                    repository=self.repository,
                    branch=self.branch,
                    sha=self.sha,
                    tree=self.tree,
                ),
                indent=2,
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
        )
        return path

    def frozen_runs(self, directory: Path) -> dict[str, dict[str, Any]]:
        return self.module.frozen_runs_from_attestation(
            self.write_attestation(directory),
            repository=self.repository,
            branch=self.branch,
            sha=self.sha,
            tree=self.tree,
        )

    def fake_core(
        self,
        execute: Any,
    ) -> Any:
        class Parser:
            def parse_args(_self, _arguments: list[str]) -> argparse.Namespace:
                return argparse.Namespace(
                    command="collect",
                    branch=self.branch,
                    function=execute,
                )

        def forbidden_selector(*_args: Any, **_kwargs: Any) -> Any:
            raise AssertionError("the original core selector must never execute")

        return types.SimpleNamespace(
            build_parser=lambda: Parser(),
            collect_gate_runs=forbidden_selector,
        )

    def test_core_consumes_exact_frozen_snapshot_without_requery(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            frozen = self.frozen_runs(Path(temporary))
            consumed: list[dict[str, dict[str, Any]]] = []
            core: Any

            def execute(_args: argparse.Namespace) -> int:
                selected = core.collect_gate_runs(
                    self.repository,
                    self.sha,
                    "token",
                    5,
                    1,
                    branch=self.branch,
                )
                consumed.append(selected)
                return 0

            core = self.fake_core(execute)
            result = self.module.run_core_collect(
                ["collect"],
                frozen_runs=frozen,
                repository=self.repository,
                branch=self.branch,
                sha=self.sha,
                core_module=core,
            )
            self.assertEqual(result, 0)
            self.assertEqual(consumed, [frozen])

    def test_core_rejects_a_second_selector_call(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            frozen = self.frozen_runs(Path(temporary))
            core: Any

            def execute(_args: argparse.Namespace) -> int:
                core.collect_gate_runs(
                    self.repository,
                    self.sha,
                    "token",
                    5,
                    1,
                    branch=self.branch,
                )
                core.collect_gate_runs(
                    self.repository,
                    self.sha,
                    "token",
                    5,
                    1,
                    branch=self.branch,
                )
                return 0

            core = self.fake_core(execute)
            with self.assertRaisesRegex(
                SystemExit,
                "more than once",
            ):
                self.module.run_core_collect(
                    ["collect"],
                    frozen_runs=frozen,
                    repository=self.repository,
                    branch=self.branch,
                    sha=self.sha,
                    core_module=core,
                )

    def test_failed_or_incomplete_latest_snapshot_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            base = hosted_attestation(
                self.module,
                repository=self.repository,
                branch=self.branch,
                sha=self.sha,
                tree=self.tree,
            )
            first_path = next(iter(self.module.AUTHORITATIVE_GATES.values()))

            for label, mutation in (
                (
                    "failed",
                    {"conclusion": "failure"},
                ),
                (
                    "empty jobs",
                    {"jobs": []},
                ),
                (
                    "wrong policy",
                    {"selection_policy": "newest_success_can_mask_failure"},
                ),
            ):
                candidate = copy.deepcopy(base)
                candidate["gates"][first_path].update(mutation)
                path = self.write_attestation(directory, candidate)
                with self.subTest(label=label), self.assertRaises(SystemExit):
                    self.module.frozen_runs_from_attestation(
                        path,
                        repository=self.repository,
                        branch=self.branch,
                        sha=self.sha,
                        tree=self.tree,
                    )

            missing = copy.deepcopy(base)
            del missing["gates"][first_path]
            path = self.write_attestation(directory, missing)
            with self.assertRaisesRegex(SystemExit, "workflow set mismatch"):
                self.module.frozen_runs_from_attestation(
                    path,
                    repository=self.repository,
                    branch=self.branch,
                    sha=self.sha,
                    tree=self.tree,
                )


class GovernanceRequiredCheckRegression(unittest.TestCase):
    def test_observer_requires_every_constituent_and_aggregate_job(self) -> None:
        hosted = load_script(
            HOSTED_CHECKER,
            "cex_hosted_gate_checker_contract_test_target",
        )
        observer = load_script(
            GOVERNANCE_OBSERVER,
            "cex_governance_observer_contract_test_target",
        )

        expected = {"repository-candidate-qualification"}
        for jobs in hosted.REQUIRED_GATES.values():
            expected.update(jobs)

        actual = set(observer.DESIRED_CHECKS)
        self.assertEqual(actual, expected)
        self.assertTrue(
            observer.required_checks_are_enforced(True, sorted(actual))
        )
        self.assertFalse(
            observer.required_checks_are_enforced(False, sorted(actual))
        )
        for missing in sorted(actual):
            with self.subTest(missing=missing):
                self.assertFalse(
                    observer.required_checks_are_enforced(
                        True,
                        sorted(actual - {missing}),
                    )
                )


if __name__ == "__main__":
    unittest.main()
