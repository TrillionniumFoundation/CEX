#!/usr/bin/env python3
"""Repin CEX PR #34 to the exact repaired World PR #60 head.

The helper advances only active source-governance and qualification pins. It
leaves historical evidence untouched, keeps both PRs Draft, and does not merge,
deploy, create statuses, dismiss reviews, or grant production authorization.
"""

from __future__ import annotations

import base64
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

CEX_REPOSITORY = "TrillionniumFoundation/CEX"
CEX_BRANCH = "fix/hepta-p0-blocker-closure-20260906"
CEX_PR = 34
WORLD_REPOSITORY = "TrillionniumFoundation/Trillionnium-World"
WORLD_PR = 60
OLD_WORLD_HEAD = "cff9f3fd3539420c676f3d1397c166b1c1e24ffb"
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


def api(token: str, method: str, repository: str, path: str, body: Any | None = None) -> Any:
    data = None if body is None else json.dumps(body).encode("utf-8")
    request = urllib.request.Request(
        f"https://api.github.com/repos/{repository}{path}",
        data=data,
        method=method,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "cex-world-exact-head-repin",
            "Content-Type": "application/json",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            raw = response.read()
            return None if not raw else json.loads(raw)
    except urllib.error.HTTPError as error:
        raw = error.read().decode("utf-8", errors="replace")
        raise RuntimeError(
            f"GitHub API {method} {repository}{path} failed with HTTP {error.code}: {raw[:500]}"
        ) from error


def run(*args: str, cwd: pathlib.Path | None = None, allow_failure: bool = False) -> str:
    completed = subprocess.run(
        list(args),
        cwd=cwd,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env={**os.environ, "GIT_TERMINAL_PROMPT": "0"},
    )
    if completed.returncode != 0 and not allow_failure:
        raise RuntimeError(f"command failed ({completed.returncode}): {' '.join(args)}")
    return completed.stdout.strip()


def choose_world_token() -> tuple[str, str]:
    attempted: list[str] = []
    for name in TOKEN_ENV_NAMES:
        token = os.environ.get(name, "").strip()
        if not token:
            continue
        attempted.append(name)
        try:
            api(token, "GET", WORLD_REPOSITORY, "")
            api(token, "GET", WORLD_REPOSITORY, f"/pulls/{WORLD_PR}")
        except RuntimeError:
            continue
        return token, name
    raise RuntimeError(f"no credential can read World PR; candidates_present={attempted}")


def choose_cex_token(destination: pathlib.Path) -> tuple[str, str]:
    candidates = [("GITHUB_TOKEN", os.environ.get("GITHUB_TOKEN", "").strip())]
    candidates.extend((name, os.environ.get(name, "").strip()) for name in TOKEN_ENV_NAMES)
    attempted: list[str] = []
    for name, token in candidates:
        if not token:
            continue
        attempted.append(name)
        shutil.rmtree(destination, ignore_errors=True)
        url = f"https://x-access-token:{token}@github.com/{CEX_REPOSITORY}.git"
        completed = subprocess.run(
            ["git", "clone", "--quiet", "--single-branch", "--branch", CEX_BRANCH, url, str(destination)],
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env={**os.environ, "GIT_TERMINAL_PROMPT": "0"},
        )
        if completed.returncode == 0:
            run("git", "remote", "set-url", "origin", f"https://github.com/{CEX_REPOSITORY}.git", cwd=destination)
            return token, name
    raise RuntimeError(f"no credential can clone CEX candidate branch; candidates_present={attempted}")


def content(token: str, repository: str, path: str, ref: str) -> str:
    encoded_path = "/".join(urllib.parse.quote(part, safe="") for part in path.split("/"))
    encoded_ref = urllib.parse.quote(ref, safe="")
    payload = api(token, "GET", repository, f"/contents/{encoded_path}?ref={encoded_ref}")
    return base64.b64decode(payload["content"]).decode("utf-8")


def active_pin_path(path: str) -> bool:
    return (
        path.startswith(".github/workflows/")
        or path == "PROJECT_BOUNDARY.json"
        or path.startswith("docs/compatibility/")
        or path == "docs/release-evidence/p0-candidate-trigger.json"
        or (path.startswith("scripts/") and "world" in pathlib.PurePosixPath(path).name.lower())
    )


def patch_cex(repo: pathlib.Path, world_head: str) -> tuple[list[str], list[str]]:
    matches = run("git", "grep", "-Il", OLD_WORLD_HEAD, "--", ".", cwd=repo, allow_failure=True).splitlines()
    changed: list[str] = []
    historical: list[str] = []
    for raw in matches:
        path = raw.removeprefix("./")
        if not active_pin_path(path):
            historical.append(path)
            continue
        file_path = repo / path
        before = file_path.read_text(encoding="utf-8")
        after = before.replace(OLD_WORLD_HEAD, world_head)
        if after != before:
            file_path.write_text(after, encoding="utf-8")
            changed.append(path)

    trigger_path = repo / "docs/release-evidence/p0-candidate-trigger.json"
    trigger = json.loads(trigger_path.read_text(encoding="utf-8"))
    if trigger.get("production_authorization") != "not_granted":
        raise RuntimeError("CEX candidate trigger attempted to self-grant production authorization")
    trigger["sequence"] = max(17, int(trigger.get("sequence", 0)) + 1)
    trigger["purpose"] = (
        "Qualify one exact v12 CEX tree against the repaired exact World source after closing the "
        "single-writer index lookalike bypass with PostgreSQL catalog-semantic validation and hostile "
        "same-predicate index fixtures. All source, prospective-merge, supply-chain, World-native, "
        "governance, runtime and human gates remain fail closed; no prior-head result is credited."
    )
    trigger["qualification_scope"] = (
        "v12-exact-cex-world-pin-plus-single-writer-index-catalog-semantics-"
        "same-predicate-hostile-fixtures-and-complete-existing-p0-closure"
    )
    trigger_text = json.dumps(trigger, indent=2, ensure_ascii=False) + "\n"
    if trigger_path.read_text(encoding="utf-8") != trigger_text:
        trigger_path.write_text(trigger_text, encoding="utf-8")
        if trigger_path.relative_to(repo).as_posix() not in changed:
            changed.append(trigger_path.relative_to(repo).as_posix())

    if world_head != OLD_WORLD_HEAD and not any(OLD_WORLD_HEAD in (repo / path).read_text(encoding="utf-8") for path in changed):
        pass
    for path in changed:
        if path.endswith(".json"):
            json.loads((repo / path).read_text(encoding="utf-8"))
    return sorted(set(changed)), sorted(set(historical))


def main() -> int:
    result_path = pathlib.Path(os.environ.get("RESULT_PATH", "/tmp/cex-world-repin-result.json"))
    result: dict[str, Any] = {
        "schema": "cex.world.exact-head-repin.v1",
        "cex_repository": CEX_REPOSITORY,
        "world_repository": WORLD_REPOSITORY,
        "production_authorization": "not_granted",
    }
    world_token, world_token_name = choose_world_token()
    result["world_credential_candidate"] = world_token_name
    world_pr = api(world_token, "GET", WORLD_REPOSITORY, f"/pulls/{WORLD_PR}")
    world_head = world_pr["head"]["sha"]
    world_commit = api(world_token, "GET", WORLD_REPOSITORY, f"/git/commits/{world_head}")
    world_tree = world_commit["tree"]["sha"]
    result["world_head"] = world_head
    result["world_tree"] = world_tree

    installer = content(world_token, WORLD_REPOSITORY, "scripts/apply-trnm-world-authority-cutover-v1.sh", world_head)
    checker = content(
        world_token,
        WORLD_REPOSITORY,
        "scripts/check-trnm-world-authority-postgres-installation.sh",
        world_head,
    )
    if "trnm_world_single_active_writer_index_semantic_drift" not in installer:
        raise RuntimeError("World repaired installer is not present on the live PR head")
    for marker in ("nonunique-same-predicate", "unique-wrong-key-same-predicate"):
        if marker not in checker:
            raise RuntimeError(f"World repaired hostile matrix is incomplete: {marker}")

    with tempfile.TemporaryDirectory(prefix="cex-world-repin-") as temporary:
        repo = pathlib.Path(temporary) / "cex"
        cex_token, cex_token_name = choose_cex_token(repo)
        result["cex_credential_candidate"] = cex_token_name
        before_sha = run("git", "rev-parse", "HEAD", cwd=repo)
        result["cex_before_sha"] = before_sha
        changed, historical = patch_cex(repo, world_head)
        result["changed_files"] = changed
        result["historical_old_pin_references_preserved"] = historical
        run("git", "diff", "--check", cwd=repo)

        if changed:
            run("git", "config", "user.name", "Trillionnium bounded maintenance", cwd=repo)
            run("git", "config", "user.email", "maintenance@trillionnium.invalid", cwd=repo)
            run("git", "add", *changed, cwd=repo)
            run("git", "commit", "-m", "chore(world): repin repaired exact authority head", cwd=repo)
            authenticated = f"https://x-access-token:{cex_token}@github.com/{CEX_REPOSITORY}.git"
            run("git", "remote", "set-url", "origin", authenticated, cwd=repo)
            try:
                run("git", "push", "origin", f"HEAD:{CEX_BRANCH}", cwd=repo)
            finally:
                run("git", "remote", "set-url", "origin", f"https://github.com/{CEX_REPOSITORY}.git", cwd=repo)

        cex_head = run("git", "rev-parse", "HEAD", cwd=repo)
        cex_tree = run("git", "rev-parse", "HEAD^{tree}", cwd=repo)
        result["cex_head"] = cex_head
        result["cex_tree"] = cex_tree

    cex_pr = api(cex_token, "GET", CEX_REPOSITORY, f"/pulls/{CEX_PR}")
    if cex_pr["head"]["sha"] != result["cex_head"]:
        raise RuntimeError("CEX PR head mismatch after exact World repin")
    body = f"""## Scope

This Draft candidate closes repository-actionable v12 P0 gaps while preserving `production_authorization=not_granted`.

## Exact current identities

- CEX source head: `{result['cex_head']}`
- CEX source tree: `{result['cex_tree']}`
- CEX base: `main@{cex_pr['base']['sha']}`
- World PR #60 source head: `{world_head}`
- World source tree: `{world_tree}`
- prospective-merge identities are computed and executed by truth-bound workflows; no prior-head result is credited.

## Current increment

CEX is repinned to the repaired World head whose supported PostgreSQL installer binds the single-writer index by catalog semantics and whose hostile matrix rejects non-unique and wrong-key same-predicate lookalikes. The existing module, capability, exact-money, migration, recovery, source-governance, advisory and release-closure controls remain active.

## Admission boundary

Keep Draft and unmerged until every applicable exact-source and prospective-merge workflow is non-empty terminal success, World-native protected contexts exist, fresh eligible reviews bind the unchanged final heads, CEX `main` governance is enforced server-side, and real deployment/data/IAM/no-dual-writer/rollback/go-live records are complete. A source commit, checked-in policy or administrator role does not manufacture those facts.

`production_authorization=not_granted`.
"""
    api(cex_token, "PATCH", CEX_REPOSITORY, f"/pulls/{CEX_PR}", {"body": body, "draft": True})
    try:
        api(
            cex_token,
            "POST",
            CEX_REPOSITORY,
            f"/pulls/{CEX_PR}/requested_reviewers",
            {"reviewers": ["Franksudoman", "Tomasrgbsf"]},
        )
        result["reviewers_requested"] = True
    except RuntimeError as error:
        result["reviewers_requested"] = False
        result["review_request_warning"] = str(error)

    encoded_sha = urllib.parse.quote(result["cex_head"], safe="")
    observed_runs: list[dict[str, Any]] = []
    for _ in range(12):
        payload = api(cex_token, "GET", CEX_REPOSITORY, f"/actions/runs?head_sha={encoded_sha}&per_page=100")
        observed_runs = payload.get("workflow_runs", [])
        if observed_runs:
            break
        time.sleep(5)
    result["cex_runs_created"] = [
        {
            "id": item.get("id"),
            "name": item.get("name"),
            "event": item.get("event"),
            "status": item.get("status"),
            "conclusion": item.get("conclusion"),
        }
        for item in observed_runs
    ]
    result["cex_run_count"] = len(observed_runs)
    result["status"] = "cex_repin_verified" if observed_runs else "cex_repin_verified_scheduler_external_blocker"
    result_path.parent.mkdir(parents=True, exist_ok=True)
    result_path.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2, sort_keys=True))
    if not observed_runs:
        raise RuntimeError("CEX exact repin head has zero workflow runs")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
