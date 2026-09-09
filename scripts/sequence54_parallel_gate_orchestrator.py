#!/usr/bin/env python3
"""Parallel, exact-run-identity Sequence 54 GitHub Actions orchestrator.

This controller never writes source or statuses. It dispatches only committed
workflow_dispatch entry points, binds every credited result to a concrete run ID
and exact candidate SHA, retries cancellation at most twice, and emits a bounded
JSON report. A terminal source/test failure is returned as failure, not retried
into a misleading green badge.
"""
from __future__ import annotations

import argparse
from dataclasses import dataclass, field
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import sys
import time
from typing import Any
from urllib.error import HTTPError
from urllib.parse import quote
from urllib.request import Request, urlopen


DESIRED_WORKFLOWS = (
    "p0-runner-diagnostics",
    "p0-rust-toolchain-convergence",
    "repository-semantic-contract-gate",
    "consumer-projection-boundary-gate",
    "execution-lifecycle-gate",
    "matrix-review-repair-regression",
    "trnm-economy-ci",
    "trnm-economy-settlement",
    "world-settlement-external-evidence",
    "p0-sequence54-integration",
    "cex-v12-native-minimal",
)
ACTIVE_STATES = {"queued", "in_progress", "waiting", "pending", "requested"}
ACCEPTED_JOB_CONCLUSIONS = {"success", "neutral", "skipped"}


class OrchestratorError(RuntimeError):
    pass


class GitHub:
    def __init__(self, repository: str, token: str, api_url: str) -> None:
        self.repository = repository
        self.token = token
        self.api_url = api_url.rstrip("/")

    def call(
        self,
        path: str,
        *,
        method: str = "GET",
        payload: dict[str, Any] | None = None,
        allow_error: bool = False,
    ) -> tuple[int, Any]:
        data = None if payload is None else json.dumps(payload).encode("utf-8")
        request = Request(
            self.api_url + path,
            method=method,
            data=data,
            headers={
                "Authorization": f"Bearer {self.token}",
                "Accept": "application/vnd.github+json",
                "X-GitHub-Api-Version": "2022-11-28",
                "User-Agent": "sequence54-parallel-gate-orchestrator",
                "Content-Type": "application/json",
            },
        )
        try:
            with urlopen(request, timeout=30) as response:
                body = response.read()
                return response.status, json.loads(body) if body else None
        except HTTPError as error:
            body = error.read().decode("utf-8", errors="replace")
            if allow_error:
                return error.code, {"error": body[:4000]}
            raise OrchestratorError(
                f"{method} {path} -> {error.code}: {body[:1200]}"
            ) from error

    def branch_sha(self, branch: str) -> str:
        _status, value = self.call(
            f"/repos/{self.repository}/branches/{quote(branch, safe='')}"
        )
        return str(value["commit"]["sha"])

    def workflow_runs(self, path: str, sha: str) -> list[dict[str, Any]]:
        encoded = quote(path, safe="")
        _status, value = self.call(
            f"/repos/{self.repository}/actions/workflows/{encoded}/runs?per_page=100"
        )
        runs = [
            run
            for run in value.get("workflow_runs", [])
            if run.get("head_sha") == sha
        ]
        runs.sort(key=lambda run: run.get("created_at", ""), reverse=True)
        return runs

    def run(self, run_id: int) -> dict[str, Any]:
        _status, value = self.call(
            f"/repos/{self.repository}/actions/runs/{run_id}"
        )
        return value

    def dispatch(self, path: str, branch: str) -> None:
        encoded = quote(path, safe="")
        status, body = self.call(
            f"/repos/{self.repository}/actions/workflows/{encoded}/dispatches",
            method="POST",
            payload={"ref": branch},
            allow_error=True,
        )
        if status not in {201, 204}:
            raise OrchestratorError(
                f"workflow dispatch failed for {path}: HTTP {status}: {body}"
            )

    def jobs(self, run_id: int) -> list[dict[str, Any]]:
        _status, value = self.call(
            f"/repos/{self.repository}/actions/runs/{run_id}/jobs?per_page=100"
        )
        return list(value.get("jobs", []))


