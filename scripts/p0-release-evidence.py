#!/usr/bin/env python3
"""Add strict local/hosted evidence attestation around the v12 evidence core.

The hosted-gate checker is the sole GitHub Actions run selector. Its validated
latest-authoritative snapshot is injected into the immutable core in process,
so the core cannot independently query for a different or older successful run.
"""

from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CORE = ROOT / "scripts/p0-release-evidence-core.py"
LOCAL_BINDER = ROOT / "scripts/bind-p0-local-evidence.py"
HOSTED_CHECKER = ROOT / "scripts/check-hosted-gate-execution.py"
MANIFEST_VALIDATOR = ROOT / "scripts/check-release-baseline-manifest.py"
ATTESTATION_EVIDENCE = {
    "local-evidence-binding": "local-evidence-binding.json",
    "hosted-gate-execution": "hosted-gate-execution.json",
}
AUTHORITATIVE_GATES = {
    "p0-migration-gate": ".github/workflows/p0-migration-gate.yml",
    "rust-service-gate": ".github/workflows/rust-service-gate.yml",
    "p0-gateway-exact-reserve-gate": ".github/workflows/p0-gateway-exact-reserve-gate.yml",
    "p0-execution-settlement-gate": ".github/workflows/p0-execution-settlement-gate.yml",
    "p0-provider-reconciliation-gate": ".github/workflows/p0-provider-reconciliation-gate.yml",
}
AUTHORITATIVE_EVENTS = {"push", "workflow_dispatch"}
LATEST_RUN_POLICY = "latest_authoritative_run_is_binding"


def option(arguments: list[str], name: str) -> str:
    try:
        index = arguments.index(name)
        value = arguments[index + 1]
    except (ValueError, IndexError) as error:
        raise SystemExit(f"missing required wrapper option: {name}") from error
    if not value:
        raise SystemExit(f"empty required wrapper option: {name}")
    return value


def rooted(path: str | Path) -> Path:
    value = Path(path)
    return value if value.is_absolute() else ROOT / value


def run(command: list[str]) -> int:
    completed = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if completed.stdout:
        print(
            completed.stdout,
            end="" if completed.stdout.endswith("\n") else "\n",
            flush=True,
        )
    return completed.returncode


def read_json(path: Path, label: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"cannot read {label}: {error}") from error
    if not isinstance(value, dict):
        raise SystemExit(f"{label} must be a JSON object")
    return value


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return "sha256:" + digest.hexdigest()


def require_equal(actual: Any, expected: Any, label: str) -> None:
    if actual != expected:
        raise SystemExit(f"{label} mismatch: expected {expected!r}, got {actual!r}")


def require_positive_int(value: Any, label: str) -> int:
    if type(value) is not int or value < 1:
        raise SystemExit(f"{label} must be a strict positive integer")
    return value


def load_core_module() -> Any:
    """Load the immutable evidence core without invoking its CLI entry point."""

    module_name = "cex_p0_release_evidence_core_frozen"
    sys.modules.pop(module_name, None)
    spec = importlib.util.spec_from_file_location(module_name, CORE)
    if spec is None or spec.loader is None:
        raise SystemExit("cannot load immutable P0 release-evidence core")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    return module


