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

for command_name in bash cmp python3 sha256sum; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "Hepta release structure gate requires $command_name" >&2
    exit 1
  }
done

shell_scripts=(
  scripts/build-hepta-research-league-image.sh
  scripts/check-hepta-research-league-compose-smoke.sh
  scripts/check-hepta-research-league-release.sh
  scripts/check-hepta-research-league-release-structure.sh
  scripts/download-pinned-buildx.sh
  scripts/generate-hepta-research-league-docker-lock.sh
  scripts/generate-hepta-research-league-runtime-sbom.sh
)
python_scripts=(
  scripts/check-hepta-route-openapi-parity.py
  scripts/generate-hepta-research-league-sbom.py
  scripts/verify-hepta-research-league-rootfs-tar.py
  scripts/verify-hepta-research-league-sbom.py
)
for relative_path in "${shell_scripts[@]}"; do
  bash -n "$repo_dir/$relative_path"
done
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
    "vendor/trnm-research-protocol/Cargo.toml",
    "vendor/trnm-research-protocol/src",
    "migrations/0031_add_hepta_research_league.sql",
    "migrations/0032_add_hepta_paper_raid_v2.sql",
    "migrations/0033_add_hepta_paper_collaboration_kernel.sql",
    "migrations/0034_add_hepta_paper_review_appeal.sql",
    "migrations/0035_add_hepta_secure_onboarding.sql",
    "migrations/0036_add_hepta_nakama_research_control.sql",
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
    blocks = stage_blocks(text)
    identities = [(base, name) for base, name, _ in blocks]
    expected_identities = [
        (
            "docker.io/library/rust@sha256:4c2fd73ef19c5ef9d54bee03b06b2839a392604fbfcd578ed948b71b37c1d7fb",
            "workspace",
        ),
        ("workspace", "lockfile-generator"),
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
    lockfile_generator = blocks[1][2]
    lockfile_export = blocks[2][2]
    builder = blocks[3][2]
    runtime_export = blocks[4][2]
    metadata_export = blocks[5][2]
    release = blocks[6][2]
    final = blocks[7][2]
    lockfile_generator_instructions = [
        line.strip()
        for line in lockfile_generator.splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    ]
    if lockfile_generator_instructions[0] != "FROM workspace AS lockfile-generator":
        fail("lockfile generator stage identity drifted")
    lockfile_heads = re.findall(
        r"(?mi)^(FROM|RUN|COPY|ADD|ARG|ENV|WORKDIR|LABEL|USER|ENTRYPOINT|CMD|HEALTHCHECK)\b",
        lockfile_generator,
    )
    if lockfile_heads != ["FROM", "RUN"]:
        fail("lockfile generator must contain exactly one RUN instruction")
    if "cargo generate-lockfile" not in lockfile_generator:
        fail("lockfile generator does not use pinned Cargo authority")
    lockfile_export_instructions = [
        line.strip()
        for line in lockfile_export.splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    ]
    if lockfile_export_instructions != [
        "FROM scratch AS cargo-lock-export",
        "COPY --from=lockfile-generator /src/Cargo.lock /Cargo.lock",
    ]:
        fail("cargo-lock-export must export exactly the generated Docker lock")
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
        dockerfile.replace("cargo generate-lockfile", "cargo update", 1),
        "pinned Cargo authority",
    ),
    (
        dockerfile.replace(
            "COPY services/hepta-research-league/docker/Cargo.lock Cargo.lock",
            "COPY Cargo.lock Cargo.lock",
            1,
        ),
        "dedicated Docker lock",
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
        "write_status=",
        'cmp "$tracked_lock" "$first_lock"',
    ),
)
if lock_script.count("verify_source_unchanged") < 5:
    fail("Docker-lock generation lacks repeated TOCTOU checks")

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
        '.finality_mode == "pending_only"',
        ".trusted_validator_sets == 0",
    ),
)
compose_smoke_text = (
    repo / "scripts/check-hepta-research-league-compose-smoke.sh"
).read_text(encoding="utf-8")
if compose_smoke_text.index("started=true") > compose_smoke_text.index(
    '"${compose[@]}" up -d postgres'
):
    fail("Compose cleanup is not armed before partial startup")

compose_text = (repo / "deploy/hepta-research-league/compose.yaml").read_text(encoding="utf-8")
for fragment in (
    'image: ${HEPTA_IMAGE:?immutable HEPTA_IMAGE digest is required}',
    "pull_policy: never",
    '127.0.0.1:${HEPTA_HOST_PORT:-7011}:7011',
):
    if fragment not in compose_text:
        fail(f"Compose release contract is missing {fragment!r}")
try:
    compose_document = yaml.safe_load(compose_text)
except yaml.YAMLError as error:
    fail(f"Compose release contract is invalid YAML: {error}")
if not isinstance(compose_document, dict):
    fail("Compose release contract is not an object")
hepta_compose = compose_document.get("services", {}).get("hepta")
if not isinstance(hepta_compose, dict):
    fail("Compose release contract has no Hepta service")
if hepta_compose.get("image") != "${HEPTA_IMAGE:?immutable HEPTA_IMAGE digest is required}":
    fail("Compose Hepta image authority drifted")
if hepta_compose.get("pull_policy") != "never" or "build" in hepta_compose:
    fail("Compose Hepta service must use only the frozen local image")
if hepta_compose.get("ports") != ["127.0.0.1:${HEPTA_HOST_PORT:-7011}:7011"]:
    fail("Compose Hepta host binding drifted")
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
