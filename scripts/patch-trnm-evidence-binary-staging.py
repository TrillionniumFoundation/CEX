#!/usr/bin/env python3
"""Patch only the TRNM build-evidence run block to consume a single-link staged binary."""

from __future__ import annotations

from pathlib import Path
import sys

SOURCE_BINARY = "target/release/trnm-economy-service"
STAGED_BINARY = '"$TRNM_BUILD_EVIDENCE_BINARY"'
MARKER = "TRNM_BUILD_EVIDENCE_SINGLE_LINK_STAGING_V1"


def run_blocks(lines: list[str]) -> list[tuple[int, int, int]]:
    blocks: list[tuple[int, int, int]] = []
    for index, line in enumerate(lines):
        stripped = line.lstrip(" ")
        indent = len(line) - len(stripped)
        if stripped.rstrip("\r\n") != "run: |":
            continue
        end = index + 1
        while end < len(lines):
            candidate = lines[end]
            if candidate.strip() and len(candidate) - len(candidate.lstrip(" ")) <= indent:
                break
            end += 1
        blocks.append((index, end, indent))
    return blocks


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: patch-trnm-evidence-binary-staging.py WORKFLOW", file=sys.stderr)
        return 64
    path = Path(sys.argv[1])
    text = path.read_text(encoding="utf-8")
    if MARKER in text:
        print("TRNM_EVIDENCE_BINARY_STAGING=ALREADY_PRESENT")
        return 0
    lines = text.splitlines(keepends=True)
    matches: list[tuple[int, int, int]] = []
    for start, end, indent in run_blocks(lines):
        block = "".join(lines[start + 1 : end])
        if "scripts/trnm_build_evidence.py" in block and SOURCE_BINARY in block:
            matches.append((start, end, indent))
    if len(matches) != 1:
        print(
            f"expected exactly one build-evidence run block consuming {SOURCE_BINARY}; found {len(matches)}",
            file=sys.stderr,
        )
        return 1
    start, end, indent = matches[0]
    body_indent = " " * (indent + 2)
    block = "".join(lines[start + 1 : end])
    count = block.count(SOURCE_BINARY)
    if count < 1:
        print("source binary path disappeared from selected run block", file=sys.stderr)
        return 1
    prelude = (
        f"{body_indent}# {MARKER}\n"
        f"{body_indent}evidence_binary_dir=\"$RUNNER_TEMP/trnm-build-evidence\"\n"
        f"{body_indent}rm -rf \"$evidence_binary_dir\"\n"
        f"{body_indent}mkdir -p \"$evidence_binary_dir\"\n"
        f"{body_indent}install -m 0555 {SOURCE_BINARY} \"$evidence_binary_dir/trnm-economy-service\"\n"
        f"{body_indent}export TRNM_BUILD_EVIDENCE_BINARY=\"$evidence_binary_dir/trnm-economy-service\"\n"
        f"{body_indent}test \"$(stat -c %h \"$TRNM_BUILD_EVIDENCE_BINARY\")\" = 1\n"
        f"{body_indent}test \"$(sha256sum {SOURCE_BINARY} | awk '{{print $1}}')\" = "
        f"\"$(sha256sum \"$TRNM_BUILD_EVIDENCE_BINARY\" | awk '{{print $1}}')\"\n"
    )
    patched_block = prelude + block.replace(SOURCE_BINARY, STAGED_BINARY)
    lines[start + 1 : end] = [patched_block]
    patched = "".join(lines)
    if patched.count(MARKER) != 1:
        print("staging marker cardinality drift", file=sys.stderr)
        return 1
    if SOURCE_BINARY not in patched:
        print("source build path unexpectedly disappeared from workflow", file=sys.stderr)
        return 1
    path.write_text(patched, encoding="utf-8", newline="\n")
    print(f"TRNM_EVIDENCE_BINARY_STAGING=PATCHED replacements={count}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
