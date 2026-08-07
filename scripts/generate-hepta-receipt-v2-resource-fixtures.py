#!/usr/bin/env python3
"""Build a fail-closed Receipt V2 resource-gate bundle.

The legal input is copied from fresh live Chain evidence; this helper never
pads or mutates it.  A separate, explicitly adversarial canonical-shape JSON
document exercises Hepta's absolute deployment allocation boundary.
"""

from __future__ import annotations

import argparse
import copy
import datetime as dt
import hashlib
import json
import os
import pathlib
import re
import stat
import tempfile
import uuid


DEFAULT_CAP = 32 * 1024
DEPLOYMENT_MAX = 1024 * 1024
MIN_LEGAL_UTILIZATION_BPS = 4_000
DEFAULT_MIN_FRESH_SECONDS = 15 * 60
ANCHOR_SCHEMA = "trnm_cometbft_trust_anchor_v1"
RECEIPT_SCHEMA = "trnm_cometbft_apphash_finality_receipt_v2"
MANIFEST_SCHEMA = "hepta.receipt_v2.resource_gate_fixture.v1"
HEX_32 = re.compile(r"[0-9a-f]{64}\Z")


class FixtureError(RuntimeError):
    pass


def strict_pairs(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise FixtureError(f"duplicate JSON member: {key}")
        result[key] = value
    return result


def absolute_no_parent(path: pathlib.Path) -> pathlib.Path:
    candidate = path if path.is_absolute() else pathlib.Path.cwd() / path
    if ".." in candidate.parts:
        raise FixtureError(f"parent traversal is forbidden: {path}")
    return candidate


def open_directory_nofollow(path: pathlib.Path) -> tuple[pathlib.Path, int]:
    absolute = absolute_no_parent(path)
    descriptor = os.open("/", os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC)
    try:
        for component in absolute.parts[1:]:
            if component in ("", "."):
                continue
            next_descriptor = os.open(
                component,
                os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NOFOLLOW,
                dir_fd=descriptor,
            )
            os.close(descriptor)
            descriptor = next_descriptor
        return absolute, descriptor
    except BaseException:
        os.close(descriptor)
        raise


def read_regular_at(
    directory_descriptor: int, name: str, maximum: int
) -> bytes:
    if not name or "/" in name or name in (".", ".."):
        raise FixtureError(f"invalid fixture basename: {name!r}")
    descriptor = os.open(
        name,
        os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK,
        dir_fd=directory_descriptor,
    )
    try:
        before = os.fstat(descriptor)
        if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1:
            raise FixtureError(f"fixture is not one strict regular file: {name}")
        if before.st_size < 0 or before.st_size > maximum:
            raise FixtureError(f"fixture exceeds its read boundary: {name}")
        chunks: list[bytes] = []
        remaining = before.st_size
        while remaining:
            chunk = os.read(descriptor, min(remaining, 1024 * 1024))
            if not chunk:
                raise FixtureError(f"fixture shortened while reading: {name}")
            chunks.append(chunk)
            remaining -= len(chunk)
        if os.read(descriptor, 1):
            raise FixtureError(f"fixture grew while reading: {name}")
        after = os.fstat(descriptor)
        identity_before = (
            before.st_dev,
            before.st_ino,
            before.st_size,
            before.st_mtime_ns,
            before.st_ctime_ns,
        )
        identity_after = (
            after.st_dev,
            after.st_ino,
            after.st_size,
            after.st_mtime_ns,
            after.st_ctime_ns,
        )
        if identity_before != identity_after:
            raise FixtureError(f"fixture changed while reading: {name}")
        return b"".join(chunks)
    finally:
        os.close(descriptor)


def read_regular_nofollow(path: pathlib.Path, maximum: int) -> bytes:
    absolute = absolute_no_parent(path)
    _, parent_descriptor = open_directory_nofollow(absolute.parent)
    try:
        return read_regular_at(parent_descriptor, absolute.name, maximum)
    finally:
        os.close(parent_descriptor)


def transport_payload(source: bytes, label: str) -> bytes:
    payload = source[:-1] if source.endswith(b"\n") else source
    if not payload or payload.endswith((b"\n", b"\r")):
        raise FixtureError(f"fixture must contain one compact JSON document: {label}")
    return payload


def decode_canonical_source(
    source: bytes, label: str
) -> tuple[dict[str, object], bytes, str]:
    payload = transport_payload(source, label)
    try:
        value = json.loads(payload, object_pairs_hook=strict_pairs)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise FixtureError(f"decode fixture {label}: {error}") from error
    if not isinstance(value, dict):
        raise FixtureError(f"fixture root is not an object: {label}")
    canonical = json.dumps(
        value, ensure_ascii=False, separators=(",", ":")
    ).encode("utf-8")
    if canonical != payload:
        raise FixtureError(f"fixture is not exact compact canonical JSON: {label}")
    return value, payload, hashlib.sha256(source).hexdigest()


def decode_canonical(path: pathlib.Path) -> tuple[dict[str, object], bytes, str]:
    source = read_regular_nofollow(path, DEPLOYMENT_MAX + 2)
    return decode_canonical_source(source, str(path))


def canonical_bytes(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode(
        "utf-8"
    )


def parse_utc(text: object) -> dt.datetime:
    if not isinstance(text, str) or not text.endswith("Z"):
        raise FixtureError("trusted_header_time_rfc3339 is not canonical UTC text")
    match = re.fullmatch(
        r"(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2})(?:\.(\d{1,9}))?Z", text
    )
    if match is None:
        raise FixtureError("trusted_header_time_rfc3339 is not strict RFC3339 UTC")
    fraction = (match.group(2) or "")[:6].ljust(6, "0")
    normalized = f"{match.group(1)}.{fraction}+00:00"
    return dt.datetime.fromisoformat(normalized)


def framed_domain_hash(domain: str, payload: bytes) -> str:
    digest = hashlib.sha256()
    digest.update(b"trnm.domain.hash.v1")
    encoded_domain = domain.encode("utf-8")
    digest.update(len(encoded_domain).to_bytes(8, "big"))
    digest.update(encoded_domain)
    digest.update(len(payload).to_bytes(8, "big"))
    digest.update(payload)
    return digest.hexdigest()


def set_receipt_hash(receipt: dict[str, object]) -> bytes:
    receipt["receipt_hash_hex"] = ""
    unsigned = canonical_bytes(receipt)
    receipt["receipt_hash_hex"] = framed_domain_hash(
        "trnm.cometbft.apphash.finality.receipt.v2", unsigned
    )
    return canonical_bytes(receipt)


def build_adversarial(receipt: dict[str, object], target_bytes: int) -> bytes:
    """Create exact-size canonical-shape input, never represented as legal."""

    candidate = copy.deepcopy(receipt)
    transaction_proof = candidate.get("transaction_inclusion_proof")
    execution_header = candidate.get("execution_header")
    if not isinstance(transaction_proof, dict) or not isinstance(execution_header, dict):
        raise FixtureError("receipt lacks transaction proof/header objects")
    if transaction_proof.get("leaf_count") != 1 or transaction_proof.get("leaf_index") != 0:
        raise FixtureError("resource generator requires the live single-transaction fixture")
    if transaction_proof.get("aunts_hex") != []:
        raise FixtureError("single-transaction fixture unexpectedly carries Merkle aunts")

    def render(raw_size: int) -> bytes:
        raw_tx = bytes(raw_size)
        tx_hash = hashlib.sha256(raw_tx).hexdigest()
        leaf_hash = hashlib.sha256(b"\x00" + bytes.fromhex(tx_hash)).hexdigest()
        candidate["raw_tx_hex"] = raw_tx.hex()
        candidate["comet_tx_hash_hex"] = tx_hash
        transaction_proof["leaf_value_hex"] = tx_hash
        transaction_proof["leaf_hash_hex"] = leaf_hash
        execution_header["data_hash_hex"] = leaf_hash
        return set_receipt_hash(candidate)

    baseline = render(1)
    difference = target_bytes - len(baseline)
    if difference < 0:
        raise FixtureError("deployment maximum is smaller than the canonical fixture")
    if difference % 2:
        command_id = candidate.get("command_id")
        if not isinstance(command_id, str) or len(command_id) >= 160:
            raise FixtureError("cannot parity-adjust adversarial fixture canonically")
        candidate["command_id"] = command_id + "a"
        baseline = render(1)
        difference = target_bytes - len(baseline)
    if difference < 0 or difference % 2:
        raise FixtureError("cannot construct exact adversarial deployment boundary")
    output = render(1 + difference // 2)
    if len(output) != target_bytes:
        raise FixtureError("adversarial deployment-boundary fixture size drifted")
    return output


def create_directory_nofollow(path: pathlib.Path) -> tuple[pathlib.Path, int]:
    absolute = absolute_no_parent(path)
    _, parent_descriptor = open_directory_nofollow(absolute.parent)
    try:
        try:
            os.stat(absolute.name, dir_fd=parent_descriptor, follow_symlinks=False)
        except FileNotFoundError:
            pass
        else:
            raise FixtureError("output directory must not already exist")
        os.mkdir(absolute.name, mode=0o700, dir_fd=parent_descriptor)
        descriptor = os.open(
            absolute.name,
            os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NOFOLLOW,
            dir_fd=parent_descriptor,
        )
        return absolute, descriptor
    finally:
        os.close(parent_descriptor)


def write_new_at(
    directory_descriptor: int, name: str, payload: bytes, mode: int = 0o600
) -> None:
    descriptor = os.open(
        name,
        os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_CLOEXEC | os.O_NOFOLLOW,
        mode,
        dir_fd=directory_descriptor,
    )
    try:
        view = memoryview(payload)
        while view:
            written = os.write(descriptor, view)
            if written <= 0:
                raise FixtureError(f"short write for fixture {name}")
            view = view[written:]
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def write_bundle(
    output: pathlib.Path, files: dict[str, bytes], read_only: bool = False
) -> pathlib.Path:
    absolute, directory_descriptor = create_directory_nofollow(output)
    try:
        for name in sorted(files):
            write_new_at(
                directory_descriptor,
                name,
                files[name],
                0o400 if read_only else 0o600,
            )
        os.fsync(directory_descriptor)
        if read_only:
            os.fchmod(directory_descriptor, 0o500)
    finally:
        os.close(directory_descriptor)
    return absolute


def validate_inputs(
    anchor: dict[str, object],
    receipt: dict[str, object],
    receipt_payload: bytes,
    default_cap: int,
    deployment_max: int,
    min_utilization_bps: int,
    min_fresh_seconds: int,
    enforce_fresh: bool,
) -> tuple[str, str, dt.datetime]:
    if anchor.get("schema") != ANCHOR_SCHEMA:
        raise FixtureError("unsupported trust-anchor schema")
    if receipt.get("schema") != RECEIPT_SCHEMA:
        raise FixtureError("unsupported Receipt V2 schema")
    anchor_hash = anchor.get("anchor_hash_hex")
    receipt_hash = receipt.get("receipt_hash_hex")
    if not isinstance(anchor_hash, str) or HEX_32.fullmatch(anchor_hash) is None:
        raise FixtureError("anchor_hash_hex is not 32-byte lowercase hex")
    if not isinstance(receipt_hash, str) or HEX_32.fullmatch(receipt_hash) is None:
        raise FixtureError("receipt_hash_hex is not 32-byte lowercase hex")
    if not 0 < default_cap <= deployment_max:
        raise FixtureError("invalid deployment caps")
    if len(receipt_payload) > default_cap:
        raise FixtureError("fresh legal receipt exceeds the default deployment cap")
    utilization = len(receipt_payload) * 10_000 // default_cap
    if utilization < min_utilization_bps:
        raise FixtureError(
            "fresh legal receipt is not near the default cap: "
            f"{utilization}bps < {min_utilization_bps}bps"
        )
    trusting_period = anchor.get("trusting_period_seconds")
    if not isinstance(trusting_period, int) or isinstance(trusting_period, bool):
        raise FixtureError("trusting_period_seconds is not an integer")
    if trusting_period <= 0:
        raise FixtureError("trusting_period_seconds must be positive")
    expires_at = parse_utc(anchor.get("trusted_header_time_rfc3339")) + dt.timedelta(
        seconds=trusting_period
    )
    now = dt.datetime.now(dt.timezone.utc)
    if enforce_fresh and expires_at <= now + dt.timedelta(seconds=min_fresh_seconds):
        raise FixtureError(
            "trust anchor is expired or too close to expiry; generate fresh live evidence"
        )
    return anchor_hash, receipt_hash, expires_at


def create_bundle(args: argparse.Namespace, enforce_fresh: bool = True) -> pathlib.Path:
    anchor, anchor_payload, anchor_source_sha = decode_canonical(args.anchor)
    receipt, receipt_payload, receipt_source_sha = decode_canonical(args.receipt)
    anchor_hash, receipt_hash, expires_at = validate_inputs(
        anchor,
        receipt,
        receipt_payload,
        args.default_cap,
        args.deployment_max,
        args.min_utilization_bps,
        args.min_fresh_seconds,
        enforce_fresh,
    )
    adversarial = build_adversarial(receipt, args.deployment_max)
    default_plus_one = b" " * (args.default_cap + 1)
    max_plus_one = b" " * (args.deployment_max + 1)
    paper_id = str(uuid.UUID(args.paper_id)) if args.paper_id else str(uuid.uuid4())

    manifest = {
        "schema": MANIFEST_SCHEMA,
        "paper_id": paper_id,
        "anchor_hash_hex": anchor_hash,
        "receipt_hash_hex": receipt_hash,
        "anchor_source_sha256": anchor_source_sha,
        "receipt_source_sha256": receipt_source_sha,
        "trust_anchor_sha256": hashlib.sha256(anchor_payload).hexdigest(),
        "legal_receipt_sha256": hashlib.sha256(receipt_payload).hexdigest(),
        "adversarial_sha256": hashlib.sha256(adversarial).hexdigest(),
        "default_plus_one_sha256": hashlib.sha256(default_plus_one).hexdigest(),
        "max_plus_one_sha256": hashlib.sha256(max_plus_one).hexdigest(),
        "legal_receipt_bytes": len(receipt_payload),
        "default_cap_bytes": args.default_cap,
        "deployment_max_bytes": args.deployment_max,
        "min_legal_utilization_bps": args.min_utilization_bps,
        "legal_utilization_bps": len(receipt_payload) * 10_000 // args.default_cap,
        "anchor_expires_at": expires_at.isoformat().replace("+00:00", "Z"),
        "generated_at": dt.datetime.now(dt.timezone.utc)
        .isoformat()
        .replace("+00:00", "Z"),
    }
    return write_bundle(
        args.output,
        {
            "canonical-shape-adversarial.json": adversarial,
            "default-plus-one.body": default_plus_one,
            "legal-receipt-v2.json": receipt_payload,
            "manifest.json": canonical_bytes(manifest) + b"\n",
            "max-plus-one.body": max_plus_one,
            "trust-anchor.json": anchor_payload,
        },
    )


def read_bundle(path: pathlib.Path) -> tuple[pathlib.Path, dict[str, bytes]]:
    root, directory_descriptor = open_directory_nofollow(path)
    expected_names = {
        "canonical-shape-adversarial.json",
        "default-plus-one.body",
        "legal-receipt-v2.json",
        "manifest.json",
        "max-plus-one.body",
        "trust-anchor.json",
    }
    try:
        if set(os.listdir(directory_descriptor)) != expected_names:
            raise FixtureError("resource fixture bundle contains missing or extra files")
        files = {
            name: read_regular_at(directory_descriptor, name, DEPLOYMENT_MAX + 1)
            for name in sorted(expected_names)
        }
    finally:
        os.close(directory_descriptor)
    return root, files


def validate_bundle_files(
    files: dict[str, bytes], enforce_fresh: bool = True
) -> dict[str, object]:
    manifest_raw = files["manifest.json"]
    if not manifest_raw.endswith(b"\n") or manifest_raw.endswith(b"\n\n"):
        raise FixtureError("manifest must end in one transport newline")
    manifest = json.loads(manifest_raw[:-1], object_pairs_hook=strict_pairs)
    if not isinstance(manifest, dict) or manifest.get("schema") != MANIFEST_SCHEMA:
        raise FixtureError("unsupported resource fixture manifest")
    if canonical_bytes(manifest) + b"\n" != manifest_raw:
        raise FixtureError("resource fixture manifest is not canonical")

    for name, field in (
        ("trust-anchor.json", "trust_anchor_sha256"),
        ("legal-receipt-v2.json", "legal_receipt_sha256"),
        ("canonical-shape-adversarial.json", "adversarial_sha256"),
        ("default-plus-one.body", "default_plus_one_sha256"),
        ("max-plus-one.body", "max_plus_one_sha256"),
    ):
        actual = hashlib.sha256(files[name]).hexdigest()
        if actual != manifest.get(field):
            raise FixtureError(f"bundle digest mismatch: {name}")

    default_cap = manifest.get("default_cap_bytes")
    deployment_max = manifest.get("deployment_max_bytes")
    min_utilization = manifest.get("min_legal_utilization_bps")
    if (default_cap, deployment_max) != (DEFAULT_CAP, DEPLOYMENT_MAX):
        raise FixtureError("bundle caps do not match the compiled Hepta deployment contract")
    if min_utilization != MIN_LEGAL_UTILIZATION_BPS:
        raise FixtureError("bundle utilization policy drifted")
    receipt_bytes = files["legal-receipt-v2.json"]
    if len(receipt_bytes) != manifest.get("legal_receipt_bytes"):
        raise FixtureError("legal Receipt byte count drifted")
    if len(files["canonical-shape-adversarial.json"]) != deployment_max:
        raise FixtureError("adversarial fixture is not at the deployment maximum")
    adversarial_value, adversarial_payload, _ = decode_canonical_source(
        files["canonical-shape-adversarial.json"],
        "canonical-shape-adversarial.json",
    )
    if adversarial_value.get("schema") != RECEIPT_SCHEMA:
        raise FixtureError("adversarial fixture schema drifted")
    if set_receipt_hash(copy.deepcopy(adversarial_value)) != adversarial_payload:
        raise FixtureError("adversarial fixture receipt hash drifted")
    default_plus_one = files["default-plus-one.body"]
    if default_plus_one != b" " * (default_cap + 1):
        raise FixtureError("default+1 fixture content or length drifted")
    max_plus_one = files["max-plus-one.body"]
    if len(max_plus_one) != deployment_max + 1:
        raise FixtureError("max+1 fixture length drifted")
    if max_plus_one != b" " * (deployment_max + 1):
        raise FixtureError("max+1 fixture content drifted")

    anchor, anchor_payload, _ = decode_canonical_source(
        files["trust-anchor.json"], "trust-anchor.json"
    )
    receipt, receipt_payload, _ = decode_canonical_source(
        files["legal-receipt-v2.json"], "legal-receipt-v2.json"
    )
    anchor_hash, receipt_hash, expires_at = validate_inputs(
        anchor,
        receipt,
        receipt_payload,
        default_cap,
        deployment_max,
        min_utilization,
        DEFAULT_MIN_FRESH_SECONDS,
        enforce_fresh,
    )
    if anchor_hash != manifest.get("anchor_hash_hex"):
        raise FixtureError("manifest anchor hash drifted")
    if receipt_hash != manifest.get("receipt_hash_hex"):
        raise FixtureError("manifest receipt hash drifted")
    if expires_at.isoformat().replace("+00:00", "Z") != manifest.get(
        "anchor_expires_at"
    ):
        raise FixtureError("manifest anchor expiry drifted")
    canonical_paper_id = str(manifest.get("paper_id"))
    if str(uuid.UUID(canonical_paper_id)) != canonical_paper_id:
        raise FixtureError("manifest paper_id is not a canonical UUID")
    return manifest


def verify_bundle(path: pathlib.Path, enforce_fresh: bool = True) -> dict[str, object]:
    _, files = read_bundle(path)
    return validate_bundle_files(files, enforce_fresh)


def snapshot_bundle(
    source: pathlib.Path, output: pathlib.Path, enforce_fresh: bool = True
) -> tuple[pathlib.Path, dict[str, object]]:
    _, files = read_bundle(source)
    manifest = validate_bundle_files(files, enforce_fresh)
    snapshot = write_bundle(output, files, read_only=True)
    if verify_bundle(snapshot, enforce_fresh) != manifest:
        raise FixtureError("private resource fixture snapshot verification drifted")
    return snapshot, manifest


def self_test(repo: pathlib.Path) -> None:
    anchor_source = repo / "vendor/trnm-finality-verifier/fixtures/cometbft-trust-anchor-v1.json"
    receipt_source = (
        repo
        / "vendor/trnm-finality-verifier/fixtures/cometbft-apphash-finality-receipt-v2.json"
    )
    with tempfile.TemporaryDirectory(prefix="hepta-receipt-resource-fixture.") as scratch:
        scratch_path = pathlib.Path(scratch)

        def expect_failure(label: str, operation: object) -> None:
            try:
                operation()
            except (FixtureError, OSError, ValueError):
                return
            raise FixtureError(f"self-test accepted {label}")

        def copied_bundle(name: str, files: dict[str, bytes]) -> pathlib.Path:
            return write_bundle(scratch_path / name, files)

        receipt, receipt_payload, _ = decode_canonical(receipt_source)
        if set_receipt_hash(copy.deepcopy(receipt)) != receipt_payload:
            raise FixtureError("self-test Receipt V2 domain hash does not round-trip")
        args = argparse.Namespace(
            anchor=anchor_source,
            receipt=receipt_source,
            output=scratch_path / "bundle",
            paper_id="00000000-0000-4000-8000-000000000001",
            default_cap=DEFAULT_CAP,
            deployment_max=DEPLOYMENT_MAX,
            min_utilization_bps=MIN_LEGAL_UTILIZATION_BPS,
            min_fresh_seconds=0,
        )
        bundle = create_bundle(args, enforce_fresh=False)
        verify_bundle(bundle, enforce_fresh=False)
        snapshot, snapshot_manifest = snapshot_bundle(
            bundle, scratch_path / "snapshot", enforce_fresh=False
        )
        if verify_bundle(snapshot, enforce_fresh=False) != snapshot_manifest:
            raise FixtureError("self-test private snapshot drifted")

        _, pristine_files = read_bundle(bundle)
        expect_failure(
            "duplicate canonical JSON members",
            lambda: decode_canonical_source(
                b'{"schema":"first","schema":"second"}', "duplicate-member.json"
            ),
        )

        symlink = scratch_path / "bundle-symlink"
        symlink.symlink_to(bundle, target_is_directory=True)
        expect_failure(
            "a symlinked bundle path",
            lambda: verify_bundle(symlink, enforce_fresh=False),
        )

        manifest_symlink = copied_bundle("manifest-symlink", pristine_files)
        (manifest_symlink / "manifest.json").unlink()
        (manifest_symlink / "manifest.json").symlink_to(bundle / "manifest.json")
        expect_failure(
            "a symlinked manifest",
            lambda: verify_bundle(manifest_symlink, enforce_fresh=False),
        )

        manifest_fifo = copied_bundle("manifest-fifo", pristine_files)
        (manifest_fifo / "manifest.json").unlink()
        os.mkfifo(manifest_fifo / "manifest.json", mode=0o600)
        expect_failure(
            "a FIFO manifest without blocking",
            lambda: verify_bundle(manifest_fifo, enforce_fresh=False),
        )

        for filename, label in (
            ("trust-anchor.json", "a tampered trust anchor"),
            ("legal-receipt-v2.json", "a tampered legal receipt"),
            (
                "canonical-shape-adversarial.json",
                "a tampered adversarial receipt",
            ),
        ):
            files = dict(pristine_files)
            payload = bytearray(files[filename])
            payload[-1] ^= 1
            files[filename] = bytes(payload)
            tampered = copied_bundle(f"tampered-{filename}", files)
            expect_failure(
                label,
                lambda path=tampered: verify_bundle(path, enforce_fresh=False),
            )

        missing = copied_bundle("missing-file", pristine_files)
        (missing / "max-plus-one.body").unlink()
        expect_failure(
            "a bundle with a missing file",
            lambda: verify_bundle(missing, enforce_fresh=False),
        )

        extra = copied_bundle("extra-file", pristine_files)
        (extra / "unexpected.json").write_bytes(b"{}")
        expect_failure(
            "a bundle with an extra file",
            lambda: verify_bundle(extra, enforce_fresh=False),
        )

        cap_drift_files = dict(pristine_files)
        cap_manifest = json.loads(
            cap_drift_files["manifest.json"][:-1], object_pairs_hook=strict_pairs
        )
        cap_manifest["default_cap_bytes"] = DEFAULT_CAP + 1
        cap_drift_files["manifest.json"] = canonical_bytes(cap_manifest) + b"\n"
        cap_drift = copied_bundle("cap-drift", cap_drift_files)
        expect_failure(
            "fixture cap drift",
            lambda: verify_bundle(cap_drift, enforce_fresh=False),
        )

        anchor, _, _ = decode_canonical(anchor_source)
        anchor["trusted_header_time_rfc3339"] = "2000-01-01T00:00:00Z"
        anchor["trusting_period_seconds"] = 1
        expired_anchor = scratch_path / "expired-anchor.json"
        expired_anchor.write_bytes(canonical_bytes(anchor))
        expired_args = copy.copy(args)
        expired_args.anchor = expired_anchor
        expired_args.output = scratch_path / "expired-bundle"
        expired_bundle = create_bundle(expired_args, enforce_fresh=False)
        expect_failure(
            "an expired trust anchor under the freshness policy",
            lambda: verify_bundle(expired_bundle, enforce_fresh=True),
        )


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    mode = result.add_mutually_exclusive_group()
    mode.add_argument("--verify-bundle", type=pathlib.Path)
    mode.add_argument("--snapshot-bundle", type=pathlib.Path)
    mode.add_argument("--self-test", action="store_true")
    result.add_argument("--output", type=pathlib.Path)
    result.add_argument("--anchor", type=pathlib.Path)
    result.add_argument("--receipt", type=pathlib.Path)
    result.add_argument("--paper-id")
    result.add_argument("--default-cap", type=int, default=DEFAULT_CAP)
    result.add_argument("--deployment-max", type=int, default=DEPLOYMENT_MAX)
    result.add_argument(
        "--min-utilization-bps", type=int, default=MIN_LEGAL_UTILIZATION_BPS
    )
    result.add_argument(
        "--min-fresh-seconds", type=int, default=DEFAULT_MIN_FRESH_SECONDS
    )
    return result


def main() -> int:
    args = parser().parse_args()
    repo = pathlib.Path(__file__).resolve().parent.parent
    try:
        if args.self_test:
            self_test(repo)
            print("Receipt V2 resource fixture generator self-test: PASS")
            return 0
        if args.verify_bundle is not None:
            manifest = verify_bundle(args.verify_bundle)
            print(json.dumps(manifest, sort_keys=True, separators=(",", ":")))
            return 0
        if args.snapshot_bundle is not None:
            if args.output is None:
                raise FixtureError("--output is required with --snapshot-bundle")
            snapshot, manifest = snapshot_bundle(args.snapshot_bundle, args.output)
            print(
                json.dumps(
                    {"snapshot": str(snapshot), "manifest": manifest},
                    sort_keys=True,
                    separators=(",", ":"),
                )
            )
            return 0
        if args.output is None or args.anchor is None or args.receipt is None:
            raise FixtureError("--anchor and --receipt are required with --output")
        if args.min_fresh_seconds < 0:
            raise FixtureError("--min-fresh-seconds must be non-negative")
        output = create_bundle(args)
        print(output)
        return 0
    except (FixtureError, OSError, ValueError) as error:
        print(f"Receipt V2 resource fixture error: {error}", file=os.sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