@dataclass
class Subject:
    name: str
    path: str
    baseline_ids: set[int] = field(default_factory=set)
    run_id: int | None = None
    action: str | None = None
    attempts: int = 0
    terminal: dict[str, Any] | None = None


def workflow_map(root: Path) -> dict[str, list[str]]:
    result: dict[str, list[str]] = {}
    for path in sorted((root / ".github/workflows").glob("*.y*ml")):
        text = path.read_text(encoding="utf-8")
        match = re.search(r"(?m)^name:\s*[\"']?([^\"'\n]+)", text)
        if match:
            result.setdefault(match.group(1).strip(), []).append(
                path.relative_to(root).as_posix()
            )
    return result


def resolve_subjects(root: Path) -> list[Subject]:
    by_name = workflow_map(root)
    subjects: list[Subject] = []
    errors: list[str] = []
    for name in DESIRED_WORKFLOWS:
        paths = by_name.get(name, [])
        if len(paths) != 1:
            errors.append(f"{name}: expected one path, found {paths!r}")
        else:
            subjects.append(Subject(name=name, path=paths[0]))
    if errors:
        raise OrchestratorError("workflow resolution failed: " + "; ".join(errors))
    return subjects


def has_dispatch(root: Path, path: str) -> bool:
    return "workflow_dispatch:" in (root / path).read_text(encoding="utf-8")


def select_existing(subject: Subject, runs: list[dict[str, Any]]) -> bool:
    success = next(
        (
            run
            for run in runs
            if run.get("status") == "completed"
            and run.get("conclusion") == "success"
        ),
        None,
    )
    if success is not None:
        subject.run_id = int(success["id"])
        subject.action = "reuse_exact_success"
        subject.terminal = success
        return True
    active = next(
        (run for run in runs if run.get("status") in ACTIVE_STATES),
        None,
    )
    if active is not None:
        subject.run_id = int(active["id"])
        subject.action = "adopt_exact_active"
        subject.attempts = 1
        return True
    return False


def dispatch_missing(
    github: GitHub,
    root: Path,
    branch: str,
    sha: str,
    subjects: list[Subject],
) -> None:
    for subject in subjects:
        runs = github.workflow_runs(subject.path, sha)
        subject.baseline_ids = {int(run["id"]) for run in runs}
        if select_existing(subject, runs):
            continue
        if not has_dispatch(root, subject.path):
            raise OrchestratorError(
                f"{subject.name} has no exact success/active run and no workflow_dispatch"
            )
        github.dispatch(subject.path, branch)
        subject.action = "dispatch_requested"
        subject.attempts = 1


def bind_new_run_ids(
    github: GitHub,
    branch: str,
    sha: str,
    subjects: list[Subject],
    deadline: float,
) -> None:
    while time.time() < deadline:
        if github.branch_sha(branch) != sha:
            raise OrchestratorError("candidate branch moved while binding run IDs")
        pending = []
        for subject in subjects:
            if subject.run_id is not None:
                continue
            new_runs = [
                run
                for run in github.workflow_runs(subject.path, sha)
                if int(run["id"]) not in subject.baseline_ids
            ]
            if new_runs:
                subject.run_id = int(new_runs[0]["id"])
                subject.action = "dispatch_new_exact_run"
            else:
                pending.append(subject.name)
        if not pending:
            return
        time.sleep(10)
    raise OrchestratorError(
        "new run identity did not appear for: "
        + ", ".join(subject.name for subject in subjects if subject.run_id is None)
    )


