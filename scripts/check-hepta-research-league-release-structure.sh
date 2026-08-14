#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
scratch=$(mktemp -d)

cleanup() {
  case "$scratch" in
    /tmp/tmp.*) rm -rf -- "$scratch" ;;
    *) echo "refusing to remove unexpected Hepta structure-gate scratch path" >&2 ;;
  esac
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

for command_name in bash cmp python3 sha256sum timeout; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "Hepta release structure gate requires $command_name" >&2
    exit 1
  }
done

shell_scripts=(
  scripts/build-hepta-research-league-image.sh
  scripts/check-hepta-receipt-v2-resource-gate.sh
  scripts/check-hepta-research-league-compose-smoke.sh
  scripts/check-hepta-research-league-release.sh
  scripts/check-hepta-research-league-release-structure.sh
  scripts/check-paper-raid-alpha-candidate-postgres.sh
  scripts/download-pinned-buildx.sh
  scripts/generate-hepta-research-league-docker-lock.sh
  scripts/generate-hepta-research-league-runtime-sbom.sh
)
python_scripts=(
  scripts/admit-hepta-image-build-evidence.py
  scripts/check-hepta-route-openapi-parity.py
  scripts/generate-hepta-receipt-v2-resource-fixtures.py
  scripts/generate-hepta-research-league-sbom.py
  scripts/verify-hepta-clean-source.py
  scripts/verify-hepta-research-league-rootfs-tar.py
  scripts/verify-hepta-research-league-sbom.py
  scripts/verify-hepta-clean-source.py
)
for relative_path in "${shell_scripts[@]}"; do
  bash -n "$repo_dir/$relative_path"
done

timeout 30s "$repo_dir/scripts/generate-hepta-receipt-v2-resource-fixtures.py" \
  --self-test >/dev/null
for relative_path in "${python_scripts[@]}"; do
  python3 -c 'import pathlib; path = pathlib.Path(__import__("sys").argv[1]); compile(path.read_text(encoding="utf-8"), str(path), "exec")' \
    "$repo_dir/$relative_path"
done

python3 - "$repo_dir" "$scratch" <<'PY'
import copy
import hashlib
import json
import os
import pathlib
import re
import shlex
import stat
import subprocess
import sys
import tarfile
import tomllib
from collections import Counter

import yaml


repo = pathlib.Path(sys.argv[1])
scratch = pathlib.Path(sys.argv[2])
dockerfile_path = repo / "services/hepta-research-league/Dockerfile"
dockerfile = dockerfile_path.read_text(encoding="utf-8")


def fail(message):
    raise AssertionError(message)


def validate_alpha_postgres_test_isolation(text):
    expected_invocation = (
        'HEPTA_TEST_DATABASE_URL="$database_url" \\\n'
        '  flock -n "$cargo_gate" cargo test --locked -p hepta-research-league '
        '-- --test-threads=1\n'
    )
    if text.count(expected_invocation) != 1:
        fail(
            "fixed-digest candidate PostgreSQL gate must serialize the shared "
            "database at the libtest harness boundary"
        )
    if text.count("cargo test") != 1:
        fail("fixed-digest candidate PostgreSQL gate test invocation is ambiguous")


alpha_postgres_gate = (
    repo / "scripts/check-paper-raid-alpha-candidate-postgres.sh"
).read_text(encoding="utf-8")
validate_alpha_postgres_test_isolation(alpha_postgres_gate)
alpha_postgres_gate_mutations = {
    "parallel shared database": alpha_postgres_gate.replace(
        " -- --test-threads=1", "", 1
    ),
    "more than one harness thread": alpha_postgres_gate.replace(
        "--test-threads=1", "--test-threads=2", 1
    ),
    "thread bound detached from libtest": alpha_postgres_gate.replace(
        "-- --test-threads=1", "--test-threads=1 --", 1
    ),
}
for mutation_name, mutation in alpha_postgres_gate_mutations.items():
    if mutation == alpha_postgres_gate:
        fail(f"PostgreSQL harness-isolation mutation was not applied: {mutation_name}")
    try:
        validate_alpha_postgres_test_isolation(mutation)
    except AssertionError:
        pass
    else:
        fail(f"PostgreSQL harness-isolation mutation was accepted: {mutation_name}")


collaboration_source = (
    repo / "services/hepta-research-league/src/paper_collaboration_v3.rs"
).read_text(encoding="utf-8")
proposal_authority_start = collaboration_source.find("fn verify_agent_proposal(")
proposal_authority_end = collaboration_source.find(
    "\nfn verify_human_decision(", proposal_authority_start
)
if proposal_authority_start < 0 or proposal_authority_end < 0:
    fail("Agent proposal authority implementation is missing")
proposal_authority = collaboration_source[
    proposal_authority_start:proposal_authority_end
]
for forbidden in (
    "AgentRegistration",
    "agent_not_registered",
    "agent_registration_mismatch",
    "hepta_league_state",
    "league.agents",
    "state.inspect",
):
    if forbidden in proposal_authority:
        fail(f"Agent proposal escaped active AgentBinding authority: {forbidden}")
for required in (
    "binding: &AgentBinding",
    "binding.status != AgentBindingStatus::Active",
    "binding.agent_public_key_hash != expected_key_id",
    'decode_record(scope_row.get("binding_json"), "Agent binding")',
    "verify_agent_proposal(paper_id, &request, &manifest.manifest_hash, &binding)",
):
    if required not in proposal_authority:
        fail(f"Agent proposal active-binding authority is incomplete: {required}")

proposal_authority_tests = (
    repo / "services/hepta-research-league/src/paper_raid_v2_tests.rs"
).read_text(encoding="utf-8")
for required in (
    "memory_agent_proposals_trust_only_the_active_secure_binding",
    "postgres_agent_proposals_trust_only_the_active_secure_binding",
    "legacy_league_state_snapshot_bytes",
    "secure Agent bindings must not require the legacy Agent registry",
    "AgentBindingStatus::Revoked",
    '"agent_key_not_current"',
):
    if required not in proposal_authority_tests:
        fail(f"Agent proposal active-binding regression proof is missing: {required}")

paper_raid_source = (
    repo / "services/hepta-research-league/src/paper_raid_v2.rs"
).read_text(encoding="utf-8")
review_source = (
    repo / "services/hepta-research-league/src/paper_review_v4.rs"
).read_text(encoding="utf-8")
contribution_tests = proposal_authority_tests
postgres_reset_start = contribution_tests.find("pub(crate) async fn reset_postgres(")
postgres_reset_end = contribution_tests.find(
    "\nasync fn register_prerequisites(", postgres_reset_start
)
if postgres_reset_start < 0 or postgres_reset_end < 0:
    fail("PostgreSQL test reset boundary is missing")
postgres_reset_source = contribution_tests[postgres_reset_start:postgres_reset_end]
for required in (
    "drop trigger if exists hepta_contribution_ledger_reservation_truncate_guard",
    "drop trigger if exists hepta_paper_contribution_ledger_truncate_guard",
    'include_str!(\n        "../../../migrations/0047_add_hepta_contribution_ledger_authority.sql"',
    "verify_contribution_ledger_authority_catalog(&pool)",
    "verify exact contribution authority catalog after test reset",
):
    if required not in postgres_reset_source:
        fail(f"PostgreSQL test reset does not restore 0047 authority: {required}")
for forbidden in (
    "drop trigger if exists hepta_contribution_ledger_reservation_immutable_guard",
    "drop trigger if exists hepta_paper_contribution_ledger_immutable_guard",
):
    if forbidden in postgres_reset_source:
        fail(f"PostgreSQL test reset weakens row-level contribution authority: {forbidden}")
paper_raid_openapi = (
    repo / "docs/openapi/hepta-paper-raid-v2.yaml"
).read_text(encoding="utf-8")
for required in (
    "pub contribution_ledger_id: Uuid",
    "validate_contribution_ledger_id(request.contribution_ledger_id)",
    "reserve_contribution_ledger_id_memory",
    "reserve_contribution_ledger_id_postgres",
    "hepta_paper_contribution_ledger_reservations",
    "review_v4::authoritative_contribution_entries_memory",
    "review_v4::authoritative_contribution_entries_postgres",
    "review_v4::contribution_ledger_hash",
    '"contribution_ledger_hash_mismatch"',
    "release candidate must bind the complete authoritative contribution ledger",
):
    if required not in paper_raid_source:
        fail(f"release promotion contribution authority is incomplete: {required}")
for required in (
    "authoritative_contribution_entries_from_records",
    "accepted Agent proposal must have exactly one matching human acceptance",
    "from hepta_artifact_manifests where paper_project_id=$1 for share",
    "from hepta_agent_proposals where paper_project_id=$1 for share",
    "from hepta_human_decisions where paper_project_id=$1 for share",
    "from hepta_section_reviews where paper_project_id=$1 for share",
    "relational columns disagree with record_json",
    "require_complete_authoritative_contribution_entries",
    '"contribution_ledger_entries_mismatch"',
    "duplicate_contribution_reference_kind",
    "validate_loaded_contribution_ledger",
    "contribution ledger canonical hash disagrees with frozen relational authority",
    "entries_json,version,created_at,record_json",
):
    if required not in review_source:
        fail(f"authoritative contribution ledger closure is incomplete: {required}")
for required in (
    '"contribution_ledger_id":contribution_ledger_id',
    '"nil_contribution_ledger_id"',
    '"contribution_ledger_hash_mismatch"',
    '"contribution_ledger_entries_mismatch"',
    "promotion must reject a ledger hash that omits authoritative contribution refs",
    "ledger creation must reject an omitted authoritative contribution ref",
    "ledger creation must reject an added non-authoritative contribution ref",
    "ledger creation must reject duplicate contribution refs",
    "runtime readiness must reject a missing contribution ledger parity constraint",
):
    if required not in contribution_tests:
        fail(f"authoritative contribution regression proof is missing: {required}")
for required in (
    "authoritative_257_refs_are_preserved_with_milestone_points",
    "duplicate_and_cross_author_manifest_credit_is_rejected",
    "added_or_cross_author_client_refs_are_rejected",
    "ledger_id_reservation_rejects_nil_and_global_preemption",
    "canonical_loader_rejects_record_json_entry_drift_before_raid_score",
):
    if required not in review_source:
        fail(f"contribution authority unit regression proof is missing: {required}")
for required in (
    "PromoteReleaseCandidateRequest:",
    "- contribution_ledger_id",
    "NonNilUuid:",
    'contribution_ledger_id: {$ref: "#/components/schemas/NonNilUuid"}',
    'not: {const: "00000000-0000-0000-0000-000000000000"}',
    'schema: {$ref: "#/components/schemas/PromoteReleaseCandidateRequest"}',
):
    if required not in paper_raid_openapi:
        fail(f"promotion contribution OpenAPI contract is incomplete: {required}")
contribution_openapi = yaml.safe_load(paper_raid_openapi)["components"]["schemas"]
credit_input = contribution_openapi["CreditContributionInput"]["properties"]
for field in ("accepted_artifact_manifest_ids", "accepted_section_review_ids"):
    if "maxItems" in credit_input[field]:
        fail(f"authoritative contribution facts must not retain a liveness cap: {field}")
    if credit_input[field].get("uniqueItems") is not True:
        fail(f"authoritative contribution references must remain unique: {field}")
if contribution_openapi["PromoteReleaseCandidateRequest"]["properties"][
    "contribution_ledger_id"
] != {"$ref": "#/components/schemas/NonNilUuid"}:
    fail("promotion must use the non-nil contribution-ledger UUID schema")
if contribution_openapi["CreateContributionLedgerRequest"]["properties"][
    "contribution_ledger_id"
] != {"$ref": "#/components/schemas/NonNilUuid"}:
    fail("ledger creation must use the non-nil contribution-ledger UUID schema")

contribution_derivation = review_source[
    review_source.find("fn normalize_uuid_set("):
    review_source.find("pub(super) fn authoritative_contribution_entries_memory(")
]
for forbidden in ("contribution_reference_limit", "len() > 256", "frozen 256-item"):
    if forbidden in contribution_derivation:
        fail(f"contribution derivation retained the irreversible liveness trap: {forbidden}")
for required in (
    "one artifact manifest may have only one accepted proposal globally",
    "artifact_manifest_id=$1 and status='accepted'",
    '"artifact_contribution_duplicated"',
):
    if required not in collaboration_source:
        fail(f"global artifact-credit admission authority is incomplete: {required}")


def stage_blocks(text):
    matches = list(
        re.finditer(
            r"(?mi)^FROM[ \t]+([^\n]+?)(?:[ \t]+AS[ \t]+([a-zA-Z0-9_.-]+))?[ \t]*$",
            text,
        )
    )
    if not matches:
        fail("Dockerfile contains no stages")
    blocks = []
    for index, match in enumerate(matches):
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        blocks.append((match.group(1).strip(), match.group(2), text[match.start():end]))
    return blocks


expected_copy_sources = {
    "services/hepta-research-league/docker/workspace.Cargo.toml",
    "services/hepta-research-league/docker/rust-toolchain.manifest",
    "crates/hepta-paper-raid-contracts/Cargo.toml",
    "crates/hepta-paper-raid-contracts/src",
    "services/hepta-research-league/Cargo.toml",
    "services/hepta-research-league/src",
    "vendor/trnm-finality-types/Cargo.toml",
    "vendor/trnm-finality-types/src",
    "vendor/trnm-finality-verifier/Cargo.toml",
    "vendor/trnm-finality-verifier/src",
    "vendor/trnm-protocol/Cargo.toml",
    "vendor/trnm-protocol/src",
    "vendor/trnm-research-protocol/Cargo.toml",
    "vendor/trnm-research-protocol/src",
    "migrations/0031_add_hepta_research_league.sql",
    "migrations/0032_add_hepta_paper_raid_v2.sql",
    "migrations/0033_add_hepta_paper_collaboration_kernel.sql",
    "migrations/0034_add_hepta_paper_review_appeal.sql",
    "migrations/0035_add_hepta_secure_onboarding.sql",
    "migrations/0036_add_hepta_nakama_research_control.sql",
    "migrations/0037_add_hepta_paper_chain_finality_v1.sql",
    "migrations/0038_add_hepta_paper_chain_finality_v2.sql",
    "migrations/0039_add_hepta_review_assignments.sql",
    "migrations/0040_add_hepta_evaluation_draft_quorum.sql",
    "migrations/0041_add_hepta_agent_capability_disclosure.sql",
    "migrations/0042_add_hepta_team_proposal_deadlines.sql",
    "migrations/0043_add_hepta_challenge_ruleset_v1.sql",
    "migrations/0044_add_hepta_agent_proposal_v2_epoch.sql",
    "migrations/0045_add_hepta_work_item_record_parity.sql",
    "migrations/0046_add_hepta_matchmaking_record_parity_v2.sql",
    "migrations/0047_add_hepta_contribution_ledger_authority.sql",
    "migrations/0048_add_hepta_consumer_finality_v2.sql",
    "migrations/0049_add_hepta_challenge_pack_activation.sql",
    "migrations/0050_add_hepta_paper_rework.sql",
    "migrations/0051_bind_legacy_evaluation_panel_lifecycle.sql",
    "migrations/0052_harden_hepta_paper_finality_v2_preparation_ingress.sql",
    "migrations/0053_allow_review_ready_artifact_manifest_binding.sql",
    "docs/openapi/hepta-research-league-v1.yaml",
    "docs/openapi/hepta-paper-raid-v2.yaml",
}


def validate_dockerfile(text):
    pinned_syntax = "# syntax=docker/dockerfile:1@sha256:87999aa3d42bdc6bea60565083ee17e86d1f3339802f543c0d03998580f9cb89"
    if text.splitlines()[0] != pinned_syntax or text.count("# syntax=") != 1:
        fail("Dockerfile syntax frontend is not uniquely pinned")
    if re.search(r"(?mi)^\s*COPY\s+(?:--[^ \t]+[ \t]+)*\.\s+\.?/?\s*$", text):
        fail("Dockerfile contains an unbounded COPY . instruction")
    if re.search(r"(?mi)^\s*ADD(?:[ \t]|$)", text):
        fail("Dockerfile ADD instructions are forbidden")
    if re.search(r"(?mi)^\s*RUN\s+--mount(?:=|[ \t])", text):
        fail("Dockerfile RUN mounts are forbidden")
    if re.search(r"(?m)\bcargo\s+(?:generate-lockfile|update)\b", text):
        fail("Dockerfile must not resolve against a moving registry index")
    blocks = stage_blocks(text)
    identities = [(base, name) for base, name, _ in blocks]
    expected_identities = [
        (
            "docker.io/library/rust@sha256:4c2fd73ef19c5ef9d54bee03b06b2839a392604fbfcd578ed948b71b37c1d7fb",
            "workspace",
        ),
        ("workspace", "lockfile-verifier"),
        ("scratch", "cargo-lock-export"),
        ("workspace", "builder"),
        ("scratch", "runtime-binary-export"),
        ("scratch", "sbom-metadata-export"),
        ("builder", "release"),
        (
            "gcr.io/distroless/cc-debian12@sha256:471dbca9cad607b9a32c10e9c31fb09ffaeb2d460e0afbff86c27abbc80b1b98",
            None,
        ),
    ]
    if identities != expected_identities:
        fail(f"Dockerfile stage identity/order drifted: {identities!r}")
    _, _, workspace_stage = blocks[0]
    instructions = [
        line.strip()
        for line in workspace_stage.splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    ]
    if any(line.startswith(("ARG ", "LABEL ")) for line in instructions):
        fail("release metadata is visible in the compile stage")
    if any(
        line.startswith("ENV ")
        and re.search(
            r"\b(SOURCE_DATE_EPOCH|RUNTIME_BINARY_SHA256|VCS_REF|SOURCE_TREE|SBOM_SHA256)\b",
            line,
        )
        for line in instructions
    ):
        fail("release metadata environment is visible in the compile stage")
    if any("deploy/hepta-research-league/hepta-research-league.cdx.json" in line for line in instructions):
        fail("tracked SBOM is visible in the compile stage")
    copied = []
    for line in instructions:
        if not line.startswith("COPY "):
            continue
        fields = shlex.split(line)
        if len(fields) != 3 or fields[1].startswith("--"):
            fail(f"compile-stage COPY is not a single explicit source: {line}")
        copied.append(fields[1])
    if Counter(copied) != Counter(expected_copy_sources):
        fail(
            "compile-stage COPY closure drifted: missing="
            f"{sorted(expected_copy_sources - set(copied))!r} "
            f"extra={sorted(set(copied) - expected_copy_sources)!r} duplicates="
            f"{sorted(item for item, count in Counter(copied).items() if count != 1)!r}"
        )
    required_workspace_fragments = (
        'rustc 1.95.0 (59807616e 2026-04-14)',
        'cargo 1.95.0 (f2d3ce0bd 2026-03-21)',
    )
    for fragment in required_workspace_fragments:
        if fragment not in workspace_stage:
            fail(f"workspace-stage authority is missing {fragment!r}")
    lockfile_verifier = blocks[1][2]
    lockfile_export = blocks[2][2]
    builder = blocks[3][2]
    runtime_export = blocks[4][2]
    metadata_export = blocks[5][2]
    release = blocks[6][2]
    final = blocks[7][2]
    lockfile_verifier_instructions = [
        line.strip()
        for line in lockfile_verifier.splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    ]
    if lockfile_verifier_instructions[0] != "FROM workspace AS lockfile-verifier":
        fail("lockfile verifier stage identity drifted")
    lockfile_heads = re.findall(
        r"(?mi)^(FROM|RUN|COPY|ADD|ARG|ENV|WORKDIR|LABEL|USER|ENTRYPOINT|CMD|HEALTHCHECK)\b",
        lockfile_verifier,
    )
    if lockfile_heads != ["FROM", "COPY", "RUN"]:
        fail("lockfile verifier must contain exactly one COPY and one RUN instruction")
    if [
        line
        for line in lockfile_verifier_instructions
        if line.startswith("COPY ")
    ] != ["COPY services/hepta-research-league/docker/Cargo.lock Cargo.lock"]:
        fail("lockfile verifier must consume only the dedicated Docker lock")
    for fragment in (
        "cargo fetch --locked",
        "cargo metadata --locked --offline --format-version 1",
    ):
        if fragment not in lockfile_verifier:
            fail(f"lockfile verifier is missing {fragment!r}")
    if "cargo generate-lockfile" in lockfile_verifier or "cargo update" in lockfile_verifier:
        fail("lockfile verifier must not resolve against a moving registry index")
    expected_lockfile_verifier = r"""FROM workspace AS lockfile-verifier
COPY services/hepta-research-league/docker/Cargo.lock Cargo.lock
RUN CARGO_HTTP_TIMEOUT=600 \
    CARGO_HTTP_LOW_SPEED_LIMIT=1 \
    CARGO_NET_RETRY=5 \
    cargo fetch --locked \
    && cargo metadata --locked --offline --format-version 1 \
      > /cargo-lock-metadata.json"""
    if lockfile_verifier.strip() != expected_lockfile_verifier:
        fail("lockfile verifier command sequence drifted")
    lockfile_export_instructions = [
        line.strip()
        for line in lockfile_export.splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    ]
    if lockfile_export_instructions != [
        "FROM scratch AS cargo-lock-export",
        "COPY --from=lockfile-verifier /src/Cargo.lock /Cargo.lock",
    ]:
        fail("cargo-lock-export must export exactly the verified Docker lock")
    builder_instructions = [
        line.strip()
        for line in builder.splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    ]
    if not builder_instructions or builder_instructions[0] != "FROM workspace AS builder":
        fail("compile builder stage identity drifted")
    if [line for line in builder_instructions if line.startswith("COPY ")] != [
        "COPY services/hepta-research-league/docker/Cargo.lock Cargo.lock"
    ]:
        fail("compile builder must consume only the dedicated Docker lock")
    required_builder_fragments = (
        "cargo fetch --locked",
        "cargo metadata --locked --offline --format-version 1",
        "cargo build --locked --offline --release",
        "-p hepta-research-league --bin hepta-research-league",
        "env -u SOURCE_DATE_EPOCH",
        "-u RUNTIME_BINARY_SHA256",
        "-u VCS_REF",
        "-u SOURCE_TREE",
        "-u SBOM_SHA256",
    )
    for fragment in required_builder_fragments:
        if fragment not in builder:
            fail(f"compile-stage authority is missing {fragment!r}")
    expected_builder = r"""FROM workspace AS builder
COPY services/hepta-research-league/docker/Cargo.lock Cargo.lock
RUN CARGO_HTTP_TIMEOUT=600 \
    CARGO_HTTP_LOW_SPEED_LIMIT=1 \
    CARGO_NET_RETRY=5 \
    cargo fetch --locked
RUN cargo metadata --locked --offline --format-version 1 > /cargo-metadata.json
RUN env -u SOURCE_DATE_EPOCH \
        -u RUNTIME_BINARY_SHA256 \
        -u VCS_REF \
        -u SOURCE_TREE \
        -u SBOM_SHA256 \
      cargo build --locked --offline --release \
        -p hepta-research-league --bin hepta-research-league"""
    if builder.strip() != expected_builder:
        fail("compile builder command sequence drifted")
    runtime_instructions = [
        line.strip()
        for line in runtime_export.splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    ]
    metadata_instructions = [
        line.strip()
        for line in metadata_export.splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    ]
    if runtime_instructions != [
        "FROM scratch AS runtime-binary-export",
        "COPY --from=builder /src/target/release/hepta-research-league /hepta-research-league",
    ]:
        fail("runtime-binary-export must export exactly the builder binary")
    if metadata_instructions != [
        "FROM scratch AS sbom-metadata-export",
        "COPY --from=builder /cargo-metadata.json /cargo-metadata.json",
    ]:
        fail("sbom-metadata-export must export exactly Cargo metadata")
    release_copy_lines = [
        line.strip() for line in release.splitlines() if line.strip().startswith("COPY ")
    ]
    if release_copy_lines != [
        "COPY deploy/hepta-research-league/hepta-research-league.cdx.json /tracked/hepta-research-league.cdx.json"
    ]:
        fail("release assembler COPY authority drifted")
    for fragment in (
        "FROM builder AS release",
        "ARG SOURCE_DATE_EPOCH",
        "ARG RUNTIME_BINARY_SHA256",
        "deploy/hepta-research-league/hepta-research-league.cdx.json",
        "sha256sum --check --strict",
        "/release/usr/local/bin/hepta-research-league",
        "/release/usr/share/doc/hepta-research-league/sbom.cdx.json",
    ):
        if fragment not in release:
            fail(f"release assembler is missing {fragment!r}")
    for fragment in (
        "ARG VCS_REF",
        "ARG SOURCE_TREE",
        "ARG SBOM_SHA256",
        "ARG CARGO_LOCK_SHA256",
        "ARG DOCKERFILE_SHA256",
        "ARG RUST_TOOLCHAIN_SHA256",
        'USER 65532:65532',
        'ENTRYPOINT ["/usr/local/bin/hepta-research-league"]',
    ):
        if fragment not in final:
            fail(f"final image contract is missing {fragment!r}")
    final_copy_lines = [
        line.strip() for line in final.splitlines() if line.strip().startswith("COPY ")
    ]
    if final_copy_lines != ["COPY --from=release /release/ /"]:
        fail("final image COPY authority drifted")


