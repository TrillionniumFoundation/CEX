#!/usr/bin/env python3
"""Prove that every selected hosted gate ran real jobs and required steps."""

from __future__ import annotations

import argparse
import json
import os
import re
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

REQUIRED_GATES: dict[str, dict[str, set[str]]] = {
    ".github/workflows/p0-migration-gate.yml": {
        "fresh-postgres-migrations": {
            "Checkout",
            "Candidate tree hygiene",
            "Fresh apply and P0 database assertions",
            "Term Exchange receipt partial-upgrade regression",
            "Invocation Ledger terminal exclusivity",
        }
    },
    ".github/workflows/rust-service-gate.yml": {
        "repository-integrity": {
            "Checkout exact tree",
            "Validate development-document authority",
            "Emit exact-tree integrity record",
            "Upload repository-integrity evidence",
        },
        "service-local-gate-windows": {
            "Checkout",
            "Candidate tree hygiene",
            "Verify P0 static wiring",
            "Verify formatting",
            "Compile portable workspace targets (Windows)",
            "Run service-local gate (Windows)",
        },
        "service-local-gate-linux": {
            "Checkout",
            "Candidate tree hygiene",
            "Verify P0 static wiring",
            "Verify formatting",
            "Compile workspace all targets (Linux)",
            "Run service-local gate (Linux)",
        },
        "hepta-postgres-integration": {
            "Checkout",
            "Development-document contract",
            "Exact Hepta lint ownership contract",
            "Strict Hepta PostgreSQL package gate",
            "Upload Hepta PostgreSQL evidence",
        },
    },
    ".github/workflows/p0-gateway-exact-reserve-gate.yml": {
        "gateway-exact-reserve": {
            "Checkout",
            "Candidate tree hygiene",
            "Gateway package tests",
            "Workspace all-target compile",
            "Gateway Clippy warnings denied",
            "Gateway exact reserve command lifecycle",
        }
    },
    ".github/workflows/p0-execution-settlement-gate.yml": {
        "execution-settlement": {
            "Checkout",
            "Candidate tree hygiene",
            "Execution package tests",
            "Workspace all-target compile",
            "Durable settlement command lifecycle",
        }
    },
    ".github/workflows/p0-provider-reconciliation-gate.yml": {
        "provider-reconciliation": {
            "Checkout",
            "Candidate tree hygiene",
            "Apply fresh migration chain",
            "Provider unknown-outcome reconciliation lifecycle",
        }
    },
}
AUTHORITATIVE_EVENTS = {"push", "workflow_dispatch"}
GIT_SHA_RE = re.compile(r"^[0-9a-f]{40}$")


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def is_positive_int(value: Any) -> bool:
    return type(value) is int and value > 0