def wait_all(
    github: GitHub,
    root: Path,
    branch: str,
    sha: str,
    subjects: list[Subject],
    deadline: float,
) -> None:
    while time.time() < deadline:
        if github.branch_sha(branch) != sha:
            raise OrchestratorError("candidate branch moved during qualification")
        pending = []
        for subject in subjects:
            if subject.terminal is not None:
                continue
            if subject.run_id is None:
                raise OrchestratorError(f"{subject.name} lacks a bound run ID")
            run = github.run(subject.run_id)
            if run.get("status") != "completed":
                pending.append(subject.name)
                continue
            if run.get("conclusion") == "cancelled" and subject.attempts < 3:
                if not has_dispatch(root, subject.path):
                    subject.terminal = run
                    continue
                baseline = github.workflow_runs(subject.path, sha)
                subject.baseline_ids = {int(value["id"]) for value in baseline}
                subject.run_id = None
                subject.terminal = None
                subject.attempts += 1
                github.dispatch(subject.path, branch)
                subject.action = f"redispatch_after_cancel_{subject.attempts}"
                pending.append(subject.name)
                continue
            subject.terminal = run
        unbound = [subject for subject in subjects if subject.run_id is None]
        if unbound:
            bind_new_run_ids(
                github,
                branch,
                sha,
                unbound,
                min(deadline, time.time() + 1200),
            )
            pending.extend(subject.name for subject in unbound)
        if not pending and all(subject.terminal is not None for subject in subjects):
            return
        time.sleep(15)
    raise OrchestratorError("timed out waiting for exact-head workflow families")


def job_summary(job: dict[str, Any]) -> dict[str, Any]:
    steps = [
        {
            "name": step.get("name"),
            "status": step.get("status"),
            "conclusion": step.get("conclusion"),
        }
        for step in job.get("steps") or []
    ]
    return {
        "id": job.get("id"),
        "name": job.get("name"),
        "status": job.get("status"),
        "conclusion": job.get("conclusion"),
        "runner_name": job.get("runner_name"),
        "steps": steps,
    }


def build_report(
    github: GitHub,
    repository: str,
    branch: str,
    sha: str,
    tree: str,
    subjects: list[Subject],
) -> tuple[dict[str, Any], list[str]]:
    workflows: dict[str, Any] = {}
    failed: list[str] = []
    for subject in subjects:
        assert subject.run_id is not None and subject.terminal is not None
        jobs = [job_summary(job) for job in github.jobs(subject.run_id)]
        executed = [job for job in jobs if job.get("conclusion") != "skipped"]
        nonempty = bool(executed) and all(job["steps"] for job in executed)
        conclusion = subject.terminal.get("conclusion")
        workflows[subject.name] = {
            "path": subject.path,
            "run_id": subject.run_id,
            "run_attempt": subject.terminal.get("run_attempt"),
            "event": subject.terminal.get("event"),
            "status": subject.terminal.get("status"),
            "conclusion": conclusion,
            "action": subject.action,
            "dispatch_attempts": subject.attempts,
            "nonempty_executed_jobs": nonempty,
            "jobs": jobs,
        }
        if conclusion != "success" or not nonempty:
            failed.append(subject.name)
    report = {
        "schema": "cex.sequence54-parallel-exact-head-gates.v1",
        "observed_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "repository": repository,
        "branch": branch,
        "head_sha": sha,
        "tree_sha": tree,
        "workflows": workflows,
        "failed": sorted(failed),
        "all_required_gate_families_green": not failed,
        "production_authorization": "not_granted",
    }
    return report, failed


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--repository", required=True)
    parser.add_argument("--branch", required=True)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--tree", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--timeout-seconds", type=int, default=18_000)
    arguments = parser.parse_args()

    token = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
    if not token:
        raise OrchestratorError("GH_TOKEN is required")
    github = GitHub(
        arguments.repository,
        token,
        os.environ.get("GITHUB_API_URL", "https://api.github.com"),
    )
    if github.branch_sha(arguments.branch) != arguments.sha:
        raise OrchestratorError("candidate branch moved before orchestration")

    subjects = resolve_subjects(arguments.root)
    dispatch_missing(github, arguments.root, arguments.branch, arguments.sha, subjects)
    deadline = time.time() + arguments.timeout_seconds
    bind_new_run_ids(
        github,
        arguments.branch,
        arguments.sha,
        subjects,
        min(deadline, time.time() + 1200),
    )
    wait_all(
        github,
        arguments.root,
        arguments.branch,
        arguments.sha,
        subjects,
        deadline,
    )
    report, failed = build_report(
        github,
        arguments.repository,
        arguments.branch,
        arguments.sha,
        arguments.tree,
        subjects,
    )
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    if failed:
        print("exact-head workflow failures: " + ", ".join(failed), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, OrchestratorError) as error:
        print(f"Sequence 54 parallel orchestrator failed: {error}", file=sys.stderr)
        raise SystemExit(1)
