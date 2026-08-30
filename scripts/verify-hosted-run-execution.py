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
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

GIT_SHA_RE = __import__("re").compile(r"^[0-9a-f]{40}$")
EXPECTED_JOBS = {
    "p0-migration-gate": {"fresh-postgres-migrations"},
    "rust-service-gate": {
        "repository-integrity",
        "service-local-gate-windows",
        "service-local-gate-linux",
        "hepta-postgres-integration",
    },
    "p0-gateway-exact-reserve-gate": {"gateway-exact-reserve"},
    "p0-execution-settlement-gate": {"execution-settlement"},
    "p0-provider-reconciliation-gate": {"provider-reconciliation"},
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
    run_attempt: int,
) -> dict[str, Any]:
    job_name = job.get("name")
    if not isinstance(job_name, str) or not job_name:
        raise SystemExit(f"{gate_name} contains a job without a name")
    if job.get("head_sha") != sha:
        raise SystemExit(f"{gate_name}/{job_name} is bound to a different commit")
    if int(job.get("run_attempt") or 0) != run_attempt:
        raise SystemExit(f"{gate_name}/{job_name} is bound to a different run attempt")
    if job.get("status") != "completed" or job.get("conclusion") != "success":
        raise SystemExit(f"{gate_name}/{job_name} is not a completed success")
    if not positive_int(job.get("runner_id")):
        raise SystemExit(f"{gate_name}/{job_name} did not receive a real runner")
    runner_name = job.get("runner_name")
    if not isinstance(runner_name, str) or not runner_name.strip():
        raise SystemExit(f"{gate_name}/{job_name} lacks a runner name")

    steps = job.get("steps")
    if not isinstance(steps, list) or not steps:
        raise SystemExit(f"{gate_name}/{job_name} has no executed steps")
    checkout_succeeded = False
    substantive_successes = 0
    normalized_steps: list[dict[str, Any]] = []
    for step in steps:
        if not isinstance(step, dict):
            raise SystemExit(f"{gate_name}/{job_name} contains an invalid step record")
        step_name = step.get("name")
        if not isinstance(step_name, str) or not step_name:
            raise SystemExit(f"{gate_name}/{job_name} contains an unnamed step")
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
    if not checkout_succeeded:
        raise SystemExit(f"{gate_name}/{job_name} lacks a successful checkout step")
    if substantive_successes < 1:
        raise SystemExit(f"{gate_name}/{job_name} has no successful substantive step")

    record = {
        "job_id": job.get("id"),
        "name": job_name,
        "head_sha": job.get("head_sha"),
        "run_attempt": run_attempt,
        "status": job.get("status"),
        "conclusion": job.get("conclusion"),
        "runner_id": job.get("runner_id"),
        "runner_name": runner_name,
        "runner_group_id": job.get("runner_group_id"),
        "runner_group_name": job.get("runner_group_name"),
        "labels": job.get("labels"),
        "started_at": job.get("started_at"),
        "completed_at": job.get("completed_at"),
        "steps": normalized_steps,
    }
    record["record_sha256"] = canonical_digest(record)
    return record


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--context", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    token = os.environ.get("GITHUB_TOKEN", "")
    if not token:
        raise SystemExit("GITHUB_TOKEN is required")
    context = json.loads(args.context.read_text(encoding="utf-8"))
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
        if set(names) != expected_jobs:
            raise SystemExit(
                f"{gate_name} job set mismatch: expected={sorted(expected_jobs)!r} "
                f"actual={sorted(str(name) for name in names)!r}"
            )
        verified = [
            validate_job(gate_name, job, sha=sha, run_attempt=run_attempt)
            for job in sorted(jobs, key=lambda item: str(item.get("name") or ""))
        ]
        gate_record = {
            "run_id": run_id,
            "run_attempt": run_attempt,
            "workflow_path": gate.get("workflow_path"),
            "status": "success",
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
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
