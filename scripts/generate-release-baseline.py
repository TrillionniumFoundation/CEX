#!/usr/bin/env python3
"""Generate a draft CEX release-baseline manifest from a clean Git checkout."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

MIGRATION_RE = re.compile(r"^(\d{4})_[a-z0-9][a-z0-9._-]*\.sql$")


def run_git(root: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(root), *args],
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return f"sha256:{digest.hexdigest()}"


def migration_head(root: Path) -> Path:
    candidates: list[tuple[int, str, Path]] = []
    for path in (root / "migrations").glob("*.sql"):
        match = MIGRATION_RE.fullmatch(path.name)
        if match:
            candidates.append((int(match.group(1)), path.name, path))
    if not candidates:
        raise SystemExit("no numbered SQL migrations found")
    candidates.sort()
    return candidates[-1][2]


def clean_worktree(root: Path) -> None:
    dirty = run_git(root, "status", "--porcelain")
    if dirty:
        raise SystemExit("release manifest generation requires a clean worktree")


def build_manifest(root: Path, release_id: str) -> dict[str, Any]:
    clean_worktree(root)
    head = run_git(root, "rev-parse", "HEAD")
    tree = run_git(root, "rev-parse", "HEAD^{tree}")
    branch = run_git(root, "branch", "--show-current") or "detached-head"
    migration = migration_head(root)

    return {
        "schema": "cex.release-baseline-manifest.v1",
        "status": "draft",
        "project_id": "hepta-control-plane",
        "release_id": release_id,
        "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "source": {
            "repository": "TrillionniumFoundation/CEX",
            "branch": branch,
            "commit_sha": head,
            "tree_sha": tree,
        },
        "dependencies": {
            "cargo_lock_sha256": sha256_file(root / "Cargo.lock"),
        },
        "database": {
            "migration_head": migration.name,
            "migration_sha256": sha256_file(migration),
        },
        "build": {
            "workflow_run_id": None,
            "artifacts": [],
            "images": [],
            "sbom": None,
            "provenance": None,
        },
        "evidence": [
            {"name": name, "status": "pending", "uri": None, "sha256": None, "waiver": None}
            for name in (
                "hosted-ci",
                "migration-upgrade",
                "backup-restore",
                "soak",
            )
        ],
        "approvals": [],
        "revocation": None,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--release-id", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()

    release_id = args.release_id.strip().lower()
    if not re.fullmatch(r"[a-z0-9][a-z0-9._-]{0,127}", release_id):
        raise SystemExit("release id must match ^[a-z0-9][a-z0-9._-]{0,127}$")

    manifest = build_manifest(args.repo_root.resolve(), release_id)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
