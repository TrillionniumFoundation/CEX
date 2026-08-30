#!/usr/bin/env python3
"""Fail closed unless a frozen hosted-gate run/attempt set is still the latest.

This verifier never substitutes a newer workflow run into release context. It
compares the exact frozen run and attempt selected by the authoritative hosted
checker with the current latest authoritative branch/SHA state. A newer success,
failure, cancellation, skip, or active rerun invalidates the candidate.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import re
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
HOSTED_GATE_CHECKER = ROOT / "scripts/check-hosted-gate-execution.py"
CANDIDATE_WORKFLOW = ROOT / ".github/workflows/p0-release-candidate-gate.yml"
SELECTION_POLICY = "latest_authoritative_run_is_binding"
HOSTED_GATE_ATTESTATION = "hosted-gate-execution.json"
GIT_SHA_RE = re.compile(r"^[0-9a-f]{40}$")
SHA256_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
WORKFLOW_PATHS = {
    "p0-migration-gate": ".github/workflows/p0-migration-gate.yml",
    "rust-service-gate": ".github/workflows/rust-service-gate.yml",
    "p0-gateway-exact-reserve-gate": ".github/workflows/p0-gateway-exact-reserve-gate.yml",
    "p0-execution-settlement-gate": ".github/workflows/p0-execution-settlement-gate.yml",
    "p0-provider-reconciliation-gate": ".github/workflows/p0-provider-reconciliation-gate.yml",
}
EXPECTED_JOBS = {
    "p0-migration-gate": {"fresh-postgres-migrations"},
    "rust-service-gate": {
        "repository-integrity",
        "service-local-gate-windows",
        "service-local-gate-linux",
        "hepta-postgres-integration",
    },
    "p0-gateway-exact-reserve-gate": {"gateway-exact-reserve"},
    "p0-execution-settlement-gate": {"execution-settlement"},
    "p0-provider-reconciliation-gate": {"provider-reconciliation"},
}
REQUIRED_WORKFLOW_MARKERS = (
    "Revalidate frozen latest hosted snapshot after exact-job verification",
    "Revalidate frozen latest hosted snapshot before manifest",
    "Revalidate frozen latest hosted snapshot after manifest publication",
)


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def positive_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and value > 0


def require_equal(actual: Any, expected: Any, label: str) -> None:
    if actual != expected:
        raise SystemExit(f"{label} mismatch: expected {expected!r}, got {actual!r}")


def load_selector_module() -> Any:
    module_name = "cex_hosted_gate_checker_for_snapshot_freshness"
    sys.modules.pop(module_name, None)
    spec = importlib.util.spec_from_file_location(module_name, HOSTED_GATE_CHECKER)
    if spec is None or spec.loader is None:
        raise SystemExit("cannot load the hosted-gate latest-run selector")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    return module


def validate_selector_contract(module: Any) -> None:
    required = getattr(module, "REQUIRED_GATES", None)
    events = getattr(module, "AUTHORITATIVE_EVENTS", None)
    paged = getattr(module, "paged_collection", None)
    latest = getattr(module, "latest_run_states", None)
    if not isinstance(required, dict):
        raise SystemExit("hosted-gate selector lacks REQUIRED_GATES")
    expected_by_path = {
        WORKFLOW_PATHS[name]: jobs for name, jobs in EXPECTED_JOBS.items()
    }
    actual_by_path = {
        path: set(job_contracts)
        for path, job_contracts in required.items()
        if isinstance(path, str) and isinstance(job_contracts, dict)
    }
    require_equal(actual_by_path, expected_by_path, "hosted-gate selector contract")
    require_equal(events, {"push", "workflow_dispatch"}, "authoritative event set")
    if not callable(paged) or not callable(latest):
        raise SystemExit("hosted-gate selector lacks pagination/latest-run helpers")


def validate_context(
    context: Any,
) -> tuple[str, str, str, str, dict[str, dict[str, Any]]]:
    if not isinstance(context, dict):
        raise SystemExit("release context must be an object")

    repository = context.get("repository")
    branch = context.get("branch")
    sha = context.get("commit_sha")
    tree = context.get("tree_sha")
    hosted = context.get("hosted_gates")
    if not isinstance(repository, str) or not repository:
        raise SystemExit("release context repository is invalid")
    if not isinstance(branch, str) or not branch or branch.startswith("refs/"):
        raise SystemExit("release context branch is invalid")
    if not isinstance(sha, str) or not GIT_SHA_RE.fullmatch(sha):
        raise SystemExit("release context commit_sha is invalid")
    if not isinstance(tree, str) or not GIT_SHA_RE.fullmatch(tree):
        raise SystemExit("release context tree_sha is invalid")
    if not isinstance(hosted, dict) or set(hosted) != set(EXPECTED_JOBS):
        raise SystemExit("release context hosted gate set is invalid")

    for gate_name, workflow_path in WORKFLOW_PATHS.items():
        gate = hosted.get(gate_name)
        if not isinstance(gate, dict):
            raise SystemExit(f"hosted gate record is invalid: {gate_name}")
        require_equal(gate.get("repository"), repository, f"{gate_name}.repository")
        require_equal(gate.get("branch"), branch, f"{gate_name}.branch")
        require_equal(gate.get("head_branch"), branch, f"{gate_name}.head_branch")
        require_equal(gate.get("head_sha"), sha, f"{gate_name}.head_sha")
        require_equal(gate.get("workflow_path"), workflow_path, f"{gate_name}.workflow_path")
        require_equal(gate.get("status"), "completed", f"{gate_name}.status")
        require_equal(gate.get("conclusion"), "success", f"{gate_name}.conclusion")
        if not positive_int(gate.get("run_id")) or not positive_int(
            gate.get("run_attempt")
        ):
            raise SystemExit(f"{gate_name} has an invalid run identity")

    selection = context.get("hosted_gate_selection")
    if not isinstance(selection, dict):
        raise SystemExit("release context lacks hosted_gate_selection")
    require_equal(
        selection.get("schema"),
        "cex.hosted-gate-selection-binding.v1",
        "hosted_gate_selection.schema",
    )
    require_equal(
        selection.get("policy"),
        SELECTION_POLICY,
        "hosted_gate_selection.policy",
    )
    require_equal(
        selection.get("source"),
        HOSTED_GATE_ATTESTATION,
        "hosted_gate_selection.source",
    )
    digest = selection.get("sha256")
    if not isinstance(digest, str) or not SHA256_RE.fullmatch(digest):
        raise SystemExit("hosted_gate_selection.sha256 is invalid")

    files = context.get("files")
    if not isinstance(files, dict):
        raise SystemExit("release context files map is invalid")
    require_equal(
        files.get(HOSTED_GATE_ATTESTATION),
        digest,
        "hosted gate selection source digest",
    )

    selected_ids = selection.get("selected_run_ids")
    if not isinstance(selected_ids, dict) or set(selected_ids) != set(EXPECTED_JOBS):
        raise SystemExit("hosted_gate_selection selected run-id set is invalid")
    for gate_name, gate in hosted.items():
        require_equal(
            selected_ids.get(gate_name),
            gate.get("run_id"),
            f"hosted_gate_selection.selected_run_ids.{gate_name}",
        )

    attestations = context.get("attestations")
    if not isinstance(attestations, dict):
        raise SystemExit("release context lacks attestation bindings")
    hosted_attestation = attestations.get("hosted-gate-execution")
    if not isinstance(hosted_attestation, dict):
        raise SystemExit("release context lacks hosted-gate-execution attestation")
    require_equal(
        hosted_attestation.get("path"),
        HOSTED_GATE_ATTESTATION,
        "hosted-gate-execution attestation path",
    )
    require_equal(
        hosted_attestation.get("sha256"),
        digest,
        "hosted-gate-execution attestation digest",
    )
    return repository, branch, sha, tree, hosted


def verify_latest_snapshot(
    context: dict[str, Any],
    token: str,
    *,
    selector_module: Any | None = None,
    runs_override: list[dict[str, Any]] | None = None,
) -> dict[str, dict[str, Any]]:
    """Compare the frozen context to current latest state without substitution."""

    repository, branch, sha, _tree, hosted = validate_context(context)
    selector = selector_module if selector_module is not None else load_selector_module()
    validate_selector_contract(selector)

    if runs_override is None:
        runs = selector.paged_collection(
            f"https://api.github.com/repos/{repository}/actions/runs",
            token,
            "workflow_runs",
            query={"branch": branch, "head_sha": sha},
        )
    else:
        runs = [dict(item) for item in runs_override]

    selected, pending, failures = selector.latest_run_states(runs, branch, sha)
    if failures:
        raise SystemExit(
            "frozen snapshot was superseded by a failed latest authoritative run: "
            + ", ".join(failures)
        )
    if pending:
        raise SystemExit(
            "frozen snapshot was superseded by a pending or missing latest authoritative run: "
            + ", ".join(pending)
        )
    if set(selected) != set(WORKFLOW_PATHS.values()):
        raise SystemExit("latest authoritative hosted workflow set is incomplete")

    normalized: dict[str, dict[str, Any]] = {}
    for gate_name, workflow_path in WORKFLOW_PATHS.items():
        frozen = hosted[gate_name]
        latest = selected[workflow_path]
        latest_id = latest.get("id")
        latest_attempt = latest.get("run_attempt")
        if not positive_int(latest_id) or not positive_int(latest_attempt):
            raise SystemExit(f"{gate_name} latest run identity is invalid")
        if (
            latest_id != frozen.get("run_id")
            or latest_attempt != frozen.get("run_attempt")
        ):
            raise SystemExit(
                f"{gate_name} selected run/attempt is no longer latest: "
                f"frozen={frozen.get('run_id')}/attempt-{frozen.get('run_attempt')} "
                f"latest={latest_id}/attempt-{latest_attempt}"
            )
        require_equal(latest.get("path"), workflow_path, f"{gate_name}.latest.path")
        require_equal(latest.get("head_branch"), branch, f"{gate_name}.latest.branch")
        require_equal(latest.get("head_sha"), sha, f"{gate_name}.latest.sha")
        if latest.get("event") not in {"push", "workflow_dispatch"}:
            raise SystemExit(f"{gate_name} latest run event is not authoritative")
        require_equal(latest.get("status"), "completed", f"{gate_name}.latest.status")
        require_equal(
            latest.get("conclusion"),
            "success",
            f"{gate_name}.latest.conclusion",
        )
        normalized[gate_name] = {
            "workflow_path": workflow_path,
            "run_id": latest_id,
            "run_attempt": latest_attempt,
            "event": latest.get("event"),
            "head_branch": latest.get("head_branch"),
            "head_sha": latest.get("head_sha"),
            "status": latest.get("status"),
            "conclusion": latest.get("conclusion"),
            "created_at": latest.get("created_at"),
            "updated_at": latest.get("updated_at"),
        }
    return normalized


def validate_workflow_wiring() -> list[str]:
    failures: list[str] = []
    try:
        text = CANDIDATE_WORKFLOW.read_text(encoding="utf-8")
    except OSError as error:
        return [f"cannot read candidate workflow: {error}"]
    for marker in REQUIRED_WORKFLOW_MARKERS:
        if marker not in text:
            failures.append(f"candidate workflow lacks freshness marker: {marker}")
    command = "python3 scripts/verify-hosted-snapshot-freshness.py"
    if text.count(command) != len(REQUIRED_WORKFLOW_MARKERS):
        failures.append(
            "candidate workflow must invoke snapshot freshness exactly three times"
        )
    return failures


def self_test() -> list[str]:
    failures = validate_workflow_wiring()
    try:
        selector = load_selector_module()
        validate_selector_contract(selector)
        repository = "TrillionniumFoundation/CEX"
        branch = "candidate/freshness"
        sha = "a" * 40
        tree = "b" * 40
        digest = "sha256:" + "c" * 64

        hosted: dict[str, dict[str, Any]] = {}
        selected_ids: dict[str, int] = {}
        runs: list[dict[str, Any]] = []
        for index, (gate_name, workflow_path) in enumerate(
            WORKFLOW_PATHS.items(), start=1
        ):
            run_id = 100 + index
            hosted[gate_name] = {
                "repository": repository,
                "branch": branch,
                "head_branch": branch,
                "head_sha": sha,
                "workflow_path": workflow_path,
                "status": "completed",
                "conclusion": "success",
                "run_id": run_id,
                "run_attempt": 1,
            }
            selected_ids[gate_name] = run_id
            runs.append(
                {
                    "id": run_id,
                    "path": workflow_path,
                    "head_branch": branch,
                    "head_sha": sha,
                    "event": "push",
                    "status": "completed",
                    "conclusion": "success",
                    "run_attempt": 1,
                    "created_at": f"2026-08-30T00:00:{index:02d}Z",
                    "updated_at": f"2026-08-30T00:01:{index:02d}Z",
                }
            )
        context = {
            "repository": repository,
            "branch": branch,
            "commit_sha": sha,
            "tree_sha": tree,
            "hosted_gates": hosted,
            "files": {HOSTED_GATE_ATTESTATION: digest},
            "attestations": {
                "hosted-gate-execution": {
                    "path": HOSTED_GATE_ATTESTATION,
                    "sha256": digest,
                }
            },
            "hosted_gate_selection": {
                "schema": "cex.hosted-gate-selection-binding.v1",
                "policy": SELECTION_POLICY,
                "source": HOSTED_GATE_ATTESTATION,
                "sha256": digest,
                "selected_run_ids": selected_ids,
            },
        }

        verify_latest_snapshot(
            context,
            "unused",
            selector_module=selector,
            runs_override=runs,
        )

        first_gate = next(iter(WORKFLOW_PATHS))
        first_path = WORKFLOW_PATHS[first_gate]
        base = next(run for run in runs if run["path"] == first_path)
        negative_cases = {
            "newer failure": {
                **base,
                "id": 900,
                "conclusion": "failure",
                "created_at": "2026-08-30T01:00:00Z",
                "updated_at": "2026-08-30T01:01:00Z",
            },
            "newer active": {
                **base,
                "id": 901,
                "status": "in_progress",
                "conclusion": None,
                "created_at": "2026-08-30T02:00:00Z",
                "updated_at": "2026-08-30T02:01:00Z",
            },
            "newer success": {
                **base,
                "id": 902,
                "created_at": "2026-08-30T03:00:00Z",
                "updated_at": "2026-08-30T03:01:00Z",
            },
            "newer rerun attempt": {
                **base,
                "run_attempt": 2,
                "updated_at": "2026-08-30T04:01:00Z",
            },
        }
        for label, newer in negative_cases.items():
            try:
                verify_latest_snapshot(
                    context,
                    "unused",
                    selector_module=selector,
                    runs_override=[*runs, newer],
                )
            except SystemExit:
                pass
            else:
                failures.append(f"freshness self-test accepted {label}")

        bad_binding = json.loads(json.dumps(context))
        bad_binding["hosted_gate_selection"]["selected_run_ids"][first_gate] = 999
        try:
            validate_context(bad_binding)
        except SystemExit:
            pass
        else:
            failures.append("freshness self-test accepted a mismatched selection binding")
    except Exception as error:
        failures.append(f"snapshot freshness self-test crashed: {error}")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--context", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    failures = self_test()
    if failures:
        raise SystemExit("hosted snapshot freshness self-test failed: " + "; ".join(failures))
    if args.self_test:
        print(
            json.dumps(
                {
                    "schema": "cex.hosted-snapshot-freshness-self-test.v1",
                    "status": "ok",
                    "ok": True,
                    "negative_cases": [
                        "newer-failure",
                        "newer-active",
                        "newer-success",
                        "newer-rerun-attempt",
                        "selection-binding-mismatch",
                    ],
                    "workflow_revalidation_count": 3,
                },
                indent=2,
                sort_keys=True,
            )
        )
        return 0
    if args.context is None:
        raise SystemExit("--context is required unless --self-test is used")

    token = os.environ.get("GITHUB_TOKEN", "")
    if not token:
        raise SystemExit("GITHUB_TOKEN is required")
    try:
        context = json.loads(args.context.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"cannot read release context: {error}") from error
    repository, branch, sha, tree, _hosted = validate_context(context)
    snapshot = verify_latest_snapshot(context, token)
    result = {
        "schema": "cex.hosted-snapshot-freshness-verification.v1",
        "status": "ok",
        "ok": True,
        "repository": repository,
        "branch": branch,
        "commit_sha": sha,
        "tree_sha": tree,
        "selection_policy": SELECTION_POLICY,
        "verified_at": utc_now(),
        "selected_runs": snapshot,
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
