#!/usr/bin/env python3
"""Self-test strict release-evidence wiring and negative candidate cases."""

from __future__ import annotations

import copy
import importlib.util
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/p0-release-candidate-gate.yml"
STRICT_WRAPPER = ROOT / "scripts/p0-release-evidence-strict.py"
CANONICAL_GENERATOR = ROOT / "scripts/p0-release-evidence.py"
LOCAL_BINDER = ROOT / "scripts/bind-p0-local-evidence.py"
HOSTED_CHECKER = ROOT / "scripts/check-hosted-gate-execution.py"
HOSTED_CHECKER_IMPL = ROOT / "scripts/check-hosted-gate-execution-impl.py"
EXECUTION_VERIFIER = ROOT / "scripts/verify-hosted-run-execution.py"
SNAPSHOT_FRESHNESS = ROOT / "scripts/verify-hosted-snapshot-freshness.py"
SNAPSHOT_FRESHNESS_IMPL = ROOT / "scripts/verify-hosted-snapshot-freshness-impl.py"
EVIDENCE_CORE = ROOT / "scripts/p0-release-evidence-core.py"
EVIDENCE_CORE_IMPL = ROOT / "scripts/p0-release-evidence-core-impl.py"
CONTRACT = ROOT / "scripts/check-release-evidence-contract.py"
PRIMARY_CONTRACT = ROOT / "scripts/check-release-baseline-manifest.py"
TEMPLATE = ROOT / "docs/templates/cex-release-baseline-manifest-v1.json"
SCHEMA = ROOT / "docs/schemas/cex-release-baseline-manifest-v1.schema.json"
PROBLEMS: list[str] = []

RELEASE_WRAPPER_BOUNDARIES = {
    HOSTED_CHECKER: (
        HOSTED_CHECKER_IMPL,
        ("authoritative_run_sort_key", "_IMPLEMENTATION_SELF_TEST = self_test"),
        ("latest_authoritative_run_is_binding", "build_frozen_attestation"),
    ),
    EVIDENCE_CORE: (
        EVIDENCE_CORE_IMPL,
        ("authoritative_run_sort_key", "_IMPLEMENTATION_SELF_TEST = self_test"),
        ("tree_sha = payload.get(\"tree_sha\")", "def revalidate_gate_runs("),
    ),
    SNAPSHOT_FRESHNESS: (
        SNAPSHOT_FRESHNESS_IMPL,
        ("authoritative_order_self_test", "_IMPLEMENTATION_SELF_TEST = self_test"),
        ("latest_authoritative_run_is_binding", "detail-status-drift"),
    ),
}


def require_file(path: Path) -> str:
    if not path.is_file():
        PROBLEMS.append(f"missing required file: {path.relative_to(ROOT).as_posix()}")
        return ""
    return path.read_text(encoding="utf-8")


def require_markers(path: Path, *markers: str) -> None:
    text = require_file(path)
    for marker in markers:
        if marker not in text:
            PROBLEMS.append(
                f"{path.relative_to(ROOT).as_posix()} lacks strict evidence marker: {marker}"
            )


def verify_failure_diagnostics_isolation() -> None:
    """Reject failure uploads that enter the frozen canonical payload tree."""

    content = require_file(WORKFLOW)
    if not content:
        return
    lines = content.splitlines()
    starts = [
        index
        for index, line in enumerate(lines)
        if re.match(r"^\s*-\s+name:\s*Upload failure diagnostics\s*$", line)
    ]
    if len(starts) != 1:
        PROBLEMS.append(
            "p0-release-candidate-gate.yml must contain exactly one "
            "Upload failure diagnostics step"
        )
        return
    start = starts[0]
    step_indent = len(lines[start]) - len(lines[start].lstrip())
    end = len(lines)
    for index in range(start + 1, len(lines)):
        line = lines[index]
        indent = len(line) - len(line.lstrip())
        if indent <= step_indent and re.match(r"^\s*-\s+name:\s*", line):
            end = index
            break
    body = "\n".join(lines[start + 1 : end])
    if not re.search(r"(?m)^\s*if:\s*failure\(\)\s*$", body):
        PROBLEMS.append(
            "failure diagnostics upload must remain conditional on failure()"
        )
    path_values = re.findall(
        r"(?m)^\s*path:\s*([^#\s]+)\s*(?:#.*)?$", body
    )
    if path_values != ["run/p0-release-support"]:
        PROBLEMS.append(
            "failure diagnostics upload must use only run/p0-release-support; "
            f"got {path_values!r}"
        )


def verify_release_wrapper_boundaries() -> None:
    """Ensure split release adapters execute their checked-in implementations."""

    generic = (
        'globals()["__name__"] = f"{_ORIGINAL_MODULE_NAME}.__impl__"',
        "_SOURCE = read_regular_nofollow(_IMPL_PATH)",
        'exec(compile(_SOURCE, str(_IMPL_PATH), "exec", dont_inherit=True), globals())',
        'globals()["__name__"] = _ORIGINAL_MODULE_NAME',
        "read_regular_nofollow",
    )
    for wrapper, (implementation, wrapper_markers, implementation_markers) in RELEASE_WRAPPER_BOUNDARIES.items():
        wrapper_name = wrapper.relative_to(ROOT).as_posix()
        implementation_name = implementation.relative_to(ROOT).as_posix()
        if wrapper.is_symlink() or not wrapper.is_file():
            PROBLEMS.append(f"release-evidence wrapper is not a regular file: {wrapper_name}")
            continue
        if implementation.is_symlink() or not implementation.is_file():
            PROBLEMS.append(
                f"release-evidence implementation is not a regular file: {implementation_name}"
            )
            continue
        wrapper_text = wrapper.read_text(encoding="utf-8")
        implementation_text = implementation.read_text(encoding="utf-8")
        expected_assignment = f'_IMPL_PATH = _SCRIPT_DIR / "{implementation.name}"'
        if wrapper_text.count(expected_assignment) != 1:
            PROBLEMS.append(
                f"{wrapper_name} must assign its implementation exactly once: {expected_assignment}"
            )
        for marker in generic + wrapper_markers:
            if marker not in wrapper_text:
                PROBLEMS.append(f"{wrapper_name} lacks wrapper boundary marker: {marker}")
        for marker in implementation_markers:
            if marker not in implementation_text:
                PROBLEMS.append(f"{implementation_name} lacks implementation marker: {marker}")


def load_contract_module() -> Any:
    spec = importlib.util.spec_from_file_location("cex_release_contract", CONTRACT)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load strict release evidence contract")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def load_execution_module() -> Any:
    spec = importlib.util.spec_from_file_location(
        "cex_hosted_execution_verifier", EXECUTION_VERIFIER
    )
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load hosted execution verifier")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def load_strict_module() -> Any:
    spec = importlib.util.spec_from_file_location(
        "cex_release_evidence_strict_wrapper",
        ROOT / "scripts/p0-release-evidence-strict.py",
    )
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load strict evidence wrapper")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def git_value(*args: str) -> str:
    """Read immutable checkout metadata used by the strict contract fixtures."""

    return subprocess.check_output(
        ["git", *args], cwd=ROOT, text=True, stderr=subprocess.STDOUT
    ).strip()


def checkout_metadata(module: Any) -> dict[str, str]:
    """Return metadata from this checkout, never synthetic placeholder values."""

    migration_head, migration_sha256, migration_chain_sha256 = module.migration_state(ROOT)
    return {
        "repository": "TrillionniumFoundation/CEX",
        "branch": git_value("branch", "--show-current"),
        "commit_sha": git_value("rev-parse", "HEAD"),
        "tree_sha": git_value("rev-parse", "HEAD^{tree}"),
        "cargo_lock_sha256": module.sha256_file(ROOT / "Cargo.lock"),
        "migration_head": migration_head,
        "migration_sha256": migration_sha256,
        "migration_chain_sha256": migration_chain_sha256,
    }


