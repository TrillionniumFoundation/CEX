#!/usr/bin/env python3
"""Self-test strict release-evidence wiring and negative candidate cases."""

from __future__ import annotations

import copy
import importlib.util
import json
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/p0-release-candidate-gate.yml"
STRICT_WRAPPER = ROOT / "scripts/p0-release-evidence-strict.py"
EXECUTION_VERIFIER = ROOT / "scripts/verify-hosted-run-execution.py"
CONTRACT = ROOT / "scripts/check-release-evidence-contract.py"
PROBLEMS: list[str] = []


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
        "candidate-hygiene": "candidate-hygiene.json",
        "repository-integrity": "repository-integrity.json",
        "hepta-postgres-integration": "hepta-postgres-integration.json",
        "migration-and-lifecycle-matrix": "database-lifecycle.json",
        "exact-ledger-soak": "exact-ledger-soak.json",
        "backup-restore": "backup-restore.json",
        "repository-governance": "repository-governance.json",
        "hosted-run-execution": "hosted-run-execution.json",
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
        return {**common, "problems": []}
    if name == "repository-integrity":
        return {
            **common,
            "repository_commit_sha": source["commit_sha"],
            "repository_tree_sha": source["tree_sha"],
            "generated_at": "2026-08-30T00:00:00Z",
            "active_plan": "CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md",
            "active_addendum": "none",
            "migration_head": "0087_add_term_exchange_receipt_event_history.sql",
            "production_authorization": "not_granted",
            "digests": {},
            "documentation_check": {"ok": True},
        }
    if name == "hepta-postgres-integration":
        return {
            **common,
            "mode": "fixture",
            "postgres_required": True,
            "lint_policy": "strict",
            "completed_at": "2026-08-30T00:00:00Z",
            "checks": ["fixture-check"],
        }
    if name == "migration-and-lifecycle-matrix":
        return {
            "schema": module.LOCAL_SCHEMAS[name],
            "ok": True,
            "commit_sha": source["commit_sha"],
            "tree_sha": source["tree_sha"],
            "completed_at": "2026-08-30T00:00:00Z",
            "checks": ["term-exchange-receipt-partial-upgrade-regression"],
        }
    if name == "exact-ledger-soak":
        return {
            "schema": module.LOCAL_SCHEMAS[name],
            "ok": True,
            "iterations": 1,
            "account_id": "fixture-account",
            "balance_minor": 0,
            "reserved_minor": 0,
            "ledger_entry_count": 0,
            "distinct_operation_count": 0,
            "compatibility_entry_count": 0,
            "audit_effect_count": 0,
            "started_at_epoch": 1,
            "ended_at_epoch": 2,
            "duration_seconds": 1,
            "commit_sha": source["commit_sha"],
            "tree_sha": source["tree_sha"],
        }
    if name == "backup-restore":
        restore_state = {"fixture": "ok"}
        return {
            "schema": module.LOCAL_SCHEMAS[name],
            "ok": True,
            "source": restore_state,
            "restored": restore_state,
            "dump_sha256": "4" * 64,
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
        "service-local-gate-linux",
        "service-local-gate-windows",
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
        "candidate_legacy_required_checks_enforced": False,
        "candidate_ruleset_required_checks_enforced": False,
        "required_candidate_checks_enforced": False,
        "rulesets_readable": True,
        "rulesets": [],
        "candidate_rulesets": [],
        "ruleset_count": 0,
        "candidate_ruleset_count": 0,
        "candidate_required_status_contexts": desired,
        "actual_required_status_contexts": desired,
        "desired_required_status_contexts": desired,
        "repository_candidate_enforcement": "not_enforced",
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
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": f"CEX P0 candidate {source['commit_sha']}",
        "documentNamespace": (
            f"https://github.com/{source['repository']}/p0-sbom/{source['commit_sha']}"
            f"/{context['workflow_run_id']}/attempt/{context['workflow_run_attempt']}"
        ),
        "creationInfo": {
            "created": "2026-08-30T00:00:00Z",
            "creators": ["Tool: cex-p0-release-evidence"],
        },
        "packages": [{"SPDXID": "SPDXRef-Package-fixture", "name": "fixture"}],
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
        if name == "repository-governance":
            payloads[relative] = governance_fixture(module, disk_context, source)
        elif name == "hosted-run-execution":
            payloads[relative] = execution_fixture(module, execution, disk_context, source)
        else:
            payloads[relative] = local_fixture(name, module, source)
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
        module = load_contract_module()
        execution = load_execution_module()
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
            if item["name"] != "hosted-run-execution"
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
        # migration-chain digests, as well as the candidate trigger/freeze
        # policy checked by validate_context_metadata.
        context = valid_context(module, base)

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
                "forged-governance-payload",
                "repository-governance.json",
                lambda value: value.update(candidate_tree_matches_commit=False),
            )
            forged_file_case(
                "forged-execution-payload",
                "hosted-run-execution.json",
                lambda value: value["gates"][module.HOSTED_GATE_NAMES[0]].update(
                    status="failure"
                ),
            )

            (evidence_root / "sbom.spdx.json").write_bytes(b"mutated")
            expect_context_rejected(
                module,
                "mutated-payload-file",
                disk_base,
                disk_context,
                evidence_root,
            )
    except Exception as error:  # fail closed with a useful diagnostic
        PROBLEMS.append(f"strict evidence contract self-test crashed: {error}")


def main() -> int:
    template_contract_self_test()
    require_markers(
        WORKFLOW,
        "scripts/p0-release-evidence-strict.py collect",
        "scripts/p0-release-evidence-strict.py manifest",
        "scripts/check-release-evidence-contract.py",
        "--evidence-dir",
        "--context run/p0-release-context.json",
        "Bind exact-tree hosted gate and job evidence",
        "--tree-sha \"$CANDIDATE_TREE\"",
        "actions: read",
    )
    require_markers(
        STRICT_WRAPPER,
        "verify-hosted-run-execution.py",
        "repository-governance.json",
        "hosted-run-execution.json",
        "run_core(forwarded)",
        "repository governance evidence lacks an exact tree",
        "validate_manifest(output_path, context_path, evidence_dir)",
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
        CONTRACT,
        "waivers are forbidden",
        "EXPECTED_EVIDENCE",
        "EXPECTED_EXTERNAL_GATES",
        "repository-qualification-automation",
        "validate_context_binding",
        "REQUIRED_ROOT_FIELDS",
    )
    require_markers(
        ROOT / "docs/release-evidence/.qualification-freeze",
        "sequence=26",
        "production_authorization=not_granted",
    )
    run_contract_self_tests()

    result = {
        "schema": "cex.strict-release-evidence-wiring-check.v1",
        "status": "failed" if PROBLEMS else "ok",
        "problems": PROBLEMS,
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 1 if PROBLEMS else 0


if __name__ == "__main__":
    raise SystemExit(main())
