#!/usr/bin/env python3
"""Safely exercise GitHub repository-rule rejection on a disposable branch."""

from __future__ import annotations

import argparse
import json
import os
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_POLICY = ROOT / "docs/governance/main-ruleset-v1.json"
API_VERSION = "2022-11-28"


class ProbeError(RuntimeError):
    pass


def load_policy(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ProbeError(f"cannot read policy {path}: {error}") from error
    if not isinstance(value, dict):
        raise ProbeError("ruleset policy root must be an object")
    if value.get("schema") != "cex.github-main-ruleset.v1":
        raise ProbeError("ruleset policy schema drift")
    if value.get("repository") != "TrillionniumFoundation/CEX":
        raise ProbeError("ruleset repository drift")
    if value.get("target_branch") != "main":
        raise ProbeError("ruleset target branch drift")
    if value.get("bypass_actors") != []:
        raise ProbeError("negative probe requires an empty bypass actor list")
    if value.get("production_authorization") != "not_granted":
        raise ProbeError("negative probe policy cannot grant production authorization")
    return value


def request_json(
    method: str,
    url: str,
    token: str,
    payload: dict[str, Any] | None = None,
) -> tuple[int, Any]:
    body = None if payload is None else json.dumps(payload).encode("utf-8")
    request = urllib.request.Request(
        url,
        data=body,
        method=method,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json",
            "User-Agent": "cex-ruleset-negative-probe",
            "X-GitHub-Api-Version": API_VERSION,
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            raw = response.read()
            return int(response.status), json.loads(raw) if raw else None
    except urllib.error.HTTPError as error:
        raw = error.read().decode("utf-8", errors="replace")
        try:
            value: Any = json.loads(raw)
        except json.JSONDecodeError:
            value = {"message": raw}
        return int(error.code), value


def response_summary(status: int, value: Any) -> dict[str, Any]:
    if isinstance(value, dict):
        message = value.get("message")
        errors = value.get("errors")
    else:
        message = str(value) if value is not None else None
        errors = None
    return {"http_status": status, "message": message, "errors": errors}


def rejection_is_ruleset(status: int, value: Any) -> bool:
    if status not in {403, 409, 422}:
        return False
    rendered = json.dumps(value, sort_keys=True).lower()
    return any(
        marker in rendered
        for marker in (
            "repository rule",
            "rule violations",
            "protected branch",
            "pull request",
            "cannot delete",
            "non-fast-forward",
            "non fast forward",
        )
    )


def probe_ruleset_payload(policy: dict[str, Any], probe_ref: str, name: str) -> dict[str, Any]:
    pull = policy["pull_request"]
    rules: list[dict[str, Any]] = [
        {"type": "deletion"},
        {"type": "non_fast_forward"},
        {"type": "required_signatures"},
        {
            "type": "pull_request",
            "parameters": {
                "required_approving_review_count": pull["required_approving_review_count"],
                "dismiss_stale_reviews_on_push": pull["dismiss_stale_reviews_on_push"],
                "require_code_owner_review": pull["require_code_owner_review"],
                "require_last_push_approval": pull["require_last_push_approval"],
                "required_review_thread_resolution": pull[
                    "required_review_thread_resolution"
                ],
                "allowed_merge_methods": pull["allowed_merge_methods"],
            },
        },
    ]
    return {
        "name": name,
        "target": "branch",
        "enforcement": "active",
        "bypass_actors": [],
        "conditions": {"ref_name": {"include": [probe_ref], "exclude": []}},
        "rules": rules,
    }


def get_ref_sha(base: str, ref_path: str, token: str) -> str:
    status, value = request_json("GET", f"{base}/git/ref/{ref_path}", token)
    if status != 200 or not isinstance(value, dict):
        raise ProbeError(f"cannot read ref {ref_path}: HTTP {status}")
    sha = value.get("object", {}).get("sha")
    if not isinstance(sha, str) or len(sha) != 40:
        raise ProbeError(f"ref {ref_path} lacks an exact commit SHA")
    return sha


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--policy", type=Path, default=DEFAULT_POLICY)
    parser.add_argument("--token-env", default="GITHUB_ADMIN_TOKEN")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()

    probe_ruleset_id: int | None = None
    probe_ref_created = False
    result: dict[str, Any]
    try:
        policy = load_policy(args.policy)
        token = os.environ.get(args.token_env, "").strip()
        if not token:
            raise ProbeError(f"{args.token_env} is required")
        repository = policy["repository"]
        base = f"https://api.github.com/repos/{repository}"
        main_ref_path = "heads/main"
        main_before = get_ref_sha(base, main_ref_path, token)
        run_token = f"{int(time.time())}-{uuid.uuid4().hex[:10]}"
        probe_branch = f"ruleset-negative-probe-{run_token}"
        probe_full_ref = f"refs/heads/{probe_branch}"
        probe_ref_path = f"heads/{probe_branch}"
        probe_ruleset_name = f"CEX disposable ruleset probe {run_token}"

        create_ref_status, create_ref = request_json(
            "POST",
            f"{base}/git/refs",
            token,
            {"ref": probe_full_ref, "sha": main_before},
        )
        if create_ref_status != 201:
            raise ProbeError(
                f"cannot create disposable probe ref: HTTP {create_ref_status}: {create_ref}"
            )
        probe_ref_created = True

        create_ruleset_status, create_ruleset = request_json(
            "POST",
            f"{base}/rulesets",
            token,
            probe_ruleset_payload(policy, probe_full_ref, probe_ruleset_name),
        )
        if create_ruleset_status != 201 or not isinstance(create_ruleset, dict):
            raise ProbeError(
                f"cannot create disposable probe ruleset: HTTP {create_ruleset_status}: {create_ruleset}"
            )
        raw_ruleset_id = create_ruleset.get("id")
        if not isinstance(raw_ruleset_id, int):
            raise ProbeError("disposable ruleset response lacks integer id")
        probe_ruleset_id = raw_ruleset_id

        commit_status, commit_value = request_json(
            "GET", f"{base}/git/commits/{main_before}", token
        )
        if commit_status != 200 or not isinstance(commit_value, dict):
            raise ProbeError(f"cannot read main commit: HTTP {commit_status}")
        tree_sha = commit_value.get("tree", {}).get("sha")
        parents = commit_value.get("parents")
        if not isinstance(tree_sha, str) or len(tree_sha) != 40:
            raise ProbeError("main commit lacks exact tree SHA")
        if not isinstance(parents, list) or not parents:
            raise ProbeError("main commit lacks parent for force-update probe")
        main_parent = parents[0].get("sha") if isinstance(parents[0], dict) else None
        if not isinstance(main_parent, str) or len(main_parent) != 40:
            raise ProbeError("main parent is not an exact commit SHA")

        child_status, child_value = request_json(
            "POST",
            f"{base}/git/commits",
            token,
            {
                "message": "Disposable CEX ruleset direct-update probe",
                "tree": tree_sha,
                "parents": [main_before],
            },
        )
        if child_status != 201 or not isinstance(child_value, dict):
            raise ProbeError(f"cannot create disposable child commit: HTTP {child_status}")
        child_sha = child_value.get("sha")
        if not isinstance(child_sha, str) or len(child_sha) != 40:
            raise ProbeError("disposable child commit lacks exact SHA")

        sibling_status, sibling_value = request_json(
            "POST",
            f"{base}/git/commits",
            token,
            {
                "message": "Disposable CEX ruleset force-update probe",
                "tree": tree_sha,
                "parents": [main_parent],
            },
        )
        if sibling_status != 201 or not isinstance(sibling_value, dict):
            raise ProbeError(f"cannot create disposable sibling commit: HTTP {sibling_status}")
        sibling_sha = sibling_value.get("sha")
        if not isinstance(sibling_sha, str) or len(sibling_sha) != 40:
            raise ProbeError("disposable sibling commit lacks exact SHA")

        direct_status, direct_value = request_json(
            "PATCH",
            f"{base}/git/refs/{probe_ref_path}",
            token,
            {"sha": child_sha, "force": False},
        )
        force_status, force_value = request_json(
            "PATCH",
            f"{base}/git/refs/{probe_ref_path}",
            token,
            {"sha": sibling_sha, "force": True},
        )
        delete_status, delete_value = request_json(
            "DELETE", f"{base}/git/refs/{probe_ref_path}", token
        )

        probes = {
            "direct_update": response_summary(direct_status, direct_value),
            "force_update": response_summary(force_status, force_value),
            "deletion": response_summary(delete_status, delete_value),
        }
        rejected = {
            "direct_update": rejection_is_ruleset(direct_status, direct_value),
            "force_update": rejection_is_ruleset(force_status, force_value),
            "deletion": rejection_is_ruleset(delete_status, delete_value),
        }
        if not all(rejected.values()):
            raise ProbeError(
                "one or more disposable ref mutations were not rejected by repository rules: "
                + json.dumps({"probes": probes, "rejected": rejected}, sort_keys=True)
            )

        main_after = get_ref_sha(base, main_ref_path, token)
        if main_after != main_before:
            raise ProbeError(
                f"main changed during disposable probe: before={main_before} after={main_after}"
            )

        result = {
            "schema": "cex.github-main-ruleset-negative-probe.v1",
            "status": "verified",
            "repository": repository,
            "main_before": main_before,
            "main_after": main_after,
            "main_unchanged": True,
            "disposable_probe_ref": probe_full_ref,
            "disposable_ruleset_id": probe_ruleset_id,
            "probes": probes,
            "rejected_by_repository_rules": rejected,
            "production_authorization": "not_granted",
        }
    except ProbeError as error:
        result = {
            "schema": "cex.github-main-ruleset-negative-probe.v1",
            "status": "blocked",
            "problems": [str(error)],
            "production_authorization": "not_granted",
        }
    finally:
        token = os.environ.get(args.token_env, "").strip()
        policy_value: dict[str, Any] | None = None
        try:
            policy_value = load_policy(args.policy)
        except ProbeError:
            pass
        if token and policy_value:
            base = f"https://api.github.com/repos/{policy_value['repository']}"
            if probe_ruleset_id is not None:
                request_json("DELETE", f"{base}/rulesets/{probe_ruleset_id}", token)
            if probe_ref_created and 'probe_ref_path' in locals():
                request_json("DELETE", f"{base}/git/refs/{probe_ref_path}", token)
            try:
                observed_main = get_ref_sha(base, "heads/main", token)
                result["main_after_cleanup"] = observed_main
                if "main_before" in locals():
                    result["main_unchanged_after_cleanup"] = observed_main == main_before
                    if observed_main != main_before:
                        result["status"] = "blocked"
                        result.setdefault("problems", []).append(
                            "main identity changed during or after cleanup"
                        )
            except ProbeError as error:
                result["status"] = "blocked"
                result.setdefault("problems", []).append(str(error))

    rendered = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    return 0 if result.get("status") == "verified" else 1


if __name__ == "__main__":
    raise SystemExit(main())
