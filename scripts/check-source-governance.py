#!/usr/bin/env python3
"""Validate repository-side governance controls without fabricating GitHub settings."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROBLEMS: list[str] = []
CONTENTS_WRITE = re.compile(r"(?m)^\s*contents:\s*write\s*$")
GIT_PUSH = re.compile(r"(?m)^\s*git\s+push(?:\s|$)")


def read(relative: str) -> str:
    path = ROOT / relative
    if not path.is_file():
        PROBLEMS.append(f"missing required file: {relative}")
        return ""
    return path.read_text(encoding="utf-8")


codeowners = read(".github/CODEOWNERS")
policy = read("docs/repository-governance-policy-v1.md")
hygiene = read("scripts/check-p0-release-candidate-hygiene.py")

for marker in (
    "* @ProfHepta",
    "/.github/workflows/",
    "/docs/development-doc-authority-v1.json",
    "/docs/traceability/",
    "/migrations/",
    "/services/ledger-service/",
    "/services/capability-service/",
    "/PROJECT_BOUNDARY.json",
):
    if marker not in codeowners:
        PROBLEMS.append(f"CODEOWNERS lacks critical ownership marker: {marker}")

for marker in (
    "block force pushes",
    "require at least two approving reviews",
    "require CODEOWNERS review",
    "require resolved conversations",
    "Production authorization: `not_granted`",
    "Actual GitHub ruleset state is not inferred from source",
):
    if marker not in policy:
        PROBLEMS.append(f"governance policy lacks marker: {marker}")

for workflow in sorted((ROOT / ".github/workflows").glob("*.y*ml")):
    text = workflow.read_text(encoding="utf-8")
    relative = workflow.relative_to(ROOT).as_posix()
    if CONTENTS_WRITE.search(text):
        PROBLEMS.append(f"workflow has forbidden contents: write permission: {relative}")
    if GIT_PUSH.search(text):
        PROBLEMS.append(f"workflow performs forbidden source push: {relative}")

for marker in (
    "CONTENTS_WRITE_PERMISSION",
    "workflow has forbidden contents: write permission",
    "**/closure-ci-trigger-*",
):
    if marker not in hygiene:
        PROBLEMS.append(f"release hygiene lacks governance marker: {marker}")

result = {
    "schema": "cex.source-governance-check.v1",
    "status": "failed" if PROBLEMS else "ok",
    "codeowners": "present",
    "workflow_contents_write": "forbidden",
    "workflow_git_push": "forbidden",
    "actual_ruleset_state": "independent_github_setting",
    "production_authorization": "not_granted",
    "problems": PROBLEMS,
}
print(json.dumps(result, indent=2, sort_keys=True))
sys.exit(1 if PROBLEMS else 0)
