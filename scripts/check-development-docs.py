#!/usr/bin/env python3
"""Run the active documentation core plus external-evidence intake contract."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CORE = ROOT / "scripts/check-development-docs-core.py"
EXTERNAL = ROOT / "scripts/check-external-production-evidence-contract.py"


def run_json(arguments: list[str], label: str) -> tuple[int, dict[str, Any] | None, str]:
    completed = subprocess.run(
        arguments,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    raw = completed.stdout.strip()
    try:
        value = json.loads(raw)
    except json.JSONDecodeError:
        value = None
    if value is not None and not isinstance(value, dict):
        value = None
    return completed.returncode, value, raw or f"<{label} emitted no output>"


def main() -> int:
    core_code, core, core_raw = run_json(
        [sys.executable, str(CORE)],
        "documentation core",
    )
    if core is None:
        result = {
            "schema": "cex.development-doc-check.v1",
            "status": "failed",
            "active_plan": "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md",
            "active_addendum": "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md",
            "migration_head": "0088_enforce_provider_terminal_evidence_binding.sql",
            "requirements": 18,
            "repository_qualification_result": "PENDING_EXACT_SHA_HOSTED_EVIDENCE",
            "repository_qualification_authority": "generated_candidate_manifest_only",
            "production_authorization": "not_granted",
            "problems": ["documentation core did not emit one JSON object: " + core_raw],
        }
        print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
        return 1

    problems = core.get("problems")
    if not isinstance(problems, list):
        problems = ["documentation core problems field is invalid"]
        core["problems"] = problems

    external_code, external, external_raw = run_json(
        [sys.executable, str(EXTERNAL), "--contract-only"],
        "external evidence contract",
    )
    if external is None:
        problems.append(
            "external evidence contract did not emit one JSON object: " + external_raw
        )
    else:
        external_problems = external.get("problems")
        if isinstance(external_problems, list):
            problems.extend(
                "external evidence contract: " + str(item)
                for item in external_problems
            )
        elif external.get("status") != "ok":
            problems.append("external evidence contract failed without diagnostics")
        if external.get("production_authorization") != "not_granted":
            problems.append("external evidence checker changed production authorization")
        if external.get("checker_may_grant_production_authorization") is not False:
            problems.append("external evidence checker may not grant production authorization")

    if core_code != 0 and not problems:
        problems.append("documentation core failed without diagnostics")
    if external_code != 0 and not problems:
        problems.append("external evidence contract failed without diagnostics")

    core["status"] = "failed" if problems else "ok"
    core["production_authorization"] = "not_granted"
    print(json.dumps(core, indent=2, ensure_ascii=False, sort_keys=True))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
