#!/usr/bin/env python3
"""Fail-closed Sequence 54 non-regression and integration contract."""
from __future__ import annotations

import json
import re
import sys
import tomllib
from pathlib import Path
from typing import Any, Iterable

ROOT = Path(__file__).resolve().parents[1]
PROBLEMS: list[str] = []
EXPECTED_MIGRATION_HEAD = "0088_enforce_provider_terminal_evidence_binding.sql"
EXPECTED_MEMBERS = [
    "crates/shared-types",
    "crates/shared-errors",
    "crates/shared-tracing",
    "crates/shared-config",
    "crates/hepta-paper-raid-contracts",
    "services/identity-service",
    "services/ledger-service",
    "services/trnm-economy-service",
    "services/gateway-service",
    "services/execution-service",
    "services/audit-service",
    "services/capability-service",
    "services/consumer-entry-api",
    "services/hepta-research-league",
    "services/paper-raid-bff",
    "services/matrix-entry-adapter",
    "apps/matrix-bot-relay",
    "apps/matrix-bot-poller",
    "vendor/trnm-economy-protocol",
    "vendor/trnm-finality-types",
    "vendor/trnm-finality-verifier",
    "vendor/trnm-protocol",
    "vendor/trnm-research-protocol",
]
EXPECTED_CONTEXTS = {
    "fresh-postgres-migrations",
    "repository-integrity",
    "service-local-gate-linux",
    "service-local-gate-windows",
    "hepta-postgres-integration",
    "gateway-exact-reserve",
    "execution-settlement",
    "provider-reconciliation",
    "rust-toolchain-convergence",
    "sequence54-integration-closure",
    "economy-workspace",
    "trnm-economy-settlement/static-contracts",
    "trnm-economy-settlement/rust-postgresql-contracts",
    "repository-candidate-qualification",
}
OBSOLETE_WORKFLOWS = (
    ".github/workflows/seq44-exact-sha-convergence.yml",
    ".github/workflows/world-settlement-final-validation-v2.yml",
    ".github/workflows/world-settlement-final-convergence-v2.yml",
    ".github/workflows/world-settlement-final-convergence-v3.yml",
)


def read(relative: str) -> str:
    path = ROOT / relative
    if not path.is_file():
        PROBLEMS.append(f"missing required file: {relative}")
        return ""
    try:
        return path.read_text(encoding="utf-8")
    except UnicodeDecodeError as error:
        PROBLEMS.append(f"required file is not UTF-8: {relative}: {error}")
        return ""


def load_json(relative: str) -> dict[str, Any]:
    raw = read(relative)
    if not raw:
        return {}
    try:
        value = json.loads(raw)
    except json.JSONDecodeError as error:
        PROBLEMS.append(f"invalid JSON: {relative}: {error}")
        return {}
    if not isinstance(value, dict):
        PROBLEMS.append(f"JSON root must be an object: {relative}")
        return {}
    return value


def require_markers(relative: str, markers: Iterable[str]) -> None:
    text = read(relative)
    for marker in markers:
        if marker not in text:
            PROBLEMS.append(f"{relative} lacks Sequence 54 marker: {marker}")


def dependency_tables(document: dict[str, Any]) -> Iterable[tuple[str, Any]]:
    for table_name in ("dependencies", "dev-dependencies", "build-dependencies"):
        table = document.get(table_name)
        if isinstance(table, dict):
            yield table_name, table
    targets = document.get("target")
    if isinstance(targets, dict):
        for target_name, target in targets.items():
            if not isinstance(target, dict):
                continue
            for table_name in ("dependencies", "dev-dependencies", "build-dependencies"):
                table = target.get(table_name)
                if isinstance(table, dict):
                    yield f"target.{target_name}.{table_name}", table