def api_json(url: str, token: str) -> dict[str, Any]:
    request = urllib.request.Request(
        url,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "User-Agent": "cex-hosted-gate-execution-check",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        payload = json.load(response)
    if not isinstance(payload, dict):
        raise SystemExit(f"GitHub API returned a non-object for {url}")
    return payload


def paged_collection(
    endpoint: str,
    token: str,
    key: str,
    *,
    query: dict[str, Any] | None = None,
) -> list[dict[str, Any]]:
    page = 1
    per_page = 100
    items: list[dict[str, Any]] = []
    while True:
        params = dict(query or {})
        params.update({"per_page": per_page, "page": page})
        payload = api_json(endpoint + "?" + urllib.parse.urlencode(params), token)
        page_items = payload.get(key)
        if not isinstance(page_items, list):
            raise SystemExit(f"GitHub API response lacks {key}")
        items.extend(item for item in page_items if isinstance(item, dict))
        total_count = payload.get("total_count")
        if isinstance(total_count, int) and page * per_page >= total_count:
            break
        if len(page_items) < per_page:
            break
        page += 1
        if page > 1000:
            raise SystemExit(f"GitHub API pagination exceeded 1000 pages for {key}")
    return items


def run_sort_key(run: dict[str, Any]) -> tuple[str, int, int]:
    def as_int(value: Any) -> int:
        try:
            return int(value or 0)
        except (TypeError, ValueError):
            return 0

    return (
        str(run.get("created_at") or ""),
        as_int(run.get("run_attempt")),
        as_int(run.get("id")),
    )


def select_runs(repository: str, branch: str, sha: str, token: str) -> dict[str, dict[str, Any]]:
    runs = paged_collection(
        f"https://api.github.com/repos/{repository}/actions/runs",
        token,
        "workflow_runs",
        query={"branch": branch, "head_sha": sha},
    )
    selected: dict[str, dict[str, Any]] = {}
    for workflow_path in REQUIRED_GATES:
        candidates = [
            run
            for run in runs
            if run.get("path") == workflow_path
            and run.get("head_sha") == sha
            and run.get("head_branch") == branch
            and run.get("event") in AUTHORITATIVE_EVENTS
            and run.get("status") == "completed"
            and run.get("conclusion") == "success"
        ]
        if not candidates:
            terminal = [
                run
                for run in runs
                if run.get("path") == workflow_path
                and run.get("head_sha") == sha
                and run.get("head_branch") == branch
                and run.get("event") in AUTHORITATIVE_EVENTS
            ]
            if terminal:
                newest = max(terminal, key=run_sort_key)
                raise SystemExit(
                    "no successful exact-branch run for "
                    f"{workflow_path}; newest={newest.get('id')} "
                    f"status={newest.get('status')} conclusion={newest.get('conclusion')}"
                )
            raise SystemExit(f"missing exact-branch run for {workflow_path}")
        selected[workflow_path] = max(candidates, key=run_sort_key)
    return selected


def jobs_for_attempt(
    repository: str,
    run_id: int,
    run_attempt: int,
    token: str,
) -> list[dict[str, Any]]:
    return paged_collection(
        (
            f"https://api.github.com/repos/{repository}/actions/runs/{run_id}"
            f"/attempts/{run_attempt}/jobs"
        ),
        token,
        "jobs",
    )


def validate_job_set(
    workflow_path: str,
    expected: dict[str, set[str]],
    jobs: list[dict[str, Any]],
    *,
    sha: str,
    run_id: int,
    run_attempt: int,
) -> tuple[list[str], list[dict[str, Any]]]:
    problems: list[str] = []
    by_name: dict[str, dict[str, Any]] = {}
    for job in jobs:
        name = job.get("name")
        if not isinstance(name, str) or not name:
            problems.append(f"{workflow_path}: job without a valid name")
            continue
        if name in by_name:
            problems.append(f"{workflow_path}: duplicate job name {name}")
        by_name[name] = job

    if set(by_name) != set(expected):
        problems.append(
            f"{workflow_path}: job set mismatch "
            f"missing={sorted(set(expected) - set(by_name))} "
            f"extra={sorted(set(by_name) - set(expected))}"
        )

    summaries: list[dict[str, Any]] = []
    for name, required_steps in expected.items():
        job = by_name.get(name)
        if job is None:
            continue
        if job.get("head_sha") != sha:
            problems.append(f"{workflow_path}/{name}: job is bound to a different commit")
        if job.get("run_id") != run_id:
            problems.append(f"{workflow_path}/{name}: job is bound to a different run")
        if job.get("run_attempt") != run_attempt:
            problems.append(f"{workflow_path}/{name}: job is bound to a different attempt")
        if job.get("status") != "completed" or job.get("conclusion") != "success":
            problems.append(f"{workflow_path}/{name}: job is not a completed success")
        if not is_positive_int(job.get("id")):
            problems.append(f"{workflow_path}/{name}: job id is invalid")
        if not is_positive_int(job.get("runner_id")):
            problems.append(f"{workflow_path}/{name}: no real runner was allocated")
        if not isinstance(job.get("runner_name"), str) or not job.get("runner_name").strip():
            problems.append(f"{workflow_path}/{name}: runner_name is empty")

        steps = job.get("steps")
        step_by_name: dict[str, dict[str, Any]] = {}
        if not isinstance(steps, list) or not steps:
            problems.append(f"{workflow_path}/{name}: job has no executed step records")
            steps = []
        for step in steps:
            if not isinstance(step, dict):
                continue
            step_name = step.get("name")
            if isinstance(step_name, str) and step_name:
                if step_name in step_by_name:
                    problems.append(f"{workflow_path}/{name}: duplicate step name {step_name}")
                step_by_name[step_name] = step

        for step_name in sorted(required_steps):
            step = step_by_name.get(step_name)
            if step is None:
                problems.append(f"{workflow_path}/{name}: missing required step {step_name}")
                continue
            if step.get("status") != "completed" or step.get("conclusion") != "success":
                problems.append(
                    f"{workflow_path}/{name}: required step did not succeed: "
                    f"{step_name} ({step.get('status')}/{step.get('conclusion')})"
                )

        summaries.append(
            {
                "job_id": job.get("id"),
                "name": name,
                "runner_id": job.get("runner_id"),
                "runner_name": job.get("runner_name"),
                "status": job.get("status"),
                "conclusion": job.get("conclusion"),
                "required_steps": sorted(required_steps),
                "observed_step_count": len(steps),
            }
        )
    return problems, summaries


def self_test() -> list[str]:
    workflow_path = ".github/workflows/example.yml"
    expected = {"gate": {"Checkout", "Run tests"}}
    valid_job = {
        "id": 1,
        "name": "gate",
        "head_sha": "a" * 40,
        "run_id": 2,
        "run_attempt": 1,
        "status": "completed",
        "conclusion": "success",
        "runner_id": 3,
        "runner_name": "GitHub Actions 3",
        "steps": [
            {"name": "Checkout", "status": "completed", "conclusion": "success"},
            {"name": "Run tests", "status": "completed", "conclusion": "success"},
        ],
    }
    problems, _ = validate_job_set(
        workflow_path,
        expected,
        [valid_job],
        sha="a" * 40,
        run_id=2,
        run_attempt=1,
    )
    failures = ["valid job fixture was rejected"] if problems else []

    for label, mutation in (
        ("zero-step", {**valid_job, "steps": []}),
        ("zero-runner", {**valid_job, "runner_id": 0, "runner_name": ""}),
        (
            "missing-required-step",
            {
                **valid_job,
                "steps": [
                    {"name": "Checkout", "status": "completed", "conclusion": "success"}
                ],
            },
        ),
    ):
        problems, _ = validate_job_set(
            workflow_path,
            expected,
            [mutation],
            sha="a" * 40,
            run_id=2,
            run_attempt=1,
        )
        if not problems:
            failures.append(f"negative self-test accepted {label}")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", required=True)
    parser.add_argument("--branch", required=True)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--tree", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    failures = self_test()
    if failures:
        raise SystemExit("hosted-gate checker self-test failed: " + "; ".join(failures))
    if not GIT_SHA_RE.fullmatch(args.sha) or not GIT_SHA_RE.fullmatch(args.tree):
        raise SystemExit("sha/tree must be 40-character lowercase Git object ids")
    token = os.environ.get("GITHUB_TOKEN", "")
    if not token:
        raise SystemExit("GITHUB_TOKEN is required")

    selected = select_runs(args.repository, args.branch, args.sha, token)
    gate_summaries: dict[str, Any] = {}
    all_problems: list[str] = []
    for workflow_path, expected_jobs in REQUIRED_GATES.items():
        run = selected[workflow_path]
        run_id = run.get("id")
        run_attempt = run.get("run_attempt")
        if not is_positive_int(run_id) or not is_positive_int(run_attempt):
            all_problems.append(f"{workflow_path}: run id/attempt is invalid")
            continue
        jobs = jobs_for_attempt(args.repository, run_id, run_attempt, token)
        problems, summaries = validate_job_set(
            workflow_path,
            expected_jobs,
            jobs,
            sha=args.sha,
            run_id=run_id,
            run_attempt=run_attempt,
        )
        all_problems.extend(problems)
        gate_summaries[workflow_path] = {
            "run_id": run_id,
            "run_attempt": run_attempt,
            "event": run.get("event"),
            "head_branch": run.get("head_branch"),
            "head_sha": run.get("head_sha"),
            "status": run.get("status"),
            "conclusion": run.get("conclusion"),
            "jobs": summaries,
        }

    if all_problems:
        raise SystemExit("hosted gate execution evidence failed: " + "; ".join(all_problems))

    result = {
        "schema": "cex.hosted-gate-execution.v1",
        "status": "ok",
        "ok": True,
        "repository": args.repository,
        "branch": args.branch,
        "commit_sha": args.sha,
        "tree_sha": args.tree,
        "generated_at": utc_now(),
        "gates": gate_summaries,
    }
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
