#!/usr/bin/env python3
"""Shared exact, paginated GitHub Ruleset operations for CEX admission control."""
from __future__ import annotations

import hashlib
import json
import os
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable

ROOT = Path(__file__).resolve().parents[1]
POLICY_PATH = ROOT / "docs/repository-ruleset-required-contexts-v1.json"
API_VERSION = "2022-11-28"


class RulesetError(RuntimeError):
    pass


@dataclass(frozen=True)
class Response:
    status: int
    body: Any
    request_id: str | None
    link: str | None


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RulesetError(message)


def canonical_json(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def load_policy() -> tuple[dict[str, Any], str]:
    raw = POLICY_PATH.read_bytes()
    try:
        policy = json.loads(raw)
    except json.JSONDecodeError as error:
        raise RulesetError(f"invalid Ruleset policy JSON: {error}") from error
    require(isinstance(policy, dict), "Ruleset policy must be an object")
    require(policy.get("schema") == "cex.repository-required-contexts.v2", "Ruleset policy schema drift")
    require(policy.get("status") == "active", "Ruleset policy must be active")
    require(policy.get("production_authorization") == "not_granted", "Ruleset policy cannot authorize production")
    return policy, sha256_bytes(raw)


def token() -> str:
    value = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
    require(bool(value), "GITHUB_TOKEN or GH_TOKEN is required")
    return str(value)


def api_request(
    method: str,
    repository: str,
    path: str,
    value: Any = None,
    *,
    accept_error_statuses: Iterable[int] = (),
) -> Response:
    data = None if value is None else canonical_json(value)
    request = urllib.request.Request(
        f"https://api.github.com/repos/{repository}/{path}",
        data=data,
        method=method,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token()}",
            "X-GitHub-Api-Version": API_VERSION,
            "User-Agent": "cex-v12-ruleset-control",
            **({"Content-Type": "application/json"} if data is not None else {}),
        },
    )
    accepted = set(accept_error_statuses)
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            raw = response.read()
            body: Any = json.loads(raw) if raw else None
            return Response(
                int(response.status),
                body,
                response.headers.get("X-GitHub-Request-Id"),
                response.headers.get("Link"),
            )
    except urllib.error.HTTPError as error:
        raw = error.read()
        try:
            body = json.loads(raw) if raw else None
        except json.JSONDecodeError:
            body = {"message": raw.decode("utf-8", errors="replace")}
        if int(error.code) in accepted:
            return Response(
                int(error.code),
                body,
                error.headers.get("X-GitHub-Request-Id"),
                error.headers.get("Link"),
            )
        raise RulesetError(
            f"GitHub API {method} {path} failed: HTTP {error.code}: "
            f"{json.dumps(body, sort_keys=True)}"
        ) from error


def paginated_rulesets(repository: str, *, includes_parents: bool = True) -> list[dict[str, Any]]:
    values: list[dict[str, Any]] = []
    page = 1
    while True:
        query = urllib.parse.urlencode(
            {
                "includes_parents": "true" if includes_parents else "false",
                "per_page": 100,
                "page": page,
            }
        )
        response = api_request("GET", repository, f"rulesets?{query}")
        require(response.status == 200 and isinstance(response.body, list), "Ruleset collection response is invalid")
        batch = response.body
        for item in batch:
            require(isinstance(item, dict), "Ruleset summary must be an object")
            values.append(item)
        if len(batch) < 100:
            break
        page += 1
        require(page <= 100, "Ruleset pagination exceeded 10,000 objects")
    return values


def desired_payload(policy: dict[str, Any]) -> dict[str, Any]:
    rules = policy["rules"]
    ordered = [
        {"type": "deletion"},
        {"type": "non_fast_forward"},
        {"type": "pull_request", "parameters": rules["pull_request"]},
        {
            "type": "required_status_checks",
            "parameters": rules["required_status_checks"],
        },
    ]
    return {
        "name": policy["ruleset"]["name"],
        "target": policy["ruleset"]["target"],
        "enforcement": policy["ruleset"]["enforcement"],
        "bypass_actors": policy["ruleset"]["bypass_actors"],
        "conditions": policy["conditions"],
        "rules": ordered,
    }


