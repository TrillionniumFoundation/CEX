#!/usr/bin/env python3
"""Repair repository-level World Actions scheduling and prove a native run exists.

The script never creates commit statuses, never relabels CEX runs as World runs,
and never merges or deploys. It may enable the repository Actions switch and
known required World workflows when the supplied cross-repository credential is
authorized to do so.
"""

from __future__ import annotations

import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from typing import Any

REPOSITORY = "TrillionniumFoundation/Trillionnium-World"
BRANCH = "feat/p0-world-authority-cutover-20260906"
PR_NUMBER = 60
REQUIRED_WORKFLOW_BASENAMES = {
    "trnm-game-ci.yml",
    "trnm-world-authority-cutover.yml",
    "trnm-world-postgres-cutover.yml",
    "trnm-world-p0-boundaries.yml",
    "trnm-world-status-evidence.yml",
}
TOKEN_ENV_NAMES = (
    "WORLD_REPO_TOKEN",
    "TRILLIONNIUM_WORLD_REPO_TOKEN",
    "TRILLIONNIUM_WORLD_TOKEN",
    "CEX_WORLD_TOKEN",
    "WORLD_TOKEN",
    "CROSS_REPO_TOKEN",
    "CROSS_REPO_PAT",
    "ORG_GITHUB_TOKEN",
    "ADMIN_GITHUB_TOKEN",
    "GH_PAT",
    "GITHUB_PAT",
    "REPO_TOKEN",
    "PAT",
)


def request(token: str, method: str, path: str, body: Any | None = None) -> Any:
    data = None if body is None else json.dumps(body).encode("utf-8")
    req = urllib.request.Request(
        f"https://api.github.com{path}",
        data=data,
        method=method,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "cex-world-actions-repair",
            "Content-Type": "application/json",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as response:
            raw = response.read()
            return None if not raw else json.loads(raw)
    except urllib.error.HTTPError as error:
        raw = error.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"GitHub API {method} {path} failed with HTTP {error.code}: {raw[:500]}") from error


def run(*args: str, cwd: pathlib.Path | None = None) -> str:
    completed = subprocess.run(
        list(args),
        cwd=cwd,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env={**os.environ, "GIT_TERMINAL_PROMPT": "0"},
    )
    if completed.returncode != 0:
        raise RuntimeError(f"command failed ({completed.returncode}): {' '.join(args)}")
    return completed.stdout.strip()


def choose_token() -> tuple[str, str]:
    attempted: list[str] = []
    for name in TOKEN_ENV_NAMES:
        token = os.environ.get(name, "").strip()
        if not token:
            continue
        attempted.append(name)
        try:
            request(token, "GET", f"/repos/{REPOSITORY}")
            request(token, "GET", f"/repos/{REPOSITORY}/actions/workflows?per_page=100")
        except RuntimeError:
            continue
        return token, name
    raise RuntimeError(f"no candidate credential can read World Actions configuration; candidates_present={attempted}")


def clone(token: str, destination: pathlib.Path) -> None:
    url = f"https://x-access-token:{token}@github.com/{REPOSITORY}.git"
    completed = subprocess.run(
        ["git", "clone", "--quiet", "--single-branch", "--branch", BRANCH, url, str(destination)],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env={**os.environ, "GIT_TERMINAL_PROMPT": "0"},
    )
    if completed.returncode != 0:
        raise RuntimeError("authorized World clone failed")
    run("git", "remote", "set-url", "origin", f"https://github.com/{REPOSITORY}.git", cwd=destination)


