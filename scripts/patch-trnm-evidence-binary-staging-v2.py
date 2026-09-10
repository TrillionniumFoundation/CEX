#!/usr/bin/env python3
"""Stage only the built TRNM binary before the evidence collector consumes it."""

from __future__ import annotations

from pathlib import Path
import sys

SOURCE = "target/release/trnm-economy-service"
QUOTED_SOURCE = f'"{SOURCE}"'
STAGED = '"$TRNM_BUILD_EVIDENCE_BINARY"'
MARKER = "TRNM_BUILD_EVIDENCE_SINGLE_LINK_STAGING_V1"


def blocks(lines: list[str]) -> list[tuple[int, int, int]]:
    result: list[tuple[int, int, int]] = []
    for start, line in enumerate(lines):
        stripped = line.lstrip(" ")
        indent = len(line) - len(stripped)
        if stripped.rstrip("\r\n") != "run: |":
            continue
        end = start + 1
        while end < len(lines):
            candidate = lines[end]
            if candidate.strip() and len(candidate) - len(candidate.lstrip(" ")) <= indent:
                break
            end += 1
        result.append((start, end, indent))
    return result


def main() -> int:
    if len(sys.argv) != 2:
        return 64
    path = Path(sys.argv[1])
    text = path.read_text(encoding="utf-8")
    if MARKER in text:
        print("TRNM_EVIDENCE_BINARY_STAGING=ALREADY_PRESENT")
        return 0
    lines = text.splitlines(keepends=True)
    selected = []
    for start, end, indent in blocks(lines):
        body = "".join(lines[start + 1 : end])
        if "scripts/trnm_build_evidence.py" in body and SOURCE in body:
            selected.append((start, end, indent, body))
    if len(selected) != 1:
        print(f"expected one collector block, found {len(selected)}", file=sys.stderr)
        return 1
    start, end, indent, body = selected[0]
    replacements = body.count(SOURCE)
    body = body.replace(QUOTED_SOURCE, STAGED).replace(SOURCE, STAGED)
    body_indent = " " * (indent + 2)
    prelude = "".join(
        [
            f"{body_indent}# {MARKER}\n",
            f'{body_indent}evidence_binary_dir="$RUNNER_TEMP/trnm-build-evidence"\n',
            f'{body_indent}rm -rf "$evidence_binary_dir"\n',
            f'{body_indent}mkdir -p "$evidence_binary_dir"\n',
            f'{body_indent}install -m 0555 {SOURCE} "$evidence_binary_dir/trnm-economy-service"\n',
            f'{body_indent}export TRNM_BUILD_EVIDENCE_BINARY="$evidence_binary_dir/trnm-economy-service"\n',
            f'{body_indent}test "$(stat -c %h "$TRNM_BUILD_EVIDENCE_BINARY")" = 1\n',
            f'{body_indent}source_binary_sha="$(sha256sum {SOURCE} | awk \'{{print $1}}\')"\n',
            f'{body_indent}staged_binary_sha="$(sha256sum "$TRNM_BUILD_EVIDENCE_BINARY" | awk \'{{print $1}}\')"\n',
            f'{body_indent}test "$source_binary_sha" = "$staged_binary_sha"\n',
        ]
    )
    lines[start + 1 : end] = [prelude + body]
    patched = "".join(lines)
    if patched.count(MARKER) != 1 or patched.count(STAGED) < replacements:
        print("staging patch cardinality failed", file=sys.stderr)
        return 1
    path.write_text(patched, encoding="utf-8", newline="\n")
    print(f"TRNM_EVIDENCE_BINARY_STAGING=PATCHED replacements={replacements}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
