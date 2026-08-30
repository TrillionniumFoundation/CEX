#!/usr/bin/env python3
"""Offline regression tests for strict P0 evidence collection.

The real collector talks to the GitHub Actions API and invokes three other
evidence producers.  These tests replace those boundaries with deterministic
fixtures so the release workflow can prove the local invariants without a
network token: the hosted checker selects once, the core consumes that frozen
selection in-process exactly once, and the final payload index includes the
two payload-only attestations.

The fixture deliberately keeps the hosted-gate records stable.  It supplies a
normal-mode checker attestation for the initial selection and then invokes the
checker in frozen-context mode for the final job attestation.  The checker is
patched so any accidental direct call to its legacy latest-run selector fails
the test.
"""

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
HOSTED_CHECKER = ROOT / "scripts/check-hosted-gate-execution.py"


def load_wrapper() -> Any:
    """Load the wrapper as a module without executing its CLI."""

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


def load_checker() -> Any:
    spec = importlib.util.spec_from_file_location(
        "cex_hosted_gate_execution_test_target", HOSTED_CHECKER
    )
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load hosted gate checker")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class StrictCollectionRegression(unittest.TestCase):
    """Exercise ``collect`` while keeping every file in the real payload set."""

    repository = "TrillionniumFoundation/CEX"
    branch = "candidate/single-pass"
    sha = "1" * 40
    tree = "2" * 40
    workflow_run_id = 7001
    workflow_run_attempt = 3

    def selected_gates(self, module: Any) -> dict[str, dict[str, Any]]:
        gates: dict[str, dict[str, Any]] = {}
        for index, (name, workflow_path) in enumerate(
            module.payload_contract().HOSTED_WORKFLOW_PATHS.items(), start=1
        ):
            gates[name] = {
                "schema": "cex.hosted-gate-evidence.v1",
                "name": name,
                "workflow_path": workflow_path,
                "repository": self.repository,
                "branch": self.branch,
                "head_branch": self.branch,
                "head_sha": self.sha,
                "run_id": 100 + index,
                "run_attempt": 1,
                "event": "push",
                "status": "completed",
                "conclusion": "success",
                "created_at": f"2026-08-30T00:0{index}:00Z",
                "updated_at": f"2026-08-30T00:0{index}:01Z",
                "html_url": (
                    f"https://github.com/{self.repository}/actions/runs/{100 + index}"
                ),
            }
        return gates

    @staticmethod
    def write_json(path: Path, value: Any) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(
            json.dumps(value, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )

    def collect_fixture(self, module: Any) -> None:
        """Run one isolated collect fixture and assert its observable state."""

        with tempfile.TemporaryDirectory(prefix="cex-strict-collect-") as temporary:
            base = Path(temporary)
            evidence_dir = base / "evidence"
            context_path = base / "context.json"
            evidence_dir.mkdir()
            selected = self.selected_gates(module)
            checker = load_checker()
            initial_context = {
                "repository": self.repository,
                "branch": self.branch,
                "commit_sha": self.sha,
                "tree_sha": self.tree,
                "workflow_run_id": self.workflow_run_id,
                "workflow_run_attempt": self.workflow_run_attempt,
                "payload_name": (
                    f"cex-p0-evidence-{self.sha}-attempt-{self.workflow_run_attempt}"
                ),
                "payload_digest": None,
                "server_url": "https://github.com",
                "hosted_gates": selected,
                "files": {},
            }
            governance = {
                "schema": "cex.repository-governance-observation.v1",
                "ok": True,
                "repository": self.repository,
                "candidate_branch": self.branch,
                "commit_sha": self.sha,
                "tree_sha": self.tree,
                "candidate_commit_matches_branch": True,
            }
            core_calls: list[list[str]] = []
            subprocess_calls: list[list[str]] = []

            execution_fixture: dict[str, Any] = {
                "schema": "cex.hosted-run-execution-verification.v1",
                "status": "ok",
                "ok": True,
                "repository": self.repository,
                "branch": self.branch,
                "commit_sha": self.sha,
                "tree_sha": self.tree,
                "gates": {},
            }
            for name, record in selected.items():
                expected_jobs = checker.REQUIRED_GATES[record["workflow_path"]]
                jobs: list[dict[str, Any]] = []
                for job_index, (job_name, contract) in enumerate(
                    expected_jobs.items(), start=1
                ):
                    jobs.append(
                        {
                            "job_id": 10000 + job_index,
                            "run_id": record["run_id"],
                            "run_attempt": record["run_attempt"],
                            "name": job_name,
                            "head_sha": self.sha,
                            "runner_id": 20000 + job_index,
                            "runner_name": f"fixture-runner-{job_index}",
                            "labels": [contract["runner_label"]],
                            "required_runner_label": contract["runner_label"],
                            "status": "completed",
                            "conclusion": "success",
                            "required_steps": sorted(contract["steps"]),
                            "steps": [
                                {
                                    "name": step_name,
                                    "number": step_number,
                                    "status": "completed",
                                    "conclusion": "success",
                                    "started_at": None,
                                    "completed_at": None,
                                }
                                for step_number, step_name in enumerate(
                                    sorted(contract["steps"]), start=1
                                )
                            ],
                        }
                    )
                execution_fixture["gates"][name] = {
                    **record,
                    "status": "success",
                    "jobs": jobs,
                }

            def fake_core(
                arguments: list[str],
                *,
                frozen_runs: dict[str, dict[str, Any]],
                repository: str,
                branch: str,
                sha: str,
            ) -> None:
                core_calls.append(list(arguments))
                if len(core_calls) != 1:
                    raise AssertionError("strict collect invoked the core more than once")
                if not arguments or arguments[0] != "collect":
                    raise AssertionError(f"unexpected core command: {arguments!r}")
                if repository != self.repository or branch != self.branch or sha != self.sha:
                    raise AssertionError("frozen core candidate identity drifted")
                if set(frozen_runs) != set(selected):
                    raise AssertionError("frozen core received an incomplete gate set")
                for gate_name, record in selected.items():
                    frozen = frozen_runs[gate_name]
                    if frozen.get("id") != record["run_id"]:
                        raise AssertionError("frozen core received a different run id")
                    if frozen.get("run_attempt") != record["run_attempt"]:
                        raise AssertionError("frozen core received a different run attempt")
                self.write_json(context_path, initial_context)
                self.write_json(
                    evidence_dir / "repository-governance.json", governance
                )
                # Populate every canonical file the core normally emits.  The
                # three downstream producers overwrite their own outputs.
                for relative in sorted(module.canonical_payload_files()):
                    if relative in {
                        "payload-index.json",
                        "repository-governance.json",
                        "hosted-run-execution.json",
                        "local-evidence-binding.json",
                        "hosted-gate-execution.json",
                    }:
                        continue
                    self.write_json(
                        evidence_dir / relative,
                        {"schema": "cex.strict-fixture.v1", "ok": True},
                    )

            def fake_subprocess_run(
                command: list[str], **_: Any
            ) -> subprocess.CompletedProcess[Any]:
                command = list(command)
                subprocess_calls.append(command)
                if "--output" not in command:
                    raise AssertionError(f"downstream command lacks --output: {command!r}")
                output = Path(command[command.index("--output") + 1])
                if str(module.VERIFY_EXECUTION) in command:
                    self.write_json(output, execution_fixture)
                elif str(module.LOCAL_BINDER) in command:
                    self.write_json(
                        output,
                        {
                            "schema": "cex.p0-local-evidence-binding.v1",
                            "status": "ok",
                            "ok": True,
                        },
                    )
                elif str(module.HOSTED_CHECKER) in command:
                    if "--context" not in command:
                        # Normal mode is the sole latest-run selector.  The
                        # fixture emits the same shape as the live checker,
                        # including job proof, for the in-process core pass.
                        self.assertNotIn("--execution", command)
                        gates: dict[str, Any] = {}
                        for gate_name, record in selected.items():
                            gate_jobs = execution_fixture["gates"][gate_name]["jobs"]
                            gates[record["workflow_path"]] = {
                                "run_id": record["run_id"],
                                "run_attempt": record["run_attempt"],
                                "event": record["event"],
                                "head_branch": record["head_branch"],
                                "head_sha": record["head_sha"],
                                "status": record["status"],
                                "conclusion": record["conclusion"],
                                "created_at": record["created_at"],
                                "updated_at": record["updated_at"],
                                "selection_policy": "latest_authoritative_run_is_binding",
                                "jobs": gate_jobs,
                            }
                        self.write_json(
                            output,
                            {
                                "schema": "cex.hosted-gate-execution.v1",
                                "status": "ok",
                                "ok": True,
                                "repository": self.repository,
                                "branch": self.branch,
                                "commit_sha": self.sha,
                                "tree_sha": self.tree,
                                "selection_policy": "latest_authoritative_run_is_binding",
                                "gates": gates,
                            },
                        )
                    else:
                        self.assertIn("--execution", command)
                        self.assertEqual(
                            Path(command[command.index("--context") + 1]), context_path
                        )
                        self.assertEqual(
                            Path(command[command.index("--execution") + 1]),
                            evidence_dir / "hosted-run-execution.json",
                        )
                        frozen = checker.build_frozen_attestation(
                            evidence_dir / "hosted-run-execution.json",
                            context_path,
                            repository=self.repository,
                            branch=self.branch,
                            sha=self.sha,
                            tree=self.tree,
                        )
                        self.write_json(output, frozen)
                else:
                    raise AssertionError(f"unexpected downstream command: {command!r}")
                return subprocess.CompletedProcess(command, 0)

            args = argparse.Namespace(
                repo_root=ROOT,
                evidence_dir=evidence_dir,
                context=context_path,
                repository=self.repository,
                branch=self.branch,
                sha=self.sha,
                tree=self.tree,
                run_id=self.workflow_run_id,
                run_attempt=self.workflow_run_attempt,
                server_url="https://github.com",
            )
            with (
                mock.patch.object(module, "run_core_collect_frozen", side_effect=fake_core),
                mock.patch.object(
                    module.subprocess,
                    "run",
                    side_effect=fake_subprocess_run,
                ),
                mock.patch.dict(module.os.environ, {"GITHUB_TOKEN": "fixture"}, clear=False),
                mock.patch.object(
                    checker,
                    "select_runs",
                    side_effect=AssertionError(
                        "frozen hosted checker must not select latest runs"
                    ),
                ),
                contextlib.redirect_stdout(io.StringIO()),
            ):
                module.collect(args)

            final_context = json.loads(context_path.read_text(encoding="utf-8"))
            hosted_execution = json.loads(
                (evidence_dir / "hosted-gate-execution.json").read_text(
                    encoding="utf-8"
                )
            )
            payload_index = json.loads(
                (evidence_dir / "payload-index.json").read_text(encoding="utf-8")
            )
            self.assertEqual(len(core_calls), 1)
            self.assertEqual(
                sum(str(module.VERIFY_EXECUTION) in command for command in subprocess_calls),
                1,
            )
            self.assertEqual(
                sum(str(module.LOCAL_BINDER) in command for command in subprocess_calls),
                1,
            )
            self.assertEqual(
                sum(str(module.HOSTED_CHECKER) in command for command in subprocess_calls),
                2,
            )
            self.assertEqual(
                sum(
                    str(module.HOSTED_CHECKER) in command
                    and "--context" not in command
                    for command in subprocess_calls
                ),
                1,
            )
            self.assertEqual(
                sum(
                    str(module.HOSTED_CHECKER) in command
                    and "--context" in command
                    for command in subprocess_calls
                ),
                1,
            )
            self.assertEqual(final_context["hosted_gates"], selected)
            self.assertEqual(
                set(final_context["attestations"]),
                set(module.ATTESTATION_EVIDENCE),
            )
            self.assertEqual(
                set(final_context["payload_only_attestations"]),
                set(module.PAYLOAD_ONLY_ATTESTATIONS),
            )
            selection = final_context["hosted_gate_selection"]
            self.assertEqual(selection["schema"], module.HOSTED_GATE_SELECTION_SCHEMA)
            self.assertEqual(selection["policy"], module.LATEST_RUN_POLICY)
            self.assertEqual(
                selection["selected_run_ids"],
                {name: record["run_id"] for name, record in selected.items()},
            )
            self.assertEqual(
                set(final_context["files"]), module.canonical_payload_files()
            )
            self.assertEqual(
                set(payload_index["files"]),
                module.canonical_payload_files() - {"payload-index.json"},
            )
            self.assertEqual(
                payload_index["files"],
                {
                    key: value
                    for key, value in final_context["files"].items()
                    if key != "payload-index.json"
                },
            )
            for relative in module.PAYLOAD_ONLY_ATTESTATIONS:
                self.assertIn(relative, payload_index["files"])
                self.assertEqual(
                    final_context["files"][relative],
                    module.sha256_file(evidence_dir / relative),
                )
            hosted_records = hosted_execution.get("gates")
            self.assertIsInstance(hosted_records, dict)
            for name, record in selected.items():
                observed = hosted_records[record["workflow_path"]]
                self.assertEqual(observed["run_id"], record["run_id"])
                self.assertEqual(observed["run_attempt"], record["run_attempt"])

    def test_collect_single_core_pass_and_payload_index(self) -> None:
        """A stable hosted snapshot is collected and indexed exactly once."""

        module = load_wrapper()
        self.collect_fixture(module)

    def test_frozen_core_selector_is_injected_once(self) -> None:
        """The in-process core pass cannot perform a second API selection."""

        module = load_wrapper()
        paths = module.payload_contract().HOSTED_WORKFLOW_PATHS
        frozen = {
            name: {
                "id": index,
                "run_attempt": 1,
                "path": workflow_path,
                "event": "push",
                "head_branch": self.branch,
                "head_sha": self.sha,
                "status": "completed",
                "conclusion": "success",
                "created_at": "2026-08-30T00:00:00Z",
                "updated_at": "2026-08-30T00:00:01Z",
            }
            for index, (name, workflow_path) in enumerate(paths.items(), start=1)
        }
        original_marker = object()

        class MinimalCore:
            repository = self.repository
            branch = self.branch
            sha = self.sha

            def __init__(self) -> None:
                self.collect_gate_runs = original_marker

            def build_parser(self) -> Any:
                owner = self

                class Parser:
                    def parse_args(self, _arguments: list[str]) -> argparse.Namespace:
                        return argparse.Namespace(
                            command="collect",
                            branch=owner.branch,
                            function=owner.collect,
                        )

                return Parser()

            def collect(self, _parsed: argparse.Namespace) -> int:
                # Two calls model a regression in the core.  The wrapper must
                # reject the second call rather than silently reselecting runs.
                self.collect_gate_runs(
                    self.repository, self.sha, "unused-token", 1, 1, branch=self.branch
                )
                self.collect_gate_runs(
                    self.repository, self.sha, "unused-token", 1, 1, branch=self.branch
                )
                return 0

        fake_core = MinimalCore()
        with mock.patch.object(module, "load_core_module", return_value=fake_core):
            with self.assertRaises(SystemExit):
                module.run_core_collect_frozen(
                    ["collect", "--branch", self.branch],
                    frozen_runs=frozen,
                    repository=self.repository,
                    branch=self.branch,
                    sha=self.sha,
                )
        self.assertIs(fake_core.collect_gate_runs, original_marker)


if __name__ == "__main__":
    unittest.main()
