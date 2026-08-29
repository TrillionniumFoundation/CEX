#!/usr/bin/env python3
"""Collect exact-tree P0 evidence and emit a repository-qualified candidate manifest."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import time
import tomllib
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

REQUIRED_GATES = {
    "p0-migration-gate": ".github/workflows/p0-migration-gate.yml",
    "rust-service-gate": ".github/workflows/rust-service-gate.yml",
    "p0-gateway-exact-reserve-gate": ".github/workflows/p0-gateway-exact-reserve-gate.yml",
    "p0-execution-settlement-gate": ".github/workflows/p0-execution-settlement-gate.yml",
    "p0-provider-reconciliation-gate": ".github/workflows/p0-provider-reconciliation-gate.yml",
}
LOCAL_EVIDENCE = {
    "candidate-hygiene": "candidate-hygiene.json",
    "migration-and-lifecycle-matrix": "database-lifecycle.json",
    "exact-ledger-soak": "exact-ledger-soak.json",
    "backup-restore": "backup-restore.json",
}
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
GIT_SHA_RE = re.compile(r"^[0-9a-f]{40}$")
MIGRATION_RE = re.compile(r"^(\d{4})_[a-z0-9][a-z0-9._-]*\.sql$")


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def sha256_bytes(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return "sha256:" + digest.hexdigest()


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def run_git(root: Path, *args: str) -> str:
    return subprocess.run(
        ["git", "-C", str(root), *args],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


def migration_state(root: Path) -> tuple[Path, str]:
    numbered: list[tuple[int, str, Path]] = []
    for path in (root / "migrations").glob("*.sql"):
        match = MIGRATION_RE.fullmatch(path.name)
        if match:
            numbered.append((int(match.group(1)), path.name, path))
    if not numbered:
        raise SystemExit("no numbered migrations found")
    numbered.sort()
    digest = hashlib.sha256()
    for _, name, path in numbered:
        digest.update(name.encode("utf-8"))
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return numbered[-1][2], "sha256:" + digest.hexdigest()


def api_json(url: str, token: str) -> dict[str, Any]:
    request = urllib.request.Request(
        url,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "User-Agent": "cex-p0-release-evidence",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def _run_sort_key(run: dict[str, Any]) -> tuple[str, int, int]:
    """Return a stable newest-first sort key for a workflow run.

    GitHub normally returns integer ``id``/``run_attempt`` values, but keeping
    this conversion defensive means a malformed (or test-double) response
    cannot make evidence collection crash while trying to report the run.
    """

    def as_int(value: Any) -> int:
        try:
            return int(value or 0)
        except (TypeError, ValueError):
            return 0

    return (
        str(run.get("created_at") or ""),
        as_int(run.get("run_attempt")),
        as_int(run.get("id")),
    )


def _workflow_runs(
    repository: str,
    sha: str,
    branch: str,
    token: str,
) -> list[dict[str, Any]]:
    """Read all workflow-run pages for one exact SHA and branch.

    ``head_sha`` alone is not sufficient here: the repository can have many
    branch refs pointing at the same commit, and each ref may have a separate
    run.  The API's ``branch`` filter narrows the response, while the caller
    still performs an exact ``head_branch`` check before selecting evidence.
    Pagination is intentionally handled here rather than relying on the
    default first page (which can silently omit an older successful run).
    """

    per_page = 100
    page = 1
    runs_by_id: dict[int, dict[str, Any]] = {}
    runs_without_id: list[dict[str, Any]] = []
    endpoint = f"https://api.github.com/repos/{repository}/actions/runs"

    while True:
        url = endpoint + "?" + urllib.parse.urlencode(
            {
                "head_sha": sha,
                "branch": branch,
                "per_page": per_page,
                "page": page,
            }
        )
        payload = api_json(url, token)
        page_runs = payload.get("workflow_runs")
        if not isinstance(page_runs, list):
            raise SystemExit("GitHub Actions response lacks workflow_runs")

        for run in page_runs:
            if not isinstance(run, dict):
                continue
            try:
                run_id = int(run.get("id"))
            except (TypeError, ValueError):
                runs_without_id.append(run)
            else:
                # A run should occur on exactly one page.  Deduplicating by
                # id makes collection deterministic if a run is moved between
                # pages while the API is being polled.
                runs_by_id[run_id] = run

        if not page_runs:
            break

        total_count: int | None = None
        try:
            if payload.get("total_count") is not None:
                total_count = int(payload["total_count"])
        except (TypeError, ValueError):
            total_count = None

        # The normal GitHub response gives either a short final page or a
        # total_count.  If neither is useful, fetch one empty page and stop;
        # this is conservative and also works with lightweight API doubles.
        if (total_count is not None and page * per_page >= total_count) or (
            total_count is None and len(page_runs) < per_page
        ):
            break
        page += 1
        # A broken API response must not make a release job loop forever.
        if page > 1000:
            raise SystemExit("GitHub Actions pagination exceeded 1000 pages")

    return list(runs_by_id.values()) + runs_without_id


def collect_gate_runs(
    repository: str,
    sha: str,
    token: str,
    attempts: int,
    interval_seconds: int,
    *,
    branch: str,
) -> dict[str, dict[str, Any]]:
    if not branch:
        raise SystemExit("branch is required to bind hosted gate evidence")

    for attempt in range(1, attempts + 1):
        all_runs = _workflow_runs(repository, sha, branch, token)

        selected: dict[str, dict[str, Any]] = {}
        pending: list[str] = []
        failures: list[str] = []
        for name, path in REQUIRED_GATES.items():
            candidates = [
                run
                for run in all_runs
                if run.get("head_sha") == sha and run.get("path") == path
                and run.get("head_branch") == branch
            ]
            if not candidates:
                pending.append(f"{name}:missing(branch={branch})")
                continue

            # A newer cancelled/failed rerun must not erase a valid successful
            # run for this exact branch and SHA.  Prefer the newest successful
            # run first, then wait on any active run, and only report a
            # terminal failure when no success exists.
            successful = [
                run
                for run in candidates
                if str(run.get("status") or "").lower() == "completed"
                and str(run.get("conclusion") or "").lower() == "success"
            ]
            if successful:
                selected[name] = max(successful, key=_run_sort_key)
                continue

            active = [
                run
                for run in candidates
                if str(run.get("status") or "").lower() != "completed"
            ]
            if active:
                run = max(active, key=_run_sort_key)
                pending.append(f"{name}:{run.get('status') or 'unknown'}")
                continue

            terminal = [
                run
                for run in candidates
                if str(run.get("status") or "").lower() == "completed"
            ]
            run = max(terminal, key=_run_sort_key)
            failures.append(
                f"{name}:{run.get('conclusion') or 'unknown'}:run={run.get('id')}"
            )

        if failures:
            raise SystemExit("authoritative hosted gate failed: " + ", ".join(failures))
        if len(selected) == len(REQUIRED_GATES):
            return selected
        print(
            f"gate evidence poll {attempt}/{attempts}: "
            + (", ".join(pending) if pending else "waiting"),
            flush=True,
        )
        if attempt < attempts:
            time.sleep(interval_seconds)
    raise SystemExit("timed out waiting for exact-tree authoritative hosted gates")


def generate_sbom(root: Path, output: Path, repository: str, sha: str, run_id: int) -> None:
    lock = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))
    raw_packages = lock.get("package")
    if not isinstance(raw_packages, list) or not raw_packages:
        raise SystemExit("Cargo.lock contains no packages")
    packages: list[dict[str, Any]] = []
    relationships: list[dict[str, str]] = []
    for index, package in enumerate(raw_packages, start=1):
        name = str(package["name"])
        version = str(package["version"])
        safe = re.sub(r"[^A-Za-z0-9.-]", "-", f"{name}-{version}-{index}")
        spdx_id = f"SPDXRef-Package-{safe}"
        item: dict[str, Any] = {
            "SPDXID": spdx_id,
            "name": name,
            "versionInfo": version,
            "downloadLocation": str(package.get("source") or "NOASSERTION"),
            "filesAnalyzed": False,
            "licenseConcluded": "NOASSERTION",
            "licenseDeclared": "NOASSERTION",
            "copyrightText": "NOASSERTION",
            "externalRefs": [
                {
                    "referenceCategory": "PACKAGE-MANAGER",
                    "referenceType": "purl",
                    "referenceLocator": f"pkg:cargo/{urllib.parse.quote(name)}@{urllib.parse.quote(version)}",
                }
            ],
        }
        checksum = package.get("checksum")
        if isinstance(checksum, str) and SHA256_RE.fullmatch(checksum):
            item["checksums"] = [{"algorithm": "SHA256", "checksumValue": checksum}]
        packages.append(item)
        relationships.append(
            {
                "spdxElementId": "SPDXRef-DOCUMENT",
                "relationshipType": "DESCRIBES",
                "relatedSpdxElement": spdx_id,
            }
        )
    write_json(
        output,
        {
            "spdxVersion": "SPDX-2.3",
            "dataLicense": "CC0-1.0",
            "SPDXID": "SPDXRef-DOCUMENT",
            "name": f"CEX P0 candidate {sha}",
            "documentNamespace": f"https://github.com/{repository}/p0-sbom/{sha}/{run_id}",
            "creationInfo": {
                "created": utc_now(),
                "creators": ["Tool: cex-p0-release-evidence"],
                "licenseListVersion": "3.25",
            },
            "packages": packages,
            "relationships": relationships,
        },
    )


def local_evidence_is_successful(name: str, payload: Any) -> bool:
    if not isinstance(payload, dict):
        return False
    if payload.get("ok") is True:
        return True
    if payload.get("status") in {"ok", "passed", "success"}:
        return True
    return False


def collect(args: argparse.Namespace) -> int:
    root = args.repo_root.resolve()
    evidence_dir = args.evidence_dir.resolve()
    context_path = args.context.resolve()
    if not GIT_SHA_RE.fullmatch(args.sha) or not GIT_SHA_RE.fullmatch(args.tree):
        raise SystemExit("sha and tree must be 40 lowercase hex")
    if run_git(root, "rev-parse", "HEAD") != args.sha:
        raise SystemExit("requested evidence SHA does not match checked out HEAD")
    if run_git(root, "rev-parse", "HEAD^{tree}") != args.tree:
        raise SystemExit("requested evidence tree does not match checked out tree")
    token = os.environ.get("GITHUB_TOKEN", "")
    if not token:
        raise SystemExit("GITHUB_TOKEN is required to bind hosted gate evidence")

    evidence_dir.mkdir(parents=True, exist_ok=True)
    for name, relative in LOCAL_EVIDENCE.items():
        path = evidence_dir / relative
        if not path.is_file():
            raise SystemExit(f"missing local evidence {name}: {path}")
        payload = json.loads(path.read_text(encoding="utf-8"))
        if not local_evidence_is_successful(name, payload):
            raise SystemExit(f"local evidence is not successful: {name}")
        commit_sha = payload.get("commit_sha")
        if commit_sha not in {None, "unknown", args.sha}:
            raise SystemExit(f"local evidence {name} is bound to a different commit")

    attempts = int(os.environ.get("CEX_P0_GATE_POLL_ATTEMPTS", "120"))
    interval = int(os.environ.get("CEX_P0_GATE_POLL_INTERVAL_SECONDS", "15"))
    if attempts < 1 or attempts > 240 or interval < 1 or interval > 60:
        raise SystemExit("gate polling bounds are invalid")
    runs = collect_gate_runs(
        args.repository,
        args.sha,
        token,
        attempts,
        interval,
        branch=args.branch,
    )

    gate_records: dict[str, dict[str, Any]] = {}
    gates_dir = evidence_dir / "hosted-gates"
    for name, path in REQUIRED_GATES.items():
        run = runs[name]
        record = {
            "schema": "cex.hosted-gate-evidence.v1",
            "name": name,
            "workflow_path": path,
            "repository": args.repository,
            "branch": args.branch,
            "head_sha": args.sha,
            "head_branch": run.get("head_branch"),
            "run_id": int(run["id"]),
            "run_attempt": int(run.get("run_attempt") or 1),
            "event": run.get("event"),
            "status": run.get("status"),
            "conclusion": run.get("conclusion"),
            "created_at": run.get("created_at"),
            "updated_at": run.get("updated_at"),
            "html_url": run.get("html_url"),
        }
        path_out = gates_dir / f"{name}.json"
        write_json(path_out, record)
        gate_records[name] = {
            "path": path_out.relative_to(evidence_dir).as_posix(),
            "sha256": sha256_file(path_out),
            "run_id": record["run_id"],
            "html_url": record["html_url"],
        }

    migration_head, migration_chain_sha256 = migration_state(root)
    cargo_sha256 = sha256_file(root / "Cargo.lock")
    sbom_path = evidence_dir / "sbom.spdx.json"
    generate_sbom(root, sbom_path, args.repository, args.sha, args.run_id)
    provenance_path = evidence_dir / "provenance.intoto.json"
    write_json(
        provenance_path,
        {
            "_type": "https://in-toto.io/Statement/v1",
            "subject": [
                {
                    "name": args.repository,
                    "digest": {"gitCommit": args.sha, "gitTree": args.tree},
                }
            ],
            "predicateType": "https://slsa.dev/provenance/v1",
            "predicate": {
                "buildDefinition": {
                    "buildType": "https://github.com/TrillionniumFoundation/CEX/p0-release-candidate-gate/v1",
                    "externalParameters": {
                        "repository": args.repository,
                        "branch": args.branch,
                        "commit_sha": args.sha,
                        "workflow_run_id": args.run_id,
                    },
                    "internalParameters": {"migration_head": migration_head.name},
                    "resolvedDependencies": [
                        {
                            "uri": f"git+https://github.com/{args.repository}@{args.sha}",
                            "digest": {"gitCommit": args.sha, "gitTree": args.tree},
                        },
                        {"uri": "file:Cargo.lock", "digest": {"sha256": cargo_sha256[7:]}},
                        {
                            "uri": "file:migrations/",
                            "digest": {"sha256": migration_chain_sha256[7:]},
                        },
                    ],
                },
                "runDetails": {
                    "builder": {"id": "https://github.com/actions/runner"},
                    "metadata": {
                        "invocationId": f"{args.server_url}/{args.repository}/actions/runs/{args.run_id}",
                        "startedOn": utc_now(),
                    },
                },
            },
        },
    )

    files: dict[str, str] = {}
    for path in sorted(evidence_dir.rglob("*")):
        if path.is_file() and path.name != "payload-index.json":
            files[path.relative_to(evidence_dir).as_posix()] = sha256_file(path)
    payload_index_path = evidence_dir / "payload-index.json"
    write_json(
        payload_index_path,
        {
            "schema": "cex.p0-release-evidence-payload.v1",
            "repository": args.repository,
            "branch": args.branch,
            "commit_sha": args.sha,
            "tree_sha": args.tree,
            "workflow_run_id": args.run_id,
            "generated_at": utc_now(),
            "files": files,
        },
    )
    files["payload-index.json"] = sha256_file(payload_index_path)

    context = {
        "repository": args.repository,
        "branch": args.branch,
        "commit_sha": args.sha,
        "tree_sha": args.tree,
        "workflow_run_id": args.run_id,
        "server_url": args.server_url,
        "generated_at": utc_now(),
        "cargo_lock_sha256": cargo_sha256,
        "migration_head": migration_head.name,
        "migration_sha256": sha256_file(migration_head),
        "migration_chain_sha256": migration_chain_sha256,
        "files": files,
        "hosted_gates": gate_records,
    }
    write_json(context_path, context)
    print(context_path)
    return 0


def normalize_digest(value: str) -> str:
    raw = value.strip().lower()
    if raw.startswith("sha256:"):
        raw = raw[7:]
    if not SHA256_RE.fullmatch(raw):
        raise SystemExit("payload digest must be a canonical SHA-256")
    return "sha256:" + raw


def validate_hosted_gate_context(context: dict[str, Any]) -> None:
    """Fail closed if hosted gate records are not bound to one branch/SHA."""

    hosted = context.get("hosted_gates")
    if not isinstance(hosted, dict):
        raise SystemExit("release context lacks hosted_gates")
    if set(hosted) != set(REQUIRED_GATES):
        raise SystemExit("release context hosted_gates do not match required gates")

    repository = context.get("repository")
    branch = context.get("branch")
    sha = context.get("commit_sha")
    if not isinstance(repository, str) or not repository:
        raise SystemExit("release context repository is missing")
    if not isinstance(branch, str) or not branch:
        raise SystemExit("release context branch is missing")
    if not isinstance(sha, str) or not GIT_SHA_RE.fullmatch(sha):
        raise SystemExit("release context commit_sha is invalid")

    for name, expected_path in REQUIRED_GATES.items():
        record = hosted.get(name)
        if not isinstance(record, dict):
            raise SystemExit(f"release context hosted gate is invalid: {name}")
        if record.get("repository") != repository:
            raise SystemExit(f"hosted gate {name} is bound to a different repository")
        if record.get("workflow_path") != expected_path:
            raise SystemExit(f"hosted gate {name} has an unexpected workflow path")
        if record.get("branch") != branch or record.get("head_branch") != branch:
            raise SystemExit(f"hosted gate {name} is bound to a different branch")
        if record.get("head_sha") != sha:
            raise SystemExit(f"hosted gate {name} is bound to a different commit")
        if record.get("status") != "completed" or record.get("conclusion") != "success":
            raise SystemExit(f"hosted gate {name} is not a completed success")
        try:
            run_id = int(record.get("run_id"))
        except (TypeError, ValueError):
            run_id = 0
        if run_id <= 0:
            raise SystemExit(f"hosted gate {name} has an invalid run id")


def manifest(args: argparse.Namespace) -> int:
    context = json.loads(args.context.read_text(encoding="utf-8"))
    if not isinstance(context, dict):
        raise SystemExit("release context must be an object")
    validate_hosted_gate_context(context)
    payload_digest = normalize_digest(args.payload_digest)
    payload_uri = (
        f"gh://{context['repository']}/actions/runs/{context['workflow_run_id']}"
        f"/artifacts/{args.payload_name}"
    )
    files = context["files"]
    evidence: list[dict[str, Any]] = []
    for name, gate in context["hosted_gates"].items():
        evidence.append(
            {
                "name": f"hosted:{name}",
                "status": "pass",
                "uri": f"gh://{context['repository']}/actions/runs/{gate['run_id']}",
                "sha256": gate["sha256"],
                "waiver": None,
            }
        )
    for name, relative in LOCAL_EVIDENCE.items():
        evidence.append(
            {
                "name": name,
                "status": "pass",
                "uri": f"artifact://{args.payload_name}/{relative}",
                "sha256": files[relative],
                "waiver": None,
            }
        )

    data = {
        "schema": "cex.release-baseline-manifest.v1",
        "status": "candidate",
        "qualification_scope": "repository-exact-money-control-plane",
        "production_ready": False,
        "production_authorization": "not_granted",
        "project_id": "hepta-control-plane",
        "release_id": args.release_id,
        "generated_at": utc_now(),
        "source": {
            "repository": context["repository"],
            "branch": context["branch"],
            "commit_sha": context["commit_sha"],
            "tree_sha": context["tree_sha"],
        },
        "dependencies": {"cargo_lock_sha256": context["cargo_lock_sha256"]},
        "database": {
            "migration_head": context["migration_head"],
            "migration_sha256": context["migration_sha256"],
            "migration_chain_sha256": context["migration_chain_sha256"],
        },
        "build": {
            "workflow_run_id": context["workflow_run_id"],
            "artifacts": [
                {
                    "name": args.payload_name,
                    "uri": payload_uri,
                    "sha256": payload_digest,
                }
            ],
            "images": [],
            "sbom": {
                "name": "sbom.spdx.json",
                "uri": f"artifact://{args.payload_name}/sbom.spdx.json",
                "sha256": files["sbom.spdx.json"],
            },
            "provenance": {
                "name": "provenance.intoto.json",
                "uri": f"artifact://{args.payload_name}/provenance.intoto.json",
                "sha256": files["provenance.intoto.json"],
            },
        },
        "evidence": evidence,
        "approvals": [
            {
                "role": "repository-qualification-automation",
                "actor": "github-actions[bot]",
                "decision": "approve",
                "decided_at": utc_now(),
                "scope": "repository candidate only; not production, financial, security, legal, or operations approval",
            }
        ],
        "external_gates": {
            "status": "independent_approval_required",
            "items": [
                "production deployment and rollback rehearsal",
                "real provider reconciliation artifacts",
                "credential custody and break-glass review",
                "two-hour production-like soak and SLO qualification",
                "independent security, operations and financial-control review",
                "human go/no-go approval",
            ],
        },
        "revocation": None,
    }
    write_json(args.output, data)
    print(args.output)
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    collect_parser = subparsers.add_parser("collect")
    collect_parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[1])
    collect_parser.add_argument("--evidence-dir", required=True, type=Path)
    collect_parser.add_argument("--context", required=True, type=Path)
    collect_parser.add_argument("--repository", required=True)
    collect_parser.add_argument("--branch", required=True)
    collect_parser.add_argument("--sha", required=True)
    collect_parser.add_argument("--tree", required=True)
    collect_parser.add_argument("--run-id", required=True, type=int)
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
