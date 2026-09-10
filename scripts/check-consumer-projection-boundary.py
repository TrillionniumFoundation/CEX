#!/usr/bin/env python3
"""Enforce that Consumer Entry World/League code remains projection-only."""

from __future__ import annotations

from pathlib import Path
import re
import sys
from typing import Iterable

from rust_route_contract import (
    RouteSyntaxError,
    Token,
    decode_string,
    extract_routes,
    tokenize,
)
from semantic_source_snapshot import regular_bytes

ROOT = Path(__file__).resolve().parents[1]
SOURCE_ROOT = ROOT / "services/consumer-entry-api/src"
POLICY = ROOT / "docs/repository-semantic-policy-v1.json"
MODULE_DOC = ROOT / "docs/modules/consumer-entry-api.md"

SPECIAL_FILES = {
    "real_world_map_shell.rs",
    "openstreetmap_geodata.rs",
    "trillionnium_world_adapters.rs",
}

SQL_IDENTIFIER = r'(?:[A-Za-z_][A-Za-z0-9_]*|"[A-Za-z_][A-Za-z0-9_]*"|`[A-Za-z_][A-Za-z0-9_]*`)'
SQL_TARGET = rf'(?P<target>{SQL_IDENTIFIER}(?:\s*\.\s*{SQL_IDENTIFIER})?)'
MUTATION_PATTERNS = (
    re.compile(rf"\binsert\s+into\s+{SQL_TARGET}", re.IGNORECASE),
    re.compile(
        rf"\bupdate\s+(?:only\s+)?{SQL_TARGET}"
        rf"(?:\s+(?:as\s+)?{SQL_IDENTIFIER})?\s+set\b",
        re.IGNORECASE,
    ),
    re.compile(rf"\bdelete\s+from\s+(?:only\s+)?{SQL_TARGET}", re.IGNORECASE),
    re.compile(rf"\btruncate(?:\s+table)?\s+{SQL_TARGET}", re.IGNORECASE),
)
DDL_RE = re.compile(
    r"\b(?:create|alter|drop)\s+(?:table|view|materialized\s+view|function|trigger)\b",
    re.IGNORECASE,
)

ALLOWED_MUTATION_PREFIXES = (
    "trillionnium_world_",
    "trillionnium_league_",
    "trillionnium_repository_",
    "trillionnium_tactics_",
    "trillionnium_item_",
    "trillionnium_resource_",
    "trillionnium_region_",
    "trillionnium_combat_",
    "world_",
    "league_",
    "consumer_",
    "edge_",
)
FORBIDDEN_AUTHORITY_PREFIXES = (
    "ledger_",
    "account_",
    "accounts",
    "audit_",
    "execution_",
    "provider_",
    "hepta_",
    "trnm_",
    "identity_",
)
FORBIDDEN_ROUTE_SEGMENTS = {
    "admin",
    "operator",
    "ledger",
    "audit",
    "execution",
    "provider",
    "mint",
    "debit",
    "credit",
    "finality",
    "production-authorize",
}
FORBIDDEN_LITERAL_PATTERNS = (
    re.compile(r'"authoritative"\s*:\s*true', re.IGNORECASE),
    re.compile(r'"production_authorization"\s*:\s*"granted"', re.IGNORECASE),
    re.compile(r'"chain_finality_verified"\s*:\s*true', re.IGNORECASE),
    re.compile(r'"ledger_settled"\s*:\s*true', re.IGNORECASE),
)
FORBIDDEN_INTERNAL_ENDPOINTS = (
    re.compile(
        r'(?:^|[\s\'"(=:])/(?:v[0-9]+/)?'
        r'(?:ledger|audit|executions?|providers?)(?=/|$|[?#{])',
        re.IGNORECASE,
    ),
    re.compile(
        r'(?:^|[\s\'"(=:])/(?:v[0-9]+/)?'
        r'(?:chain[-_]?finality|finality)(?=/|$|[?#{])',
        re.IGNORECASE,
    ),
    re.compile(
        r'/v[0-9]+/(?:ledger|audit|executions?|providers?|chain[-_]?finality|finality)'
        r'(?=/|$|[?#{])',
        re.IGNORECASE,
    ),
    re.compile(
        r'https?://[^\s/]+/(?:v[0-9]+/)?'
        r'(?:ledger|audit|executions?|providers?|chain[-_]?finality|finality)'
        r'(?=/|$|[?#{])',
        re.IGNORECASE,
    ),
)
FORBIDDEN_BOOLEAN_KEYS = {
    "authoritative",
    "chain_finality_verified",
    "ledger_settled",
}
FORBIDDEN_STRING_VALUES = {
    "production_authorization": "granted",
}


