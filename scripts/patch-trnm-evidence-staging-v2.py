#!/usr/bin/env python3
"""Insert a private-inode executable stage immediately before the TRNM collector."""

from __future__ import annotations

from pathlib import Path
import tempfile
import sys

MARKER = "# Evidence staging: copy Cargo's hard-linked executable to one private inode."
COMMAND_MARKER = "scripts/trnm_build_evidence.py"
BINARY = "target/release/trnm-economy-service"


def indentation(line: str) -> int:
    return len(line) - len(line.lstrip(" "))


def patch_text(text: str) -> tuple[str, int]:
    if MARKER in text:
        if text.count(MARKER) != 1:
            raise ValueError("evidence staging marker is duplicated")
        return text, 0

    lines = text.splitlines(keepends=True)
    command_lines = [index for index, line in enumerate(lines) if COMMAND_MARKER in line]
    if len(command_lines) != 1:
        raise ValueError(
            f"expected exactly one {COMMAND_MARKER!r} command, found {len(command_lines)}"
        )
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
        raise ValueError("build evidence command is not inside a run block")
    run_indent = indentation(lines[run_index])
    block_end = run_index + 1
    while block_end < len(lines):
        line = lines[block_end]
        if line.strip() and indentation(line) <= run_indent:
            break
        block_end += 1
    if not (run_index < command_index < block_end):
        raise ValueError("build evidence command escaped its run block")

    command_end = command_index + 1
    while command_end < block_end and lines[command_end - 1].rstrip().endswith("\\"):
        command_end += 1
    command = "".join(lines[command_index:command_end])
    binary_count = command.count(BINARY)
    if binary_count < 1:
        raise ValueError(
            "exact release binary is not an argument of the build evidence command"
        )
    command = command.replace(BINARY, "$TRNM_EVIDENCE_BINARY")
    if BINARY in command:
        raise ValueError("release binary argument replacement was incomplete")

    prefix = " " * indentation(lines[command_index])
    staging_lines = (
        MARKER,
        'TRNM_EVIDENCE_BINARY="$RUNNER_TEMP/trnm-economy-service-evidence"',
        'rm -f -- "$TRNM_EVIDENCE_BINARY"',
        f'install -m 0755 -- {BINARY} "$TRNM_EVIDENCE_BINARY"',
        'test -f "$TRNM_EVIDENCE_BINARY"',
        '! test -L "$TRNM_EVIDENCE_BINARY"',
        'test "$(stat -c %h "$TRNM_EVIDENCE_BINARY")" = 1',
        f'test "$(sha256sum {BINARY} | awk \'{{print $1}}\')" = "$(sha256sum "$TRNM_EVIDENCE_BINARY" | awk \'{{print $1}}\')"',
    )
    staging = "".join(f"{prefix}{line}\n" for line in staging_lines)
    lines[command_index:command_end] = [staging + command]
    patched = "".join(lines)
    if patched.count(MARKER) != 1:
        raise ValueError("evidence staging marker count drifted")
    if patched.count(COMMAND_MARKER) != 1:
        raise ValueError("build evidence command count drifted")
    return patched, binary_count


def self_test() -> None:
    fixture = """jobs:
  collect:
    steps:
      - name: Build and collect
        run: |
          set -euo pipefail
          cargo build --release --workspace --locked
          test -f target/release/trnm-economy-service
          python3 scripts/trnm_build_evidence.py collect \\
            --binary target/release/trnm-economy-service \\
            --output packet.json
          test -s packet.json
      - name: Later
        run: echo done
"""
    patched, count = patch_text(fixture)
    assert count == 1
    build = patched.index("cargo build --release")
    marker = patched.index(MARKER)
    collector = patched.index("python3 scripts/trnm_build_evidence.py")
    later = patched.index("- name: Later")
    assert build < marker < collector < later
    assert "cargo build --release --workspace --locked" in patched
    assert "test -f target/release/trnm-economy-service" in patched
    assert "--binary $TRNM_EVIDENCE_BINARY" in patched
    assert patched.count(BINARY) == 3  # pre-existing proof plus two staging proofs
    again, again_count = patch_text(patched)
    assert again == patched and again_count == 0


def main() -> int:
    if len(sys.argv) == 2 and sys.argv[1] == "--self-test":
        self_test()
        print("TRNM_EVIDENCE_STAGING_TRANSFORMER_SELF_TEST=PASS")
        return 0
    if len(sys.argv) != 2:
        print("usage: patch-trnm-evidence-staging-v2.py WORKFLOW", file=sys.stderr)
        return 64
    path = Path(sys.argv[1])
    try:
        patched, count = patch_text(path.read_text(encoding="utf-8"))
    except ValueError as error:
        print(f"TRNM_EVIDENCE_STAGING_PATCH=FAIL reason={error}", file=sys.stderr)
        return 1
    path.write_text(patched, encoding="utf-8", newline="\n")
    state = "already_present" if count == 0 else "PASS"
    print(f"TRNM_EVIDENCE_STAGING_PATCH={state} replaced_binary_arguments={count}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
