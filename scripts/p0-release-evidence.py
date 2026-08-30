#!/usr/bin/env python3
"""Add strict local/hosted evidence attestation around the v12 evidence core."""

from __future__ import annotations

import hashlib
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

        hosted = [
            sys.executable,
            str(HOSTED_CHECKER),
            *common,
            "--output",
            str(evidence_dir / ATTESTATION_EVIDENCE["hosted-gate-execution"]),
        ]
        if run(hosted) != 0:
            return 1

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
