#!/usr/bin/env python3
"""Fail closed when active CEX build surfaces diverge from Rust 1.98.1."""

from __future__ import annotations

import json
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXPECTED = "1.98.1"
PROBLEMS: list[str] = []

ROOT_TOOLCHAIN = ROOT / "rust-toolchain.toml"
DOCKERFILE = ROOT / "services/hepta-research-league/Dockerfile"
DOCKER_INTENT = ROOT / "services/hepta-research-league/docker/rust-toolchain.manifest"
WORKFLOW_ROOT = ROOT / ".github/workflows"

OLD_ACTIVE_VERSION = re.compile(r"(?<![0-9.])(?:1\.95\.0|1\.98\.0)(?![0-9.])")
FLOATING_TOOLCHAIN = re.compile(
    r"(?im)(?:toolchain\s*:\s*|rustup\s+(?:default|toolchain\s+install)\s+)(?:stable|latest|beta|nightly)(?:\s|$)"
)
FLOATING_RUST_IMAGE = re.compile(r"(?im)^\s*FROM\s+(?:docker\.io/library/)?rust:(?:latest|stable)\b")
RUST_ACTIVITY = re.compile(
    r"(?im)(?:\bcargo\s+(?:build|check|clippy|test|run|metadata|tree|audit|deny)\b|"
    r"\brustc\s+--version\b|\brustup\b|dtolnay/rust-toolchain@)"
)


def read(path: Path) -> str:
    if not path.is_file():
        PROBLEMS.append(f"missing active toolchain surface: {path.relative_to(ROOT).as_posix()}")
        return ""
    try:
        return path.read_text(encoding="utf-8")
    except UnicodeDecodeError as error:
        PROBLEMS.append(f"active toolchain surface is not UTF-8: {path}: {error}")
        return ""


def check_root_toolchain() -> None:
    text = read(ROOT_TOOLCHAIN)
    if not text:
        return
    try:
        value = tomllib.loads(text)
    except tomllib.TOMLDecodeError as error:
        PROBLEMS.append(f"invalid rust-toolchain.toml: {error}")
        return
    toolchain = value.get("toolchain")
    if not isinstance(toolchain, dict):
        PROBLEMS.append("rust-toolchain.toml lacks [toolchain]")
        return
    if toolchain.get("channel") != EXPECTED:
        PROBLEMS.append(
            f"workspace Rust channel must be {EXPECTED}, got {toolchain.get('channel')!r}"
        )
    if toolchain.get("profile") != "minimal":
        PROBLEMS.append("workspace Rust profile must be minimal")
    components = toolchain.get("components")
    if not isinstance(components, list) or set(components) != {"rustfmt", "clippy"}:
        PROBLEMS.append("workspace Rust components must be exactly rustfmt and clippy")


def check_workflows() -> list[str]:
    checked: list[str] = []
    if not WORKFLOW_ROOT.is_dir():
        PROBLEMS.append("missing .github/workflows")
        return checked
    for path in sorted([*WORKFLOW_ROOT.glob("*.yml"), *WORKFLOW_ROOT.glob("*.yaml")]):
        text = read(path)
        relative = path.relative_to(ROOT).as_posix()
        if OLD_ACTIVE_VERSION.search(text):
            PROBLEMS.append(f"active workflow retains old Rust version: {relative}")
        if FLOATING_TOOLCHAIN.search(text):
            PROBLEMS.append(f"active workflow uses a floating Rust toolchain: {relative}")
        if RUST_ACTIVITY.search(text):
            checked.append(relative)
            if EXPECTED not in text:
                PROBLEMS.append(
                    f"Rust-active workflow does not bind exact {EXPECTED}: {relative}"
                )
    if not checked:
        PROBLEMS.append("no Rust-active workflow was discovered")
    return checked


def check_hepta_container() -> None:
    manifest = read(DOCKER_INTENT)
    expected_lines = [
        "bootstrap_builder=docker.io/library/rust@sha256:4c2fd73ef19c5ef9d54bee03b06b2839a392604fbfcd578ed948b71b37c1d7fb",
        f"rust_toolchain={EXPECTED}",
        "rustup_dist_server=https://static.rust-lang.org",
    ]
    if manifest.splitlines() != expected_lines:
        PROBLEMS.append("Hepta Docker toolchain intent must match the exact three-line policy")

    dockerfile = read(DOCKERFILE)
    for marker in (
        f"ARG RUST_TOOLCHAIN={EXPECTED}",
        "RUSTUP_DIST_SERVER=https://static.rust-lang.org",
        'channel-rust-${RUST_TOOLCHAIN}.toml.sha256',
        "sha256sum --check --strict",
        'rustup toolchain install "$RUST_TOOLCHAIN" --profile minimal --component rustfmt,clippy',
        'test "$(rustc --version | awk \'{print $2}\')" = "$RUST_TOOLCHAIN"',
        "/toolchain/rustc-version.txt",
        "/toolchain/cargo-version.txt",
        'io.trillionnium.hepta.rust-toolchain="1.98.1"',
    ):
        if marker not in dockerfile:
            PROBLEMS.append(f"Hepta Dockerfile lacks exact-toolchain marker: {marker}")
    if OLD_ACTIVE_VERSION.search(dockerfile):
        PROBLEMS.append("Hepta Dockerfile retains an old active Rust version")
    if FLOATING_TOOLCHAIN.search(dockerfile) or FLOATING_RUST_IMAGE.search(dockerfile):
        PROBLEMS.append("Hepta Dockerfile uses a floating Rust authority")


def main() -> int:
    check_root_toolchain()
    workflows = check_workflows()
    check_hepta_container()
    result = {
        "schema": "cex.rust-toolchain-convergence-check.v1",
        "status": "failed" if PROBLEMS else "ok",
        "expected_rust_toolchain": EXPECTED,
        "rust_active_workflows": workflows,
        "hepta_container_attestation": "embedded_in_artifact",
        "historical_evidence_scanned": False,
        "production_authorization": "not_granted",
        "problems": PROBLEMS,
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 1 if PROBLEMS else 0


if __name__ == "__main__":
    sys.exit(main())
