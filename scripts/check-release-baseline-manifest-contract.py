#!/usr/bin/env python3
"""Extend the v12 manifest contract with explicit attestation evidence."""

from __future__ import annotations

import argparse
import copy
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CORE = ROOT / "scripts/check-release-baseline-manifest-contract-core.py"
SHA256_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
ZERO_SHA256 = "sha256:" + "0" * 64
BASE_EVIDENCE_ORDER = (
    "hosted:p0-migration-gate",
    "hosted:rust-service-gate",
    "hosted:p0-gateway-exact-reserve-gate",
    "hosted:p0-execution-settlement-gate",
    "hosted:p0-provider-reconciliation-gate",
    "candidate-hygiene",
    "repository-integrity",
    "hepta-postgres-integration",
    "migration-and-lifecycle-matrix",
    "exact-ledger-soak",
    "backup-restore",
)
ATTESTATIONS = {
    "local-evidence-binding": "local-evidence-binding.json",
    "hosted-gate-execution": "hosted-gate-execution.json",
}
EXPECTED_EVIDENCE_ORDER = BASE_EVIDENCE_ORDER + tuple(ATTESTATIONS)
EXTERNAL_GATES = (
    "X1: production-like backup and restore rehearsal against representative data volume and the real storage topology",
    "X2: deployment/cutover and rollback rehearsal with real service identities, network policy and secret custody",
    "X3: real provider reconciliation artifacts for success, definite non-execution and indeterminate outcome",
    "X4: credential issuance, rotation, revocation and break-glass custody review",
    "X5: sustained production-like soak/endurance run with queue age, retry, reconciliation, parity and Audit delivery SLOs",
    "X6: independent security, operations and financial-control review",
    "X7: legal, commercial or provider approvals where the production integration requires them",
    "X8: final human go/no-go decision bound to the immutable release candidate",
)


def payload_name(data: dict[str, Any]) -> str | None:
    build = data.get("build")
    artifacts = build.get("artifacts") if isinstance(build, dict) else None
    if not isinstance(artifacts, list) or len(artifacts) != 1:
        return None
    item = artifacts[0]
    value = item.get("name") if isinstance(item, dict) else None
    return value if isinstance(value, str) and value else None


def evidence_names(data: dict[str, Any]) -> list[Any]:
    evidence = data.get("evidence")
    if not isinstance(evidence, list):
        return []
    return [item.get("name") if isinstance(item, dict) else None for item in evidence]


def outer_contract_problems(data: Any) -> list[str]:
    if not isinstance(data, dict):
        return ["manifest root must be an object"]
    problems: list[str] = []
    evidence = data.get("evidence")
    if not isinstance(evidence, list):
        problems.append("candidate evidence must be a list")
    else:
        names = evidence_names(data)
        if names != list(EXPECTED_EVIDENCE_ORDER):
            problems.append(
                "manifest evidence order must equal the canonical v12 sequence: "
                + ", ".join(EXPECTED_EVIDENCE_ORDER)
            )
        if len(names) != len(set(names)):
            problems.append("candidate evidence names must be unique")

    external = data.get("external_gates")
    if not isinstance(external, dict):
        problems.append("external_gates must be an object")
    else:
        if external.get("status") != "independent_approval_required":
            problems.append("external_gates.status is invalid")
        if tuple(external.get("items") or ()) != EXTERNAL_GATES:
            problems.append("external gates must equal the canonical X1-X8 contract")
    return problems


def split_attestations(data: Any) -> tuple[dict[str, Any] | None, list[str]]:
    problems = outer_contract_problems(data)
    if not isinstance(data, dict):
        return None, problems
    evidence = data.get("evidence")
    if not isinstance(evidence, list):
        return None, problems

    retained: list[Any] = []
    found: dict[str, dict[str, Any]] = {}
    for item in evidence:
        if not isinstance(item, dict):
            retained.append(item)
            continue
        name = item.get("name")
        if name in ATTESTATIONS:
            if name in found:
                problems.append(f"duplicate attestation evidence: {name}")
            else:
                found[name] = item
        else:
            retained.append(item)

    missing = sorted(set(ATTESTATIONS) - set(found))
    if missing:
        problems.append(f"missing attestation evidence: {missing}")

    payload = payload_name(data)
    if payload is None:
        problems.append("cannot resolve the unique payload artifact name")
    for name, relative in ATTESTATIONS.items():
        item = found.get(name)
        if item is None:
            continue
        if item.get("status") != "pass":
            problems.append(f"attestation evidence {name} must be pass")
        if item.get("waiver") is not None:
            problems.append(f"attestation evidence {name} must not have a waiver")
        digest = item.get("sha256")
        if (
            not isinstance(digest, str)
            or SHA256_RE.fullmatch(digest) is None
            or digest == ZERO_SHA256
        ):
            problems.append(f"attestation evidence {name} must have a nonzero SHA-256")
        expected_uri = f"artifact://{payload}/{relative}" if payload else None
        if item.get("uri") != expected_uri:
            problems.append(f"attestation evidence {name} is not payload-bound")

    filtered = copy.deepcopy(data)
    filtered["evidence"] = retained
    if evidence_names(filtered) != list(BASE_EVIDENCE_ORDER):
        problems.append("base candidate evidence order is not canonical after attestation split")
    return filtered, problems


