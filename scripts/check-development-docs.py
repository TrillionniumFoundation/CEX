#!/usr/bin/env python3
"""Run the active documentation core plus architecture and evidence checks."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CORE = ROOT / "scripts/check-development-docs-core.py"
AGENT_BOUNDARY = ROOT / "scripts/check-external-agent-runtime-boundary.py"
EXECUTION_STATE_BOUNDARY = ROOT / "scripts/check-execution-default-state-boundary.py"
EXTERNAL = ROOT / "scripts/check-external-production-evidence-contract.py"


def run_json(
    arguments: list[str], label: str
) -> tuple[int, dict[str, Any] | None, str]:
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


def fallback_result(problem_text: str) -> dict[str, Any]:
    return {
        "schema": "cex.development-doc-check.v1",
        "status": "failed",
        "active_plan": "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md",
        "active_addendum": (
            "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-"
            "IMPLEMENTATION-ADDENDUM.md"
        ),
        "migration_head": "0088_enforce_provider_terminal_evidence_binding.sql",
        "requirements": 18,
        "repository_qualification_result": "PENDING_EXACT_SHA_HOSTED_EVIDENCE",
        "repository_qualification_authority": "generated_candidate_manifest_only",
        "production_authorization": "not_granted",
        "problems": [problem_text],
    }


def append_check(
    problems: list[object],
    *,
    arguments: list[str],
    label: str,
    expected_schema: str,
) -> int:
    code, result, raw = run_json(arguments, label)
    if result is None:
        problems.append(f"{label} did not emit one JSON object: {raw}")
        return code
    child_problems = result.get("problems")
    if isinstance(child_problems, list):
        problems.extend(f"{label}: {item}" for item in child_problems)
    elif result.get("status") != "ok":
        problems.append(f"{label} failed without diagnostics")
    if result.get("schema") != expected_schema:
        problems.append(f"{label} emitted an unexpected schema")
    if result.get("production_authorization") != "not_granted":
        problems.append(f"{label} changed production authorization")
    if result.get("checker_may_grant_production_authorization") is not False:
        problems.append(f"{label} may not grant production authorization")
    if code != 0 and not child_problems:
        problems.append(f"{label} exited nonzero without diagnostics")
    return code


def main() -> int:
    core_code, core, core_raw = run_json(
        [sys.executable, str(CORE)],
        "documentation core",
    )
    if core is None:
        result = fallback_result(
            "documentation core did not emit one JSON object: " + core_raw
        )
        print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
        return 1

    problems = core.get("problems")
    if not isinstance(problems, list):
        problems = ["documentation core problems field is invalid"]
        core["problems"] = problems

    boundary_code = append_check(
        problems,
        arguments=[sys.executable, str(AGENT_BOUNDARY)],
        label="external Agent runtime boundary",
        expected_schema="cex.external-agent-runtime-boundary-check.v1",
    )
    state_boundary_code = append_check(
        problems,
        arguments=[sys.executable, str(EXECUTION_STATE_BOUNDARY)],
        label="Execution default-state privacy boundary",
        expected_schema="cex.execution-default-state-boundary-check.v1",
    )
    contract_code = append_check(
        problems,
        arguments=[sys.executable, str(EXTERNAL), "--contract-only"],
        label="external evidence contract",
        expected_schema="cex.external-production-evidence-contract-check.v1",
    )
    self_test_code = append_check(
        problems,
        arguments=[sys.executable, str(EXTERNAL), "--self-test"],
        label="external evidence binding self-test",
        expected_schema="cex.external-production-evidence-binding-self-test.v1",
    )

    if core_code != 0 and not problems:
        problems.append("documentation core failed without diagnostics")
    if boundary_code != 0 and not problems:
        problems.append("external Agent runtime boundary failed without diagnostics")
    if state_boundary_code != 0 and not problems:
        problems.append("Execution default-state privacy boundary failed without diagnostics")
    if contract_code != 0 and not problems:
        problems.append("external evidence contract failed without diagnostics")
    if self_test_code != 0 and not problems:
        problems.append(
            "external evidence binding self-test failed without diagnostics"
        )

    core["status"] = "failed" if problems else "ok"
    core["production_authorization"] = "not_granted"
    print(json.dumps(core, indent=2, ensure_ascii=False, sort_keys=True))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