def valid_manifest(module: Any) -> dict[str, Any]:
    metadata = checkout_metadata(module)
    sha = metadata["commit_sha"]
    tree = metadata["tree_sha"]
    digest = "sha256:" + "3" * 64
    payload = f"cex-p0-evidence-{sha}-attempt-1"
    local_paths = {
        **dict(module.LOCAL_EVIDENCE),
        **dict(module.ATTESTATION_EVIDENCE),
    }
    workflow_run_id = 9001
    evidence = []
    for index, name in enumerate(module.EXPECTED_EVIDENCE_ORDER, start=1):
        if name.startswith("hosted:"):
            gate_run_id = 9100 + index
            uri = f"gh://{metadata['repository']}/actions/runs/{gate_run_id}/attempts/1"
        else:
            uri = f"artifact://{payload}/{local_paths[name]}"
        evidence.append(
            {
                "name": name,
                "status": "pass",
                "uri": uri,
                "sha256": digest,
                "waiver": None,
            }
        )
    return {
        "schema": "cex.release-baseline-manifest.v1",
        "status": "candidate",
        "qualification_scope": module.QUALIFICATION_SCOPE,
        "production_ready": False,
        "production_authorization": "not_granted",
        "project_id": "hepta-control-plane",
        "release_id": f"cex-p0-{sha}",
        "generated_at": "2026-08-30T00:00:00Z",
        "source": {
            "repository": metadata["repository"],
            "branch": metadata["branch"],
            "commit_sha": sha,
            "tree_sha": tree,
        },
        "dependencies": {"cargo_lock_sha256": metadata["cargo_lock_sha256"]},
        "database": {
            "migration_head": metadata["migration_head"],
            "migration_sha256": metadata["migration_sha256"],
            "migration_chain_sha256": metadata["migration_chain_sha256"],
        },
        "build": {
            "workflow_run_id": workflow_run_id,
            "artifacts": [
                {
                    "name": payload,
                    "uri": f"gh://{metadata['repository']}/actions/runs/{workflow_run_id}/attempts/1/artifacts/{payload}",
                    "sha256": digest,
                }
            ],
            "images": [],
            "sbom": {
                "name": "sbom.spdx.json",
                "uri": f"artifact://{payload}/sbom.spdx.json",
                "sha256": digest,
            },
            "provenance": {
                "name": "provenance.intoto.json",
                "uri": f"artifact://{payload}/provenance.intoto.json",
                "sha256": digest,
            },
        },
        "evidence": evidence,
        "approvals": [
            {
                "role": "repository-qualification-automation",
                "actor": "github-actions[bot]",
                "decision": "approve",
                "decided_at": "2026-08-30T00:00:00Z",
                "scope": module.APPROVAL_SCOPE,
            }
        ],
        "external_gates": {
            "status": "independent_approval_required",
            "items": list(module.EXPECTED_EXTERNAL_GATES),
        },
        "revocation": None,
    }


def valid_context(module: Any, manifest: dict[str, Any]) -> dict[str, Any]:
    """Build a complete collector context bound to the real checkout."""

    metadata = checkout_metadata(module)
    source = manifest["source"]
    workflow_run_id = manifest["build"]["workflow_run_id"]
    workflow_run_attempt = 1
    digest = "sha256:" + "3" * 64
    context: dict[str, Any] = {
        **metadata,
        "workflow_run_id": workflow_run_id,
        "workflow_run_attempt": workflow_run_attempt,
        "payload_name": manifest["build"]["artifacts"][0]["name"],
        "payload_digest": manifest["build"]["artifacts"][0]["sha256"],
        "server_url": "https://github.com",
        "generated_at": "2026-08-30T00:00:00Z",
        "files": {},
        "hosted_gates": {},
        "qualification_scope": module.QUALIFICATION_SCOPE,
    }
    for index, gate_name in enumerate(module.HOSTED_GATE_NAMES, start=1):
        gate_run_id = 9100 + index
        relative = f"hosted-gates/{gate_name}.json"
        context["files"][relative] = digest
        context["hosted_gates"][gate_name] = {
            "repository": source["repository"],
            "branch": source["branch"],
            "head_branch": source["branch"],
            "head_sha": source["commit_sha"],
            "workflow_path": module.HOSTED_WORKFLOW_PATHS[gate_name],
            "event": "push",
            "status": "completed",
            "conclusion": "success",
            "run_id": gate_run_id,
            "run_attempt": workflow_run_attempt,
            "created_at": "2026-08-30T00:00:01Z",
            "updated_at": "2026-08-30T00:00:02Z",
            "sha256": digest,
        }
    for relative in module.LOCAL_EVIDENCE.values():
        context["files"][relative] = digest
    for relative in module.PAYLOAD_ONLY_EVIDENCE.values():
        context["files"][relative] = digest
    for relative in module.ATTESTATION_EVIDENCE.values():
        context["files"][relative] = digest
    context["payload_only_attestations"] = {
        relative: digest for relative in module.PAYLOAD_ONLY_EVIDENCE.values()
    }
    context["attestations"] = {
        name: {"path": relative, "sha256": digest}
        for name, relative in module.ATTESTATION_EVIDENCE.items()
    }
    hosted_relative = module.ATTESTATION_EVIDENCE["hosted-gate-execution"]
    context["hosted_gate_selection"] = {
        "schema": "cex.hosted-gate-selection-binding.v1",
        "policy": "latest_authoritative_run_is_binding",
        "source": hosted_relative,
        "sha256": context["files"][hosted_relative],
        "selected_run_ids": {
            gate_name: context["hosted_gates"][gate_name]["run_id"]
            for gate_name in module.HOSTED_GATE_NAMES
        },
    }
    context["files"]["sbom.spdx.json"] = digest
    context["files"]["provenance.intoto.json"] = digest
    return context


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def local_fixture(name: str, module: Any, source: dict[str, str]) -> dict[str, Any]:
    """Return the smallest valid producer payload for ordinary local evidence."""

    common = {
        "schema": module.LOCAL_SCHEMAS[name],
        "status": "ok",
        "ok": True,
        "commit_sha": source["commit_sha"],
        "tree_sha": source["tree_sha"],
    }
    if name == "candidate-hygiene":
        workflow_scope = module.canonical_workflow_scope(ROOT)
        return {
            **common,
            "problems": [],
            # These values mirror check-p0-release-candidate-hygiene-core.py;
            # keeping them in the fixture exercises the same canonical
            # authority/workflow binding as a hosted producer record.
            "plan": module.CANDIDATE_ACTIVE_PLAN,
            "addendum": module.CANDIDATE_ACTIVE_ADDENDUM,
            "authoritative_workflows": list(module.HOSTED_WORKFLOW_PATHS.values()),
            "release_workflow": module.EXPECTED_AGGREGATE_RELEASE_WORKFLOW,
            "workflow_pin_scope": workflow_scope,
            "shared_trigger": module.TRIGGER_PATH,
            "documentation_contract": "ok",
            "migration_head": module.migration_state(ROOT)[0],
            "workflow_trust": {
                "status": "ok",
                "workflow_count": len(workflow_scope),
                "local_action_descriptor_count": 0,
            },
            "candidate_trigger_authority": {
                "path": "docs/release-evidence/p0-candidate-trigger.json",
                "sole_authority": True,
                "secondary_freeze_markers": [],
            },
        }
    if name == "repository-integrity":
        repository_root = Path(module.__file__).parent.parent
        integrity_digests = module.repository_integrity_expected_digests(repository_root)
        return {
            **common,
            "repository_commit_sha": source["commit_sha"],
            "repository_tree_sha": source["tree_sha"],
            "generated_at": "2026-08-30T00:00:00Z",
            "active_plan": "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md",
            "active_addendum": "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md",
            "migration_head": "0088_enforce_provider_terminal_evidence_binding.sql",
            "production_authorization": "not_granted",
            "digests": integrity_digests,
            "documentation_check": {
                "schema": "cex.development-doc-check.v1",
                "status": "ok",
                "active_plan": "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md",
                "active_addendum": "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md",
                "migration_head": "0088_enforce_provider_terminal_evidence_binding.sql",
                "requirements": 18,
                "repository_qualification_result": "PENDING_EXACT_SHA_HOSTED_EVIDENCE",
                "repository_qualification_authority": "generated_candidate_manifest_only",
                "production_authorization": "not_granted",
                "problems": [],
            },
        }
    if name == "hepta-postgres-integration":
        return {
            **common,
            # The aggregate release workflow intentionally records its
            # recovery-only rerun; the Rust service gate separately proves
            # full mode through its authoritative hosted job.
            "mode": "recovery-only",
            "postgres_required": True,
            "lint_policy": module.HEPTA_LINT_POLICY,
            "completed_at": "2026-08-30T00:00:00Z",
            "checks": sorted(module.HEPTA_REQUIRED_CHECKS),
        }
    if name == "migration-and-lifecycle-matrix":
        return {
            "schema": module.LOCAL_SCHEMAS[name],
            "ok": True,
            "commit_sha": source["commit_sha"],
            "tree_sha": source["tree_sha"],
            "completed_at": "2026-08-30T00:00:00Z",
            "checks": sorted(module.LIFECYCLE_REQUIRED_CHECKS),
        }
    if name == "exact-ledger-soak":
        iterations = module.MIN_EXACT_SOAK_ITERATIONS
        entry_count = 1 + iterations * module.EXACT_SOAK_LEDGER_ENTRIES_PER_ITERATION
        return {
            "schema": module.LOCAL_SCHEMAS[name],
            "ok": True,
            "iterations": iterations,
            "account_id": module.EXACT_SOAK_ACCOUNT_ID,
            "balance_minor": (
                module.EXACT_SOAK_INITIAL_BALANCE_MINOR
                + iterations * module.EXACT_SOAK_GRANT_MINOR_PER_ITERATION
            ),
            "reserved_minor": 0,
            "ledger_entry_count": entry_count,
            "distinct_operation_count": entry_count,
            "compatibility_entry_count": 0,
            "audit_effect_count": entry_count,
            "started_at_epoch": 1,
            "ended_at_epoch": 2,
            "duration_seconds": 1,
            "commit_sha": source["commit_sha"],
            "tree_sha": source["tree_sha"],
        }
    if name == "backup-restore":
        restore_state = {
            "public_table_count": 1,
            "organization_count": 1,
            "account_count": 1,
            "ledger_entry_count": 0,
            "audit_outbox_count": 0,
            "soak_account": {
                "account_id": "90000000-0000-4000-8000-000000000101",
                "balance_minor": 0,
                "reserved_minor": 0,
                "currency_unit": "TRNM",
                "currency_scale": 6,
            },
            "soak_operation_count": 0,
            "soak_ledger_sha256": "sha256:" + "4" * 64,
            "soak_audit_sha256": "sha256:" + "4" * 64,
        }
        return {
            "schema": module.LOCAL_SCHEMAS[name],
            "ok": True,
            "source": restore_state,
            "restored": restore_state,
            "dump_sha256": "sha256:" + "4" * 64,
            "dump_bytes": 1,
            "archive_items": 1,
            "started_at_epoch": 1,
            "ended_at_epoch": 2,
            "duration_seconds": 1,
            "commit_sha": source["commit_sha"],
            "tree_sha": source["tree_sha"],
            "restore_database_retained": False,
        }
    raise AssertionError(f"unknown local fixture: {name}")


