#!/usr/bin/env python3
"""Exercise CEX Ruleset enforcement without treating arbitrary API failure as proof."""
from __future__ import annotations

import argparse
import json
import os
import re
import urllib.parse
import uuid
from typing import Any

from repository_ruleset_common import (
    RulesetError,
    api_request,
    load_policy,
    main_identity,
    repository_identity,
    require,
    unique_named_ruleset,
    validate_exact_ruleset,
)

SHA40 = re.compile(r"^[0-9a-f]{40}$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")


def branch_path(branch: str) -> str:
    return urllib.parse.quote(branch, safe="")


def commit_with_tree(repository: str, tree: str, parents: list[str], message: str) -> str:
    response = api_request(
        "POST",
        repository,
        "git/commits",
        {"message": message, "tree": tree, "parents": parents},
    )
    require(response.status == 201 and isinstance(response.body, dict), "commit creation response is invalid")
    sha = response.body.get("sha")
    require(isinstance(sha, str) and SHA40.fullmatch(sha) is not None, "created commit SHA is invalid")
    return sha


def create_branch(repository: str, branch: str, sha: str) -> None:
    response = api_request(
        "POST",
        repository,
        "git/refs",
        {"ref": f"refs/heads/{branch}", "sha": sha},
    )
    require(response.status == 201, f"failed to create positive-control branch: {branch}")


def update_branch(repository: str, branch: str, sha: str, *, force: bool, accepted: tuple[int, ...] = ()):
    return api_request(
        "PATCH",
        repository,
        f"git/refs/heads/{branch_path(branch)}",
        {"sha": sha, "force": force},
        accept_error_statuses=accepted,
    )


def delete_branch(repository: str, branch: str, *, accepted: tuple[int, ...] = ()):
    return api_request(
        "DELETE",
        repository,
        f"git/refs/heads/{branch_path(branch)}",
        accept_error_statuses=accepted,
    )


