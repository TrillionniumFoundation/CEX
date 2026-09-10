#!/usr/bin/env python3
"""Insert one exact, verified build-binary staging boundary into settlement CI."""

from __future__ import annotations

from pathlib import Path
import sys

MARKER = "# Evidence staging: copy Cargo's hard-linked executable to one private inode."
COMMAND_MARKER = "scripts/trnm_build_evidence.py"
BINARY = "target/release/trnm-economy-service"


def indentation(line: str) -> int:
    return len(line) - len(line.lstrip(" "))


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: patch-trnm-evidence-staging.py WORKFLOW", file=sys.stderr)
        return 64
    path = Path(sys.argv[1])
    text = path.read_text(encoding="utf-8")
    if MARKER in text:
        if text.count(MARKER) != 1:
            print("evidence staging marker is duplicated", file=sys.stderr)
            return 1
        print("TRNM_EVIDENCE_STAGING_PATCH=already_present")
        return 0

    lines = text.splitlines(keepends=True)
    command_lines = [index for index, line in enumerate(lines) if COMMAND_MARKER in line]
    if len(command_lines) != 1:
        print(
            f"expected exactly one {COMMAND_MARKER!r} command, found {len(command_lines)}",
            file=sys.stderr,
        )
        return 1
    command_index = command_lines[0]

    run_index = None
    for index in range(command_index, -1, -1):
        stripped = lines[index].strip()
        if stripped.startswith("run:"):
            run_index = index
            break
        if stripped.startswith("- name:") and index != command_index:
            break
    if run_index is None:
        print("build evidence command is not inside a run block", file=sys.stderr)
        return 1
    run_indent = indentation(lines[run_index])
    block_indent = run_indent + 2
    block_end = run_index + 1
    while block_end < len(lines):
        line = lines[block_end]
        if line.strip() and indentation(line) <= run_indent:
            break
        block_end += 1
    if not (run_index < command_index < block_end):
        print("build evidence command escaped its run block", file=sys.stderr)
        return 1

    block = "".join(lines[run_index + 1 : block_end])
    binary_count = block.count(BINARY)
    if binary_count < 1:
        print("build evidence run block does not reference the exact release binary", file=sys.stderr)
        return 1
    replaced = block.replace(BINARY, "$TRNM_EVIDENCE_BINARY")
    if BINARY in replaced:
        print("release binary replacement was incomplete", file=sys.stderr)
        return 1

    prefix = " " * block_indent
    staging = "".join(
        f"{prefix}{line}\n"
        for line in (
            MARKER,
            'TRNM_EVIDENCE_BINARY="$RUNNER_TEMP/trnm-economy-service-evidence"',
            'rm -f -- "$TRNM_EVIDENCE_BINARY"',
            f'install -m 0755 -- {BINARY} "$TRNM_EVIDENCE_BINARY"',
            'test -f "$TRNM_EVIDENCE_BINARY"',
            '! test -L "$TRNM_EVIDENCE_BINARY"',
            'test "$(stat -c %h "$TRNM_EVIDENCE_BINARY")" = 1',
            f'test "$(sha256sum {BINARY} | awk \'{{print $1}}\')" = "$(sha256sum "$TRNM_EVIDENCE_BINARY" | awk \'{{print $1}}\')"',
        )
    )
    lines[run_index + 1 : block_end] = [staging + replaced]
    patched = "".join(lines)
    if patched.count(MARKER) != 1:
        print("evidence staging marker count drifted", file=sys.stderr)
        return 1
    if patched.count(COMMAND_MARKER) != 1:
        print("build evidence command count drifted", file=sys.stderr)
        return 1
    path.write_text(patched, encoding="utf-8", newline="\n")
    print(f"TRNM_EVIDENCE_STAGING_PATCH=PASS replaced_binary_references={binary_count}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
