#!/usr/bin/env python3
"""Record GitHub branch/ruleset enforcement without claiming unavailable controls."""

from __future__ import annotations

import argparse
import fnmatch
import json
import os
import re
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

_SCRIPT_DIR = Path(__file__).resolve().parent
if str(_SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPT_DIR))
from evidence_safe_io import SafeIOError, write_json_nofollow  # noqa: E402

DESIRED_CHECKS = [
    "fresh-postgres-migrations",
    "repository-integrity",
    "service-local-gate-linux",
    "service-local-gate-windows",
    "hepta-postgres-integration",
    "gateway-exact-reserve",
    "execution-settlement",
    "provider-reconciliation",
    "repository-candidate-qualification",
]
GIT_SHA_RE = re.compile(r"^[0-9a-f]{40}$")
BRANCH_RE = re.compile(r"^[A-Za-z0-9._/-]+$")


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def request_json(url: str, token: str) -> tuple[int, Any]:
    request = urllib.request.Request(
        url,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "User-Agent": "cex-repository-governance-observer",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return int(response.status), json.load(response)
    except urllib.error.HTTPError as error:
        try:
            payload: Any = json.loads(error.read().decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError):
            payload = {"message": str(error)}
        return int(error.code), payload


def request_rulesets(base: str, token: str) -> tuple[int, Any]:
    """Read the complete ruleset collection, failing closed on bad pages.

    The GitHub endpoint is paginated even for small repositories.  A single
    ``per_page=100`` request can therefore miss an applicable ruleset when an
    organisation grows beyond that limit.  Keep the first request's shape
    stable for lightweight API doubles, then fetch subsequent pages until a
    short page is observed.  IDs are deduplicated because a ruleset can move
    between pages while the collection is being read.
    """

    endpoint = base.split("?", 1)[0]
    page = 1
    rulesets_by_id: dict[int, dict[str, Any]] = {}
    rulesets_without_id: list[dict[str, Any]] = []
    while page <= 1000:
        if page == 1:
            url = f"{endpoint}?per_page=100"
        else:
            url = f"{endpoint}?per_page=100&page={page}"
        status, payload = request_json(url, token)
        if status != 200 or not isinstance(payload, list):
            return status, payload
        for ruleset in payload:
            if not isinstance(ruleset, dict):
                continue
            try:
                ruleset_id = int(ruleset.get("id"))
            except (TypeError, ValueError):
                rulesets_without_id.append(ruleset)
            else:
                rulesets_by_id[ruleset_id] = ruleset
        if len(payload) < 100:
            return 200, list(rulesets_by_id.values()) + rulesets_without_id
        page += 1
    return 502, {"message": "ruleset pagination exceeded 1000 pages"}


def valid_branch(value: Any) -> bool:
    """Accept the conservative branch subset used by the release contract."""

    return (
        isinstance(value, str)
        and bool(BRANCH_RE.fullmatch(value))
        and not value.startswith("/")
        and not value.endswith("/")
        and "//" not in value
        and ".." not in value
        and "@{" not in value
        and not value.startswith("refs/")
        and all(segment not in {"", ".", ".."} for segment in value.split("/"))
        and value not in {"HEAD", ".", ".."}
    )


def branch_commit_sha(payload: Any) -> str | None:
    if not isinstance(payload, dict):
        return None
    commit = payload.get("commit")
    if not isinstance(commit, dict):
        return None
    value = commit.get("sha")
    return value if isinstance(value, str) and GIT_SHA_RE.fullmatch(value) else None


def required_contexts(payload: Any) -> list[str]:
    """Extract legacy branch-protection required contexts deterministically."""

    if not isinstance(payload, dict):
        return []
    protection = payload.get("protection")
    if not isinstance(protection, dict):
        return []
    required = protection.get("required_status_checks")
    if not isinstance(required, dict):
        return []
    values: set[str] = set()
    contexts = required.get("contexts")
    if isinstance(contexts, list):
        values.update(value for value in contexts if isinstance(value, str) and value)
    # Newer branch-protection responses expose checks as structured objects;
    # accept both forms while preserving one canonical context list.
    checks = required.get("checks")
    if isinstance(checks, list):
        for check in checks:
            if isinstance(check, str) and check:
                values.add(check)
            elif isinstance(check, dict):
                context = check.get("context")
                if isinstance(context, str) and context:
                    values.add(context)
                else:
                    name = check.get("name")
                    if isinstance(name, str) and name:
                        values.add(name)
    return sorted(values)


def _normalise_ref_pattern(pattern: str) -> str:
    if pattern.startswith("refs/heads/"):
        return pattern[len("refs/heads/") :]
    return pattern


def ruleset_applies_to_branch(ruleset: Any, branch: str) -> bool:
    """Return whether a repository ruleset's ref conditions include *branch*.

    GitHub's ruleset API uses fnmatch-style ref patterns and the special
    ``~ALL`` pattern.  Unknown targets are deliberately ignored rather than
    treated as an enforcing branch ruleset.
    """

    if not isinstance(ruleset, dict):
        return False
    target = ruleset.get("target")
    if target != "branch":
        return False
    conditions = ruleset.get("conditions")
    if not isinstance(conditions, dict):
        return False
    ref_name = conditions.get("ref_name")
    if not isinstance(ref_name, dict):
        # The API normally supplies a ref_name object.  If the response is
        # incomplete or malformed, do not infer that a ruleset covers the
        # candidate branch.
        return False
    includes = ref_name.get("include")
    excludes = ref_name.get("exclude")
    if not isinstance(includes, list):
        return False
    includes = [value for value in includes if isinstance(value, str) and value]
    if not includes:
        return False
    excludes = [value for value in excludes if isinstance(value, str)] if isinstance(excludes, list) else []

    def matches(pattern: str) -> bool:
        pattern = _normalise_ref_pattern(pattern)
        if pattern in {"~ALL", "*", "**"}:
            return True
        return fnmatch.fnmatchcase(branch, pattern)

    included = any(matches(pattern) for pattern in includes)
    excluded = any(matches(pattern) for pattern in excludes)
    return included and not excluded


def ruleset_required_contexts(ruleset: Any) -> list[str]:
    """Extract required-status-check names from one ruleset."""

    if not isinstance(ruleset, dict) or not isinstance(ruleset.get("rules"), list):
        return []
    contexts: set[str] = set()
    for rule in ruleset["rules"]:
        if not isinstance(rule, dict) or rule.get("type") != "required_status_checks":
            continue
        parameters = rule.get("parameters")
        if not isinstance(parameters, dict):
            continue
        checks = parameters.get("required_status_checks")
        if not isinstance(checks, list):
            continue
        for check in checks:
            if isinstance(check, str) and check:
                contexts.add(check)
            elif isinstance(check, dict):
                context = check.get("context")
                if isinstance(context, str) and context:
                    contexts.add(context)
    return sorted(contexts)


def ruleset_bypass_state(ruleset: Any) -> str:
    """Return ``none``, ``present`` or ``unknown`` for bypass actors.

    Required checks are not universal enforcement when a ruleset grants a
    bypass actor.  If the API omits or malforms this field, the observer must
    not claim that the ruleset enforces the candidate branch.
    """

    if not isinstance(ruleset, dict) or "bypass_actors" not in ruleset:
        return "unknown"
    actors = ruleset.get("bypass_actors")
    if not isinstance(actors, list):
        return "unknown"
    return "none" if not actors else "present"


def summarise_ruleset(ruleset: dict[str, Any], branch: str) -> dict[str, Any]:
    applies = ruleset_applies_to_branch(ruleset, branch)
    enforcement = ruleset.get("enforcement")
    return {
        "id": ruleset.get("id"),
        "name": ruleset.get("name"),
        "target": ruleset.get("target"),
        "enforcement": enforcement,
        "active": enforcement == "active",
        "applies_to_candidate_branch": applies,
        "required_status_contexts": ruleset_required_contexts(ruleset),
        "bypass_state": ruleset_bypass_state(ruleset),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", required=True)
    parser.add_argument("--commit-sha", required=True)
    parser.add_argument(
        "--tree-sha",
        required=True,
        help="exact checked-out Git tree SHA to bind to the observation",
    )
    parser.add_argument(
        "--candidate-branch",
        "--branch",
        dest="candidate_branch",
        required=True,
        help="branch whose exact commit and governance controls are being observed",
    )
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    token = os.environ.get("GITHUB_TOKEN", "")
    if not token:
        raise SystemExit("GITHUB_TOKEN is required")
    if not GIT_SHA_RE.fullmatch(args.commit_sha):
        raise SystemExit("commit-sha must be a 40-character lowercase Git SHA")
    if not GIT_SHA_RE.fullmatch(args.tree_sha):
        raise SystemExit("tree-sha must be a 40-character lowercase Git SHA")
    if not valid_branch(args.candidate_branch):
        raise SystemExit("candidate-branch is not a canonical branch name")

    try:
        checkout_tree = subprocess.run(
            ["git", "rev-parse", "HEAD^{tree}"],
            cwd=Path(__file__).resolve().parents[1],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"cannot read checked-out Git tree: {error}") from error
    if checkout_tree != args.tree_sha:
        raise SystemExit(
            "checked-out tree does not match requested tree "
            f"(checkout={checkout_tree!r}, requested={args.tree_sha!r})"
        )

    base = f"https://api.github.com/repos/{args.repository}"
    repo_status, repo = request_json(base, token)
    if repo_status != 200 or not isinstance(repo, dict):
        raise SystemExit(f"cannot read repository metadata: HTTP {repo_status}")
    default_branch_name = repo.get("default_branch")
    if not isinstance(default_branch_name, str) or not default_branch_name:
        raise SystemExit("repository metadata lacks default_branch")

    candidate_branch_status, candidate_branch = request_json(
        f"{base}/branches/{urllib.parse.quote(args.candidate_branch, safe='')}", token
    )
    if candidate_branch_status != 200 or not isinstance(candidate_branch, dict):
        raise SystemExit(
            f"cannot read candidate branch metadata: HTTP {candidate_branch_status}"
        )
    candidate_branch_sha = branch_commit_sha(candidate_branch)
    if candidate_branch_sha != args.commit_sha:
        raise SystemExit(
            "candidate branch does not point to the requested commit "
            f"(branch={candidate_branch_sha!r}, requested={args.commit_sha!r})"
        )

    commit_status, commit_payload = request_json(
        f"{base}/commits/{urllib.parse.quote(args.commit_sha, safe='')}", token
    )
    commit_tree = None
    if isinstance(commit_payload, dict):
        tree = commit_payload.get("commit")
        if isinstance(tree, dict):
            tree_object = tree.get("tree")
            if isinstance(tree_object, dict):
                value = tree_object.get("sha")
                if isinstance(value, str):
                    commit_tree = value
    if commit_status != 200 or commit_tree != args.tree_sha:
        raise SystemExit(
            "commit tree does not match requested tree "
            f"(commit={commit_tree!r}, requested={args.tree_sha!r})"
        )

    if default_branch_name == args.candidate_branch:
        default_branch_payload = candidate_branch
    else:
        default_status, default_payload = request_json(
            f"{base}/branches/{urllib.parse.quote(default_branch_name, safe='')}", token
        )
        if default_status != 200 or not isinstance(default_payload, dict):
            raise SystemExit(f"cannot read default branch metadata: HTTP {default_status}")
        default_branch_payload = default_payload

    rulesets_status, rulesets = request_rulesets(f"{base}/rulesets", token)
    rulesets_readable = rulesets_status == 200 and isinstance(rulesets, list)
    if not rulesets_readable:
        rulesets = []

    actual_contexts = required_contexts(candidate_branch)
    protected = bool(candidate_branch.get("protected"))
    default_protected = bool(default_branch_payload.get("protected"))
    default_contexts = required_contexts(default_branch_payload)
    ruleset_summaries = []
    for ruleset in rulesets:
        if not isinstance(ruleset, dict):
            continue
        ruleset_summaries.append(summarise_ruleset(ruleset, args.candidate_branch))

    # Re-read the branch after all repository/ruleset observations.  A branch
    # move during this window must not leave a stale governance attestation
    # claiming that the candidate still points at the requested commit.
    final_branch_status, final_branch = request_json(
        f"{base}/branches/{urllib.parse.quote(args.candidate_branch, safe='')}", token
    )
    final_branch_sha = branch_commit_sha(final_branch)
    if final_branch_status != 200 or final_branch_sha != args.commit_sha:
        raise SystemExit(
            "candidate branch moved during governance observation "
            f"(initial={candidate_branch_sha!r}, final={final_branch_sha!r}, requested={args.commit_sha!r})"
        )

    candidate_rulesets = [
        item
        for item in ruleset_summaries
        if item["applies_to_candidate_branch"]
    ]
    legacy_checks_enforced = protected and set(DESIRED_CHECKS).issubset(actual_contexts)
    ruleset_checks_enforced = any(
        item["active"]
        and item["bypass_state"] == "none"
        and set(DESIRED_CHECKS).issubset(item["required_status_contexts"])
        for item in candidate_rulesets
    )
    required_checks_enforced = legacy_checks_enforced or ruleset_checks_enforced
    if required_checks_enforced:
        enforcement = "enforced"
    elif protected or rulesets_readable:
        enforcement = "not_enforced"
    else:
        enforcement = "unverifiable"

    result = {
        "schema": "cex.repository-governance-observation.v1",
        "ok": True,
        "repository": args.repository,
        "commit_sha": args.commit_sha,
        "tree_sha": args.tree_sha,
        "observed_at": utc_now(),
        "default_branch": default_branch_name,
        "candidate_branch": args.candidate_branch,
        "candidate_branch_commit_sha": candidate_branch_sha,
        "candidate_commit_matches_branch": candidate_branch_sha == args.commit_sha,
        "candidate_branch_commit_sha_final": final_branch_sha,
        "candidate_branch_stable_during_observation": final_branch_sha == args.commit_sha,
        "candidate_tree_matches_commit": commit_tree == args.tree_sha,
        "default_branch_protected": default_protected,
        "candidate_branch_protected": protected,
        "branch_protection_enabled": bool(
            (candidate_branch.get("protection") or {}).get("enabled")
        ),
        "candidate_branch_protection_enabled": bool(
            (candidate_branch.get("protection") or {}).get("enabled")
        ),
        "default_branch_protection_enabled": bool(
            (default_branch_payload.get("protection") or {}).get("enabled")
        ),
        "default_branch_required_status_contexts": default_contexts,
        "actual_required_status_contexts": actual_contexts,
        "candidate_required_status_contexts": actual_contexts,
        "desired_required_status_contexts": DESIRED_CHECKS,
        "candidate_legacy_required_checks_enforced": legacy_checks_enforced,
        "candidate_ruleset_required_checks_enforced": ruleset_checks_enforced,
        "required_candidate_checks_enforced": required_checks_enforced,
        "rulesets_http_status": rulesets_status,
        "rulesets_readable": rulesets_readable,
        "ruleset_count": len(ruleset_summaries),
        "rulesets": ruleset_summaries,
        "candidate_ruleset_count": len(candidate_rulesets),
        "candidate_rulesets": candidate_rulesets,
        "repository_candidate_enforcement": enforcement,
        "production_authorization": "not_granted",
        "interpretation": (
            "This is an observation of GitHub controls. Source files and CI prose do not create "
            "branch protection or ruleset enforcement."
        ),
    }
    try:
        write_json_nofollow(args.output, result)
    except SafeIOError as error:
        raise SystemExit(str(error)) from error
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
