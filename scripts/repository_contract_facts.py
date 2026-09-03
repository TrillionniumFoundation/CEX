#!/usr/bin/env python3
"""Derive deterministic repository contract facts from source and migrations."""

from __future__ import annotations

import json
import re
import tomllib
from pathlib import Path
from typing import Any, Iterable

INVENTORY_SCHEMA = "cex.repository-contract-inventory.v1"
CHECK_SCHEMA = "cex.repository-contract-inventory-check.v1"
PRODUCTION_AUTHORIZATION = "not_granted"

ENV_PATTERNS = (
    re.compile(r"(?:(?:std::)?env::var(?:_os)?|(?:std::)?env::remove_var|(?:std::)?env::set_var)\s*\(\s*\"([A-Z][A-Z0-9_]*)\""),
    re.compile(r"(?:required_env|trimmed_env|optional_non_empty_env|env_flag|bool_env|bounded_[a-z0-9_]*_env)\s*\(\s*\"([A-Z][A-Z0-9_]*)\""),
)
METRIC_RE = re.compile(r"\bcex_[a-zA-Z0-9_:]+\b")
SQL_OBJECT_PATTERNS = (
    ("table", re.compile(r"\bcreate\s+(?:unlogged\s+)?table\s+(?:if\s+not\s+exists\s+)?([a-zA-Z0-9_.\"]+)", re.I)),
    ("view", re.compile(r"\bcreate\s+(?:or\s+replace\s+)?view\s+([a-zA-Z0-9_.\"]+)", re.I)),
    ("materialized_view", re.compile(r"\bcreate\s+materialized\s+view\s+(?:if\s+not\s+exists\s+)?([a-zA-Z0-9_.\"]+)", re.I)),
    ("function", re.compile(r"\bcreate\s+(?:or\s+replace\s+)?function\s+([a-zA-Z0-9_.\"]+)", re.I)),
    ("type", re.compile(r"\bcreate\s+type\s+([a-zA-Z0-9_.\"]+)", re.I)),
    ("trigger", re.compile(r"\bcreate\s+(?:constraint\s+)?trigger\s+([a-zA-Z0-9_.\"]+)", re.I)),
)