def validate_workspace() -> None:
    try:
        root_manifest = tomllib.loads(read("Cargo.toml"))
    except tomllib.TOMLDecodeError as error:
        PROBLEMS.append(f"Cargo.toml is invalid: {error}")
        return
    members = root_manifest.get("workspace", {}).get("members")
    if members != EXPECTED_MEMBERS:
        PROBLEMS.append(
            "Sequence 54 workspace drift: expected="
            + json.dumps(EXPECTED_MEMBERS)
            + " actual="
            + json.dumps(members)
        )
        return

    catalog = load_json("docs/module-catalog-v1.json")
    modules = catalog.get("modules")
    catalog_members = (
        [item.get("workspace_member") for item in modules if isinstance(item, dict)]
        if isinstance(modules, list)
        else []
    )
    if catalog_members != EXPECTED_MEMBERS:
        PROBLEMS.append("module catalog no longer exactly matches the 23-member workspace")
    if catalog.get("production_authorization") != "not_granted":
        PROBLEMS.append("module catalog grants production authorization")
    if not isinstance(modules, list) or len(modules) != len(EXPECTED_MEMBERS):
        PROBLEMS.append("module catalog must contain exactly 23 entries")
        modules = []
    seen_documents: set[str] = set()
    for item in modules:
        if not isinstance(item, dict):
            PROBLEMS.append("module catalog entry must be an object")
            continue
        member = item.get("workspace_member")
        document = item.get("documentation")
        if member not in EXPECTED_MEMBERS:
            PROBLEMS.append(f"unknown catalog workspace member: {member!r}")
        if not isinstance(document, str) or not document.startswith("docs/modules/"):
            PROBLEMS.append(f"invalid module documentation path for {member}: {document!r}")
        elif document in seen_documents:
            PROBLEMS.append(f"duplicate module documentation path: {document}")
        else:
            seen_documents.add(document)
            if not (ROOT / document).is_file():
                PROBLEMS.append(f"catalog module documentation is missing: {document}")
    for member in EXPECTED_MEMBERS:
        if not (ROOT / member / "Cargo.toml").is_file():
            PROBLEMS.append(f"workspace member lacks Cargo.toml: {member}")

    workspace_dependencies = root_manifest.get("workspace", {}).get("dependencies", {})
    manifests = [ROOT / "Cargo.toml", *(ROOT / member / "Cargo.toml" for member in EXPECTED_MEMBERS)]
    for manifest_path in manifests:
        if not manifest_path.is_file():
            continue
        try:
            document = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        except (UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
            PROBLEMS.append(f"invalid Cargo manifest {manifest_path.relative_to(ROOT)}: {error}")
            continue
        for section, table in dependency_tables(document):
            for alias, spec in table.items():
                effective = spec
                base = manifest_path.parent
                if isinstance(spec, dict) and spec.get("workspace") is True:
                    effective = workspace_dependencies.get(alias)
                    base = ROOT
                if not isinstance(effective, dict):
                    continue
                if isinstance(effective.get("git"), str):
                    PROBLEMS.append(
                        f"git dependency is forbidden in Sequence 54: {manifest_path.relative_to(ROOT)} [{section}] {alias}"
                    )
                path_value = effective.get("path")
                if not isinstance(path_value, str):
                    continue
                resolved = (base / path_value).resolve()
                try:
                    resolved.relative_to(ROOT.resolve())
                except ValueError:
                    PROBLEMS.append(
                        f"repository-external Cargo path dependency: {manifest_path.relative_to(ROOT)} [{section}] {alias}={path_value}"
                    )


def validate_migrations() -> None:
    migrations = sorted((ROOT / "migrations").glob("[0-9][0-9][0-9][0-9]_*.sql"))
    actual_head = migrations[-1].name if migrations else None
    if actual_head != EXPECTED_MIGRATION_HEAD:
        PROBLEMS.append(
            f"migration head regressed: expected={EXPECTED_MIGRATION_HEAD} actual={actual_head}"
        )
    numbers: dict[str, list[str]] = {}
    for path in migrations:
        numbers.setdefault(path.name[:4], []).append(path.name)
    duplicates = {number: names for number, names in numbers.items() if len(names) != 1}
    if duplicates:
        PROBLEMS.append(f"duplicate migration numbers: {duplicates}")


def validate_candidate_authority() -> None:
    authority = load_json("docs/development-doc-authority-v1.json")
    if authority.get("candidate_sequence") != 54:
        PROBLEMS.append("development-document authority is not Sequence 54")
    if authority.get("migration_head") != EXPECTED_MIGRATION_HEAD:
        PROBLEMS.append("development-document authority migration head regressed")
    if authority.get("integration_plan") != "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-SEQUENCE54-INTEGRATION.md":
        PROBLEMS.append("development-document authority lacks Sequence 54 plan")
    if authority.get("integration_traceability") != "docs/traceability/v12-sequence54-integration-v1.json":
        PROBLEMS.append("development-document authority lacks Sequence 54 traceability")
    if authority.get("production_authorization") != "not_granted":
        PROBLEMS.append("development-document authority grants production authorization")

    trigger = load_json("docs/release-evidence/p0-candidate-trigger.json")
    if trigger.get("sequence") != 54:
        PROBLEMS.append("candidate trigger is not Sequence 54")
    if trigger.get("production_authorization") != "not_granted":
        PROBLEMS.append("candidate trigger grants production authorization")
    integration = load_json("docs/traceability/v12-sequence54-integration-v1.json")
    if integration.get("schema") != "cex.v12-sequence54-integration.v1":
        PROBLEMS.append("Sequence 54 traceability schema is invalid")
    if integration.get("production_authorization") != "not_granted":
        PROBLEMS.append("Sequence 54 traceability grants production authorization")
    if integration.get("functional_parent", {}).get("sha") != "acc798ad07c1005dd8b94b54c4a71eff6288f4c7":
        PROBLEMS.append("Sequence 54 functional parent drift")
    if integration.get("security_parent", {}).get("sha") != "9e1426c09cb19d062504b4a6732bf83867f675ad":
        PROBLEMS.append("Sequence 54 security parent drift")


def validate_toolchain_and_governance() -> None:
    toolchain = load_json("docs/security/rust-toolchain-surfaces-v1.json")
    expected_toolchain = {
        "expected_rust_toolchain": "1.98.1",
        "expected_rust_release_commit": "48a229ceaefd4985c50990b14116b6d856af0985",
        "source_inventory": "git_tree_derived",
        "source_inventory_detail": "recursive_git_tree_and_blob_content",
        "production_authorization": "not_granted",
    }
    for key, expected in expected_toolchain.items():
        if toolchain.get(key) != expected:
            PROBLEMS.append(f"toolchain policy {key} drift: expected={expected!r} actual={toolchain.get(key)!r}")

    ruleset = load_json("docs/repository-ruleset-required-contexts-v1.json")
    if ruleset.get("schema") != "cex.repository-required-contexts.v2":
        PROBLEMS.append("Ruleset policy schema is invalid")
    if ruleset.get("production_authorization") != "not_granted":
        PROBLEMS.append("Ruleset policy grants production authorization")
    checks = ruleset.get("rules", {}).get("required_status_checks", {}).get("required_status_checks", [])
    contexts = {item.get("context") for item in checks if isinstance(item, dict)}
    if contexts != EXPECTED_CONTEXTS or len(checks) != len(EXPECTED_CONTEXTS):
        PROBLEMS.append(
            f"Ruleset required-context drift: missing={sorted(EXPECTED_CONTEXTS - contexts)} extra={sorted(contexts - EXPECTED_CONTEXTS)}"
        )
    pull_request = ruleset.get("rules", {}).get("pull_request", {})
    expected_pr = {
        "dismiss_stale_reviews_on_push": True,
        "require_code_owner_review": True,
        "require_last_push_approval": True,
        "required_approving_review_count": 2,
        "required_review_thread_resolution": True,
    }
    for key, expected in expected_pr.items():
        if pull_request.get(key) != expected:
            PROBLEMS.append(f"Ruleset pull-request control drift: {key}")
    if ruleset.get("ruleset", {}).get("bypass_actors") != []:
        PROBLEMS.append("Ruleset bypass actors must remain empty")

    contents_write = re.compile(r"(?m)^\s*contents:\s*write\s*$")
    git_push = re.compile(r"(?m)^\s*git\s+push(?:\s|$)")
    for workflow in sorted((ROOT / ".github/workflows").glob("*.y*ml")):
        text = workflow.read_text(encoding="utf-8")
        relative = workflow.relative_to(ROOT).as_posix()
        if contents_write.search(text):
            PROBLEMS.append(f"workflow has forbidden contents: write: {relative}")
        if git_push.search(text):
            PROBLEMS.append(f"workflow pushes source: {relative}")
    for relative in OBSOLETE_WORKFLOWS:
        if (ROOT / relative).exists():
            PROBLEMS.append(f"obsolete convergence workflow remains: {relative}")


def validate_sequence52_feature_preservation() -> None:
    require_markers(
        "PROJECT_BOUNDARY.md",
        (
            "Runtime policy: `external_only`",
            "World gameplay, campaigns, economy simulation or authoritative World state",
            "Matrix homeserver accounts, rooms, event ordering, sync tokens or redaction authority",
            "Production authorization: `not_granted`",
        ),
    )
    require_markers(
        "docs/architecture/external-agent-runtime-boundary-sequence-52.md",
        (
            "Runtime policy: `external_only`",
            "participating Agent runtimes",
            "retired `/process` route",
            "Production authorization: `not_granted`",
        ),
    )
    require_markers(
        "services/paper-raid-bff/src/hepta.rs",
        (
            "struct PaperRoomEnvelopeV3",
            "struct PaperReviewStateEnvelopeV1",
            "create_nakama_research_session_control_v2",
            "resume_nakama_research_session_control_v2",
            "replace_nakama_research_session_roster_control_v2",
            "complete_nakama_research_session_control_v2",
        ),
    )
    combined = "\n".join(
        read(path)
        for path in (
            "services/paper-raid-bff/src/browser.js",
            "services/paper-raid-bff/src/html.rs",
            "services/paper-raid-bff/src/hepta.rs",
        )
    )
    for marker in (
        "Registration may already be committed",
        'window.location.assign("/league/start")',
        "agent_proof_nonce_must_equal_idempotency_key",
        "agent_rotation_old_binding_mismatch",
        "PaperAppealSigningV1",
    ):
        if marker not in combined:
            PROBLEMS.append(f"Paper Raid Sequence 52 browser/control marker regressed: {marker}")

    require_markers(
        "services/paper-raid-bff/browser-e2e/mobile-a11y.mjs",
        (
            "backendDOMNodeId",
            "assert.deepEqual(visited, expected",
            "metrics.cssLayoutViewport.clientWidth",
            "post_agent_focus_transition_real_e2e_proved: false",
            "production_bridge_execution_proved: false",
        ),
    )
    require_markers(
        "services/paper-raid-bff/scripts/check-image.sh",
        (
            'scan_runtime_image "$repro_image_id"',
            "runtime_binary_authority_sha256",
            'source_context="$scratch_dir/source"',
            "verify_source_unchanged",
            'if scan_runtime_image "$sentinel_image"',
            "refusing to replace an existing local export image reference",
            'verify_tag_binding "$export_image_ref" "$image_id"',
            "platform=linux/amd64",
        ),
    )
    require_markers(
        "services/paper-raid-bff/scripts/generate-runtime-sbom.sh",
        (
            "independent pinned-builder runtime binaries are not byte-deterministic",
            "runtime binary embeds revision/tree/SBOM/self-hash material",
            "runtime SBOM output must be the exact tracked BFF SBOM path",
            "verify_source_unchanged",
        ),
    )
    require_markers(
        "services/paper-raid-bff/scripts/bind-accessctl-runtime-sbom.sh",
        (
            "accessctl SBOM binding requires a clean committed source tree",
            "independent pinned-builder accessctl binaries are not byte-deterministic",
            "accessctl runtime binary embeds revision/tree/self-hash material",
            '"bound-release-binary"',
        ),
    )
    require_markers(
        "docs/modules/matrix-bot-relay.md",
        ("replay", "queue", "Production authorization: `not_granted`"),
    )
    require_markers(
        "docs/modules/matrix-bot-poller.md",
        ("cursor", "dedupe", "Production authorization: `not_granted`"),
    )


def main() -> int:
    validate_workspace()
    validate_migrations()
    validate_candidate_authority()
    validate_toolchain_and_governance()
    validate_sequence52_feature_preservation()
    result = {
        "schema": "cex.sequence54-integration-check.v1",
        "status": "failed" if PROBLEMS else "ok",
        "workspace_members": len(EXPECTED_MEMBERS),
        "migration_head": EXPECTED_MIGRATION_HEAD,
        "rust_toolchain": "1.98.1",
        "required_status_contexts": len(EXPECTED_CONTEXTS),
        "checker_may_grant_production_authorization": False,
        "production_authorization": "not_granted",
        "problems": PROBLEMS,
    }
    print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
    return 1 if PROBLEMS else 0


if __name__ == "__main__":
    sys.exit(main())
