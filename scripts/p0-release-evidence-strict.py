#!/usr/bin/env python3
"""Strict wrapper around the v12 P0 evidence collector and manifest generator.

The canonical generator produces exactly thirteen ordered manifest evidence
records. This wrapper keeps repository-governance and exact-attempt hosted job
execution as payload-only attestations: it validates and indexes them without
adding a second pair of manifest records that would split the schema contract.

Hosted runs are selected exactly once. After the payload-only attestations are
written, the wrapper refreshes the payload index in place so a later workflow
run cannot be selected into the final context while the execution attestation
still describes an earlier set.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
LEGACY = ROOT / "scripts/p0-release-evidence.py"
VERIFY_EXECUTION = ROOT / "scripts/verify-hosted-run-execution.py"
PAYLOAD_ONLY_ATTESTATIONS = (
    "repository-governance.json",
    "hosted-run-execution.json",
)
CANONICAL_EVIDENCE_ORDER = (
    "hosted:p0-migration-gate",
    "hosted:rust-service-gate",
    "hosted:p0-gateway-exact-reserve-gate",
    "hosted:p0-execution-settlement-gate",
    "hosted:p0-provider-reconciliation-gate",
    "candidate-hygiene",
    "repository-integrity",
    "hepta-postgres-integration",
    "migration-and-lifecycle-matrix",
    "exact-ledger-soak",
    "backup-restore",
    "local-evidence-binding",
    "hosted-gate-execution",
)
FORBIDDEN_SPLIT_BRAIN_EVIDENCE = {
    "repository-governance",
    "hosted-run-execution",
}


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return "sha256:" + digest.hexdigest()


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def run_legacy(arguments: list[str]) -> None:
    subprocess.run([sys.executable, str(LEGACY), *arguments], cwd=ROOT, check=True)


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def read_json_object(path: Path, label: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"cannot read {label}: {error}") from error
    if not isinstance(value, dict):
        raise SystemExit(f"{label} must be a JSON object")
    return value


def require_nonempty_string(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value:
        raise SystemExit(f"{label} must be a non-empty string")
    return value


def require_positive_int(value: Any, label: str) -> int:
    if not isinstance(value, int) or isinstance(value, bool) or value < 1:
        raise SystemExit(f"{label} must be a positive integer")
    return value


def refresh_payload_index(
    *,
    context_path: Path,
    evidence_dir: Path,
) -> dict[str, Any]:
    """Re-index the existing single-pass evidence set without reselecting runs."""

    context = read_json_object(context_path, "strict release context")
    repository = require_nonempty_string(context.get("repository"), "context.repository")
    branch = require_nonempty_string(context.get("branch"), "context.branch")
    commit_sha = require_nonempty_string(context.get("commit_sha"), "context.commit_sha")
    tree_sha = require_nonempty_string(context.get("tree_sha"), "context.tree_sha")
    workflow_run_id = require_positive_int(
        context.get("workflow_run_id"), "context.workflow_run_id"
    )
    workflow_run_attempt = require_positive_int(
        context.get("workflow_run_attempt"), "context.workflow_run_attempt"
    )
    payload_name = require_nonempty_string(context.get("payload_name"), "context.payload_name")

    files: dict[str, str] = {}
    for path in sorted(evidence_dir.rglob("*")):
        if path.is_file() and path.name != "payload-index.json":
            files[path.relative_to(evidence_dir).as_posix()] = sha256_file(path)

    payload_index_path = evidence_dir / "payload-index.json"
    write_json(
        payload_index_path,
        {
            "schema": "cex.p0-release-evidence-payload.v1",
            "repository": repository,
            "branch": branch,
            "commit_sha": commit_sha,
            "tree_sha": tree_sha,
            "workflow_run_id": workflow_run_id,
            "workflow_run_attempt": workflow_run_attempt,
            "payload_name": payload_name,
            "generated_at": utc_now(),
            "files": files,
        },
    )
    files["payload-index.json"] = sha256_file(payload_index_path)
    context["files"] = files
    return context


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

    # Single pass: resolve and cross-bind one exact hosted-run set. Never call
    # the collector again after the execution attestation has been generated.
    run_legacy(forwarded)

    evidence_dir = args.evidence_dir.resolve()
    context_path = args.context.resolve()
    governance_path = evidence_dir / "repository-governance.json"
    if not governance_path.is_file():
        raise SystemExit("repository governance evidence is missing")
    governance = read_json_object(governance_path, "repository governance evidence")
    if governance.get("ok") is not True:
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

    # The selected hosted gate set is now frozen in context. Recompute only the
    # payload digests so governance and execution attestations are covered.
    context = refresh_payload_index(
        context_path=context_path,
        evidence_dir=evidence_dir,
    )
    files = context.get("files")
    if not isinstance(files, dict):
        raise SystemExit("strict release context lacks a files index")

    payload_only: dict[str, str] = {}
    for relative in PAYLOAD_ONLY_ATTESTATIONS:
        path = evidence_dir / relative
        if not path.is_file():
            raise SystemExit(f"payload-only attestation is missing: {relative}")
        expected = sha256_file(path)
        if files.get(relative) != expected:
            raise SystemExit(f"strict release context did not index {relative}")
        payload_only[relative] = expected
    context["payload_only_attestations"] = payload_only
    write_json(context_path, context)
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

    context = read_json_object(args.context, "strict release context")
    manifest_value = read_json_object(args.output, "generated candidate manifest")
    files = context.get("files")
    evidence = manifest_value.get("evidence")
    if not isinstance(files, dict) or not isinstance(evidence, list):
        raise SystemExit("strict manifest inputs lack files/evidence")

    names = [
        item.get("name")
        for item in evidence
        if isinstance(item, dict) and isinstance(item.get("name"), str)
    ]
    if names != list(CANONICAL_EVIDENCE_ORDER):
        raise SystemExit(
            "generated candidate manifest is not the canonical ordered thirteen-record contract"
        )
    if FORBIDDEN_SPLIT_BRAIN_EVIDENCE.intersection(names):
        raise SystemExit("payload-only attestations leaked into manifest evidence")

    payload_only = context.get("payload_only_attestations")
    if not isinstance(payload_only, dict) or set(payload_only) != set(
        PAYLOAD_ONLY_ATTESTATIONS
    ):
        raise SystemExit("strict release context lacks the payload-only attestation set")
    for relative in PAYLOAD_ONLY_ATTESTATIONS:
        digest = payload_only.get(relative)
        if not isinstance(digest, str) or files.get(relative) != digest:
            raise SystemExit(
                f"strict release context lacks the exact payload-only digest for {relative}"
            )

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
