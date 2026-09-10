#!/usr/bin/env python3
"""Fail-closed external-evidence intake over one immutable candidate snapshot."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import re
import stat
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[1]
CORE = ROOT / "scripts/check-external-production-evidence-contract-core.py"
BINDING = ROOT / "scripts/check-external-production-evidence-binding-core.py"
MANIFEST_CHECKER = ROOT / "scripts/check-release-baseline-manifest.py"
CONTRACT = ROOT / "docs/external-production-evidence-contract-v1.md"
ADDENDUM = ROOT / "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md"
TRACEABILITY = ROOT / "docs/traceability/v12-requirements-v1.json"
TRIGGER = ROOT / "docs/release-evidence/p0-candidate-trigger.json"
GATES = tuple(f"V12-X{i}" for i in range(1, 9))
DIGEST_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
URI_RE = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*://")
MAX_BYTES = 64 * 1024 * 1024
MANIFEST_RESULT_SCHEMA = "cex.active-v12-manifest-guard.v1"
MIGRATION_HEAD = "0088_enforce_provider_terminal_evidence_binding.sql"
SEQUENCE = 54


class IntakeError(Exception):
    pass


def load_binding() -> Any:
    spec = importlib.util.spec_from_file_location("cex_binding_core", BINDING)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load external evidence binding core")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def within(path: Path, root: Path) -> bool:
    try:
        path.resolve().relative_to(root.resolve())
        return True
    except (OSError, ValueError):
        return False


def state(value: os.stat_result) -> tuple[int, int, int, int, int]:
    return (
        int(value.st_dev),
        int(value.st_ino),
        int(value.st_size),
        int(value.st_mtime_ns),
        int(value.st_ctime_ns),
    )


def exact_read(path: Path, label: str) -> bytes:
    try:
        if path.is_symlink():
            raise IntakeError(f"{label} path may not be a symlink")
        resolved = path.resolve(strict=True)
    except OSError as error:
        raise IntakeError(f"cannot resolve {label}: {error}") from error
    if within(resolved, ROOT):
        raise IntakeError(f"{label} must remain outside the source tree")

    flags = os.O_RDONLY | getattr(os, "O_BINARY", 0)
    flags |= getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(resolved, flags)
    except OSError as error:
        raise IntakeError(f"cannot open {label} without following links: {error}") from error
    try:
        before = os.fstat(descriptor)
        if not stat.S_ISREG(before.st_mode):
            raise IntakeError(f"{label} must be a regular file")
        if before.st_size > MAX_BYTES:
            raise IntakeError(f"{label} exceeds the {MAX_BYTES}-byte intake limit")
        chunks: list[bytes] = []
        total = 0
        while True:
            chunk = os.read(descriptor, min(1024 * 1024, MAX_BYTES + 1 - total))
            if not chunk:
                break
            chunks.append(chunk)
            total += len(chunk)
            if total > MAX_BYTES:
                raise IntakeError(f"{label} exceeds the {MAX_BYTES}-byte intake limit")
        after = os.fstat(descriptor)
        raw = b"".join(chunks)
        if state(before) != state(after) or len(raw) != after.st_size:
            raise IntakeError(f"{label} changed while its immutable bytes were read")
        return raw
    finally:
        os.close(descriptor)


def snapshot(directory: Path, name: str, raw: bytes) -> Path:
    path = directory / name
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
    flags |= getattr(os, "O_BINARY", 0) | getattr(os, "O_CLOEXEC", 0)
    descriptor = os.open(path, flags, 0o600)
    try:
        view = memoryview(raw)
        offset = 0
        while offset < len(view):
            count = os.write(descriptor, view[offset:])
            if count <= 0:
                raise IntakeError(f"cannot complete immutable snapshot write: {name}")
            offset += count
        os.fsync(descriptor)
    finally:
        os.close(descriptor)
    os.chmod(path, 0o400)
    return path


def verify_snapshot(path: Path, raw: bytes, label: str) -> None:
    try:
        if path.read_bytes() != raw:
            raise IntakeError(f"{label} snapshot changed during validation")
    except OSError as error:
        raise IntakeError(f"cannot verify {label} snapshot: {error}") from error


def trailing_object(raw: str) -> dict[str, Any] | None:
    decoder = json.JSONDecoder()
    for index in range(len(raw) - 1, -1, -1):
        if raw[index] != "{":
            continue
        try:
            value, used = decoder.raw_decode(raw[index:])
        except json.JSONDecodeError:
            continue
        if not raw[index + used :].strip() and isinstance(value, dict):
            return value
    return None


def run(arguments: list[str]) -> tuple[int, dict[str, Any] | None, str]:
    completed = subprocess.run(
        arguments,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    output = completed.stdout.strip()
    return completed.returncode, trailing_object(output), output or "<no output>"


def object_bytes(raw: bytes, label: str, problems: list[str]) -> dict[str, Any]:
    try:
        value = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        problems.append(f"{label} is not valid UTF-8 JSON: {error}")
        return {}
    if not isinstance(value, dict):
        problems.append(f"{label} root must be an object")
        return {}
    return value


def child_result(
    code: int,
    result: dict[str, Any] | None,
    output: str,
    label: str,
) -> tuple[list[str], bool]:
    if result is None:
        return [f"{label} did not emit a trailing JSON object: {output}"], False
    problems: list[str] = []
    values = result.get("problems")
    if isinstance(values, list):
        problems.extend(f"{label}: {item}" for item in values)
    elif result.get("status") != "ok":
        problems.append(f"{label} failed without structured diagnostics")
    if result.get("schema") != "cex.external-production-evidence-contract-check.v1":
        problems.append(f"{label} emitted an unexpected schema")
    if result.get("production_authorization") != "not_granted":
        problems.append(f"{label} changed production authorization")
    if result.get("checker_may_grant_production_authorization") is not False:
        problems.append(f"{label} may not grant production authorization")
    if code != 0 and not values:
        problems.append(f"{label} exited nonzero without diagnostics")
    return problems, code == 0 and result.get("status") == "ok" and not problems


def collect(value: object, uris: set[str], digests: set[str]) -> None:
    if isinstance(value, dict):
        for child in value.values():
            collect(child, uris, digests)
    elif isinstance(value, list):
        for child in value:
            collect(child, uris, digests)
    elif isinstance(value, str):
        if DIGEST_RE.fullmatch(value):
            digests.add(value)
        if URI_RE.match(value):
            uris.add(value)


def repository_uri(value: object) -> bool:
    if not isinstance(value, str):
        return False
    lowered = value.lower()
    if lowered.startswith("gh://trillionniumfoundation/cex/"):
        return True
    if lowered.startswith("artifact://cex-p0-evidence-"):
        return True
    try:
        parsed = urlsplit(value)
    except ValueError:
        return False
    return (
        parsed.netloc.lower()
        in {"github.com", "api.github.com", "raw.githubusercontent.com"}
        and "/trillionniumfoundation/cex/" in parsed.path.lower() + "/"
    )


def identity_isolation(bundle: dict[str, Any], manifest: dict[str, Any]) -> list[str]:
    reserved_uris: set[str] = set()
    reserved_digests: set[str] = set()
    collect(manifest, reserved_uris, reserved_digests)
    reference = bundle.get("repository_candidate_manifest")
    if isinstance(reference, dict):
        if isinstance(reference.get("uri"), str):
            reserved_uris.add(reference["uri"])
        if isinstance(reference.get("sha256"), str):
            reserved_digests.add(reference["sha256"])

    problems: list[str] = []
    gates = bundle.get("gates")
    if not isinstance(gates, list):
        return problems
    for gate_index, gate in enumerate(gates):
        evidence = gate.get("evidence") if isinstance(gate, dict) else None
        if not isinstance(evidence, list):
            continue
        for record_index, record in enumerate(evidence):
            if not isinstance(record, dict):
                continue
            label = f"gates[{gate_index}].evidence[{record_index}]"
            uri = record.get("uri")
            digest = record.get("sha256")
            if isinstance(uri, str) and uri in reserved_uris:
                problems.append(f"{label}.uri reuses candidate-manifest or repository evidence")
            if isinstance(digest, str) and digest in reserved_digests:
                problems.append(f"{label}.sha256 reuses candidate-manifest or repository evidence")
            if repository_uri(uri):
                problems.append(f"{label}.uri is repository-owned and cannot prove an external gate")
    return problems


def wiring() -> list[str]:
    problems: list[str] = []
    for path in (CORE, BINDING, MANIFEST_CHECKER, CONTRACT, ADDENDUM, TRACEABILITY, TRIGGER):
        if not path.is_file():
            problems.append(f"missing required Sequence-50 path: {path.relative_to(ROOT)}")

    def text(path: Path, label: str) -> str:
        try:
            return path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError) as error:
            problems.append(f"cannot read {label}: {error}")
            return ""

    contract = text(CONTRACT, "external evidence contract")
    for marker in (
        "single-read immutable snapshot",
        "candidate-manifest or repository evidence",
        "trailing JSON object",
        "repository-owned",
    ):
        if marker not in contract:
            problems.append(f"external evidence contract lacks Sequence-50 marker: {marker}")

    addendum = text(ADDENDUM, "implementation addendum")
    for marker in (
        "single-read immutable snapshot",
        "repository-owned evidence identity",
        "trailing JSON object",
    ):
        if marker not in addendum:
            problems.append(f"implementation addendum lacks Sequence-50 marker: {marker}")

    try:
        trace = json.loads(TRACEABILITY.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        trace = {}
        problems.append(f"cannot read v12 traceability: {error}")
    requirements = trace.get("requirements") if isinstance(trace, dict) else None
    v12_h = next(
        (item for item in requirements if isinstance(item, dict) and item.get("id") == "V12-H"),
        None,
    ) if isinstance(requirements, list) else None
    if not isinstance(v12_h, dict):
        problems.append("V12-H traceability entry is missing")
    else:
        observed = {str(item) for item in v12_h.get("implementation", [])}
        required = {
            "scripts/check-external-production-evidence-contract-core.py",
            "scripts/check-external-production-evidence-binding-core.py",
            "scripts/check-external-production-evidence-contract.py",
        }
        if required - observed:
            problems.append(
                "V12-H implementation omits Sequence-50 evidence paths: "
                + ",".join(sorted(required - observed))
            )

    try:
        trigger = json.loads(TRIGGER.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        trigger = {}
        problems.append(f"cannot read shared candidate trigger: {error}")
    if not isinstance(trigger, dict) or trigger.get("sequence") != SEQUENCE:
        problems.append(f"shared candidate trigger must be sequence {SEQUENCE}")
    if isinstance(trigger, dict) and trigger.get("production_authorization") != "not_granted":
        problems.append("shared candidate trigger changed production authorization")
    return problems


def self_tests(binding: Any) -> tuple[list[str], int]:
    failures, count = binding.run_self_tests()
    mixed = "diagnostic\n" + json.dumps(
        {"schema": MANIFEST_RESULT_SCHEMA, "status": "ok"}, indent=2
    )
    count += 1
    if (trailing_object(mixed) or {}).get("status") != "ok":
        failures.append("trailing JSON parser rejected authoritative mixed stdout")
    count += 1
    if trailing_object(mixed + "\ntrailing garbage") is not None:
        failures.append("trailing JSON parser accepted trailing data")

    manifest = binding.base_manifest()
    raw = (json.dumps(manifest, sort_keys=True) + "\n").encode()
    original = binding.base_bundle(raw)

    value = json.loads(json.dumps(original))
    value["gates"][0]["evidence"][0]["uri"] = value["repository_candidate_manifest"]["uri"]
    count += 1
    if not any("candidate-manifest" in item for item in identity_isolation(value, manifest)):
        failures.append("candidate-manifest URI alias was accepted")

    value = json.loads(json.dumps(original))
    value["gates"][0]["evidence"][0]["sha256"] = value["repository_candidate_manifest"]["sha256"]
    count += 1
    if not any("candidate-manifest" in item for item in identity_isolation(value, manifest)):
        failures.append("candidate-manifest digest alias was accepted")

    nested = json.loads(json.dumps(manifest))
    nested["build"] = {"artifacts": [{
        "uri": "artifact://cex-p0-evidence-internal/candidate.json",
        "sha256": "sha256:" + "e" * 64,
    }]}
    value = json.loads(json.dumps(original))
    value["gates"][0]["evidence"][0].update(nested["build"]["artifacts"][0])
    count += 1
    if len(identity_isolation(value, nested)) < 2:
        failures.append("nested candidate artifact identity was accepted")

    value = json.loads(json.dumps(original))
    value["gates"][0]["evidence"][0]["uri"] = (
        "gh://TrillionniumFoundation/CEX/actions/runs/1/attempts/1"
    )
    count += 1
    if not any("repository-owned" in item for item in identity_isolation(value, manifest)):
        failures.append("repository-owned Actions URI was accepted")

    count += 1
    try:
        with tempfile.TemporaryDirectory(prefix="cex-external-single-read-") as directory:
            source = Path(directory) / "source.json"
            source.write_bytes(b'{"schema":"test"}\n')
            value = exact_read(source, "self-test input")
            target = Path(directory) / "snapshots"
            target.mkdir(mode=0o700)
            copied = snapshot(target, "input.json", value)
            verify_snapshot(copied, value, "self-test")
    except (OSError, IntakeError) as error:
        failures.append(f"single-read snapshot self-test failed: {error}")
    return failures, count


def emit(
    mode: str,
    problems: list[str],
    eligible: bool = False,
    cases: int | None = None,
) -> int:
    result: dict[str, Any] = {
        "schema": (
            "cex.external-production-evidence-binding-self-test.v1"
            if mode == "self_test"
            else "cex.external-production-evidence-contract-check.v1"
        ),
        "status": "failed" if problems else "ok",
        "gate_ids": list(GATES),
        "production_authorization": "not_granted",
        "checker_may_grant_production_authorization": False,
        "problems": problems,
    }
    if mode == "self_test":
        result["cases"] = cases
    else:
        result["mode"] = mode
        result["structurally_eligible_for_human_decision"] = eligible
    print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
    return 1 if problems else 0


def main() -> int:
    parser = argparse.ArgumentParser()
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument("--contract-only", action="store_true")
    modes.add_argument("--bundle", type=Path)
    modes.add_argument("--self-test", action="store_true")
    parser.add_argument("--candidate-manifest", type=Path)
    args = parser.parse_args()
    if args.bundle is not None and args.candidate_manifest is None:
        parser.error("--candidate-manifest is required with --bundle")
    if args.bundle is None and args.candidate_manifest is not None:
        parser.error("--candidate-manifest is only valid with --bundle")

    try:
        binding = load_binding()
    except Exception as error:
        return emit(
            "self_test" if args.self_test else "contract_only",
            [f"cannot load external evidence binding core: {error}"],
            cases=0 if args.self_test else None,
        )

    if args.self_test:
        failures, count = self_tests(binding)
        return emit("self_test", failures, cases=count)

    if args.contract_only:
        code, result, output = run([sys.executable, str(CORE), "--contract-only"])
        problems, _ = child_result(code, result, output, "external evidence core")
        problems.extend(wiring())
        return emit("contract_only", problems)

    try:
        bundle_raw = exact_read(args.bundle, "external evidence bundle")
        manifest_raw = exact_read(args.candidate_manifest, "candidate manifest")
    except IntakeError as error:
        return emit("bundle", [str(error)])

    problems: list[str] = []
    bundle = object_bytes(bundle_raw, "external evidence bundle", problems)
    manifest = object_bytes(manifest_raw, "candidate manifest", problems)
    core_eligible = False
    binding_eligible = False

    try:
        with tempfile.TemporaryDirectory(prefix="cex-external-snapshot-") as directory:
            root = Path(directory)
            bundle_path = snapshot(root, "bundle.json", bundle_raw)
            manifest_path = snapshot(root, "candidate-manifest.json", manifest_raw)

            code, result, output = run(
                [sys.executable, str(MANIFEST_CHECKER), str(manifest_path)]
            )
            manifest_ok = (
                code == 0
                and isinstance(result, dict)
                and result.get("schema") == MANIFEST_RESULT_SCHEMA
                and result.get("status") == "ok"
                and result.get("expected_migration_head") == MIGRATION_HEAD
                and result.get("manifest") == str(manifest_path)
            )
            if not manifest_ok:
                problems.append(
                    "candidate manifest failed the authoritative trailing-JSON "
                    "validator contract: " + output
                )

            code, result, output = run(
                [sys.executable, str(CORE), "--bundle", str(bundle_path)]
            )
            child, core_ok = child_result(code, result, output, "external evidence core")
            problems.extend(child)
            core_eligible = (
                core_ok
                and isinstance(result, dict)
                and result.get("structurally_eligible_for_human_decision") is True
            )

            child, binding_eligible = binding.validate_binding(
                bundle,
                manifest_raw,
                manifest,
                validator_ok=manifest_ok,
                bundle_path=bundle_path,
            )
            problems.extend(child)
            problems.extend(identity_isolation(bundle, manifest))
            verify_snapshot(bundle_path, bundle_raw, "external evidence bundle")
            verify_snapshot(manifest_path, manifest_raw, "candidate manifest")
    except (OSError, IntakeError) as error:
        problems.append(str(error))

    return emit(
        "bundle",
        problems,
        not problems and core_eligible and binding_eligible,
    )


if __name__ == "__main__":
    raise SystemExit(main())
