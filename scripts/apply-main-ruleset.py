#!/usr/bin/env python3
"""Create or update the CEX v12 main-branch ruleset through GitHub's API."""
from __future__ import annotations

import json
import os
import sys
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
POLICY = ROOT / "docs/repository-ruleset-required-contexts-v1.json"
REPOSITORY = os.environ.get("GITHUB_REPOSITORY", "TrillionniumFoundation/CEX")
TOKEN = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
NAME = "CEX v12 main admission"

if not TOKEN:
    raise SystemExit("GITHUB_TOKEN or GH_TOKEN is required")
policy = json.loads(POLICY.read_text(encoding="utf-8"))
contexts = policy["required_status_checks"]
pr = policy["pull_request"]
payload = {
    "name": NAME,
    "target": "branch",
    "enforcement": "active",
    "bypass_actors": [],
    "conditions": {"ref_name": {"include": [policy["target"]], "exclude": []}},
    "rules": [
        {"type": "deletion"},
        {"type": "non_fast_forward"},
        {"type": "pull_request", "parameters": {
            **pr,
            "automatic_copilot_code_review_enabled": False,
            "allowed_merge_methods": ["merge", "squash", "rebase"],
        }},
        {"type": "required_status_checks", "parameters": {
            "strict_required_status_checks_policy": True,
            "do_not_enforce_on_create": False,
            "required_status_checks": [{"context": value} for value in contexts],
        }},
    ],
}

def request(method: str, path: str, value=None):
    body = None if value is None else json.dumps(value).encode()
    req = urllib.request.Request(
        f"https://api.github.com/repos/{REPOSITORY}/{path}",
        data=body,
        method=method,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {TOKEN}",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "cex-v12-ruleset-applicator",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as error:
        detail = error.read().decode("utf-8", errors="replace")
        raise SystemExit(f"GitHub API {method} {path} failed: HTTP {error.code}: {detail}")

_, rulesets = request("GET", "rulesets?per_page=100")
current = next((item for item in rulesets if item.get("name") == NAME), None)
if current:
    status, result = request("PUT", f"rulesets/{current['id']}", payload)
else:
    status, result = request("POST", "rulesets", payload)
print(json.dumps({
    "schema": "cex.repository-ruleset-application.v1",
    "status": "applied",
    "http_status": status,
    "ruleset": result,
    "production_authorization": "not_granted",
}, indent=2, sort_keys=True))
