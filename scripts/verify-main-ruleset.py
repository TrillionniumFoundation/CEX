#!/usr/bin/env python3
"""Verify the unique live CEX main Ruleset as one exact closed policy object."""
from __future__ import annotations

import argparse
import json
import re

from repository_ruleset_common import (
    RulesetError,
    load_policy,
    main_identity,
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
    parser.add_argument("--expected-ruleset-id", required=True, type=int)
    args = parser.parse_args()
    policy, policy_sha = load_policy()
    repository = policy["repository"]["full_name"]
    require(SHA40.fullmatch(args.expected_main_sha) is not None, "expected main SHA is invalid")
    require(SHA256.fullmatch(args.expected_policy_sha256) is not None, "expected policy digest is invalid")
    require(policy_sha == args.expected_policy_sha256, "policy digest changed before verification")
    repository_identity(repository, policy)
    branch = main_identity(repository, policy, args.expected_main_sha)
    actual, summaries = unique_named_ruleset(repository, policy)
    require(actual is not None, "required Ruleset is absent")
    require(actual.get("id") == args.expected_ruleset_id, "Ruleset id changed")
    validate_exact_ruleset(actual, policy)
    require(branch.get("protected") is True, "GitHub does not report main as protected")
    result = {
        "schema": "cex.repository-ruleset-readback.v2",
        "status": "unique_exact_ruleset_active",
        "repository": repository,
        "repository_id": policy["repository"]["repository_id"],
        "main_sha": args.expected_main_sha,
        "main_protected": True,
        "policy_sha256": policy_sha,
        "ruleset_sha256": payload_digest(actual),
        "ruleset_id": actual["id"],
        "ruleset_count_observed": len(summaries),
        "production_authorization": "not_granted",
        "problems": [],
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except RulesetError as error:
        print(json.dumps({
            "schema": "cex.repository-ruleset-readback.v2",
            "status": "failed",
            "problems": [str(error)],
            "production_authorization": "not_granted",
        }, indent=2, sort_keys=True))
        raise SystemExit(1)