def fixture_item(name: str, *, pending: bool = False) -> dict[str, Any]:
    return {
        "name": name,
        "status": "pending" if pending else "pass",
        "uri": None if pending else "artifact://payload/example.json",
        "sha256": None if pending else "sha256:" + "a" * 64,
        "waiver": None,
    }


def self_test() -> list[str]:
    base = {
        "build": {"artifacts": [{"name": "payload"}]},
        "evidence": [
            *[fixture_item(name) for name in BASE_EVIDENCE_ORDER],
            *[
                {
                    "name": name,
                    "status": "pass",
                    "uri": f"artifact://payload/{relative}",
                    "sha256": "sha256:" + "a" * 64,
                    "waiver": None,
                }
                for name, relative in ATTESTATIONS.items()
            ],
        ],
        "external_gates": {
            "status": "independent_approval_required",
            "items": list(EXTERNAL_GATES),
        },
    }
    filtered, problems = split_attestations(base)
    failures = []
    if (
        problems
        or filtered is None
        or evidence_names(filtered) != list(BASE_EVIDENCE_ORDER)
    ):
        failures.append("valid attestation fixture was rejected")

    def reorder(value: dict[str, Any]) -> None:
        value["evidence"][0], value["evidence"][1] = (
            value["evidence"][1],
            value["evidence"][0],
        )

    for label, mutate in (
        ("missing", lambda value: value["evidence"].pop()),
        (
            "waived",
            lambda value: value["evidence"][-1].update(
                status="waived", waiver="x"
            ),
        ),
        (
            "duplicate",
            lambda value: value["evidence"].append(
                copy.deepcopy(value["evidence"][-1])
            ),
        ),
        (
            "wrong-uri",
            lambda value: value["evidence"][-1].update(uri="file:///tmp/x"),
        ),
        ("reordered", reorder),
        (
            "external-drift",
            lambda value: value["external_gates"]["items"].__setitem__(
                0, "X1: shortened"
            ),
        ),
    ):
        candidate = copy.deepcopy(base)
        mutate(candidate)
        _, negative = split_attestations(candidate)
        if not negative:
            failures.append(f"negative self-test accepted {label}")

    template = {
        "evidence": [fixture_item(name, pending=True) for name in EXPECTED_EVIDENCE_ORDER],
        "external_gates": {
            "status": "independent_approval_required",
            "items": list(EXTERNAL_GATES),
        },
    }
    if outer_contract_problems(template):
        failures.append("valid template envelope was rejected")
    reordered_template = copy.deepcopy(template)
    reordered_template["evidence"][0], reordered_template["evidence"][1] = (
        reordered_template["evidence"][1],
        reordered_template["evidence"][0],
    )
    if not outer_contract_problems(reordered_template):
        failures.append("negative self-test accepted reordered template")
    return failures


def run_core(path: Path, allow_template: bool) -> tuple[int, str]:
    command = [sys.executable, str(CORE), str(path)]
    if allow_template:
        command.append("--allow-template")
    completed = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    return completed.returncode, completed.stdout


def print_output(output: str) -> None:
    if output:
        print(output, end="" if output.endswith("\n") else "\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--allow-template", action="store_true")
    args = parser.parse_args()

    problems = [f"checker self-test failed: {item}" for item in self_test()]
    try:
        data = json.loads(args.manifest.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        problems.append(f"cannot read manifest: {error}")
        data = None

    core_code = 1
    core_output = ""
    if data is not None:
        if args.allow_template:
            problems.extend(outer_contract_problems(data))
            core_code, core_output = run_core(args.manifest, True)
        else:
            filtered, attestation_problems = split_attestations(data)
            problems.extend(attestation_problems)
            if filtered is not None:
                with tempfile.TemporaryDirectory(
                    prefix="cex-manifest-contract-"
                ) as directory:
                    filtered_path = (
                        Path(directory) / "candidate-without-attestations.json"
                    )
                    filtered_path.write_text(
                        json.dumps(filtered, indent=2, sort_keys=True) + "\n",
                        encoding="utf-8",
                    )
                    core_code, core_output = run_core(filtered_path, False)

    print_output(core_output)
    if core_code != 0:
        problems.append("base v12 manifest contract failed")

    result = {
        "schema": "cex.release-baseline-attestation-contract.v1",
        "status": "failed" if problems else "ok",
        "mode": "template" if args.allow_template else "candidate",
        "manifest": str(args.manifest),
        "required_evidence_order": list(EXPECTED_EVIDENCE_ORDER),
        "required_attestations": ATTESTATIONS,
        "external_gates": list(EXTERNAL_GATES),
        "problems": problems,
    }
    print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
    return 1 if problems else 0


if __name__ == "__main__":
    raise SystemExit(main())
