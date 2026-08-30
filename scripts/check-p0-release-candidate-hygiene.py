#!/usr/bin/env python3
"""Combine the legacy candidate hygiene check with strict workflow trust validation."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CORE = ROOT / "scripts/check-p0-release-candidate-hygiene-core.py"
TRUST = ROOT / "scripts/check-workflow-trust.py"


def run_json(path: Path) -> tuple[int, dict[str, Any] | None, str]:
    completed = subprocess.run(
        [sys.executable, str(path)],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    raw = completed.stdout.strip()
    try:
        payload = json.loads(raw)
    except json.JSONDecodeError:
        payload = None
    return completed.returncode, payload, raw


def payload_problems(
    label: str,
    returncode: int,
    payload: dict[str, Any] | None,
    raw: str,
) -> list[str]:
    if payload is None:
        return [f"{label} did not emit one JSON object: {raw or '<empty>'}"]
    problems = payload.get("problems")
    result = [str(item) for item in problems] if isinstance(problems, list) else []
    if returncode != 0 and not result:
        result.append(f"{label} failed with exit code {returncode}")
    return result


def main() -> int:
    core_code, core, core_raw = run_json(CORE)
    trust_code, trust, trust_raw = run_json(TRUST)

    problems = payload_problems("candidate hygiene core", core_code, core, core_raw)
    problems.extend(payload_problems("workflow trust check", trust_code, trust, trust_raw))

    result: dict[str, Any] = dict(core or {})
    result["schema"] = "cex.p0-release-candidate-hygiene.v2"
    result["status"] = "failed" if problems else "ok"
    result["ok"] = not problems
    result["problems"] = problems
    result["workflow_trust"] = {
        "status": trust.get("status") if trust else "failed",
        "workflow_count": trust.get("workflow_count") if trust else None,
        "local_action_descriptor_count": (
            trust.get("local_action_descriptor_count") if trust else None
        ),
    }
    if result.get("commit_sha") is None and trust:
        result["commit_sha"] = trust.get("commit_sha")
    if result.get("tree_sha") is None and trust:
        result["tree_sha"] = trust.get("tree_sha")

    print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
    return 1 if problems else 0


if __name__ == "__main__":
    raise SystemExit(main())
