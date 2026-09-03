#!/usr/bin/env python3
"""Run the active documentation gates; no child may turn failure into success."""

from __future__ import annotations

import json
import os
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CORE = ROOT / "scripts/check-development-docs-core.py"
AGENT_BOUNDARY = ROOT / "scripts/check-external-agent-runtime-boundary.py"
EXECUTION_STATE_BOUNDARY = ROOT / "scripts/check-execution-default-state-boundary.py"
EXTERNAL = ROOT / "scripts/check-external-production-evidence-contract.py"
MAX_OUTPUT_BYTES = 1_048_576
CHECK_TIMEOUT_SECONDS = 120.0


def unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON member")
        result[key] = value
    return result


def reject_constant(_value: str) -> None:
    raise ValueError("non-finite JSON number")


def stop_process(process: subprocess.Popen[bytes]) -> None:
    # These children are checkers, not services. Reap their descendants too.
    if os.name == "posix":
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    elif process.poll() is None:
        try:
            subprocess.run(
                ["taskkill", "/PID", str(process.pid), "/T", "/F"],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                timeout=5, check=False,
            )
        except (OSError, subprocess.TimeoutExpired):
            process.kill()
    if process.poll() is None:
        process.kill()
    process.wait(timeout=5)


def run_json(
    arguments: list[str], label: str
) -> tuple[int, dict[str, Any] | None, str]:
    # Spool separately to disk; never buffer unbounded child output in memory
    # and never treat stderr as a substitute for the stdout result object.
    try:
        with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
            process = subprocess.Popen(
                arguments, cwd=ROOT, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr,
                start_new_session=os.name == "posix",
            )
            deadline = time.monotonic() + CHECK_TIMEOUT_SECONDS
            failure = ""
            while True:
                size = os.fstat(stdout.fileno()).st_size + os.fstat(stderr.fileno()).st_size
                if size > MAX_OUTPUT_BYTES:
                    failure = "output limit exceeded"
                    break
                if time.monotonic() >= deadline:
                    failure = "timeout"
                    break
                if process.poll() is not None:
                    break
                time.sleep(0.02)
            if failure:
                stop_process(process)
                return 1, None, f"{label}: {failure}"
            code = process.wait(timeout=5)
            if os.fstat(stdout.fileno()).st_size + os.fstat(stderr.fileno()).st_size > MAX_OUTPUT_BYTES:
                return 1, None, f"{label}: output limit exceeded"
            stdout.seek(0)
            raw = stdout.read(MAX_OUTPUT_BYTES + 1).decode("utf-8", errors="strict").strip()
        value = json.loads(raw, object_pairs_hook=unique_object, parse_constant=reject_constant)
        if not isinstance(value, dict):
            return code, None, f"{label}: JSON root must be an object"
        return code, value, ""
    except (OSError, ValueError, RecursionError, subprocess.TimeoutExpired):
        # Child bytes can contain credentials or private research material.
        # Do not echo malformed stdout/stderr or raw exception strings.
        return 1, None, f"{label}: unavailable checker or invalid JSON result"


def fallback_result(problem_text: str) -> dict[str, Any]:
    return {
        "schema": "cex.development-doc-check.v1",
        "status": "failed",
        "active_plan": "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md",
        "active_addendum": "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md",
        "migration_head": "0088_enforce_provider_terminal_evidence_binding.sql",
        "requirements": 18,
        "repository_qualification_result": "PENDING_EXACT_SHA_HOSTED_EVIDENCE",
        "repository_qualification_authority": "generated_candidate_manifest_only",
        "production_authorization": "not_granted",
        "problems": [problem_text],
    }


def validate_result(
    code: int,
    result: dict[str, Any] | None,
    diagnostic: str,
    *,
    label: str,
    expected_schema: str,
    require_denial_flag: bool = True,
) -> list[str]:
    problems: list[str] = []
    if result is None:
        return [diagnostic or f"{label}: missing result object"]
    # Each assertion is independent. Empty diagnostics must not hide a failed
    # status, missing status, unexpected schema, or nonzero exit code.
    if code != 0:
        problems.append(f"{label}: exited nonzero ({code})")
    if result.get("status") != "ok":
        problems.append(f"{label}: status is not ok")
    if result.get("schema") != expected_schema:
        problems.append(f"{label}: unexpected schema")
    if result.get("production_authorization") != "not_granted":
        problems.append(f"{label}: invalid production authorization")
    if require_denial_flag and result.get("checker_may_grant_production_authorization") is not False:
        problems.append(f"{label}: missing explicit authorization denial")
    children = result.get("problems")
    if not isinstance(children, list) or len(children) > 200 or any(
        not isinstance(item, str) or not item.strip() or len(item) > 4096
        for item in children
    ):
        problems.append(f"{label}: invalid problems array")
    else:
        problems.extend(f"{label}: {item}" for item in children)
    return problems


def append_check(
    problems: list[object], *, arguments: list[str], label: str, expected_schema: str
) -> int:
    code, result, diagnostic = run_json(arguments, label)
    problems.extend(validate_result(
        code, result, diagnostic, label=label, expected_schema=expected_schema,
    ))
    return code


def main() -> int:
    core_code, core, diagnostic = run_json([sys.executable, str(CORE)], "documentation core")
    problems = validate_result(
        core_code, core, diagnostic, label="documentation core",
        expected_schema="cex.development-doc-check.v1", require_denial_flag=False,
    )
    result = fallback_result("")
    # Emit only the established public result fields, never arbitrary child
    # metadata. Verify all authority fields before normalizing the result.
    if core is not None:
        for key, value in result.items():
            if key not in {"status", "problems"} and (
                type(core.get(key)) is not type(value) or core.get(key) != value
            ):
                problems.append(f"documentation core: invalid {key}")
    checks = (
        (AGENT_BOUNDARY, [], "external Agent runtime boundary", "cex.external-agent-runtime-boundary-check.v1"),
        (EXECUTION_STATE_BOUNDARY, [], "Execution default-state privacy boundary", "cex.execution-default-state-boundary-check.v1"),
        (EXTERNAL, ["--contract-only"], "external evidence contract", "cex.external-production-evidence-contract-check.v1"),
        (EXTERNAL, ["--self-test"], "external evidence binding self-test", "cex.external-production-evidence-binding-self-test.v1"),
    )
    for path, flags, label, schema in checks:
        append_check(problems, arguments=[sys.executable, str(path), *flags], label=label, expected_schema=schema)
    result["problems"] = problems
    result["status"] = "failed" if problems else "ok"
    print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