def normalize_rule(rule: dict[str, Any]) -> dict[str, Any]:
    require(isinstance(rule, dict) and isinstance(rule.get("type"), str), "Ruleset rule is invalid")
    value: dict[str, Any] = {"type": rule["type"]}
    if "parameters" in rule:
        parameters = json.loads(json.dumps(rule["parameters"]))
        if rule["type"] == "required_status_checks":
            checks = parameters.get("required_status_checks", [])
            require(isinstance(checks, list), "required_status_checks must be a list")
            normalized_checks = []
            for check in checks:
                require(isinstance(check, dict) and isinstance(check.get("context"), str), "required check is invalid")
                item = {"context": check["context"]}
                if check.get("integration_id") is not None:
                    item["integration_id"] = check["integration_id"]
                normalized_checks.append(item)
            parameters["required_status_checks"] = sorted(
                normalized_checks,
                key=lambda item: (item["context"], item.get("integration_id", -1)),
            )
        elif rule["type"] == "pull_request":
            methods = parameters.get("allowed_merge_methods", [])
            require(isinstance(methods, list), "allowed_merge_methods must be a list")
            parameters["allowed_merge_methods"] = sorted(methods)
        value["parameters"] = parameters
    return value


def normalize_ruleset(value: dict[str, Any]) -> dict[str, Any]:
    require(isinstance(value, dict), "Ruleset must be an object")
    rules = value.get("rules", [])
    require(isinstance(rules, list), "Ruleset rules must be a list")
    normalized_rules = [normalize_rule(rule) for rule in rules]
    types = [rule["type"] for rule in normalized_rules]
    require(len(types) == len(set(types)), f"duplicate rule types are forbidden: {types}")
    conditions = value.get("conditions")
    require(isinstance(conditions, dict), "Ruleset conditions are absent")
    bypass = value.get("bypass_actors", [])
    require(isinstance(bypass, list), "Ruleset bypass_actors must be a list")
    return {
        "name": value.get("name"),
        "target": value.get("target"),
        "enforcement": value.get("enforcement"),
        "bypass_actors": bypass,
        "conditions": conditions,
        "rules": sorted(normalized_rules, key=lambda item: item["type"]),
    }


def payload_digest(value: dict[str, Any]) -> str:
    return sha256_bytes(canonical_json(normalize_ruleset(value)))


def validate_exact_ruleset(actual: dict[str, Any], policy: dict[str, Any]) -> None:
    expected = normalize_ruleset(desired_payload(policy))
    observed = normalize_ruleset(actual)
    require(observed == expected, "live Ruleset differs from the complete policy object")


def repository_identity(repository: str, policy: dict[str, Any]) -> dict[str, Any]:
    response = api_request("GET", repository, "")
    require(response.status == 200 and isinstance(response.body, dict), "repository metadata response is invalid")
    value = response.body
    expected = policy["repository"]
    require(value.get("full_name") == expected["full_name"] == repository, "repository full_name drift")
    require(value.get("id") == expected["repository_id"], "repository ID drift")
    require(value.get("default_branch") == expected["default_branch"], "default branch drift")
    return value


def main_identity(repository: str, policy: dict[str, Any], expected_sha: str) -> dict[str, Any]:
    branch = policy["repository"]["default_branch"]
    response = api_request("GET", repository, f"branches/{urllib.parse.quote(branch, safe='')}")
    require(response.status == 200 and isinstance(response.body, dict), "main branch response is invalid")
    actual_sha = response.body.get("commit", {}).get("sha")
    require(actual_sha == expected_sha, f"main moved: expected={expected_sha} actual={actual_sha}")
    return response.body


def unique_named_ruleset(repository: str, policy: dict[str, Any]) -> tuple[dict[str, Any] | None, list[dict[str, Any]]]:
    summaries = paginated_rulesets(repository, includes_parents=True)
    name = policy["ruleset"]["name"]
    matches = [item for item in summaries if item.get("name") == name]
    require(len(matches) <= 1, f"duplicate named Rulesets are forbidden: {name} count={len(matches)}")
    if not matches:
        return None, summaries
    item = matches[0]
    require(item.get("source_type") in {None, "Repository"}, "required Ruleset is inherited rather than repository-owned")
    ruleset_id = item.get("id")
    require(isinstance(ruleset_id, int), "Ruleset summary lacks integer id")
    response = api_request("GET", repository, f"rulesets/{ruleset_id}")
    require(response.status == 200 and isinstance(response.body, dict), "full Ruleset response is invalid")
    return response.body, summaries
