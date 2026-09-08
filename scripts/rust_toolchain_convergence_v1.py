"""Fail-closed Rust compiler convergence for active CEX release surfaces."""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Iterable

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_VERSION = "1.98.1"
EXPECTED_RUST_COMMIT = "48a229ceaefd4985c50990b14116b6d856af0985"
EXPECTED_CARGO_COMMIT = "797e8a9bca276c1c9f9f738d2a20f484fa4eea9d"
EXPECTED_DIST_DATE = "2026-09-03"
EXPECTED_DTOLNAY_ACTION = "dtolnay/rust-toolchain@6bed0761d98439e5a578e2877258200ad565ba87"
EXPECTED_INSTALLER = "scripts/install-rust-toolchain-1.98.1.sh"
EXPECTED_PREFIX = "/opt/cex-rust/1.98.1"
EXPECTED_ARCHIVE_HASHES = {
    "x86_64-unknown-linux-gnu": "5326b36c53de11d148c8f8dab6553a3d1006c2cfd32123683073fad3c302605b",
    "aarch64-unknown-linux-gnu": "0b514a8cc1cbcd939bff0f151661fe58b6ea5c7a7f645a5098c69e32e8c1e0a2",
}

ACTIVE_FILES = (
    "services/hepta-research-league/Dockerfile",
    "services/hepta-research-league/docker/rust-toolchain.manifest",
    "services/paper-raid-bff/Dockerfile",
    "services/paper-raid-bff/Dockerfile.accessctl",
    "services/paper-raid-bff/scripts/check-docker-lock.sh",
    "services/paper-raid-bff/scripts/check-boundaries.sh",
    "services/paper-raid-bff/scripts/check-browser-mobile-a11y.sh",
    EXPECTED_INSTALLER,
)
RETIRED_ONE_SHOT_WORKFLOWS = (
    ".github/workflows/world-settlement-external-evidence.yml",
    ".github/workflows/world-settlement-final-validation-v2.yml",
    ".github/workflows/world-settlement-final-convergence-v2.yml",
    ".github/workflows/world-settlement-final-convergence-v3.yml",
)
FORBIDDEN_ACTIVE_PATTERNS = (
    re.compile(r"\b1\.95\.0\b"),
    re.compile(r"\b1\.98\.0\b"),
    re.compile(r"dtolnay/rust-toolchain@(stable|main|master|v\d+)(?:\s|$)"),
    re.compile(r"(?im)^\s*rustup\s+(?:toolchain\s+install|default)\s+(?:stable|1\.95\.0|1\.98\.0)\b"),
    re.compile(r"(?im)^\s*(?:RUST_TOOLCHAIN|RUST_VERSION):?\s*['\"]?(?:stable|1\.95\.0|1\.98\.0)['\"]?\s*$"),
)


