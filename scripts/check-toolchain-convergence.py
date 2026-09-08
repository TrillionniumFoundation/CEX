#!/usr/bin/env python3
"""Reject mixed or floating Rust toolchains on active CEX build/release surfaces."""

from __future__ import annotations

import json
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXPECTED = "1.98.1"
PIN = ROOT / "rust-toolchain.toml"
ACTIVE_EXACT = (
    "services/hepta-research-league/Dockerfile",
    "services/hepta-research-league/docker/rust-toolchain.manifest",
    "docs/security/rust-advisory-exceptions-v1.json",
)
ACTIVE_GLOBS = (
    ".github/workflows/*.yml",
    ".github/workflows/*.yaml",
)
FORBIDDEN = (
    (re.compile(r"(?<![0-9])1\.98\.0(?![0-9])"), "unpatched Rust 1.98.0"),
    (re.compile(r"(?<![0-9])1\.95\.0(?![0-9])"), "stale Rust 1.95.0"),
    (re.compile(r"(?im)^\s*(?:toolchain\s*:\s*|rustup\s+(?:default|toolchain\s+install)\s+)(?:stable|latest)(?:\s|$)"), "floating Rust channel"),
)
BUILD_MARKERS = (
    "cargo build",
    "cargo test",
    "cargo clippy",
    "cargo fmt",
    "rust-toolchain",
    "rustup toolchain install",
    "dtolnay/rust-toolchain",
)


def active_paths() -> list[Path]:
    paths = {ROOT / relative for relative in ACTIVE_EXACT}
    for pattern in ACTIVE_GLOBS:
        paths.update(ROOT.glob(pattern))
    return sorted(path for path in paths if path.is_file())


def main() -> int:
    problems: list[str] = []
    try:
        pin = tomllib.loads(PIN.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        problems.append(f"cannot read rust-toolchain.toml: {error}")
        pin = {}
    toolchain = pin.get("toolchain", {}) if isinstance(pin, dict) else {}
    if toolchain.get("channel") != EXPECTED:
        problems.append(f"rust-toolchain.toml must pin {EXPECTED}")
    if toolchain.get("profile") != "minimal":
        problems.append("rust-toolchain.toml must use the minimal profile")
    components = toolchain.get("components")
    if not isinstance(components, list) or set(components) != {"rustfmt", "clippy"}:
        problems.append("rust-toolchain.toml must install rustfmt and clippy exactly")

    scanned: list[str] = []
    build_surfaces: list[str] = []
    exact_markers: list[str] = []
    for path in active_paths():
        relative = path.relative_to(ROOT).as_posix()
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError) as error:
            problems.append(f"cannot read active toolchain surface {relative}: {error}")
            continue
        scanned.append(relative)
        lower = text.lower()
        is_build_surface = any(marker in lower for marker in BUILD_MARKERS)
        if is_build_surface:
            build_surfaces.append(relative)
        for pattern, label in FORBIDDEN:
            for match in pattern.finditer(text):
                line = text.count("\n", 0, match.start()) + 1
                problems.append(f"{relative}:{line}: {label}")
        if EXPECTED in text:
            exact_markers.append(relative)
        if relative == "docs/security/rust-advisory-exceptions-v1.json":
            try:
                policy = json.loads(text)
            except json.JSONDecodeError as error:
                problems.append(f"invalid advisory policy JSON: {error}")
            else:
                if policy.get("rust_toolchain") != EXPECTED:
                    problems.append("Rust advisory policy toolchain does not match repository pin")

    for relative in ACTIVE_EXACT:
        if relative not in scanned:
            problems.append(f"missing active toolchain surface: {relative}")
    if not build_surfaces:
        problems.append("no active Rust build surface was discovered")
    if not exact_markers:
        problems.append(f"no active surface names exact Rust {EXPECTED}")

    result = {
        "schema": "cex.rust-toolchain-convergence-check.v1",
        "status": "failed" if problems else "ok",
        "expected_rust_toolchain": EXPECTED,
        "active_surfaces_scanned": scanned,
        "build_surfaces": build_surfaces,
        "exact_version_surfaces": exact_markers,
        "production_authorization": "not_granted",
        "problems": problems,
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
