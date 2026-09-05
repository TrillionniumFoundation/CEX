#!/usr/bin/env python3
"""Collect a closed-set TRNM build packet without rewriting source.

This collector binds local build-step observations and bytes. It does not prove
hosted execution, independent approval, deployment, or production readiness.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import tempfile
from typing import Mapping

REPOSITORY = "TrillionniumFoundation/CEX"
SCHEMA = "trnm_cex_settlement_build_evidence_v1"
SOURCE_FILES = {
    "Cargo.lock": "Cargo.lock",
    "settlement_v1.sql": "services/trnm-economy-service/migrations/settlement_v1.sql",
    "source-status.json": "docs/status/trnm-economy-settlement-v1.json",
}
BINARY = "target/release/trnm-economy-service"
PACKET_FILES = frozenset({*SOURCE_FILES, "trnm-economy-service", "manifest.json", "SHA256SUMS"})
STEP_KEYS = ("lock", "format", "tests", "clippy", "build", "source_unchanged")
MAX_TEXT = 16 * 1024 * 1024
MAX_BINARY = 256 * 1024 * 1024


class EvidenceError(RuntimeError):
    pass


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()


def positive(value: str, field: str) -> int:
    if not re.fullmatch(r"[1-9][0-9]{0,19}", value):
        raise EvidenceError(f"invalid {field}")
    return int(value)


def plain_path(path: Path) -> Path:
    path = path.absolute()
    if ".." in path.parts or any(ord(c) < 32 for c in str(path)):
        raise EvidenceError("invalid evidence path")
    for current in (path, *path.parents):
        if current.is_symlink():
            raise EvidenceError("symbolic evidence path is forbidden")
    return path


def read_regular(path: Path, limit: int) -> bytes:
    path = plain_path(path)
    try:
        fd = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0))
        with os.fdopen(fd, "rb") as stream:
            before = os.fstat(stream.fileno())
            if not stat.S_ISREG(before.st_mode) or before.st_size > limit or before.st_nlink != 1:
                raise EvidenceError("nonregular, linked, or oversized evidence input")
            value = stream.read(limit + 1)
            after = os.fstat(stream.fileno())
        if len(value) > limit or (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) != (
                after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns):
            raise EvidenceError("evidence input changed during acquisition")
        return value
    except OSError:
        raise EvidenceError("required evidence input is unavailable") from None


def git(root: Path, *arguments: str) -> bytes:
    try:
        result = subprocess.run(["git", "-C", str(root), *arguments], capture_output=True, timeout=30, check=False)
    except (OSError, subprocess.TimeoutExpired):
        raise EvidenceError("Git source verification could not execute") from None
    if result.returncode:
        raise EvidenceError("Git source verification failed")
    return result.stdout


def identity(root: Path) -> tuple[str, str]:
    if Path(os.fsdecode(git(root, "rev-parse", "--show-toplevel")).strip()).resolve() != root.resolve():
        raise EvidenceError("source root is not the checkout root")
    commit = git(root, "rev-parse", "HEAD").decode("ascii").strip()
    tree = git(root, "rev-parse", "HEAD^{tree}").decode("ascii").strip()
    if not all(re.fullmatch(r"[0-9a-f]{40}", item) for item in (commit, tree)):
        raise EvidenceError("invalid Git source identity")
    flags = git(root, "ls-files", "-v", "-z").split(b"\0")
    if any(entry and (entry[:1].islower() or entry[:1] == b"S") for entry in flags):
        raise EvidenceError("assume-unchanged or skip-worktree inputs are forbidden")
    git(root, "diff", "--no-ext-diff", "--no-textconv", "--exit-code", "HEAD", "--")
    # Ignored build products are outside the source set; unknown nonignored
    # files cannot silently participate in this exact-checkout packet.
    if git(root, "ls-files", "--others", "--exclude-standard", "-z"):
        raise EvidenceError("untracked source files remain in checkout")
    return commit, tree


def observed_context(environment: Mapping[str, str]) -> dict:
    if environment.get("GITHUB_REPOSITORY") != REPOSITORY or environment.get("GITHUB_ACTIONS") != "true":
        raise EvidenceError("collector requires the expected repository workflow context")
    head = environment.get("EXPECTED_HEAD_SHA", "")
    if not re.fullmatch(r"[0-9a-f]{40}", head):
        raise EvidenceError("invalid expected source head")
    event = environment.get("GITHUB_EVENT_NAME", "")
    if event not in {"push", "pull_request", "workflow_dispatch"}:
        raise EvidenceError("unsupported build evidence event")
    if environment.get("TRNM_STATIC_RESULT") != "success":
        raise EvidenceError("static dependency job did not succeed")
    outcomes = {key: environment.get("TRNM_" + key.upper() + "_OUTCOME", "") for key in STEP_KEYS}
    if any(value != "success" for value in outcomes.values()):
        raise EvidenceError("required build step did not succeed")
    toolchain = environment.get("TRNM_RUST_TOOLCHAIN", "")
    image = environment.get("TRNM_POSTGRES_IMAGE", "")
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", toolchain) or not re.fullmatch(r"postgres:16\.[0-9]+-(?:alpine|bookworm)", image):
        raise EvidenceError("declared build environment is missing or malformed")
    return {
        "rust_toolchain": toolchain,
        "postgres_image": image,
        "build_environment_identity": "workflow_declared_not_image_digest_attestation",
        "repository": REPOSITORY,
        "commit": head,
        "workflow_run_id": positive(environment.get("GITHUB_RUN_ID", ""), "workflow run ID"),
        "workflow_run_attempt": positive(environment.get("GITHUB_RUN_ATTEMPT", ""), "workflow attempt"),
        "event": event,
        "static_job_result": "success",
        "step_outcomes": outcomes,
    }


def parse_json(data: bytes) -> object:
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise EvidenceError("duplicate evidence JSON key")
            result[key] = value
        return result
    def invalid_constant(_):
        raise EvidenceError("nonfinite evidence JSON value")
    return json.loads(data, object_pairs_hook=pairs, parse_constant=invalid_constant)


def validate_source_status(data: bytes) -> None:
    try:
        value = parse_json(data)
    except (ValueError, UnicodeError):
        raise EvidenceError("invalid source status") from None
    if not isinstance(value, dict) or value.get("schema") != "trnm_cex_settlement_runtime_status_v1":
        raise EvidenceError("invalid source status schema")
    if (value.get("owner_repository") != REPOSITORY or value.get("status") != "implemented_pending_exact_commit_ci"
            or "verified_commit" not in value or value.get("release_effect") != "none"
            or value.get("trusted_settlement") is not False or value.get("public_online") is not False
            or value.get("public_player_market") is not False or value.get("verified_commit") is not None):
        raise EvidenceError("source status overclaims qualification")


def validate_binary(data: bytes) -> None:
    # This is only an ELF shape check; tests and execution are separate steps.
    if (len(data) < 64 or data[:6] != b"\x7fELF\x02\x01" or data[6] != 1
            or int.from_bytes(data[16:18], "little") not in {2, 3}
            or int.from_bytes(data[18:20], "little") != 62):
        raise EvidenceError("candidate binary is not an x86-64 ELF image")


def verify_packet(directory: Path) -> dict:
    directory = plain_path(directory)
    if not directory.is_dir() or {p.name for p in directory.iterdir()} != PACKET_FILES:
        raise EvidenceError("packet file set differs from the closed manifest")
    values = {name: read_regular(directory / name, MAX_BINARY if name == "trnm-economy-service" else MAX_TEXT)
              for name in PACKET_FILES}
    try:
        manifest = parse_json(values["manifest.json"])
    except (ValueError, UnicodeError):
        raise EvidenceError("invalid packet manifest") from None
    if not isinstance(manifest, dict) or manifest.get("schema") != SCHEMA:
        raise EvidenceError("invalid packet schema")
    if (manifest.get("repository") != REPOSITORY or manifest.get("static_job_result") != "success"
            or manifest.get("event") not in {"push", "pull_request", "workflow_dispatch"}
            or manifest.get("step_outcomes") != {key: "success" for key in STEP_KEYS}
            or any(not isinstance(manifest.get(key), str) or not re.fullmatch(r"[0-9a-f]{40}", manifest[key])
                   for key in ("commit", "tree"))
            or any(type(manifest.get(key)) is not int or manifest[key] <= 0
                   for key in ("workflow_run_id", "workflow_run_attempt"))):
        raise EvidenceError("invalid build observation context")
    commitment = manifest.pop("payload_sha256", None)
    if commitment != digest(canonical(manifest)):
        raise EvidenceError("manifest commitment mismatch")
    expected = {name: digest(values[name]) for name in {*SOURCE_FILES, "trnm-economy-service"}}
    if manifest.get("sha256") != expected:
        raise EvidenceError("packet payload digest mismatch")
    checksums = "".join(f"{digest(values[name])}  {name}\n" for name in sorted(PACKET_FILES - {"SHA256SUMS"}))
    if values["SHA256SUMS"] != checksums.encode():
        raise EvidenceError("packet checksum list mismatch")
    if manifest.get("production_authorization") != "not_granted":
        raise EvidenceError("packet cannot grant production authorization")
    validate_binary(values["trnm-economy-service"])
    validate_source_status(values["source-status.json"])
    manifest["payload_sha256"] = commitment
    return manifest


def collect(root: Path, output_parent: Path, environment: Mapping[str, str]) -> Path:
    root, output_parent = plain_path(root), plain_path(output_parent)
    if not root.is_dir() or not output_parent.is_dir():
        raise EvidenceError("source and output parent directories must exist")
    try:
        output_parent.relative_to(root)
    except ValueError:
        pass
    else:
        raise EvidenceError("build evidence must be outside the source checkout")
    context = observed_context(environment)
    before = identity(root)
    if context["commit"] != before[0]:
        raise EvidenceError("checkout does not match the requested source head")
    inputs = {name: read_regular(root / relative, MAX_TEXT) for name, relative in SOURCE_FILES.items()}
    for name, relative in SOURCE_FILES.items():
        if git(root, "cat-file", "blob", "HEAD:" + relative) != inputs[name]:
            raise EvidenceError("packet source bytes differ from committed bytes")
    inputs["trnm-economy-service"] = read_regular(root / BINARY, MAX_BINARY)
    validate_binary(inputs["trnm-economy-service"])
    validate_source_status(inputs["source-status.json"])
    manifest = {
        "schema": SCHEMA,
        **context,
        "tree": before[1],
        "contract": "trnm_cex_settlement_receipt_lookup_v1",
        "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "checks": ["exact-head-checkout", "committed-lock-unchanged", "static-dependency-job-success",
                   "required-step-outcomes-success", "closed-packet-file-set", "packet-readback-digests"],
        "sha256": {name: digest(data) for name, data in inputs.items()},
        "production_authorization": "not_granted",
        "limitations": ["Workflow step observations are supplied by the hosting workflow, not independently certified here.",
                        "ELF shape and hashes do not prove execution, reproducible compilation or deployment.",
                        "The full repository candidate manifest and independent external gates remain mandatory.",
                        "No trusted-settlement, public-online or production promotion is granted."],
    }
    manifest["payload_sha256"] = digest(canonical(manifest))
    values = {**inputs, "manifest.json": (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode()}
    values["SHA256SUMS"] = "".join(f"{digest(values[name])}  {name}\n" for name in sorted(values)).encode()
    destination = Path(tempfile.mkdtemp(prefix=f"trnm-build-{before[0]}-{context['workflow_run_id']}-{context['workflow_run_attempt']}-",
                                        dir=output_parent))
    try:
        for name, data in values.items():
            mode = 0o700 if name == "trnm-economy-service" else 0o600
            fd = os.open(destination / name, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
            with os.fdopen(fd, "wb") as stream:
                stream.write(data)
                stream.flush()
                os.fsync(stream.fileno())
        verify_packet(destination)
        for name, relative in {**SOURCE_FILES, "trnm-economy-service": BINARY}.items():
            if read_regular(root / relative, MAX_BINARY if name == "trnm-economy-service" else MAX_TEXT) != inputs[name]:
                raise EvidenceError("build input changed while collecting packet")
        if identity(root) != before:
            raise EvidenceError("source identity changed while collecting packet")
    except Exception:
        shutil.rmtree(destination)
        raise
    return destination


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--output-parent", type=Path)
    parser.add_argument("--verify", type=Path)
    args = parser.parse_args()
    try:
        if args.verify is not None:
            if args.output_parent is not None:
                raise EvidenceError("verification cannot also collect a packet")
            verify_packet(args.verify)
            print("TRNM build packet integrity: OK; not production authorization")
        else:
            if args.output_parent is None:
                raise EvidenceError("an external output parent is required")
            print(collect(args.root, args.output_parent, os.environ))
        return 0
    except (EvidenceError, OSError, UnicodeError):
        print("TRNM build packet collection/verification failed; no qualification granted", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