class ToolchainViolation(RuntimeError):
    """Raised when an active surface can escape the exact toolchain."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ToolchainViolation(message)


def read(relative: str) -> str:
    path = ROOT / relative
    require(path.is_file() and not path.is_symlink(), f"missing regular file: {relative}")
    try:
        return path.read_text(encoding="utf-8")
    except UnicodeDecodeError as error:
        raise ToolchainViolation(f"active toolchain surface is not UTF-8: {relative}: {error}") from error


def active_workflows() -> Iterable[Path]:
    workflow_root = ROOT / ".github/workflows"
    require(workflow_root.is_dir(), "missing .github/workflows")
    yield from sorted(path for path in workflow_root.iterdir() if path.suffix in {".yml", ".yaml"})


def validate_text(relative: str, text: str) -> None:
    for pattern in FORBIDDEN_ACTIVE_PATTERNS:
        match = pattern.search(text)
        require(match is None, f"forbidden active Rust toolchain marker in {relative}: {match.group(0)!r}")


def validate_workflows() -> list[str]:
    checked: list[str] = []
    for path in active_workflows():
        relative = path.relative_to(ROOT).as_posix()
        require(relative not in RETIRED_ONE_SHOT_WORKFLOWS, f"retired one-shot workflow still exists: {relative}")
        text = path.read_text(encoding="utf-8")
        validate_text(relative, text)
        if "dtolnay/rust-toolchain@" in text:
            require(EXPECTED_DTOLNAY_ACTION in text, f"workflow uses non-immutable Rust action: {relative}")
            require("toolchain: 1.98.1" in text, f"workflow omits exact Rust 1.98.1 input: {relative}")
        if "rustup toolchain install" in text or "rustup default" in text:
            require("rustup toolchain install 1.98.1" in text, f"workflow Rust install is not exact: {relative}")
            require("rustup default 1.98.1" in text, f"workflow Rust default is not exact: {relative}")
        checked.append(relative)
    return checked


def validate_static() -> dict[str, object]:
    checked = validate_workflows()
    for relative in ACTIVE_FILES:
        validate_text(relative, read(relative))
        checked.append(relative)

    installer = read(EXPECTED_INSTALLER)
    for marker in (
        EXPECTED_VERSION,
        EXPECTED_RUST_COMMIT,
        EXPECTED_CARGO_COMMIT,
        EXPECTED_DIST_DATE,
        *EXPECTED_ARCHIVE_HASHES.values(),
        "https://static.rust-lang.org/dist/",
        "sha256sum --check --strict",
        "--without=rust-docs",
    ):
        require(marker in installer, f"installer lacks exact marker: {marker}")

    manifest = read("services/hepta-research-league/docker/rust-toolchain.manifest")
    expected_manifest = {
        "toolchain": EXPECTED_VERSION,
        "rust_release_commit": EXPECTED_RUST_COMMIT,
        "cargo_commit": EXPECTED_CARGO_COMMIT,
        "dist_date": EXPECTED_DIST_DATE,
        "dist_x86_64_unknown_linux_gnu_sha256": EXPECTED_ARCHIVE_HASHES["x86_64-unknown-linux-gnu"],
        "dist_aarch64_unknown_linux_gnu_sha256": EXPECTED_ARCHIVE_HASHES["aarch64-unknown-linux-gnu"],
        "production_authorization": "not_granted",
    }
    pairs: dict[str, str] = {}
    for raw in manifest.splitlines():
        if not raw or raw.lstrip().startswith("#"):
            continue
        require("=" in raw, f"invalid toolchain manifest line: {raw!r}")
        key, value = raw.split("=", 1)
        require(key not in pairs, f"duplicate toolchain manifest key: {key}")
        pairs[key] = value
    for key, value in expected_manifest.items():
        require(pairs.get(key) == value, f"toolchain manifest drift: {key}")

    for relative in (
        "services/hepta-research-league/Dockerfile",
        "services/paper-raid-bff/Dockerfile",
        "services/paper-raid-bff/Dockerfile.accessctl",
    ):
        text = read(relative)
        for marker in (
            f"COPY {EXPECTED_INSTALLER} /toolchain/install-rust-toolchain.sh",
            "RUN /toolchain/install-rust-toolchain.sh",
            f'ENV PATH="{EXPECTED_PREFIX}/bin:${{PATH}}"',
            "python3 scripts/check-rust-toolchain-convergence.py --verify-installed",
        ):
            require(marker in text, f"{relative} lacks exact toolchain marker: {marker}")

    for relative in (
        "services/paper-raid-bff/scripts/check-docker-lock.sh",
        "services/paper-raid-bff/scripts/check-boundaries.sh",
        "services/paper-raid-bff/scripts/check-browser-mobile-a11y.sh",
    ):
        require(
            "scripts/check-rust-toolchain-convergence.py --verify-installed" in read(relative),
            f"{relative} does not use the shared installed-toolchain verifier",
        )

    return {
        "schema": "cex.rust-toolchain-convergence.v1",
        "status": "static_exact_toolchain_valid",
        "rust_version": EXPECTED_VERSION,
        "rust_commit": EXPECTED_RUST_COMMIT,
        "cargo_commit": EXPECTED_CARGO_COMMIT,
        "active_surface_count": len(set(checked)),
        "retired_one_shot_workflows": list(RETIRED_ONE_SHOT_WORKFLOWS),
        "production_authorization": "not_granted",
    }


def command(*args: str) -> str:
    result = subprocess.run(
        args,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    require(result.returncode == 0, f"command failed: {' '.join(args)}: {result.stderr.strip()}")
    return result.stdout


def version_field(output: str, key: str) -> str:
    prefix = f"{key}: "
    for line in output.splitlines():
        if line.startswith(prefix):
            return line[len(prefix) :].strip()
    raise ToolchainViolation(f"version output lacks {key}: {output!r}")


def validate_installed() -> dict[str, object]:
    rust = command("rustc", "-vV")
    cargo = command("cargo", "-Vv")
    require(version_field(rust, "release") == EXPECTED_VERSION, "installed rustc release drift")
    require(version_field(rust, "commit-hash") == EXPECTED_RUST_COMMIT, "installed rustc commit drift")
    require(version_field(cargo, "commit-hash") == EXPECTED_CARGO_COMMIT, "installed Cargo commit drift")
    for binary in ("rustfmt", "clippy-driver"):
        result = subprocess.run([binary, "--version"], text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        require(result.returncode == 0, f"required Rust component unavailable: {binary}: {result.stderr.strip()}")
    return {
        "schema": "cex.rust-toolchain-runtime.v1",
        "status": "installed_exact_toolchain_valid",
        "rust_version": EXPECTED_VERSION,
        "rust_commit": EXPECTED_RUST_COMMIT,
        "cargo_commit": EXPECTED_CARGO_COMMIT,
        "host": version_field(rust, "host"),
        "production_authorization": "not_granted",
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--verify-installed", action="store_true")
    args = parser.parse_args()
    try:
        evidence: dict[str, object] = {"static": validate_static()}
        if args.verify_installed:
            evidence["installed"] = validate_installed()
        print(json.dumps(evidence, indent=2, sort_keys=True))
        return 0
    except (OSError, ToolchainViolation) as error:
        print(
            json.dumps(
                {
                    "schema": "cex.rust-toolchain-convergence.v1",
                    "status": "failed",
                    "problems": [str(error)],
                    "production_authorization": "not_granted",
                },
                indent=2,
                sort_keys=True,
            )
        )
        return 1


if __name__ == "__main__":
    sys.exit(main())
