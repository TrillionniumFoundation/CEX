#!/usr/bin/env python3
"""Fail closed when a P0 release candidate still contains closure scaffolding."""

from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROBLEMS: list[str] = []

AUTHORITATIVE_WORKFLOWS = (
    ".github/workflows/p0-migration-gate.yml",
    ".github/workflows/rust-service-gate.yml",
    ".github/workflows/p0-gateway-exact-reserve-gate.yml",
    ".github/workflows/p0-execution-settlement-gate.yml",
    ".github/workflows/p0-provider-reconciliation-gate.yml",
)
RELEASE_WORKFLOW = ".github/workflows/p0-release-candidate-gate.yml"
TRIGGER_PATH = "docs/release-evidence/p0-candidate-trigger.json"
TEMPORARY_EXACT_PATHS = (
    "scripts/apply-closure-fixes.py",
    "scripts/patch-0082-provider-view.py",
    "scripts/patch-execution-settlement-fixture.py",
    "scripts/patch-ledger-http-flow-v2.py",
    "scripts/patch-p0-audit-outbox-fixture.py",
    "scripts/patch-p0-migration-account-fixture.py",
)
UNPINNED_ACTION = re.compile(r"^\s*uses:\s*[^#\s]+@(v\d+|stable|main|master)\s*(?:#.*)?$", re.MULTILINE)
MIGRATION_RE = re.compile(r"^(\d{4})_[a-z0-9][a-z0-9._-]*\.sql$")


def relative(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def require_file(path: str) -> str:
    full = ROOT / path
    if not full.is_file():
        PROBLEMS.append(f"missing required file: {path}")
        return ""
    return full.read_text(encoding="utf-8")


for pattern in (
    ".github/workflows/closure-*.yml",
    ".github/workflows/closure-*.yaml",
    "scripts/closure-ci-trigger-*",
    "migrations/closure-ci-trigger-*",
):
    for path in sorted(ROOT.glob(pattern)):
        PROBLEMS.append(f"temporary closure artifact remains: {relative(path)}")
for path in TEMPORARY_EXACT_PATHS:
    if (ROOT / path).exists():
        PROBLEMS.append(f"temporary patcher remains: {path}")

plan = require_file("docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md")
for marker in (
    "Definition of repository closure",
    "External gates that repository edits cannot self-certify",
    "not production-ready",
):
    if marker not in plan:
        PROBLEMS.append(f"v12 plan lacks required marker: {marker}")

trigger_raw = require_file(TRIGGER_PATH)
if trigger_raw:
    try:
        trigger = json.loads(trigger_raw)
    except json.JSONDecodeError as error:
        PROBLEMS.append(f"candidate trigger is invalid JSON: {error}")
    else:
        if trigger.get("schema") != "cex.p0-candidate-trigger.v1":
            PROBLEMS.append("candidate trigger schema is invalid")
        if trigger.get("plan") != "CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md":
            PROBLEMS.append("candidate trigger is not bound to plan v12")
        if not isinstance(trigger.get("sequence"), int) or trigger["sequence"] < 1:
            PROBLEMS.append("candidate trigger sequence must be a positive integer")

for workflow_path in (*AUTHORITATIVE_WORKFLOWS, RELEASE_WORKFLOW):
    content = require_file(workflow_path)
    if not content:
        continue
    if TRIGGER_PATH not in content:
        PROBLEMS.append(f"{workflow_path} does not listen to the shared candidate trigger")
    for match in UNPINNED_ACTION.finditer(content):
        PROBLEMS.append(
            f"{workflow_path} contains an unpinned third-party action: {match.group(0).strip()}"
        )

numbered: list[tuple[int, str]] = []
for path in (ROOT / "migrations").glob("*.sql"):
    match = MIGRATION_RE.fullmatch(path.name)
    if match:
        numbered.append((int(match.group(1)), path.name))
numbered.sort()
if not numbered:
    PROBLEMS.append("no numbered migrations found")
else:
    expected = list(range(numbered[0][0], numbered[-1][0] + 1))
    actual = [number for number, _ in numbered]
    if actual != expected:
        PROBLEMS.append("numbered migration sequence is not contiguous")
    manifest_raw = require_file("docs/templates/cex-release-baseline-manifest-v1.json")
    if manifest_raw:
        try:
            manifest_head = json.loads(manifest_raw)["database"]["migration_head"]
        except (json.JSONDecodeError, KeyError, TypeError) as error:
            PROBLEMS.append(f"cannot read template migration head: {error}")
        else:
            if manifest_head != numbered[-1][1]:
                PROBLEMS.append(
                    f"template migration head {manifest_head!r} != repository head {numbered[-1][1]!r}"
                )

result = {
    "status": "failed" if PROBLEMS else "ok",
    "plan": "CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md",
    "authoritative_workflows": list(AUTHORITATIVE_WORKFLOWS),
    "release_workflow": RELEASE_WORKFLOW,
    "shared_trigger": TRIGGER_PATH,
    "migration_head": numbered[-1][1] if numbered else None,
    "problems": PROBLEMS,
}
print(json.dumps(result, indent=2, ensure_ascii=False))
raise SystemExit(1 if PROBLEMS else 0)
