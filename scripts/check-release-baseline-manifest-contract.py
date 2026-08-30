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
ATTESTATIONS = {
    "local-evidence-binding": "local-evidence-binding.json",
    "hosted-gate-execution": "hosted-gate-execution.json",
}


def payload_name(data: dict[str, Any]) -> str | None:
    build = data.get("build")
    artifacts = build.get("artifacts") if isinstance(build, dict) else None
    if not isinstance(artifacts, list) or len(artifacts) != 1:
        return None
    item = artifacts[0]
    value = item.get("name") if isinstance(item, dict) else None
    return value if isinstance(value, str) and value else None


def split_attestations(data: Any) -> tuple[dict[str, Any] | None, list[str]]:
    problems: list[str] = []
    if not isinstance(data, dict):
        return None, ["manifest root must be an object"]
    evidence = data.get("evidence")
    if not isinstance(evidence, list):
        return None, ["candidate evidence must be a list"]

    retained: list[Any] = []
    found: dict[str, dict[str, Any]] = {}
    all_names: list[str] = []
    for item in evidence:
        if not isinstance(item, dict):
            retained.append(item)
            continue
        name = item.get("name")
        if isinstance(name, str):
            all_names.append(name)
        if name in ATTESTATIONS:
            if name in found:
                problems.append(f"duplicate attestation evidence: {name}")
            else:
                found[name] = item
        else:
            retained.append(item)

    if len(all_names) != len(set(all_names)):
        problems.append("candidate evidence names must be unique")
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
    return filtered, problems


def self_test() -> list[str]:
    digest = "sha256:" + "a" * 64
    base = {
        "build": {"artifacts": [{"name": "payload"}]},
        "evidence": [
            {
                "name": name,
                "status": "pass",
                "uri": f"artifact://payload/{relative}",
                "sha256": digest,
                "waiver": None,
            }
            for name, relative in ATTESTATIONS.items()
        ],
    }
    filtered, problems = split_attestations(base)
    failures = []
    if problems or filtered is None or filtered.get("evidence") != []:
        failures.append("valid attestation fixture was rejected")

    for label, mutate in (
        ("missing", lambda value: value["evidence"].pop()),
        ("waived", lambda value: value["evidence"][0].update(status="waived", waiver="x")),
        ("duplicate", lambda value: value["evidence"].append(copy.deepcopy(value["evidence"][0]))),
        ("wrong-uri", lambda value: value["evidence"][0].update(uri="file:///tmp/x")),
    ):
        candidate = copy.deepcopy(base)
        mutate(candidate)
        _, negative = split_attestations(candidate)
        if not negative:
            failures.append(f"negative self-test accepted {label}")
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


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--allow-template", action="store_true")
    args = parser.parse_args()

    self_test_failures = self_test()
    if self_test_failures:
        print(json.dumps({"status": "failed", "problems": self_test_failures}, indent=2))
        return 1

    if args.allow_template:
        code, output = run_core(args.manifest, True)
        if output:
            print(output, end="" if output.endswith("\n") else "\n")
        return code

    try:
        data = json.loads(args.manifest.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        print(f"release attestation validation failed: {error}")
        return 1

    filtered, problems = split_attestations(data)
    core_code = 1
    core_output = ""
    if filtered is not None:
        with tempfile.TemporaryDirectory(prefix="cex-manifest-contract-") as directory:
            filtered_path = Path(directory) / "candidate-without-attestations.json"
            filtered_path.write_text(
                json.dumps(filtered, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
            core_code, core_output = run_core(filtered_path, False)
    if core_output:
        print(core_output, end="" if core_output.endswith("\n") else "\n")
    if core_code != 0:
        problems.append("base v12 candidate-manifest contract failed")

    result = {
        "schema": "cex.release-baseline-attestation-contract.v1",
        "status": "failed" if problems else "ok",
        "manifest": str(args.manifest),
        "required_attestations": ATTESTATIONS,
        "problems": problems,
    }
    print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
    return 1 if problems else 0


if __name__ == "__main__":
    raise SystemExit(main())
