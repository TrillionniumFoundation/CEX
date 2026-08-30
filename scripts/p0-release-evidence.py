#!/usr/bin/env python3
"""Add strict local/hosted evidence attestation around the v12 evidence core."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CORE = ROOT / "scripts/p0-release-evidence-core.py"
LOCAL_BINDER = ROOT / "scripts/bind-p0-local-evidence.py"
HOSTED_CHECKER = ROOT / "scripts/check-hosted-gate-execution.py"
MANIFEST_VALIDATOR = ROOT / "scripts/check-release-baseline-manifest.py"


def option(arguments: list[str], name: str) -> str:
    try:
        index = arguments.index(name)
        value = arguments[index + 1]
    except (ValueError, IndexError) as error:
        raise SystemExit(f"missing required wrapper option: {name}") from error
    if not value:
        raise SystemExit(f"empty required wrapper option: {name}")
    return value


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


def main() -> int:
    arguments = sys.argv[1:]
    if not arguments:
        raise SystemExit("p0 release evidence command is required")
    command = arguments[0]

    if command == "collect":
        evidence_dir = Path(option(arguments, "--evidence-dir"))
        if not evidence_dir.is_absolute():
            evidence_dir = ROOT / evidence_dir
        common = [
            "--repository",
            option(arguments, "--repository"),
            "--branch",
            option(arguments, "--branch"),
            "--sha",
            option(arguments, "--sha"),
            "--tree",
            option(arguments, "--tree"),
        ]
        binder = [
            sys.executable,
            str(LOCAL_BINDER),
            "--evidence-dir",
            str(evidence_dir),
            *common,
            "--run-id",
            option(arguments, "--run-id"),
            "--run-attempt",
            option(arguments, "--run-attempt"),
            "--output",
            str(evidence_dir / "local-evidence-binding.json"),
        ]
        if run(binder) != 0:
            return 1

        hosted = [
            sys.executable,
            str(HOSTED_CHECKER),
            *common,
            "--output",
            str(evidence_dir / "hosted-gate-execution.json"),
        ]
        if run(hosted) != 0:
            return 1

    core_result = run([sys.executable, str(CORE), *arguments])
    if core_result != 0:
        return core_result

    if command == "manifest":
        output = option(arguments, "--output")
        return run([sys.executable, str(MANIFEST_VALIDATOR), output])

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