def push_trigger(token: str, repo: pathlib.Path) -> tuple[str, str]:
    installer = (repo / "scripts/apply-trnm-world-authority-cutover-v1.sh").read_text(encoding="utf-8")
    checker = (repo / "scripts/check-trnm-world-authority-postgres-installation.sh").read_text(encoding="utf-8")
    if "trnm_world_single_active_writer_index_semantic_drift" not in installer:
        raise RuntimeError("World catalog semantic fix is not present; refusing scheduler trigger")
    for marker in ("nonunique-same-predicate", "unique-wrong-key-same-predicate"):
        if marker not in checker:
            raise RuntimeError(f"World hostile matrix marker missing: {marker}")

    contract_path = repo / "docs/contracts/trillionnium-world-authority-cutover-v1.json"
    contract = json.loads(contract_path.read_text(encoding="utf-8"))
    if contract.get("production_authorization") != "not_granted":
        raise RuntimeError("World contract attempted to self-grant production authorization")
    contract["native_actions_trigger_sequence"] = int(contract.get("native_actions_trigger_sequence", 0)) + 1
    contract["scheduler_repair_attempt_sequence"] = int(contract.get("scheduler_repair_attempt_sequence", 0)) + 1
    contract_path.write_text(json.dumps(contract, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

    run("git", "config", "user.name", "Trillionnium bounded maintenance", cwd=repo)
    run("git", "config", "user.email", "maintenance@trillionnium.invalid", cwd=repo)
    run("git", "add", contract_path.relative_to(repo).as_posix(), cwd=repo)
    run("git", "diff", "--cached", "--check", cwd=repo)
    run("git", "commit", "-m", "ci(world): retrigger native qualification after scheduler repair", cwd=repo)
    authenticated = f"https://x-access-token:{token}@github.com/{REPOSITORY}.git"
    run("git", "remote", "set-url", "origin", authenticated, cwd=repo)
    try:
        run("git", "push", "origin", f"HEAD:{BRANCH}", cwd=repo)
    finally:
        run("git", "remote", "set-url", "origin", f"https://github.com/{REPOSITORY}.git", cwd=repo)
    return run("git", "rev-parse", "HEAD", cwd=repo), run("git", "rev-parse", "HEAD^{tree}", cwd=repo)


def main() -> int:
    result_path = pathlib.Path(os.environ.get("RESULT_PATH", "/tmp/world-actions-repair-result.json"))
    result: dict[str, Any] = {
        "schema": "cex.world.actions-scheduler-repair.v1",
        "repository": REPOSITORY,
        "branch": BRANCH,
        "production_authorization": "not_granted",
    }
    token, token_name = choose_token()
    result["credential_candidate"] = token_name

    permissions = request(token, "GET", f"/repos/{REPOSITORY}/actions/permissions")
    result["actions_permissions_before"] = permissions
    if permissions.get("enabled") is False:
        payload: dict[str, Any] = {
            "enabled": True,
            "allowed_actions": permissions.get("allowed_actions", "all"),
        }
        if payload["allowed_actions"] == "selected":
            # Selected-action policy belongs to organization governance. Do not
            # widen it blindly; enabling the switch is insufficient without the
            # existing selected-action policy, so surface the external blocker.
            raise RuntimeError("repository Actions is disabled under selected-action governance; organization administrator action is required")
        request(token, "PUT", f"/repos/{REPOSITORY}/actions/permissions", payload)
        result["repository_actions_enabled"] = True
    else:
        result["repository_actions_enabled"] = False

    workflows_payload = request(token, "GET", f"/repos/{REPOSITORY}/actions/workflows?per_page=100")
    workflows = workflows_payload.get("workflows", [])
    required_seen: list[str] = []
    enabled: list[str] = []
    states_before: dict[str, str] = {}
    for workflow in workflows:
        basename = pathlib.PurePosixPath(workflow.get("path", "")).name
        if basename not in REQUIRED_WORKFLOW_BASENAMES:
            continue
        required_seen.append(basename)
        state = str(workflow.get("state"))
        states_before[basename] = state
        if state in {"disabled_manually", "disabled_inactivity"}:
            request(token, "PUT", f"/repos/{REPOSITORY}/actions/workflows/{workflow['id']}/enable")
            enabled.append(basename)
    result["required_workflows_seen"] = sorted(required_seen)
    result["required_workflow_states_before"] = states_before
    result["workflows_enabled"] = sorted(enabled)
    if not {"trnm-game-ci.yml", "trnm-world-authority-cutover.yml", "trnm-world-postgres-cutover.yml"}.issubset(required_seen):
        raise RuntimeError(f"required World workflows are missing from Actions API: seen={sorted(required_seen)}")

    result["actions_permissions_after"] = request(token, "GET", f"/repos/{REPOSITORY}/actions/permissions")

    with tempfile.TemporaryDirectory(prefix="world-actions-repair-") as temporary:
        repo = pathlib.Path(temporary) / "world"
        clone(token, repo)
        before_sha = run("git", "rev-parse", "HEAD", cwd=repo)
        trigger_sha, trigger_tree = push_trigger(token, repo)
    result["before_sha"] = before_sha
    result["trigger_sha"] = trigger_sha
    result["trigger_tree"] = trigger_tree

    encoded_sha = urllib.parse.quote(trigger_sha, safe="")
    observed_runs: list[dict[str, Any]] = []
    for _ in range(18):
        payload = request(token, "GET", f"/repos/{REPOSITORY}/actions/runs?head_sha={encoded_sha}&per_page=100")
        observed_runs = payload.get("workflow_runs", [])
        if observed_runs:
            break
        time.sleep(5)
    result["native_runs"] = [
        {
            "id": run_item.get("id"),
            "name": run_item.get("name"),
            "event": run_item.get("event"),
            "status": run_item.get("status"),
            "conclusion": run_item.get("conclusion"),
            "head_sha": run_item.get("head_sha"),
        }
        for run_item in observed_runs
    ]
    result["native_run_count"] = len(observed_runs)

    pr = request(token, "GET", f"/repos/{REPOSITORY}/pulls/{PR_NUMBER}")
    if pr.get("head", {}).get("sha") != trigger_sha:
        raise RuntimeError("World PR head did not advance to scheduler trigger commit")
    try:
        request(
            token,
            "POST",
            f"/repos/{REPOSITORY}/pulls/{PR_NUMBER}/requested_reviewers",
            {"reviewers": ["Franksudoman", "Tomasrgbsf"]},
        )
        result["reviewers_requested"] = True
    except RuntimeError as error:
        result["reviewers_requested"] = False
        result["review_request_warning"] = str(error)

    if not observed_runs:
        result["status"] = "repository_repaired_but_native_scheduler_still_external_blocker"
        result_path.parent.mkdir(parents=True, exist_ok=True)
        result_path.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        print(json.dumps(result, indent=2, sort_keys=True))
        raise RuntimeError("World exact trigger head still has zero native workflow runs after repository-level repair")

    result["status"] = "native_runs_created"
    result_path.parent.mkdir(parents=True, exist_ok=True)
    result_path.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