validate_dockerfile(dockerfile)
for mutation, expected in (
    (dockerfile.replace("WORKDIR /src", "WORKDIR /src\nCOPY . .", 1), "unbounded COPY"),
    (
        dockerfile.replace(
            "WORKDIR /src",
            "WORKDIR /src\nARG SOURCE_DATE_EPOCH",
            1,
        ),
        "release metadata",
    ),
    (
        dockerfile.replace(
            "WORKDIR /src",
            "WORKDIR /src\nCOPY deploy/hepta-research-league/hepta-research-league.cdx.json /compile/sbom.json",
            1,
        ),
        "tracked SBOM",
    ),
    (
        dockerfile.replace("WORKDIR /src", "WORKDIR /src\nADD . /src", 1),
        "ADD instructions",
    ),
    (
        dockerfile.replace(
            "WORKDIR /src",
            "WORKDIR /src\nRUN --mount=type=bind,source=.,target=/host true",
            1,
        ),
        "RUN mounts",
    ),
    (
        dockerfile.replace(
            "COPY --from=release /release/ /",
            "COPY --from=release /release/ /\nCOPY --from=builder /src/Cargo.lock /Cargo.lock",
            1,
        ),
        "final image COPY",
    ),
    (
        dockerfile.replace("cargo fetch --locked", "cargo fetch", 1),
        "cargo fetch --locked",
    ),
    (
        dockerfile.replace(
            "cargo fetch --locked \\",
            "cargo fetch --locked || true \\",
            1,
        ),
        "lockfile verifier command sequence",
    ),
    (
        dockerfile.replace(
            "cargo metadata --locked --offline --format-version 1",
            "cargo metadata --locked --format-version 1",
            1,
        ),
        "cargo metadata --locked --offline --format-version 1",
    ),
    (
        dockerfile.replace(
            "cargo metadata --locked --offline --format-version 1",
            "cargo metadata --offline --format-version 1",
            1,
        ),
        "cargo metadata --locked --offline --format-version 1",
    ),
    (
        dockerfile.replace(
            "FROM workspace AS lockfile-verifier\nCOPY services/hepta-research-league/docker/Cargo.lock Cargo.lock",
            "FROM workspace AS lockfile-verifier\nCOPY Cargo.lock Cargo.lock",
            1,
        ),
        "lockfile verifier",
    ),
    (
        dockerfile.replace(
            "cargo fetch --locked \\",
            "cargo fetch --locked \\\n    && cargo generate-lockfile \\",
            1,
        ),
        "moving registry index",
    ),
    (
        dockerfile.replace(
            "FROM workspace AS builder\nCOPY services/hepta-research-league/docker/Cargo.lock Cargo.lock",
            "FROM workspace AS builder\nCOPY Cargo.lock Cargo.lock",
            1,
        ),
        "dedicated Docker lock",
    ),
    (
        dockerfile.replace(
            "RUN cargo metadata --locked --offline --format-version 1 > /cargo-metadata.json",
            "RUN cargo metadata --locked --offline --format-version 1 > /cargo-metadata.json\nRUN true",
            1,
        ),
        "compile builder command sequence",
    ),
    (
        dockerfile.replace(
            "FROM workspace AS builder",
            "FROM workspace AS builder\nRUN cargo update",
            1,
        ),
        "moving registry index",
    ),
):
    try:
        validate_dockerfile(mutation)
    except AssertionError as error:
        if expected not in str(error):
            fail(f"Dockerfile negative mutation failed for the wrong reason: {error}")
    else:
        fail(f"Dockerfile negative mutation was accepted: {expected}")

manifest_path = repo / "services/hepta-research-league/docker/workspace.Cargo.toml"
with manifest_path.open("rb") as stream:
    workspace = tomllib.load(stream)
with (repo / "Cargo.toml").open("rb") as stream:
    canonical_workspace_dependencies = tomllib.load(stream)["workspace"]["dependencies"]
expected_members = [
    "crates/hepta-paper-raid-contracts",
    "services/hepta-research-league",
    "vendor/trnm-finality-types",
    "vendor/trnm-finality-verifier",
    "vendor/trnm-protocol",
    "vendor/trnm-research-protocol",
]
expected_workspace_dependencies = {
    "axum",
    "chrono",
    "hepta-paper-raid-contracts",
    "reqwest",
    "serde",
    "serde_json",
    "sqlx",
    "tokio",
    "tracing",
    "tracing-subscriber",
    "trnm-finality-types",
    "trnm-finality-verifier",
    "trnm-protocol",
    "trnm-research-protocol",
    "uuid",
}


def validate_minimal_workspace(candidate):
    candidate_workspace = candidate.get("workspace", {})
    if candidate_workspace.get("members") != expected_members:
        fail("minimal compile workspace member closure drifted")
    if candidate_workspace.get("resolver") != "2":
        fail("minimal compile workspace resolver drifted")
    if candidate_workspace.get("package") != {
        "edition": "2021",
        "license": "MIT",
        "version": "0.1.0",
        "authors": ["Qi Team"],
    }:
        fail("minimal workspace package metadata differs from the root authority")
    workspace_dependencies = candidate_workspace.get("dependencies")
    if not isinstance(workspace_dependencies, dict) or set(workspace_dependencies) != expected_workspace_dependencies:
        fail("minimal compile workspace dependency set drifted")
    for name in expected_workspace_dependencies:
        if workspace_dependencies[name] != canonical_workspace_dependencies.get(name):
            fail(f"minimal workspace dependency differs from canonical workspace: {name}")
    for name, dependency in workspace_dependencies.items():
        if isinstance(dependency, dict) and "path" in dependency:
            parts = pathlib.PurePosixPath(dependency["path"]).parts
            if not parts or parts[0] not in {"crates", "services", "vendor"} or ".." in parts:
                fail(f"minimal workspace dependency escapes the archive: {name}")
            if "git" in dependency or "rev" in dependency:
                fail(f"minimal workspace dependency mixes path and Git authority: {name}")


validate_minimal_workspace(workspace)
for mutate in (
    lambda value: value["workspace"]["members"].append("services/untrusted"),
    lambda value: value["workspace"]["dependencies"].pop("reqwest"),
    lambda value: value["workspace"]["dependencies"].update({"reqwest": {"version": "9"}}),
    lambda value: value["workspace"]["dependencies"]["hepta-paper-raid-contracts"].update(
        {"path": "../outside"}
    ),
):
    mutation = copy.deepcopy(workspace)
    mutate(mutation)
    try:
        validate_minimal_workspace(mutation)
    except AssertionError:
        pass
    else:
        fail("minimal workspace negative mutation was accepted")

docker_lock_path = repo / "services/hepta-research-league/docker/Cargo.lock"
if docker_lock_path.is_symlink() or not docker_lock_path.is_file():
    fail("dedicated Docker lock is missing or non-regular")
with docker_lock_path.open("rb") as stream:
    docker_lock = tomllib.load(stream)


def validate_docker_lock(candidate):
    if not isinstance(candidate, dict) or set(candidate) != {"version", "package"}:
        fail("dedicated Docker lock top-level shape drifted")
    if candidate.get("version") != 4:
        fail("dedicated Docker lock version drifted")
    packages = candidate.get("package")
    if not isinstance(packages, list) or not packages:
        fail("dedicated Docker lock package closure is empty")
    identities = []
    local = []
    for package in packages:
        if not isinstance(package, dict) or not {"name", "version"}.issubset(package):
            fail("dedicated Docker lock package shape is invalid")
        if not set(package).issubset({"name", "version", "source", "checksum", "dependencies"}):
            fail("dedicated Docker lock package contains an unknown field")
        name = package["name"]
        version = package["version"]
        source = package.get("source")
        if not isinstance(name, str) or not name or not isinstance(version, str) or not version:
            fail("dedicated Docker lock package identity is invalid")
        if source is None:
            if "checksum" in package:
                fail("local Docker-lock package unexpectedly has a checksum")
            local.append((name, version))
        else:
            if source != "registry+https://github.com/rust-lang/crates.io-index":
                fail("dedicated Docker lock contains a non-crates.io dependency")
            if not re.fullmatch(r"[0-9a-f]{64}", package.get("checksum", "")):
                fail("dedicated Docker lock registry checksum is not canonical")
        dependencies = package.get("dependencies", [])
        if not isinstance(dependencies, list) or not all(
            isinstance(dependency, str) and dependency for dependency in dependencies
        ):
            fail("dedicated Docker lock dependency list is invalid")
        identities.append((name, version, source))
    if len(identities) != len(set(identities)):
        fail("dedicated Docker lock contains duplicate package identities")
    expected_local = {
        ("hepta-paper-raid-contracts", "0.1.0"),
        ("hepta-research-league", "0.1.0"),
        ("trnm-finality-types", "0.1.0"),
        ("trnm-finality-verifier", "0.1.0"),
        ("trnm-protocol", "0.1.0"),
        ("trnm-research-protocol", "0.1.0"),
    }
    if set(local) != expected_local or len(local) != len(expected_local):
        fail("dedicated Docker lock local package closure drifted")


validate_docker_lock(docker_lock)
for mutate in (
    lambda value: value.update({"version": 3}),
    lambda value: value["package"].append(copy.deepcopy(value["package"][0])),
    lambda value: value["package"][0].update(
        {"source": "git+https://example.invalid/forbidden"}
    ),
    lambda value: value["package"].__setitem__(
        slice(None),
        [package for package in value["package"] if package["name"] != "hepta-research-league"],
    ),
):
    mutation = copy.deepcopy(docker_lock)
    mutate(mutation)
    try:
        validate_docker_lock(mutation)
    except AssertionError:
        pass
    else:
        fail("dedicated Docker lock negative mutation was accepted")

toolchain_lines = (
    repo / "services/hepta-research-league/docker/rust-toolchain.manifest"
).read_text(encoding="utf-8").splitlines()
if toolchain_lines != [
    "builder=docker.io/library/rust@sha256:4c2fd73ef19c5ef9d54bee03b06b2839a392604fbfcd578ed948b71b37c1d7fb",
    "rustc=rustc 1.95.0 (59807616e 2026-04-14)",
    "cargo=cargo 1.95.0 (f2d3ce0bd 2026-03-21)",
]:
    fail("Rust toolchain manifest drifted")


def require_fragments(relative_path, fragments):
    text = (repo / relative_path).read_text(encoding="utf-8")
    for fragment in fragments:
        if fragment not in text:
            fail(f"{relative_path} is missing release invariant {fragment!r}")
    return text


clean_source_helper_path = repo / "scripts/verify-hepta-clean-source.py"
if (
    clean_source_helper_path.is_symlink()
    or not clean_source_helper_path.is_file()
    or stat.S_IMODE(clean_source_helper_path.stat().st_mode) != 0o755
):
    fail("clean-source helper must be one executable, non-symlink regular file")
clean_source_helper = clean_source_helper_path.read_text(encoding="utf-8")


def validate_clean_source_helper(text):
    try:
        compile(text, "verify-hepta-clean-source.py", "exec")
    except SyntaxError as error:
        fail(f"clean-source helper does not compile: {error}")
    if hashlib.sha256(text.encode("utf-8")).hexdigest() != (
        "995b9fccd290f7e4411078c268a605ea2dd3659a656b65e18fbf63c3d6f57560"
    ):
        fail("clean-source helper authority drifted")
    for fragment in (
        'argparse.ArgumentParser(allow_abbrev=False)',
        'shutil.which("git", path=os.defpath)',
        'b"GIT_NO_REPLACE_OBJECTS": b"1"',
        'b"--no-replace-objects"',
        'b"core.fsmonitor=false"',
        'b"core.untrackedCache=false"',
        'env=git_environment',
        'git_output(b"ls-tree", b"-r", b"-z", b"--full-tree", expected_revision)',
        'git_output(b"ls-files", b"--stage", b"-z")',
        'stage != b"0"',
        'mode not in {b"100644", b"100755"}',
        'commit_entries.get(path) != (mode, object_id)',
        'git_output(b"ls-files", b"-v", b"-z", b"--cached")',
        'flag != b"H"',
        'index_entries != commit_entries',
        'index_flag_paths != set(index_entries)',
        'b".cargo/config.toml"',
        'b"rust-toolchain.toml"',
        'os.path.realpath(worktree_path) != worktree_path',
        'os.stat(worktree_path, follow_symlinks=False)',
        'stat.S_ISREG(before.st_mode)',
        'before.st_uid != os.geteuid() or before.st_nlink != 1',
        'stat.S_IMODE(before.st_mode) & 0o111',
        'actual_executable != expected_executable',
        'b"hash-object"',
        'b"--no-filters"',
        'actual_object_id != expected_object_id',
        'after_identity != before_identity',
        'b"--untracked-files=all"',
        'b"--ignore-submodules=none"',
        'verify_identity_and_status()',
        '"tracked_files": len(commit_entries)',
        'sort_keys=True',
        'separators=(",", ":")',
    ):
        if fragment not in text:
            fail(f"clean-source helper is missing {fragment!r}")
    if text.count('b"--no-filters"') != 1:
        fail("clean-source helper raw-byte hashing authority is not unique")
    if 'b"--path="' in text:
        fail("clean-source helper must not permit Git clean filters to mask raw-byte drift")
    if text.count("verify_identity_and_status()") != 3:
        fail("clean-source helper must verify identity/status exactly before and after hashing")


validate_clean_source_helper(clean_source_helper)
clean_source_helper_mutations = {
    "commit tree authority removed": clean_source_helper.replace(
        'git_output(b"ls-tree", b"-r", b"-z", b"--full-tree", expected_revision)',
        'git_output(b"ls-files", b"--stage", b"-z")',
        1,
    ),
    "non-zero index stages accepted": clean_source_helper.replace(
        'stage != b"0" or mode not in {b"100644", b"100755"}',
        'mode not in {b"100644", b"100755"}',
        1,
    ),
    "index blob detached from commit": clean_source_helper.replace(
        'commit_entries.get(path) != (mode, object_id)',
        '(mode, object_id) != (mode, object_id)',
        1,
    ),
    "assume-unchanged accepted": clean_source_helper.replace(
        'flag != b"H"',
        'flag.lower() != b"h"',
        1,
    ),
    "non-regular worktree accepted": clean_source_helper.replace(
        'if not stat.S_ISREG(before.st_mode):',
        'if False:',
        1,
    ),
    "executable mode ignored": clean_source_helper.replace(
        'if actual_executable != expected_executable:',
        'if False:',
        1,
    ),
    "Git clean filters allowed": clean_source_helper.replace(
        'b"--no-filters",',
        'b"--path=" + path,',
        1,
    ),
    "worktree blob detached from commit": clean_source_helper.replace(
        'if actual_object_id != expected_object_id:',
        'if False:',
        1,
    ),
    "replace objects enabled": clean_source_helper.replace(
        'b"--no-replace-objects",',
        '',
        1,
    ),
    "fsmonitor config enabled": clean_source_helper.replace(
        'b"core.fsmonitor=false",',
        'b"core.fsmonitor=true",',
        1,
    ),
    "untracked cache config enabled": clean_source_helper.replace(
        'b"core.untrackedCache=false",',
        'b"core.untrackedCache=true",',
        1,
    ),
    "inherited Git environment accepted": clean_source_helper.replace(
        'env=git_environment,',
        'env=os.environ,',
        1,
    ),
    "untracked toolchain authority accepted": clean_source_helper.replace(
        'if authority_path not in commit_entries and os.path.lexists(',
        'if False and os.path.lexists(',
        1,
    ),
    "ancestor symlink accepted": clean_source_helper.replace(
        'if os.path.realpath(worktree_path) != worktree_path:',
        'if False:',
        1,
    ),
    "unsafe owner or hardlink accepted": clean_source_helper.replace(
        'if before.st_uid != os.geteuid() or before.st_nlink != 1:',
        'if False:',
        1,
    ),
    "tracked-file hash race ignored": clean_source_helper.replace(
        'if after_identity != before_identity:',
        'if False:',
        1,
    ),
}
for mutation_name, mutation in clean_source_helper_mutations.items():
    if mutation == clean_source_helper:
        fail(f"clean-source helper negative mutation was not applied: {mutation_name}")
    try:
        validate_clean_source_helper(mutation)
    except AssertionError:
        pass
    else:
        fail(f"clean-source helper negative mutation was accepted: {mutation_name}")


