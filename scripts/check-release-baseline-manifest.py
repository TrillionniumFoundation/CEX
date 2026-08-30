#!/usr/bin/env python3
"""Run the legacy schema validator plus the strict v12 evidence contract."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CORE = ROOT / "scripts/check-release-baseline-manifest-core.py"
STRICT = ROOT / "scripts/check-release-baseline-manifest-contract.py"


def run(command: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )


def main() -> int:
    core = run([sys.executable, str(CORE), *sys.argv[1:]])
    if core.stdout:
        print(core.stdout, end="" if core.stdout.endswith("\n") else "\n")
    if core.returncode != 0:
        return core.returncode

    strict = run([sys.executable, str(STRICT), *sys.argv[1:]])
    if strict.stdout:
        print(strict.stdout, end="" if strict.stdout.endswith("\n") else "\n")
    return strict.returncode


if __name__ == "__main__":
    raise SystemExit(main())