def projection_files() -> list[Path]:
    result: list[Path] = []
    for path in sorted(SOURCE_ROOT.glob("*.rs")):
        name = path.name
        if name.startswith("world_") or name.startswith("league_") or name in SPECIAL_FILES:
            result.append(path)
    if not result:
        raise AssertionError("no Consumer World/League projection source files discovered")
    return result


def source_line(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def rust_tokens(text: str, relative: str) -> list[Token]:
    try:
        return tokenize(text)
    except RouteSyntaxError as error:
        raise AssertionError(
            f"projection Rust lexical parsing failed in {relative}: {error}"
        ) from error


def decoded_literal(token: Token, relative: str) -> str:
    try:
        return decode_string(token)
    except RouteSyntaxError as error:
        raise AssertionError(
            f"projection Rust string decoding failed in {relative}: {error}"
        ) from error


def string_literals(tokens: Iterable[Token], relative: str) -> list[tuple[Token, str]]:
    return [
        (token, decoded_literal(token, relative))
        for token in tokens
        if token.kind in {"string", "raw_string"}
    ]


def sql_code_mask(text: str) -> str:
    """Mask SQL comments and quoted values while retaining identifier structure.

    This is deliberately not a SQL parser. It only prevents comments and values
    inside a Rust string literal from manufacturing mutation/DDL evidence.
    Double-quoted/backtick identifiers remain visible to the namespace check.
    """

    output = list(text)
    length = len(text)
    index = 0
    block_depth = 0

    def blank(start: int, end: int) -> None:
        for offset in range(start, min(end, length)):
            if output[offset] not in "\r\n":
                output[offset] = " "

    while index < length:
        if block_depth:
            if text.startswith("/*", index):
                blank(index, index + 2)
                block_depth += 1
                index += 2
            elif text.startswith("*/", index):
                blank(index, index + 2)
                block_depth -= 1
                index += 2
            else:
                if output[index] not in "\r\n":
                    output[index] = " "
                index += 1
            continue

        if text.startswith("--", index):
            end = text.find("\n", index + 2)
            end = length if end < 0 else end
            blank(index, end)
            index = end
            continue
        if text.startswith("/*", index):
            blank(index, index + 2)
            block_depth = 1
            index += 2
            continue
        if text[index] == "'":
            cursor = index + 1
            while cursor < length:
                if text[cursor] == "'":
                    if cursor + 1 < length and text[cursor + 1] == "'":
                        cursor += 2
                        continue
                    cursor += 1
                    break
                cursor += 1
            blank(index, cursor)
            index = cursor
            continue
        if text[index] == "$":
            tag = re.match(r"(?:\$\$|\$[A-Za-z_][A-Za-z0-9_]*\$)", text[index:])
            if tag:
                marker = tag.group(0)
                end = text.find(marker, index + len(marker))
                end = length if end < 0 else end + len(marker)
                blank(index, end)
                index = end
                continue
        index += 1

    return "".join(output)


def mutation_target(raw: str) -> str:
    final = re.split(r"\s*\.\s*", raw)[-1]
    return final.strip('"`').lower()


def authority_token_problems(
    tokens: list[Token], text: str, relative: str
) -> list[str]:
    problems: list[str] = []
    for index, token in enumerate(tokens):
        if token.kind not in {"string", "raw_string"}:
            continue
        key = decoded_literal(token, relative)
        if key not in FORBIDDEN_BOOLEAN_KEYS and key not in FORBIDDEN_STRING_VALUES:
            continue
        if index + 2 >= len(tokens) or tokens[index + 1].text != ":":
            continue
        value = tokens[index + 2]
        forbidden = key in FORBIDDEN_BOOLEAN_KEYS and value.kind == "ident" and value.text == "true"
        if key in FORBIDDEN_STRING_VALUES and value.kind in {"string", "raw_string"}:
            forbidden = decoded_literal(value, relative).lower() == FORBIDDEN_STRING_VALUES[key]
        if forbidden:
            problems.append(
                f"{relative}:{source_line(text, token.offset)}: projection code declares an authoritative outcome"
            )
    return problems


def check_file(path: Path) -> list[str]:
    text = regular_bytes(ROOT, path).decode("utf-8")
    relative = path.relative_to(ROOT).as_posix()
    problems: list[str] = []
    tokens = rust_tokens(text, relative)
    literals = string_literals(tokens, relative)

    for token, value in literals:
        sql = sql_code_mask(value)
        for match in DDL_RE.finditer(sql):
            problems.append(
                f"{relative}:{source_line(text, token.offset)}: runtime DDL is forbidden in projection code"
            )
        for expression in MUTATION_PATTERNS:
            for match in expression.finditer(sql):
                table = mutation_target(match.group("target"))
                if table.startswith(FORBIDDEN_AUTHORITY_PREFIXES):
                    problems.append(
                        f"{relative}:{source_line(text, token.offset)}: direct mutation of authoritative table {table!r}"
                    )
                elif not table.startswith(ALLOWED_MUTATION_PREFIXES):
                    problems.append(
                        f"{relative}:{source_line(text, token.offset)}: mutation target {table!r} lacks an approved projection namespace"
                    )

        for expression in FORBIDDEN_LITERAL_PATTERNS:
            if expression.search(value):
                problems.append(
                    f"{relative}:{source_line(text, token.offset)}: projection code declares an authoritative outcome"
                )

        for expression in FORBIDDEN_INTERNAL_ENDPOINTS:
            if expression.search(value):
                problems.append(
                    f"{relative}:{source_line(text, token.offset)}: projection code calls an internal authoritative endpoint directly"
                )

    problems.extend(authority_token_problems(tokens, text, relative))

    try:
        declarations = extract_routes(text)
    except RouteSyntaxError as error:
        raise AssertionError(f"projection route parsing failed in {relative}: {error}") from error
    for declaration in declarations:
        line = source_line(text, declaration.offset)
        if declaration.value is None:
            problems.append(f"{relative}:{line}: dynamic projection route path requires explicit reviewed resolution")
            continue
        route = declaration.value
        segments = {part.lower() for part in route.split("/") if part and not part.startswith(":")}
        forbidden = sorted(segments & FORBIDDEN_ROUTE_SEGMENTS)
        if forbidden:
            problems.append(f"{relative}:{line}: projection route {route!r} contains authoritative segment(s) {forbidden}")

    return list(dict.fromkeys(problems))


def check_contract_files() -> list[str]:
    problems: list[str] = []
    policy = POLICY.read_text(encoding="utf-8")
    module_doc = MODULE_DOC.read_text(encoding="utf-8")
    for marker in (
        '"id": "consumer-world-projection"',
        '"id": "consumer-league-projection"',
        '"authority_mode": "projection_only"',
        '"retirement": "externalize_to_trillionnium-world_then_remove_edge_compatibility"',
    ):
        if marker not in policy:
            problems.append(f"{POLICY.relative_to(ROOT)}: missing {marker}")
    for marker in (
        "World/League projections",
        "must not silently expand the edge into a second authoritative monolith",
        "Production authorization: `not_granted`",
    ):
        if marker not in module_doc:
            problems.append(f"{MODULE_DOC.relative_to(ROOT)}: missing boundary marker {marker!r}")
    return problems


def main() -> int:
    problems: list[str] = []
    files = projection_files()
    for path in files:
        problems.extend(check_file(path))
    problems.extend(check_contract_files())

    if problems:
        print("Consumer World/League projection boundary: FAILED", file=sys.stderr)
        for problem in problems:
            print(f"- {problem}", file=sys.stderr)
        return 1

    print(
        f"Consumer World/League projection boundary: OK ({len(files)} source files checked)"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, FileNotFoundError) as error:
        print(f"Consumer World/League projection boundary: FAILED: {error}", file=sys.stderr)
        raise SystemExit(1)