def frozen_runs_from_attestation(
    path: Path,
    *,
    repository: str,
    branch: str,
    sha: str,
    tree: str,
) -> dict[str, dict[str, Any]]:
    """Return the exact run set already selected and validated by the checker."""

    payload = read_json(path, "hosted gate execution attestation")
    require_equal(
        payload.get("schema"),
        "cex.hosted-gate-execution.v1",
        "hosted gate execution schema",
    )
    require_equal(payload.get("status"), "ok", "hosted gate execution status")
    require_equal(payload.get("ok"), True, "hosted gate execution ok")
    require_equal(payload.get("repository"), repository, "hosted gate repository")
    require_equal(payload.get("branch"), branch, "hosted gate branch")
    require_equal(payload.get("commit_sha"), sha, "hosted gate commit")
    require_equal(payload.get("tree_sha"), tree, "hosted gate tree")
    require_equal(
        payload.get("selection_policy"),
        LATEST_RUN_POLICY,
        "hosted gate selection policy",
    )

    gates = payload.get("gates")
    if not isinstance(gates, dict):
        raise SystemExit("hosted gate execution attestation lacks a gate map")
    expected_paths = set(AUTHORITATIVE_GATES.values())
    if set(gates) != expected_paths:
        raise SystemExit(
            "hosted gate execution workflow set mismatch: "
            f"expected={sorted(expected_paths)!r} actual={sorted(str(key) for key in gates)!r}"
        )

    selected: dict[str, dict[str, Any]] = {}
    for name, workflow_path in AUTHORITATIVE_GATES.items():
        summary = gates.get(workflow_path)
        if not isinstance(summary, dict):
            raise SystemExit(f"hosted gate summary is invalid: {workflow_path}")
        run_id = require_positive_int(
            summary.get("run_id"), f"{workflow_path}.run_id"
        )
        run_attempt = require_positive_int(
            summary.get("run_attempt"), f"{workflow_path}.run_attempt"
        )
        require_equal(
            summary.get("selection_policy"),
            LATEST_RUN_POLICY,
            f"{workflow_path}.selection_policy",
        )
        if summary.get("event") not in AUTHORITATIVE_EVENTS:
            raise SystemExit(f"{workflow_path} has a non-authoritative event")
        require_equal(
            summary.get("head_branch"), branch, f"{workflow_path}.head_branch"
        )
        require_equal(summary.get("head_sha"), sha, f"{workflow_path}.head_sha")
        require_equal(summary.get("status"), "completed", f"{workflow_path}.status")
        require_equal(
            summary.get("conclusion"), "success", f"{workflow_path}.conclusion"
        )
        jobs = summary.get("jobs")
        if (
            not isinstance(jobs, list)
            or not jobs
            or any(not isinstance(job, dict) for job in jobs)
        ):
            raise SystemExit(f"{workflow_path} lacks validated non-empty jobs")

        selected[name] = {
            "id": run_id,
            "run_attempt": run_attempt,
            "path": workflow_path,
            "event": summary.get("event"),
            "head_branch": summary.get("head_branch"),
            "head_sha": summary.get("head_sha"),
            "status": summary.get("status"),
            "conclusion": summary.get("conclusion"),
            "created_at": summary.get("created_at"),
            "updated_at": summary.get("updated_at"),
            "html_url": f"https://github.com/{repository}/actions/runs/{run_id}",
        }
    return selected


def run_core_collect(
    arguments: list[str],
    *,
    frozen_runs: dict[str, dict[str, Any]],
    repository: str,
    branch: str,
    sha: str,
    core_module: Any | None = None,
) -> int:
    """Run core collection with exactly one injected, immutable run snapshot."""

    if set(frozen_runs) != set(AUTHORITATIVE_GATES):
        raise SystemExit("frozen hosted-run set does not match authoritative gates")

    module = core_module if core_module is not None else load_core_module()
    parser_factory = getattr(module, "build_parser", None)
    original_selector = getattr(module, "collect_gate_runs", None)
    if not callable(parser_factory) or not callable(original_selector):
        raise SystemExit("immutable core lacks its parser or hosted-run selector")

    parsed = parser_factory().parse_args(arguments)
    if getattr(parsed, "command", None) != "collect":
        raise SystemExit("frozen core execution is permitted only for collect")

    calls = 0
    expected_branch = branch

    def frozen_selector(
        selected_repository: str,
        selected_sha: str,
        _token: str,
        attempts: int,
        interval_seconds: int,
        *,
        branch: str,
    ) -> dict[str, dict[str, Any]]:
        nonlocal calls
        calls += 1
        if calls != 1:
            raise SystemExit("immutable core attempted to select hosted runs more than once")
        require_equal(selected_repository, repository, "core selector repository")
        require_equal(selected_sha, sha, "core selector commit")
        require_equal(branch, parsed.branch, "core selector parsed branch")
        require_equal(branch, expected_branch, "core selector frozen branch")
        if type(attempts) is not int or attempts < 1:
            raise SystemExit("core selector polling attempts are invalid")
        if type(interval_seconds) is not int or interval_seconds < 1:
            raise SystemExit("core selector polling interval is invalid")
        return copy.deepcopy(frozen_runs)

    module.collect_gate_runs = frozen_selector
    try:
        result = int(parsed.function(parsed))
    finally:
        module.collect_gate_runs = original_selector

    if calls != 1:
        raise SystemExit(
            f"immutable core consumed the frozen hosted-run snapshot {calls} times"
        )
    return result


