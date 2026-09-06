#!/usr/bin/env python3
"""Validate the CEX/World ownership boundary and frozen compatibility inventory."""

from __future__ import annotations

import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
MANIFEST_PATH = ROOT / "docs/compatibility/world-surface-freeze-v1.json"
BOUNDARY_PATH = ROOT / "PROJECT_BOUNDARY.json"
PROBLEMS: list[str] = []


def read(relative: str) -> str:
    path = ROOT / relative
    if not path.is_file():
        PROBLEMS.append(f"missing required file: {relative}")
        return ""
    return path.read_text(encoding="utf-8")


def git_blob(relative: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(ROOT), "hash-object", relative],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        PROBLEMS.append(f"cannot hash frozen file {relative}: {result.stderr.strip()}")
        return ""
    return result.stdout.strip()


def walk_paths(value: Any, manifest_dir: Path) -> None:
    if isinstance(value, dict):
        path_value = value.get("path")
        if isinstance(path_value, str):
            resolved = (manifest_dir / path_value).resolve()
            try:
                resolved.relative_to(ROOT.resolve())
            except ValueError:
                PROBLEMS.append(
                    f"Cargo path dependency escapes CEX repository: "
                    f"{manifest_dir.relative_to(ROOT)}/{path_value}"
                )
        for nested in value.values():
            walk_paths(nested, manifest_dir)
    elif isinstance(value, list):
        for nested in value:
            walk_paths(nested, manifest_dir)


try:
    manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
except (FileNotFoundError, UnicodeDecodeError, json.JSONDecodeError) as error:
    print(json.dumps({"status": "failed", "problems": [str(error)]}, indent=2))
    raise SystemExit(1)

try:
    boundary = json.loads(BOUNDARY_PATH.read_text(encoding="utf-8"))
except (FileNotFoundError, UnicodeDecodeError, json.JSONDecodeError) as error:
    PROBLEMS.append(f"invalid PROJECT_BOUNDARY.json: {error}")
    boundary = {}

if manifest.get("schema") != "cex.world-surface-freeze.v1":
    PROBLEMS.append("World surface manifest schema is invalid")
if manifest.get("status") != "active":
    PROBLEMS.append("World surface manifest must be active")
if manifest.get("production_authorization") != "not_granted":
    PROBLEMS.append("World surface manifest must deny production authorization")
if manifest.get("owner_repository") != "TrillionniumFoundation/Trillionnium-World":
    PROBLEMS.append("World authority owner repository is invalid")
if manifest.get("cex_role") != "compatibility_edge_only":
    PROBLEMS.append("CEX World role must remain compatibility_edge_only")

entries = manifest.get("frozen_files")
if not isinstance(entries, list) or not entries:
    PROBLEMS.append("World frozen_files must be a nonempty list")
    entries = []

paths: set[str] = set()
world_paths: set[str] = set()
for index, entry in enumerate(entries):
    if not isinstance(entry, dict):
        PROBLEMS.append(f"frozen_files[{index}] must be an object")
        continue
    relative = entry.get("path")
    expected_blob = entry.get("git_blob")
    kind = entry.get("kind")
    if not isinstance(relative, str) or not relative:
        PROBLEMS.append(f"frozen_files[{index}] lacks path")
        continue
    if relative in paths:
        PROBLEMS.append(f"duplicate frozen World path: {relative}")
    paths.add(relative)
    if kind == "world_source":
        world_paths.add(relative)
    if not isinstance(expected_blob, str) or not re.fullmatch(r"[0-9a-f]{40}", expected_blob):
        PROBLEMS.append(f"invalid frozen Git blob for {relative}")
        continue
    if not (ROOT / relative).is_file():
        PROBLEMS.append(f"frozen World file is missing: {relative}")
        continue
    actual_blob = git_blob(relative)
    if actual_blob and actual_blob != expected_blob:
        PROBLEMS.append(
            f"frozen World file changed without inventory review: "
            f"{relative} expected={expected_blob} actual={actual_blob}"
        )

source_root = ROOT / "services/consumer-entry-api/src"
discovered = {
    path.relative_to(ROOT).as_posix()
    for path in source_root.glob("*.rs")
    if "world" in path.stem or "openstreetmap" in path.stem
}
if discovered != world_paths:
    PROBLEMS.append(
        "World compatibility inventory mismatch: missing="
        + ",".join(sorted(discovered - world_paths))
        + " extra="
        + ",".join(sorted(world_paths - discovered))
    )

if "services/matrix-entry-adapter/src/lib.rs" not in paths:
    PROBLEMS.append("Matrix World command carrier is not frozen")

for marker in (
    "hepta-control-plane",
    "TrillionniumFoundation/CEX",
    "external_path_dependencies",
    "compatibility_inventory",
):
    if marker not in json.dumps(boundary, sort_keys=True):
        PROBLEMS.append(f"PROJECT_BOUNDARY.json lacks marker: {marker}")

deny_regex = boundary.get("deny_changed_paths_regex")
for marker in ("world_", "real_world_", "openstreetmap_", "trillionnium_world_"):
    if not isinstance(deny_regex, str) or marker not in deny_regex:
        PROBLEMS.append(f"boundary deny regex does not cover {marker}")

workspace = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
members = workspace.get("workspace", {}).get("members", [])
for member in members:
    cargo = ROOT / member / "Cargo.toml"
    if not cargo.is_file():
        PROBLEMS.append(f"workspace member lacks Cargo.toml: {member}")
        continue
    walk_paths(tomllib.loads(cargo.read_text(encoding="utf-8")), cargo.parent)

for relative in (
    "services/consumer-entry-api/MODULE.md",
    "services/matrix-entry-adapter/MODULE.md",
    "docs/compatibility/world-surface-freeze-v1.md",
):
    text = read(relative)
    for marker in ("World", "compatibility", "not_granted"):
        if marker not in text:
            PROBLEMS.append(f"{relative} lacks boundary marker: {marker}")

result = {
    "schema": "cex.project-boundary-check.v1",
    "status": "failed" if PROBLEMS else "ok",
    "world_source_files": len(world_paths),
    "frozen_files": len(paths),
    "owner_repository": manifest.get("owner_repository"),
    "transfer_status": manifest.get("transfer_status"),
    "production_authorization": "not_granted",
    "problems": PROBLEMS,
}
print(json.dumps(result, indent=2, sort_keys=True))
sys.exit(1 if PROBLEMS else 0)
