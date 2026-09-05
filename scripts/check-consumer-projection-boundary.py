#!/usr/bin/env python3
"""Enforce that Consumer Entry World/League code remains projection-only."""

from __future__ import annotations

from pathlib import Path
import re
import sys

from rust_route_contract import RouteSyntaxError, extract_routes
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

MUTATION_RE = re.compile(
    r"\b(?:insert\s+into|update|delete\s+from|truncate(?:\s+table)?)\s+"
    r"(?:(?:public|[a-zA-Z_][a-zA-Z0-9_]*)\.)?[\"`]?([a-zA-Z_][a-zA-Z0-9_]*)",
    re.IGNORECASE,
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
FORBIDDEN_LITERAL_PATTERNS = [
    re.compile(r'\"authoritative\"\s*:\s*true', re.IGNORECASE),
    re.compile(r'\"production_authorization\"\s*:\s*\"granted\"', re.IGNORECASE),
    re.compile(r'\"chain_finality_verified\"\s*:\s*true', re.IGNORECASE),
    re.compile(r'\"ledger_settled\"\s*:\s*true', re.IGNORECASE),
]
FORBIDDEN_INTERNAL_ENDPOINTS = [
    re.compile(r'/(?:v[0-9]+/)?(?:ledger|audit|executions?|providers?)(?:/|\")', re.IGNORECASE),
    re.compile(r'/(?:v[0-9]+/)?(?:chain[-_]?finality|finality)(?:/|\")', re.IGNORECASE),
]


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


def check_file(path: Path) -> list[str]:
    text = regular_bytes(ROOT, path).decode("utf-8")
    relative = path.relative_to(ROOT).as_posix()
    problems: list[str] = []

    for match in DDL_RE.finditer(text):
        problems.append(
            f"{relative}:{source_line(text, match.start())}: runtime DDL is forbidden in projection code"
        )

    for match in MUTATION_RE.finditer(text):
        table = match.group(1).lower()
        if table.startswith(FORBIDDEN_AUTHORITY_PREFIXES):
            problems.append(
                f"{relative}:{source_line(text, match.start())}: direct mutation of authoritative table {table!r}"
            )
        elif not table.startswith(ALLOWED_MUTATION_PREFIXES):
            problems.append(
                f"{relative}:{source_line(text, match.start())}: mutation target {table!r} lacks an approved projection namespace"
            )

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

    for expression in FORBIDDEN_LITERAL_PATTERNS:
        for match in expression.finditer(text):
            problems.append(
                f"{relative}:{source_line(text, match.start())}: projection code declares an authoritative outcome"
            )

    for expression in FORBIDDEN_INTERNAL_ENDPOINTS:
        for match in expression.finditer(text):
            problems.append(
                f"{relative}:{source_line(text, match.start())}: projection code calls an internal authoritative endpoint directly"
            )

    return problems


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