def hosted_fixture(module: Any, context: dict[str, Any], gate_name: str) -> dict[str, Any]:
    """Materialize one hosted-gate attestation exactly as the collector does."""

    record = context["hosted_gates"][gate_name]
    return {
        "schema": module.HOSTED_GATE_SCHEMA,
        "name": gate_name,
        "repository": record["repository"],
        "branch": record["branch"],
        "head_branch": record["head_branch"],
        "head_sha": record["head_sha"],
        "workflow_path": record["workflow_path"],
        "event": record["event"],
        "status": record["status"],
        "conclusion": record["conclusion"],
        "run_id": record["run_id"],
        "run_attempt": record["run_attempt"],
        "created_at": record["created_at"],
        "updated_at": record["updated_at"],
    }


def governance_fixture(module: Any, context: dict[str, Any], source: dict[str, Any]) -> dict[str, Any]:
    """Materialize a coherent, non-production repository-governance observation."""

    desired = [
        "fresh-postgres-migrations",
        "repository-integrity",
        "service-local-gate-linux",
        "service-local-gate-windows",
        "hepta-postgres-integration",
        "gateway-exact-reserve",
        "execution-settlement",
        "provider-reconciliation",
        "repository-candidate-qualification",
    ]
    return {
        "schema": module.GOVERNANCE_SCHEMA,
        "ok": True,
        "repository": source["repository"],
        "commit_sha": source["commit_sha"],
        "tree_sha": source["tree_sha"],
        "candidate_branch": source["branch"],
        "candidate_branch_commit_sha": source["commit_sha"],
        "candidate_branch_commit_sha_final": source["commit_sha"],
        "candidate_commit_matches_branch": True,
        "candidate_branch_stable_during_observation": True,
        "candidate_tree_matches_commit": True,
        "production_authorization": "not_granted",
        "observed_at": "2026-08-30T00:00:00Z",
        "default_branch": "main",
        "default_branch_protected": False,
        "candidate_branch_protected": False,
        "branch_protection_enabled": False,
        "candidate_branch_protection_enabled": False,
        "default_branch_protection_enabled": False,
        "default_branch_required_status_contexts": [],
        "candidate_legacy_required_checks_enforced": False,
        "candidate_ruleset_required_checks_enforced": False,
        "required_candidate_checks_enforced": False,
        "rulesets_http_status": 200,
        "rulesets_readable": True,
        "rulesets": [],
        "candidate_rulesets": [],
        "ruleset_count": 0,
        "candidate_ruleset_count": 0,
        "candidate_required_status_contexts": sorted(desired),
        "actual_required_status_contexts": sorted(desired),
        "desired_required_status_contexts": desired,
        "repository_candidate_enforcement": "not_enforced",
        "interpretation": "This is an observation of GitHub controls. Source files and CI prose do not create branch protection or ruleset enforcement.",
    }


