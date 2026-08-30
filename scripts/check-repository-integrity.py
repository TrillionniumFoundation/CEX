#!/usr/bin/env python3
"""Emit an exact-tree integrity record for the active CEX v12 authority set."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Iterable

ROOT = Path(__file__).resolve().parents[1]
AUTHORITY_PATH = ROOT / "docs/development-doc-authority-v1.json"


def run_git(*arguments: str) -> str:
    return subprocess.run(
        ["git", "-C", str(ROOT), *arguments],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return "sha256:" + digest.hexdigest()


def digest_set(paths: Iterable[Path]) -> str:
    digest = hashlib.sha256()
    for path in sorted(paths, key=lambda item: item.relative_to(ROOT).as_posix()):
        relative = path.relative_to(ROOT).as_posix()
        digest.update(relative.encode("utf-8"))
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return "sha256:" + digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path)
    parser.add_argument("--expected-sha")
    parser.add_argument("--expected-tree")
    args = parser.parse_args()

    docs_check = subprocess.run(
        [sys.executable, str(ROOT / "scripts/check-development-docs.py")],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if docs_check.returncode != 0:
        print(docs_check.stdout, end="")
        raise SystemExit("development document contract failed")

    if subprocess.run(["git", "-C", str(ROOT), "diff", "--quiet"]).returncode != 0:
        raise SystemExit("tracked worktree changes exist during integrity attestation")
    if subprocess.run(["git", "-C", str(ROOT), "diff", "--cached", "--quiet"]).returncode != 0:
        raise SystemExit("staged changes exist during integrity attestation")

    commit_sha = run_git("rev-parse", "HEAD")
    tree_sha = run_git("rev-parse", "HEAD^{tree}")
    if args.expected_sha and args.expected_sha != commit_sha:
        raise SystemExit("checked-out commit does not match expected SHA")
    if args.expected_tree and args.expected_tree != tree_sha:
        raise SystemExit("checked-out tree does not match expected tree")

    authority = json.loads(AUTHORITY_PATH.read_text(encoding="utf-8"))
    canonical = [ROOT / value for value in authority["canonical_documents"].values()]
    canonical.extend(
        [
            ROOT / authority["active_plan"],
            ROOT / authority["active_addendum"],
            AUTHORITY_PATH,
        ]
    )
    migration_paths = sorted(
        (ROOT / "migrations").glob("[0-9][0-9][0-9][0-9]_*.sql"),
        key=lambda path: path.name,
    )
    if not migration_paths:
        raise SystemExit("no numbered migrations found for integrity attestation")
    workflow_paths = [ROOT / path for path in authority["authoritative_workflows"]]
    workflow_paths.append(ROOT / authority["aggregate_release_workflow"])
    freeze_path = authority.get("qualification_freeze")
    if not isinstance(freeze_path, str) or not (ROOT / freeze_path).is_file():
        raise SystemExit("development authority qualification freeze is missing")

    record = {
        "schema": "cex.repository-integrity.v1",
        "status": "ok",
        "ok": True,
        # Keep the generic evidence identity names alongside the historical
        # repository_* fields.  The release collector consumes every local
        # attestation through one exact commit/tree contract; without these
        # aliases this otherwise valid integrity record could not be admitted
        # as a first-class manifest evidence item.
        "commit_sha": commit_sha,
        "tree_sha": tree_sha,
        "repository_commit_sha": commit_sha,
        "repository_tree_sha": tree_sha,
        "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "active_plan": authority["active_plan"],
        "active_addendum": authority["active_addendum"],
        "migration_head": migration_paths[-1].name,
        "production_authorization": "not_granted",
        "digests": {
            "cargo_lock": sha256_file(ROOT / "Cargo.lock"),
            "migration_chain": digest_set(migration_paths),
            "authoritative_workflows": digest_set(workflow_paths),
            "canonical_documents": digest_set(canonical),
            "candidate_trigger": sha256_file(ROOT / authority["shared_trigger"]),
            "qualification_freeze": sha256_file(ROOT / freeze_path),
            "root_readme_observed_only": (
                sha256_file(ROOT / "readme.md")
                if (ROOT / "readme.md").is_file()
                else None
            ),
        },
        "documentation_check": json.loads(docs_check.stdout),
    }
    encoded = json.dumps(record, indent=2, sort_keys=True) + "\n"
    if args.output:
        output = args.output if args.output.is_absolute() else ROOT / args.output
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(encoded, encoding="utf-8")
    print(encoded, end="")
    return 0


if __name__ == "__main__":
    sys.exit(main())
