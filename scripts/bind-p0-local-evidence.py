#!/usr/bin/env python3
"""Bind every local P0 evidence record to one exact commit/tree and content digest."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

GIT_SHA_RE = re.compile(r"^[0-9a-f]{40}$")
LOCAL_EVIDENCE = {
    "candidate-hygiene": "candidate-hygiene.json",
    "repository-integrity": "repository-integrity.json",
    "hepta-postgres-integration": "hepta-postgres-integration.json",
    "migration-and-lifecycle-matrix": "database-lifecycle.json",
    "exact-ledger-soak": "exact-ledger-soak.json",
    "backup-restore": "backup-restore.json",
}


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return "sha256:" + digest.hexdigest()


def successful(payload: Any) -> bool:
    if not isinstance(payload, dict):
        return False
    status = payload.get("status")
    if status is not None and status not in {"ok", "passed", "success"}:
        return False
    return payload.get("ok") is True or status in {"ok", "passed", "success"}


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence-dir", required=True, type=Path)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--branch", required=True)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--tree", required=True)
    parser.add_argument("--run-id", required=True, type=int)
    parser.add_argument("--run-attempt", required=True, type=int)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()

    if not GIT_SHA_RE.fullmatch(args.sha) or not GIT_SHA_RE.fullmatch(args.tree):
        raise SystemExit("sha/tree must be 40-character lowercase Git object ids")
    if type(args.run_id) is not int or args.run_id < 1:
        raise SystemExit("run-id must be a strict positive integer")
    if type(args.run_attempt) is not int or args.run_attempt < 1:
        raise SystemExit("run-attempt must be a strict positive integer")

    evidence_dir = args.evidence_dir.resolve()
    records: dict[str, Any] = {}
    for name, relative in LOCAL_EVIDENCE.items():
        path = evidence_dir / relative
        if not path.is_file():
            raise SystemExit(f"missing local evidence {name}: {path}")
        try:
            payload = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            raise SystemExit(f"invalid local evidence {name}: {error}") from error
        if not successful(payload):
            raise SystemExit(f"local evidence is not successful: {name}")
        if payload.get("commit_sha") != args.sha:
            raise SystemExit(f"local evidence is not bound to the exact commit: {name}")
        producer_tree = payload.get("tree_sha")
        if not isinstance(producer_tree, str) or not GIT_SHA_RE.fullmatch(producer_tree):
            raise SystemExit(
                f"local evidence {name} is missing a valid exact tree_sha"
            )
        if producer_tree != args.tree:
            raise SystemExit(f"local evidence is bound to a different tree: {name}")
        records[name] = {
            "path": relative,
            "sha256": sha256_file(path),
            "producer_schema": payload.get("schema"),
            "producer_commit_sha": payload.get("commit_sha"),
            "producer_tree_sha": producer_tree,
            "status": payload.get("status"),
            "ok": payload.get("ok"),
        }

    root = Path(__file__).resolve().parents[1]
    try:
        actual_sha = subprocess.run(
            ["git", "-C", str(root), "rev-parse", "HEAD"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        actual_tree = subprocess.run(
            ["git", "-C", str(root), "rev-parse", "HEAD^{tree}"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"cannot resolve checked-out git identity: {error}") from error
    if actual_sha != args.sha or actual_tree != args.tree:
        raise SystemExit("local evidence binder arguments do not match checked-out commit/tree")

    result = {
        "schema": "cex.p0-local-evidence-binding.v1",
        "status": "ok",
        "ok": True,
        "repository": args.repository,
        "branch": args.branch,
        "commit_sha": args.sha,
        "tree_sha": args.tree,
        "workflow_run_id": args.run_id,
        "workflow_run_attempt": args.run_attempt,
        "generated_at": utc_now(),
        "records": records,
    }
    output = args.output or (evidence_dir / "local-evidence-binding.json")
    if not output.is_absolute():
        output = root / output
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
