#!/usr/bin/env python3
"""Strict wrapper around the v12 P0 evidence collector and manifest generator.

The legacy generator remains the implementation source for SBOM, provenance and
hosted-run selection.  This wrapper adds the final fail-closed controls that
must not be optional: exact-attempt job execution verification, exact-tree
binding of governance evidence, and first-class manifest entries for both.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
LEGACY = ROOT / "scripts/p0-release-evidence.py"
VERIFY_EXECUTION = ROOT / "scripts/verify-hosted-run-execution.py"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return "sha256:" + digest.hexdigest()


def run_legacy(arguments: list[str]) -> None:
    subprocess.run([sys.executable, str(LEGACY), *arguments], cwd=ROOT, check=True)


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def collect(args: argparse.Namespace) -> int:
    forwarded = [
        "collect",
        "--repo-root", str(args.repo_root),
        "--evidence-dir", str(args.evidence_dir),
        "--context", str(args.context),
        "--repository", args.repository,
        "--branch", args.branch,
        "--sha", args.sha,
        "--tree", args.tree,
        "--run-id", str(args.run_id),
        "--run-attempt", str(args.run_attempt),
        "--server-url", args.server_url,
    ]

    # First pass resolves the exact successful hosted runs into a context file.
    run_legacy(forwarded)

    evidence_dir = args.evidence_dir.resolve()
    context_path = args.context.resolve()
    governance_path = evidence_dir / "repository-governance.json"
    if not governance_path.is_file():
        raise SystemExit("repository governance evidence is missing")
    governance = json.loads(governance_path.read_text(encoding="utf-8"))
    if not isinstance(governance, dict) or governance.get("ok") is not True:
        raise SystemExit("repository governance evidence is not successful")
    if governance.get("repository") != args.repository:
        raise SystemExit("repository governance evidence is bound to another repository")
    if governance.get("commit_sha") != args.sha:
        raise SystemExit("repository governance evidence is bound to another commit")
    existing_tree = governance.get("tree_sha")
    if existing_tree not in {None, args.tree}:
        raise SystemExit("repository governance evidence is bound to another tree")
    governance["tree_sha"] = args.tree
    write_json(governance_path, governance)

    execution_path = evidence_dir / "hosted-run-execution.json"
    subprocess.run(
        [
            sys.executable,
            str(VERIFY_EXECUTION),
            "--context", str(context_path),
            "--output", str(execution_path),
        ],
        cwd=ROOT,
        check=True,
        env=os.environ.copy(),
    )

    # Re-run collection so the payload index and release context cover the two
    # newly verified evidence files.  Hosted-run selection is deterministic for
    # the exact branch/SHA and prefers a completed success.
    run_legacy(forwarded)
    context = json.loads(context_path.read_text(encoding="utf-8"))
    files = context.get("files") if isinstance(context, dict) else None
    if not isinstance(files, dict):
        raise SystemExit("strict release context lacks a files index")
    for relative in ("repository-governance.json", "hosted-run-execution.json"):
        path = evidence_dir / relative
        expected = sha256_file(path)
        if files.get(relative) != expected:
            raise SystemExit(f"strict release context did not index {relative}")
    print(context_path)
    return 0


def manifest(args: argparse.Namespace) -> int:
    forwarded = [
        "manifest",
        "--context", str(args.context),
        "--output", str(args.output),
        "--release-id", args.release_id,
        "--payload-name", args.payload_name,
        "--payload-digest", args.payload_digest,
    ]
    run_legacy(forwarded)

    context = json.loads(args.context.read_text(encoding="utf-8"))
    manifest_value = json.loads(args.output.read_text(encoding="utf-8"))
    if not isinstance(context, dict) or not isinstance(manifest_value, dict):
        raise SystemExit("strict manifest inputs must be JSON objects")
    files = context.get("files")
    evidence = manifest_value.get("evidence")
    if not isinstance(files, dict) or not isinstance(evidence, list):
        raise SystemExit("strict manifest inputs lack files/evidence")

    existing_names = {
        item.get("name")
        for item in evidence
        if isinstance(item, dict) and isinstance(item.get("name"), str)
    }
    for name, relative in (
        ("repository-governance", "repository-governance.json"),
        ("hosted-run-execution", "hosted-run-execution.json"),
    ):
        if name in existing_names:
            raise SystemExit(f"strict evidence entry already exists: {name}")
        digest = files.get(relative)
        if not isinstance(digest, str):
            raise SystemExit(f"strict release context lacks digest for {relative}")
        evidence.append(
            {
                "name": name,
                "status": "pass",
                "uri": f"artifact://{args.payload_name}/{relative}",
                "sha256": digest,
                "waiver": None,
            }
        )
    write_json(args.output, manifest_value)
    print(args.output)
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    collect_parser = subparsers.add_parser("collect")
    collect_parser.add_argument("--repo-root", type=Path, default=ROOT)
    collect_parser.add_argument("--evidence-dir", required=True, type=Path)
    collect_parser.add_argument("--context", required=True, type=Path)
    collect_parser.add_argument("--repository", required=True)
    collect_parser.add_argument("--branch", required=True)
    collect_parser.add_argument("--sha", required=True)
    collect_parser.add_argument("--tree", required=True)
    collect_parser.add_argument("--run-id", required=True, type=int)
    collect_parser.add_argument("--run-attempt", required=True, type=int)
    collect_parser.add_argument("--server-url", required=True)
    collect_parser.set_defaults(function=collect)

    manifest_parser = subparsers.add_parser("manifest")
    manifest_parser.add_argument("--context", required=True, type=Path)
    manifest_parser.add_argument("--output", required=True, type=Path)
    manifest_parser.add_argument("--release-id", required=True)
    manifest_parser.add_argument("--payload-name", required=True)
    manifest_parser.add_argument("--payload-digest", required=True)
    manifest_parser.set_defaults(function=manifest)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    return int(args.function(args))


if __name__ == "__main__":
    raise SystemExit(main())