def _line(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def _decode_string(value: str) -> str:
    try:
        return json.loads('"' + value + '"')
    except json.JSONDecodeError:
        return value


def mask_comments(text: str) -> str:
    """Replace Rust comments with spaces while preserving strings and offsets."""
    out = list(text)
    i = 0
    state = "code"
    block_depth = 0
    while i < len(text):
        ch = text[i]
        nxt = text[i + 1] if i + 1 < len(text) else ""
        if state == "code":
            if ch == '"':
                state = "string"
            elif ch == "'":
                state = "char"
            elif ch == "/" and nxt == "/":
                out[i] = out[i + 1] = " "
                i += 1
                state = "line_comment"
            elif ch == "/" and nxt == "*":
                out[i] = out[i + 1] = " "
                i += 1
                state = "block_comment"
                block_depth = 1
        elif state == "string":
            if ch == "\\":
                i += 1
            elif ch == '"':
                state = "code"
        elif state == "char":
            if ch == "\\":
                i += 1
            elif ch == "'":
                state = "code"
        elif state == "line_comment":
            if ch == "\n":
                state = "code"
            else:
                out[i] = " "
        else:
            if ch == "/" and nxt == "*":
                out[i] = out[i + 1] = " "
                i += 1
                block_depth += 1
            elif ch == "*" and nxt == "/":
                out[i] = out[i + 1] = " "
                i += 1
                block_depth -= 1
                if block_depth == 0:
                    state = "code"
            elif ch != "\n":
                out[i] = " "
        i += 1
    return "".join(out)


def _balanced_call(text: str, open_paren: int) -> tuple[str, int] | None:
    depth = 0
    i = open_paren
    state = "code"
    block_depth = 0
    while i < len(text):
        ch = text[i]
        nxt = text[i + 1] if i + 1 < len(text) else ""
        if state == "code":
            if ch == '"':
                state = "string"
            elif ch == "'":
                state = "char"
            elif ch == "/" and nxt == "/":
                state = "line_comment"
                i += 1
            elif ch == "/" and nxt == "*":
                state = "block_comment"
                block_depth = 1
                i += 1
            elif ch == "(":
                depth += 1
            elif ch == ")":
                depth -= 1
                if depth == 0:
                    return text[open_paren + 1 : i], i + 1
        elif state == "string":
            if ch == "\\":
                i += 1
            elif ch == '"':
                state = "code"
        elif state == "char":
            if ch == "\\":
                i += 1
            elif ch == "'":
                state = "code"
        elif state == "line_comment":
            if ch == "\n":
                state = "code"
        elif ch == "/" and nxt == "*":
            block_depth += 1
            i += 1
        elif ch == "*" and nxt == "/":
            block_depth -= 1
            i += 1
            if block_depth == 0:
                state = "code"
        i += 1
    return None


def extract_routes(text: str, source: str) -> tuple[list[dict[str, Any]], int]:
    masked = mask_comments(text)
    routes: list[dict[str, Any]] = []
    unresolved = 0
    for match in re.finditer(r"\.route(?:_service)?\s*\(", masked):
        open_paren = masked.find("(", match.start())
        call = _balanced_call(masked, open_paren)
        if call is None:
            unresolved += 1
            continue
        body, _ = call
        path_match = re.match(r"\s*\"((?:\\.|[^\"\\])*)\"\s*,", body, re.S)
        if not path_match:
            unresolved += 1
            continue
        route_path = _decode_string(path_match.group(1))
        method_matches = list(
            re.finditer(
                r"\b(get|post|put|delete|patch|head|options|trace|any)\s*\(\s*([A-Za-z_][A-Za-z0-9_:]*)",
                body[path_match.end() :],
            )
        )
        if not method_matches:
            unresolved += 1
            continue
        for method in method_matches:
            routes.append(
                {
                    "method": method.group(1).upper(),
                    "path": route_path,
                    "handler": method.group(2),
                    "source": source,
                    "line": _line(text, match.start()),
                }
            )
    routes.sort(key=lambda item: (item["path"], item["method"], item["source"], item["line"]))
    return routes, unresolved


def extract_configuration(text: str, source: str) -> list[dict[str, Any]]:
    masked = mask_comments(text)
    found: dict[tuple[str, int], dict[str, Any]] = {}
    for pattern in ENV_PATTERNS:
        for match in pattern.finditer(masked):
            key = match.group(1)
            line = _line(text, match.start())
            found[(key, line)] = {"key": key, "source": source, "line": line}
    return [found[key] for key in sorted(found)]


def extract_metrics(text: str, source: str) -> list[dict[str, Any]]:
    masked = mask_comments(text)
    found: dict[tuple[str, int], dict[str, Any]] = {}
    for match in METRIC_RE.finditer(masked):
        name = match.group(0).rstrip(":")
        line = _line(text, match.start())
        found[(name, line)] = {"name": name, "source": source, "line": line}
    return [found[key] for key in sorted(found)]


def _target(path: Path, root: Path, kind: str, name: str, source: str) -> dict[str, str]:
    return {
        "kind": kind,
        "name": name,
        "path": path.relative_to(root).as_posix(),
        "discovery": source,
    }


def discover_targets(root: Path, member: str, manifest: dict[str, Any]) -> list[dict[str, str]]:
    member_root = root / member
    package = manifest.get("package") if isinstance(manifest.get("package"), dict) else {}
    package_name = str(package.get("name") or Path(member).name)
    targets: dict[tuple[str, str], dict[str, str]] = {}

    def add(path: Path, kind: str, name: str, source: str) -> None:
        if path.is_file():
            value = _target(path, root, kind, name, source)
            targets[(kind, value["path"])] = value

    if package.get("autolib", True) is not False:
        add(member_root / "src/lib.rs", "lib", package_name, "conventional")
    if package.get("autobins", True) is not False:
        add(member_root / "src/main.rs", "bin", package_name, "conventional")
        bin_root = member_root / "src/bin"
        if bin_root.is_dir():
            for path in sorted(bin_root.rglob("*.rs")):
                name = path.parent.name if path.name == "main.rs" else path.stem
                add(path, "bin", name, "auto")
    for directory, kind, enabled in (
        ("tests", "test", package.get("autotests", True) is not False),
        ("examples", "example", package.get("autoexamples", True) is not False),
        ("benches", "bench", package.get("autobenches", True) is not False),
    ):
        base = member_root / directory
        if enabled and base.is_dir():
            for path in sorted(base.rglob("*.rs")):
                add(path, kind, path.stem, "auto")
    build = package.get("build")
    if isinstance(build, str) and build:
        add(member_root / build, "build", "build-script", "explicit")
    elif build is True or (build is None and (member_root / "build.rs").is_file()):
        add(member_root / "build.rs", "build", "build-script", "conventional")

    for key, kind in (("bin", "bin"), ("test", "test"), ("example", "example"), ("bench", "bench")):
        entries = manifest.get(key)
        if not isinstance(entries, list):
            continue
        for entry in entries:
            if not isinstance(entry, dict):
                continue
            path_value = entry.get("path")
            name = str(entry.get("name") or "unnamed")
            if isinstance(path_value, str) and path_value:
                add(member_root / path_value, kind, name, "explicit")
    return sorted(targets.values(), key=lambda item: (item["kind"], item["path"]))


def _load_toml(path: Path, problems: list[str]) -> dict[str, Any]:
    try:
        value = tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        problems.append(f"cannot load TOML {path.as_posix()}: {error}")
        return {}
    return value if isinstance(value, dict) else {}


def _load_json(path: Path, problems: list[str]) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        problems.append(f"cannot load JSON {path.as_posix()}: {error}")
        return {}
    return value if isinstance(value, dict) else {}


def _sql_files(root: Path, members: Iterable[str]) -> list[Path]:
    paths: set[Path] = set()
    root_migrations = root / "migrations"
    if root_migrations.is_dir():
        paths.update(root_migrations.rglob("*.sql"))
    for member in members:
        directory = root / member / "migrations"
        if directory.is_dir():
            paths.update(directory.rglob("*.sql"))
    return sorted(paths)


def extract_sql_objects(root: Path, paths: Iterable[Path], problems: list[str]) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    for path in paths:
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError) as error:
            problems.append(f"cannot read SQL {path.relative_to(root).as_posix()}: {error}")
            continue
        stripped = re.sub(
            r"--[^\n]*|/\*.*?\*/",
            lambda match: "\n" * match.group(0).count("\n"),
            text,
            flags=re.S,
        )
        source = path.relative_to(root).as_posix()
        for kind, pattern in SQL_OBJECT_PATTERNS:
            for match in pattern.finditer(stripped):
                result.append(
                    {
                        "kind": kind,
                        "name": match.group(1).replace('"', ""),
                        "source": source,
                        "line": _line(stripped, match.start()),
                    }
                )
    result.sort(key=lambda item: (item["kind"], item["name"], item["source"], item["line"]))
    return result


