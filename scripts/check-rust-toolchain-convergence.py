#!/usr/bin/env python3
"""Fail closed when active CEX build lanes disagree on the Rust compiler."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXPECTED = "1.98.1"
PROBLEMS: list[str] = []

ACTIVE_ROOTS = (
    ROOT / ".github/workflows",
    ROOT / "services/hepta-research-league",
    ROOT / "scripts",
    ROOT / "docs/development-doc-authority-v1.json",
    ROOT / "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md",
)
SKIP_PARTS = {"target", ".git", "archive", "historical", "evidence"}
TEXT_SUFFIXES = {".yml", ".yaml", ".toml", ".json", ".md", ".txt", ".manifest", ".sh", ".ps1", ".py"}
VERSION = re.compile(r"(?<![0-9])1\.(?:95\.0|98\.0|98\.1)(?![0-9])")
FLOATING = re.compile(r"(?i)(?:toolchain\s*:\s*|rustup\s+(?:default|toolchain\s+install)\s+)(stable|latest)\b")


def iter_files() -> list[Path]:
    found: set[Path] = set()
    for root in ACTIVE_ROOTS:
        if root.is_file():
            found.add(root)
            continue
        if not root.is_dir():
            PROBLEMS.append(f"missing active toolchain surface: {root.relative_to(ROOT)}")
            continue
        for path in root.rglob("*"):
            if not path.is_file() or path.suffix.lower() not in TEXT_SUFFIXES:
                continue
            relative = path.relative_to(ROOT)
            if any(part.lower() in SKIP_PARTS for part in relative.parts):
                continue
            found.add(path)
    return sorted(found)


observed: dict[str, list[str]] = {}
for path in iter_files():
    relative = path.relative_to(ROOT).as_posix()
    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError as error:
        PROBLEMS.append(f"active toolchain surface is not UTF-8: {relative}: {error}")
        continue
    versions = sorted(set(VERSION.findall(text)))
    if versions:
        observed[relative] = versions
        for version in versions:
            if version != EXPECTED:
                PROBLEMS.append(f"stale active Rust toolchain {version}: {relative}")
    for match in FLOATING.finditer(text):
        PROBLEMS.append(f"floating Rust toolchain {match.group(1)}: {relative}")

required = {
    ".github/workflows/rust-service-gate.yml",
    ".github/workflows/p0-release-candidate-gate.yml",
    ".github/workflows/trnm-economy-ci.yml",
    "services/hepta-research-league/docker/rust-toolchain.manifest",
    "services/hepta-research-league/Dockerfile",
}
for relative in sorted(required):
    versions = observed.get(relative, [])
    if EXPECTED not in versions:
        PROBLEMS.append(f"required build lane does not bind Rust {EXPECTED}: {relative}")

result = {
    "schema": "cex.rust-toolchain-convergence-check.v1",
    "status": "failed" if PROBLEMS else "ok",
    "expected_rust_toolchain": EXPECTED,
    "active_surfaces_with_version": observed,
    "production_authorization": "not_granted",
    "problems": PROBLEMS,
}
print(json.dumps(result, indent=2, sort_keys=True))
sys.exit(1 if PROBLEMS else 0)
