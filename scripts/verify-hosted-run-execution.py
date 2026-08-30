#!/usr/bin/env python3
"""Verify exact-attempt GitHub Actions jobs before CEX candidate qualification.

A workflow-level ``conclusion=success`` is not sufficient release evidence.  This
checker binds every selected authoritative run to the expected branch and SHA,
then requires the exact run attempt to contain the complete expected job set,
a real assigned runner, a successful checkout, and non-empty substantive steps.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

_SCRIPT_DIR = Path(__file__).resolve().parent
if str(_SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPT_DIR))
from evidence_safe_io import (  # noqa: E402
    SafeIOError,
    read_json_nofollow,
    write_json_nofollow,
)

ROOT = Path(__file__).resolve().parents[1]
GIT_SHA_RE = __import__("re").compile(r"^[0-9a-f]{40}$")
EXPECTED_JOBS = {
    "p0-migration-gate": {
        "fresh-postgres-migrations": {
            "runner_label": "ubuntu-latest",
            "steps": {
                "Checkout",
                "Candidate tree hygiene",
                "Fresh apply and P0 database assertions",
                "Term Exchange receipt partial-upgrade regression",
                "Invocation Ledger terminal exclusivity",
            },
        }
    },
    "rust-service-gate": {
        "repository-integrity": {
            "runner_label": "ubuntu-latest",
            "steps": {
                "Checkout exact tree",
                "Validate development-document authority",
                "Emit exact-tree integrity record",
                "Upload repository-integrity evidence",
            },
        },
        "service-local-gate-windows": {
            "runner_label": "windows-latest",
            "steps": {
                "Checkout",
                "Candidate tree hygiene",
                "Verify P0 static wiring",
                "Verify formatting",
                "Compile portable workspace targets (Windows)",
                "Run service-local gate (Windows)",
            },
        },
        "service-local-gate-linux": {
            "runner_label": "ubuntu-latest",
            "steps": {
                "Checkout",
                "Candidate tree hygiene",
                "Verify P0 static wiring",
                "Verify formatting",
                "Compile workspace all targets (Linux)",
                "Run service-local gate (Linux)",
            },
        },
        "hepta-postgres-integration": {
            "runner_label": "ubuntu-latest",
            "steps": {
                "Checkout",
                "Development-document contract",
                "Exact Hepta lint ownership contract",
                "Strict Hepta PostgreSQL package gate",
                "Upload Hepta PostgreSQL evidence",
            },
        },
    },
    "p0-gateway-exact-reserve-gate": {
        "gateway-exact-reserve": {
            "runner_label": "ubuntu-latest",
            "steps": {
                "Checkout",
                "Candidate tree hygiene",
                "Gateway package tests",
                "Workspace all-target compile",
                "Gateway Clippy warnings denied",
                "Gateway exact reserve command lifecycle",
            },
        }
    },
    "p0-execution-settlement-gate": {
        "execution-settlement": {
            "runner_label": "ubuntu-latest",
            "steps": {
                "Checkout",
                "Candidate tree hygiene",
                "Execution package tests",
                "Workspace all-target compile",
                "Durable settlement command lifecycle",
            },
        }
    },
    "p0-provider-reconciliation-gate": {
        "provider-reconciliation": {
            "runner_label": "ubuntu-latest",
            "steps": {
                "Checkout",
                "Candidate tree hygiene",
                "Apply fresh migration chain",
                "Provider unknown-outcome reconciliation lifecycle",
            },
        }
    },
}
EXPECTED_WORKFLOW_PATHS = {
    name: f".github/workflows/{name}.yml" for name in EXPECTED_JOBS
}
ALLOWED_STEP_CONCLUSIONS = {"success", "skipped"}
SYSTEM_STEP_NAMES = {
    "Set up job",
    "Complete job",
    "Initialize containers",
    "Start containers",
    "Stop containers",
}


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def positive_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and value > 0


def api_json(url: str, token: str) -> dict[str, Any]:
    request = urllib.request.Request(
        url,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "User-Agent": "cex-hosted-execution-verifier",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        value = json.load(response)
    if not isinstance(value, dict):
        raise SystemExit("GitHub Actions API response is not an object")
    return value


def canonical_digest(value: Any) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return "sha256:" + hashlib.sha256(encoded).hexdigest()


def jobs_for_attempt(
    repository: str,
    run_id: int,
    run_attempt: int,
    token: str,
) -> list[dict[str, Any]]:
    jobs: list[dict[str, Any]] = []
    per_page = 100
    page = 1
    endpoint = (
        f"https://api.github.com/repos/{repository}/actions/runs/{run_id}"
        f"/attempts/{run_attempt}/jobs"
    )
    while True:
        url = endpoint + "?" + urllib.parse.urlencode(
            {"per_page": per_page, "page": page}
        )
        payload = api_json(url, token)
        page_jobs = payload.get("jobs")
        if not isinstance(page_jobs, list):
            raise SystemExit(f"run {run_id} attempt {run_attempt} response lacks jobs")
        jobs.extend(item for item in page_jobs if isinstance(item, dict))
        total = payload.get("total_count")
        try:
            total_count = int(total)
        except (TypeError, ValueError):
            total_count = None
        if not page_jobs:
            break
        if (total_count is not None and page * per_page >= total_count) or (
            total_count is None and len(page_jobs) < per_page
        ):
            break
        page += 1
        if page > 1000:
            raise SystemExit("GitHub Actions jobs pagination exceeded 1000 pages")
    return jobs


def substantive_step(name: str) -> bool:
    if name in SYSTEM_STEP_NAMES:
        return False
    if name.startswith("Post "):
        return False
    return True


def validate_job(
    gate_name: str,
    job: dict[str, Any],
    *,
    sha: str,
    run_id: int,
    run_attempt: int,
    expected: dict[str, Any],
) -> dict[str, Any]:
    job_name = job.get("name")
    if not isinstance(job_name, str) or not job_name:
        raise SystemExit(f"{gate_name} contains a job without a name")
    if job.get("head_sha") != sha:
        raise SystemExit(f"{gate_name}/{job_name} is bound to a different commit")
    job_id = job.get("id")
    if not positive_int(job_id):
        raise SystemExit(f"{gate_name}/{job_name} has an invalid job id")
    job_run_id = job.get("run_id")
    if not positive_int(job_run_id) or job_run_id != run_id:
        raise SystemExit(f"{gate_name}/{job_name} is bound to a different run")
    job_attempt = job.get("run_attempt")
    if not positive_int(job_attempt) or job_attempt != run_attempt:
        raise SystemExit(f"{gate_name}/{job_name} is bound to a different run attempt")
    if job.get("status") != "completed" or job.get("conclusion") != "success":
        raise SystemExit(f"{gate_name}/{job_name} is not a completed success")
    if not positive_int(job.get("runner_id")):
        raise SystemExit(f"{gate_name}/{job_name} did not receive a real runner")
    runner_name = job.get("runner_name")
    if not isinstance(runner_name, str) or not runner_name.strip():
        raise SystemExit(f"{gate_name}/{job_name} lacks a runner name")
    labels = job.get("labels")
    required_label = expected.get("runner_label")
    if not isinstance(labels, list) or not isinstance(required_label, str) or required_label not in labels:
        raise SystemExit(
            f"{gate_name}/{job_name} lacks required runner label {required_label!r}"
        )

    steps = job.get("steps")
    if not isinstance(steps, list) or not steps:
        raise SystemExit(f"{gate_name}/{job_name} has no executed steps")
    checkout_succeeded = False
    substantive_successes = 0
    normalized_steps: list[dict[str, Any]] = []
    step_names: set[str] = set()
    for step in steps:
        if not isinstance(step, dict):
            raise SystemExit(f"{gate_name}/{job_name} contains an invalid step record")
        step_name = step.get("name")
        if not isinstance(step_name, str) or not step_name:
            raise SystemExit(f"{gate_name}/{job_name} contains an unnamed step")
        if step_name in step_names:
            raise SystemExit(f"{gate_name}/{job_name} contains duplicate step {step_name}")
        step_names.add(step_name)
        status = step.get("status")
        conclusion = step.get("conclusion")
        if status != "completed":
            raise SystemExit(f"{gate_name}/{job_name}/{step_name} did not complete")
        if conclusion not in ALLOWED_STEP_CONCLUSIONS:
            raise SystemExit(
                f"{gate_name}/{job_name}/{step_name} has disallowed conclusion {conclusion!r}"
            )
        if step_name.lower().startswith("checkout") and conclusion == "success":
            checkout_succeeded = True
        if substantive_step(step_name) and conclusion == "success":
            substantive_successes += 1
        normalized_steps.append(
            {
                "name": step_name,
                "number": step.get("number"),
                "status": status,
                "conclusion": conclusion,
                "started_at": step.get("started_at"),
                "completed_at": step.get("completed_at"),
            }
        )
    required_steps = expected.get("steps")
    if not isinstance(required_steps, set) or not required_steps.issubset(step_names):
        missing = sorted(required_steps - step_names) if isinstance(required_steps, set) else []
        raise SystemExit(
            f"{gate_name}/{job_name} is missing required step(s): {', '.join(missing)}"
        )
    by_step = {step["name"]: step for step in steps if isinstance(step, dict) and isinstance(step.get("name"), str)}
    for required_name in sorted(required_steps):
        required_step = by_step[required_name]
        if required_step.get("status") != "completed" or required_step.get("conclusion") != "success":
            raise SystemExit(
                f"{gate_name}/{job_name}/{required_name} did not succeed"
            )
    if not checkout_succeeded:
        raise SystemExit(f"{gate_name}/{job_name} lacks a successful checkout step")
    if substantive_successes < 1:
        raise SystemExit(f"{gate_name}/{job_name} has no successful substantive step")

    record = {
        "job_id": job_id,
        "run_id": run_id,
        "name": job_name,
        "head_sha": job.get("head_sha"),
        "run_attempt": run_attempt,
        "status": job.get("status"),
        "conclusion": job.get("conclusion"),
        "runner_id": job.get("runner_id"),
        "runner_name": runner_name,
        "runner_group_id": job.get("runner_group_id"),
        "runner_group_name": job.get("runner_group_name"),
        "labels": labels,
        "required_runner_label": required_label,
        "required_steps": sorted(required_steps),
        "started_at": job.get("started_at"),
        "completed_at": job.get("completed_at"),
        "steps": normalized_steps,
    }
    record["record_sha256"] = canonical_digest(record)
    return record


def self_test() -> list[str]:
    """Exercise required runner/step binding without contacting GitHub."""

    failures: list[str] = []
    gate_name = "p0-migration-gate"
    job_name = "fresh-postgres-migrations"
    expected = EXPECTED_JOBS[gate_name][job_name]
    required_steps = sorted(expected["steps"])
    valid = {
        "id": 101,
        "name": job_name,
        "head_sha": "a" * 40,
        "run_id": 202,
        "run_attempt": 1,
        "status": "completed",
        "conclusion": "success",
        "runner_id": 303,
        "runner_name": "GitHub Actions 303",
        "labels": ["ubuntu-latest"],
        "steps": [
            {"name": name, "status": "completed", "conclusion": "success"}
            for name in required_steps
        ],
    }
    try:
        validate_job(
            gate_name,
            valid,
            sha="a" * 40,
            run_id=202,
            run_attempt=1,
            expected=expected,
        )
    except SystemExit:
        failures.append("valid required-job fixture was rejected")

    mutations = {
        "wrong-run": {**valid, "run_id": 999},
        "wrong-label": {**valid, "labels": ["self-hosted"]},
        "missing-step": {
            **valid,
            "steps": valid["steps"][1:],
        },
        "zero-runner": {**valid, "runner_id": 0, "runner_name": ""},
    }
    for name, mutated in mutations.items():
        try:
            validate_job(
                gate_name,
                mutated,
                sha="a" * 40,
                run_id=202,
                run_attempt=1,
                expected=expected,
            )
        except SystemExit:
            continue
        failures.append(f"negative required-job fixture was accepted: {name}")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--context", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    failures = self_test()
    if failures:
        raise SystemExit("hosted execution verifier self-test failed: " + "; ".join(failures))
    if args.self_test:
        print(json.dumps({"schema": "cex.hosted-run-execution-verifier-self-test.v1", "status": "ok"}))
        return 0
    if args.context is None or args.output is None:
        parser.error("--context and --output are required unless --self-test is used")

    token = os.environ.get("GITHUB_TOKEN", "")
    if not token:
        raise SystemExit("GITHUB_TOKEN is required")
    context_path = args.context if args.context.is_absolute() else ROOT / args.context
    output_path = args.output if args.output.is_absolute() else ROOT / args.output
    try:
        context = read_json_nofollow(context_path, label="release context")
    except SafeIOError as error:
        raise SystemExit(str(error)) from error
    if not isinstance(context, dict):
        raise SystemExit("release context must be an object")

    repository = context.get("repository")
    branch = context.get("branch")
    sha = context.get("commit_sha")
    tree = context.get("tree_sha")
    hosted = context.get("hosted_gates")
    if not isinstance(repository, str) or not repository:
        raise SystemExit("release context repository is invalid")
    if not isinstance(branch, str) or not branch:
        raise SystemExit("release context branch is invalid")
    if not isinstance(sha, str) or not GIT_SHA_RE.fullmatch(sha):
        raise SystemExit("release context commit_sha is invalid")
    if not isinstance(tree, str) or not GIT_SHA_RE.fullmatch(tree):
        raise SystemExit("release context tree_sha is invalid")
    if not isinstance(hosted, dict) or set(hosted) != set(EXPECTED_JOBS):
        raise SystemExit("release context hosted gate set is invalid")

    gate_records: dict[str, Any] = {}
    for gate_name, expected_jobs in EXPECTED_JOBS.items():
        gate = hosted.get(gate_name)
        if not isinstance(gate, dict):
            raise SystemExit(f"hosted gate record is invalid: {gate_name}")
        if gate.get("repository") != repository:
            raise SystemExit(f"{gate_name} is bound to a different repository")
        if gate.get("workflow_path") != EXPECTED_WORKFLOW_PATHS[gate_name]:
            raise SystemExit(f"{gate_name} has an unexpected workflow path")
        if gate.get("branch") != branch or gate.get("head_branch") != branch:
            raise SystemExit(f"{gate_name} is bound to a different branch")
        if gate.get("head_sha") != sha:
            raise SystemExit(f"{gate_name} is bound to a different commit")
        if gate.get("status") != "completed" or gate.get("conclusion") != "success":
            raise SystemExit(f"{gate_name} is not a completed success")
        run_id = gate.get("run_id")
        run_attempt = gate.get("run_attempt")
        if not positive_int(run_id) or not positive_int(run_attempt):
            raise SystemExit(f"{gate_name} has an invalid run identity")

        jobs = jobs_for_attempt(repository, run_id, run_attempt, token)
        names = [job.get("name") for job in jobs]
        if len(names) != len(set(names)):
            raise SystemExit(f"{gate_name} contains duplicate job names")
        if set(names) != set(expected_jobs):
            raise SystemExit(
                f"{gate_name} job set mismatch: expected={sorted(expected_jobs)!r} "
                f"actual={sorted(str(name) for name in names)!r}"
            )
        verified = [
            validate_job(
                gate_name,
                job,
                sha=sha,
                run_id=run_id,
                run_attempt=run_attempt,
                expected=expected_jobs[job.get("name")],
            )
            for job in sorted(jobs, key=lambda item: str(item.get("name") or ""))
        ]
        gate_record = {
            "repository": repository,
            "branch": branch,
            "head_branch": gate.get("head_branch"),
            "head_sha": sha,
            "event": gate.get("event"),
            "run_id": run_id,
            "run_attempt": run_attempt,
            "workflow_path": EXPECTED_WORKFLOW_PATHS[gate_name],
            "status": "success",
            "created_at": gate.get("created_at"),
            "updated_at": gate.get("updated_at"),
            "jobs": verified,
        }
        gate_record["jobs_sha256"] = canonical_digest(verified)
        gate_records[gate_name] = gate_record

    result = {
        "schema": "cex.hosted-run-execution-verification.v1",
        "status": "ok",
        "ok": True,
        "repository": repository,
        "branch": branch,
        "commit_sha": sha,
        "tree_sha": tree,
        "verified_at": utc_now(),
        "gates": gate_records,
    }
    try:
        write_json_nofollow(output_path, result)
    except SafeIOError as error:
        raise SystemExit(str(error)) from error
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
