#!/usr/bin/env python3
"""Fail closed when a P0 release candidate still contains closure scaffolding."""

from __future__ import annotations

import json
import re
import subprocess
import sys
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
WORKFLOW_PATTERNS = (
    ".github/workflows/*.yml",
    ".github/workflows/*.yaml",
)
TRIGGER_PATH = "docs/release-evidence/p0-candidate-trigger.json"
ACTIVE_PLAN = "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md"
ACTIVE_ADDENDUM = "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md"
DOC_CHECKER = "scripts/check-development-docs.py"
EXPECTED_MIGRATION_HEAD = "0087_add_term_exchange_receipt_event_history.sql"
TEMPORARY_EXACT_PATHS = (
    "scripts/apply-closure-fixes.py",
    "scripts/patch-0082-provider-view.py",
    "scripts/patch-execution-settlement-fixture.py",
    "scripts/patch-ledger-http-flow-v2.py",
    "scripts/patch-p0-audit-outbox-fixture.py",
    "scripts/patch-p0-migration-account-fixture.py",
    "scripts/p0-v12-trnm-exact-cutover-extract.py",
    ".github/workflows/p0-v12-trnm-exact-cutover.yml",
    ".github/workflows/p0-v12-trnm-exact-cutover-runner.yml",
    ".github/workflows/p0-v12-trnm-receipt-smoke.yml",
    ".github/workflows/world-settlement-final-convergence-v2.yml",
    ".github/workflows/world-settlement-final-convergence-v3.yml",
    ".github/workflows/world-settlement-final-validation-v2.yml",
)
TEMPORARY_WORKFLOW_PATTERNS = (
    ".github/workflows/*self-repair*.yml",
    ".github/workflows/*self-repair*.yaml",
    ".github/workflows/*source-export*.yml",
    ".github/workflows/*source-export*.yaml",
    ".github/workflows/*source-snapshot*.yml",
    ".github/workflows/*source-snapshot*.yaml",
    ".github/workflows/*rustfmt-patch*.yml",
    ".github/workflows/*rustfmt-patch*.yaml",
    ".github/workflows/*gofmt-remediation*.yml",
    ".github/workflows/*gofmt-remediation*.yaml",
    ".github/workflows/*lock-refresh*.yml",
    ".github/workflows/*lock-refresh*.yaml",
    ".github/workflows/*remediation-publish*.yml",
    ".github/workflows/*remediation-publish*.yaml",
    ".github/workflows/*source-remediation*.yml",
    ".github/workflows/*source-remediation*.yaml",
    ".github/workflows/*unit-diagnostics*.yml",
    ".github/workflows/*unit-diagnostics*.yaml",
    ".github/workflows/*gap-closure-validate*.yml",
    ".github/workflows/*gap-closure-validate*.yaml",
)
ACTION_USE = re.compile(
    r"^\s*(?:-\s*)?uses:\s*([^#\s]+)\s*(?:#.*)?$", re.MULTILINE
)
PINNED_ACTION_REF = re.compile(r"^[0-9a-f]{40}$")
PINNED_DOCKER_USE = re.compile(
    r"^docker://[^@\s]+@sha256:[0-9a-f]{64}$"
)
PULL_REQUEST_EVENT = re.compile(r"(?m)^\s{2}pull_request:\s*$")
MIGRATION_RE = re.compile(r"^(\d{4})_[a-z0-9][a-z0-9._-]*\.sql$")