def execution_fixture(
    module: Any,
    execution: Any,
    context: dict[str, Any],
    source: dict[str, Any],
) -> dict[str, Any]:
    """Build normalized exact-run/job evidence without contacting GitHub."""

    gates: dict[str, Any] = {}
    next_job_id = 200_000
    for gate_name in module.HOSTED_GATE_NAMES:
        context_gate = context["hosted_gates"][gate_name]
        normalized_jobs: list[dict[str, Any]] = []
        for job_name in sorted(execution.EXPECTED_JOBS[gate_name]):
            expected = execution.EXPECTED_JOBS[gate_name][job_name]
            next_job_id += 1
            raw_job = {
                "id": next_job_id,
                "run_id": context_gate["run_id"],
                "name": job_name,
                "head_sha": source["commit_sha"],
                "run_attempt": context_gate["run_attempt"],
                "status": "completed",
                "conclusion": "success",
                "runner_id": next_job_id + 1_000_000,
                "runner_name": f"fixture-runner-{next_job_id}",
                "runner_group_id": None,
                "runner_group_name": None,
                "started_at": "2026-08-30T00:00:01Z",
                "completed_at": "2026-08-30T00:00:02Z",
                "labels": [expected["runner_label"]],
                "steps": [
                    {
                        "name": step_name,
                        "number": step_number,
                        "status": "completed",
                        "conclusion": "success",
                        "started_at": "2026-08-30T00:00:01Z",
                        "completed_at": "2026-08-30T00:00:02Z",
                    }
                    for step_number, step_name in enumerate(sorted(expected["steps"]), start=1)
                ],
            }
            normalized_jobs.append(
                execution.validate_job(
                    gate_name,
                    raw_job,
                    sha=source["commit_sha"],
                    run_id=context_gate["run_id"],
                    run_attempt=context_gate["run_attempt"],
                    expected=expected,
                )
            )
        normalized_jobs.sort(key=lambda item: item["name"])
        gates[gate_name] = {
            "repository": source["repository"],
            "branch": source["branch"],
            "head_branch": source["branch"],
            "head_sha": source["commit_sha"],
            "event": context_gate["event"],
            "run_id": context_gate["run_id"],
            "run_attempt": context_gate["run_attempt"],
            "workflow_path": module.HOSTED_WORKFLOW_PATHS[gate_name],
            "status": "success",
            "created_at": context_gate["created_at"],
            "updated_at": context_gate["updated_at"],
            "jobs": normalized_jobs,
            "jobs_sha256": execution.canonical_digest(normalized_jobs),
        }
    return {
        "schema": module.EXECUTION_SCHEMA,
        "status": "ok",
        "ok": True,
        "repository": source["repository"],
        "branch": source["branch"],
        "commit_sha": source["commit_sha"],
        "tree_sha": source["tree_sha"],
        "verified_at": "2026-08-30T00:00:00Z",
        "gates": gates,
    }


def sbom_fixture(context: dict[str, Any], source: dict[str, Any]) -> dict[str, Any]:
    return {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": f"CEX P0 candidate {source['commit_sha']}",
        "documentNamespace": (
            f"https://github.com/{source['repository']}/p0-sbom/{source['commit_sha']}"
            f"/{context['workflow_run_id']}/attempt/{context['workflow_run_attempt']}"
        ),
        "creationInfo": {
            "created": "2026-08-30T00:00:00Z",
            "creators": ["Tool: cex-p0-release-evidence"],
            "licenseListVersion": "3.25",
        },
        "packages": [
            {
                "SPDXID": "SPDXRef-Package-fixture",
                "name": "fixture",
                "versionInfo": "1.0.0",
                "downloadLocation": "NOASSERTION",
                "filesAnalyzed": False,
                "licenseConcluded": "NOASSERTION",
                "licenseDeclared": "NOASSERTION",
                "copyrightText": "NOASSERTION",
                "externalRefs": [
                    {
                        "referenceCategory": "PACKAGE-MANAGER",
                        "referenceType": "purl",
                        "referenceLocator": "pkg:cargo/fixture@1.0.0",
                    }
                ],
            }
        ],
        "relationships": [
            {
                "spdxElementId": "SPDXRef-DOCUMENT",
                "relationshipType": "DESCRIBES",
                "relatedSpdxElement": "SPDXRef-Package-fixture",
            }
        ],
    }


def provenance_fixture(module: Any, context: dict[str, Any], source: dict[str, Any]) -> dict[str, Any]:
    return {
        "_type": "https://in-toto.io/Statement/v1",
        "subject": [
            {
                "name": source["repository"],
                "digest": {
                    "gitCommit": source["commit_sha"],
                    "gitTree": source["tree_sha"],
                },
            }
        ],
        "predicateType": "https://slsa.dev/provenance/v1",
        "predicate": {
            "buildDefinition": {
                "buildType": "https://github.com/TrillionniumFoundation/CEX/p0-release-candidate-gate/v1",
                "externalParameters": {
                    "repository": source["repository"],
                    "branch": source["branch"],
                    "commit_sha": source["commit_sha"],
                    "workflow_run_id": context["workflow_run_id"],
                    "workflow_run_attempt": context["workflow_run_attempt"],
                },
                "internalParameters": {"migration_head": context["migration_head"]},
                "resolvedDependencies": [
                    {
                        "uri": f"git+https://github.com/{source['repository']}@{source['commit_sha']}",
                        "digest": {
                            "gitCommit": source["commit_sha"],
                            "gitTree": source["tree_sha"],
                        },
                    },
                    {
                        "uri": "file:Cargo.lock",
                        "digest": {"sha256": context["cargo_lock_sha256"][7:]},
                    },
                    {
                        "uri": "file:migrations/",
                        "digest": {"sha256": context["migration_chain_sha256"][7:]},
                    },
                ],
            },
            "runDetails": {
                "builder": {"id": "https://github.com/actions/runner"},
                "metadata": {
                    "invocationId": (
                        f"{context['server_url']}/{source['repository']}/actions/runs/"
                        f"{context['workflow_run_id']}/attempts/{context['workflow_run_attempt']}"
                    ),
                    "startedOn": "2026-08-30T00:00:00Z",
                },
            },
        },
    }


def bind_fixture_digest(
    module: Any,
    manifest: dict[str, Any],
    context: dict[str, Any],
    relative: str,
    digest: str,
) -> None:
    """Bind one materialized file hash into both context and manifest."""

    context["files"][relative] = digest
    if relative.startswith("hosted-gates/"):
        gate_name = Path(relative).stem
        context["hosted_gates"][gate_name]["sha256"] = digest
        index = module.EXPECTED_EVIDENCE_ORDER.index(f"hosted:{gate_name}")
        manifest["evidence"][index]["sha256"] = digest
    elif relative in module.LOCAL_EVIDENCE.values():
        evidence_name = next(
            name for name, path_name in module.LOCAL_EVIDENCE.items() if path_name == relative
        )
        index = module.EXPECTED_EVIDENCE_ORDER.index(evidence_name)
        manifest["evidence"][index]["sha256"] = digest
    elif relative in module.ATTESTATION_EVIDENCE.values():
        evidence_name = next(
            name
            for name, path_name in module.ATTESTATION_EVIDENCE.items()
            if path_name == relative
        )
        index = module.EXPECTED_EVIDENCE_ORDER.index(evidence_name)
        manifest["evidence"][index]["sha256"] = digest
        if isinstance(context.get("attestations"), dict):
            context["attestations"][evidence_name]["sha256"] = digest
        if evidence_name == "hosted-gate-execution":
            selection = context.get("hosted_gate_selection")
            if isinstance(selection, dict):
                selection["sha256"] = digest
    elif relative in module.PAYLOAD_ONLY_EVIDENCE.values():
        if isinstance(context.get("payload_only_attestations"), dict):
            context["payload_only_attestations"][relative] = digest
    elif relative in {"sbom.spdx.json", "provenance.intoto.json"}:
        field = "sbom" if relative == "sbom.spdx.json" else "provenance"
        manifest["build"][field]["sha256"] = digest


