#!/usr/bin/env python3
"""Fail closed around the exact Hepta inherited-trait lint ownership boundary."""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
IMPORT = "use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};"
REASON = (
    "the exact source body imports base64::Engine locally while paper_raid_v2 "
    "already supplies the trait; body identity is machine-locked"
)
MODULES = {
    "services/hepta-research-league/src/paper_collaboration_v3.rs": (
        "paper_collaboration_v3_body.rs",
        "736724a738e5bd8f314918f936ddda19aa648ab1",
    ),
    "services/hepta-research-league/src/paper_review_v4.rs": (
        "paper_review_v4_body.rs",
        "a9e8b8d342711677ef93ec87bb06dd4b52f06ea2",
    ),
    "services/hepta-research-league/src/paper_rework_v1.rs": (
        "paper_rework_v1_body.rs",
        "ac027a1ed06b92bd337c822fc09c8f544fd33a83",
    ),
    "services/hepta-research-league/src/paper_raid_v2_tests.rs": (
        "paper_raid_v2_tests_body.rs",
        "53ee14684b2df5e4e65f984436356a4f1c2f69a2",
    ),
}
PROBLEMS: list[str] = []


def git_blob_sha(data: bytes) -> str:
    header = f"blob {len(data)}\0".encode("ascii")
    return hashlib.sha1(header + data).hexdigest()


def committed_blob_sha(relative: str, fallback_bytes: bytes) -> str:
    """Return the committed Git object identity, independent of checkout EOL filters."""

    result = subprocess.run(
        ["git", "-C", str(ROOT), "rev-parse", f"HEAD:{relative}"],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    value = result.stdout.strip().lower()
    if result.returncode == 0 and re.fullmatch(r"[0-9a-f]{40}", value):
        return value

    # Source archives may not carry .git metadata. The tracked Hepta sources use
    # canonical LF; normalize checkout CRLF before computing the Git blob fallback.
    normalized = fallback_bytes.replace(b"\r\n", b"\n")
    fallback = git_blob_sha(normalized)
    PROBLEMS.append(
        "Git metadata unavailable while checking exact source body "
        f"{relative}; normalized worktree fallback={fallback}: {result.stderr.strip()}"
    )
    return fallback


def expected_wrapper(body_name: str) -> str:
    return (
        "#![expect(\n"
        "    unused_imports,\n"
        f'    reason = "{REASON}"\n'
        ")]\n\n"
        f'include!("{body_name}");\n'
    )


def main() -> int:
    for wrapper_relative, (body_name, expected_sha) in MODULES.items():
        wrapper = ROOT / wrapper_relative
        body = wrapper.with_name(body_name)
        if not wrapper.is_file():
            PROBLEMS.append(f"missing lint-ownership wrapper: {wrapper_relative}")
            continue
        if not body.is_file():
            PROBLEMS.append(f"missing exact source body: {body.relative_to(ROOT).as_posix()}")
            continue

        wrapper_text = wrapper.read_text(encoding="utf-8")
        if wrapper_text != expected_wrapper(body_name):
            PROBLEMS.append(f"lint-ownership wrapper drifted: {wrapper_relative}")
        if "allow(unused_imports" in wrapper_text or "allow(warnings" in wrapper_text:
            PROBLEMS.append(f"broad lint allowance forbidden: {wrapper_relative}")

        body_relative = body.relative_to(ROOT).as_posix()
        body_bytes = body.read_bytes()
        actual_sha = committed_blob_sha(body_relative, body_bytes)
        if actual_sha != expected_sha:
            PROBLEMS.append(
                f"source body identity drifted: {body_relative} "
                f"expected={expected_sha} actual={actual_sha}"
            )

        try:
            body_text = body.read_text(encoding="utf-8")
        except UnicodeDecodeError as error:
            PROBLEMS.append(f"source body is not UTF-8: {body}: {error}")
            continue
        if body_text.count(IMPORT) != 1:
            PROBLEMS.append(
                "source body must contain exactly one inherited Engine import: "
                f"{body_relative}"
            )
        if "#![allow(unused_imports" in body_text or "#![allow(warnings" in body_text:
            PROBLEMS.append(f"source body contains a broad lint allowance: {body_relative}")

    result = {
        "schema": "cex.hepta-lint-ownership.v1",
        "status": "failed" if PROBLEMS else "ok",
        "ok": not PROBLEMS,
        "modules": len(MODULES),
        "policy": "exact_committed_git_blob_plus_module_local_expectation",
        "cross_platform_eol_independent": True,
        "broad_lint_allowance": False,
        "problems": PROBLEMS,
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 1 if PROBLEMS else 0


if __name__ == "__main__":
    sys.exit(main())
