#!/usr/bin/env python3
"""Apply the strict v12 candidate-evidence contract to a generated manifest."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

_SCRIPT_DIR = Path(__file__).resolve().parent
if str(_SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPT_DIR))
from evidence_safe_io import (  # noqa: E402
    SafeIOError,
    read_json_nofollow,
    read_regular_nofollow,
    sha256_file_nofollow,
)

GIT_SHA_RE = re.compile(r"^[0-9a-f]{40}$")
SHA256_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
BRANCH_RE = re.compile(
    r"^(?!/)(?!.*//)(?!.*\.\.)(?!.*(?:^|/)\.(?:/|$))"
    r"(?!.*(?:^|/)\.\.(?:/|$))(?!.*@\{)(?!refs/)(?!HEAD$)(?!.*/$)"
    r"[A-Za-z0-9._/-]+$"
)
PAYLOAD_NAME_RE = re.compile(
    r"^cex-p0-evidence-(?P<sha>[0-9a-f]{40})-attempt-(?P<attempt>[1-9][0-9]*)$"
)
HOSTED_URI_RE = re.compile(
    r"^gh://TrillionniumFoundation/CEX/actions/runs/[1-9][0-9]*/"
    r"attempts/[1-9][0-9]*$"
)
ZERO_SHA256 = "sha256:" + "0" * 64
QUALIFICATION_SCOPE = (
    "repository-exact-money-control-plane-plus-hepta-durability-doc-integrity-"
    "full-suite-lint-receipt-recovery-and-trnm-production-config-hardening"
)
APPROVAL_SCOPE = (
    "repository candidate only; not production, financial, security, legal, "
    "or operations approval"
)
EXPECTED_EVIDENCE_ORDER = (
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
EXPECTED_EVIDENCE = set(EXPECTED_EVIDENCE_ORDER)
HOSTED_EVIDENCE = set(EXPECTED_EVIDENCE_ORDER[:5])
HOSTED_GATE_NAMES = tuple(name.removeprefix("hosted:") for name in EXPECTED_EVIDENCE_ORDER[:5])
HOSTED_WORKFLOW_PATHS = {
    "p0-migration-gate": ".github/workflows/p0-migration-gate.yml",
    "rust-service-gate": ".github/workflows/rust-service-gate.yml",
    "p0-gateway-exact-reserve-gate": ".github/workflows/p0-gateway-exact-reserve-gate.yml",
    "p0-execution-settlement-gate": ".github/workflows/p0-execution-settlement-gate.yml",
    "p0-provider-reconciliation-gate": ".github/workflows/p0-provider-reconciliation-gate.yml",
}
MIGRATION_RE = re.compile(r"^(\d{4})_[a-z0-9][a-z0-9._-]*\.sql$")
LOCAL_EVIDENCE = {
    "candidate-hygiene": "candidate-hygiene.json",
    "repository-integrity": "repository-integrity.json",
    "hepta-postgres-integration": "hepta-postgres-integration.json",
    "migration-and-lifecycle-matrix": "database-lifecycle.json",
    "exact-ledger-soak": "exact-ledger-soak.json",
    "backup-restore": "backup-restore.json",
}
# These observations are required in the uploaded, exact-tree payload but are
# deliberately payload-only.  Promoting either one to a manifest record would
# recreate the historical 15-versus-13 split-brain contract.
PAYLOAD_ONLY_EVIDENCE = {
    "repository-governance": "repository-governance.json",
    "hosted-run-execution": "hosted-run-execution.json",
}
ATTESTATION_EVIDENCE = {
    "local-evidence-binding": "local-evidence-binding.json",
    "hosted-gate-execution": "hosted-gate-execution.json",
}
FORBIDDEN_SPLIT_BRAIN_EVIDENCE = set(PAYLOAD_ONLY_EVIDENCE)
# The evidence artifact is intentionally a closed set.  A caller may not add a
# convenient log, environment dump or alternate JSON file and have it silently
# become part of the release payload.  Keep this list derived from the manifest
# vocabulary so the producer and final verifier share one canonical namespace.
CANONICAL_PAYLOAD_FILES = frozenset(
    {
        *(f"hosted-gates/{name}.json" for name in HOSTED_GATE_NAMES),
        *LOCAL_EVIDENCE.values(),
        *PAYLOAD_ONLY_EVIDENCE.values(),
        *ATTESTATION_EVIDENCE.values(),
        "sbom.spdx.json",
        "provenance.intoto.json",
        "payload-index.json",
    }
)
CANONICAL_PAYLOAD_DIRECTORIES = frozenset({"hosted-gates"})
SECRET_LIKE_PATH_RE = re.compile(
    r"(?i)(?:^|[/_.-])(?:\.env(?:\.[^/]+)?|secrets?|tokens?|passwords?|"
    r"credentials?|private[-_.]?keys?|id_rsa|.*\.pem|.*\.key)(?:$|[/_.-])"
)
SECRET_LIKE_FIELD_RE = re.compile(
    r"(?i)^(?:password|passwd|secret|token|access[_-]?token|api[_-]?key|"
    r"client[_-]?secret|private[_-]?key|credential|credentials|database[_-]?url)$"
)
SECRET_VALUE_RE = re.compile(
    r"(?i)(?:-----BEGIN [^-\r\n]*PRIVATE KEY-----|"
    r"\b(?:gh[pousr]_[A-Za-z0-9_]{20,}|github_pat_[A-Za-z0-9_]{20,}|"
    r"AKIA[0-9A-Z]{16}|xox[baprs]-[A-Za-z0-9-]{20,})\b|"
    r"\bBearer\s+[A-Za-z0-9._~+/=-]{24,})"
)
REDACTED_SECRET_VALUES = {
    "",
    "none",
    "null",
    "redacted",
    "masked",
    "not_granted",
    "unavailable",
    "unknown",
    "fixture",
    "placeholder",
}
REQUIRED_EVIDENCE_FIELDS = {"name", "status", "uri", "sha256", "waiver"}
EXPECTED_EXTERNAL_GATES = [
    "X1: production-like backup and restore rehearsal against representative data volume and the real storage topology",
    "X2: deployment/cutover and rollback rehearsal with real service identities, network policy and secret custody",
    "X3: real provider reconciliation artifacts for success, definite non-execution and indeterminate outcome",
    "X4: credential issuance, rotation, revocation and break-glass custody review",
    "X5: sustained production-like soak/endurance run with queue age, retry, reconciliation, parity and Audit delivery SLOs",
    "X6: independent security, operations and financial-control review",
    "X7: legal, commercial or provider approvals where the production integration requires them",
    "X8: final human go/no-go decision bound to the immutable release candidate",
]
REQUIRED_ROOT_FIELDS = {
    "schema",
    "status",
    "qualification_scope",
    "production_ready",
    "production_authorization",
    "project_id",
    "release_id",
    "generated_at",
    "source",
    "dependencies",
    "database",
    "build",
    "evidence",
    "approvals",
    "external_gates",
    "revocation",
}
CONTEXT_FIELDS = {
    "repository",
    "branch",
    "commit_sha",
    "tree_sha",
    "workflow_run_id",
    "workflow_run_attempt",
    "payload_name",
    "payload_digest",
    "server_url",
    "generated_at",
    "cargo_lock_sha256",
    "migration_head",
    "migration_sha256",
    "migration_chain_sha256",
    "files",
    "hosted_gates",
    "hosted_gate_selection",
    "qualification_scope",
    "payload_only_attestations",
    "attestations",
}
HOSTED_GATE_SCHEMA = "cex.hosted-gate-evidence.v1"
PAYLOAD_INDEX_SCHEMA = "cex.p0-release-evidence-payload.v1"
GOVERNANCE_SCHEMA = "cex.repository-governance-observation.v1"
EXECUTION_SCHEMA = "cex.hosted-run-execution-verification.v1"
LOCAL_BINDING_SCHEMA = "cex.p0-local-evidence-binding.v1"
HOSTED_GATE_EXECUTION_SCHEMA = "cex.hosted-gate-execution.v1"
HOSTED_GATE_SELECTION_SCHEMA = "cex.hosted-gate-selection-binding.v1"
HOSTED_GATE_SELECTION_POLICY = "latest_authoritative_run_is_binding"
LOCAL_SCHEMAS = {
    "candidate-hygiene": "cex.p0-release-candidate-hygiene.v2",
    "repository-integrity": "cex.repository-integrity.v1",
    "hepta-postgres-integration": "cex.hepta-postgres-integration-evidence.v1",
    "migration-and-lifecycle-matrix": "cex.p0-database-lifecycle.v1",
    "exact-ledger-soak": "cex.p0-exact-ledger-soak.v1",
    "backup-restore": "cex.p0-backup-restore.v1",
}
EXPECTED_HEPTA_CHECKS = [
    "exact-lint-ownership",
    "restart-persistence",
    "multi-instance-disjoint-outbox-claim",
    "wrong-owner-ack-rejection",
    "expired-lease-recovery",
    "readiness-and-metrics",
]
EXPECTED_LIFECYCLE_CHECKS = [
    "development-document-authority",
    "repository-integrity",
    "fresh-migration-and-p0-assertions",
    "existing-row-audit-baseline",
    "ledger-operation-identity-and-replay",
    "invocation-contract-lifecycle",
    "invocation-terminal-mutual-exclusion",
    "gateway-exact-reserve-fault-matrix",
    "execution-settlement-fault-matrix",
    "provider-unknown-outcome-reconciliation",
    "hepta-postgres-restart-and-lease-recovery",
    "trnm-settlement-receipt-response-loss-recovery",
    "term-exchange-receipt-partial-upgrade-regression",
]
LOCAL_REQUIRED_FIELDS = {
    "candidate-hygiene": {"schema", "status", "ok", "problems", "commit_sha", "tree_sha"},
    "repository-integrity": {
        "schema", "status", "ok", "commit_sha", "tree_sha", "repository_commit_sha",
        "repository_tree_sha", "generated_at", "active_plan", "active_addendum",
        "migration_head", "production_authorization", "digests", "documentation_check",
    },
    "hepta-postgres-integration": {
        "schema", "status", "ok", "commit_sha", "tree_sha", "mode", "postgres_required",
        "lint_policy", "completed_at", "checks",
    },
    "migration-and-lifecycle-matrix": {"schema", "ok", "commit_sha", "tree_sha", "completed_at", "checks"},
    "exact-ledger-soak": {
        "schema", "ok", "iterations", "account_id", "balance_minor", "reserved_minor",
        "ledger_entry_count", "distinct_operation_count", "compatibility_entry_count",
        "audit_effect_count", "started_at_epoch", "ended_at_epoch", "duration_seconds",
        "commit_sha", "tree_sha",
    },
    "backup-restore": {
        "schema", "ok", "source", "restored", "dump_sha256", "dump_bytes", "archive_items",
        "started_at_epoch", "ended_at_epoch", "duration_seconds", "commit_sha", "tree_sha",
        "restore_database_retained",
    },
}
# Producer check lists are part of the v12 evidence meaning, not free-form
# notes.  Derive the sets from the ordered declarations above so the fixture,
# producer and contract cannot drift silently when a check is added.
HEPTA_REQUIRED_CHECKS = frozenset(EXPECTED_HEPTA_CHECKS)
LIFECYCLE_REQUIRED_CHECKS = frozenset(EXPECTED_LIFECYCLE_CHECKS)
EXPECTED_RUNNER_LABELS = {
    "ubuntu": "ubuntu-latest",
    "windows": "windows-latest",
}


class ContractError(Exception):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ContractError(message)


def object_at(value: Any, path: str) -> dict[str, Any]:
    require(isinstance(value, dict), f"{path} must be an object")
    return value


def reject_unknown(value: dict[str, Any], allowed: set[str], path: str) -> None:
    require(
        all(isinstance(key, str) for key in value),
        f"{path} contains a non-string field name",
    )
    unknown = sorted(set(value) - allowed)
    require(not unknown, f"{path} contains unknown field(s): {', '.join(unknown)}")


def require_fields(value: dict[str, Any], required: set[str], path: str) -> None:
    require(
        all(isinstance(key, str) for key in value),
        f"{path} contains a non-string field name",
    )
    missing = sorted(required - set(value))
    require(not missing, f"{path} is missing required field(s): {', '.join(missing)}")


def sha256_file(path: Path) -> str:
    try:
        return sha256_file_nofollow(path)
    except SafeIOError as error:
        raise ContractError(str(error)) from error


def validate_payload_file_names(files: Any, path: str = "$context.files") -> dict[str, Any]:
    """Require the exact, relative file vocabulary used by the payload.

    This is deliberately stricter than checking the files needed by the
    manifest: an extra indexed file would otherwise be an unreviewed channel
    for logs, credentials or a second attestation.
    """

    values = object_at(files, path)
    require(
        all(isinstance(key, str) for key in values),
        f"{path} contains a non-string path key",
    )
    names = set(values)
    require(
        names == set(CANONICAL_PAYLOAD_FILES),
        f"{path} must contain exactly the canonical payload file set",
    )
    for relative in names:
        require(
            isinstance(relative, str)
            and relative in CANONICAL_PAYLOAD_FILES
            and relative == Path(relative).as_posix()
            and not Path(relative).is_absolute()
            and ".." not in Path(relative).parts,
            f"{path} contains a non-canonical path: {relative!r}",
        )
    return values


def reject_secret_like_payload(relative: str, raw: bytes, path: str) -> None:
    """Fail closed on credential-shaped names, values and key material.

    Evidence is public release metadata; it must never become a side channel
    for a runner environment or provider credential.  The field check permits
    explicit redaction sentinels (for example ``production_authorization`` is
    intentionally ``not_granted``) while rejecting populated secret fields.
    """

    require(
        not SECRET_LIKE_PATH_RE.search(relative),
        f"{path} has a secret-like path",
    )
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as error:
        raise ContractError(f"{path} is not UTF-8 evidence") from error
    require(not SECRET_VALUE_RE.search(text), f"{path} contains secret-like material")
    try:
        value = json.loads(text)
    except json.JSONDecodeError:
        # The regular JSON loader emits the more useful syntax error later;
        # still scan raw bytes above so malformed secret-bearing files fail
        # closed rather than being reported only as ordinary invalid JSON.
        return

    def walk(node: Any, location: str) -> None:
        if isinstance(node, dict):
            for key, child in node.items():
                key_text = str(key)
                if SECRET_LIKE_FIELD_RE.fullmatch(key_text):
                    if isinstance(child, str):
                        normalized = child.strip().lower()
                        require(
                            normalized in REDACTED_SECRET_VALUES,
                            f"{location}.{key_text} contains a non-redacted secret-like value",
                        )
                    else:
                        require(
                            child is None or child is False,
                            f"{location}.{key_text} contains a non-redacted secret-like value",
                        )
                walk(child, f"{location}.{key_text}")
        elif isinstance(node, list):
            for index, child in enumerate(node):
                walk(child, f"{location}[{index}]")

    walk(value, path)


def validate_payload_directory(root: Path, files: dict[str, Any]) -> None:
    """Validate path, symlink, content and exact-set invariants on disk."""

    require(
        root.is_dir() and not root.is_symlink(),
        "evidence directory is not a real directory",
    )
    root = root.absolute()
    validate_payload_file_names(files)
    actual_paths: set[str] = set()
    for path in root.rglob("*"):
        relative = path.relative_to(root).as_posix()
        require(not path.is_symlink(), f"evidence path is a symlink: {relative}")
        if path.is_dir():
            require(
                relative in CANONICAL_PAYLOAD_DIRECTORIES,
                f"evidence directory contains a non-canonical directory: {relative}",
            )
            continue
        require(path.is_file(), f"evidence path is not a regular file: {relative}")
        actual_paths.add(relative)
        require(
            relative in CANONICAL_PAYLOAD_FILES,
            f"evidence directory contains a non-canonical file: {relative}",
        )
        try:
            raw = read_regular_nofollow(path)
        except SafeIOError as error:
            raise ContractError(
                f"$evidence.files.{relative} cannot be read safely: {error}"
            ) from error
        reject_secret_like_payload(relative, raw, f"$evidence.files.{relative}")
    require(
        actual_paths == set(CANONICAL_PAYLOAD_FILES),
        "evidence directory does not contain exactly the canonical payload files",
    )


def migration_state(root: Path) -> tuple[str, str, str]:
    migrations = sorted(
        path
        for path in (root / "migrations").glob("*.sql")
        if MIGRATION_RE.fullmatch(path.name)
    )
    require(bool(migrations), "repository contains no numbered migrations")
    digest = hashlib.sha256()
    for path in migrations:
        digest.update(path.name.encode("utf-8"))
        digest.update(b"\0")
        try:
            # Migration files are part of the candidate identity.  Read them
            # through the same no-follow boundary as evidence payloads so a
            # checked-out symlink (or a replacement during hashing) cannot
            # redirect the chain digest outside the repository.
            digest.update(read_regular_nofollow(path))
        except SafeIOError as error:
            raise ContractError(f"cannot read migration {path.name} safely: {error}") from error
        digest.update(b"\0")
    head = migrations[-1]
    return head.name, sha256_file(head), "sha256:" + digest.hexdigest()


def load_json_file(root: Path, relative: str, path: str) -> dict[str, Any]:
    root_absolute = root if root.is_absolute() else Path.cwd() / root
    relative_path = Path(relative)
    require(
        not relative_path.is_absolute() and ".." not in relative_path.parts,
        f"{path} escapes evidence directory",
    )
    target = root_absolute / relative_path
    try:
        value = read_json_nofollow(target, label=path)
    except (SafeIOError, OSError, json.JSONDecodeError) as error:
        raise ContractError(f"{path} is not valid JSON: {error}") from error
    return object_at(value, path)


def load_execution_module() -> Any:
    """Load the single source of truth for the exact hosted job contract."""

    verifier = Path(__file__).resolve().with_name("verify-hosted-run-execution.py")
    spec = importlib.util.spec_from_file_location("cex_hosted_execution_contract", verifier)
    require(spec is not None and spec.loader is not None, "cannot load hosted execution verifier")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def positive_int(value: Any, path: str) -> int:
    require(isinstance(value, int) and not isinstance(value, bool) and value > 0, f"{path} must be a positive integer")
    return int(value)


def git_sha(value: Any, path: str) -> str:
    raw = nonempty_string(value, path)
    require(bool(GIT_SHA_RE.fullmatch(raw)) and raw != "0" * 40, f"{path} must be a non-placeholder Git SHA")
    return raw


def validate_context_metadata(ctx: dict[str, Any], source: dict[str, Any]) -> None:
    """Validate context values against the checked-out repository authority.

    Context is generated inside the aggregate workflow, but it is uploaded as
    an artifact and therefore must be treated as untrusted input when the
    final manifest is checked.  In particular, a caller must not be able to
    replace the migration or dependency digests with self-consistent garbage.
    """

    reject_unknown(ctx, CONTEXT_FIELDS, "$context")
    require(ctx.get("repository") == source["repository"], "context repository differs from manifest")
    branch = nonempty_string(ctx.get("branch"), "$context.branch")
    require(bool(BRANCH_RE.fullmatch(branch)), "$context.branch is not canonical")
    require(branch == source["branch"], "context branch differs from manifest")
    require(git_sha(ctx.get("commit_sha"), "$context.commit_sha") == source["commit_sha"], "context commit differs from manifest")
    require(git_sha(ctx.get("tree_sha"), "$context.tree_sha") == source["tree_sha"], "context tree differs from manifest")
    positive_int(ctx.get("workflow_run_id"), "$context.workflow_run_id")
    positive_int(ctx.get("workflow_run_attempt"), "$context.workflow_run_attempt")
    payload_match = PAYLOAD_NAME_RE.fullmatch(nonempty_string(ctx.get("payload_name"), "$context.payload_name"))
    require(payload_match is not None, "$context.payload_name is not canonical")
    require(payload_match.group("sha") == source["commit_sha"], "$context.payload_name is not bound to commit")
    require(int(payload_match.group("attempt")) == ctx["workflow_run_attempt"], "$context.payload_name is not bound to run attempt")
    canonical_sha256(ctx.get("payload_digest"), "$context.payload_digest")
    server_url = nonempty_string(ctx.get("server_url"), "$context.server_url")
    require(server_url.startswith("https://") and not any(ch.isspace() for ch in server_url), "$context.server_url must be an HTTPS URL")
    utc_timestamp(ctx.get("generated_at"), "$context.generated_at")
    require(ctx.get("qualification_scope") == QUALIFICATION_SCOPE, "$context.qualification_scope is stale")

    root = Path(__file__).resolve().parents[1]
    cargo_digest = canonical_sha256(ctx.get("cargo_lock_sha256"), "$context.cargo_lock_sha256")
    require(cargo_digest == sha256_file(root / "Cargo.lock"), "$context Cargo.lock digest differs from checkout")
    head_name, head_digest, chain_digest = migration_state(root)
    require(ctx.get("migration_head") == head_name, "$context migration head differs from checkout")
    require(canonical_sha256(ctx.get("migration_sha256"), "$context.migration_sha256") == head_digest, "$context migration head digest differs from checkout")
    require(canonical_sha256(ctx.get("migration_chain_sha256"), "$context.migration_chain_sha256") == chain_digest, "$context migration chain digest differs from checkout")

    trigger = load_json_file(root, "docs/release-evidence/p0-candidate-trigger.json", "$trigger")
    require(trigger.get("schema") == "cex.p0-candidate-trigger.v1", "$trigger.schema is invalid")
    require(trigger.get("qualification_scope") == QUALIFICATION_SCOPE, "$trigger.qualification_scope is stale")
    require(trigger.get("production_authorization") == "not_granted", "$trigger production authorization must remain denied")
    sequence = trigger.get("sequence")
    require(isinstance(sequence, int) and not isinstance(sequence, bool) and sequence > 0, "$trigger.sequence is invalid")
    # The shared candidate trigger is the sole freeze authority.  A secondary
    # dot-marker can drift without retriggering the exact-tree workflows, so
    # reject any such marker instead of attempting to reconcile two sources.
    secondary_markers = []
    evidence_root = root / "docs/release-evidence"
    for pattern in ("*qualification*freeze*", ".*qualification*freeze*"):
        secondary_markers.extend(
            path for path in evidence_root.glob(pattern)
            if path.name != "p0-candidate-trigger.json"
        )
    require(
        not secondary_markers,
        "non-authoritative qualification freeze marker remains: "
        + ", ".join(sorted(path.name for path in secondary_markers)),
    )
    # `trigger` above was loaded from this exact shared path; validating its
    # sequence and authorization here binds the freeze decision to the same
    # bytes used by the workflow trigger.
    require(trigger.get("sequence") == sequence, "candidate trigger sequence is not self-consistent")
    require(
        trigger.get("production_authorization") == "not_granted",
        "candidate trigger authorization must remain denied",
    )


def nonempty_string(value: Any, path: str) -> str:
    require(isinstance(value, str) and bool(value.strip()), f"{path} must be non-empty")
    return value


def canonical_sha256(value: Any, path: str) -> str:
    raw = nonempty_string(value, path)
    require(bool(SHA256_RE.fullmatch(raw)), f"{path} must be canonical SHA-256")
    require(raw != ZERO_SHA256, f"{path} must not be a placeholder")
    return raw


def utc_timestamp(value: Any, path: str) -> str:
    raw = nonempty_string(value, path)
    require(raw.endswith("Z"), f"{path} must be an explicit UTC Z timestamp")
    try:
        parsed = datetime.fromisoformat(raw[:-1] + "+00:00")
    except ValueError as error:
        raise ContractError(f"{path} must be RFC3339: {error}") from error
    require(
        parsed.tzinfo is not None
        and parsed.utcoffset() == timezone.utc.utcoffset(parsed),
        f"{path} must be an explicit UTC Z timestamp",
    )
    return raw


# Keep the shorter name used by the canonical v12 contract documentation as a
# compatibility alias; both paths enforce an explicit UTC ``Z`` timestamp.
def utc_datetime(value: Any, path: str) -> str:
    return utc_timestamp(value, path)


def immutable_uri(value: Any, path: str, *, hosted: bool) -> str:
    raw = nonempty_string(value, path)
    require(not raw.startswith("file://"), f"{path} must not be a local mutable URI")
    if hosted:
        require(raw.startswith("gh://"), f"{path} must use an immutable gh:// run URI")
        require("/attempts/" in raw, f"{path} must bind an exact run attempt")
    else:
        require(raw.startswith("artifact://"), f"{path} must use artifact://")
    return raw


def hosted_uri_identity(value: str, path: str) -> tuple[int, int]:
    match = HOSTED_URI_RE.fullmatch(value)
    require(match is not None, f"{path} is not an exact hosted attempt")
    parts = value.rstrip("/").split("/")
    try:
        run_id = int(parts[-3])
        run_attempt = int(parts[-1])
    except (IndexError, ValueError) as error:
        raise ContractError(f"{path} has an invalid hosted run identity") from error
    require(run_id > 0 and run_attempt > 0, f"{path} has an invalid hosted run identity")
    return run_id, run_attempt


def validate_hosted_gate_payload(
    record: dict[str, Any], gate_name: str, ctx: dict[str, Any], source: dict[str, Any]
) -> None:
    """Validate the JSON attestation emitted for one selected workflow run."""

    path = f"$evidence.hosted-gates.{gate_name}"
    require(record.get("schema") == HOSTED_GATE_SCHEMA, f"{path}.schema is invalid")
    require(record.get("name") == gate_name, f"{path}.name is invalid")
    require(record.get("repository") == source["repository"], f"{path}.repository is invalid")
    require(record.get("branch") == source["branch"], f"{path}.branch is invalid")
    require(record.get("head_branch") == source["branch"], f"{path}.head_branch is invalid")
    require(record.get("head_sha") == source["commit_sha"], f"{path}.head_sha is invalid")
    require(record.get("workflow_path") == HOSTED_WORKFLOW_PATHS[gate_name], f"{path}.workflow_path is invalid")
    require(record.get("event") in {"push", "workflow_dispatch"}, f"{path}.event is not authoritative")
    require(record.get("status") == "completed" and record.get("conclusion") == "success", f"{path} is not successful")
    run_id = positive_int(record.get("run_id"), f"{path}.run_id")
    run_attempt = positive_int(record.get("run_attempt"), f"{path}.run_attempt")
    context_record = object_at(ctx["hosted_gates"].get(gate_name), f"$context.hosted_gates.{gate_name}")
    for field in (
        "repository", "branch", "head_branch", "head_sha", "workflow_path", "event",
        "status", "conclusion", "run_id", "run_attempt", "created_at", "updated_at",
    ):
        require(record.get(field) == context_record.get(field), f"{path}.{field} differs from context")
    require(run_id == context_record["run_id"] and run_attempt == context_record["run_attempt"], f"{path} run identity differs from context")
    utc_timestamp(record.get("created_at"), f"{path}.created_at")
    utc_timestamp(record.get("updated_at"), f"{path}.updated_at")


def validate_local_payload(
    name: str, payload: dict[str, Any], source: dict[str, Any]
) -> None:
    """Validate the producer-level identity of each local evidence record."""

    path = f"$evidence.{name}"
    require_fields(payload, LOCAL_REQUIRED_FIELDS[name], path)
    require(payload.get("schema") == LOCAL_SCHEMAS[name], f"{path}.schema is invalid")
    require(payload.get("ok") is True, f"{path}.ok must be true")
    status = payload.get("status")
    if status is not None:
        require(status in {"ok", "passed", "success"}, f"{path}.status is not successful")
    require(payload.get("commit_sha") == source["commit_sha"], f"{path}.commit_sha differs from manifest")
    require(payload.get("tree_sha") == source["tree_sha"], f"{path}.tree_sha differs from manifest")
    if name in {"candidate-hygiene", "repository-integrity"}:
        if name == "candidate-hygiene":
            require(isinstance(payload.get("problems"), list), f"{path}.problems must be an array")
            require(payload["problems"] == [], f"{path}.problems must be empty for a passing candidate")
    if name == "repository-integrity":
        require(payload.get("repository_commit_sha") == source["commit_sha"], f"{path}.repository_commit_sha differs from manifest")
        require(payload.get("repository_tree_sha") == source["tree_sha"], f"{path}.repository_tree_sha differs from manifest")
        require(payload.get("production_authorization") == "not_granted", f"{path}.production_authorization must remain denied")
        require(isinstance(payload.get("digests"), dict), f"{path}.digests must be an object")
        documentation_check = object_at(payload.get("documentation_check"), f"{path}.documentation_check")
        require(documentation_check.get("status") == "ok", f"{path}.documentation_check.status is not successful")
        require(documentation_check.get("problems") == [], f"{path}.documentation_check.problems must be empty")
    if name in {"hepta-postgres-integration", "migration-and-lifecycle-matrix"}:
        require(isinstance(payload.get("checks"), list) and payload["checks"], f"{path}.checks must be non-empty")
        require(
            all(isinstance(check, str) and check.strip() for check in payload["checks"]),
            f"{path}.checks must contain only non-empty strings",
        )
        require(
            len(payload["checks"]) == len(set(payload["checks"])),
            f"{path}.checks must not contain duplicate names",
        )
        required_checks = (
            HEPTA_REQUIRED_CHECKS
            if name == "hepta-postgres-integration"
            else LIFECYCLE_REQUIRED_CHECKS
        )
        require(
            required_checks.issubset(set(payload["checks"])),
            f"{path}.checks omits one or more required v12 lifecycle checks",
        )
    if name == "hepta-postgres-integration":
        require(payload.get("postgres_required") is True, f"{path}.postgres_required must be true")
    if name == "migration-and-lifecycle-matrix":
        require(
            "term-exchange-receipt-partial-upgrade-regression" in payload["checks"],
            f"{path} omits partial-upgrade regression",
        )
    if name == "exact-ledger-soak":
        require(positive_int(payload.get("iterations"), f"{path}.iterations") > 0, f"{path}.iterations is invalid")
        for field in (
            "balance_minor", "reserved_minor", "ledger_entry_count",
            "distinct_operation_count", "compatibility_entry_count", "audit_effect_count",
            "started_at_epoch", "ended_at_epoch", "duration_seconds",
        ):
            require(isinstance(payload.get(field), int) and not isinstance(payload.get(field), bool), f"{path}.{field} must be an integer")
        require(payload["balance_minor"] == 0 and payload["reserved_minor"] == 0, f"{path} final money state is not zero")
        require(payload["compatibility_entry_count"] == 0, f"{path} contains compatibility entries")
        require(payload["ended_at_epoch"] >= payload["started_at_epoch"] and payload["duration_seconds"] == payload["ended_at_epoch"] - payload["started_at_epoch"], f"{path} duration is inconsistent")
        require(payload["ledger_entry_count"] == payload["distinct_operation_count"] == payload["audit_effect_count"], f"{path} ledger/audit counts are inconsistent")
    if name == "backup-restore":
        require(isinstance(payload.get("source"), dict) and isinstance(payload.get("restored"), dict), f"{path}.source/restored must be objects")
        require(payload.get("source") == payload.get("restored"), f"{path} restored state differs from source")
        # The PostgreSQL drill publishes the same canonical ``sha256:``
        # representation used by every other evidence digest.  Reject raw or
        # all-zero values rather than accepting two digest dialects.
        canonical_sha256(payload.get("dump_sha256"), f"{path}.dump_sha256")
        for field in ("dump_bytes", "archive_items", "started_at_epoch", "ended_at_epoch", "duration_seconds"):
            require(isinstance(payload.get(field), int) and not isinstance(payload.get(field), bool) and payload[field] >= 0, f"{path}.{field} is invalid")
        require(payload["ended_at_epoch"] >= payload["started_at_epoch"] and payload["duration_seconds"] == payload["ended_at_epoch"] - payload["started_at_epoch"], f"{path} duration is inconsistent")
        require(payload.get("restore_database_retained") is False, f"{path} retained a restore database")
    for field in ("generated_at", "completed_at"):
        if field in payload:
            utc_timestamp(payload[field], f"{path}.{field}")


def validate_governance_payload(
    payload: dict[str, Any], ctx: dict[str, Any], source: dict[str, Any]
) -> None:
    path = "$evidence.repository-governance"
    require(payload.get("schema") == GOVERNANCE_SCHEMA, f"{path}.schema is invalid")
    require(payload.get("ok") is True, f"{path}.ok must be true")
    require(payload.get("repository") == source["repository"], f"{path}.repository is invalid")
    require(payload.get("commit_sha") == source["commit_sha"], f"{path}.commit_sha is invalid")
    require(payload.get("tree_sha") == source["tree_sha"], f"{path}.tree_sha is invalid")
    require(payload.get("candidate_branch") == source["branch"], f"{path}.candidate_branch is invalid")
    require(payload.get("candidate_branch_commit_sha") == source["commit_sha"], f"{path}.candidate_branch_commit_sha is invalid")
    require(payload.get("candidate_branch_commit_sha_final") == source["commit_sha"], f"{path}.candidate_branch_commit_sha_final is invalid")
    require(payload.get("candidate_commit_matches_branch") is True, f"{path} does not bind the candidate branch")
    require(payload.get("candidate_branch_stable_during_observation") is True, f"{path} does not prove branch stability")
    require(payload.get("candidate_tree_matches_commit") is True, f"{path} does not bind the candidate tree")
    require(payload.get("production_authorization") == "not_granted", f"{path} production authorization must remain denied")
    utc_timestamp(payload.get("observed_at"), f"{path}.observed_at")
    require(isinstance(payload.get("default_branch"), str) and payload["default_branch"], f"{path}.default_branch is invalid")
    for field in (
        "default_branch_protected", "candidate_branch_protected",
        "branch_protection_enabled", "candidate_branch_protection_enabled",
        "default_branch_protection_enabled", "candidate_legacy_required_checks_enforced",
        "candidate_ruleset_required_checks_enforced", "required_candidate_checks_enforced",
        "rulesets_readable",
    ):
        require(isinstance(payload.get(field), bool), f"{path}.{field} must be boolean")
    require(isinstance(payload.get("rulesets"), list), f"{path}.rulesets must be an array")
    require(isinstance(payload.get("candidate_rulesets"), list), f"{path}.candidate_rulesets must be an array")
    require(payload.get("ruleset_count") == len(payload["rulesets"]), f"{path}.ruleset_count is inconsistent")
    require(payload.get("candidate_ruleset_count") == len(payload["candidate_rulesets"]), f"{path}.candidate_ruleset_count is inconsistent")
    require(payload.get("candidate_required_status_contexts") == payload.get("actual_required_status_contexts"), f"{path} required-check context aliases differ")
    require(payload.get("desired_required_status_contexts") == [
        "fresh-postgres-migrations", "repository-integrity",
        "service-local-gate-linux", "service-local-gate-windows",
        "hepta-postgres-integration", "gateway-exact-reserve",
        "execution-settlement", "provider-reconciliation",
        "repository-candidate-qualification",
    ], f"{path}.desired_required_status_contexts is not canonical")
    expected_enforced = bool(
        payload["candidate_legacy_required_checks_enforced"]
        or payload["candidate_ruleset_required_checks_enforced"]
    )
    require(payload["required_candidate_checks_enforced"] is expected_enforced, f"{path}.required_candidate_checks_enforced is inconsistent")
    require(payload.get("repository_candidate_enforcement") in {"enforced", "not_enforced", "unverifiable"}, f"{path}.repository_candidate_enforcement is invalid")
    if payload["repository_candidate_enforcement"] == "enforced":
        require(expected_enforced, f"{path} claims enforcement without required checks")
    require(ctx.get("repository") == payload["repository"], f"{path} repository differs from context")


def validate_execution_payload(
    payload: dict[str, Any], ctx: dict[str, Any], source: dict[str, Any]
) -> None:
    """Re-check the exact job/runner/step contract in the persisted verifier output."""

    path = "$evidence.hosted-run-execution"
    require(payload.get("schema") == EXECUTION_SCHEMA, f"{path}.schema is invalid")
    require(payload.get("status") == "ok" and payload.get("ok") is True, f"{path} is not successful")
    require(payload.get("repository") == source["repository"], f"{path}.repository is invalid")
    require(payload.get("branch") == source["branch"], f"{path}.branch is invalid")
    require(payload.get("commit_sha") == source["commit_sha"], f"{path}.commit_sha is invalid")
    require(payload.get("tree_sha") == source["tree_sha"], f"{path}.tree_sha is invalid")
    utc_timestamp(payload.get("verified_at"), f"{path}.verified_at")
    gates = object_at(payload.get("gates"), f"{path}.gates")
    require(set(gates) == set(HOSTED_GATE_NAMES), f"{path}.gates set is not canonical")
    verifier = load_execution_module()
    for gate_name in HOSTED_GATE_NAMES:
        gate = object_at(gates.get(gate_name), f"{path}.gates.{gate_name}")
        context_gate = object_at(ctx["hosted_gates"].get(gate_name), f"$context.hosted_gates.{gate_name}")
        require(gate.get("repository") == source["repository"], f"{path}.{gate_name}.repository is invalid")
        require(gate.get("branch") == source["branch"] and gate.get("head_branch") == source["branch"], f"{path}.{gate_name}.branch is invalid")
        require(gate.get("head_sha") == source["commit_sha"], f"{path}.{gate_name}.head_sha is invalid")
        require(gate.get("event") in {"push", "workflow_dispatch"}, f"{path}.{gate_name}.event is invalid")
        require(gate.get("workflow_path") == HOSTED_WORKFLOW_PATHS[gate_name], f"{path}.{gate_name}.workflow_path is invalid")
        require(gate.get("status") == "success", f"{path}.{gate_name}.status is invalid")
        require(gate.get("run_id") == context_gate.get("run_id") and gate.get("run_attempt") == context_gate.get("run_attempt"), f"{path}.{gate_name} run identity differs from context")
        for field in ("created_at", "updated_at"):
            utc_timestamp(gate.get(field), f"{path}.gates.{gate_name}.{field}")
        jobs = gate.get("jobs")
        require(isinstance(jobs, list), f"{path}.gates.{gate_name}.jobs must be an array")
        expected_jobs = verifier.EXPECTED_JOBS[gate_name]
        require(len(jobs) == len(expected_jobs), f"{path}.{gate_name} job count is not canonical")
        require({job.get("name") for job in jobs if isinstance(job, dict)} == set(expected_jobs), f"{path}.{gate_name} job set is not canonical")
        verified_jobs: list[dict[str, Any]] = []
        for job in sorted(jobs, key=lambda item: str(item.get("name") if isinstance(item, dict) else "")):
            job_value = object_at(job, f"{path}.gates.{gate_name}.jobs[]")
            job_name = job_value.get("name")
            require(
                isinstance(job_name, str) and job_name.strip(),
                f"{path}.{gate_name} job name is invalid",
            )
            require(job_name in expected_jobs, f"{path}.{gate_name} contains an unexpected job")
            raw_job = dict(job_value)
            raw_job["id"] = raw_job.get("job_id")
            try:
                verified = verifier.validate_job(
                    gate_name,
                    raw_job,
                    sha=source["commit_sha"],
                    run_id=context_gate["run_id"],
                    run_attempt=context_gate["run_attempt"],
                    expected=expected_jobs[job_name],
                )
            except (KeyError, TypeError, ValueError) as error:
                raise ContractError(
                    f"{path}.{gate_name}/{job_name} has malformed job evidence: {error}"
                ) from error
            require(verified == job_value, f"{path}.{gate_name}/{job_name} normalized record is inconsistent")
            verified_jobs.append(verified)
        require(gate.get("jobs_sha256") == verifier.canonical_digest(verified_jobs), f"{path}.{gate_name}.jobs_sha256 is invalid")


def validate_local_binding_payload(
    payload: dict[str, Any], ctx: dict[str, Any], source: dict[str, Any]
) -> None:
    """Validate the canonical local-evidence binding attestation."""

    path = "$evidence.local-evidence-binding"
    require(payload.get("schema") == LOCAL_BINDING_SCHEMA, f"{path}.schema is invalid")
    require(payload.get("status") == "ok" and payload.get("ok") is True, f"{path} is not successful")
    for field in ("repository", "branch", "commit_sha", "tree_sha"):
        require(payload.get(field) == source[field], f"{path}.{field} is invalid")
    for field in ("workflow_run_id", "workflow_run_attempt"):
        require(payload.get(field) == ctx[field], f"{path}.{field} differs from context")
    utc_timestamp(payload.get("generated_at"), f"{path}.generated_at")
    records = object_at(payload.get("records"), f"{path}.records")
    require(set(records) == set(LOCAL_EVIDENCE), f"{path}.records set is not canonical")
    files = object_at(ctx.get("files"), "$context.files")
    for name, relative in LOCAL_EVIDENCE.items():
        record = object_at(records.get(name), f"{path}.records.{name}")
        require(record.get("path") == relative, f"{path}.records.{name}.path is invalid")
        digest = canonical_sha256(record.get("sha256"), f"{path}.records.{name}.sha256")
        require(files.get(relative) == digest, f"{path}.records.{name} digest differs from context")
        require(record.get("producer_commit_sha") == source["commit_sha"], f"{path}.records.{name} commit is invalid")
        require(record.get("producer_tree_sha") == source["tree_sha"], f"{path}.records.{name} tree is invalid")


def validate_hosted_gate_execution_payload(
    payload: dict[str, Any], ctx: dict[str, Any], source: dict[str, Any]
) -> None:
    """Validate the canonical hosted-gate job/runner attestation."""

    path = "$evidence.hosted-gate-execution"
    require(payload.get("schema") == HOSTED_GATE_EXECUTION_SCHEMA, f"{path}.schema is invalid")
    require(payload.get("status") == "ok" and payload.get("ok") is True, f"{path} is not successful")
    for field in ("repository", "branch", "commit_sha", "tree_sha"):
        require(payload.get(field) == source[field], f"{path}.{field} is invalid")
    require(payload.get("selection_policy") == "latest_authoritative_run_is_binding", f"{path}.selection_policy is invalid")
    utc_timestamp(payload.get("generated_at"), f"{path}.generated_at")
    gates = object_at(payload.get("gates"), f"{path}.gates")
    expected_paths = set(HOSTED_WORKFLOW_PATHS.values())
    require(set(gates) == expected_paths, f"{path}.gates set is not canonical")
    context_gates = object_at(ctx.get("hosted_gates"), "$context.hosted_gates")
    # Import the checker only for its declarative required-job map.  It does
    # not contact GitHub during validation.
    checker_path = Path(__file__).resolve().with_name("check-hosted-gate-execution.py")
    spec = importlib.util.spec_from_file_location("cex_hosted_gate_checker_contract", checker_path)
    require(spec is not None and spec.loader is not None, f"{path} cannot load hosted gate contract")
    checker = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(checker)
    for gate_name, workflow_path in HOSTED_WORKFLOW_PATHS.items():
        summary = object_at(gates.get(workflow_path), f"{path}.gates.{workflow_path}")
        context_gate = object_at(context_gates.get(gate_name), f"$context.hosted_gates.{gate_name}")
        for field in ("run_id", "run_attempt", "event", "head_branch", "head_sha", "status", "conclusion", "created_at", "updated_at"):
            expected = context_gate.get(field)
            actual = summary.get(field)
            require(actual == expected, f"{path}.gates.{workflow_path}.{field} differs from context")
        require(summary.get("head_branch") == source["branch"], f"{path}.gates.{workflow_path}.branch is invalid")
        require(summary.get("head_sha") == source["commit_sha"], f"{path}.gates.{workflow_path}.head_sha is invalid")
        require(summary.get("event") in {"push", "workflow_dispatch"}, f"{path}.gates.{workflow_path}.event is invalid")
        utc_timestamp(summary.get("created_at"), f"{path}.gates.{workflow_path}.created_at")
        utc_timestamp(summary.get("updated_at"), f"{path}.gates.{workflow_path}.updated_at")
        jobs = summary.get("jobs")
        require(isinstance(jobs, list) and jobs, f"{path}.gates.{workflow_path}.jobs is empty")
        expected_jobs = checker.REQUIRED_GATES[workflow_path]
        require(len(jobs) == len(expected_jobs), f"{path}.gates.{workflow_path}.job count is not canonical")
        job_names: list[str] = []
        for index, raw_job in enumerate(jobs):
            job = object_at(raw_job, f"{path}.gates.{workflow_path}.jobs[{index}]")
            job_name = job.get("name")
            require(
                isinstance(job_name, str) and job_name.strip(),
                f"{path}.gates.{workflow_path}.jobs[{index}].name is invalid",
            )
            job_names.append(job_name)
        require(
            len(job_names) == len(set(job_names)),
            f"{path}.gates.{workflow_path}.jobs contain duplicate names",
        )
        by_name = dict(zip(job_names, jobs))
        require(set(by_name) == set(expected_jobs), f"{path}.gates.{workflow_path}.jobs set is not canonical")
        for job_name, contract in expected_jobs.items():
            job = object_at(by_name.get(job_name), f"{path}.gates.{workflow_path}.jobs.{job_name}")
            require(isinstance(job.get("job_id"), int) and job["job_id"] > 0, f"{path}.{job_name}.job_id is invalid")
            require(isinstance(job.get("runner_id"), int) and job["runner_id"] > 0, f"{path}.{job_name}.runner_id is invalid")
            require(isinstance(job.get("runner_name"), str) and job["runner_name"].strip(), f"{path}.{job_name}.runner_name is invalid")
            require(job.get("status") == "completed", f"{path}.{job_name}.status is invalid")
            require(job.get("conclusion") == "success", f"{path}.{job_name}.conclusion is invalid")
            runner_labels = job.get("runner_labels")
            require(isinstance(runner_labels, list), f"{path}.{job_name}.runner_labels is invalid")
            require(
                contract.get("runner_label") in runner_labels,
                f"{path}.{job_name} lacks required runner label {contract.get('runner_label')!r}",
            )
            require(job.get("required_steps") == sorted(contract["steps"]), f"{path}.{job_name}.required_steps is invalid")
            require(isinstance(job.get("observed_step_count"), int) and job["observed_step_count"] >= len(contract["steps"]), f"{path}.{job_name}.observed_step_count is invalid")


def validate_context_binding(
    data: dict[str, Any], context: Any, evidence_dir: Path | None = None
) -> None:
    """Bind manifest pointers and digests to the collector's final context.

    Shape-only URI checks cannot detect a swapped hosted run or a digest copied
    from another evidence file.  The context is produced by the exact-tree
    collector and carries the final run-list/detail snapshot plus the payload
    file index.  When an evidence directory is supplied, re-hash every indexed
    file as the last local TOCTOU check before qualification.
    """

    ctx = object_at(context, "$context")
    reject_secret_like_payload(
        "context.json",
        (json.dumps(ctx, sort_keys=True, separators=(",", ":")) + "\n").encode("utf-8"),
        "$context",
    )
    source = data["source"]
    validate_context_metadata(ctx, source)
    # The strict wrapper writes both maps after all producer checks.  They are
    # part of the v12 context contract, not optional compatibility metadata:
    # omitting either map would re-open the historical split-brain between
    # payload-only observations and the canonical manifest attestations.
    require_fields(ctx, CONTEXT_FIELDS, "$context")
    require(ctx["repository"] == source["repository"], "context repository differs from manifest")
    require(ctx["branch"] == source["branch"], "context branch differs from manifest")
    require(ctx["commit_sha"] == source["commit_sha"], "context commit differs from manifest")
    require(ctx["tree_sha"] == source["tree_sha"], "context tree differs from manifest")

    build = data["build"]
    require(
        ctx["workflow_run_id"] == build["workflow_run_id"],
        "context workflow run differs from manifest",
    )
    payload = build["artifacts"][0]
    payload_match = PAYLOAD_NAME_RE.fullmatch(payload["name"])
    require(payload_match is not None, "manifest payload name is not canonical")
    require(ctx["payload_name"] == payload["name"], "context payload name differs from manifest")
    require(
        ctx["workflow_run_attempt"] == int(payload_match.group("attempt")),
        "context payload attempt differs from manifest",
    )
    require(payload["sha256"] == ctx["payload_digest"], "payload digest differs from context")

    files = validate_payload_file_names(ctx["files"])
    hosted = object_at(ctx["hosted_gates"], "$context.hosted_gates")
    require(set(hosted) == set(HOSTED_GATE_NAMES), "$context hosted gate set is not canonical")
    # The hosted checker selection is an immutable binding, not a convenience
    # summary.  It prevents a later freshness check from silently substituting
    # another run while retaining the same-looking gate records.
    selection = object_at(ctx.get("hosted_gate_selection"), "$context.hosted_gate_selection")
    reject_unknown(
        selection,
        {"schema", "policy", "source", "sha256", "selected_run_ids"},
        "$context.hosted_gate_selection",
    )
    require(
        selection.get("schema") == HOSTED_GATE_SELECTION_SCHEMA,
        "$context.hosted_gate_selection.schema is invalid",
    )
    require(
        selection.get("policy") == HOSTED_GATE_SELECTION_POLICY,
        "$context.hosted_gate_selection.policy is invalid",
    )
    require(
        selection.get("source") == ATTESTATION_EVIDENCE["hosted-gate-execution"],
        "$context.hosted_gate_selection.source is invalid",
    )
    selection_digest = canonical_sha256(
        selection.get("sha256"), "$context.hosted_gate_selection.sha256"
    )
    require(
        selection_digest == files.get(ATTESTATION_EVIDENCE["hosted-gate-execution"]),
        "$context.hosted_gate_selection digest differs from hosted attestation",
    )
    selected_ids = selection.get("selected_run_ids")
    require(
        isinstance(selected_ids, dict)
        and set(selected_ids) == set(HOSTED_GATE_NAMES),
        "$context.hosted_gate_selection.selected_run_ids set is not canonical",
    )
    for gate_name in HOSTED_GATE_NAMES:
        require(
            isinstance(selected_ids.get(gate_name), int)
            and not isinstance(selected_ids.get(gate_name), bool)
            and selected_ids.get(gate_name) > 0,
            f"$context.hosted_gate_selection.selected_run_ids.{gate_name} is invalid",
        )
        require(
            selected_ids.get(gate_name) == hosted[gate_name].get("run_id"),
            f"$context.hosted_gate_selection.selected_run_ids.{gate_name} differs from context",
        )
    by_name = {item["name"]: item for item in data["evidence"]}
    expected_files: dict[str, str] = {}
    for gate_name in HOSTED_GATE_NAMES:
        record = object_at(hosted.get(gate_name), f"$context.hosted_gates.{gate_name}")
        require(record.get("repository") == source["repository"], f"context hosted gate {gate_name} repository differs")
        require(record.get("branch") == source["branch"], f"context hosted gate {gate_name} branch differs")
        require(record.get("head_branch") == source["branch"], f"context hosted gate {gate_name} head branch differs")
        require(record.get("head_sha") == source["commit_sha"], f"context hosted gate {gate_name} commit differs")
        require(record.get("workflow_path") == HOSTED_WORKFLOW_PATHS[gate_name], f"context hosted gate {gate_name} workflow path differs")
        require(record.get("event") in {"push", "workflow_dispatch"}, f"context hosted gate {gate_name} event is not authoritative")
        require(record.get("status") == "completed" and record.get("conclusion") == "success", f"context hosted gate {gate_name} is not successful")
        run_id = record.get("run_id")
        run_attempt = record.get("run_attempt")
        require(isinstance(run_id, int) and not isinstance(run_id, bool) and run_id > 0, f"context hosted gate {gate_name} run id is invalid")
        require(isinstance(run_attempt, int) and not isinstance(run_attempt, bool) and run_attempt > 0, f"context hosted gate {gate_name} run attempt is invalid")
        evidence_name = f"hosted:{gate_name}"
        item = object_at(by_name.get(evidence_name), f"$.evidence[{evidence_name}]")
        observed_run, observed_attempt = hosted_uri_identity(item["uri"], f"$.evidence[{evidence_name}].uri")
        require((observed_run, observed_attempt) == (run_id, run_attempt), f"evidence {evidence_name} URI is not bound to its context run")
        digest = canonical_sha256(record.get("sha256"), f"$context.hosted_gates.{gate_name}.sha256")
        require(item["sha256"] == digest, f"evidence {evidence_name} digest differs from its context record")
        expected_files[f"hosted-gates/{gate_name}.json"] = digest

    for name, relative in LOCAL_EVIDENCE.items():
        item = object_at(by_name.get(name), f"$.evidence[{name}]")
        digest = canonical_sha256(item["sha256"], f"$.evidence[{name}].sha256")
        context_digest = files.get(relative)
        require(context_digest == digest, f"evidence {name} digest is not bound to context file {relative}")
        expected_files[relative] = digest

    # Only these two attestation records are promoted into the canonical
    # thirteen-record manifest.  Governance and exact-run execution remain
    # payload-only, but must still be present and digest-bound below.
    attestations = object_at(ctx.get("attestations"), "$context.attestations")
    require(
        set(attestations) == set(ATTESTATION_EVIDENCE),
        "$context.attestations set is not canonical",
    )
    for name, relative in ATTESTATION_EVIDENCE.items():
        item = object_at(by_name.get(name), f"$.evidence[{name}]")
        digest = canonical_sha256(item["sha256"], f"$.evidence[{name}].sha256")
        require(
            files.get(relative) == digest,
            f"evidence {name} digest is not bound to context file {relative}",
        )
        expected_files[relative] = digest
        attestation = object_at(attestations.get(name), f"$context.attestations.{name}")
        require(attestation.get("path") == relative, f"$context.attestations.{name}.path is invalid")
        require(attestation.get("sha256") == digest, f"$context.attestations.{name}.sha256 differs from manifest")

    payload_only = object_at(
        ctx.get("payload_only_attestations"),
        "$context.payload_only_attestations",
    )
    require(
        set(payload_only) == set(PAYLOAD_ONLY_EVIDENCE.values()),
        "$context.payload_only_attestations set is not canonical",
    )
    for relative in PAYLOAD_ONLY_EVIDENCE.values():
        digest = canonical_sha256(
            payload_only.get(relative),
            f"$context.payload_only_attestations.{relative}",
        )
        require(
            files.get(relative) == digest,
            f"payload-only attestation digest is not bound: {relative}",
        )
        expected_files[relative] = digest

    for field in ("sbom", "provenance"):
        artifact = build[field]
        relative = artifact["name"]
        digest = canonical_sha256(artifact["sha256"], f"$.build.{field}.sha256")
        require(files.get(relative) == digest, f"build.{field} digest is not bound to context file {relative}")
        expected_files[relative] = digest

    for relative, digest in expected_files.items():
        require(canonical_sha256(files.get(relative), f"$context.files.{relative}") == digest, f"context file digest is invalid: {relative}")

    if evidence_dir is not None:
        require(
            not evidence_dir.is_symlink(),
            "evidence directory must not be a symlink",
        )
        root = evidence_dir.absolute()
        validate_payload_directory(root, files)
        for relative, expected_digest in files.items():
            canonical_sha256(expected_digest, f"$context.files.{relative}")
            path = root / relative
            require(path.is_file() and not path.is_symlink(), f"context file is missing: {relative}")
            require(sha256_file(path) == expected_digest, f"context file hash changed: {relative}")
        actual_files = {
            path.relative_to(root).as_posix()
            for path in root.rglob("*")
            if path.is_file()
        }
        require(actual_files == set(CANONICAL_PAYLOAD_FILES), "evidence directory files differ from canonical payload set")
        payload_index = load_json_file(root, "payload-index.json", "$evidence.payload-index")
        require_fields(
            payload_index,
            {
                "schema", "repository", "branch", "commit_sha", "tree_sha",
                "workflow_run_id", "workflow_run_attempt", "payload_name",
                "generated_at", "files",
            },
            "$evidence.payload-index",
        )
        reject_unknown(
            payload_index,
            {
                "schema", "repository", "branch", "commit_sha", "tree_sha",
                "workflow_run_id", "workflow_run_attempt", "payload_name",
                "generated_at", "files",
            },
            "$evidence.payload-index",
        )
        require(payload_index.get("schema") == PAYLOAD_INDEX_SCHEMA, "$evidence.payload-index.schema is invalid")
        for field in (
            "repository", "branch", "commit_sha", "tree_sha", "workflow_run_id",
            "workflow_run_attempt", "payload_name", "generated_at",
        ):
            require(payload_index.get(field) == ctx.get(field), f"$evidence.payload-index.{field} differs from context")
        require(isinstance(payload_index.get("files"), dict), "$evidence.payload-index.files must be an object")
        require(
            all(isinstance(key, str) for key in payload_index["files"]),
            "$evidence.payload-index.files contains a non-string path",
        )
        require(
            set(payload_index["files"]) == set(CANONICAL_PAYLOAD_FILES) - {"payload-index.json"},
            "$evidence.payload-index.files set is not canonical",
        )
        require(payload_index["files"] == {key: value for key, value in files.items() if key != "payload-index.json"}, "$evidence.payload-index.files differs from context")

        for gate_name in HOSTED_GATE_NAMES:
            gate_file = load_json_file(root, f"hosted-gates/{gate_name}.json", f"$evidence.hosted-gates.{gate_name}")
            validate_hosted_gate_payload(gate_file, gate_name, ctx, source)
        for name, relative in LOCAL_EVIDENCE.items():
            local_file = load_json_file(root, relative, f"$evidence.{name}")
            validate_local_payload(name, local_file, source)

        governance_file = load_json_file(
            root, PAYLOAD_ONLY_EVIDENCE["repository-governance"], "$evidence.repository-governance"
        )
        validate_governance_payload(governance_file, ctx, source)
        execution_file = load_json_file(
            root, PAYLOAD_ONLY_EVIDENCE["hosted-run-execution"], "$evidence.hosted-run-execution"
        )
        validate_execution_payload(execution_file, ctx, source)
        binding_file = load_json_file(
            root, ATTESTATION_EVIDENCE["local-evidence-binding"], "$evidence.local-evidence-binding"
        )
        validate_local_binding_payload(binding_file, ctx, source)
        hosted_execution_file = load_json_file(
            root, ATTESTATION_EVIDENCE["hosted-gate-execution"], "$evidence.hosted-gate-execution"
        )
        validate_hosted_gate_execution_payload(hosted_execution_file, ctx, source)

        sbom = load_json_file(root, "sbom.spdx.json", "$evidence.sbom")
        require(sbom.get("spdxVersion") == "SPDX-2.3", "$evidence.sbom.spdxVersion is invalid")
        require(sbom.get("SPDXID") == "SPDXRef-DOCUMENT", "$evidence.sbom.SPDXID is invalid")
        require(sbom.get("name") == f"CEX P0 candidate {source['commit_sha']}", "$evidence.sbom.name is not candidate-bound")
        require(sbom.get("documentNamespace") == f"https://github.com/{source['repository']}/p0-sbom/{source['commit_sha']}/{ctx['workflow_run_id']}/attempt/{ctx['workflow_run_attempt']}", "$evidence.sbom namespace is not candidate-bound")
        require(isinstance(sbom.get("packages"), list) and sbom["packages"], "$evidence.sbom.packages is empty")
        creation_info = object_at(sbom.get("creationInfo"), "$evidence.sbom.creationInfo")
        utc_timestamp(creation_info.get("created"), "$evidence.sbom.creationInfo.created")
        require("Tool: cex-p0-release-evidence" in creation_info.get("creators", []), "$evidence.sbom creator is invalid")

        provenance = load_json_file(root, "provenance.intoto.json", "$evidence.provenance")
        require(provenance.get("_type") == "https://in-toto.io/Statement/v1", "$evidence.provenance._type is invalid")
        require(provenance.get("predicateType") == "https://slsa.dev/provenance/v1", "$evidence.provenance.predicateType is invalid")
        subjects = provenance.get("subject")
        require(isinstance(subjects, list) and len(subjects) == 1, "$evidence.provenance.subject is invalid")
        subject = object_at(subjects[0], "$evidence.provenance.subject[0]")
        require(subject.get("name") == source["repository"], "$evidence.provenance subject name is invalid")
        subject_digest = object_at(subject.get("digest"), "$evidence.provenance.subject.digest")
        require(subject_digest.get("gitCommit") == source["commit_sha"] and subject_digest.get("gitTree") == source["tree_sha"], "$evidence.provenance subject is not candidate-bound")
        predicate = object_at(provenance.get("predicate"), "$evidence.provenance.predicate")
        build_definition = object_at(predicate.get("buildDefinition"), "$evidence.provenance.buildDefinition")
        external_parameters = object_at(build_definition.get("externalParameters"), "$evidence.provenance.externalParameters")
        require(external_parameters == {
            "repository": source["repository"],
            "branch": source["branch"],
            "commit_sha": source["commit_sha"],
            "workflow_run_id": ctx["workflow_run_id"],
            "workflow_run_attempt": ctx["workflow_run_attempt"],
        }, "$evidence.provenance external parameters are not candidate-bound")
        require(object_at(build_definition.get("internalParameters"), "$evidence.provenance.internalParameters").get("migration_head") == ctx["migration_head"], "$evidence.provenance migration head is invalid")
        dependencies = build_definition.get("resolvedDependencies")
        require(isinstance(dependencies, list), "$evidence.provenance resolvedDependencies is invalid")
        dependency_map = {item.get("uri"): item for item in dependencies if isinstance(item, dict)}
        git_dependency = dependency_map.get(f"git+https://github.com/{source['repository']}@{source['commit_sha']}")
        require(isinstance(git_dependency, dict), "$evidence.provenance git dependency is missing")
        require(object_at(git_dependency.get("digest"), "$evidence.provenance git digest").get("gitTree") == source["tree_sha"], "$evidence.provenance git tree is invalid")
        cargo_dependency = object_at(dependency_map.get("file:Cargo.lock"), "$evidence.provenance Cargo.lock dependency")
        require(object_at(cargo_dependency.get("digest"), "$evidence.provenance Cargo.lock digest").get("sha256") == ctx["cargo_lock_sha256"][7:], "$evidence.provenance Cargo.lock digest is invalid")
        migration_dependency = object_at(dependency_map.get("file:migrations/"), "$evidence.provenance migrations dependency")
        require(object_at(migration_dependency.get("digest"), "$evidence.provenance migration digest").get("sha256") == ctx["migration_chain_sha256"][7:], "$evidence.provenance migration digest is invalid")
        run_details = object_at(predicate.get("runDetails"), "$evidence.provenance.runDetails")
        require(object_at(run_details.get("builder"), "$evidence.provenance builder").get("id") == "https://github.com/actions/runner", "$evidence.provenance builder is invalid")
        metadata = object_at(run_details.get("metadata"), "$evidence.provenance metadata")
        require(metadata.get("invocationId") == f"{ctx['server_url']}/{source['repository']}/actions/runs/{ctx['workflow_run_id']}/attempts/{ctx['workflow_run_attempt']}", "$evidence.provenance invocation is not candidate-bound")
        utc_timestamp(metadata.get("startedOn"), "$evidence.provenance metadata.startedOn")

        # The first pass protects the JSON reads from an omitted/changed file;
        # this second pass closes the small window between hashing and parsing
        # (for example, an artifact-side writer replacing a governance record).
        # Qualification succeeds only when the bytes that were semantically
        # checked are still exactly the bytes indexed by the context.
        for relative, expected_digest in files.items():
            # Keep the path lexical for the final no-follow read.  Resolving
            # here would follow a symlink introduced between the semantic
            # parse and this second hash pass, defeating the TOCTOU guard.
            path = root / relative
            require(
                sha256_file(path) == expected_digest,
                f"context file changed during semantic validation: {relative}",
            )


def validate_manifest(
    data: Any, *, context: Any | None = None, evidence_dir: Path | None = None
) -> None:
    root = object_at(data, "$")
    require_fields(root, REQUIRED_ROOT_FIELDS, "$")
    reject_unknown(
        root,
        {
            "schema", "status", "qualification_scope", "production_ready",
            "production_authorization", "project_id", "release_id", "generated_at",
            "source", "dependencies", "database", "build", "evidence", "approvals",
            "external_gates", "revocation",
        },
        "$",
    )
    require(root.get("schema") == "cex.release-baseline-manifest.v1", "$.schema is invalid")
    require(root.get("status") == "candidate", "$.status must be candidate")
    require(root.get("qualification_scope") == QUALIFICATION_SCOPE, "$.qualification_scope is stale")
    require(root.get("production_ready") is False, "$.production_ready must be false")
    require(root.get("production_authorization") == "not_granted", "production authorization must remain denied")
    require(root.get("project_id") == "hepta-control-plane", "$.project_id is invalid")

    source = object_at(root.get("source"), "$.source")
    require_fields(source, {"repository", "branch", "commit_sha", "tree_sha"}, "$.source")
    reject_unknown(source, {"repository", "branch", "commit_sha", "tree_sha"}, "$.source")
    require(source.get("repository") == "TrillionniumFoundation/CEX", "source repository is invalid")
    branch = nonempty_string(source.get("branch"), "$.source.branch")
    require(bool(BRANCH_RE.fullmatch(branch)), "source branch is not canonical")
    commit_sha = nonempty_string(source.get("commit_sha"), "$.source.commit_sha")
    tree_sha = nonempty_string(source.get("tree_sha"), "$.source.tree_sha")
    require(bool(GIT_SHA_RE.fullmatch(commit_sha)), "commit SHA must be 40 lowercase hex")
    require(bool(GIT_SHA_RE.fullmatch(tree_sha)), "tree SHA must be 40 lowercase hex")
    require(commit_sha != "0" * 40 and tree_sha != "0" * 40, "source identity must not be a placeholder")

    release_id = nonempty_string(root.get("release_id"), "$.release_id")
    require(release_id == f"cex-p0-{commit_sha}", "release_id is not bound to source.commit_sha")
    utc_timestamp(root.get("generated_at"), "$.generated_at")

    dependencies = object_at(root.get("dependencies"), "$.dependencies")
    require_fields(dependencies, {"cargo_lock_sha256"}, "$.dependencies")
    reject_unknown(dependencies, {"cargo_lock_sha256"}, "$.dependencies")
    canonical_sha256(dependencies.get("cargo_lock_sha256"), "$.dependencies.cargo_lock_sha256")
    database = object_at(root.get("database"), "$.database")
    require_fields(database, {"migration_head", "migration_sha256", "migration_chain_sha256"}, "$.database")
    reject_unknown(
        database,
        {"migration_head", "migration_sha256", "migration_chain_sha256"},
        "$.database",
    )
    require(database.get("migration_head") == "0087_add_term_exchange_receipt_event_history.sql", "migration head is stale")
    canonical_sha256(database.get("migration_sha256"), "$.database.migration_sha256")
    canonical_sha256(database.get("migration_chain_sha256"), "$.database.migration_chain_sha256")

    build = object_at(root.get("build"), "$.build")
    require_fields(build, {"workflow_run_id", "artifacts", "images", "sbom", "provenance"}, "$.build")
    reject_unknown(build, {"workflow_run_id", "artifacts", "images", "sbom", "provenance"}, "$.build")
    run_id = build.get("workflow_run_id")
    require(isinstance(run_id, int) and not isinstance(run_id, bool) and run_id > 0, "workflow_run_id is invalid")
    artifacts = build.get("artifacts")
    require(isinstance(artifacts, list) and len(artifacts) == 1, "candidate requires exactly one evidence payload")
    payload = object_at(artifacts[0], "$.build.artifacts[0]")
    require_fields(payload, {"name", "uri", "sha256"}, "$.build.artifacts[0]")
    reject_unknown(payload, {"name", "uri", "sha256"}, "$.build.artifacts[0]")
    payload_name = nonempty_string(payload.get("name"), "$.build.artifacts[0].name")
    payload_match = PAYLOAD_NAME_RE.fullmatch(payload_name)
    require(payload_match is not None, "payload name is not canonical")
    require(payload_match.group("sha") == commit_sha, "payload name is not bound to commit")
    payload_attempt = int(payload_match.group("attempt"))
    payload_uri = immutable_uri(payload.get("uri"), "$.build.artifacts[0].uri", hosted=True)
    require(
        payload_uri
        == f"gh://TrillionniumFoundation/CEX/actions/runs/{run_id}/attempts/{payload_attempt}/artifacts/{payload_name}",
        "payload URI is not bound to run/attempt/name",
    )
    canonical_sha256(payload.get("sha256"), "$.build.artifacts[0].sha256")
    require(build.get("images") == [], "candidate build images must be empty until separately qualified")
    for field, expected_name in (
        ("sbom", "sbom.spdx.json"),
        ("provenance", "provenance.intoto.json"),
    ):
        artifact = object_at(build.get(field), f"$.build.{field}")
        require_fields(artifact, {"name", "uri", "sha256"}, f"$.build.{field}")
        reject_unknown(artifact, {"name", "uri", "sha256"}, f"$.build.{field}")
        require(artifact.get("name") == expected_name, f"$.build.{field}.name is invalid")
        immutable_uri(artifact.get("uri"), f"$.build.{field}.uri", hosted=False)
        require(
            artifact.get("uri") == f"artifact://{payload_name}/{expected_name}",
            f"$.build.{field}.uri is not payload-bound",
        )
        canonical_sha256(artifact.get("sha256"), f"$.build.{field}.sha256")

    evidence = root.get("evidence")
    require(isinstance(evidence, list), "$.evidence must be an array")
    names: list[str] = []
    uris: list[str] = []
    for index, raw in enumerate(evidence):
        item = object_at(raw, f"$.evidence[{index}]")
        require_fields(item, REQUIRED_EVIDENCE_FIELDS, f"$.evidence[{index}]")
        reject_unknown(item, {"name", "status", "uri", "sha256", "waiver"}, f"$.evidence[{index}]")
        name = nonempty_string(item.get("name"), f"$.evidence[{index}].name")
        names.append(name)
        require(item.get("status") == "pass", f"evidence {name} must be pass; waivers are forbidden")
        require(item.get("waiver") is None, f"evidence {name} must not carry a waiver")
        uri = immutable_uri(
            item.get("uri"),
            f"$.evidence[{index}].uri",
            hosted=name in HOSTED_EVIDENCE,
        )
        if name in HOSTED_EVIDENCE:
            require(bool(HOSTED_URI_RE.fullmatch(uri)), f"evidence {name} URI is not an exact hosted attempt")
        if name in LOCAL_EVIDENCE:
            require(
                uri == f"artifact://{payload_name}/{LOCAL_EVIDENCE[name]}",
                f"evidence {name} URI is not payload-bound",
            )
        if name in ATTESTATION_EVIDENCE:
            require(
                uri == f"artifact://{payload_name}/{ATTESTATION_EVIDENCE[name]}",
                f"evidence {name} URI is not payload-bound",
            )
        uris.append(uri)
        canonical_sha256(item.get("sha256"), f"$.evidence[{index}].sha256")
    require(len(names) == len(set(names)), "evidence names must be unique")
    require(len(uris) == len(set(uris)), "evidence URIs must be unique")
    require(tuple(names) == EXPECTED_EVIDENCE_ORDER, "evidence order is not canonical")
    require(set(names) == EXPECTED_EVIDENCE, "candidate evidence set is incomplete or contains additions")
    require(
        not FORBIDDEN_SPLIT_BRAIN_EVIDENCE.intersection(names),
        "payload-only attestations must not become extra manifest evidence",
    )

    approvals = root.get("approvals")
    require(isinstance(approvals, list) and len(approvals) == 1, "candidate requires exactly one repository automation approval")
    approval = object_at(approvals[0], "$.approvals[0]")
    require_fields(approval, {"role", "actor", "decision", "decided_at", "scope"}, "$.approvals[0]")
    reject_unknown(approval, {"role", "actor", "decision", "decided_at", "scope"}, "$.approvals[0]")
    require(approval.get("role") == "repository-qualification-automation", "approval role is invalid")
    require(approval.get("actor") == "github-actions[bot]", "approval actor is invalid")
    require(approval.get("decision") == "approve", "approval decision is invalid")
    require(approval.get("scope") == APPROVAL_SCOPE, "approval scope exceeds repository qualification")
    utc_timestamp(approval.get("decided_at"), "$.approvals[0].decided_at")

    external = object_at(root.get("external_gates"), "$.external_gates")
    require_fields(external, {"status", "items"}, "$.external_gates")
    reject_unknown(external, {"status", "items"}, "$.external_gates")
    require(external.get("status") == "independent_approval_required", "external gate status is invalid")
    require(external.get("items") == EXPECTED_EXTERNAL_GATES, "external X1-X8 gate set/order is invalid")
    require("revocation" in root, "$.revocation is required")
    require(root["revocation"] is None, "candidate revocation must be null")
    if context is not None:
        require(evidence_dir is not None, "evidence_dir is required when binding a release context")
        validate_context_binding(root, context, evidence_dir)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", type=Path)
    parser.add_argument(
        "--context",
        type=Path,
        required=True,
        help="collector context used to bind hosted run IDs and evidence digests",
    )
    parser.add_argument(
        "--evidence-dir",
        type=Path,
        required=True,
        help="payload directory to re-hash when --context is supplied",
    )
    args = parser.parse_args()
    try:
        # The manifest and context are untrusted artifact inputs at this
        # boundary.  Do not follow a caller-supplied symlink while deciding
        # whether a candidate qualifies.
        value = read_json_nofollow(args.manifest, label="candidate manifest")
        context = None
        if args.context is not None:
            context = read_json_nofollow(args.context, label="release context")
        evidence_dir = args.evidence_dir
        if evidence_dir is not None and not evidence_dir.is_absolute():
            evidence_dir = Path(__file__).resolve().parents[1] / evidence_dir
        validate_manifest(value, context=context, evidence_dir=evidence_dir)
    except (
        OSError,
        json.JSONDecodeError,
        SafeIOError,
        ContractError,
        KeyError,
        TypeError,
        ValueError,
    ) as error:
        print(f"strict release evidence contract failed: {error}")
        return 1
    print(f"strict release evidence contract passed: {args.manifest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