def relative(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def require_file(path: str) -> str:
    full = ROOT / path
    if not full.is_file():
        PROBLEMS.append(f"missing required file: {path}")
        return ""
    return full.read_text(encoding="utf-8")


def git_identity() -> tuple[str | None, str | None]:
    """Return the checked-out commit/tree used by this hygiene record.

    The release collector consumes this JSON as local evidence. Stamping the
    identity here means the packet cannot silently fall back to an unbound
    ``unknown`` record when the checker is run in the hosted checkout.
    """

    values: list[str | None] = []
    for revision in ("HEAD", "HEAD^{tree}"):
        try:
            value = subprocess.run(
                ["git", "-C", str(ROOT), "rev-parse", revision],
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip()
        except (OSError, subprocess.CalledProcessError):
            value = None
        values.append(value or None)
    return values[0], values[1]


def workflow_paths() -> list[Path]:
    paths: set[Path] = set()
    for pattern in WORKFLOW_PATTERNS:
        paths.update(path for path in ROOT.glob(pattern) if path.is_file())
    return sorted(paths, key=relative)


def action_pin_problem(use: str) -> str | None:
    """Return a fail-closed pin error for one GitHub Actions ``uses`` value."""

    if use.startswith("./"):
        return None
    if use.startswith("docker://"):
        if PINNED_DOCKER_USE.fullmatch(use):
            return None
        return (
            f"{use}; docker actions must use "
            "docker://<immutable-image>@sha256:<64 lowercase hex>"
        )

    action, separator, ref = use.rpartition("@")
    if action and separator and PINNED_ACTION_REF.fullmatch(ref):
        return None
    return f"{use}; external actions must use a 40-character lowercase commit SHA"


def validate_action_pin_checker() -> None:
    """Self-test all supported immutable and mutable reference classes."""

    cases = (
        ("./.github/actions/local", True),
        ("./.github/workflows/local-reusable.yml", True),
        ("actions/checkout@" + "a" * 40, True),
        (
            "owner/repository/.github/workflows/reusable.yml@" + "b" * 40,
            True,
        ),
        ("docker://ghcr.io/example/image@sha256:" + "c" * 64, True),
        ("actions/checkout@v4", False),
        ("actions/checkout@latest", False),
        ("actions/checkout@develop", False),
        ("actions/checkout@refs/heads/main", False),
        ("owner/repository/.github/workflows/reusable.yml@main", False),
        ("docker://alpine:3.20", False),
        ("docker://ghcr.io/example/image@sha256:abc", False),
        ("${{ matrix.action }}", False),
    )
    for use, expected_valid in cases:
        actual_valid = action_pin_problem(use) is None
        if actual_valid != expected_valid:
            PROBLEMS.append(
                "workflow action pin checker self-test failed for "
                f"{use!r}: expected_valid={expected_valid}, actual_valid={actual_valid}"
            )


def validate_action_use_parser() -> None:
    """Prove both block and compact YAML step forms enter the pin validator."""

    pinned = "actions/checkout@" + "d" * 40
    sample = (
        f"jobs:\n  block:\n    steps:\n      - name: block\n        uses: {pinned}\n"
        f"      - uses: {pinned} # compact\n"
    )
    parsed = ACTION_USE.findall(sample)
    if parsed != [pinned, pinned]:
        PROBLEMS.append(
            "workflow action uses parser self-test failed: "
            f"expected two pinned references, parsed={parsed!r}"
        )


def validate_workflow_action_pins(paths: list[Path]) -> None:
    """Require immutable external action identities in every workflow.

    Self-hosted and auxiliary workflows execute with the same repository trust
    as release workflows. Restricting pin checks to the six candidate gates
    would leave an avoidable supply-chain bypass through a scheduled or manual
    helper workflow.
    """

    for path in paths:
        content = path.read_text(encoding="utf-8")
        for match in ACTION_USE.finditer(content):
            use = match.group(1)
            if problem := action_pin_problem(use):
                PROBLEMS.append(
                    f"{relative(path)} contains a mutable action reference: {problem}"
                )


for pattern in (
    ".github/workflows/closure-*.yml",
    ".github/workflows/closure-*.yaml",
    "scripts/closure-ci-trigger-*",
    "migrations/closure-ci-trigger-*",
    "**/closure-ci-trigger-*",
    *TEMPORARY_WORKFLOW_PATTERNS,
):
    for path in sorted(ROOT.glob(pattern)):
        PROBLEMS.append(f"temporary closure artifact remains: {relative(path)}")
for path in TEMPORARY_EXACT_PATHS:
    if (ROOT / path).exists():
        PROBLEMS.append(f"temporary patcher remains: {path}")

all_workflows = workflow_paths()
validate_action_pin_checker()
validate_action_use_parser()
validate_workflow_action_pins(all_workflows)

plan = require_file(ACTIVE_PLAN)
for marker in (
    "Definition of repository closure",
    "External gates that repository edits cannot self-certify",
    "not production-ready",
    f"Candidate migration head: `{EXPECTED_MIGRATION_HEAD}`.",
):
    if marker not in plan:
        PROBLEMS.append(f"v12 plan lacks required marker: {marker}")

addendum = require_file(ACTIVE_ADDENDUM)
for marker in (
    "Block H",
    "Block I",
    "REPOSITORY_CLOSED_CANDIDATE",
    "External production gates remain upstream blockers",
):
    if marker not in addendum:
        PROBLEMS.append(f"v12 addendum lacks required marker: {marker}")

trigger_raw = require_file(TRIGGER_PATH)
if trigger_raw:
    try:
        trigger = json.loads(trigger_raw)
    except json.JSONDecodeError as error:
        PROBLEMS.append(f"candidate trigger is invalid JSON: {error}")
    else:
        if trigger.get("schema") != "cex.p0-candidate-trigger.v1":
            PROBLEMS.append("candidate trigger schema is invalid")
        if trigger.get("plan") != Path(ACTIVE_PLAN).name:
            PROBLEMS.append("candidate trigger is not bound to plan v12")
        if not isinstance(trigger.get("sequence"), int) or trigger["sequence"] < 1:
            PROBLEMS.append("candidate trigger sequence must be a positive integer")
        if trigger.get("production_authorization") != "not_granted":
            PROBLEMS.append("candidate trigger must explicitly deny production authorization")

for workflow_path in (*AUTHORITATIVE_WORKFLOWS, RELEASE_WORKFLOW):
    content = require_file(workflow_path)
    if not content:
        continue
    if TRIGGER_PATH not in content:
        PROBLEMS.append(f"{workflow_path} does not listen to the shared candidate trigger")
    if "workflow_dispatch:" not in content:
        PROBLEMS.append(f"{workflow_path} lacks a manual recovery dispatch")
    if PULL_REQUEST_EVENT.search(content):
        PROBLEMS.append(
            f"{workflow_path} must not accept pull_request merge trees as release evidence"
        )

release_content = require_file(RELEASE_WORKFLOW)
if release_content:
    if "GITHUB_REF_TYPE" not in release_content:
        PROBLEMS.append(
            f"{RELEASE_WORKFLOW} must reject tag refs before collecting evidence"
        )
    if "release evidence requires a branch ref" not in release_content:
        PROBLEMS.append(
            f"{RELEASE_WORKFLOW} lacks the explicit branch-ref fail-closed guard"
        )
    if "scripts/check-term-exchange-receipt-partial-upgrade-postgres.sh" not in release_content:
        PROBLEMS.append(
            f"{RELEASE_WORKFLOW} must execute the term-exchange partial-upgrade regression"
        )
    if "term-exchange-receipt-partial-upgrade-regression" not in release_content:
        PROBLEMS.append(
            f"{RELEASE_WORKFLOW} lifecycle evidence omits the term-exchange partial-upgrade regression"
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
    if numbered[-1][1] != EXPECTED_MIGRATION_HEAD:
        PROBLEMS.append(
            f"repository migration head {numbered[-1][1]!r} != active v12 head {EXPECTED_MIGRATION_HEAD!r}"
        )
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

documentation = subprocess.run(
    [sys.executable, str(ROOT / DOC_CHECKER)],
    cwd=ROOT,
    text=True,
    stdout=subprocess.PIPE,
    stderr=subprocess.STDOUT,
    check=False,
)
if documentation.returncode != 0:
    PROBLEMS.append("development-document contract failed: " + documentation.stdout.strip())

commit_sha, tree_sha = git_identity()
if commit_sha is None or tree_sha is None:
    PROBLEMS.append("cannot resolve exact git commit/tree identity")

result = {
    "status": "failed" if PROBLEMS else "ok",
    "plan": Path(ACTIVE_PLAN).name,
    "addendum": Path(ACTIVE_ADDENDUM).name,
    "authoritative_workflows": list(AUTHORITATIVE_WORKFLOWS),
    "release_workflow": RELEASE_WORKFLOW,
    "workflow_pin_scope": [relative(path) for path in all_workflows],
    "shared_trigger": TRIGGER_PATH,
    "commit_sha": commit_sha,
    "tree_sha": tree_sha,
    "migration_head": numbered[-1][1] if numbered else None,
    "documentation_contract": "ok" if documentation.returncode == 0 else "failed",
    "problems": PROBLEMS,
}
print(json.dumps(result, indent=2, ensure_ascii=False))
raise SystemExit(1 if PROBLEMS else 0)
