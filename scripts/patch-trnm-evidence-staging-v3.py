#!/usr/bin/env python3
"""Bind the TRNM collector to a verified private-inode copy of its release binary."""

from __future__ import annotations

from pathlib import Path
import re
import sys

MARKER = "# Evidence staging: copy Cargo's hard-linked executable to one private inode."
COMMAND_RE = re.compile(r"\bpython3\s+scripts/trnm_build_evidence\.py\b")
BINARY = "target/release/trnm-economy-service"
VARIABLE_RE = re.compile(r"\$(?:\{)?([A-Za-z_][A-Za-z0-9_]*)(?:\})?")


def indentation(line: str) -> int:
    return len(line) - len(line.lstrip(" "))


def find_run_block(lines: list[str], command_index: int) -> tuple[int, int]:
    run_index = None
    for index in range(command_index, -1, -1):
        stripped = lines[index].strip()
        if stripped.startswith("run:"):
            run_index = index
            break
        if stripped.startswith("- name:") and index != command_index:
            break
    if run_index is None:
        raise ValueError("collector command is not inside a run block")
    run_indent = indentation(lines[run_index])
    block_end = run_index + 1
    while block_end < len(lines):
        line = lines[block_end]
        if line.strip() and indentation(line) <= run_indent:
            break
        block_end += 1
    if not (run_index < command_index < block_end):
        raise ValueError("collector command escaped its run block")
    return run_index, block_end


def binary_bound_variables(text: str, command: str) -> list[str]:
    candidates = sorted(set(VARIABLE_RE.findall(command)))
    bound: list[str] = []
    for variable in candidates:
        shell = re.compile(
            rf"(?m)^\s*(?:export\s+)?{re.escape(variable)}\s*=\s*"
            rf"(?:['\"])?{re.escape(BINARY)}(?:['\"])?\s*(?:#.*)?$"
        )
        yaml = re.compile(
            rf"(?m)^\s*{re.escape(variable)}\s*:\s*"
            rf"(?:['\"])?{re.escape(BINARY)}(?:['\"])?\s*(?:#.*)?$"
        )
        if shell.search(text) or yaml.search(text):
            bound.append(variable)
    return bound


def patch_text(text: str) -> tuple[str, str]:
    if MARKER in text:
        if text.count(MARKER) != 1:
            raise ValueError("evidence staging marker is duplicated")
        return text, "already_present"

    lines = text.splitlines(keepends=True)
    command_indices = [index for index, line in enumerate(lines) if COMMAND_RE.search(line)]
    if len(command_indices) != 1:
        raise ValueError(
            f"expected exactly one Python collector invocation, found {len(command_indices)}"
        )
    command_index = command_indices[0]
    _, block_end = find_run_block(lines, command_index)
    command_end = command_index + 1
    while command_end < block_end and lines[command_end - 1].rstrip().endswith("\\"):
        command_end += 1
    command = "".join(lines[command_index:command_end])

    mode: str
    override: str | None = None
    if BINARY in command:
        command = command.replace(BINARY, "$TRNM_EVIDENCE_BINARY")
        if BINARY in command:
            raise ValueError("direct release-binary replacement was incomplete")
        mode = "direct_argument"
    else:
        bound = binary_bound_variables(text, command)
        if len(bound) != 1:
            raise ValueError(
                "collector binary must be one direct literal or one uniquely bound variable; "
                f"observed_variables={bound}"
            )
        override = bound[0]
        mode = f"variable:{override}"

    prefix = " " * indentation(lines[command_index])
    staging_lines = [
        MARKER,
        'TRNM_EVIDENCE_BINARY="$RUNNER_TEMP/trnm-economy-service-evidence"',
        'rm -f -- "$TRNM_EVIDENCE_BINARY"',
        f'install -m 0755 -- {BINARY} "$TRNM_EVIDENCE_BINARY"',
        'test -f "$TRNM_EVIDENCE_BINARY"',
        '! test -L "$TRNM_EVIDENCE_BINARY"',
        'test "$(stat -c %h "$TRNM_EVIDENCE_BINARY")" = 1',
        f'test "$(sha256sum {BINARY} | awk \'{{print $1}}\')" = "$(sha256sum "$TRNM_EVIDENCE_BINARY" | awk \'{{print $1}}\')"',
    ]
    if override is not None:
        staging_lines.append(f'{override}="$TRNM_EVIDENCE_BINARY"')
    staging = "".join(f"{prefix}{line}\n" for line in staging_lines)
    lines[command_index:command_end] = [staging + command]
    patched = "".join(lines)
    if patched.count(MARKER) != 1:
        raise ValueError("evidence staging marker count drifted")
    if len(COMMAND_RE.findall(patched)) != 1:
        raise ValueError("collector invocation count drifted")
    return patched, mode


def self_test() -> None:
    direct = """jobs:
  collect:
    steps:
      - name: Build and collect
        run: |
          cargo build --release --workspace --locked
          python3 scripts/trnm_build_evidence.py collect \\
            --binary target/release/trnm-economy-service \\
            --output packet.json
"""
    patched, mode = patch_text(direct)
    assert mode == "direct_argument"
    assert patched.index("cargo build --release") < patched.index(MARKER)
    assert patched.index(MARKER) < patched.index("python3 scripts/trnm_build_evidence.py")
    assert "--binary $TRNM_EVIDENCE_BINARY" in patched

    variable = """env:
  TRNM_BUILD_BINARY: target/release/trnm-economy-service
jobs:
  collect:
    steps:
      - name: Build and collect
        run: |
          cargo build --release --workspace --locked
          python3 scripts/trnm_build_evidence.py collect \\
            --binary "$TRNM_BUILD_BINARY" \\
            --output packet.json
"""
    patched, mode = patch_text(variable)
    assert mode == "variable:TRNM_BUILD_BINARY"
    assert 'TRNM_BUILD_BINARY="$TRNM_EVIDENCE_BINARY"' in patched
    assert '--binary "$TRNM_BUILD_BINARY"' in patched
    assert patched.index(MARKER) < patched.index("python3 scripts/trnm_build_evidence.py")

    again, mode = patch_text(patched)
    assert mode == "already_present" and again == patched


def main() -> int:
    if len(sys.argv) == 2 and sys.argv[1] == "--self-test":
        self_test()
        print("TRNM_EVIDENCE_STAGING_V3_SELF_TEST=PASS")
        return 0
    if len(sys.argv) != 2:
        print("usage: patch-trnm-evidence-staging-v3.py WORKFLOW", file=sys.stderr)
        return 64
    path = Path(sys.argv[1])
    try:
        patched, mode = patch_text(path.read_text(encoding="utf-8"))
    except ValueError as error:
        print(f"TRNM_EVIDENCE_STAGING_PATCH=FAIL reason={error}", file=sys.stderr)
        return 1
    path.write_text(patched, encoding="utf-8", newline="\n")
    print(f"TRNM_EVIDENCE_STAGING_PATCH=PASS mode={mode}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
