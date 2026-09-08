#!/usr/bin/env python3
"""Reject mixed or obsolete Rust compiler identities in active release surfaces."""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXPECTED = "1.98.1"
OBSOLETE = ("1.98.0", "1.95.0")
ACTIVE_GLOBS = (
    ".github/workflows/*.yml",
    ".github/workflows/*.yaml",
    "services/*/Dockerfile",
    "services/*/Dockerfile.*",
    "services/*/docker/rust-toolchain.manifest",
    "docs/development-doc-authority-v1.json",
    "docs/release-evidence/p0-candidate-trigger.json",
    "docs/security/*.json",
)
FLOATING = re.compile(r"(?i)(?:toolchain|rust(?:c)?(?:[-_ ]version)?)[^\n]{0,40}(?:stable|latest)")
problems: list[str] = []
scanned: list[str] = []
expected_hits: list[str] = []

paths: set[Path] = set()
for pattern in ACTIVE_GLOBS:
    paths.update(path for path in ROOT.glob(pattern) if path.is_file())

for path in sorted(paths):
    relative = path.relative_to(ROOT).as_posix()
    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError as error:
        problems.append(f"active toolchain surface is not UTF-8: {relative}: {error}")
        continue
    scanned.append(relative)
    for version in OBSOLETE:
        if version in text:
            problems.append(f"obsolete active Rust identity {version}: {relative}")
    if FLOATING.search(text):
        problems.append(f"floating Rust identity in active surface: {relative}")
    if EXPECTED in text:
        expected_hits.append(relative)

required = {
    ".github/workflows/rust-service-gate.yml",
    ".github/workflows/p0-release-candidate-gate.yml",
    ".github/workflows/trnm-economy-ci.yml",
    "services/hepta-research-league/Dockerfile",
    "services/hepta-research-league/docker/rust-toolchain.manifest",
}
missing = sorted(path for path in required if path not in scanned)
if missing:
    problems.append("required active toolchain surfaces missing: " + ", ".join(missing))
for relative in sorted(required):
    path = ROOT / relative
    if path.is_file() and EXPECTED not in path.read_text(encoding="utf-8"):
        problems.append(f"required surface does not bind Rust {EXPECTED}: {relative}")

result = {
    "schema": "cex.rust-toolchain-convergence-check.v1",
    "status": "failed" if problems else "ok",
    "expected_rust_toolchain": EXPECTED,
    "active_surfaces_scanned": scanned,
    "expected_identity_surfaces": expected_hits,
    "obsolete_identities_forbidden": list(OBSOLETE),
    "floating_identities_forbidden": True,
    "production_authorization": "not_granted",
    "problems": problems,
}
print(json.dumps(result, indent=2, sort_keys=True))
raise SystemExit(1 if problems else 0)
