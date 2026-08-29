#!/usr/bin/env python3
"""Fail closed around the exact Hepta inherited-trait lint ownership boundary.

The large Paper Raid modules are included below ``paper_raid_v2``.  The parent
module already imports the Base64 engine, so keeping a second ``Engine as _``
import in each extracted body is both redundant and compiler-version
dependent: newer Clippy versions report the module-level ``expect`` as
*unfulfilled*.  The ownership contract therefore freezes the cleaned bodies
and requires thin, attribute-free wrappers.
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE64_IMPORT = "use base64::engine::general_purpose::STANDARD as BASE64;"
REDUNDANT_ENGINE_IMPORT = "use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};"
MODULES = {
    "services/hepta-research-league/src/paper_collaboration_v3.rs": (
        "paper_collaboration_v3_body.rs",
        "aed0cfd6d591009bce65e268330f5557f70a5cc5",
    ),
    "services/hepta-research-league/src/paper_review_v4.rs": (
        "paper_review_v4_body.rs",
        "691d1a98fa416f22b7e4516f1072d13179b02215",
    ),
    "services/hepta-research-league/src/paper_rework_v1.rs": (
        "paper_rework_v1_body.rs",
        "3e03d686a1855a8fc8911a2d35cfbfe1122ee362",
    ),
    "services/hepta-research-league/src/paper_raid_v2_tests.rs": (
        "paper_raid_v2_tests_body.rs",
        "e5a39b1f269e82c110cb367734f73d073b16e02d",
    ),
}
PROBLEMS: list[str] = []


def git_blob_sha(data: bytes) -> str:
    header = f"blob {len(data)}\0".encode("ascii")
    return hashlib.sha1(header + data).hexdigest()


def expected_wrapper(body_name: str) -> str:
    return f'include!("{body_name}");\n'


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
        if "allow(" in wrapper_text or "expect(" in wrapper_text:
            PROBLEMS.append(f"wrapper lint suppression forbidden: {wrapper_relative}")
        body_bytes = body.read_bytes()
        actual_sha = git_blob_sha(body_bytes)
        if actual_sha != expected_sha:
            PROBLEMS.append(
                f"source body identity drifted: {body.relative_to(ROOT).as_posix()} "
                f"expected={expected_sha} actual={actual_sha}"
            )
        try:
            body_text = body_bytes.decode("utf-8")
        except UnicodeDecodeError as error:
            PROBLEMS.append(f"source body is not UTF-8: {body}: {error}")
            continue
        if body_text.count(BASE64_IMPORT) != 1:
            PROBLEMS.append(
                f"source body must contain exactly one Base64 value import: "
                f"{body.relative_to(ROOT).as_posix()}"
            )
        if REDUNDANT_ENGINE_IMPORT in body_text:
            PROBLEMS.append(
                f"source body retains a redundant inherited Engine import: "
                f"{body.relative_to(ROOT).as_posix()}"
            )
        if "#![allow(unused_imports" in body_text or "#![allow(warnings" in body_text:
            PROBLEMS.append(
                f"source body contains a broad lint allowance: {body.relative_to(ROOT).as_posix()}"
            )

    result = {
        "schema": "cex.hepta-lint-ownership.v1",
        "status": "failed" if PROBLEMS else "ok",
        "ok": not PROBLEMS,
        "modules": len(MODULES),
        "policy": "exact_body_hash_plus_inherited_trait_cleanup",
        "broad_lint_allowance": False,
        "problems": PROBLEMS,
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 1 if PROBLEMS else 0


if __name__ == "__main__":
    sys.exit(main())
