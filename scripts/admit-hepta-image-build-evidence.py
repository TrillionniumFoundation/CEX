#!/usr/bin/env python3
"""Snapshot and validate immutable Hepta image-build evidence."""

import argparse
import hashlib
import json
import os
import pathlib
import re
import stat
import sys


MAX_LOG_BYTES = 64 * 1024 * 1024
PROVENANCE_SCHEMA = "hepta.release_image_provenance.v3"
SHA256 = re.compile(r"^[0-9a-f]{64}$")
DOCKER_DIGEST = re.compile(r"^sha256:[0-9a-f]{64}$")
GIT_ID = re.compile(r"^[0-9a-f]{40}$")


class DuplicateKeyError(ValueError):
    pass


def reject_duplicate_keys(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise DuplicateKeyError(f"duplicate JSON member: {key}")
        result[key] = value
    return result


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--stdout", required=True)
    parser.add_argument("--stderr", required=True)
    parser.add_argument("--repo-dir", required=True)
    parser.add_argument("--staging-fd", type=int, required=True)
    parser.add_argument("--image-ref", required=True)
    parser.add_argument("--image-id", required=True)
    parser.add_argument("--source-revision", required=True)
    parser.add_argument("--source-tree", required=True)
    parser.add_argument("--source-date-epoch", type=int, required=True)
    parser.add_argument("--dockerfile-sha256", required=True)
    parser.add_argument("--cargo-lock-sha256", required=True)
    parser.add_argument("--rust-toolchain-sha256", required=True)
    parser.add_argument("--sbom-sha256", required=True)
    parser.add_argument("--vendor-manifest-sha256", required=True)
    parser.add_argument("--runtime-binary-sha256", required=True)
    return parser.parse_args()


def file_identity(metadata):
    return (
        metadata.st_dev,
        metadata.st_ino,
        metadata.st_uid,
        metadata.st_gid,
        stat.S_IMODE(metadata.st_mode),
        metadata.st_nlink,
        metadata.st_size,
        metadata.st_mtime_ns,
        metadata.st_ctime_ns,
    )


def identity_text(metadata):
    return ":".join(str(value) for value in file_identity(metadata))


def admit_path(raw_path, repo_dir, label):
    path = pathlib.Path(raw_path)
    if not path.is_absolute() or os.path.normpath(raw_path) != raw_path:
        raise RuntimeError(f"{label} must be an absolute normalized path")
    resolved = path.resolve(strict=True)
    if str(resolved) != raw_path:
        raise RuntimeError(f"{label} must not traverse symlinks or aliases")
    try:
        if os.path.commonpath((str(repo_dir), raw_path)) == str(repo_dir):
            raise RuntimeError(f"{label} must be outside the Git repository")
    except ValueError as error:
        raise RuntimeError(f"{label} path authority is invalid") from error
    parent = resolved.parent.stat()
    if (
        not stat.S_ISDIR(parent.st_mode)
        or parent.st_uid != os.geteuid()
        or stat.S_IMODE(parent.st_mode) & 0o022
    ):
        raise RuntimeError(f"{label} parent ownership or mode is unsafe")
    path_metadata = os.stat(raw_path, follow_symlinks=False)
    if (
        not stat.S_ISREG(path_metadata.st_mode)
        or path_metadata.st_nlink != 1
        or path_metadata.st_uid != os.geteuid()
        or stat.S_IMODE(path_metadata.st_mode) & 0o022
        or path_metadata.st_size > MAX_LOG_BYTES
    ):
        raise RuntimeError(f"{label} must be a bounded caller-owned regular file")
    flags = os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK
    descriptor = os.open(raw_path, flags)
    metadata = os.fstat(descriptor)
    if (
        not stat.S_ISREG(metadata.st_mode)
        or metadata.st_nlink != 1
        or metadata.st_uid != os.geteuid()
        or stat.S_IMODE(metadata.st_mode) & 0o022
    ):
        os.close(descriptor)
        raise RuntimeError(f"{label} must be a caller-owned single-linked regular file")
    if metadata.st_size > MAX_LOG_BYTES:
        os.close(descriptor)
        raise RuntimeError(f"{label} exceeds the 64 MiB admission ceiling")
    if file_identity(metadata) != file_identity(path_metadata):
        os.close(descriptor)
        raise RuntimeError(f"{label} was replaced during admission")
    return descriptor, metadata


def read_stable(descriptor, before, label):
    chunks = []
    total = 0
    while True:
        chunk = os.read(descriptor, min(1024 * 1024, MAX_LOG_BYTES + 1 - total))
        if not chunk:
            break
        chunks.append(chunk)
        total += len(chunk)
        if total > MAX_LOG_BYTES:
            raise RuntimeError(f"{label} grew beyond the 64 MiB admission ceiling")
    after = os.fstat(descriptor)
    if file_identity(after) != file_identity(before) or total != before.st_size:
        raise RuntimeError(f"{label} changed while it was being admitted")
    return b"".join(chunks)


def require_exact_object(value, keys, label):
    if type(value) is not dict or set(value) != set(keys):
        raise RuntimeError(f"{label} shape differs")


def validate_provenance(provenance, expected):
    require_exact_object(
        provenance,
        (
            "schema",
            "image_ref",
            "image_id",
            "oci_index_digest",
            "iid",
            "source_revision",
            "source_tree",
            "source_date_epoch",
            "buildx",
            "dockerfile_sha256",
            "cargo_lock_sha256",
            "rust_toolchain_sha256",
            "vendor_manifest_sha256",
            "application_sbom",
            "runtime_binary",
            "reproducibility",
            "compose_postgres_sigkill_smoke",
        ),
        "image provenance",
    )
    require_exact_object(provenance["buildx"], ("version", "binary_sha256"), "buildx")
    require_exact_object(provenance["application_sbom"], ("path", "sha256"), "SBOM")
    require_exact_object(provenance["runtime_binary"], ("path", "sha256"), "runtime binary")
    require_exact_object(
        provenance["reproducibility"],
        (
            "independent_no_cache_builds",
            "identical_image_ids",
            "extracted_binaries_identical",
            "extracted_sboms_identical",
        ),
        "reproducibility",
    )
    exact = {
        "schema": PROVENANCE_SCHEMA,
        "image_ref": expected["image_ref"],
        "image_id": expected["image_id"],
        "source_revision": expected["source_revision"],
        "source_tree": expected["source_tree"],
        "source_date_epoch": expected["source_date_epoch"],
        "dockerfile_sha256": expected["dockerfile_sha256"],
        "cargo_lock_sha256": expected["cargo_lock_sha256"],
        "rust_toolchain_sha256": expected["rust_toolchain_sha256"],
        "vendor_manifest_sha256": expected["vendor_manifest_sha256"],
        "compose_postgres_sigkill_smoke": True,
    }
    for key, value in exact.items():
        if type(provenance.get(key)) is not type(value) or provenance.get(key) != value:
            raise RuntimeError(f"image provenance field differs: {key}")
    if not GIT_ID.fullmatch(provenance["source_revision"]) or not GIT_ID.fullmatch(
        provenance["source_tree"]
    ):
        raise RuntimeError("image provenance Git identity is not canonical")
    for key in ("image_id", "oci_index_digest", "iid"):
        if type(provenance[key]) is not str or not DOCKER_DIGEST.fullmatch(provenance[key]):
            raise RuntimeError(f"image provenance digest is not canonical: {key}")
    if provenance["iid"] not in (provenance["image_id"], provenance["oci_index_digest"]):
        raise RuntimeError("image provenance iid does not bind the image or OCI index")
    if provenance["buildx"] != {
        "version": "v0.36.1",
        "binary_sha256": "48af8a397ebd60178778bf63611dbcebe5f5e7a9be90eb9147b24b9587455778",
    }:
        raise RuntimeError("image provenance Buildx identity is not canonical")
    if provenance["application_sbom"] != {
        "path": "/usr/share/doc/hepta-research-league/sbom.cdx.json",
        "sha256": expected["sbom_sha256"],
    }:
        raise RuntimeError("image provenance SBOM identity differs")
    if provenance["runtime_binary"] != {
        "path": "/usr/local/bin/hepta-research-league",
        "sha256": expected["runtime_binary_sha256"],
    }:
        raise RuntimeError("image provenance runtime binary identity differs")
    reproducibility = provenance["reproducibility"]
    if (
        type(reproducibility["independent_no_cache_builds"]) is not int
        or type(reproducibility["identical_image_ids"]) is not bool
        or type(reproducibility["extracted_binaries_identical"]) is not bool
        or type(reproducibility["extracted_sboms_identical"]) is not bool
        or reproducibility
        != {
        "independent_no_cache_builds": 2,
        "identical_image_ids": True,
        "extracted_binaries_identical": True,
        "extracted_sboms_identical": True,
        }
    ):
        raise RuntimeError("image provenance reproducibility contract differs")


def extract_provenance(raw_stdout, expected):
    try:
        text = raw_stdout.decode("utf-8")
    except UnicodeDecodeError as error:
        raise RuntimeError("image build stdout is not UTF-8") from error
    decoder = json.JSONDecoder(object_pairs_hook=reject_duplicate_keys)
    matches = []
    cursor = 0
    candidates = 0
    while True:
        candidate = text.find("{", cursor)
        if candidate < 0:
            break
        candidates += 1
        if candidates > 100_000:
            raise RuntimeError("image build stdout contains too many JSON candidates")
        try:
            value, end = decoder.raw_decode(text, candidate)
        except DuplicateKeyError:
            raise
        except json.JSONDecodeError:
            cursor = candidate + 1
            continue
        if type(value) is dict and value.get("schema") == PROVENANCE_SCHEMA:
            matches.append((value, end))
        cursor = candidate + 1
    if len(matches) != 1:
        raise RuntimeError("image build stdout must contain exactly one provenance object")
    provenance, end = matches[0]
    if any(character not in " \t\r\n" for character in text[end:]):
        raise RuntimeError("image provenance must be the final stdout object")
    validate_provenance(provenance, expected)
    return provenance


def write_artifact(staging_fd, name, payload):
    descriptor = os.open(
        name,
        os.O_WRONLY
        | os.O_CREAT
        | os.O_EXCL
        | os.O_CLOEXEC
        | os.O_NOFOLLOW
        | os.O_NONBLOCK,
        0o600,
        dir_fd=staging_fd,
    )
    try:
        offset = 0
        while offset < len(payload):
            offset += os.write(descriptor, payload[offset : offset + 1024 * 1024])
        os.fchmod(descriptor, 0o600)
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def digest_artifact(staging_fd, name):
    descriptor = os.open(
        name,
        os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK,
        dir_fd=staging_fd,
    )
    try:
        metadata = os.fstat(descriptor)
        if (
            not stat.S_ISREG(metadata.st_mode)
            or metadata.st_nlink != 1
            or metadata.st_uid != os.geteuid()
            or stat.S_IMODE(metadata.st_mode) != 0o600
        ):
            raise RuntimeError(f"admitted evidence artifact metadata differs: {name}")
        digest = hashlib.sha256()
        while True:
            chunk = os.read(descriptor, 1024 * 1024)
            if not chunk:
                break
            digest.update(chunk)
        return digest.hexdigest()
    finally:
        os.close(descriptor)


def main():
    args = parse_args()
    repo_dir = pathlib.Path(args.repo_dir).resolve(strict=True)
    expected = {
        "image_ref": args.image_ref,
        "image_id": args.image_id,
        "source_revision": args.source_revision,
        "source_tree": args.source_tree,
        "source_date_epoch": args.source_date_epoch,
        "dockerfile_sha256": args.dockerfile_sha256,
        "cargo_lock_sha256": args.cargo_lock_sha256,
        "rust_toolchain_sha256": args.rust_toolchain_sha256,
        "sbom_sha256": args.sbom_sha256,
        "vendor_manifest_sha256": args.vendor_manifest_sha256,
        "runtime_binary_sha256": args.runtime_binary_sha256,
    }
    if not DOCKER_DIGEST.fullmatch(args.image_id):
        raise RuntimeError("expected image ID is not canonical")
    for key in (
        "dockerfile_sha256",
        "cargo_lock_sha256",
        "rust_toolchain_sha256",
        "sbom_sha256",
        "vendor_manifest_sha256",
        "runtime_binary_sha256",
    ):
        if not SHA256.fullmatch(expected[key]):
            raise RuntimeError(f"expected input digest is not canonical: {key}")
    stdout_fd, stdout_before = admit_path(args.stdout, repo_dir, "image build stdout")
    try:
        stderr_fd, stderr_before = admit_path(args.stderr, repo_dir, "image build stderr")
    except Exception:
        os.close(stdout_fd)
        raise
    try:
        if (stdout_before.st_dev, stdout_before.st_ino) == (
            stderr_before.st_dev,
            stderr_before.st_ino,
        ):
            raise RuntimeError("image build stdout and stderr must be distinct files")
        raw_stdout = read_stable(stdout_fd, stdout_before, "image build stdout")
        raw_stderr = read_stable(stderr_fd, stderr_before, "image build stderr")
        if file_identity(os.fstat(stdout_fd)) != file_identity(stdout_before):
            raise RuntimeError("image build stdout changed before admission completed")
        if file_identity(os.fstat(stderr_fd)) != file_identity(stderr_before):
            raise RuntimeError("image build stderr changed before admission completed")
    finally:
        os.close(stderr_fd)
        os.close(stdout_fd)
    provenance = extract_provenance(raw_stdout, expected)
    canonical = (
        json.dumps(provenance, sort_keys=True, separators=(",", ":")) + "\n"
    ).encode("utf-8")
    reparsed = json.loads(
        canonical.decode("utf-8"), object_pairs_hook=reject_duplicate_keys
    )
    if reparsed != provenance:
        raise RuntimeError("canonical image provenance did not round-trip exactly")
    staging = os.fstat(args.staging_fd)
    if (
        not stat.S_ISDIR(staging.st_mode)
        or staging.st_uid != os.geteuid()
        or stat.S_IMODE(staging.st_mode) != 0o700
    ):
        raise RuntimeError("retained evidence staging descriptor is unsafe")
    write_artifact(args.staging_fd, "image-build.stdout", raw_stdout)
    write_artifact(args.staging_fd, "image-build.stderr", raw_stderr)
    write_artifact(args.staging_fd, "image-provenance.json", canonical)
    os.fsync(args.staging_fd)
    stdout_sha256 = digest_artifact(args.staging_fd, "image-build.stdout")
    stderr_sha256 = digest_artifact(args.staging_fd, "image-build.stderr")
    provenance_sha256 = digest_artifact(args.staging_fd, "image-provenance.json")
    if (
        stdout_sha256 != hashlib.sha256(raw_stdout).hexdigest()
        or stderr_sha256 != hashlib.sha256(raw_stderr).hexdigest()
        or provenance_sha256 != hashlib.sha256(canonical).hexdigest()
    ):
        raise RuntimeError("admitted image-build evidence digest differs after snapshot")
    result = {
        "stdout_identity": identity_text(stdout_before),
        "stderr_identity": identity_text(stderr_before),
        "stdout_sha256": stdout_sha256,
        "stderr_sha256": stderr_sha256,
        "provenance_sha256": provenance_sha256,
        "provenance": provenance,
    }
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"image-build evidence admission failed: {error}", file=sys.stderr)
        raise SystemExit(1)