def fixture_git(fixture, *arguments):
    result = subprocess.run(
        ["git", "-C", str(fixture), *arguments],
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        fail(f"clean-source helper fixture Git command failed: {arguments!r}")
    return result.stdout


def run_clean_source_helper(fixture, revision, tree, environment=None):
    return subprocess.run(
        [
            sys.executable,
            str(clean_source_helper_path),
            "--repo-dir",
            str(fixture),
            "--revision",
            revision,
            "--tree",
            tree,
        ],
        capture_output=True,
        check=False,
        timeout=15,
        env=environment,
    )


source_fixture = scratch / "clean-source-helper-fixture"
source_fixture.mkdir(mode=0o700)
fixture_git(source_fixture, "init", "-q")
fixture_git(source_fixture, "config", "user.name", "Hepta verifier fixture")
fixture_git(source_fixture, "config", "user.email", "hepta-verifier@example.invalid")
(source_fixture / ".gitattributes").write_text(
    "filtered.txt filter=mask\n", encoding="utf-8"
)
(source_fixture / "source.txt").write_text("source authority\n", encoding="utf-8")
(source_fixture / "filtered.txt").write_text("canonical\n", encoding="utf-8")
(source_fixture / "runner").write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
(source_fixture / "runner").chmod(0o755)
fixture_git(source_fixture, "add", ".gitattributes", "source.txt", "filtered.txt", "runner")
fixture_git(source_fixture, "commit", "-qm", "source verifier fixture")
fixture_revision = fixture_git(source_fixture, "rev-parse", "HEAD").decode().strip()
fixture_tree = fixture_git(source_fixture, "rev-parse", "HEAD^{tree}").decode().strip()
fixture_result = run_clean_source_helper(
    source_fixture, fixture_revision, fixture_tree
)
if fixture_result.returncode != 0:
    fail("clean-source helper rejected a canonical clean fixture")
try:
    fixture_summary = json.loads(fixture_result.stdout)
except (UnicodeDecodeError, json.JSONDecodeError):
    fail("clean-source helper did not emit canonical JSON")
if fixture_summary != {
    "revision": fixture_revision,
    "tree": fixture_tree,
    "tracked_files": 4,
}:
    fail("clean-source helper summary identity drifted")
if fixture_result.stdout != (
    json.dumps(
        fixture_summary, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")
    + b"\n"
):
    fail("clean-source helper summary is not canonical JSON")

hostile_git_environment = dict(os.environ)
hostile_git_environment.update(
    {
        "GIT_DIR": "/nonexistent/hostile-git-dir",
        "GIT_INDEX_FILE": "/nonexistent/hostile-index",
        "GIT_OBJECT_DIRECTORY": "/nonexistent/hostile-objects",
        "GIT_REPLACE_REF_BASE": "refs/hostile-replacements/",
        "GIT_CONFIG_COUNT": "1",
        "GIT_CONFIG_KEY_0": "core.bare",
        "GIT_CONFIG_VALUE_0": "true",
    }
)
if run_clean_source_helper(
    source_fixture,
    fixture_revision,
    fixture_tree,
    hostile_git_environment,
).returncode != 0:
    fail("clean-source helper inherited hostile Git environment authority")

fixture_git(source_fixture, "update-index", "--assume-unchanged", "source.txt")
if run_clean_source_helper(source_fixture, fixture_revision, fixture_tree).returncode == 0:
    fail("clean-source helper accepted assume-unchanged")
fixture_git(source_fixture, "update-index", "--no-assume-unchanged", "source.txt")
fixture_git(source_fixture, "update-index", "--skip-worktree", "source.txt")
if run_clean_source_helper(source_fixture, fixture_revision, fixture_tree).returncode == 0:
    fail("clean-source helper accepted skip-worktree")
fixture_git(source_fixture, "update-index", "--no-skip-worktree", "source.txt")

hardlink_peer = scratch / "clean-source-hardlink-peer"
os.link(source_fixture / "source.txt", hardlink_peer)
if run_clean_source_helper(source_fixture, fixture_revision, fixture_tree).returncode == 0:
    fail("clean-source helper accepted an externally mutable tracked hardlink")
hardlink_peer.unlink()

info_exclude = source_fixture / ".git/info/exclude"
info_exclude.write_text(
    info_exclude.read_text(encoding="utf-8") + "\n.cargo/\n",
    encoding="utf-8",
)
(source_fixture / ".cargo").mkdir()
(source_fixture / ".cargo/config.toml").write_text(
    '[build]\nrustflags = ["--cfg", "untracked_authority"]\n',
    encoding="utf-8",
)
if fixture_git(
    source_fixture,
    "status",
    "--porcelain=v1",
    "-z",
    "--untracked-files=all",
):
    fail("untracked Cargo-authority fixture is not ignored")
if run_clean_source_helper(source_fixture, fixture_revision, fixture_tree).returncode == 0:
    fail("clean-source helper accepted an ignored Cargo authority")
(source_fixture / ".cargo/config.toml").unlink()
(source_fixture / ".cargo").rmdir()

fixture_git(source_fixture, "config", "filter.mask.clean", "printf 'canonical\\n'")
fixture_git(source_fixture, "config", "filter.mask.required", "true")
(source_fixture / "filtered.txt").write_text("tampered!\n", encoding="utf-8")
if fixture_git(
    source_fixture,
    "hash-object",
    "--path=filtered.txt",
    "--",
    "filtered.txt",
).strip() != fixture_git(source_fixture, "rev-parse", "HEAD:filtered.txt").strip():
    fail("clean-source helper filter-masking fixture is invalid")
if fixture_git(
    source_fixture,
    "status",
    "--porcelain=v1",
    "-z",
    "--untracked-files=all",
):
    fail("clean-source helper filter-masking fixture did not hide from porcelain")
filter_result = run_clean_source_helper(
    source_fixture, fixture_revision, fixture_tree
)
if filter_result.returncode == 0 or b"bytes differ from HEAD" not in filter_result.stderr:
    fail("clean-source helper allowed a clean filter to mask raw-byte drift")


def validate_release_source_identity_gate(text):
    database_marker = (
        ': "${HEPTA_TEST_DATABASE_URL:?HEPTA_TEST_DATABASE_URL is required for the live PostgreSQL release gate}"\n'
    )
    if text.count(database_marker) != 1:
        fail("release gate database boundary is missing or ambiguous")
    boundary_end = text.index(database_marker) + len(database_marker)
    source_boundary = text[:boundary_end]
    if hashlib.sha256(source_boundary.encode("utf-8")).hexdigest() != (
        "57a79758f89933eb48880dc79a6615deffd175bbeab83507cdb89f5d485fc9b0"
    ):
        fail("release gate initial clean-source authority drifted")
    for fragment in (
        'git_binary="$(PATH=/usr/bin:/bin command -v git)"',
        "git_authority() {",
        "env -i",
        "GIT_CONFIG_NOSYSTEM=1",
        "GIT_CONFIG_GLOBAL=/dev/null",
        "GIT_NO_REPLACE_OBJECTS=1",
        '"$git_binary" --no-replace-objects \\',
        '-c core.fsmonitor=false',
        '-c core.untrackedCache=false',
        'release_lock="$(git_authority -C "$repo_dir" rev-parse --path-format=absolute',
        '--git-path hepta-release-authority.lock)',
        'exec 9>"$release_lock"',
        'flock -n 9',
        'release_revision="$(git_authority -C "$repo_dir" rev-parse --verify HEAD^{commit})"',
        'release_tree="$(git_authority -C "$repo_dir" rev-parse --verify "$release_revision^{tree}")"',
        'observed_status="$(git_authority -C "$repo_dir" status --porcelain=v1 --untracked-files=all)"',
        'helper_entry="$(git_authority -C "$repo_dir" ls-tree "$release_revision" -- "$helper_relative_path")"',
        '"$helper_mode" != "100755"',
        '"$helper_object_type" != "blob"',
        'actual_helper_blob="$(git_authority -C "$repo_dir" hash-object --no-filters -- "$helper_relative_path")"',
        '"$actual_helper_blob" != "$expected_helper_blob"',
        'python3 "$helper_path"',
        '--revision "$release_revision"',
        '--tree "$release_tree"',
        ">/dev/null",
    ):
        if fragment not in source_boundary:
            fail(f"release gate clean-source boundary is missing {fragment!r}")
    expected_end = r'''cargo_locked fmt --all -- --check
cargo_locked test --locked -p trnm-finality-verifier --lib
cargo_locked test --locked -p hepta-research-league
cargo_locked check --locked --workspace
cargo_locked clippy --locked --workspace --all-targets -- -D warnings
verify_release_source_unchanged'''
    if not text.rstrip().endswith(expected_end):
        fail("release gate does not end by re-verifying the exact clean source identity")
    if text.count("verify_release_source_unchanged() {") != 1:
        fail("release gate clean source identity verifier definition is not unique")
    if text.count("\nverify_release_source_unchanged\n") != 2:
        fail("release gate must invoke the clean source verifier exactly at start and end")
    if text.count('python3 "$helper_path"') != 1:
        fail("release gate shared clean-source helper invocation is not unique")
    if text.count("git status --porcelain=v1 --untracked-files=all"):
        fail("release gate must not use inherited Git authority")



release_script = (repo / "scripts/check-hepta-research-league-release.sh").read_text(
    encoding="utf-8"
)
validate_release_source_identity_gate(release_script)
release_gate_mutations = {
    "initial check removed": release_script.replace(
        "verify_release_source_unchanged\n\n: \"${HEPTA_TEST_DATABASE_URL",
        "true\n\n: \"${HEPTA_TEST_DATABASE_URL",
        1,
    ),
    "untracked files ignored": release_script.replace(
        "git_authority -C \"$repo_dir\" status --porcelain=v1 --untracked-files=all",
        "git_authority -C \"$repo_dir\" status --porcelain=v1 --untracked-files=no",
        1,
    ),
    "HEAD comparison weakened": release_script.replace(
        '[[ "$observed_revision" != "$release_revision" ]]',
        '[[ "$observed_revision" != "$observed_revision" ]]',
        1,
    ),
    "tree comparison weakened": release_script.replace(
        '[[ "$observed_tree" != "$release_tree" ]]',
        '[[ "$observed_tree" != "$observed_tree" ]]',
        1,
    ),
    "dirty check ignored": release_script.replace(
        'if [[ -n "$observed_status" ]]; then',
        'if [[ -z "$observed_status" ]]; then',
        1,
    ),
    "shared helper removed": release_script.replace(
        'python3 "$helper_path"',
        'python3 -c "raise SystemExit(0)"',
        1,
    ),
    "shared helper failure ignored": release_script.replace(
        '    >/dev/null',
        '    >/dev/null || true',
        1,
    ),
    "shared helper revision detached": release_script.replace(
        '    --revision "$release_revision" \\',
        '    --revision "$(git rev-parse HEAD)" \\',
        1,
    ),
    "shared helper tree detached": release_script.replace(
        '    --tree "$release_tree" \\',
        '    --tree "$(git rev-parse HEAD^{tree})" \\',
        1,
    ),
    "inherited Git environment restored": release_script.replace(
        "  env -i \\",
        "  env \\",
        1,
    ),
    "replace objects restored": release_script.replace(
        '    "$git_binary" --no-replace-objects \\',
        '    "$git_binary" \\',
        1,
    ),
    "release authority lock removed": release_script.replace(
        'flock -n 9 || {',
        'true || {',
        1,
    ),
    "helper commit authority removed": release_script.replace(
        '  helper_entry="$(git_authority -C "$repo_dir" ls-tree "$release_revision" -- "$helper_relative_path")"',
        '  helper_entry=""',
        1,
    ),
    "helper executable mode weakened": release_script.replace(
        '"$helper_mode" != "100755"',
        '"$helper_mode" != "100644"',
        1,
    ),
    "helper raw hash filtered": release_script.replace(
        'hash-object --no-filters -- "$helper_relative_path"',
        'hash-object --path="$helper_relative_path" -- "$helper_relative_path"',
        1,
    ),
    "helper blob comparison ignored": release_script.replace(
        'if [[ "$actual_helper_blob" != "$expected_helper_blob" ]]; then',
        'if false; then',
        1,
    ),
    "vendored verifier tests removed": release_script.replace(
        "cargo_locked test --locked -p trnm-finality-verifier --lib\n",
        "",
        1,
    ),
    "final check removed": release_script.rsplit(
        "verify_release_source_unchanged", 1
    )[0]
    + "true\n",
    "final check failure ignored": release_script.rsplit(
        "verify_release_source_unchanged", 1
    )[0]
    + "verify_release_source_unchanged || true\n",
}
for mutation_name, mutation in release_gate_mutations.items():
    if mutation == release_script:
        fail(f"release source identity negative mutation was not applied: {mutation_name}")
    try:
        validate_release_source_identity_gate(mutation)
    except AssertionError:
        pass
    else:
        fail(f"release source identity negative mutation was accepted: {mutation_name}")



runtime_script = require_fragments(
    "scripts/generate-hepta-research-league-runtime-sbom.sh",
    (
        'git -C "$repo_dir" archive "$revision"',
        "verify_source_unchanged",
        'build_export runtime-binary-export "$scratch/first"',
        'build_export runtime-binary-export "$scratch/second"',
        "--no-cache",
        "--pull=false",
        'cmp -s "$first_binary" "$second_binary"',
        "build_export sbom-metadata-export",
        "--runtime-binary",
        'cmp "$tracked_sbom" "$scratch/first.cdx.json"',
        'sudo -n chown -R -- "$(id -u):$(id -g)" "$destination"',
        "hepta-release-authority.lock",
        "write_status=",
        "services/hepta-research-league/docker/Cargo.lock",
    ),
)
if runtime_script.count("verify_source_unchanged") < 5:
    fail("runtime SBOM generation lacks repeated TOCTOU checks")

lock_script = require_fragments(
    "scripts/generate-hepta-research-league-docker-lock.sh",
    (
        'git -C "$repo_dir" archive "$revision"',
        "verify_source_unchanged",
        "hepta-release-authority.lock",
        "--target cargo-lock-export",
        'build_export "$scratch/first"',
        'build_export "$scratch/second"',
        "--no-cache",
        "--pull=false",
        'cmp "$first_lock" "$second_lock"',
        'sudo -n chown -R -- "$(id -u):$(id -g)" "$destination"',
        "expected_local",
        '*) echo "usage: $0 --check" >&2; exit 2 ;;',
        'cmp "$tracked_lock" "$first_lock"',
    ),
)
if lock_script.count("verify_source_unchanged") < 5:
    fail("Docker-lock verification lacks repeated TOCTOU checks")
if "--write" in lock_script:
    fail("ordinary Docker-lock verification must not expose an online write mode")

image_script = require_fragments(
    "scripts/build-hepta-research-league-image.sh",
    (
        'git archive "$revision"',
        "verify_source_unchanged",
        "HEPTA_IMAGE_SENTINEL_DO_NOT_SHIP_7e07109a",
        'build_image "$image_ref"',
        'build_image "$repro_ref"',
        "--no-cache",
        "independent no-cache Hepta image builds differ",
        "Config.Env",
        "history --no-trunc",
        'scan_image "$image_id" first',
        'scan_image "$repro_image_id" second',
        "first|second|sentinel-negative)",
        '[[ -f "$rootfs_tar" && ! -L "$rootfs_tar" ]]',
        'sudo -n chown -- "$(id -u):$(id -g)" "$rootfs_tar"',
        '[[ -O "$rootfs_tar" ]]',
        'verify-hepta-research-league-rootfs-tar.py',
        "rootfs contains forbidden build or credential paths",
        "--target sbom-metadata-export",
        'sudo -n chown -R -- "$(id -u):$(id -g)" "$release_dir/sbom-metadata"',
        "regenerated.cdx.json",
        "containerimage.digest",
        "sentinel_scan_status",
        "gate_succeeded=true",
        "original_image_id",
        "check-hepta-research-league-compose-smoke.sh",
        "services/hepta-research-league/docker/Cargo.lock",
    ),
)
if image_script.count("verify_source_unchanged") < 5:
    fail("image gate lacks repeated TOCTOU checks")


def validate_image_rootfs_export_ownership(text):
    exact_chown = 'sudo -n chown -- "$(id -u):$(id -g)" "$rootfs_tar"'
    if text.count(exact_chown) != 1:
        fail("image rootfs export must have one exact non-recursive ownership repair")
    if re.search(r"chown[ \t]+-R[^\n]*(rootfs|\$scan)", text):
        fail("image rootfs export ownership repair must not be recursive or broad")
    if text.count('[[ -f "$rootfs_tar" && ! -L "$rootfs_tar" ]]') != 1:
        fail("image rootfs export must be validated before ownership repair")
    if text.count("first|second|sentinel-negative)") != 1:
        fail("image rootfs scanner labels are not a closed set")


validate_image_rootfs_export_ownership(image_script)
unsafe_image_script = image_script.replace(
    'sudo -n chown -- "$(id -u):$(id -g)" "$rootfs_tar"',
    'sudo -n chown -R -- "$(id -u):$(id -g)" "$scan"',
    1,
)
try:
    validate_image_rootfs_export_ownership(unsafe_image_script)
except AssertionError:
    pass
else:
    fail("recursive image rootfs ownership negative mutation was accepted")


rootfs_tar_verifier = require_fragments(
    "scripts/verify-hepta-research-league-rootfs-tar.py",
    (
        'ALLOWED_SOURCE_DIRECTORY = "usr/src"',
        'member.name != ALLOWED_SOURCE_DIRECTORY',
        'parts != ("usr", "src")',
        'len(source_entries) != 1',
        'not source_entry.isdir()',
        'source_entry.issym()',
        'source_entry.islnk()',
    ),
)


def validate_rootfs_tar_verifier_contract(text):
    if text.count('ALLOWED_SOURCE_DIRECTORY = "usr/src"') != 1:
        fail("rootfs tar verifier source-directory allowlist is not exact")
    if text.count('parts != ("usr", "src")') != 1:
        fail("rootfs tar verifier does not reject other src path segments")
    if text.count('len(source_entries) != 1') != 1:
        fail("rootfs tar verifier does not require one exact usr/src entry")


validate_rootfs_tar_verifier_contract(rootfs_tar_verifier)
unsafe_rootfs_tar_verifier = rootfs_tar_verifier.replace(
    'parts != ("usr", "src")',
    "False",
    1,
)
try:
    validate_rootfs_tar_verifier_contract(unsafe_rootfs_tar_verifier)
except AssertionError:
    pass
else:
    fail("broadened rootfs src allowlist static mutation was accepted")


def write_rootfs_tar_fixture(path, entries):
    with tarfile.open(path, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        for name, kind, linkname in entries:
            member = tarfile.TarInfo(name)
            member.mtime = 0
            member.uid = 0
            member.gid = 0
            member.uname = ""
            member.gname = ""
            member.size = 0
            if kind == "directory":
                member.type = tarfile.DIRTYPE
                member.mode = 0o755
            elif kind == "file":
                member.type = tarfile.REGTYPE
                member.mode = 0o644
            elif kind == "symlink":
                member.type = tarfile.SYMTYPE
                member.linkname = linkname
                member.mode = 0o777
            elif kind == "hardlink":
                member.type = tarfile.LNKTYPE
                member.linkname = linkname
                member.mode = 0o755
            else:
                fail(f"unknown rootfs tar fixture kind: {kind}")
            archive.addfile(member)


rootfs_tar_verifier_path = repo / "scripts/verify-hepta-research-league-rootfs-tar.py"
positive_rootfs_entries = [
    ("usr", "directory", ""),
    ("usr/src", "directory", ""),
    ("usr/bin", "directory", ""),
    ("usr/bin/hepta-research-league", "file", ""),
]
positive_rootfs_tar = scratch / "rootfs-src-positive.tar"
write_rootfs_tar_fixture(positive_rootfs_tar, positive_rootfs_entries)
positive_result = subprocess.run(
    [sys.executable, str(rootfs_tar_verifier_path), "--tar", str(positive_rootfs_tar)],
    capture_output=True,
    check=False,
)
if positive_result.returncode != 0:
    fail("exact empty usr/src directory fixture was rejected")

negative_rootfs_fixtures = {
    "missing": [("usr", "directory", "")],
    "root-src": positive_rootfs_entries + [("src", "directory", "")],
    "other-src": positive_rootfs_entries + [("opt/app/src", "directory", "")],
    "usr-src-descendant": positive_rootfs_entries + [("usr/src/main.rs", "file", "")],
    "usr-src-file": [("usr", "directory", ""), ("usr/src", "file", "")],
    "usr-src-symlink": [("usr", "directory", ""), ("usr/src", "symlink", "tmp")],
    "usr-src-hardlink": [("usr", "directory", ""), ("usr/src", "hardlink", "usr")],
    "usr-src-duplicate": positive_rootfs_entries + [("usr/src", "directory", "")],
}
for fixture_name, fixture_entries in negative_rootfs_fixtures.items():
    fixture_path = scratch / f"rootfs-src-{fixture_name}.tar"
    write_rootfs_tar_fixture(fixture_path, fixture_entries)
    result = subprocess.run(
        [sys.executable, str(rootfs_tar_verifier_path), "--tar", str(fixture_path)],
        capture_output=True,
        check=False,
    )
    if result.returncode == 0:
        fail(f"rootfs src negative fixture was accepted: {fixture_name}")

require_fragments(
    "docs/openapi/hepta-paper-raid-v2.yaml",
    (
        "HEPTA_TRNM_RECEIPT_V2_MAX_BODY_BYTES",
        "32 KiB by default",
        "never above Hepta's 1 MiB deployment ceiling",
        "Authenticated Receipt V2 verification capacity is busy; body was not read",
        "enum: [claimed, pinned, consumed, expired]",
        "Pinned seats remain authoritative past the original",
        "all three panel assignments are consumed",
        'schema: {$ref: "#/components/schemas/PaperRoomReadModelV1"}',
        "RoleResourceActionAvailabilityV1:",
        "RoleResourceProjectionV1:",
        "AuthorRaidProgressV1:",
        "PaperRoomReadModelV1:",
    ),
)
require_fragments(
    "services/hepta-research-league/README.md",
    (
        "fixed 384 MiB policy ceiling",
        "paper-room cursor sequence",
        "not be group/world writable",
        "removes every Compose container/network/volume",
        "O_NOFOLLOW|O_NONBLOCK",
        "dirfd-relative atomic",
    ),
)

require_fragments(
    "services/hepta-research-league/README.md",
    (
        "257 or more accepted artifact/review IDs remain in the canonical ledger",
        "partial unique index for the concurrent race",
        "Every evaluation loader re-requires the exact reservation triple",
        "Paper cannot preempt the ID before later ledger creation",
    ),
)
require_fragments(
    "docs/adr/ADR-006-hepta-paper-collaboration-v3.md",
    (
        "### 8. Contribution authority and liveness",
        "there is no\n256-reference ceiling",
        "The\n257th fact",
        "partial unique index on accepted proposals",
        "loader checks relational parity",
    ),
)
require_fragments(
    "docs/hepta-paper-raid-operations-v1.md",
    (
        "0047 contribution-authority catalog",
        "Never repair contribution ownership by deleting references",
        "non-nil ledger ID retains its exact Paper/release reservation",
        "fail-closed before RaidScore is derived",
    ),
)

require_fragments(
    "scripts/check-hepta-research-league-compose-smoke.sh",
    (
        "HEPTA_EXPECTED_IMAGE_ID",
        "postgres:17.6-alpine3.22@sha256:ef257d85f76e48da1c64832459b59fcaba1a4dac97bf5d7450c77753542eee94",
        "pull_policy: never",
        'kill -s SIGKILL hepta',
        'kill -s SIGKILL postgres',
        "state_rows_before",
        "state_rows_after",
        "content_type_count",
        '.finality_mode == "verified"',
        ".trusted_validator_sets == 0",
        ".pinned_cometbft_trust_anchor_hashes == 2",
        "HEPTA_TRNM_COMETBFT_TRUST_ANCHOR_HASHES_JSON",
        "configured_receipt_cap",
        "configured_receipt_in_flight",
        ".trnm_receipt_v2_max_body_bytes == 32768",
        ".trnm_receipt_v2_max_in_flight == 1",
        '.paper_chain_finality_v2_command_lane == "awaiting_chain_verifier_upgrade"',
        '.paper_scientific_finality_policy == "hepta.paper_raid.scientific_finality_policy.v1"',
        ".paper_no_appeal_window_seconds == 86400",
        "hepta_trnm_cometbft_time_checkpoints_v1",
        "hepta_paper_chain_finality_window_arms_v2",
        "hepta_paper_chain_finality_preparations_v2",
        "verified_v2_trigger_count",
        "verified_v2_constraint_catalog",
        "hepta_paper_finality_v2_constraint_catalog_fingerprint",
        "t.tgfoid = to_regprocedure(e.function_name)",
        "t.tgtype = e.trigger_type",
        "t.tgenabled = 'A'",
        "hepta_paper_evaluations_finality_v2_source_guard",
    ),
)

require_fragments(
    "migrations/0038_add_hepta_paper_chain_finality_v2.sql",
    (
        "hepta_paper_finality_v2_preparation_guard",
        "hepta_paper_finality_v2_preparation_seal_guard",
        "hepta_paper_chain_finality_window_arms_v2",
        "hepta_trnm_cometbft_time_checkpoints_v1",
        "hepta_reject_paper_finality_v2_source_mutation",
        "hepta_reject_paper_finality_v2_truncate",
        "hepta_validate_paper_finality_v2_time_checkpoint",
        "hepta_paper_finality_v2_constraint_catalog_fingerprint",
        "hepta_paper_chain_finality_v2_constraint_catalog_mismatch",
        "enable always trigger",
        "hepta_paper_chain_finality_v2_source_sealed",
        "hepta_nakama_completions_finality_v2_source_guard",
    ),
)

require_fragments(
    "migrations/0039_add_hepta_review_assignments.sql",
    (
        "hepta_paper_review_assignments",
        "(paper_project_id, review_round, slot)",
        "(paper_project_id, review_round, player_id)",
        "where status = 'claimed'",
        "expires_at timestamptz not null",
        "'evaluator', 'reviewer_1', 'reviewer_2', 'reproducer'",
        "references hepta_joint_paper_submissions(submission_id, paper_project_id)",
    ),
)

require_fragments(
    "migrations/0040_add_hepta_evaluation_draft_quorum.sql",
    (
        "'claimed', 'pinned', 'consumed', 'expired'",
        "where status in ('claimed', 'pinned')",
        "hepta_guard_review_assignment_draft_lifecycle_v1",
        "hepta_review_assignment_draft_lifecycle_guard",
        "old.status = 'pinned' and new.status = 'consumed'",
        "hepta_paper_evaluation_drafts",
        "hepta_paper_evaluation_draft_attestations",
        "hepta_paper_evaluation_drafts_round_idx",
        "hepta_paper_evaluation_draft_attestations_slot_idx",
        "hepta_guard_evaluation_draft_lifecycle_v1",
        "hepta_evaluation_draft_truncate_guard",
        "hepta_evaluation_draft_attestation_update_guard",
        "evaluation draft records are append-only",
    ),
)

require_fragments(
    "migrations/0041_add_hepta_agent_capability_disclosure.sql",
    (
        "capability_disclosure_hash text",
        "hepta_agent_bindings_capability_record_check",
        "hepta_agent_capability_disclosure_valid_v1",
        "count(*) from jsonb_object_keys(disclosure)) <> 5",
        "max_parallel_text::integer not between 1 and 32",
        "research_session_signing",
        "hepta.paper_raid.agent_capability_disclosure.v1",
        "self_declared_unverified",
        "hepta_reject_agent_capability_disclosure_mutation",
        "hepta_agent_capability_disclosure_update_guard",
        "Agent capability disclosure is immutable after binding creation",
    ),
)

require_fragments(
    "migrations/0042_add_hepta_team_proposal_deadlines.sql",
    (
        "add column if not exists expires_at timestamptz",
        "created_at + interval '5 minutes'",
        "jsonb_set(record_json, '{expires_at}', to_jsonb(expires_at), true)",
        "team proposal deadline column/JSON projection diverged; operator repair required",
        "alter column expires_at set not null",
        "hepta_team_proposals_open_deadline_idx",
        "duplicate queued/matched matchmaking authority requires operator review",
        "where status in ('proposed', 'accepted')",
        "status in ('proposed', 'accepted', 'materialized', 'declined', 'expired')",
        "where status in ('queued', 'matched')",
        "status in ('queued', 'matched', 'consumed', 'cancelled', 'expired')",
        "hepta_research_teams t on t.team_id = p.proposal_id",
        "'\"materialized\"'::jsonb",
        "materialized team/proposal authority diverged; operator repair required",
        "materialized team/roster authority diverged; operator repair required",
        "materialized team/source-ticket authority diverged; operator repair required",
        "materialized proposal/ticket terminal backfill did not converge",
        "open team proposal topology diverged; operator repair required",
        "open team proposal/source-ticket authority diverged; operator repair required",
        "one matched ticket cannot belong to multiple open team proposals",
        "generate_subscripts(p.source_ticket_ids, 1)",
        "one matchmaking ticket cannot materialize multiple research teams",
        "materialized proposal is missing its proven research team",
        "hepta.paper_raid.team.materialized.v1",
    ),
)

require_fragments(
    "migrations/0043_add_hepta_challenge_ruleset_v1.sql",
    (
        "challenge_ruleset_snapshot_hash text",
        "outcome in ('in_progress','submission_ready','failed','expired','abandoned')",
        "hepta_guard_paper_challenge_ruleset_v1",
        "paper challenge ruleset snapshot and deadlines are immutable",
        "authoritative paper challenge requires typed rules and deadlines",
        "legacy-unranked paper challenge cannot invent typed rules or deadlines",
        "terminal_at >= grace_expires_at",
        "paper terminal challenge outcome is immutable",
        "hepta_paper_projects_active_deadline_v1_idx",
    ),
)

require_fragments(
    "migrations/0044_add_hepta_agent_proposal_v2_epoch.sql",
    (
        "hepta_agent_proposal_v2_requires_empty_legacy_proposal_table",
        "add column if not exists lease_id uuid",
        "add column if not exists lease_fencing_token bigint",
        "add column if not exists expected_work_version bigint",
        "add column if not exists artifact_manifest_hash text",
        "hepta_agent_proposals_record_json_parity_check",
        "hepta_agent_proposals_payload_hash_check",
        "check (payload_hash ~ '^sha256:[0-9a-f]{64}$')",
        "hepta_agent_proposals_artifact_manifest_hash_check",
        "check (artifact_manifest_hash ~ '^sha256:[0-9a-f]{64}$')",
        "(record_json->>'proposal_id') is not distinct from proposal_id::text",
        "(record_json->>'lease_id') is not distinct from lease_id::text",
        "(record_json->>'lease_fencing_token') is not distinct from lease_fencing_token::text",
        "(record_json->>'expected_work_version') is not distinct from expected_work_version::text",
        "(record_json->>'artifact_manifest_hash') is not distinct from artifact_manifest_hash",
        "signed_at is not distinct from to_timestamp((record_json->>'signed_at_unix')::double precision)",
        "hepta_agent_proposals_lease_scope_fkey",
        "foreign key (lease_id,paper_project_id)",
        "references hepta_section_leases(lease_id,paper_project_id)",
        "hepta_agent_proposals_lease_epoch_v2_idx",
    ),
)

require_fragments(
    "migrations/0045_add_hepta_work_item_record_parity.sql",
    (
        "hepta_paper_work_items_record_json_parity_check",
        "(record_json->>'work_item_id') is not distinct from work_item_id::text",
        "(record_json->>'paper_project_id') is not distinct from paper_project_id::text",
        "(record_json->>'assigned_player_id') is not distinct from assigned_player_id::text",
        "(record_json->>'assigned_binding_id') is not distinct from assigned_binding_id::text",
        "(record_json->>'status') is not distinct from status",
        "(record_json->>'version') is not distinct from version::text",
        ") not valid",
        "validate constraint hepta_paper_work_items_record_json_parity_check",
    ),
)

require_fragments(
    "migrations/0047_add_hepta_contribution_ledger_authority.sql",
    (
        "duplicate_accepted_artifact_contribution_requires_operator_review",
        "in access exclusive mode",
        "contribution_ledger_table_shape_requires_operator_review",
        "contribution_authority_table_metadata_requires_operator_review",
        "hepta_agent_proposals_one_accepted_artifact_manifest_idx",
        "drop index if exists public.hepta_agent_proposals_one_accepted_artifact_manifest_idx",
        "where status = 'accepted'",
        "hepta_paper_contribution_ledger_reservations",
        "contribution_ledger_id uuid primary key",
        "hepta_contribution_ledger_reservations_non_nil_id_check",
        "hepta_contribution_ledger_reservations_ownership_key",
        "unique (contribution_ledger_id, paper_project_id, release_candidate_hash)",
        "legacy_release_candidate_missing_contribution_ledger",
        "left join hepta_paper_contribution_ledger_reservations reservation",
        "on reservation.paper_project_id = revision.paper_project_id",
        "and reservation.release_candidate_hash = revision.release_candidate_hash",
        "and reservation.contribution_ledger_id is null",
        "contribution_ledger_reservation_backfill_mismatch",
        "add column if not exists entries_json jsonb",
        "alter column entries_json set not null",
        "hepta_paper_contribution_ledgers_record_json_parity_check",
        "hepta_paper_contribution_ledgers_paper_release_key",
        "hepta_paper_contribution_ledgers_paper_hash_key",
        "(record_json->'entries') is not distinct from entries_json",
        "hepta_paper_contribution_ledgers_reservation_fkey",
        "foreign key (contribution_ledger_id, paper_project_id, release_candidate_hash)",
        "hepta_reject_frozen_contribution_authority_mutation",
        "enable always trigger hepta_paper_contribution_ledger_immutable_guard",
        "hepta_contribution_ledger_reservation_truncate_guard",
        "hepta_paper_contribution_ledger_truncate_guard",
        "for each statement execute function hepta_reject_frozen_contribution_authority_mutation()",
    ),
)

require_fragments(
    "services/hepta-research-league/src/challenge_ruleset_v1.rs",
    (
        'CHALLENGE_RULESET_V1: &str = "hepta.challenge.ruleset.v1"',
        "phase_gates must contain every forward transition exactly once",
        "BenchmarkAblation",
        "RetainedFailedRuns",
        "canonical_json_sha256(self)",
        "LegacyUnranked",
        "ChallengeRoleResourcesV1",
        "retained_failure_focus_refund",
        "captain_focus must be supportable by evidence, experiment, and run allocations",
        "validate_role_resource_reachability",
        "run_budget must be at least {required_run_budget}",
        "experiment_focus must be at least {successful_runs}",
        "optional_role_resources_preserve_old_typed_bytes_and_join_the_hash_when_present",
        "role_run_budget_must_cover_disjoint_hard_run_minima",
        "experiment_focus_must_cover_successful_run_minimum",
    ),
)

role_resources_source = require_fragments(
    "services/hepta-research-league/src/paper_collaboration_v3/role_resources_v1.rs",
    (
        'ROLE_RESOURCE_STATE_V1: &str = "hepta.paper_raid.role_resources.v1"',
        "role resources are non-economic and cannot unlock ranking, reward, or economic eligibility",
        "pub(super) fn apply_run_role_resources(",
        "pub(crate) fn validate_paper_role_resources(",
        "Paper role-resource authority disagrees with its frozen ChallengeRuleset snapshot",
        "run.status == RunStatus::Failed && run.failure_hash.is_some()",
        "let action_at = now.max(state.updated_at).max(paper.updated_at)",
        "super::super::authoritative_paper_ruleset(paper)?",
        "super::super::require_canonical_author_role_contract(team)?",
        '"run_budget_exhausted"',
        '"run_resources_already_consumed"',
        '"evidence_role_required"',
        '"checkpoint_dependencies_incomplete"',
        "where paper_project_id=$4 and version=$5",
        "where paper_project_id=$1 for update",
        '"/v2/hepta/papers/:paper_id/role-resources/actions"',
        "cancelled_run_never_receives_retained_failure_refund",
        "same_run_cannot_consume_resources_twice",
        "role_mismatch_is_rejected_without_spending_focus",
        "evidence_card_can_consume_evidence_focus_exactly_once",
        "captain_checkpoint_requires_fresh_actions_from_both_other_roles",
        "resource_allocation_must_match_the_frozen_challenge_snapshot",
        "matching_state_and_mutated_ruleset_still_reject_stale_snapshot_hash",
        "explicit_actions_require_the_canonical_roster_contract",
        "terminal_and_elapsed_challenges_project_no_available_role_actions",
        "run_availability_fails_closed_outside_a_run_phase",
        "run_handler_guard_uses_the_supplied_authoritative_deadline_clock",
        "explicit_action_time_cannot_regress_paper_authority",
        "observed_clock_regression_cannot_regress_the_replay_ledger",
    ),
)

role_resource_validator_definition = re.compile(
    r"(?m)^[ \t]*(pub(?:\([^()\r\n]+\))?)[ \t]+fn[ \t]+"
    r"validate_paper_role_resources[ \t]*\("
)


def role_resource_validator_visibility_is_canonical(source):
    return role_resource_validator_definition.findall(source) == ["pub(crate)"]


if not role_resource_validator_visibility_is_canonical(role_resources_source):
    fail(
        "Paper role-resource validator must have exactly one pub(crate) definition; "
        "it is shared inside the crate but must not be public outside it"
    )
for hostile_visibility in ("pub", "pub(super)", "pub(in crate)", ""):
    mutant = role_resources_source.replace(
        "pub(crate) fn validate_paper_role_resources(",
        f"{hostile_visibility + ' ' if hostile_visibility else ''}fn "
        "validate_paper_role_resources(",
        1,
    )
    if role_resource_validator_visibility_is_canonical(mutant):
        fail(
            "Paper role-resource validator visibility gate accepted hostile "
            f"visibility {hostile_visibility or 'private'}"
        )
duplicate_validator = role_resources_source + (
    "\npub(crate) fn validate_paper_role_resources("
    "paper: &PaperProject) -> Result<(), ApiError> { let _ = paper; Ok(()) }\n"
)
if role_resource_validator_visibility_is_canonical(duplicate_validator):
    fail("Paper role-resource validator visibility gate accepted a duplicate definition")

required_parent_reexport = '''pub(super) use role_resources_v1::{
    role_resource_state_from_snapshot, validate_paper_role_resources,
};'''
if required_parent_reexport not in collaboration_source:
    fail("Paper role-resource validator no longer has its crate-internal parent re-export")
if paper_raid_source.count(
    "collaboration_v3::validate_paper_role_resources(&paper)?;"
) < 2:
    fail("Paper Raid sibling paths no longer justify crate-internal validator visibility")

require_fragments(
    "services/hepta-research-league/src/paper_collaboration_v3.rs",
    (
        '#[path = "paper_collaboration_v3/role_resources_v1.rs"]',
        "project_role_resources(",
        "apply_run_role_resources(",
        "require_collaboration_phase_at(",
        "run_record_from_request(",
        'sqlx::query_scalar("select clock_timestamp()")',
        '"failure_retained":record.status == RunStatus::Failed && record.failure_hash.is_some()',
        '"ranking_eligible":false',
        '"reward_eligible":false',
        '"economic_eligibility":false',
    ),
)

require_fragments(
    "services/hepta-research-league/src/paper_collaboration_v3.rs",
    (
        '#[path = "paper_collaboration_v3/matchmaking_party_v1.rs"]',
        "pub party_code_hash: Option<String>",
        "pub private_party: bool",
        "matchmaking_partition_compatible",
        "validate_premade_party_admission",
        '"party_availability_conflict"',
        '"party_role_conflict"',
        "postgres_party_admission_lock_key",
        "horizon.truncate(MAX_ALPHA_MATCH_CANDIDATES)",
        "globally_selected = first_compatible_triplet(&horizon)",
        "eligible_player_ids.contains(&ticket.player_id)",
        "validate_materialization_matchmaking_source",
        "deterministic_match_key_for_ticket_epochs",
        "MATCHMAKING_SOLVER_VERSION_V2",
        "source_preferences",
        "role_assignments",
        '"waiting_for_party_members"',
        '"party_queue_full"',
        "MatchmakingTicketView::from",
        "matchmaking_ticket_created_event_payload",
        "matchmaking_ticket_cancelled_event_payload",
    ),
)
require_fragments(
    "services/hepta-research-league/src/paper_collaboration_v3/matchmaking_party_v1.rs",
    (
        "challenge_id: uuid::Uuid",
        "party_code_hash: Option<&'a str>",
        "matchmaking_partition_key(left) == matchmaking_partition_key(right)",
        "live_party_ticket_count",
        "postgres_party_admission_lock_key",
        "hides_the_party_hash",
    ),
)
require_fragments(
    "services/hepta-research-league/src/paper_raid_v2_tests.rs",
    (
        "exercise_private_party_matchmaking",
        '"party_role_conflict"',
        '"party_availability_conflict"',
        '"party_queue_full"',
        "cancel_matchmaking_ticket_for",
        "materialize_team_proposal_for",
        '"changed-role-party"',
    ),
)
require_fragments(
    "docs/openapi/hepta-paper-raid-v2.yaml",
    (
        "party_code_hash:",
        "private_party:",
        "the digest is never returned by ticket APIs",
        "waiting_for_party_members",
    ),
)

matcher_source = (
    repo / "services/hepta-research-league/src/paper_collaboration_v3.rs"
).read_text(encoding="utf-8")
for helper in (
    "matchmaking_ticket_created_event_payload",
    "matchmaking_ticket_cancelled_event_payload",
):
    start = matcher_source.index(f"fn {helper}")
    end = matcher_source.index("\n}\n", start)
    event_helper = matcher_source[start:end]
    if "party_code" in event_helper or "private_party" in event_helper:
        fail(f"{helper} must not disclose premade-party affinity")

collaboration_source = (
    repo / "services/hepta-research-league/src/paper_collaboration_v3.rs"
).read_text(encoding="utf-8")
run_handler_start = collaboration_source.find("async fn create_run_record(")
run_handler_end = collaboration_source.find(
    "\nasync fn create_figure_lineage(", run_handler_start
)
if run_handler_start < 0 or run_handler_end < 0:
    fail("RunRecord handler boundary is missing")
run_handler = collaboration_source[run_handler_start:run_handler_end]
memory_replay = run_handler.find("memory_replay(")
memory_authority = run_handler.find("paper_and_team_memory(")
postgres_start = run_handler.find("let (mut tx, replay)")
postgres_replay = run_handler.find("if let Some(replay)", postgres_start)
postgres_authority = run_handler.find(
    "paper_and_team_for_role_resource_mutation_postgres(", postgres_replay
)
database_clock = run_handler.find(
    'sqlx::query_scalar("select clock_timestamp()")', postgres_authority
)
database_phase = run_handler.find(
    "require_collaboration_phase_at(&paper, CollaborationMutation::Run, authority_now)",
    database_clock,
)
if not (0 <= memory_replay < memory_authority < postgres_start):
    fail("memory RunRecord replay must precede aggregate authority lookup")
if not (
    postgres_start
    < postgres_replay
    < postgres_authority
    < database_clock
    < database_phase
):
    fail("PostgreSQL RunRecord replay/authority/database-clock/phase order drifted")
if run_handler.count("run_record_from_request(paper_id, &request, authority_now)") != 2:
    fail("memory and PostgreSQL RunRecord paths must share one authoritative-time builder")

role_resource_source = (
    repo
    / "services/hepta-research-league/src/paper_collaboration_v3/role_resources_v1.rs"
).read_text(encoding="utf-8")
if role_resource_source.count(
    "now.max(state.updated_at).max(paper.updated_at)"
) != 2:
    fail("all role-resource actions must clamp time to both ledger and Paper authority")

require_fragments(
    "services/hepta-research-league/src/paper_raid_v2.rs",
    (
        "pub role_resources: Option<RoleResourceStateV1>",
        "role_resource_state_from_snapshot(&ruleset_snapshot, now)?",
    ),
)

require_fragments(
    "docs/adr/ADR-008-hepta-challenge-ruleset-v1.md",
    (
        "gameplay.role_resources",
        "same aggregate",
        "write or PostgreSQL transaction",
        "a cancelled run does",
        "coordination feedback rather than",
        "scientific, phase, victory, finality, ranking, reward, or economic gate",
    ),
)

if "CaptainCheckpoint" in (
    repo / "services/hepta-research-league/src/challenge_ruleset_v1.rs"
).read_text(encoding="utf-8"):
    fail("Captain checkpoint must never become a Challenge phase or victory gate")

require_fragments(
    "services/hepta-research-league/src/lib.rs",
    (
        'include_str!("../../../migrations/0043_add_hepta_challenge_ruleset_v1.sql")',
        'include_str!("../../../migrations/0044_add_hepta_agent_proposal_v2_epoch.sql")',
        'include_str!("../../../migrations/0045_add_hepta_work_item_record_parity.sql")',
        'include_str!("../../../migrations/0046_add_hepta_matchmaking_record_parity_v2.sql")',
        'include_str!("../../../migrations/0047_add_hepta_contribution_ledger_authority.sql")',
        'include_str!("../../../migrations/0048_add_hepta_consumer_finality_v2.sql")',
        'include_str!("../../../migrations/0049_add_hepta_challenge_pack_activation.sql")',
        'include_str!("../../../migrations/0050_add_hepta_paper_rework.sql")',
        '"../../../migrations/0051_bind_legacy_evaluation_panel_lifecycle.sql"',
        '"../../../migrations/0052_harden_hepta_paper_finality_v2_preparation_ingress.sql"',
        '"../../../migrations/0053_allow_review_ready_artifact_manifest_binding.sql"',
        "verify_agent_proposal_v2_migration_catalog",
        "Agent proposal V2 record parity constraint is incomplete",
        "Agent proposal V2 managed constraint catalog must contain exactly 6 entries",
        "hepta_agent_proposals_lease_epoch_v2_idx",
        "verify_work_item_record_parity_catalog",
        "WorkItem record parity constraint is incomplete",
        "verify_contribution_ledger_authority_catalog",
        "verify_consumer_finality_v2_catalog",
        "challenge_pack_activation::verify_migration_catalog",
        "paper_raid_v2::verify_rework_migration_catalog",
        "paper_raid_v2::verify_legacy_evaluation_panel_lifecycle_catalog",
        "verify_finality_v2_preparation_ingress_catalog",
        "verify_review_ready_artifact_manifest_binding_catalog",
        "verify_finality_v2_preparation_ingress_canonical_shape_guards",
        "Paper finality V2 preparation ingress catalog requires exactly two managed functions",
        "Paper finality V2 preparation ingress requires one globally unique exact ENABLE ALWAYS BEFORE INSERT guard",
        "created_at_submicrosecond_9_digits",
        "created_at_24_hour_alias",
        "created_at_leap_second_alias",
        "optional_key_missing",
        "stringified_boolean",
        "rework_null_instead_of_absent",
        "rework_compact_uuid",
        "contribution_authority_constraint_definition_is_exact",
        "globally unique managed names",
        "without unmanaged extras",
        "Accepted artifact authority index is invalid",
        'operator_class") != "uuid_ops"',
        'operator_class_schema") != "pg_catalog"',
        'operator_class_default")',
        'operator_class_input_type") != "uuid"',
        'operator_class_default_key_type")',
        'operator_class_method_matches")',
        'operator_family_schema") != "pg_catalog"',
        'operator_family") != "uuid_ops"',
        'operator_family_method_matches")',
        "Frozen contribution authority requires exactly four globally table-scoped mutation/TRUNCATE guards",
        "IMMUTABLE_TRIGGER_BODY",
        "normalized_catalog_definition(&function_body) != IMMUTABLE_TRIGGER_BODY",
        'mod challenge_ruleset_v1;',
        'mod challenge_pack_activation;',
        'pub use challenge_ruleset_v1::*;',
    ),
)

review_ready_binding_migration = (
    repo / "migrations/0053_allow_review_ready_artifact_manifest_binding.sql"
).read_text(encoding="utf-8")
require_fragments(
    "migrations/0053_allow_review_ready_artifact_manifest_binding.sql",
    (
        "drop constraint if exists hepta_artifact_manifests_binding_schema_check",
        "add constraint hepta_artifact_manifests_binding_schema_check",
        "binding_schema = 'hepta.paper_raid.artifact_manifest_binding.v1'",
        "binding_schema = 'hepta.paper_raid.review_ready_artifact_manifest_binding.v1'",
    ),
)
if (
    review_ready_binding_migration.lower().count("begin;") != 1
    or review_ready_binding_migration.lower().count("commit;") != 1
    or review_ready_binding_migration.count("binding_schema = ") != 2
    or "not valid" in review_ready_binding_migration.lower()
):
    fail("0053 must be one atomic, validated, exact two-schema forward migration")

require_fragments(
    "migrations/0048_add_hepta_consumer_finality_v2.sql",
    (
        "refuses to infer bindings for existing V1 projections",
        "hepta.paper_raid.chain_finality_projection.v2",
        "hepta_consumer_finality_v2_projection_guard",
        "consumer-finality V2 reproduction binding is not exact",
        "consumer-finality V2 uphold does not activate the effective evaluation",
    ),
)
require_fragments(
    "migrations/0049_add_hepta_challenge_pack_activation.sql",
    (
        "hepta_challenge_pack_activations_v1",
        "paper-raid-evidence-audit-seeded-v1",
        "hepta_validate_challenge_pack_activation_v1",
        "Challenge Pack activation record relational/JSON parity failed",
        "hepta_challenge_pack_activation_immutable_guard",
        "hepta_challenge_pack_activation_truncate_guard",
        "enable always trigger hepta_challenge_pack_activation_validate_guard",
        "enable always trigger hepta_challenge_pack_activation_immutable_guard",
        "enable always trigger hepta_challenge_pack_activation_truncate_guard",
        "Challenge Pack activation records are append-only",
    ),
)
require_fragments(
    "migrations/0050_add_hepta_paper_rework.sql",
    (
        "hepta_paper_reworks",
        "hepta_paper_rework_resubmissions",
        "interval '24 hours'",
        "rework_window_elapsed",
        "rejected_rework_content_commitment_sha256",
        "hepta_paper_rework_content_commitment_sha256",
        "pg_catalog.sha256",
        "replacement_review_round = 1",
        "hepta_joint_submission_rework_withdrawal_trigger",
        "enable always trigger hepta_paper_reworks_finality_v2_source_guard",
        "hepta_paper_rework_finality_v2_lineage_guard",
        "hepta_paper_finality_v2_rework_lineage_mismatch",
    ),
)
if "hepta_paper_one_active_rework_idx" in (
    repo / "migrations/0050_add_hepta_paper_rework.sql"
).read_text(encoding="utf-8"):
    fail("0050 must not use a subquery-backed partial index for active reworks")
require_fragments(
    "migrations/0051_bind_legacy_evaluation_panel_lifecycle.sql",
    (
        "begin;",
        "commit;",
        "drop constraint if exists hepta_paper_review_assignments_pinned_evaluation_fkey",
        "create or replace function hepta_guard_review_assignment_draft_lifecycle_v1",
        "hepta_paper_evaluation_panel_attestations",
        "evaluation.evaluation_id = new.pinned_evaluation_id",
        "enable always trigger hepta_review_assignment_draft_lifecycle_guard",
    ),
)

finality_v2_ingress_predicate_sha256 = (
    "c861ea0fea786979507fc23dd0435002"
    "c838a4bdbaa1f23e9c4e0420953570e8"
)
finality_v2_ingress_guard_sha256 = (
    "f9db620e91b35d1ae38b2c3abef60a1e"
    "44d0c8522b6c30195c0de9bac2835b87"
)


def finality_v2_ingress_function_body(text, function_name):
    matches = re.findall(
        rf"create or replace function public\.{re.escape(function_name)}\(.*?"
        r"\)\nreturns .*?\nas \$function\$(.*?)\$function\$;",
        text,
        flags=re.DOTALL,
    )
    if len(matches) != 1:
        fail(
            f"0052 must define exactly one catalogued function body for {function_name}"
        )
    return matches[0]


def validate_finality_v2_preparation_ingress_hardening(text):
    if text.count("begin;") != 1 or text.count("commit;") != 1:
        fail("0052 must be one explicit atomic forward migration")
    if "drop constraint" in text.lower() or "alter constraint" in text.lower():
        fail("0052 must not replace historical 0038 constraints")

    predicate_body = finality_v2_ingress_function_body(
        text, "hepta_paper_finality_v2_preparation_ingress_valid_v1"
    )
    guard_body = finality_v2_ingress_function_body(
        text, "hepta_validate_paper_finality_v2_preparation_ingress_v1"
    )
    if hashlib.sha256(predicate_body.encode()).hexdigest() != (
        finality_v2_ingress_predicate_sha256
    ):
        fail("0052 canonical preparation predicate body digest drifted")
    if hashlib.sha256(guard_body.encode()).hexdigest() != (
        finality_v2_ingress_guard_sha256
    ):
        fail("0052 preparation trigger body digest drifted")

    zero_digest = (
        "sha256:0000000000000000000000000000000000000000000000000000000000000000"
    )
    zero_raw_hash = "0" * 64
    if predicate_body.count(zero_digest) != 6:
        fail("0052 must reject all six canonical all-zero preparation digests")
    if predicate_body.count(zero_raw_hash) != 8:
        # Six prefixed digests contain the same 64-zero suffix, plus two raw hashes.
        fail("0052 must reject both canonical all-zero preparation raw hashes")
    canonical_created_at_pattern = (
        "^[0-9]{4}-(0[1-9]|1[0-2])-(0[1-9]|[12][0-9]|3[01])"
        "T([01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9]"
        "(Z|[.](?!000Z)[0-9]{3}Z|[.][0-9]{3}(?!000Z)[0-9]{3}Z)$"
    )
    canonical_uuid_pattern = (
        "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-"
        "[0-9a-f]{4}-[0-9a-f]{12}$"
    )
    if canonical_created_at_pattern not in predicate_body:
        fail("0052 must require exact Chrono UTC AutoSi created_at spelling")
    if predicate_body.count("(?!000Z)") != 2:
        fail("0052 must reject redundant 3/6-digit AutoSi zero padding")
    if "date_trunc(" in predicate_body or (
        "(($1).record_json ->> 'created_at')::timestamptz\n"
        "                = ($1).created_at"
    ) not in predicate_body:
        fail("0052 created_at spelling must retain exact timestamptz cast parity")
    if (
        "extract(epoch from ($1).created_at) * 1000\n"
        "                = ($1).final_consensus_time_unix_ms::numeric"
    ) not in predicate_body:
        fail("0052 created_at must remain the exact millisecond Chain checkpoint")
    if "[.][0-9]{6}(?!000Z)[0-9]{3}Z" in predicate_body:
        fail("0052 must reject unrepresentable 9-digit PostgreSQL timestamps")
    if predicate_body.count(canonical_uuid_pattern) != 18:
        fail("0052 must canonicalize every preparation/binding UUID spelling")
    for uuid_path in (
        "'preparation_id'",
        "{binding,window_arm_id}",
        "{binding,paper_project_id}",
        "{binding,submission_id}",
        "{binding,rework_lineage,rework_id}",
        "{binding,rework_lineage,rejected_submission_id}",
        "{binding,rework_lineage,replacement_submission_id}",
        "{binding,rework_lineage,rejected_revision_id}",
        "{binding,rework_lineage,replacement_revision_id}",
        "{binding,evaluation_id}",
        "{binding,evaluation_supersedes_evaluation_id}",
        "{binding,evaluation_superseded_by_evaluation_id}",
        "{binding,latest_reproduction_id}",
        "{binding,reproduction_supersedes_reproduction_id}",
        "{binding,reproduction_superseded_by_reproduction_id}",
        "{binding,appeal_id}",
        "{binding,appealed_evaluation_id}",
        "{binding,appeal_resolution_id}",
    ):
        if uuid_path not in predicate_body:
            fail(f"0052 canonical UUID predicate lost {uuid_path!r}")
    for shape_fragment in (
        "($1).record_json ?& array[",
        "from jsonb_object_keys(($1).record_json) as top_key",
        ") = 8",
        "($1).record_json -> 'binding') ?& array[",
        ") = 55 + case",
        "from jsonb_each(($1).record_json) as top_field",
        "as optional_field(field_name, field_value)",
        "optional_field.field_value <> 'null'::jsonb",
        "as number_field(field_name, field_value)",
        "!~ '^(0|[1-9][0-9]*)$'",
        "> 18446744073709551615::numeric",
        "as boolean_field(field_name, field_value)",
        "#> '{binding,rework_lineage}' is null",
        "#> '{binding,rework_lineage}') ?& array[",
        "as rework_string(field_name, field_value)",
        ") = 13",
    ):
        if shape_fragment not in predicate_body:
            fail(f"0052 exact serde JSON shape predicate lost {shape_fragment!r}")
    for fragment in (
        "length(($1).final_chain_id) between 1 and 64",
        "($1).final_chain_id collate \"C\" ~ '^[a-z0-9._:-]+$'",
        "($1).record_json ->> 'request_hash' = ($1).request_hash",
        "($1).record_json ->> 'binding_fingerprint'",
        "{binding,commitment_id}",
        "{binding,source_fingerprint}",
        "{binding,match_evidence_commitment_id}",
        "{binding,final_checkpoint_hash}",
        "{binding,final_checkpoint_anchor_hash}",
        "{binding,start_checkpoint_chain_id}",
        "{binding,final_checkpoint_chain_id}",
        "{binding,final_checkpoint_header_hash}",
        "{binding,final_checkpoint_consensus_time_unix_ms}",
        "{binding,scientific_finality}",
        "{binding,score_eligible}",
        "{binding,ranking_eligible}",
        "{binding,reward_eligible}",
        "{binding,economic_eligible}",
    ):
        if fragment not in predicate_body:
            fail(f"0052 canonical relational/JSON predicate lost {fragment!r}")

    for fragment in (
        "hepta_paper_finality_v2_preparation_ingress_backfill_forbidden",
        "where not public.hepta_paper_finality_v2_preparation_ingress_valid_v1(",
        "create trigger hepta_paper_finality_v2_preparation_ingress_guard",
        "before insert on public.hepta_paper_chain_finality_preparations_v2",
        "enable always trigger hepta_paper_finality_v2_preparation_ingress_guard",
        "trigger_row.tgtype = 7",
        "trigger_row.tgenabled = 'A'",
        "global_trigger_name_count <> 1",
        "hepta_paper_finality_v2_historical_constraint_catalog_mismatch",
        "910d4454106f5722ad44c6c9095bf48d585dfaa9501fc40d9ef377fd57c3f3ba",
        "hepta_paper_finality_v2_preparation_ingress_catalog_mismatch",
        "hepta_paper_finality_v2_record_json_check",
        "Historical full relational/JSON parity CHECK must remain present and validated",
    ):
        if fragment not in text:
            fail(f"0052 exact ingress/catalog contract lost {fragment!r}")
    if text.count(finality_v2_ingress_predicate_sha256) != 2:
        fail("0052 predicate body digest must be pinned by both comparison and evidence")
    if text.count(finality_v2_ingress_guard_sha256) != 2:
        fail("0052 guard body digest must be pinned by both comparison and evidence")


finality_v2_ingress_migration = (
    repo
    / "migrations/0052_harden_hepta_paper_finality_v2_preparation_ingress.sql"
).read_text(encoding="utf-8")
validate_finality_v2_preparation_ingress_hardening(finality_v2_ingress_migration)
finality_v2_ingress_mutations = {
    "zero digest admitted": finality_v2_ingress_migration.replace(
        "sha256:0000000000000000000000000000000000000000000000000000000000000000",
        "sha256:1000000000000000000000000000000000000000000000000000000000000000",
        1,
    ),
    "chain id length widened": finality_v2_ingress_migration.replace(
        "between 1 and 64", "between 1 and 128", 1
    ),
    "chain id uppercase admitted": finality_v2_ingress_migration.replace(
        "^[a-z0-9._:-]+$", "^[A-Za-z0-9._:-]+$", 1
    ),
    "created_at offset alias admitted": finality_v2_ingress_migration.replace(
        "[0-5][0-9](Z|[.](?!000Z)",
        "[0-5][0-9]([+]00:00|Z|[.](?!000Z)",
        1,
    ),
    "created_at normalized hour or second admitted": finality_v2_ingress_migration.replace(
        "([01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9]",
        "[0-9]{2}:[0-9]{2}:[0-9]{2}",
        1,
    ),
    "created_at redundant AutoSi zeros admitted": finality_v2_ingress_migration.replace(
        "(?!000Z)", "", 1
    ),
    "created_at nanoseconds admitted": finality_v2_ingress_migration.replace(
        "|[.][0-9]{3}(?!000Z)[0-9]{3}Z)$",
        "|[.][0-9]{3}(?!000Z)[0-9]{3}Z|[.][0-9]{6}(?!000Z)[0-9]{3}Z)$",
        1,
    ),
    "top-level optional key admitted missing": finality_v2_ingress_migration.replace(
        ") = 8", ") >= 7", 1
    ),
    "binding optional key admitted missing": finality_v2_ingress_migration.replace(
        ") = 55 + case", ") >= 54 + case", 1
    ),
    "stringified number admitted": finality_v2_ingress_migration.replace(
        "jsonb_typeof(number_field.field_value) <> 'number'",
        "jsonb_typeof(number_field.field_value) <> 'string'",
        1,
    ),
    "rework null admitted": finality_v2_ingress_migration.replace(
        "#> '{binding,rework_lineage}' is null",
        "#> '{binding,rework_lineage}' is null or "
        "($1).record_json #> '{binding,rework_lineage}') = 'null'::jsonb",
        1,
    ),
    "UUID uppercase alias admitted": finality_v2_ingress_migration.replace(
        "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$",
        "^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$",
        1,
    ),
    "optional supersession UUID guard detached": finality_v2_ingress_migration.replace(
        "{binding,evaluation_supersedes_evaluation_id}",
        "{binding,evaluation_supersedes_id}",
        1,
    ),
    "rework UUID guard inverted": finality_v2_ingress_migration.replace(
        "#> '{binding,rework_lineage}' is null",
        "#> '{binding,rework_lineage}' is not null",
        1,
    ),
    "json commitment detached": finality_v2_ingress_migration.replace(
        "{binding,commitment_id}", "{binding,commitment}", 1
    ),
    "legacy row preflight inverted": finality_v2_ingress_migration.replace(
        "where not public.hepta_paper_finality_v2_preparation_ingress_valid_v1(",
        "where public.hepta_paper_finality_v2_preparation_ingress_valid_v1(",
        1,
    ),
    "always trigger downgraded": finality_v2_ingress_migration.replace(
        "enable always trigger hepta_paper_finality_v2_preparation_ingress_guard",
        "enable trigger hepta_paper_finality_v2_preparation_ingress_guard",
        1,
    ),
    "trigger phase weakened": finality_v2_ingress_migration.replace(
        "before insert on public.hepta_paper_chain_finality_preparations_v2",
        "after insert on public.hepta_paper_chain_finality_preparations_v2",
        1,
    ),
    "predicate digest unpinned": finality_v2_ingress_migration.replace(
        finality_v2_ingress_predicate_sha256,
        "0" * 64,
        1,
    ),
}
for mutation_name, mutation in finality_v2_ingress_mutations.items():
    if mutation == finality_v2_ingress_migration:
        fail(f"0052 hostile mutation was not applied: {mutation_name}")
    try:
        validate_finality_v2_preparation_ingress_hardening(mutation)
    except AssertionError:
        pass
    else:
        fail(f"0052 static proof accepted hostile mutation: {mutation_name}")

require_fragments(
    "services/hepta-research-league/src/paper_rework_v1.rs",
    (
        '"/v2/hepta/papers/:paper_id/reworks"',
        "PAPER_REWORK_LEASE_HOURS: i64 = 24",
        "rework_content_commitment_sha256",
        "replacement_review_round: 1",
        "verify_rework_migration_catalog",
    ),
)
require_fragments(
    "services/hepta-research-league/src/challenge_pack_activation.rs",
    (
        '"/v1/hepta/operator/challenges/:challenge_id/pack-activation"',
        "OPERATOR_TOKEN_HEADER",
        "state_json #>> array['challenges',$4::text,'status']='draft'",
        "challenge_pack_activation_replay_conflict",
        "hepta.challenge_pack.activated.v1",
        "cross_paper_denial_receipt_sha256",
        "paper-raid-evidence-audit-seeded-v1",
    ),
)
require_fragments(
    "services/hepta-research-league/src/paper_review_v4.rs",
    (
        "prepared_replacement_for_resolution",
        "denied Appeal must not omit or reference an already prepared replacement evaluation",
        "where paper_project_id=$1 and supersedes_evaluation_id=$2",
        "effective_finality_resolution_id",
        "the evaluation requires an exact root-to-leaf upheld Appeal lineage before downstream mutation",
        "paper_finality_reproduction_before_activation",
        "hepta.paper_raid.consumer_finality.v2",
        "effective_reproduction_id",
        "effective_appeal_resolution_id",
    ),
)
require_fragments(
    "services/hepta-research-league/src/paper_chain_finality_v2.rs",
    (
        "every intermediate generation must be activated by its exact chronological upheld Appeal resolution",
        "the final generation may terminate only in one exact chronological denied resolution",
        "a resolution outside the exact parent chain claims an evaluation in the final lineage",
        "the final submission contains an evaluation outside the single exact root-to-leaf lineage",
        "paper_trnm_consumer_resolution_binding_mismatch",
    ),
)
require_fragments(
    "services/hepta-research-league/src/paper_raid_v2_tests.rs",
    (
        "premature-child-appeal",
        "a prepared child must not acquire its own Appeal before the exact parent uphold activates it",
    ),
)
require_fragments(
    "services/paper-raid-bff/src/html.rs",
    (
        '"hepta.paper_raid.consumer_finality.v2"',
        'finality.get("effective_evaluation_id")',
        'finality.get("effective_reproduction_id")',
        'finality.get("effective_appeal_resolution_id")',
        "effective_reproduction_id == reproduction.reproduction_id",
    ),
)
require_fragments(
    "docs/openapi/hepta-paper-raid-v2.yaml",
    (
        "PaperChainFinalityProjectionV2:",
        "ConsumerPaperFinalityV2:",
        "hepta.paper_raid.chain_finality_projection.v2",
        "hepta.paper_raid.consumer_finality.v2",
        "effective_appeal_resolution_id",
    ),
)

contribution_catalog_source = (
    repo / "services/hepta-research-league/src/lib.rs"
).read_text(encoding="utf-8")
contribution_catalog_start = contribution_catalog_source.find(
    "fn contribution_authority_constraint_definition_is_exact("
)
contribution_catalog_end = contribution_catalog_source.find(
    "\nfn matchmaking_v2_column_is_exact(", contribution_catalog_start
)
if contribution_catalog_start < 0 or contribution_catalog_end < 0:
    fail("Contribution authority exact catalog verifier boundary is missing")
contribution_catalog = contribution_catalog_source[
    contribution_catalog_start:contribution_catalog_end
]
for forbidden in (
    "let mut definitions = HashMap",
    ".all(|fragment| definition.contains(fragment))",
):
    if forbidden in contribution_catalog:
        fail(f"Contribution authority catalog reverted to fragment/name-only trust: {forbidden}")

contribution_migration = (
    repo / "migrations/0047_add_hepta_contribution_ledger_authority.sql"
).read_text(encoding="utf-8")


def validate_contribution_legacy_upgrade_guard(source):
    guard_start = source.find("-- A pre-0047 release candidate")
    guard_end = source.find("-- All constraints on these two tables", guard_start)
    if guard_start < 0 or guard_end < 0:
        fail("0047 legacy-upgrade guard boundary is missing")
    guard = source[guard_start:guard_end]
    for required in (
        "left join hepta_paper_contribution_ledgers ledger",
        "on ledger.paper_project_id = revision.paper_project_id",
        "and ledger.release_candidate_hash = revision.release_candidate_hash",
        "left join hepta_paper_contribution_ledger_reservations reservation",
        "on reservation.paper_project_id = revision.paper_project_id",
        "and reservation.release_candidate_hash = revision.release_candidate_hash",
        "and ledger.contribution_ledger_id is null",
        "and reservation.contribution_ledger_id is null",
        "legacy_release_candidate_missing_contribution_ledger",
    ):
        if guard.count(required) != 1:
            fail(f"0047 legacy-upgrade three-table predicate is incomplete: {required}")


validate_contribution_legacy_upgrade_guard(contribution_migration)
legacy_upgrade_guard_mutations = {
    "reservation authority removed": contribution_migration.replace(
        "          left join hepta_paper_contribution_ledger_reservations reservation\n"
        "            on reservation.paper_project_id = revision.paper_project_id\n"
        "           and reservation.release_candidate_hash = revision.release_candidate_hash\n",
        "",
        1,
    ),
    "reservation release detached": contribution_migration.replace(
        "and reservation.release_candidate_hash = revision.release_candidate_hash",
        "and reservation.release_candidate_hash = reservation.release_candidate_hash",
        1,
    ),
    "reservation presence required": contribution_migration.replace(
        "and reservation.contribution_ledger_id is null",
        "and reservation.contribution_ledger_id is not null",
        1,
    ),
    "either authority missing is fatal": contribution_migration.replace(
        "and reservation.contribution_ledger_id is null",
        "or reservation.contribution_ledger_id is null",
        1,
    ),
}
for mutation_name, mutation in legacy_upgrade_guard_mutations.items():
    if mutation == contribution_migration:
        fail(f"0047 legacy-upgrade negative mutation was not applied: {mutation_name}")
    try:
        validate_contribution_legacy_upgrade_guard(mutation)
    except AssertionError:
        pass
    else:
        fail(f"0047 legacy-upgrade negative mutation was accepted: {mutation_name}")

for required in (
    "replay_0047_after_reservation",
    "post-0047 promotion must own exactly one reservation triple",
    "0047 replay must accept a live post-0047 candidate with its exact reservation and no frozen ledger yet",
    "0047 first upgrade must reject a live candidate missing both authorities",
    'legacy_upgrade_database_error.message(),\n        "legacy_release_candidate_missing_contribution_ledger"',
):
    if required not in contribution_tests:
        fail(f"0047 reservation-only replay/legacy-failure proof is missing: {required}")

if contribution_migration.count("before truncate on hepta_paper_contribution_") != 2:
    fail("Both contribution authority tables require statement BEFORE TRUNCATE guards")

require_fragments(
    "services/hepta-research-league/src/paper_review_v4.rs",
    (
        "ledger_loader_requires_the_exact_memory_reservation_triple",
        "require_contribution_ledger_reservation_memory(",
        "require_contribution_ledger_reservation_postgres(",
        "an unreserved ledger must never reach RaidScore",
    ),
)

require_fragments(
    "crates/hepta-paper-raid-contracts/src/lib.rs",
    (
        'AGENT_PROPOSAL_V1: &str = "hepta.paper_raid.agent_proposal.v1"',
        'AGENT_PROPOSAL_V2: &str = "hepta.paper_raid.agent_proposal.v2"',
        "pub fn agent_proposal_signing_bytes(",
        "pub fn agent_proposal_v2_signing_bytes(",
        'CanonicalFrame::new("hepta_paper_raid_agent_proposal_v2")',
        ".string(&proposal.lease_id.to_string())?",
        ".u64(proposal.lease_fencing_token)",
        ".u64(proposal.expected_work_version)",
        ".string(&proposal.artifact_manifest_id.to_string())?",
        "agent_proposal_v2_binds_epoch_work_version_and_manifest_identity",
    ),
)

require_fragments(
    "services/hepta-research-league/src/paper_collaboration_v3.rs",
    (
        "AgentProposalSigningV2",
        "stale_agent_proposal_lease_epoch",
        "expected_work_version",
        "artifact_manifest_id,artifact_manifest_hash,agent_id,binding_id",
        ".bind(&proposal.artifact_manifest_hash)",
        "select clock_timestamp()",
        "Agent proposal relational scope disagrees with record_json",
        "section lease relational columns disagree with record_json",
        "TEAM_PROPOSAL_RESPONSE_TTL_SECONDS: i64 = 5 * 60",
        "team_proposal_deadline",
        "expire_team_proposal_memory",
        "expire_due_team_proposals_postgres",
        "hepta.paper_raid.team_proposal.expired.v1",
        "hepta.paper_raid.team_proposal.withdrawn.v1",
        "team_proposal_expired",
        "materialized_team_cannot_withdraw",
        "MatchmakingTicketStatus::Queued | MatchmakingTicketStatus::Matched",
        "TeamProposalStatus::Materialized",
        "MatchmakingTicketStatus::Consumed",
        "auto_match_all_queued_tickets_postgres",
        "hepta-paper-raid-team-id:",
        "matchmaking_team_id_conflict",
        "for share of p,b",
    ),
)

require_fragments(
    "services/hepta-research-league/src/paper_raid_v2_tests.rs",
    (
        "binding-authority-old-lease-exact-replay",
        "the exact old V2 body must fail after the same binding reacquires a new lease epoch",
        "binding-authority-old-work-version-exact-replay",
        "the exact old V2 body must fail after work is rejected and reopened at a new version",
        "assert_postgres_agent_proposal_v2_identity_parity",
        "runtime readiness must reject a missing Agent proposal V2 index",
        "full signed Agent proposal identity parity must reject relational-only mutation",
        "must remain a canonical SHA-256 digest even when record_json matches",
        "assert_postgres_work_item_record_parity",
        "runtime readiness must reject a missing WorkItem parity constraint",
        "runtime readiness must reject an unvalidated WorkItem parity constraint",
        "WorkItem parity must reject a relational-only status mutation",
        "WorkItem parity must reject a record_json-only",
    ),
)

require_fragments(
    "services/hepta-research-league/src/paper_raid_v2.rs",
    (
        "generic_team_id_namespace_reserved",
        "direct team creation requires a UUIDv4 team_id",
        "team_id_reserved_for_matchmaking",
        "hepta-paper-raid-team-id:",
        "where player_id = $1 for update",
    ),
)

require_fragments(
    "services/hepta-research-league/src/paper_review_v4.rs",
    (
        '"/v2/hepta/review-queue"',
        '"/v2/hepta/papers/:paper_id/review-assignments"',
        '"/v2/hepta/papers/:paper_id/review-bundle"',
        "REVIEW_ASSIGNMENT_TTL_HOURS",
        "expire_review_assignments_postgres",
        "review_assignment_author_forbidden",
        "review_assignment_panel_mismatch",
        "review_assignment_repanel_not_independent",
        "review_assignment_reproducer_mismatch",
        '"/v2/hepta/papers/:paper_id/evaluation-drafts"',
        '"/v2/hepta/papers/:paper_id/evaluation-drafts/:evaluation_id"',
        '"/v2/hepta/papers/:paper_id/evaluation-drafts/:evaluation_id/attestations"',
        '"/v2/hepta/papers/:paper_id/evaluation-drafts/:evaluation_id/finalize"',
        "PaperReviewBundleV1",
        "evaluation_quorum",
        "make_paper_review_bundle",
        "evaluation_draft_assignment_mismatch",
        "evaluation_draft_hash_mismatch",
        "evaluation_draft_quorum_incomplete",
        "transition_review_assignment_memory",
        "transition_review_assignment_postgres",
        "evaluation_draft_assignment_not_pinned",
        "ReviewAssignmentStatus::Pinned",
        "ReviewAssignmentStatus::Consumed",
        "lock_evaluation_round_postgres",
        "lock_evaluation_identity_postgres",
    ),
)

require_fragments(
    "services/paper-raid-bff/src/config.rs",
    (
        "MIN_ALPHA_AUTHOR_IDENTITIES: usize = 3",
        "MIN_ALPHA_INDEPENDENT_REVIEW_IDENTITIES: usize = 4",
        "distinct_author_role_assignment",
        "distinct_independent_review_assignment",
        "deploy_alpha_env_example_is_accepted_by_the_production_identity_parser",
        'include_str!("../deploy/alpha.env.example")',
    ),
)

alpha_env_path = repo / "services/paper-raid-bff/deploy/alpha.env.example"
alpha_prefix = "PAPER_RAID_BFF_ALPHA_IDENTITIES_JSON="
alpha_lines = [
    line[len(alpha_prefix):]
    for line in alpha_env_path.read_text(encoding="utf-8").splitlines()
    if line.startswith(alpha_prefix)
]
if len(alpha_lines) != 1:
    fail("Alpha deployment example must define exactly one identity JSON variable")
try:
    alpha_identities = json.loads(alpha_lines[0])
except json.JSONDecodeError as error:
    fail(f"Alpha deployment identity fixture is invalid JSON: {error}")
if not isinstance(alpha_identities, list) or len(alpha_identities) != 7:
    fail("Alpha deployment example must contain exactly seven identities")

allowed_scopes = {"author", "evaluator", "reviewer", "reproducer"}
allowed_roles = {"captain", "evidence", "experiment"}
for identity in alpha_identities:
    if not isinstance(identity, dict):
        fail("Alpha deployment identity fixture entries must be objects")
    if set(identity) != {
        "login_key",
        "subject_id",
        "display_name",
        "nakama_user_id",
        "player_id",
        "scopes",
        "author_roles",
    }:
        fail("Alpha deployment identities must use the explicit production field set")
    if not isinstance(identity["login_key"], str) or len(identity["login_key"]) < 32:
        fail("Alpha deployment login keys must contain at least 32 bytes")
    if not isinstance(identity["subject_id"], str) or not re.fullmatch(
        r"[A-Za-z0-9._:-]{1,128}", identity["subject_id"]
    ):
        fail("Alpha deployment subject identifiers are invalid")
    if (
        not isinstance(identity["display_name"], str)
        or not identity["display_name"].strip()
        or len(identity["display_name"]) > 80
    ):
        fail("Alpha deployment display names are invalid")
    for uuid_field in ("nakama_user_id", "player_id"):
        if not isinstance(identity[uuid_field], str) or not re.fullmatch(
            r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}",
            identity[uuid_field],
        ):
            fail(f"Alpha deployment {uuid_field} is not a canonical UUID")
    scopes = identity["scopes"]
    roles = identity["author_roles"]
    if (
        not isinstance(scopes, list)
        or not scopes
        or len(scopes) != len(set(scopes))
        or not set(scopes) <= allowed_scopes
    ):
        fail("Alpha deployment identity scopes are invalid")
    if (
        not isinstance(roles, list)
        or len(roles) != len(set(roles))
        or not set(roles) <= allowed_roles
        or (("author" in scopes) != bool(roles))
    ):
        fail("Alpha deployment identity author roles are invalid")

for field in ("login_key", "subject_id", "nakama_user_id", "player_id"):
    values = [identity[field] for identity in alpha_identities]
    if len(values) != len(set(values)):
        fail(f"Alpha deployment identity fixture has duplicate {field}")

authors = [identity for identity in alpha_identities if "author" in identity["scopes"]]
independent = [identity for identity in alpha_identities if "author" not in identity["scopes"]]
if len(authors) != 3 or len(independent) != 4:
    fail("Alpha deployment fixture must separate three authors and four non-authors")


def has_distinct_assignment(identities, requirements, field):
    def assign(position, used):
        if position == len(requirements):
            return True
        for index, identity in enumerate(identities):
            if index not in used and requirements[position] in identity[field]:
                if assign(position + 1, used | {index}):
                    return True
        return False

    return assign(0, set())


if not has_distinct_assignment(
    authors, ["captain", "evidence", "experiment"], "author_roles"
):
    fail("Alpha deployment authors cannot fill the three distinct author roles")
if not has_distinct_assignment(
    independent, ["evaluator", "reviewer", "reviewer", "reproducer"], "scopes"
):
    fail("Alpha deployment non-authors cannot fill the four distinct review seats")

require_fragments(
    "services/hepta-research-league/src/paper_chain_finality_v2.rs",
    (
        "pg_advisory_xact_lock",
        "verify_cometbft_light_finality_proof_v1_with_trust_anchor",
        "PAPER_CHAIN_TIME_MAX_LAG_MS_V1",
        "paper_trnm_v2_window_arm_stale",
        "paper_trnm_v2_final_checkpoint_not_causal",
    ),
)

require_fragments(
    "scripts/generate-hepta-receipt-v2-resource-fixtures.py",
    (
        "fresh legal receipt is not near the default cap",
        "trust anchor is expired or too close to expiry",
        "canonical-shape-adversarial.json",
        "output directory must not already exist",
        "bundle digest mismatch",
        "DEFAULT_CAP = 32 * 1024",
        "DEPLOYMENT_MAX = 1024 * 1024",
        "duplicate canonical JSON members",
        "a symlinked manifest",
        "os.O_NONBLOCK",
        "a FIFO manifest without blocking",
        "a tampered trust anchor",
        "a tampered legal receipt",
        "a bundle with a missing file",
        "a bundle with an extra file",
        "fixture cap drift",
        "an expired trust anchor under the freshness policy",
    ),
)
require_fragments(
    "scripts/admit-hepta-image-build-evidence.py",
    (
        "MAX_LOG_BYTES = 64 * 1024 * 1024",
        "os.O_NOFOLLOW | os.O_NONBLOCK",
        "metadata.st_nlink != 1",
        "metadata.st_uid != os.geteuid()",
        "file_identity(os.fstat(stdout_fd)) != file_identity(stdout_before)",
        "image build stdout and stderr must be distinct files",
        "image build stdout must contain exactly one provenance object",
        "image provenance must be the final stdout object",
        "object_pairs_hook=reject_duplicate_keys",
        'type(reproducibility["independent_no_cache_builds"]) is not int',
        'type(reproducibility["identical_image_ids"]) is not bool',
        '"version": "v0.36.1"',
        "write_artifact(args.staging_fd, \"image-build.stdout\"",
        "digest_artifact(args.staging_fd, \"image-build.stdout\")",
        "canonical image provenance did not round-trip exactly",
    ),
)
require_fragments(
    "scripts/check-hepta-receipt-v2-resource-gate.sh",
    (
        "HEPTA_IMAGE_BUILD_STDOUT",
        "HEPTA_IMAGE_BUILD_STDERR",
        "HEPTA_RESOURCE_GATE_FIXTURE_DIR",
        "HEPTA_RESOURCE_GATE_EVIDENCE_DIR",
        '--verify-bundle "$fixture_dir"',
        ".HostConfig.Memory",
        "memory.peak",
        ".State.OOMKilled",
        ".RestartCount",
        "queued_trnm_command_not_found",
        "trnm_receipt_v2_structural_invalid",
        "request_body_too_large",
        "trnm_receipt_v2_verification_busy",
        "assert_db_unchanged",
        "private_sibling_staging_then_atomic_noreplace_rename",
        "renameat2",
        "dir_fd=parent_fd",
        "os.O_DIRECTORY | os.O_CLOEXEC | os.O_NOFOLLOW",
        "os.O_NONBLOCK",
        "resource evidence artifact set differs",
        "publication_authority",
        "post_rename_inode_verified",
        "PAYLOAD.SHA256",
        "SHA256SUMS",
        "image-build.stdout",
        "image-build.stderr",
        "image-provenance.json",
        "closure_manifest_sha256",
        "publication_contract",
        "commit_index_worktree_identical",
        "verify-hepta-clean-source.py",
        "hepta-release-authority.lock",
        "GIT_NO_REPLACE_OBJECTS=1",
        '"$git_binary" --no-replace-objects',
        "org.opencontainers.image.revision",
        "io.trillionnium.hepta.application-sbom.sha256",
        "io.trillionnium.hepta.runtime-base",
        "vendor_manifest_sha256",
        "image_build_admission_source_sha256",
        "image_builder_source_sha256",
        "evidence_parent_identity",
        'evidence_dir="/proc/$$/fd/$evidence_staging_fd"',
        "compose-default-rendered.json",
        "default-container-inspect.json",
        "default-ready.json",
        "max-ready.json",
        "default_cgroup_path",
        "HEPTA_RESOURCE_GATE_MAX_PEAK_BYTES may only tighten the 384 MiB ceiling",
        "memory_peak_policy_ceiling_bytes",
        "compose_project_absent",
        "teardown_compose_project",
        "cleanup_failed=true",
        'if [[ "$original_status" -eq 0 && "$cleanup_failed" == true ]]',
        "private_scratch_and_tokens_removed",
        "fixture_snapshot_identity",
        "chmod u+rwx -- \"$fixture_dir\"",
        "exit \"$original_status\"",
        "db-baseline-db-rows.json",
        "db-baseline-db-sequences.json",
        "hepta_paper_room_events_cursor_seq",
        "jsonb_agg(to_jsonb(row_value) order by",
        "HEPTA_MIGRATION_DATABASE_URL_FILE",
        "HEPTA_FINALITY_DATABASE_URL",
        "HEPTA_RUNTIME_DATABASE_ROLE",
        "HEPTA_FINALITY_DATABASE_ROLE",
        "hepta_resource_migrator",
        "hepta_resource_runtime",
        "hepta_resource_finality",
        "runtime_role_boundary",
        "finality_role_boundary",
        "definer_public_execute_count",
        "verified_definer_count",
        "has_function_privilege",
        "aclexplode",
        "function.prosecdef",
        "function.proconfig=array['search_path=pg_catalog']::text[]",
        "hepta_assert_paper_finality_v2_source_unsealed(uuid)",
        "hepta_paper_finality_v2_lock_window_arm()",
        "hepta_paper_finality_v2_lock_preparation()",
        "hepta_paper_finality_v2_apply_seal()",
        "hepta_reject_paper_finality_v2_source_mutation()",
        "hepta-migrate",
        "--migrate",
        "--profile migration run --rm --no-deps hepta-migrate",
        "compose.migration.yaml",
        "migration-owner.url",
        'chmod 0444 "$migration_secret_file"',
        'rm -f -- "$migration_secret_file"',
        '[[ ! -e "$migration_secret_file" ]]',
        "chain_time_checkpoint_rows",
        "finality_v2_window_arm_rows",
    ),
)
image_admission_path = repo / "scripts/admit-hepta-image-build-evidence.py"
if (
    image_admission_path.is_symlink()
    or not image_admission_path.is_file()
    or stat.S_IMODE(image_admission_path.stat().st_mode) != 0o755
):
    fail("image-build evidence admission helper must be executable and regular")
image_admission_text = image_admission_path.read_text(encoding="utf-8")
if hashlib.sha256(image_admission_text.encode("utf-8")).hexdigest() != (
    "1481e8c14d20ca15d55c03d0c560530f8e6d152f4c0a4b50ff54f29f76470b1b"
):
    fail("image-build evidence admission helper authority drifted")

admission_hash = "a" * 64
admission_runtime_hash = "b" * 64
admission_revision = "c" * 40
admission_tree = "d" * 40
admission_image_id = "sha256:" + "1" * 64
admission_provenance = {
    "schema": "hepta.release_image_provenance.v3",
    "image_ref": "trnm/hepta:test",
    "image_id": admission_image_id,
    "oci_index_digest": admission_image_id,
    "iid": admission_image_id,
    "source_revision": admission_revision,
    "source_tree": admission_tree,
    "source_date_epoch": 123,
    "buildx": {
        "version": "v0.36.1",
        "binary_sha256": "48af8a397ebd60178778bf63611dbcebe5f5e7a9be90eb9147b24b9587455778",
    },
    "dockerfile_sha256": admission_hash,
    "cargo_lock_sha256": admission_hash,
    "rust_toolchain_sha256": admission_hash,
    "vendor_manifest_sha256": admission_hash,
    "application_sbom": {
        "path": "/usr/share/doc/hepta-research-league/sbom.cdx.json",
        "sha256": admission_hash,
    },
    "runtime_binary": {
        "path": "/usr/local/bin/hepta-research-league",
        "sha256": admission_runtime_hash,
    },
    "reproducibility": {
        "independent_no_cache_builds": 2,
        "identical_image_ids": True,
        "extracted_binaries_identical": True,
        "extracted_sboms_identical": True,
    },
    "compose_postgres_sigkill_smoke": True,
}


def canonical_json(document):
    return json.dumps(document, sort_keys=True, separators=(",", ":")).encode() + b"\n"


def write_private(path, payload):
    path.write_bytes(payload)
    path.chmod(0o600)


def sha256_path(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run_image_admission(name, stdout_payload, mutation=None):
    case = scratch / f"image-admission-{name}"
    case.mkdir(mode=0o700)
    stdout_path = case / "image-build.stdout"
    stderr_path = case / "image-build.stderr"
    staging = case / "staging"
    staging.mkdir(mode=0o700)
    write_private(stdout_path, stdout_payload)
    write_private(stderr_path, b"bounded build diagnostic\n")
    if mutation == "symlink":
        stdout_path.unlink()
        stdout_path.symlink_to(stderr_path.name)
    elif mutation == "hardlink":
        alias = case / "image-build-hardlink.stdout"
        os.link(stdout_path, alias)
        stdout_path = alias
    elif mutation == "fifo":
        stdout_path.unlink()
        os.mkfifo(stdout_path, mode=0o600)
    elif mutation == "oversize":
        with stdout_path.open("r+b") as stream:
            stream.truncate(64 * 1024 * 1024 + 1)
    elif mutation == "same-inode":
        stderr_path = stdout_path
    staging_fd = os.open(staging, os.O_RDONLY | os.O_DIRECTORY)
    try:
        result = subprocess.run(
            [
                sys.executable,
                str(image_admission_path),
                "--stdout",
                str(stdout_path),
                "--stderr",
                str(stderr_path),
                "--repo-dir",
                str(repo),
                "--staging-fd",
                str(staging_fd),
                "--image-ref",
                "trnm/hepta:test",
                "--image-id",
                admission_image_id,
                "--source-revision",
                admission_revision,
                "--source-tree",
                admission_tree,
                "--source-date-epoch",
                "123",
                "--dockerfile-sha256",
                admission_hash,
                "--cargo-lock-sha256",
                admission_hash,
                "--rust-toolchain-sha256",
                admission_hash,
                "--sbom-sha256",
                admission_hash,
                "--vendor-manifest-sha256",
                admission_hash,
                "--runtime-binary-sha256",
                admission_runtime_hash,
            ],
            capture_output=True,
            check=False,
            timeout=5,
            pass_fds=(staging_fd,),
        )
    finally:
        os.close(staging_fd)
    return result, staging


admission_stdout = b"compose smoke: PASS\n" + canonical_json(admission_provenance)
admission_positive, admission_staging = run_image_admission(
    "positive", admission_stdout
)
if admission_positive.returncode != 0:
    fail("image-build evidence admission rejected the canonical fixture")
try:
    admission_summary = json.loads(admission_positive.stdout)
except (UnicodeDecodeError, json.JSONDecodeError):
    fail("image-build evidence admission did not emit canonical JSON")
if admission_summary.get("provenance") != admission_provenance:
    fail("image-build evidence admission provenance summary differs")
if json.loads((admission_staging / "image-provenance.json").read_bytes()) != admission_provenance:
    fail("image-build evidence admission canonical artifact differs")
if (admission_staging / "image-build.stdout").read_bytes() != admission_stdout:
    fail("image-build evidence admission did not preserve raw stdout")

admission_mutations = {
    "duplicate-schema": admission_stdout + canonical_json(admission_provenance),
    "trailing-bytes": admission_stdout + b"not-whitespace\n",
    "bool-as-int": b"compose smoke: PASS\n"
    + canonical_json(
        {
            **admission_provenance,
            "reproducibility": {
                **admission_provenance["reproducibility"],
                "identical_image_ids": 1,
            },
        }
    ),
    "count-as-float": b"compose smoke: PASS\n"
    + canonical_json(
        {
            **admission_provenance,
            "reproducibility": {
                **admission_provenance["reproducibility"],
                "independent_no_cache_builds": 2.0,
            },
        }
    ),
    "wrong-input-hash": b"compose smoke: PASS\n"
    + canonical_json({**admission_provenance, "dockerfile_sha256": "e" * 64}),
}
duplicate_key_json = canonical_json(admission_provenance).replace(
    b'"schema":"hepta.release_image_provenance.v3"',
    b'"schema":"hepta.release_image_provenance.v3","schema":"hepta.release_image_provenance.v3"',
    1,
)
admission_mutations["duplicate-key"] = b"compose smoke: PASS\n" + duplicate_key_json
for mutation_name, payload in admission_mutations.items():
    result, _ = run_image_admission(mutation_name, payload)
    if result.returncode == 0:
        fail(f"image-build evidence admission accepted {mutation_name}")
for mutation_name in ("symlink", "hardlink", "fifo", "oversize", "same-inode"):
    result, _ = run_image_admission(
        mutation_name, admission_stdout, mutation=mutation_name
    )
    if result.returncode == 0:
        fail(f"image-build evidence admission accepted {mutation_name}")

resource_gate_text = (
    repo / "scripts/check-hepta-receipt-v2-resource-gate.sh"
).read_text(encoding="utf-8")
publisher_marker = 'closure_manifest_sha256=$(python3 - \\\n  "$evidence_parent_fd"'
publisher_call = resource_gate_text.find(publisher_marker)
if publisher_call < 0:
    fail("Receipt V2 evidence publisher invocation is missing")
publisher_source_start = resource_gate_text.find("<<'PY'\n", publisher_call)
publisher_source_end = resource_gate_text.find(
    "\nPY\n)\nevidence_published=true", publisher_source_start
)
if publisher_source_start < 0 or publisher_source_end < 0:
    fail("Receipt V2 evidence publisher source boundary drifted")
publisher_source = resource_gate_text[
    publisher_source_start + len("<<'PY'\n") : publisher_source_end
]
compile(publisher_source, "embedded-receipt-v2-evidence-publisher", "exec")


def evidence_identity(metadata):
    return (
        f"{metadata.st_dev}:{metadata.st_ino}:{metadata.st_uid}:"
        f"{metadata.st_gid}:{stat.S_IMODE(metadata.st_mode):o}"
    )


def make_publisher_fixture(name):
    parent = scratch / f"publisher-{name}"
    parent.mkdir(mode=0o700)
    staging = parent / ".published.staging.test"
    staging.mkdir(mode=0o700)
    target_name = "published"
    parent_identity = evidence_identity(parent.stat())
    staging_identity = evidence_identity(staging.stat())
    image_labels = {
        "org.opencontainers.image.revision": admission_revision,
        "org.opencontainers.image.source": "https://github.com/TrillionniumFoundation/CEX.git",
        "org.trillionnium.source.tree": admission_tree,
        "org.trillionnium.sbom.sha256": admission_hash,
        "org.trillionnium.cargo-lock.sha256": admission_hash,
        "org.trillionnium.dockerfile.sha256": admission_hash,
        "org.trillionnium.rust-toolchain.sha256": admission_hash,
        "io.trillionnium.hepta.source-date-epoch": "123",
        "io.trillionnium.hepta.source-tree": admission_tree,
        "io.trillionnium.hepta.application-sbom.path": "/usr/share/doc/hepta-research-league/sbom.cdx.json",
        "io.trillionnium.hepta.application-sbom.sha256": admission_hash,
        "io.trillionnium.hepta.runtime-binary.sha256": admission_runtime_hash,
        "io.trillionnium.hepta.runtime-base": "gcr.io/distroless/cc-debian12@sha256:471dbca9cad607b9a32c10e9c31fb09ffaeb2d460e0afbff86c27abbc80b1b98",
        "io.trillionnium.hepta.builder-base": "docker.io/library/rust@sha256:4c2fd73ef19c5ef9d54bee03b06b2839a392604fbfcd578ed948b71b37c1d7fb",
    }
    payloads = {
        "artifact.txt": b"bound evidence payload\n",
        "compose-default-rendered.json": b"{}\n",
        "compose-max-rendered.json": b"{}\n",
        "default-ready.json": b"{}\n",
        "max-ready.json": b"{}\n",
        "default-container-inspect.json": b"{}\n",
        "max-container-inspect.json": b"{}\n",
        "hepta-image-inspect.json": canonical_json(
            [{"Id": admission_image_id, "Config": {"Labels": image_labels}}]
        ),
        "image-build.stdout": admission_stdout,
        "image-build.stderr": b"bounded build diagnostic\n",
        "image-provenance.json": canonical_json(admission_provenance),
    }
    for artifact_name, payload in payloads.items():
        write_private(staging / artifact_name, payload)
    payload_names = sorted(payloads)
    payload_manifest = "".join(
        f"{sha256_path(staging / artifact)}  {artifact}\n"
        for artifact in payload_names
    ).encode("ascii")
    write_private(staging / "PAYLOAD.SHA256", payload_manifest)
    source_hashes = {
        "gate_source_sha256": admission_hash,
        "generator_source_sha256": admission_hash,
        "image_build_admission_source_sha256": admission_hash,
        "image_builder_source_sha256": admission_hash,
        "clean_source_verifier_sha256": admission_hash,
        "compose_source_sha256": admission_hash,
        "migration_compose_source_sha256": admission_hash,
        "compose_default_rendered_sha256": sha256_path(
            staging / "compose-default-rendered.json"
        ),
        "compose_max_rendered_sha256": sha256_path(
            staging / "compose-max-rendered.json"
        ),
        "image_inspect_sha256": sha256_path(staging / "hepta-image-inspect.json"),
        "default_ready_sha256": sha256_path(staging / "default-ready.json"),
        "max_ready_sha256": sha256_path(staging / "max-ready.json"),
        "default_container_inspect_sha256": sha256_path(
            staging / "default-container-inspect.json"
        ),
        "max_container_inspect_sha256": sha256_path(
            staging / "max-container-inspect.json"
        ),
    }
    committed_inputs = {
        "dockerfile_sha256": admission_hash,
        "cargo_lock_sha256": admission_hash,
        "rust_toolchain_sha256": admission_hash,
        "sbom_sha256": admission_hash,
        "vendor_manifest_sha256": admission_hash,
        "runtime_binary_sha256": admission_runtime_hash,
    }
    expected_contract = {
        "image": "trnm/hepta:test",
        "image_id": admission_image_id,
        "source_revision": admission_revision,
        "source_tree": admission_tree,
        "source_date_epoch": 123,
        "tracked_files": 4,
        "image_build": {
            "stdout_identity": "1:2:3:4:600:1:2:3:4",
            "stderr_identity": "5:6:7:8:600:1:2:3:4",
            "stdout_sha256": sha256_path(staging / "image-build.stdout"),
            "stderr_sha256": sha256_path(staging / "image-build.stderr"),
            "provenance_sha256": sha256_path(staging / "image-provenance.json"),
            "committed_inputs": committed_inputs,
        },
        "provenance": source_hashes,
    }
    summary = {
        "schema": "hepta.receipt_v2.resource_gate_evidence.v3",
        "result": "pass",
        "image": "trnm/hepta:test",
        "image_id": admission_image_id,
        "source_revision": admission_revision,
        "source_tree": admission_tree,
        "source_date_epoch": 123,
        "git_status_clean": True,
        "commit_index_worktree_identical": True,
        "tracked_files": 4,
        "publication": "private_sibling_staging_then_atomic_noreplace_rename",
        "publication_authority": {
            "parent_dev_inode_owner_mode": parent_identity,
            "evidence_dev_inode_owner_mode": staging_identity,
            "target_basename": target_name,
            "retained_dirfds": True,
            "exact_artifact_set_verified": True,
            "manifests_verified": True,
            "post_rename_inode_verified": True,
        },
        "memory_peak_policy_ceiling_bytes": 402653184,
        "enforced_max_peak_bytes": 402653184,
        "provenance": {
            **source_hashes,
            "payload_manifest_sha256": hashlib.sha256(payload_manifest).hexdigest(),
        },
        "image_build": {
            "stdout": {
                "artifact": "image-build.stdout",
                "admitted_source_identity": expected_contract["image_build"]["stdout_identity"],
                "sha256": expected_contract["image_build"]["stdout_sha256"],
            },
            "stderr": {
                "artifact": "image-build.stderr",
                "admitted_source_identity": expected_contract["image_build"]["stderr_identity"],
                "sha256": expected_contract["image_build"]["stderr_sha256"],
            },
            "canonical_provenance": {
                "artifact": "image-provenance.json",
                "sha256": expected_contract["image_build"]["provenance_sha256"],
                "document": admission_provenance,
            },
            "committed_inputs": committed_inputs,
        },
        "teardown": {
            "compose_project_absent": True,
            "named_volumes_absent": True,
            "private_scratch_and_tokens_removed": True,
        },
        "phases": {
            "canonical_default": {"readiness_evidence": "default-ready.json"},
            "deployment_max_override": {"readiness_evidence": "max-ready.json"},
        },
    }
    write_private(
        staging / "summary.json",
        json.dumps(summary, sort_keys=True, separators=(",", ":")).encode("utf-8"),
    )
    sha_entries = sorted(payload_names + ["PAYLOAD.SHA256", "summary.json"])
    sha_manifest = "".join(
        f"{sha256_path(staging / entry)}  {entry}\n" for entry in sha_entries
    ).encode("ascii")
    write_private(staging / "SHA256SUMS", sha_manifest)
    closure_manifest_sha256 = sha256_path(staging / "SHA256SUMS")
    parent_fd = os.open(parent, os.O_RDONLY | os.O_DIRECTORY)
    staging_fd = os.open(staging, os.O_RDONLY | os.O_DIRECTORY)
    return {
        "parent": parent,
        "staging": staging,
        "target_name": target_name,
        "payload_names": payload_names,
        "parent_identity": parent_identity,
        "staging_identity": staging_identity,
        "parent_fd": parent_fd,
        "staging_fd": staging_fd,
        "closure_manifest_sha256": closure_manifest_sha256,
        "expected_contract": expected_contract,
    }


def run_publisher(fixture):
    arguments = [
        sys.executable,
        "-",
        str(fixture["parent_fd"]),
        str(fixture["staging_fd"]),
        str(fixture["parent"]),
        fixture["staging"].name,
        fixture["target_name"],
        fixture["parent_identity"],
        fixture["staging_identity"],
        str(os.geteuid()),
        fixture["closure_manifest_sha256"],
        json.dumps(
            fixture["expected_contract"], sort_keys=True, separators=(",", ":")
        ),
        *fixture["payload_names"],
    ]
    return subprocess.run(
        arguments,
        input=publisher_source.encode("utf-8"),
        capture_output=True,
        check=False,
        timeout=5,
        pass_fds=(fixture["parent_fd"], fixture["staging_fd"]),
    )


def close_publisher_fixture(fixture):
    os.close(fixture["staging_fd"])
    os.close(fixture["parent_fd"])


positive_publisher = make_publisher_fixture("positive")
positive_staging_stat = positive_publisher["staging"].stat()
try:
    positive_result = run_publisher(positive_publisher)
finally:
    close_publisher_fixture(positive_publisher)
if positive_result.returncode != 0:
    fail(
        "Receipt V2 evidence publisher positive fixture failed: "
        + positive_result.stderr.decode("utf-8", errors="replace")
    )
if positive_result.stdout != (
    positive_publisher["closure_manifest_sha256"].encode("ascii") + b"\n"
):
    fail("Receipt V2 evidence publisher emitted the wrong closure digest")
positive_target = positive_publisher["parent"] / positive_publisher["target_name"]
if (
    not positive_target.is_dir()
    or positive_publisher["staging"].exists()
    or positive_target.stat().st_dev != positive_staging_stat.st_dev
    or positive_target.stat().st_ino != positive_staging_stat.st_ino
):
    fail("Receipt V2 evidence publisher did not preserve the staging inode")

for mutation in ("fifo", "symlink", "extra", "digest", "target"):
    fixture = make_publisher_fixture(mutation)
    artifact = fixture["staging"] / fixture["payload_names"][0]
    if mutation == "fifo":
        artifact.unlink()
        os.mkfifo(artifact, mode=0o600)
    elif mutation == "symlink":
        artifact.unlink()
        artifact.symlink_to("summary.json")
    elif mutation == "extra":
        write_private(fixture["staging"] / "unbound.txt", b"not manifested\n")
    elif mutation == "digest":
        write_private(artifact, b"tampered after manifest\n")
    elif mutation == "target":
        (fixture["parent"] / fixture["target_name"]).mkdir(mode=0o700)
    try:
        result = run_publisher(fixture)
    finally:
        close_publisher_fixture(fixture)
    if result.returncode == 0:
        fail(f"Receipt V2 evidence publisher accepted negative fixture: {mutation}")

contract_mismatch = make_publisher_fixture("contract-mismatch")
contract_mismatch["expected_contract"]["image_id"] = "sha256:" + "2" * 64
try:
    contract_mismatch_result = run_publisher(contract_mismatch)
finally:
    close_publisher_fixture(contract_mismatch)
if contract_mismatch_result.returncode == 0:
    fail("Receipt V2 evidence publisher accepted a mismatched publication contract")

wrong_closure = make_publisher_fixture("wrong-closure")
wrong_closure_staging_stat = wrong_closure["staging"].stat()
wrong_closure["closure_manifest_sha256"] = "f" * 64
try:
    wrong_closure_result = run_publisher(wrong_closure)
finally:
    close_publisher_fixture(wrong_closure)
wrong_closure_target = wrong_closure["parent"] / wrong_closure["target_name"]
if (
    wrong_closure_result.returncode == 0
    or wrong_closure_target.exists()
    or not wrong_closure["staging"].is_dir()
    or wrong_closure["staging"].stat().st_dev != wrong_closure_staging_stat.st_dev
    or wrong_closure["staging"].stat().st_ino != wrong_closure_staging_stat.st_ino
):
    fail("Receipt V2 evidence publisher did not roll back a closure digest mismatch")

staging_swap = make_publisher_fixture("staging-swap")
held_staging = staging_swap["parent"] / ".published.staging.held"
staging_swap["staging"].rename(held_staging)
staging_swap["staging"].mkdir(mode=0o700)
try:
    staging_swap_result = run_publisher(staging_swap)
finally:
    close_publisher_fixture(staging_swap)
if staging_swap_result.returncode == 0:
    fail("Receipt V2 evidence publisher accepted a replaced staging path")

parent_swap = make_publisher_fixture("parent-swap")
held_parent = parent_swap["parent"].with_name(parent_swap["parent"].name + "-held")
parent_swap["parent"].rename(held_parent)
parent_swap["parent"].mkdir(mode=0o700)
try:
    parent_swap_result = run_publisher(parent_swap)
finally:
    close_publisher_fixture(parent_swap)
if parent_swap_result.returncode == 0:
    fail("Receipt V2 evidence publisher accepted a replaced parent path")

override_match = re.search(
    r'cat >"\$override" <<EOF\n(?P<yaml>services:\n.*?\nvolumes:\n  pgdata: \{\})\nEOF',
    resource_gate_text,
    flags=re.DOTALL,
)
if override_match is None:
    fail("Receipt V2 resource gate generated override YAML authority drifted")


class UniqueKeyLoader(yaml.SafeLoader):
    pass


def construct_unique_mapping(loader, node, deep=False):
    result = {}
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node, deep=deep)
        if key in result:
            fail(f"Receipt V2 resource override contains duplicate YAML key: {key}")
        result[key] = loader.construct_object(value_node, deep=deep)
    return result


UniqueKeyLoader.add_constructor(
    yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG,
    construct_unique_mapping,
)
try:
    yaml.load("services: {}\nservices: {}\n", Loader=UniqueKeyLoader)
except AssertionError as error:
    if "duplicate YAML key" not in str(error):
        raise
else:
    fail("duplicate-reject YAML loader accepted a duplicate canonical Compose key")
resource_override = yaml.load(override_match.group("yaml"), Loader=UniqueKeyLoader)
if set(resource_override) != {"services", "volumes"}:
    fail("Receipt V2 resource override top-level keys drifted")
resource_services = resource_override.get("services", {})
if set(resource_services) != {"postgres", "hepta"}:
    fail("Receipt V2 resource override service keys drifted")
if resource_services["postgres"].get("environment") != {
    "POSTGRES_USER": "hepta_resource_migrator",
    "POSTGRES_PASSWORD": "hepta_resource_migrator_password",
    "POSTGRES_DB": "hepta_resource",
}:
    fail("Receipt V2 resource override PostgreSQL environment drifted")
if resource_services["hepta"].get("environment") != {
    "HEPTA_DATABASE_URL": "postgres://hepta_resource_runtime:hepta_resource_runtime_password@postgres:5432/hepta_resource",
    "HEPTA_FINALITY_DATABASE_URL": "postgres://hepta_resource_finality:hepta_resource_finality_password@postgres:5432/hepta_resource",
}:
    fail("Receipt V2 resource override Hepta environment drifted")
compose_smoke_text = (
    repo / "scripts/check-hepta-research-league-compose-smoke.sh"
).read_text(encoding="utf-8")
constraint_catalog_sha256 = (
    "910d4454106f5722ad44c6c9095bf48d"
    "585dfaa9501fc40d9ef377fd57c3f3ba"
)
for relative in (
    "migrations/0038_add_hepta_paper_chain_finality_v2.sql",
    "services/hepta-research-league/src/lib.rs",
    "services/hepta-research-league/src/paper_raid_v2_tests.rs",
    "scripts/check-hepta-research-league-compose-smoke.sh",
):
    text = (repo / relative).read_text(encoding="utf-8")
    if constraint_catalog_sha256 not in text:
        fail(f"Paper finality V2 constraint fingerprint drifted in {relative}")
if compose_smoke_text.index("started=true") > compose_smoke_text.index(
    '"${compose[@]}" up -d postgres'
):
    fail("Compose cleanup is not armed before partial startup")

compose_text = (repo / "deploy/hepta-research-league/compose.yaml").read_text(encoding="utf-8")
for fragment in (
    'image: ${HEPTA_IMAGE:?immutable HEPTA_IMAGE digest is required}',
    "pull_policy: never",
    '127.0.0.1:${HEPTA_HOST_PORT:-7011}:7011',
    "HEPTA_TRNM_COMETBFT_TRUST_ANCHOR_HASHES_JSON",
    "HEPTA_TRNM_RECEIPT_V2_MAX_BODY_BYTES",
    "HEPTA_TRNM_RECEIPT_V2_MAX_IN_FLIGHT",
    "HEPTA_FINALITY_DATABASE_URL",
):
    if fragment not in compose_text:
        fail(f"Compose release contract is missing {fragment!r}")
migration_compose_text = (
    repo / "deploy/hepta-research-league/compose.migration.yaml"
).read_text(encoding="utf-8")
for fragment in (
    'image: ${HEPTA_IMAGE:?immutable HEPTA_IMAGE digest is required}',
    "HEPTA_MIGRATION_DATABASE_URL_FILE",
    "HEPTA_RUNTIME_DATABASE_ROLE",
    "HEPTA_FINALITY_DATABASE_ROLE",
    "hepta-migrate",
    'command: ["--migrate"]',
):
    if fragment not in migration_compose_text:
        fail(f"migration-only Compose contract is missing {fragment!r}")
try:
    compose_document = yaml.load(compose_text, Loader=UniqueKeyLoader)
    migration_compose_document = yaml.load(
        migration_compose_text, Loader=UniqueKeyLoader
    )
except yaml.YAMLError as error:
    fail(f"Compose release contract is invalid YAML: {error}")
if not isinstance(compose_document, dict) or not isinstance(
    migration_compose_document, dict
):
    fail("Compose release contract is not an object")
hepta_compose = compose_document.get("services", {}).get("hepta")
if not isinstance(hepta_compose, dict):
    fail("Compose release contract has no Hepta service")
if "hepta-migrate" in compose_document.get("services", {}) or "secrets" in compose_document:
    fail("resident Compose contract must have no migrator service or owner secret")
hepta_migrate_compose = migration_compose_document.get("services", {}).get("hepta-migrate")
if not isinstance(hepta_migrate_compose, dict):
    fail("Compose release contract has no one-shot migration service")
if hepta_compose.get("image") != "${HEPTA_IMAGE:?immutable HEPTA_IMAGE digest is required}":
    fail("Compose Hepta image authority drifted")
if hepta_compose.get("pull_policy") != "never" or "build" in hepta_compose:
    fail("Compose Hepta service must use only the frozen local image")
if hepta_compose.get("ports") != ["127.0.0.1:${HEPTA_HOST_PORT:-7011}:7011"]:
    fail("Compose Hepta host binding drifted")
if {
    "HEPTA_MIGRATION_DATABASE_URL",
    "HEPTA_MIGRATION_DATABASE_URL_FILE",
} & set(hepta_compose.get("environment", {})):
    fail("resident Compose Hepta service exposes the migration-owner credential")
if hepta_compose.get("environment", {}).get(
    "HEPTA_FINALITY_DATABASE_URL"
) != "${HEPTA_FINALITY_DATABASE_URL:?isolated finality-writer URL required}":
    fail("resident Compose Hepta service must receive the isolated finality-writer URL")
if "depends_on" in hepta_compose:
    fail("resident Compose Hepta must not retain or restart the migration profile")
if hepta_migrate_compose.get("image") != "${HEPTA_IMAGE:?immutable HEPTA_IMAGE digest is required}":
    fail("Compose migration job image authority drifted")
if hepta_migrate_compose.get("pull_policy") != "never" or "build" in hepta_migrate_compose:
    fail("Compose migration job must use only the frozen local image")
if hepta_migrate_compose.get("restart") != "no":
    fail("Compose migration job must be one-shot")
if hepta_migrate_compose.get("profiles") != ["migration"]:
    fail("Compose migration job must be isolated behind the migration profile")
if hepta_migrate_compose.get("command") != ["--migrate"]:
    fail("Compose migration job must invoke the binary migration mode")
if hepta_migrate_compose.get("environment") != {
    "HEPTA_MIGRATION_DATABASE_URL_FILE": "/run/secrets/hepta_migration_database_url",
    "HEPTA_RUNTIME_DATABASE_ROLE": "${HEPTA_RUNTIME_DATABASE_ROLE:?non-owner runtime database role required}",
    "HEPTA_FINALITY_DATABASE_ROLE": "${HEPTA_FINALITY_DATABASE_ROLE:?isolated finality-writer database role required}",
}:
    fail("Compose migration job environment exceeds its three-variable authority")
if hepta_migrate_compose.get("secrets") != [
    {"source": "hepta_migration_database_url", "target": "hepta_migration_database_url"}
]:
    fail("Compose migration job must receive the owner URL only as a file secret")
if migration_compose_document.get("secrets") != {
    "hepta_migration_database_url": {
        "file": "${HEPTA_MIGRATION_DATABASE_URL_FILE:?host path to the migration-owner URL secret is required}"
    }
}:
    fail("Compose migration owner secret-file authority drifted")
if hepta_migrate_compose.get("read_only") is not True:
    fail("Compose migration job rootfs must be read-only")
if hepta_migrate_compose.get("cap_drop") != ["ALL"]:
    fail("Compose migration job capabilities are not closed")
if hepta_compose.get("environment", {}).get(
    "HEPTA_TRNM_COMETBFT_TRUST_ANCHOR_HASHES_JSON"
) != "${HEPTA_TRNM_COMETBFT_TRUST_ANCHOR_HASHES_JSON:?Receipt V2 pinned trust-anchor hashes required}":
    fail("Compose must require and forward the Receipt V2 trust-anchor pin ring")
if hepta_compose.get("environment", {}).get(
    "HEPTA_TRNM_RECEIPT_V2_MAX_BODY_BYTES"
) != "${HEPTA_TRNM_RECEIPT_V2_MAX_BODY_BYTES:-32768}":
    fail("Compose must forward the conservative Receipt V2 ingress byte cap")
if hepta_compose.get("environment", {}).get(
    "HEPTA_TRNM_RECEIPT_V2_MAX_IN_FLIGHT"
) != "${HEPTA_TRNM_RECEIPT_V2_MAX_IN_FLIGHT:-1}":
    fail("Compose must forward the bounded Receipt V2 verification concurrency")
if hepta_compose.get("read_only") is not True:
    fail("Compose Hepta rootfs must be read-only")
if hepta_compose.get("cap_drop") != ["ALL"]:
    fail("Compose Hepta capabilities are not closed")
if hepta_compose.get("healthcheck", {}).get("test") != [
    "CMD",
    "/usr/local/bin/hepta-research-league",
    "--probe-ready",
]:
    fail("Compose Hepta healthcheck is not binary-authoritative")


def run(command, expect_success=True):
    completed = subprocess.run(
        command,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    if expect_success and completed.returncode != 0:
        fail(
            f"command failed unexpectedly ({completed.returncode}): {' '.join(map(str, command))}\n"
            f"stdout={completed.stdout}\nstderr={completed.stderr}"
        )
    if not expect_success and completed.returncode == 0:
        fail(f"negative command was incorrectly accepted: {' '.join(map(str, command))}")
    return completed


downloader = repo / "scripts/download-pinned-buildx.sh"
fake_buildx = scratch / "fake-buildx"
fake_buildx.write_bytes(b"pinned-buildx-fixture\n")
fake_buildx.chmod(0o755)
fake_buildx_sha256 = hashlib.sha256(fake_buildx.read_bytes()).hexdigest()
run(
    [
        "bash",
        downloader,
        "https://example.invalid/pinned-buildx",
        fake_buildx_sha256,
        scratch / "downloaded-buildx",
        fake_buildx,
    ]
)
run(
    [
        "bash",
        downloader,
        "https://example.invalid/pinned-buildx",
        "0" * 64,
        scratch / "bad-digest-buildx",
        fake_buildx,
    ],
    expect_success=False,
)
fake_buildx_link = scratch / "fake-buildx-link"
fake_buildx_link.symlink_to(fake_buildx.name)
run(
    [
        "bash",
        downloader,
        "https://example.invalid/pinned-buildx",
        fake_buildx_sha256,
        scratch / "symlink-source-buildx",
        fake_buildx_link,
    ],
    expect_success=False,
)
run(
    [
        "bash",
        downloader,
        "http://example.invalid/unpinned-buildx",
        fake_buildx_sha256,
        scratch / "insecure-url-buildx",
        fake_buildx,
    ],
    expect_success=False,
)


fixture = scratch / "fixture"
fixture.mkdir()
metadata_path = fixture / "cargo-metadata.json"
dockerfile_fixture = fixture / "Dockerfile"
cargo_lock_fixture = fixture / "Cargo.lock"
toolchain_fixture = fixture / "rust-toolchain.manifest"
runtime_fixture = fixture / "hepta-research-league"
dockerfile_fixture.write_text("FROM scratch\n", encoding="utf-8")
cargo_lock_fixture.write_text("version = 4\n", encoding="utf-8")
toolchain_fixture.write_text("rustc=fake pinned toolchain\n", encoding="utf-8")
runtime_fixture.write_bytes(b"ELF-fixture-hepta-runtime\n")
runtime_fixture.chmod(0o755)
root_id = "path+file:///workspace/services/hepta-research-league#0.1.0"
dep_id = "registry+https://github.com/rust-lang/crates.io-index#serde@1.0.0"
metadata_path.write_text(
    json.dumps(
        {
            "workspace_root": "/workspace",
            "workspace_members": [root_id],
            "packages": [
                {
                    "id": root_id,
                    "name": "hepta-research-league",
                    "version": "0.1.0",
                    "source": None,
                    "manifest_path": "/workspace/services/hepta-research-league/Cargo.toml",
                    "license": "MIT",
                },
                {
                    "id": dep_id,
                    "name": "serde",
                    "version": "1.0.0",
                    "source": "registry+https://github.com/rust-lang/crates.io-index",
                    "manifest_path": "/cargo/registry/serde/Cargo.toml",
                    "license": "MIT OR Apache-2.0",
                },
            ],
            "resolve": {
                "nodes": [
                    {"id": root_id, "deps": [{"pkg": dep_id}]},
                    {"id": dep_id, "deps": []},
                ]
            },
        },
        separators=(",", ":"),
    ),
    encoding="utf-8",
)

generator = repo / "scripts/generate-hepta-research-league-sbom.py"
verifier = repo / "scripts/verify-hepta-research-league-sbom.py"


def generator_command(output, runtime_args=None, dockerfile_arg=None):
    if runtime_args is None:
        runtime_args = ["--runtime-binary", runtime_fixture]
    return [
        sys.executable,
        generator,
        "--metadata",
        metadata_path,
        *runtime_args,
        "--dockerfile",
        dockerfile_arg or dockerfile_fixture,
        "--cargo-lock",
        cargo_lock_fixture,
        "--rust-toolchain",
        toolchain_fixture,
        "--output",
        output,
    ]


first_sbom = fixture / "first.cdx.json"
second_sbom = fixture / "second.cdx.json"
run(generator_command(first_sbom))
run(generator_command(second_sbom))
if first_sbom.read_bytes() != second_sbom.read_bytes():
    fail("SBOM generator is not byte deterministic")
runtime_sha256 = hashlib.sha256(runtime_fixture.read_bytes()).hexdigest()


def verifier_command(sbom=first_sbom, runtime_sha=runtime_sha256, dockerfile_arg=None):
    return [
        sys.executable,
        verifier,
        "--sbom",
        sbom,
        "--runtime-sha256",
        runtime_sha,
        "--dockerfile",
        dockerfile_arg or dockerfile_fixture,
        "--cargo-lock",
        cargo_lock_fixture,
        "--rust-toolchain",
        toolchain_fixture,
    ]


run(verifier_command())
document = json.loads(first_sbom.read_text(encoding="utf-8"))
expected_properties = [
    {
        "name": "trnm:cargo-lock:sha256",
        "value": "sha256:" + hashlib.sha256(cargo_lock_fixture.read_bytes()).hexdigest(),
    },
    {
        "name": "trnm:dockerfile:sha256",
        "value": "sha256:" + hashlib.sha256(dockerfile_fixture.read_bytes()).hexdigest(),
    },
    {
        "name": "trnm:rust-toolchain:sha256",
        "value": "sha256:" + hashlib.sha256(toolchain_fixture.read_bytes()).hexdigest(),
    },
]
if document["metadata"]["properties"] != expected_properties:
    fail("generated SBOM properties are not exact and ordered")
runtime_components = [item for item in document["components"] if item.get("type") == "file"]
if len(runtime_components) != 1 or runtime_components[0]["name"] != "/usr/local/bin/hepta-research-league":
    fail("generated SBOM does not contain exactly one canonical runtime file")


def reject_mutation(name, mutate):
    candidate = json.loads(first_sbom.read_text(encoding="utf-8"))
    mutate(candidate)
    path = fixture / f"negative-{name}.json"
    path.write_text(json.dumps(candidate, separators=(",", ":")), encoding="utf-8")
    run(verifier_command(sbom=path), expect_success=False)


reject_mutation(
    "extra-property",
    lambda value: value["metadata"]["properties"].append(
        {"name": "trnm:unexpected", "value": "sha256:" + "0" * 64}
    ),
)
reject_mutation(
    "extra-file",
    lambda value: value["components"].append(dict(runtime_components[0])),
)
reject_mutation(
    "wrong-prefix",
    lambda value: value["metadata"]["properties"][0].update(
        {"value": value["metadata"]["properties"][0]["value"].removeprefix("sha256:")}
    ),
)
reject_mutation(
    "wrong-runtime",
    lambda value: value["components"][-1]["hashes"][0].update({"content": "0" * 64}),
)
reject_mutation(
    "wrong-root-ref",
    lambda value: value["metadata"]["component"].update(
        {"bom-ref": "urn:cdx:cargo:" + "0" * 64}
    ),
)
reject_mutation(
    "wrong-generator",
    lambda value: value["metadata"]["tools"]["components"][0].update(
        {"version": "untrusted"}
    ),
)
reject_mutation(
    "wrong-library-purl",
    lambda value: value["components"][0].update({"purl": "pkg:cargo/forged@9"}),
)
reject_mutation(
    "orphan-graph",
    lambda value: next(
        item
        for item in value["dependencies"]
        if item["ref"] == document["metadata"]["component"]["bom-ref"]
    )["dependsOn"].clear(),
)
reject_mutation(
    "duplicate-dependency-ref",
    lambda value: value["dependencies"].insert(0, dict(value["dependencies"][0])),
)

changed_dockerfile = fixture / "Dockerfile.changed"
changed_dockerfile.write_text("FROM scratch\n# drift\n", encoding="utf-8")
run(verifier_command(dockerfile_arg=changed_dockerfile), expect_success=False)
run(verifier_command(runtime_sha="not-a-sha256"), expect_success=False)

runtime_fixture.chmod(0o644)
run(generator_command(fixture / "nonexec.json"), expect_success=False)
runtime_fixture.chmod(0o755)
runtime_link = fixture / "runtime-link"
runtime_link.symlink_to(runtime_fixture.name)
run(
    generator_command(
        fixture / "symlink-runtime.json",
        runtime_args=["--runtime-binary", runtime_link],
    ),
    expect_success=False,
)
run(
    generator_command(
        fixture / "bad-runtime-sha.json",
        runtime_args=["--runtime-sha256", "ABC"],
    ),
    expect_success=False,
)
output_target = fixture / "output-target.json"
output_target.write_text("do not overwrite through symlink", encoding="utf-8")
output_link = fixture / "output-link.json"
output_link.symlink_to(output_target.name)
run(generator_command(output_link), expect_success=False)
dockerfile_link = fixture / "Dockerfile.link"
dockerfile_link.symlink_to(dockerfile_fixture.name)
run(verifier_command(dockerfile_arg=dockerfile_link), expect_success=False)

print("Hepta static release structure and negative gates: PASS")
PY
