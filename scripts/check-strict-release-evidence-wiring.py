#!/usr/bin/env python3
"""Self-test strict release-evidence wiring and negative candidate cases."""

from __future__ import annotations

import copy
import importlib.util
import json
import sys
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


def valid_manifest(module: Any) -> dict[str, Any]:
    sha = "1" * 40
    tree = "2" * 40
    digest = "sha256:" + "3" * 64
    payload = f"cex-p0-evidence-{sha}-attempt-1"
    evidence = []
    for name in sorted(module.EXPECTED_EVIDENCE):
        if name.startswith("hosted:"):
            uri = f"gh://TrillionniumFoundation/CEX/actions/runs/1/attempts/1"
        else:
            uri = f"artifact://{payload}/{name}.json"
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
            "repository": "TrillionniumFoundation/CEX",
            "branch": "candidate/test",
            "commit_sha": sha,
            "tree_sha": tree,
        },
        "dependencies": {"cargo_lock_sha256": digest},
        "database": {
            "migration_head": "0087_add_term_exchange_receipt_event_history.sql",
            "migration_sha256": digest,
            "migration_chain_sha256": digest,
        },
        "build": {
            "workflow_run_id": 1,
            "artifacts": [
                {
                    "name": payload,
                    "uri": f"gh://TrillionniumFoundation/CEX/actions/runs/1/attempts/1/artifacts/{payload}",
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


def expect_rejected(module: Any, name: str, value: dict[str, Any]) -> None:
    try:
        module.validate_manifest(value)
    except module.ContractError:
        return
    PROBLEMS.append(f"strict evidence negative case was accepted: {name}")


def run_contract_self_tests() -> None:
    try:
        module = load_contract_module()
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
    except Exception as error:  # fail closed with a useful diagnostic
        PROBLEMS.append(f"strict evidence contract self-test crashed: {error}")


def main() -> int:
    require_markers(
        WORKFLOW,
        "scripts/p0-release-evidence-strict.py collect",
        "scripts/p0-release-evidence-strict.py manifest",
        "scripts/check-release-evidence-contract.py",
        "Bind exact-tree hosted gate and job evidence",
        "actions: read",
    )
    require_markers(
        STRICT_WRAPPER,
        "verify-hosted-run-execution.py",
        "repository-governance.json",
        "hosted-run-execution.json",
        "run_legacy(forwarded)",
    )
    require_markers(
        EXECUTION_VERIFIER,
        "/attempts/{run_attempt}/jobs",
        "did not receive a real runner",
        "has no executed steps",
        "lacks a successful checkout step",
        "EXPECTED_JOBS",
    )
    require_markers(
        CONTRACT,
        "waivers are forbidden",
        "EXPECTED_EVIDENCE",
        "EXPECTED_EXTERNAL_GATES",
        "repository-qualification-automation",
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
