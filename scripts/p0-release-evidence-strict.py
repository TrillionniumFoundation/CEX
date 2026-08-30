#!/usr/bin/env python3
"""Strict wrapper around the v12 P0 evidence collector and manifest generator.

The evidence core remains the implementation source for SBOM, provenance and
latest hosted-run selection.  This wrapper adds the final fail-closed controls
that must not be optional: exact-attempt job execution verification, exact-tree
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
CORE = ROOT / "scripts/p0-release-evidence-core.py"
VERIFY_EXECUTION = ROOT / "scripts/verify-hosted-run-execution.py"
BASELINE_VALIDATOR = ROOT / "scripts/check-release-baseline-manifest.py"
STRICT_CONTRACT = ROOT / "scripts/check-release-evidence-contract.py"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return "sha256:" + digest.hexdigest()


def rooted(path: Path) -> Path:
    return path if path.is_absolute() else ROOT / path


def require_clean_checkout(context: dict[str, Any]) -> None:
    """Re-check the exact checkout immediately before final qualification."""

    try:
        commit = subprocess.run(
            ["git", "-C", str(ROOT), "rev-parse", "HEAD"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        tree = subprocess.run(
            ["git", "-C", str(ROOT), "rev-parse", "HEAD^{tree}"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        tracked_dirty = subprocess.run(
            ["git", "-C", str(ROOT), "diff", "--quiet"],
            check=False,
        ).returncode
        staged_dirty = subprocess.run(
            ["git", "-C", str(ROOT), "diff", "--cached", "--quiet"],
            check=False,
        ).returncode
    except (OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"cannot verify final checkout identity: {error}") from error
    if commit != context.get("commit_sha") or tree != context.get("tree_sha"):
        raise SystemExit("final checkout commit/tree differs from release context")
    if tracked_dirty != 0 or staged_dirty != 0:
        raise SystemExit("tracked worktree changes exist during strict manifest qualification")


def run_core(arguments: list[str]) -> None:
    subprocess.run([sys.executable, str(CORE), *arguments], cwd=ROOT, check=True)


def validate_manifest(path: Path, context_path: Path, evidence_dir: Path) -> None:
    subprocess.run(
        [sys.executable, str(BASELINE_VALIDATOR), str(path)],
        cwd=ROOT,
        check=True,
    )
    # The baseline checker enforces the public shape.  The strict checker also
    # binds every URI/digest to the final collector context and re-hashes the
    # uploaded payload files, so a swapped run or post-upload mutation cannot
    # qualify merely because it still has a valid-looking URI.
    subprocess.run(
        [
            sys.executable,
            str(STRICT_CONTRACT),
            str(path),
            "--context",
            str(context_path),
            "--evidence-dir",
            str(evidence_dir),
        ],
        cwd=ROOT,
        check=True,
    )


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def refresh_payload_index(evidence_dir: Path, context_path: Path) -> None:
    """Re-index files added after the core collector wrote its context.

    Governance and exact-job attestations are deliberately collected after the
    core payload.  Re-running the collector would perform another mutable API
    selection and could mix two different run snapshots.  Instead, update the
    local payload index and context atomically from the already verified files.
    """

    context = json.loads(context_path.read_text(encoding="utf-8"))
    if not isinstance(context, dict):
        raise SystemExit("strict release context must be an object")
    files: dict[str, str] = {}
    for path in sorted(evidence_dir.rglob("*")):
        if path.is_file() and path.name != "payload-index.json":
            files[path.relative_to(evidence_dir).as_posix()] = sha256_file(path)

    payload_index = {
        "schema": "cex.p0-release-evidence-payload.v1",
        "repository": context.get("repository"),
        "branch": context.get("branch"),
        "commit_sha": context.get("commit_sha"),
        "tree_sha": context.get("tree_sha"),
        "workflow_run_id": context.get("workflow_run_id"),
        "workflow_run_attempt": context.get("workflow_run_attempt"),
        "payload_name": context.get("payload_name"),
        "generated_at": context.get("generated_at"),
        "files": files,
    }
    payload_index_path = evidence_dir / "payload-index.json"
    write_json(payload_index_path, payload_index)
    files["payload-index.json"] = sha256_file(payload_index_path)
    context["files"] = files
    write_json(context_path, context)


def collect(args: argparse.Namespace) -> int:
    repo_root = rooted(args.repo_root)
    evidence_dir = rooted(args.evidence_dir)
    context_path = rooted(args.context)
    forwarded = [
        "collect",
        "--repo-root", str(repo_root),
        "--evidence-dir", str(evidence_dir),
        "--context", str(context_path),
        "--repository", args.repository,
        "--branch", args.branch,
        "--sha", args.sha,
        "--tree", args.tree,
        "--run-id", str(args.run_id),
        "--run-attempt", str(args.run_attempt),
        "--server-url", args.server_url,
    ]

    # The core collector performs the authoritative latest-run selection once.
    # The strict verifier below attests the exact jobs for that same snapshot.
    run_core(forwarded)

    governance_path = evidence_dir / "repository-governance.json"
    if not governance_path.is_file():
        raise SystemExit("repository governance evidence is missing")
    governance = json.loads(governance_path.read_text(encoding="utf-8"))
    if not isinstance(governance, dict) or governance.get("ok") is not True:
        raise SystemExit("repository governance evidence is not successful")
    if governance.get("repository") != args.repository:
        raise SystemExit("repository governance evidence is bound to another repository")
    if governance.get("candidate_branch") != args.branch:
        raise SystemExit("repository governance evidence is bound to another branch")
    if governance.get("commit_sha") != args.sha:
        raise SystemExit("repository governance evidence is bound to another commit")
    if governance.get("candidate_commit_matches_branch") is not True:
        raise SystemExit("repository governance did not confirm the candidate branch commit")
    existing_tree = governance.get("tree_sha")
    if not isinstance(existing_tree, str) or not existing_tree:
        raise SystemExit("repository governance evidence lacks an exact tree")
    if existing_tree != args.tree:
        raise SystemExit("repository governance evidence is bound to another tree")
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

    refresh_payload_index(evidence_dir, context_path)
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
    context_path = rooted(args.context)
    output_path = rooted(args.output)
    evidence_dir = rooted(args.evidence_dir) if args.evidence_dir is not None else context_path.parent / "p0-release-evidence"
    if not context_path.is_file():
        raise SystemExit(f"strict release context is missing: {context_path}")
    if not evidence_dir.is_dir():
        raise SystemExit(f"strict release evidence directory is missing: {evidence_dir}")
    payload_digest = args.payload_digest.strip().lower()
    if not payload_digest.startswith("sha256:"):
        payload_digest = "sha256:" + payload_digest
    if len(payload_digest) != len("sha256:") + 64 or any(
        character not in "0123456789abcdef" for character in payload_digest[7:]
    ) or payload_digest == "sha256:" + "0" * 64:
        raise SystemExit("payload digest must be a non-placeholder SHA-256")
    context = json.loads(context_path.read_text(encoding="utf-8"))
    if not isinstance(context, dict):
        raise SystemExit("strict release context must be an object")
    # The artifact digest is only known after upload-artifact returns.  Bind it
    # into the collector context before any strict validation so a caller
    # cannot replace the payload with another artifact while keeping the same
    # release/run identity.
    context["payload_digest"] = payload_digest
    write_json(context_path, context)
    require_clean_checkout(context)
    # Re-run the exact job/runner/step verifier immediately before generating
    # the manifest.  The collect phase's result is persisted, but a later
    # rerun or cancellation must not be hidden by that earlier snapshot.
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
    refresh_payload_index(evidence_dir, context_path)
    forwarded = [
        "manifest",
        "--context", str(context_path),
        "--output", str(output_path),
        "--release-id", args.release_id,
        "--payload-name", args.payload_name,
        "--payload-digest", args.payload_digest,
    ]
    # Use the core manifest builder directly.  The older wrapper appends the
    # superseded local-evidence-binding/hosted-gate-execution pair; strict v12
    # has one canonical thirteen-entry vocabulary instead.
    run_core(forwarded)

    context = json.loads(context_path.read_text(encoding="utf-8"))
    manifest_value = json.loads(output_path.read_text(encoding="utf-8"))
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
    write_json(output_path, manifest_value)
    validate_manifest(output_path, context_path, evidence_dir)
    print(output_path)
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
    manifest_parser.add_argument(
        "--evidence-dir",
        type=Path,
        required=True,
        help="payload directory to re-hash while binding the manifest",
    )
    manifest_parser.set_defaults(function=manifest)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    return int(args.function(args))


if __name__ == "__main__":
    raise SystemExit(main())