def bind_attestations(
    *,
    context_path: Path,
    evidence_dir: Path,
    repository: str,
    branch: str,
    sha: str,
    tree: str,
    run_id: int,
    run_attempt: int,
) -> None:
    context = read_json(context_path, "release context")
    binding_path = evidence_dir / ATTESTATION_EVIDENCE["local-evidence-binding"]
    hosted_path = evidence_dir / ATTESTATION_EVIDENCE["hosted-gate-execution"]
    binding = read_json(binding_path, "local evidence binding")
    hosted = read_json(hosted_path, "hosted gate execution attestation")

    common = {
        "repository": repository,
        "branch": branch,
        "commit_sha": sha,
        "tree_sha": tree,
    }
    for label, payload in (
        ("release context", context),
        ("local evidence binding", binding),
        ("hosted gate execution attestation", hosted),
    ):
        if payload.get("status") not in {None, "ok"} or payload.get("ok") is False:
            raise SystemExit(f"{label} is not successful")
        for field, expected in common.items():
            require_equal(payload.get(field), expected, f"{label}.{field}")

    require_equal(context.get("workflow_run_id"), run_id, "release context.workflow_run_id")
    require_equal(
        context.get("workflow_run_attempt"),
        run_attempt,
        "release context.workflow_run_attempt",
    )
    require_equal(binding.get("workflow_run_id"), run_id, "local binding.workflow_run_id")
    require_equal(
        binding.get("workflow_run_attempt"),
        run_attempt,
        "local binding.workflow_run_attempt",
    )

    context_gates = context.get("hosted_gates")
    attested_gates = hosted.get("gates")
    if not isinstance(context_gates, dict) or not isinstance(attested_gates, dict):
        raise SystemExit("release context and hosted attestation must contain gate maps")
    expected_paths = {
        record.get("workflow_path")
        for record in context_gates.values()
        if isinstance(record, dict)
    }
    if None in expected_paths or set(attested_gates) != expected_paths:
        raise SystemExit("hosted attestation workflow set does not match release context")

    selected_run_ids: dict[str, int] = {}
    for name, record in context_gates.items():
        if not isinstance(record, dict):
            raise SystemExit(f"release context hosted gate is invalid: {name}")
        path = record.get("workflow_path")
        summary = attested_gates.get(path)
        if not isinstance(summary, dict):
            raise SystemExit(f"hosted attestation lacks workflow: {path}")
        for field in (
            "run_id",
            "run_attempt",
            "event",
            "head_branch",
            "head_sha",
            "status",
            "conclusion",
        ):
            require_equal(
                summary.get(field),
                record.get(field),
                f"hosted attestation {path}.{field}",
            )
        jobs = summary.get("jobs")
        if not isinstance(jobs, list) or not jobs:
            raise SystemExit(f"hosted attestation has no successful jobs: {path}")
        selected_run_ids[name] = require_positive_int(
            record.get("run_id"), f"release context hosted gate {name}.run_id"
        )

    files = context.get("files")
    if not isinstance(files, dict):
        raise SystemExit("release context files map is missing")
    attestations: dict[str, dict[str, Any]] = {}
    for name, relative in ATTESTATION_EVIDENCE.items():
        path = evidence_dir / relative
        digest = sha256_file(path)
        require_equal(files.get(relative), digest, f"release context.files[{relative!r}]")
        attestations[name] = {"path": relative, "sha256": digest}
    context["attestations"] = attestations
    context["hosted_gate_selection"] = {
        "schema": "cex.hosted-gate-selection-binding.v1",
        "policy": LATEST_RUN_POLICY,
        "source": ATTESTATION_EVIDENCE["hosted-gate-execution"],
        "sha256": sha256_file(hosted_path),
        "selected_run_ids": selected_run_ids,
    }
    write_json(context_path, context)