def materialize_fixture(
    module: Any,
    execution: Any,
    manifest: dict[str, Any],
    context: dict[str, Any],
    evidence_root: Path,
) -> tuple[dict[str, Any], dict[str, Any]]:
    """Write valid JSON evidence and return hashes bound to the written files."""

    disk_manifest = copy.deepcopy(manifest)
    disk_context = copy.deepcopy(context)
    source = disk_manifest["source"]
    payloads: dict[str, dict[str, Any]] = {}
    for gate_name in module.HOSTED_GATE_NAMES:
        payloads[f"hosted-gates/{gate_name}.json"] = hosted_fixture(
            module, disk_context, gate_name
        )
    for name, relative in module.LOCAL_EVIDENCE.items():
        payloads[relative] = local_fixture(name, module, source)
    payloads[module.PAYLOAD_ONLY_EVIDENCE["repository-governance"]] = governance_fixture(
        module, disk_context, source
    )
    payloads[module.PAYLOAD_ONLY_EVIDENCE["hosted-run-execution"]] = execution_fixture(
        module, execution, disk_context, source
    )
    payloads["sbom.spdx.json"] = sbom_fixture(disk_context, source)
    payloads["provenance.intoto.json"] = provenance_fixture(module, disk_context, source)

    for relative, payload in payloads.items():
        path = evidence_root / relative
        write_json(path, payload)
        bind_fixture_digest(
            module,
            disk_manifest,
            disk_context,
            relative,
            module.sha256_file(path),
        )

    # The canonical manifest attestation pair is generated from the two
    # payload-only observations and the already-bound local/hosted files.
    binding_records = {
        name: {
            "path": relative,
            "sha256": disk_context["files"][relative],
            "producer_schema": module.LOCAL_SCHEMAS[name],
            "producer_commit_sha": source["commit_sha"],
            "producer_tree_sha": source["tree_sha"],
            "status": "ok",
            "ok": True,
        }
        for name, relative in module.LOCAL_EVIDENCE.items()
    }
    binding_payload = {
        "schema": "cex.p0-local-evidence-binding.v1",
        "status": "ok",
        "ok": True,
        "repository": source["repository"],
        "branch": source["branch"],
        "commit_sha": source["commit_sha"],
        "tree_sha": source["tree_sha"],
        "workflow_run_id": disk_context["workflow_run_id"],
        "workflow_run_attempt": disk_context["workflow_run_attempt"],
        "generated_at": "2026-08-30T00:00:00Z",
        "records": binding_records,
    }
    binding_relative = module.ATTESTATION_EVIDENCE["local-evidence-binding"]
    write_json(evidence_root / binding_relative, binding_payload)
    bind_fixture_digest(
        module,
        disk_manifest,
        disk_context,
        binding_relative,
        module.sha256_file(evidence_root / binding_relative),
    )

    raw_execution = payloads[module.PAYLOAD_ONLY_EVIDENCE["hosted-run-execution"]]
    hosted_gates: dict[str, Any] = {}
    for gate_name, workflow_path in module.HOSTED_WORKFLOW_PATHS.items():
        source_gate = raw_execution["gates"][gate_name]
        hosted_gates[workflow_path] = {
            "run_id": source_gate["run_id"],
            "run_attempt": source_gate["run_attempt"],
            "event": source_gate["event"],
            "head_branch": source_gate["head_branch"],
            "head_sha": source_gate["head_sha"],
            "status": "completed",
            "conclusion": "success",
            "created_at": source_gate["created_at"],
            "updated_at": source_gate["updated_at"],
            "selection_policy": "latest_authoritative_run_is_binding",
            "jobs": [
                {
                    "job_id": job["job_id"],
                    "name": job["name"],
                    "runner_id": job["runner_id"],
                    "runner_name": job["runner_name"],
                    "runner_labels": job["labels"],
                    "status": job["status"],
                    "conclusion": job["conclusion"],
                    "required_steps": job["required_steps"],
                    "observed_step_count": len(job["steps"]),
                }
                for job in source_gate["jobs"]
            ],
        }
    hosted_binding = {
        "schema": "cex.hosted-gate-execution.v1",
        "status": "ok",
        "ok": True,
        "repository": source["repository"],
        "branch": source["branch"],
        "commit_sha": source["commit_sha"],
        "tree_sha": source["tree_sha"],
        "selection_policy": "latest_authoritative_run_is_binding",
        "generated_at": "2026-08-30T00:00:00Z",
        "gates": hosted_gates,
    }
    hosted_relative = module.ATTESTATION_EVIDENCE["hosted-gate-execution"]
    write_json(evidence_root / hosted_relative, hosted_binding)
    bind_fixture_digest(
        module,
        disk_manifest,
        disk_context,
        hosted_relative,
        module.sha256_file(evidence_root / hosted_relative),
    )

    # The index intentionally excludes itself, matching the collector's
    # refresh_payload_index implementation.  Its own digest is then indexed.
    index_payload = {
        "schema": module.PAYLOAD_INDEX_SCHEMA,
        "repository": disk_context["repository"],
        "branch": disk_context["branch"],
        "commit_sha": disk_context["commit_sha"],
        "tree_sha": disk_context["tree_sha"],
        "workflow_run_id": disk_context["workflow_run_id"],
        "workflow_run_attempt": disk_context["workflow_run_attempt"],
        "payload_name": disk_context["payload_name"],
        "generated_at": disk_context["generated_at"],
        "files": dict(disk_context["files"]),
    }
    index_path = evidence_root / "payload-index.json"
    write_json(index_path, index_payload)
    disk_context["files"]["payload-index.json"] = module.sha256_file(index_path)
    return disk_manifest, disk_context


def expect_rejected(module: Any, name: str, value: dict[str, Any]) -> None:
    try:
        module.validate_manifest(value)
    except module.ContractError:
        return
    PROBLEMS.append(f"strict evidence negative case was accepted: {name}")


def expect_context_rejected(
    module: Any,
    name: str,
    value: dict[str, Any],
    context: dict[str, Any],
    evidence_dir: Path | None = None,
) -> None:
    try:
        module.validate_manifest(value, context=context, evidence_dir=evidence_dir)
    except module.ContractError:
        return
    PROBLEMS.append(f"strict evidence context negative case was accepted: {name}")


