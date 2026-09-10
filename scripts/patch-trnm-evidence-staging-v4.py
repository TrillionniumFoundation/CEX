#!/usr/bin/env python3
"""Bind the TRNM collector to a repository-local, private-inode build copy."""

from __future__ import annotations

from pathlib import Path
import re
import sys

MARKER = "# Evidence staging: copy Cargo's hard-linked executable to one private inode."
COMMAND_RE = re.compile(r"\bpython3\s+scripts/trnm_build_evidence\.py\b")
BINARY = "target/release/trnm-economy-service"
STAGED = "target/evidence/trnm-economy-service"
OLD_ASSIGNMENT_RE = re.compile(
    r'(?m)^(?P<indent>\s*)TRNM_EVIDENCE_BINARY="\$RUNNER_TEMP/'
    r'trnm-economy-service-evidence"\s*$'
)
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


def bound_variables(text: str, command: str) -> list[str]:
    result: list[str] = []
    for variable in sorted(set(VARIABLE_RE.findall(command))):
        shell = re.compile(
            rf"(?m)^\s*(?:export\s+)?{re.escape(variable)}\s*=\s*"
            rf"(?:['\"])?{re.escape(BINARY)}(?:['\"])?\s*(?:#.*)?$"
        )
        yaml = re.compile(
            rf"(?m)^\s*{re.escape(variable)}\s*:\s*"
            rf"(?:['\"])?{re.escape(BINARY)}(?:['\"])?\s*(?:#.*)?$"
        )
        if shell.search(text) or yaml.search(text):
            result.append(variable)
    return result


def upgrade_existing(text: str) -> tuple[str, str]:
    if text.count(MARKER) != 1:
        raise ValueError("evidence staging marker is duplicated")
    if f'TRNM_EVIDENCE_BINARY="{STAGED}"' in text:
        if text.count(f'TRNM_EVIDENCE_BINARY="{STAGED}"') != 1:
            raise ValueError("repository-local staging assignment is duplicated")
        if text.count("mkdir -p -- target/evidence") != 1:
            raise ValueError("repository-local staging directory proof is missing or duplicated")
        return text, "already_repository_local"
    matches = list(OLD_ASSIGNMENT_RE.finditer(text))
    if len(matches) != 1:
        raise ValueError("existing staging is neither recognized external nor repository-local form")
    match = matches[0]
    indent = match.group("indent")
    replacement = (
        f'{indent}TRNM_EVIDENCE_BINARY="{STAGED}"\n'
        f'{indent}mkdir -p -- target/evidence'
    )
    patched = text[: match.start()] + replacement + text[match.end() :]
    if "$RUNNER_TEMP/trnm-economy-service-evidence" in patched:
        raise ValueError("external staging path remained after upgrade")
    return patched, "upgraded_external_to_repository_local"


def patch_text(text: str) -> tuple[str, str]:
    if MARKER in text:
        return upgrade_existing(text)

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

    override: str | None = None
    if BINARY in command:
        command = command.replace(BINARY, "$TRNM_EVIDENCE_BINARY")
        if BINARY in command:
            raise ValueError("direct release-binary replacement was incomplete")
        mode = "direct_argument"
    else:
        variables = bound_variables(text, command)
        if len(variables) != 1:
            raise ValueError(
                "collector binary must be one direct literal or one uniquely bound variable; "
                f"observed_variables={variables}"
            )
        override = variables[0]
        mode = f"variable:{override}"

    prefix = " " * indentation(lines[command_index])
    staging_lines = [
        MARKER,
        f'TRNM_EVIDENCE_BINARY="{STAGED}"',
        "mkdir -p -- target/evidence",
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
    assert f'TRNM_EVIDENCE_BINARY="{STAGED}"' in patched
    assert "mkdir -p -- target/evidence" in patched
    assert patched.index("cargo build --release") < patched.index(MARKER)
    assert patched.index(MARKER) < patched.index("python3 scripts/trnm_build_evidence.py")

    external = patched.replace(
        f'TRNM_EVIDENCE_BINARY="{STAGED}"\n          mkdir -p -- target/evidence',
        'TRNM_EVIDENCE_BINARY="$RUNNER_TEMP/trnm-economy-service-evidence"',
    )
    upgraded, mode = patch_text(external)
    assert mode == "upgraded_external_to_repository_local"
    assert f'TRNM_EVIDENCE_BINARY="{STAGED}"' in upgraded
    assert "mkdir -p -- target/evidence" in upgraded
    again, mode = patch_text(upgraded)
    assert mode == "already_repository_local" and again == upgraded


def main() -> int:
    if len(sys.argv) == 2 and sys.argv[1] == "--self-test":
        self_test()
        print("TRNM_EVIDENCE_STAGING_V4_SELF_TEST=PASS")
        return 0
    if len(sys.argv) != 2:
        print("usage: patch-trnm-evidence-staging-v4.py WORKFLOW", file=sys.stderr)
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
