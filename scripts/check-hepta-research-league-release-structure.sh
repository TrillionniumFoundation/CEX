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