def build_inventory(root: Path) -> dict[str, Any]:
    root = root.resolve()
    problems: list[str] = []
    cargo = _load_toml(root / "Cargo.toml", problems)
    workspace = cargo.get("workspace") if isinstance(cargo.get("workspace"), dict) else {}
    raw_members = workspace.get("members") if isinstance(workspace.get("members"), list) else []
    members = [value for value in raw_members if isinstance(value, str) and value]
    if not members:
        problems.append("Cargo.toml workspace.members is empty or invalid")

    catalog = _load_json(root / "docs/module-catalog-v1.json", problems)
    catalog_entries = catalog.get("modules") if isinstance(catalog.get("modules"), list) else []
    catalog_by_member = {
        str(item.get("workspace_member")): item
        for item in catalog_entries
        if isinstance(item, dict) and isinstance(item.get("workspace_member"), str)
    }

    member_records: list[dict[str, Any]] = []
    totals = {
        "targets": 0,
        "routes": 0,
        "configuration_keys": 0,
        "metrics": 0,
        "unresolved_route_calls": 0,
    }
    for member in members:
        member_path = Path(member)
        if member_path.is_absolute() or ".." in member_path.parts or "\\" in member:
            problems.append(f"workspace member escapes repository: {member}")
            continue
        manifest = _load_toml(root / member / "Cargo.toml", problems)
        package_table = manifest.get("package") if isinstance(manifest.get("package"), dict) else {}
        package = package_table.get("name")
        if not isinstance(package, str) or not package:
            problems.append(f"workspace member lacks package.name: {member}")
            package = Path(member).name
        targets = discover_targets(root, member, manifest)
        if not targets:
            problems.append(f"workspace member has no discoverable Cargo target: {member}")

        routes: list[dict[str, Any]] = []
        configuration: list[dict[str, Any]] = []
        metrics: list[dict[str, Any]] = []
        unresolved = 0
        source_root = root / member / "src"
        if source_root.is_dir():
            for source_path in sorted(source_root.rglob("*.rs")):
                try:
                    text = source_path.read_text(encoding="utf-8")
                except (OSError, UnicodeDecodeError) as error:
                    problems.append(f"cannot read Rust source {source_path.relative_to(root).as_posix()}: {error}")
                    continue
                source = source_path.relative_to(root).as_posix()
                extracted, missing = extract_routes(text, source)
                routes.extend(extracted)
                unresolved += missing
                configuration.extend(extract_configuration(text, source))
                metrics.extend(extract_metrics(text, source))

        entry = catalog_by_member.get(member)
        if not isinstance(entry, dict):
            problems.append(f"module catalog lacks workspace member: {member}")
            catalogued: set[str] = set()
        else:
            catalogued = {
                str(value)
                for value in entry.get("source_entrypoints", [])
                if isinstance(value, str)
            }
            if entry.get("package") != package:
                problems.append(
                    f"module catalog package mismatch for {member}: "
                    f"{entry.get('package')!r} != {package!r}"
                )
        required_targets = {
            item["path"]
            for item in targets
            if item["kind"] in {"lib", "bin", "build"}
            or item["discovery"] == "explicit"
        }
        missing_targets = sorted(required_targets - catalogued)
        if missing_targets:
            problems.append(
                f"module catalog omits Cargo targets for {member}: {missing_targets}"
            )

        configuration.sort(key=lambda item: (item["key"], item["source"], item["line"]))
        metrics.sort(key=lambda item: (item["name"], item["source"], item["line"]))
        routes.sort(key=lambda item: (item["path"], item["method"], item["source"], item["line"]))
        member_records.append(
            {
                "workspace_member": member,
                "package": package,
                "targets": targets,
                "routes": routes,
                "configuration": configuration,
                "metrics": metrics,
                "unresolved_route_calls": unresolved,
                "catalog_missing_required_targets": missing_targets,
            }
        )
        totals["targets"] += len(targets)
        totals["routes"] += len(routes)
        totals["configuration_keys"] += len({item["key"] for item in configuration})
        totals["metrics"] += len({item["name"] for item in metrics})
        totals["unresolved_route_calls"] += unresolved

    sql_objects = extract_sql_objects(root, _sql_files(root, members), problems)
    totals["sql_objects"] = len(sql_objects)
    member_records.sort(key=lambda item: item["workspace_member"])
    return {
        "schema": INVENTORY_SCHEMA,
        "status": "failed" if problems else "ok",
        "workspace_member_count": len(member_records),
        "members": member_records,
        "sql_objects": sql_objects,
        "summary": totals,
        "checker_may_grant_production_authorization": False,
        "production_authorization": PRODUCTION_AUTHORIZATION,
        "problems": problems,
    }


def check_result(inventory: dict[str, Any], output: str | None) -> dict[str, Any]:
    return {
        "schema": CHECK_SCHEMA,
        "status": inventory.get("status"),
        "inventory_schema": inventory.get("schema"),
        "workspace_member_count": inventory.get("workspace_member_count"),
        "summary": inventory.get("summary"),
        "output": output,
        "checker_may_grant_production_authorization": False,
        "production_authorization": PRODUCTION_AUTHORIZATION,
        "problems": inventory.get("problems", []),
    }