def template_contract_self_test() -> None:
    """Ensure the wiring self-test also exercises the real template/schema gate."""

    completed = subprocess.run(
        [
            sys.executable,
            str(ROOT / "scripts/check-release-baseline-manifest.py"),
            str(ROOT / "docs/templates/cex-release-baseline-manifest-v1.json"),
            "--allow-template",
        ],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if completed.returncode != 0:
        PROBLEMS.append("template/schema release gate failed: " + completed.stdout.strip())


def run_contract_self_tests() -> None:
    try:
        freshness = subprocess.run(
            [sys.executable, str(SNAPSHOT_FRESHNESS), "--self-test"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        if freshness.returncode != 0:
            PROBLEMS.append(
                "hosted snapshot freshness self-test failed: "
                + freshness.stdout.strip()
            )
        module = load_contract_module()
        execution = load_execution_module()
        strict = load_strict_module()
        if strict.nofollow_supported():
            # The final contract dynamically loads declarative verifier code.
            # A symlink at that boundary must be rejected before any bytes are
            # executed; otherwise an attacker could replace the required-job
            # map while retaining a self-consistent evidence payload.
            with tempfile.TemporaryDirectory(prefix="cex-contract-loader-") as directory:
                loader_root = Path(directory)
                loader_target = loader_root / "target.py"
                loader_link = loader_root / "link.py"
                loader_target.write_text("value = 1\n", encoding="utf-8")
                loader_link.symlink_to(loader_target)
                try:
                    module.load_module_nofollow(
                        loader_link,
                        "cex_contract_loader_symlink_test",
                        "symlink loader self-test",
                    )
                except module.ContractError:
                    pass
                else:
                    PROBLEMS.append("strict evidence no-follow module loader accepted a symlink")
        execution_failures = execution.self_test()
        if execution_failures:
            PROBLEMS.extend(
                f"hosted execution verifier self-test: {item}"
                for item in execution_failures
            )
        for gate_name, jobs in execution.EXPECTED_JOBS.items():
            workflow_path = ROOT / execution.EXPECTED_WORKFLOW_PATHS[gate_name]
            workflow = require_file(workflow_path)
            for job_name, contract in jobs.items():
                if f"  {job_name}:" not in workflow:
                    PROBLEMS.append(
                        f"{workflow_path.relative_to(ROOT)} lacks expected job: {job_name}"
                    )
                label = contract.get("runner_label")
                if f"runs-on: {label}" not in workflow:
                    PROBLEMS.append(
                        f"{workflow_path.relative_to(ROOT)}/{job_name} lacks runner label: {label}"
                    )
                for step_name in contract.get("steps", set()):
                    if f"- name: {step_name}" not in workflow:
                        PROBLEMS.append(
                            f"{workflow_path.relative_to(ROOT)}/{job_name} lacks step: {step_name}"
                        )
        base = valid_manifest(module)
        module.validate_manifest(base)

        waived = copy.deepcopy(base)
        waived["evidence"][0]["status"] = "waived"
        waived["evidence"][0]["waiver"] = "not permitted"
        expect_rejected(module, "waived evidence", waived)

        missing_job_proof = copy.deepcopy(base)
        missing_job_proof["evidence"] = [
            item
            for item in missing_job_proof["evidence"]
            if item["name"] != "hosted-gate-execution"
        ]
        expect_rejected(module, "missing hosted execution proof", missing_job_proof)

        reordered_external = copy.deepcopy(base)
        reordered_external["external_gates"]["items"].reverse()
        expect_rejected(module, "reordered external gates", reordered_external)

        production_claim = copy.deepcopy(base)
        production_claim["production_ready"] = True
        expect_rejected(module, "production-ready overclaim", production_claim)

        mutable_uri = copy.deepcopy(base)
        mutable_uri["evidence"][0]["uri"] = "file:///tmp/evidence.json"
        expect_rejected(module, "mutable evidence URI", mutable_uri)

        for label, mutate in (
            ("missing-generated-at", lambda value: value.pop("generated_at")),
            ("missing-release-id", lambda value: value.pop("release_id")),
            ("missing-revocation", lambda value: value.pop("revocation")),
            ("invalid-approval-time", lambda value: value["approvals"][0].update(decided_at="garbage")),
            ("release-id-drift", lambda value: value.update(release_id="cex-p0-" + "2" * 40)),
            ("unknown-root-field", lambda value: value.update(unexpected=True)),
            (
                "reordered-evidence",
                lambda value: value["evidence"].__setitem__(
                    slice(0, 2), value["evidence"][0:2][::-1]
                ),
            ),
            ("wrong-hosted-repository", lambda value: value["evidence"][0].update(uri="gh://Other/CEX/actions/runs/1/attempts/1")),
            ("malformed-payload-suffix", lambda value: value["build"]["artifacts"][0].update(name=value["build"]["artifacts"][0]["name"] + "-extra")),
            ("wrong-branch", lambda value: value["source"].update(branch="refs/heads/main")),
            ("duplicate-uri", lambda value: value["evidence"][1].update(uri=value["evidence"][0]["uri"])),
        ):
            candidate = copy.deepcopy(base)
            mutate(candidate)
            expect_rejected(module, label, candidate)

        # Context metadata is deliberately sourced from this checkout.  The
        # fixture must therefore track the real branch/tree, Cargo.lock and
        # migration-chain digests, as well as the sole shared candidate trigger
        # authority checked by validate_context_metadata.
        context = valid_context(module, base)

        # The on-disk payload and freeze fixtures exercise POSIX descriptor
        # guarantees (O_NOFOLLOW/O_DIRECTORY).  Strict evidence collection is
        # intentionally hosted on Linux; Windows still runs all in-memory
        # contract and schema regressions above, but must not report a product
        # failure merely because those POSIX primitives do not exist.
        if not strict.nofollow_supported():
            return

        # Exercise the final on-disk re-hash path with actual JSON payloads,
        # rather than opaque bytes that bypass producer-level validators.
        with tempfile.TemporaryDirectory(prefix="cex-strict-evidence-") as directory:
            evidence_root = Path(directory)
            disk_base, disk_context = materialize_fixture(
                module, execution, base, context, evidence_root
            )
            module.validate_manifest(
                disk_base, context=disk_context, evidence_dir=evidence_root
            )

            extra_path = evidence_root / "unexpected.json"
            extra_path.write_text("{}\n", encoding="utf-8")
            expect_context_rejected(
                module,
                "non-canonical-payload-path",
                disk_base,
                disk_context,
                evidence_root,
            )
            extra_path.unlink()

            swapped = copy.deepcopy(disk_base)
            swapped["evidence"][0]["uri"] = (
                "gh://TrillionniumFoundation/CEX/actions/runs/99999/attempts/1"
            )
            expect_context_rejected(
                module, "swapped-hosted-uri", swapped, disk_context, evidence_root
            )
            changed_digest = copy.deepcopy(disk_base)
            changed_digest["evidence"][0]["sha256"] = "sha256:" + "4" * 64
            expect_context_rejected(
                module, "changed-hosted-digest", changed_digest, disk_context, evidence_root
            )

            stale_cargo = copy.deepcopy(disk_context)
            stale_cargo["cargo_lock_sha256"] = "sha256:" + "4" * 64
            expect_context_rejected(
                module, "stale-cargo-lock-digest", disk_base, stale_cargo, evidence_root
            )
            stale_migration = copy.deepcopy(disk_context)
            stale_migration["migration_chain_sha256"] = "sha256:" + "4" * 64
            expect_context_rejected(
                module,
                "stale-migration-chain-digest",
                disk_base,
                stale_migration,
                evidence_root,
            )

            def forged_file_case(
                name: str,
                relative: str,
                mutate: Any,
            ) -> None:
                """Keep hashes self-consistent so semantic forgery is rejected."""

                path = evidence_root / relative
                index_path = evidence_root / "payload-index.json"
                original_text = path.read_text(encoding="utf-8")
                original_index_text = index_path.read_text(encoding="utf-8")
                original = json.loads(original_text)
                forged = copy.deepcopy(original)
                mutate(forged)
                write_json(path, forged)

                forged_manifest = copy.deepcopy(disk_base)
                forged_context = copy.deepcopy(disk_context)
                bind_fixture_digest(
                    module,
                    forged_manifest,
                    forged_context,
                    relative,
                    module.sha256_file(path),
                )
                forged_index = json.loads(original_index_text)
                forged_index["files"] = {
                    key: value
                    for key, value in forged_context["files"].items()
                    if key != "payload-index.json"
                }
                write_json(index_path, forged_index)
                forged_context["files"]["payload-index.json"] = module.sha256_file(
                    index_path
                )
                expect_context_rejected(
                    module,
                    name,
                    forged_manifest,
                    forged_context,
                    evidence_root,
                )
                path.write_text(original_text, encoding="utf-8")
                index_path.write_text(original_index_text, encoding="utf-8")

            forged_file_case(
                "secret-like-payload-content",
                "candidate-hygiene.json",
                lambda value: value.update(token="fixture-secret-that-must-not-ship"),
            )
            forged_file_case(
                "unknown-local-producer-field",
                "candidate-hygiene.json",
                lambda value: value.update(operator_note="harmless-looking extra field"),
            )
            forged_file_case(
                "credential-bearing-uri",
                "repository-integrity.json",
                lambda value: value.update(
                    active_plan="postgres://cex:fixture-secret@db.internal/prod"
                ),
            )
            forged_file_case(
                "unknown-candidate-trust-field",
                "candidate-hygiene.json",
                lambda value: value["workflow_trust"].update(note="unexpected"),
            )
            forged_file_case(
                "unknown-integrity-digest-field",
                "repository-integrity.json",
                lambda value: value["digests"].update(note="unexpected"),
            )
            forged_file_case(
                "unknown-integrity-documentation-field",
                "repository-integrity.json",
                lambda value: value["documentation_check"].update(note="unexpected"),
            )
            forged_file_case(
                "unknown-hosted-gate-field",
                "hosted-gates/p0-migration-gate.json",
                lambda value: value.update(note="unexpected"),
            )
            forged_file_case(
                "forged-governance-payload",
                "repository-governance.json",
                lambda value: value.update(candidate_tree_matches_commit=False),
            )
            forged_file_case(
                "unknown-governance-ruleset-field",
                "repository-governance.json",
                lambda value: (
                    value["rulesets"].append(
                        {
                            "id": 1,
                            "name": "fixture",
                            "target": "branch",
                            "enforcement": "active",
                            "active": True,
                            "applies_to_candidate_branch": False,
                            "required_status_contexts": [],
                            "bypass_state": "none",
                            "note": "unexpected",
                        }
                    ),
                    value.update(ruleset_count=1),
                ),
            )
            forged_file_case(
                "nonempty-candidate-hygiene-problems",
                "candidate-hygiene.json",
                lambda value: value.update(problems=["forged warning"]),
            )
            forged_file_case(
                "candidate-plan-not-bound",
                "candidate-hygiene.json",
                lambda value: value.update(plan="CEX-DEVELOPMENT-PLAN-2026-08-28-v11.md"),
            )
            forged_file_case(
                "candidate-workflow-scope-not-bound",
                "candidate-hygiene.json",
                lambda value: value.update(workflow_pin_scope=[]),
            )
            forged_file_case(
                "candidate-trigger-not-bound",
                "candidate-hygiene.json",
                lambda value: value.update(shared_trigger="docs/release-evidence/other-trigger.json"),
            )
            forged_file_case(
                "failed-documentation-check",
                "repository-integrity.json",
                lambda value: value["documentation_check"].update(
                    status="failed", problems=["forged documentation gap"]
                ),
            )
            forged_file_case(
                "forged-execution-payload",
                "hosted-run-execution.json",
                lambda value: value["gates"][module.HOSTED_GATE_NAMES[0]].update(
                    status="failure"
                ),
            )
            forged_file_case(
                "unknown-execution-job-field",
                "hosted-run-execution.json",
                lambda value: value["gates"][module.HOSTED_GATE_NAMES[0]]["jobs"][0].update(
                    note="unexpected"
                ),
            )
            forged_file_case(
                "unknown-execution-step-field",
                "hosted-run-execution.json",
                lambda value: value["gates"][module.HOSTED_GATE_NAMES[0]]["jobs"][0]["steps"][0].update(
                    note="unexpected"
                ),
            )
            forged_file_case(
                "forged-hosted-gate-attestation",
                "hosted-gate-execution.json",
                lambda value: value["gates"][module.HOSTED_WORKFLOW_PATHS[module.HOSTED_GATE_NAMES[0]]]["jobs"][0].update(
                    conclusion="failure"
                ),
            )
            forged_file_case(
                "unknown-hosted-attestation-job-field",
                "hosted-gate-execution.json",
                lambda value: value["gates"][module.HOSTED_WORKFLOW_PATHS[module.HOSTED_GATE_NAMES[0]]]["jobs"][0].update(
                    note="unexpected"
                ),
            )
            forged_file_case(
                "unknown-local-binding-record-field",
                "local-evidence-binding.json",
                lambda value: value["records"]["candidate-hygiene"].update(
                    note="unexpected"
                ),
            )
            forged_file_case(
                "duplicate-hosted-gate-job",
                "hosted-gate-execution.json",
                lambda value: value["gates"][module.HOSTED_WORKFLOW_PATHS[module.HOSTED_GATE_NAMES[0]]]["jobs"].append(
                    copy.deepcopy(value["gates"][module.HOSTED_WORKFLOW_PATHS[module.HOSTED_GATE_NAMES[0]]]["jobs"][0])
                ),
            )
            forged_file_case(
                "unknown-sbom-package-field",
                "sbom.spdx.json",
                lambda value: value["packages"][0].update(note="unexpected"),
            )
            forged_file_case(
                "unknown-provenance-build-field",
                "provenance.intoto.json",
                lambda value: value["predicate"]["buildDefinition"].update(note="unexpected"),
            )
            forged_file_case(
                "repository-integrity-digest-not-bound",
                "repository-integrity.json",
                lambda value: value["digests"].update(
                    cargo_lock="sha256:" + "4" * 64
                ),
            )
            forged_file_case(
                "sbom-license-not-canonical",
                "sbom.spdx.json",
                lambda value: value.update(dataLicense="MIT"),
            )
            forged_file_case(
                "sbom-duplicate-package-id",
                "sbom.spdx.json",
                lambda value: value["packages"].append(copy.deepcopy(value["packages"][0])),
            )
            forged_file_case(
                "sbom-relationship-set-mismatch",
                "sbom.spdx.json",
                lambda value: value.update(relationships=[]),
            )
            forged_file_case(
                "provenance-build-type-not-canonical",
                "provenance.intoto.json",
                lambda value: value["predicate"]["buildDefinition"].update(
                    buildType="https://example.invalid/evil"
                ),
            )
            forged_file_case(
                "provenance-duplicate-dependency",
                "provenance.intoto.json",
                lambda value: value["predicate"]["buildDefinition"]["resolvedDependencies"].append(
                    copy.deepcopy(
                        value["predicate"]["buildDefinition"]["resolvedDependencies"][0]
                    )
                ),
            )
            forged_file_case(
                "provenance-unknown-dependency-uri",
                "provenance.intoto.json",
                lambda value: value["predicate"]["buildDefinition"]["resolvedDependencies"][1].update(
                    uri="file:unexpected"
                ),
            )
            forged_file_case(
                "hepta-mode-not-canonical",
                "hepta-postgres-integration.json",
                lambda value: value.update(mode="full"),
            )
            forged_file_case(
                "hepta-lint-policy-not-canonical",
                "hepta-postgres-integration.json",
                lambda value: value.update(lint_policy="strict"),
            )
            forged_file_case(
                "soak-iterations-below-workflow-floor",
                "exact-ledger-soak.json",
                lambda value: value.update(iterations=249),
            )
            forged_file_case(
                "soak-balance-invariant-mismatch",
                "exact-ledger-soak.json",
                lambda value: value.update(balance_minor=0),
            )
            forged_file_case(
                "soak-negative-count",
                "exact-ledger-soak.json",
                lambda value: value.update(ledger_entry_count=-1),
            )
            forged_file_case(
                "soak-account-not-uuid",
                "exact-ledger-soak.json",
                lambda value: value.update(account_id="not-a-uuid"),
            )
            forged_file_case(
                "backup-empty-dump",
                "backup-restore.json",
                lambda value: value.update(dump_bytes=0),
            )
            forged_file_case(
                "backup-empty-archive",
                "backup-restore.json",
                lambda value: value.update(archive_items=0),
            )
            forged_file_case(
                "governance-ruleset-status-mismatch",
                "repository-governance.json",
                lambda value: value.update(rulesets_http_status=500),
            )
            forged_file_case(
                "governance-duplicate-context",
                "repository-governance.json",
                lambda value: value["actual_required_status_contexts"].append(
                    value["actual_required_status_contexts"][0]
                ),
            )
            forged_file_case(
                "execution-null-job-timestamp",
                "hosted-run-execution.json",
                lambda value: value["gates"][module.HOSTED_GATE_NAMES[0]]["jobs"][0].update(
                    started_at=None
                ),
            )
            forged_file_case(
                "execution-duplicate-step-number",
                "hosted-run-execution.json",
                lambda value: value["gates"][module.HOSTED_GATE_NAMES[0]]["jobs"][0]["steps"][1].update(
                    number=value["gates"][module.HOSTED_GATE_NAMES[0]]["jobs"][0]["steps"][0]["number"]
                ),
            )
            forged_file_case(
                "hosted-attestation-label-type",
                "hosted-gate-execution.json",
                lambda value: value["gates"][module.HOSTED_WORKFLOW_PATHS[module.HOSTED_GATE_NAMES[0]]]["jobs"][0].update(
                    runner_labels=[1]
                ),
            )

            sbom_path = evidence_root / "sbom.spdx.json"
            sbom_original = sbom_path.read_bytes()
            sbom_path.write_bytes(b"mutated")
            expect_context_rejected(
                module,
                "mutated-payload-file",
                disk_base,
                disk_context,
                evidence_root,
            )
            sbom_path.write_bytes(sbom_original)

            # Re-indexing after the core collector has already emitted an
            # index must replace that stale index rather than treating it as
            # an unexpected payload file.
            refresh_context_path = evidence_root.parent / "refresh-context.json"
            write_json(refresh_context_path, disk_context)
            strict.refresh_payload_index(evidence_root, refresh_context_path)
            refreshed_context = json.loads(
                refresh_context_path.read_text(encoding="utf-8")
            )
            if refreshed_context.get("files") != disk_context.get("files"):
                PROBLEMS.append("refresh_payload_index changed canonical file bindings")

            # Freeze the exact bytes that the workflow uploads and verify the
            # out-of-band lock at manifest time.  Restore temporary directory
            # permissions afterwards so TemporaryDirectory cleanup succeeds.
            context_path = evidence_root.parent / f"{evidence_root.name}-context.json"
            lock_path = evidence_root.parent / f"{evidence_root.name}-payload.lock"
            write_json(context_path, disk_context)
            strict.freeze_payload(evidence_root, context_path, lock_path)
            strict.validate_payload_lock(evidence_root, disk_context, lock_path)
            if evidence_root.stat().st_mode & 0o222:
                PROBLEMS.append("freeze_payload left the evidence root writable")
            for path in evidence_root.rglob("*"):
                mode = path.stat().st_mode
                if path.is_dir():
                    path.chmod(mode | 0o700)
                else:
                    path.chmod(mode | 0o600)
            evidence_root.chmod(evidence_root.stat().st_mode | 0o700)
    except Exception as error:  # fail closed with a useful diagnostic
        PROBLEMS.append(f"strict evidence contract self-test crashed: {error}")


def main() -> int:
    verify_failure_diagnostics_isolation()
    verify_release_wrapper_boundaries()
    template_contract_self_test()
    require_markers(
        WORKFLOW,
        "scripts/p0-release-evidence-strict.py collect",
        "scripts/p0-release-evidence-strict.py freeze",
        "scripts/p0-release-evidence-strict.py manifest",
        "scripts/test-p0-release-evidence-strict.py",
        "scripts/check-release-evidence-contract.py",
        "--evidence-dir",
        "--context run/p0-release-context.json",
        "--payload-lock run/p0-release-payload.lock",
        "Bind exact-tree hosted gate and job evidence",
        "--tree-sha \"$CANDIDATE_TREE\"",
        "actions: read",
        "Revalidate frozen latest hosted snapshot after exact-job verification",
        "Revalidate frozen latest hosted snapshot before manifest",
        "Revalidate frozen latest hosted snapshot after manifest publication",
        "python3 scripts/verify-hosted-snapshot-freshness.py",
        "Upload failure diagnostics",
        "path: run/p0-release-support",
    )
    require_markers(
        STRICT_WRAPPER,
        "verify-hosted-run-execution.py",
        "bind-p0-local-evidence.py",
        "check-hosted-gate-execution.py",
        "PAYLOAD_ONLY_ATTESTATIONS",
        "ATTESTATION_EVIDENCE",
        "CANONICAL_EVIDENCE_ORDER",
        "repository-governance.json",
        "hosted-run-execution.json",
        "run_core_collect_frozen(",
        "frozen_runs_from_attestation",
        "immutable core attempted to select hosted runs more than once",
        "repository governance evidence lacks an exact tree",
        "validate_manifest(output_path, context_path, evidence_dir)",
        "CANONICAL_PAYLOAD_FILES",
        "strict evidence payload changed after freeze/upload",
        "payload-only attestations leaked into core manifest evidence",
        "--execution",
        "--frozen-hosted-context",
        "bind_hosted_gate_selection",
        "hosted_gate_selection",
        "LATEST_RUN_POLICY",
    )
    require_markers(
        CANONICAL_GENERATOR,
        '"local-evidence-binding": "local-evidence-binding.json"',
        '"hosted-gate-execution": "hosted-gate-execution.json"',
        "augment_manifest",
    )
    require_markers(
        EVIDENCE_CORE,
        "read_regular_nofollow",
        "p0-release-evidence-core-impl.py",
        "authoritative_run_sort_key",
        "__impl__",
        "exec(compile",
    )
    require_markers(
        EVIDENCE_CORE_IMPL,
        "tree_sha = payload.get(\"tree_sha\")",
        "lacks a valid exact tree_sha",
        "def revalidate_gate_runs(",
        "release-evidence core self-test failed",
    )
    require_markers(
        LOCAL_BINDER,
        "producer_tree = payload.get(\"tree_sha\")",
        "local-evidence-binding.v1",
    )
    require_markers(
        HOSTED_CHECKER,
        "read_regular_nofollow",
        "check-hosted-gate-execution-impl.py",
        "authoritative_run_sort_key",
        "__impl__",
        "exec(compile",
    )
    require_markers(
        HOSTED_CHECKER_IMPL,
        "latest_authoritative_run_is_binding",
        "no real runner was allocated",
        "build_frozen_attestation",
        "--context and --execution must be supplied together",
        "disables run re-selection",
    )
    require_markers(
        EXECUTION_VERIFIER,
        "/attempts/{run_attempt}/jobs",
        "did not receive a real runner",
        "has no executed steps",
        "lacks a successful checkout step",
        "required_runner_label",
        "missing required step",
        "is bound to a different run",
        "EXPECTED_JOBS",
    )
    require_markers(
        SNAPSHOT_FRESHNESS,
        "read_regular_nofollow",
        "verify-hosted-snapshot-freshness-impl.py",
        "authoritative_order_self_test",
        "__impl__",
        "exec(compile",
    )
    require_markers(
        SNAPSHOT_FRESHNESS_IMPL,
        "cex.hosted-gate-selection-binding.v1",
        "latest_authoritative_run_is_binding",
        "latest_run_states",
        "paged_collection",
        "newer-success",
        "newer-rerun-attempt",
        "detail-status-drift",
        "read_json_nofollow",
        "read_regular_nofollow",
        "workflow_revalidation_count",
    )
    require_markers(
        CONTRACT,
        "waivers are forbidden",
        "EXPECTED_EVIDENCE",
        "EXPECTED_EXTERNAL_GATES",
        "repository-qualification-automation",
        "validate_context_binding",
        "CANONICAL_PAYLOAD_FILES",
        "reject_secret_like_payload",
        "REQUIRED_ROOT_FIELDS",
        "FORBIDDEN_SPLIT_BRAIN_EVIDENCE",
        "local-evidence-binding",
        "hosted-gate-execution",
        "repository-governance",
        "hosted-run-execution",
    )
    require_markers(
        PRIMARY_CONTRACT,
        '"local-evidence-binding"',
        '"hosted-gate-execution"',
        "JSON Schema",
    )
    require_markers(
        SCHEMA,
        '"const": "local-evidence-binding"',
        '"const": "hosted-gate-execution"',
        '"minItems": 13',
        '"maxItems": 13',
    )
    require_markers(
        ROOT / "docs/release-evidence/p0-candidate-trigger.json",
        '"sequence":',
        '"production_authorization": "not_granted"',
    )
    run_contract_self_tests()

    result = {
        "schema": "cex.strict-release-evidence-wiring-check.v2",
        "status": "failed" if PROBLEMS else "ok",
        "canonical_evidence_count": 13,
        "payload_only_attestations": [
            "repository-governance.json",
            "hosted-run-execution.json",
        ],
        "problems": PROBLEMS,
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 1 if PROBLEMS else 0


if __name__ == "__main__":
    raise SystemExit(main())
