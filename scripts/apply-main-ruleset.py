#!/usr/bin/env python3
"""Apply the exact CEX main admission Ruleset with compare-and-swap semantics."""
from __future__ import annotations

import argparse
import json
import re

from repository_ruleset_common import (
    RulesetError,
    api_request,
    desired_payload,
    load_policy,
    main_identity,
    normalize_ruleset,
    payload_digest,
    repository_identity,
    require,
    unique_named_ruleset,
    validate_exact_ruleset,
)

SHA40 = re.compile(r"^[0-9a-f]{40}$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--expected-main-sha", required=True)
    parser.add_argument("--expected-policy-sha256", required=True)
    parser.add_argument("--allow-create", action="store_true")
    parser.add_argument("--expected-ruleset-id", type=int)
    parser.add_argument("--expected-current-ruleset-sha256")
    args = parser.parse_args()

    policy, policy_sha = load_policy()
    repository = policy["repository"]["full_name"]
    require(SHA40.fullmatch(args.expected_main_sha) is not None, "expected main SHA is invalid")
    require(SHA256.fullmatch(args.expected_policy_sha256) is not None, "expected policy digest is invalid")
    require(policy_sha == args.expected_policy_sha256, "policy digest changed before application")
    repository_identity(repository, policy)
    before = main_identity(repository, policy, args.expected_main_sha)
    current, _ = unique_named_ruleset(repository, policy)
    desired = desired_payload(policy)
    desired_digest = payload_digest(desired)

    operation = "unchanged"
    request_id = None
    if current is None:
        require(args.allow_create, "Ruleset is absent; pass --allow-create for this exact policy/main tuple")
        require(args.expected_ruleset_id is None, "cannot expect a Ruleset id while creating")
        require(args.expected_current_ruleset_sha256 is None, "cannot expect a current digest while creating")
        response = api_request("POST", repository, "rulesets", desired)
        require(response.status == 201 and isinstance(response.body, dict), "Ruleset creation response is invalid")
        ruleset_id = response.body.get("id")
        require(isinstance(ruleset_id, int), "created Ruleset lacks id")
        operation = "created"
        request_id = response.request_id
    else:
        ruleset_id = current.get("id")
        require(isinstance(ruleset_id, int), "current Ruleset lacks id")
        if args.expected_ruleset_id is not None:
            require(ruleset_id == args.expected_ruleset_id, "Ruleset id changed before application")
        current_digest = payload_digest(current)
        if normalize_ruleset(current) != normalize_ruleset(desired):
            require(
                args.expected_current_ruleset_sha256 is not None,
                "live Ruleset drift requires --expected-current-ruleset-sha256",
            )
            require(
                SHA256.fullmatch(args.expected_current_ruleset_sha256) is not None,
                "expected current Ruleset digest is invalid",
            )
            require(current_digest == args.expected_current_ruleset_sha256, "live Ruleset changed after review")
            response = api_request("PUT", repository, f"rulesets/{ruleset_id}", desired)
            require(response.status == 200 and isinstance(response.body, dict), "Ruleset update response is invalid")
            operation = "updated"
            request_id = response.request_id

    applied, summaries = unique_named_ruleset(repository, policy)
    require(applied is not None and applied.get("id") == ruleset_id, "applied Ruleset read-back identity drift")
    validate_exact_ruleset(applied, policy)
    after = main_identity(repository, policy, args.expected_main_sha)
    result = {
        "schema": "cex.repository-ruleset-application.v2",
        "status": "applied_and_exactly_read_back",
        "operation": operation,
        "repository": repository,
        "repository_id": policy["repository"]["repository_id"],
        "main_sha_before": before["commit"]["sha"],
        "main_sha_after": after["commit"]["sha"],
        "policy_sha256": policy_sha,
        "desired_ruleset_sha256": desired_digest,
        "ruleset_id": ruleset_id,
        "ruleset_count_observed": len(summaries),
        "request_id": request_id,
        "production_authorization": "not_granted",
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except RulesetError as error:
        print(json.dumps({
            "schema": "cex.repository-ruleset-application.v2",
            "status": "failed",
            "problems": [str(error)],
            "production_authorization": "not_granted",
        }, indent=2, sort_keys=True))
        raise SystemExit(1)