def rejection_summary(response: Any, label: str, accepted_statuses: set[int]) -> dict[str, Any]:
    require(response.status in accepted_statuses, f"{label} returned non-ruleset status HTTP {response.status}")
    require(response.status not in {401, 404} and response.status < 500, f"{label} is authentication/not-found/server failure")
    require(isinstance(response.body, dict), f"{label} response body is not structured JSON")
    text = json.dumps(response.body, sort_keys=True).lower()
    markers = (
        "repository rule",
        "rule violations",
        "ruleset",
        "pull request",
        "non-fast-forward",
        "force push",
    )
    require(any(marker in text for marker in markers), f"{label} lacks a Ruleset violation marker")
    return {
        "http_status": response.status,
        "request_id": response.request_id,
        "body": response.body,
        "ruleset_violation_marker_present": True,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--expected-main-sha", required=True)
    parser.add_argument("--expected-policy-sha256", required=True)
    parser.add_argument("--expected-ruleset-id", required=True, type=int)
    args = parser.parse_args()

    policy, policy_sha = load_policy()
    repository = policy["repository"]["full_name"]
    accepted_statuses = set(policy["negative_probe"]["accepted_rule_rejection_http_statuses"])
    require(SHA40.fullmatch(args.expected_main_sha) is not None, "expected main SHA is invalid")
    require(SHA256.fullmatch(args.expected_policy_sha256) is not None, "expected policy digest is invalid")
    require(policy_sha == args.expected_policy_sha256, "policy digest changed before probe")
    repository_identity(repository, policy)
    before = main_identity(repository, policy, args.expected_main_sha)
    ruleset, _ = unique_named_ruleset(repository, policy)
    require(ruleset is not None and ruleset.get("id") == args.expected_ruleset_id, "main Ruleset identity drift")
    validate_exact_ruleset(ruleset, policy)

    actor = os.environ.get("GITHUB_ACTOR", "").strip()
    require(bool(actor), "GITHUB_ACTOR is required to bind the same-actor control")
    nonce = f"{os.environ.get('GITHUB_RUN_ID', 'manual')}-{os.getpid()}-{uuid.uuid4().hex[:8]}"
    control = f"ruleset-positive-control-{nonce}"
    deletion_canary = f"ruleset-deletion-canary-{nonce}"
    canary_ruleset_id: int | None = None
    cleanup: list[dict[str, Any]] = []

    main_commit = api_request("GET", repository, f"git/commits/{args.expected_main_sha}")
    require(main_commit.status == 200 and isinstance(main_commit.body, dict), "main commit read failed")
    main_tree = main_commit.body.get("tree", {}).get("sha")
    require(isinstance(main_tree, str) and SHA40.fullmatch(main_tree) is not None, "main tree is invalid")
    parents = main_commit.body.get("parents", [])
    require(isinstance(parents, list) and parents, "main commit must have a parent for the non-fast-forward probe")
    main_parent = parents[0].get("sha") if isinstance(parents[0], dict) else None
    require(isinstance(main_parent, str) and SHA40.fullmatch(main_parent) is not None, "main parent is invalid")

    try:
        control_initial = commit_with_tree(
            repository, main_tree, [args.expected_main_sha], "CEX Ruleset positive-control initial commit"
        )
        create_branch(repository, control, control_initial)
        control_ff = commit_with_tree(
            repository, main_tree, [control_initial], "CEX Ruleset positive-control fast-forward commit"
        )
        ff_response = update_branch(repository, control, control_ff, force=False)
        require(ff_response.status == 200, "same-actor positive fast-forward control failed")
        control_divergent = commit_with_tree(
            repository, main_tree, [args.expected_main_sha], "CEX Ruleset positive-control divergent commit"
        )
        force_response = update_branch(repository, control, control_divergent, force=True)
        require(force_response.status == 200, "same-actor positive force-update control failed")
        delete_response = delete_branch(repository, control)
        require(delete_response.status == 204, "same-actor positive deletion control failed")
        control = ""

        direct_candidate = commit_with_tree(
            repository, main_tree, [args.expected_main_sha], "CEX main direct-update negative probe; never merge"
        )
        direct_response = update_branch(
            repository,
            "main",
            direct_candidate,
            force=False,
            accepted=tuple(sorted(accepted_statuses)),
        )
        direct = rejection_summary(direct_response, "direct main update", accepted_statuses)

        unrelated_candidate = commit_with_tree(
            repository, main_tree, [main_parent], "CEX main non-fast-forward negative probe; never merge"
        )
        force_main_response = update_branch(
            repository,
            "main",
            unrelated_candidate,
            force=True,
            accepted=tuple(sorted(accepted_statuses)),
        )
        force_main = rejection_summary(force_main_response, "forced main update", accepted_statuses)

        create_branch(repository, deletion_canary, args.expected_main_sha)
        canary_name = f"CEX deletion negative probe {nonce}"
        canary_payload = {
            "name": canary_name,
            "target": "branch",
            "enforcement": "active",
            "bypass_actors": [],
            "conditions": {
                "ref_name": {
                    "include": [f"refs/heads/{deletion_canary}"],
                    "exclude": [],
                }
            },
            "rules": [{"type": "deletion"}],
        }
        canary_create = api_request("POST", repository, "rulesets", canary_payload)
        require(canary_create.status == 201 and isinstance(canary_create.body, dict), "canary Ruleset creation failed")
        canary_ruleset_id = canary_create.body.get("id")
        require(isinstance(canary_ruleset_id, int), "canary Ruleset lacks id")
        canary_read = api_request("GET", repository, f"rulesets/{canary_ruleset_id}")
        require(canary_read.status == 200 and isinstance(canary_read.body, dict), "canary Ruleset read-back failed")
        require(canary_read.body.get("enforcement") == "active", "canary Ruleset is not active")
        require(canary_read.body.get("conditions") == canary_payload["conditions"], "canary conditions drift")
        require(
            [rule.get("type") for rule in canary_read.body.get("rules", [])] == ["deletion"],
            "canary deletion rule drift",
        )
        canary_delete_response = delete_branch(
            repository,
            deletion_canary,
            accepted=tuple(sorted(accepted_statuses)),
        )
        deletion = rejection_summary(canary_delete_response, "canary deletion", accepted_statuses)
        canary_ruleset_delete = api_request("DELETE", repository, f"rulesets/{canary_ruleset_id}")
        require(canary_ruleset_delete.status == 204, "canary Ruleset cleanup failed")
        canary_ruleset_id = None
        canary_branch_delete = delete_branch(repository, deletion_canary)
        require(canary_branch_delete.status == 204, "canary branch cleanup failed")
        deletion_canary = ""

        after = main_identity(repository, policy, args.expected_main_sha)
        result = {
            "schema": "cex.main-ruleset-negative-probes.v3",
            "status": "same_actor_control_and_rule_specific_negative_probes_passed",
            "repository": repository,
            "repository_id": policy["repository"]["repository_id"],
            "actor": actor,
            "policy_sha256": policy_sha,
            "ruleset_id": args.expected_ruleset_id,
            "main_before": before["commit"]["sha"],
            "main_after": after["commit"]["sha"],
            "same_actor_positive_control": {
                "fast_forward_update_succeeded": True,
                "force_update_succeeded": True,
                "deletion_succeeded": True,
            },
            "negative_probes": {
                "direct_main_update": direct,
                "forced_non_fast_forward_main_update": force_main,
                "deletion_rule_canary": deletion,
            },
            "arbitrary_failure_receives_credit": False,
            "default_branch_deletion_used_as_rule_proof": False,
            "production_authorization": "not_granted",
        }
        print(json.dumps(result, indent=2, sort_keys=True))
        return 0
    finally:
        if canary_ruleset_id is not None:
            try:
                response = api_request(
                    "DELETE",
                    repository,
                    f"rulesets/{canary_ruleset_id}",
                    accept_error_statuses=(404,),
                )
                cleanup.append({"canary_ruleset": canary_ruleset_id, "status": response.status})
            except RulesetError as error:
                cleanup.append({"canary_ruleset": canary_ruleset_id, "error": str(error)})
        for branch in (control, deletion_canary):
            if branch:
                try:
                    response = delete_branch(repository, branch, accepted=(404,))
                    cleanup.append({"branch": branch, "status": response.status})
                except RulesetError as error:
                    cleanup.append({"branch": branch, "error": str(error)})
        if cleanup:
            print(json.dumps({"cleanup": cleanup}, sort_keys=True), file=os.sys.stderr)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except RulesetError as error:
        print(json.dumps({
            "schema": "cex.main-ruleset-negative-probes.v3",
            "status": "failed",
            "problems": [str(error)],
            "production_authorization": "not_granted",
        }, indent=2, sort_keys=True))
        raise SystemExit(1)