def augment_manifest(context_path: Path, output_path: Path, payload_name: str) -> None:
    context = read_json(context_path, "release context")
    manifest = read_json(output_path, "generated candidate manifest")
    attestations = context.get("attestations")
    if not isinstance(attestations, dict) or set(attestations) != set(ATTESTATION_EVIDENCE):
        raise SystemExit("release context lacks the exact attestation evidence set")
    evidence = manifest.get("evidence")
    if not isinstance(evidence, list):
        raise SystemExit("generated candidate manifest evidence must be a list")
    names = [item.get("name") for item in evidence if isinstance(item, dict)]
    if len(names) != len(set(names)):
        raise SystemExit("generated candidate manifest already contains duplicate evidence names")

    for name, relative in ATTESTATION_EVIDENCE.items():
        if name in names:
            raise SystemExit(f"generated candidate manifest already contains {name}")
        record = attestations.get(name)
        if not isinstance(record, dict):
            raise SystemExit(f"invalid attestation context record: {name}")
        require_equal(record.get("path"), relative, f"attestations.{name}.path")
        digest = record.get("sha256")
        if not isinstance(digest, str) or not digest.startswith("sha256:"):
            raise SystemExit(f"invalid attestation digest: {name}")
        evidence.append(
            {
                "name": name,
                "status": "pass",
                "uri": f"artifact://{payload_name}/{relative}",
                "sha256": digest,
                "waiver": None,
            }
        )
    manifest["evidence"] = evidence
    write_json(output_path, manifest)


def main() -> int:
    arguments = sys.argv[1:]
    if not arguments:
        raise SystemExit("p0 release evidence command is required")
    command = arguments[0]

    evidence_dir: Path | None = None
    context_path: Path | None = None
    common_values: dict[str, str] = {}
    core_result: int
    if command == "collect":
        evidence_dir = rooted(option(arguments, "--evidence-dir"))
        context_path = rooted(option(arguments, "--context"))
        common_values = {
            "repository": option(arguments, "--repository"),
            "branch": option(arguments, "--branch"),
            "sha": option(arguments, "--sha"),
            "tree": option(arguments, "--tree"),
        }
        run_id = int(option(arguments, "--run-id"))
        run_attempt = int(option(arguments, "--run-attempt"))
        common = [
            "--repository",
            common_values["repository"],
            "--branch",
            common_values["branch"],
            "--sha",
            common_values["sha"],
            "--tree",
            common_values["tree"],
        ]
        binder = [
            sys.executable,
            str(LOCAL_BINDER),
            "--evidence-dir",
            str(evidence_dir),
            *common,
            "--run-id",
            str(run_id),
            "--run-attempt",
            str(run_attempt),
            "--output",
            str(evidence_dir / ATTESTATION_EVIDENCE["local-evidence-binding"]),
        ]
        if run(binder) != 0:
            return 1

        hosted_path = evidence_dir / ATTESTATION_EVIDENCE["hosted-gate-execution"]
        hosted = [
            sys.executable,
            str(HOSTED_CHECKER),
            *common,
            "--output",
            str(hosted_path),
        ]
        if run(hosted) != 0:
            return 1

        frozen_runs = frozen_runs_from_attestation(
            hosted_path,
            repository=common_values["repository"],
            branch=common_values["branch"],
            sha=common_values["sha"],
            tree=common_values["tree"],
        )
        core_result = run_core_collect(
            arguments,
            frozen_runs=frozen_runs,
            repository=common_values["repository"],
            branch=common_values["branch"],
            sha=common_values["sha"],
        )
    else:
        core_result = run([sys.executable, str(CORE), *arguments])

    if core_result != 0:
        return core_result

    if command == "collect":
        assert evidence_dir is not None and context_path is not None
        bind_attestations(
            context_path=context_path,
            evidence_dir=evidence_dir,
            repository=common_values["repository"],
            branch=common_values["branch"],
            sha=common_values["sha"],
            tree=common_values["tree"],
            run_id=run_id,
            run_attempt=run_attempt,
        )
        return 0

    if command == "manifest":
        context_path = rooted(option(arguments, "--context"))
        output_path = rooted(option(arguments, "--output"))
        payload_name = option(arguments, "--payload-name")
        augment_manifest(context_path, output_path, payload_name)
        return run([sys.executable, str(MANIFEST_VALIDATOR), str(output_path)])

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
