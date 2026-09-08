#!/usr/bin/env python3
"""Idempotently create or update the live CEX main-branch repository ruleset."""

from __future__ import annotations

import argparse
import json
import os
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_POLICY = ROOT / "docs/governance/main-ruleset-v1.json"
API_VERSION = "2022-11-28"


class ApplyError(RuntimeError):
    pass


def load_policy(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ApplyError(f"cannot read ruleset policy {path}: {error}") from error
    if not isinstance(value, dict):
        raise ApplyError("ruleset policy root must be an object")
    if value.get("schema") != "cex.github-main-ruleset.v1":
        raise ApplyError("ruleset policy schema drift")
    if value.get("status") != "active_required":
        raise ApplyError("ruleset policy must remain active_required")
    if value.get("production_authorization") != "not_granted":
        raise ApplyError("ruleset policy cannot grant production authorization")
    if value.get("repository") != "TrillionniumFoundation/CEX":
        raise ApplyError("ruleset policy repository drift")
    if value.get("target_branch") != "main":
        raise ApplyError("ruleset policy target branch drift")
    if value.get("bypass_actors") != []:
        raise ApplyError("ruleset policy must not contain bypass actors")
    checks = value.get("required_status_checks")
    if not isinstance(checks, list) or not checks or len(checks) != len(set(checks)):
        raise ApplyError("required_status_checks must be non-empty and duplicate-free")
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
            "User-Agent": "cex-ruleset-applier",
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


def build_payload(policy: dict[str, Any]) -> dict[str, Any]:
    pull = policy["pull_request"]
    rules: list[dict[str, Any]] = []
    if policy.get("block_deletion"):
        rules.append({"type": "deletion"})
    if policy.get("block_non_fast_forward"):
        rules.append({"type": "non_fast_forward"})
    if policy.get("require_signed_commits"):
        rules.append({"type": "required_signatures"})
    rules.append(
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
        }
    )
    rules.append(
        {
            "type": "required_status_checks",
            "parameters": {
                "strict_required_status_checks_policy": policy[
                    "strict_required_status_checks_policy"
                ],
                "do_not_enforce_on_create": policy["do_not_enforce_on_create"],
                "required_status_checks": [
                    {"context": context}
                    for context in policy["required_status_checks"]
                ],
            },
        }
    )
    deployments = policy.get("required_deployments")
    if deployments:
        rules.append(
            {
                "type": "required_deployments",
                "parameters": {"required_deployment_environments": deployments},
            }
        )
    return {
        "name": policy["ruleset_name"],
        "target": policy["target"],
        "enforcement": policy["enforcement"],
        "bypass_actors": policy["bypass_actors"],
        "conditions": {
            "ref_name": {
                "include": [f"refs/heads/{policy['target_branch']}"],
                "exclude": [],
            }
        },
        "rules": rules,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--policy", type=Path, default=DEFAULT_POLICY)
    parser.add_argument("--token-env", default="GITHUB_ADMIN_TOKEN")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()

    try:
        policy = load_policy(args.policy)
        payload = build_payload(policy)
        result: dict[str, Any] = {
            "schema": "cex.github-main-ruleset-application.v1",
            "repository": policy["repository"],
            "ruleset_name": policy["ruleset_name"],
            "target_branch": policy["target_branch"],
            "mode": "dry_run" if args.dry_run else "apply",
            "payload": payload if args.dry_run else None,
            "production_authorization": "not_granted",
        }
        if args.dry_run:
            result["status"] = "validated_dry_run"
        else:
            token = os.environ.get(args.token_env, "").strip()
            if not token:
                raise ApplyError(f"{args.token_env} is required for live repository administration")
            base = f"https://api.github.com/repos/{policy['repository']}"
            list_status, summaries = request_json(
                "GET", f"{base}/rulesets?per_page=100", token
            )
            if list_status != 200 or not isinstance(summaries, list):
                raise ApplyError(f"cannot list repository rulesets: HTTP {list_status}")
            matching = [
                item
                for item in summaries
                if isinstance(item, dict) and item.get("name") == policy["ruleset_name"]
            ]
            if len(matching) > 1:
                raise ApplyError("multiple rulesets share the authoritative name")
            if matching:
                ruleset_id = matching[0].get("id")
                method = "PUT"
                url = f"{base}/rulesets/{ruleset_id}"
                operation = "updated"
            else:
                method = "POST"
                url = f"{base}/rulesets"
                operation = "created"
            apply_status, applied = request_json(method, url, token, payload)
            if apply_status not in {200, 201} or not isinstance(applied, dict):
                message = applied.get("message") if isinstance(applied, dict) else applied
                raise ApplyError(
                    f"ruleset {operation} request failed: HTTP {apply_status}: {message}"
                )
            ruleset_id = applied.get("id")
            read_status, read_back = request_json(
                "GET", f"{base}/rulesets/{ruleset_id}", token
            )
            if read_status != 200 or not isinstance(read_back, dict):
                raise ApplyError(f"cannot read back applied ruleset: HTTP {read_status}")
            result.update(
                {
                    "status": "applied_and_read_back",
                    "operation": operation,
                    "ruleset_id": ruleset_id,
                    "observed_enforcement": read_back.get("enforcement"),
                    "observed_target": read_back.get("target"),
                    "observed_rule_types": sorted(
                        rule.get("type")
                        for rule in read_back.get("rules", [])
                        if isinstance(rule, dict) and isinstance(rule.get("type"), str)
                    ),
                }
            )
    except ApplyError as error:
        result = {
            "schema": "cex.github-main-ruleset-application.v1",
            "status": "blocked",
            "problems": [str(error)],
            "production_authorization": "not_granted",
        }

    rendered = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    return 0 if result.get("status") in {"validated_dry_run", "applied_and_read_back"} else 1


if __name__ == "__main__":
    raise SystemExit(main())
