#!/usr/bin/env python3
"""Generate/check semantic ownership for repository routes, config and SQL data facts.

The output is derived from source bytes plus docs/repository-semantic-policy-v1.json.
It intentionally does not claim runtime reachability or production authorization.
"""

from __future__ import annotations

import argparse
import fnmatch
import hashlib
import json
import os
import tempfile
from pathlib import Path
import re
import sys
from typing import Any, Iterable

from rust_route_contract import RouteSyntaxError, extract_routes
from semantic_source_snapshot import InputSnapshot, regular_bytes, require_complete_workspace, tracked_paths

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUTPUT = ROOT / "docs/repository-contract-semantics-v1.json"
POLICY_PATH = ROOT / "docs/repository-semantic-policy-v1.json"
MODULE_CATALOG_PATH = ROOT / "docs/module-catalog-v1.json"

CONFIG_CALL_RE = re.compile(
    r"(?:std::)?env::(?:var|var_os)\(\s*\"([A-Z][A-Z0-9_]{2,})\""
    r"|(?:required_env|optional_env|get_env|read_env|parse_[a-zA-Z0-9_]*_env)"
    r"\(\s*\"([A-Z][A-Z0-9_]{2,})\"",
    re.MULTILINE,
)
UPPER_LITERAL_RE = re.compile(r'\"([A-Z][A-Z0-9_]{2,})\"')
SQL_OBJECT_RE = re.compile(
    r"\b(?:create\s+table(?:\s+if\s+not\s+exists)?|alter\s+table(?:\s+if\s+exists)?|"
    r"drop\s+table(?:\s+if\s+exists)?|insert\s+into|update|delete\s+from|references)\s+"
    r"(?:(?:public|[a-zA-Z_][a-zA-Z0-9_]*)\.)?\"?([a-zA-Z_][a-zA-Z0-9_]*)\"?",
    re.IGNORECASE,
)
SQL_VIEW_RE = re.compile(
    r"\bcreate\s+(?:materialized\s+)?view(?:\s+if\s+not\s+exists)?\s+"
    r"(?:(?:public|[a-zA-Z_][a-zA-Z0-9_]*)\.)?\"?([a-zA-Z_][a-zA-Z0-9_]*)\"?",
    re.IGNORECASE,
)
SECRET_KEY_RE = re.compile(
    r"(?:TOKEN|SECRET|PASSWORD|PASSWD|PRIVATE_KEY|SIGNING_KEY|API_KEY|CREDENTIAL|DATABASE_URL|DSN)",
    re.IGNORECASE,
)
ENDPOINT_KEY_RE = re.compile(r"(?:URL|URI|ENDPOINT|HOST|HOMESERVER)", re.IGNORECASE)

SOURCE_SUFFIXES = {".rs", ".js", ".mjs", ".ts", ".tsx"}
SOURCE_ROOTS = (ROOT / "apps", ROOT / "services", ROOT / "crates")
SQL_GLOBS = (
    "migrations/*.sql",
    "services/*/migrations/*.sql",
    "services/*/migrations/**/*.sql",
    "deploy/sql/*.sql",
)


def sha256_bytes(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def line_number(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def normalized_path(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def source_files() -> Iterable[Path]:
    for root in SOURCE_ROOTS:
        if root.is_symlink():
            raise AssertionError("semantic source root must not be a symbolic link")
        if not root.exists():
            continue
        for path in sorted(root.rglob("*")):
            relative = normalized_path(path)
            if any(part in {"target", "node_modules", "vendor"} for part in path.relative_to(ROOT).parts):
                continue
            if path.is_symlink():
                raise AssertionError(f"symbolic path under semantic source root: {relative}")
            if path.suffix not in SOURCE_SUFFIXES:
                continue
            if not path.is_file():
                if not path.is_dir():
                    raise AssertionError(f"nonregular semantic source: {relative}")
                continue
            yield path


def sql_files() -> Iterable[Path]:
    seen: set[Path] = set()
    for pattern in SQL_GLOBS:
        for path in sorted(ROOT.glob(pattern)):
            if path.is_file() and path not in seen:
                seen.add(path)
                yield path


def load_json(path: Path, snapshot: InputSnapshot | None = None) -> dict[str, Any]:
    try:
        data = json.loads(snapshot.read(path) if snapshot else regular_bytes(ROOT, path))
    except FileNotFoundError as error:
        raise AssertionError(f"required file is absent: {normalized_path(path)}") from error
    except json.JSONDecodeError as error:
        raise AssertionError(f"invalid JSON in {normalized_path(path)}: {error}") from error
    if not isinstance(data, dict):
        raise AssertionError(f"expected JSON object in {normalized_path(path)}")
    return data


def load_modules(snapshot: InputSnapshot | None = None) -> list[dict[str, Any]]:
    catalog = load_json(MODULE_CATALOG_PATH, snapshot)
    modules = catalog.get("modules")
    if not isinstance(modules, list) or not modules:
        raise AssertionError("module catalog has no modules")
    normalized: list[dict[str, Any]] = []
    for module in modules:
        if not isinstance(module, dict):
            raise AssertionError("module catalog entry is not an object")
        for required in ("workspace_member", "package", "owner", "authority"):
            if not isinstance(module.get(required), str) or not module[required].strip():
                raise AssertionError(f"module catalog entry lacks {required}: {module}")
        normalized.append(module)
    normalized.sort(key=lambda item: len(item["workspace_member"]), reverse=True)
    return normalized


def owning_module(path: str, modules: list[dict[str, Any]]) -> dict[str, Any] | None:
    for module in modules:
        root = module["workspace_member"].rstrip("/")
        if path == root or path.startswith(root + "/"):
            return module
    return None


def compile_policy(snapshot: InputSnapshot | None = None) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    policy = load_json(POLICY_PATH, snapshot)
    rules = policy.get("rules")
    if not isinstance(rules, list) or not rules:
        raise AssertionError("semantic policy has no rules")
    required_fields = policy.get("required_fields")
    if not isinstance(required_fields, list) or not required_fields:
        raise AssertionError("semantic policy required_fields is absent")

    compiled: list[dict[str, Any]] = []
    ids: set[str] = set()
    for index, rule in enumerate(rules):
        if not isinstance(rule, dict):
            raise AssertionError(f"semantic rule {index} is not an object")
        rule_id = rule.get("id")
        if not isinstance(rule_id, str) or not rule_id:
            raise AssertionError(f"semantic rule {index} has no id")
        if rule_id in ids:
            raise AssertionError(f"duplicate semantic rule id: {rule_id}")
        ids.add(rule_id)
        fact_types = rule.get("fact_types")
        path_globs = rule.get("path_globs")
        if not isinstance(fact_types, list) or not fact_types:
            raise AssertionError(f"semantic rule {rule_id} has no fact_types")
        if not isinstance(path_globs, list) or not path_globs:
            raise AssertionError(f"semantic rule {rule_id} has no path_globs")
        unknown_types = sorted(set(fact_types) - {"route", "config", "data"})
        if unknown_types:
            raise AssertionError(f"semantic rule {rule_id} has unknown fact types: {unknown_types}")
        for field in required_fields:
            if not isinstance(rule.get(field), str) or not rule[field].strip():
                raise AssertionError(f"semantic rule {rule_id} lacks {field}")
        item = dict(rule)
        expression = item.get("value_regex")
        item["_compiled_value_regex"] = re.compile(expression) if expression else None
        compiled.append(item)
    return policy, compiled


def matches_rule(rule: dict[str, Any], fact: dict[str, Any]) -> bool:
    if fact["fact_type"] not in rule["fact_types"]:
        return False
    if not any(fnmatch.fnmatch(fact["source_path"], pattern) for pattern in rule["path_globs"]):
        return False
    value_regex = rule.get("_compiled_value_regex")
    return value_regex is None or value_regex.search(fact["value"]) is not None


def select_rule(rules: list[dict[str, Any]], fact: dict[str, Any]) -> dict[str, Any]:
    for rule in rules:
        if matches_rule(rule, fact):
            return rule
    raise AssertionError(
        f"unclassified {fact['fact_type']} fact {fact['value']!r} in {fact['source_path']}"
    )


def extract_route_facts(path: Path, text: str) -> list[dict[str, Any]]:
    if path.suffix != ".rs":
        return []  # Non-Rust routing is explicitly excluded in coverage metadata.
    relative = normalized_path(path)
    try:
        declarations = extract_routes(text)
    except RouteSyntaxError as error:
        raise AssertionError(f"route extraction failed in {relative}: {error}") from error
    return [
        {
            "fact_type": "route",
            "value": declaration.value if declaration.value is not None else "<dynamic-route-path>",
            "methods": list(declaration.methods),
            "observed_methods": list(declaration.observed_methods),
            "registration": declaration.registration,
            "path_resolution": declaration.path_resolution,
            "method_resolution": declaration.method_resolution,
            "expression_sha256": declaration.expression_sha256,
            "source_path": relative,
            "source_line": line_number(text, declaration.offset),
            "source_column": declaration.offset - text.rfind("\n", 0, declaration.offset),
        }
        for declaration in declarations
    ]


def looks_like_config_key(value: str, context: str) -> bool:
    if "_" not in value or value.startswith(("HTTP_", "CONTENT_", "ACCESS_CONTROL_")):
        return False
    context_upper = context.upper()
    return any(
        marker in context_upper
        for marker in ("ENV", "CONFIG", "PROFILE", "SETTING", "REQUIRED", "OPTIONAL")
    )


def extract_config_facts(path: Path, text: str) -> list[dict[str, Any]]:
    relative = normalized_path(path)
    values: dict[str, int] = {}
    for match in CONFIG_CALL_RE.finditer(text):
        value = match.group(1) or match.group(2)
        values.setdefault(value, line_number(text, match.start()))
    for match in UPPER_LITERAL_RE.finditer(text):
        value = match.group(1)
        context = text[max(0, match.start() - 100) : match.end() + 100]
        if looks_like_config_key(value, context):
            values.setdefault(value, line_number(text, match.start()))
    return [
        {
            "fact_type": "config",
            "value": value,
            "source_path": relative,
            "source_line": values[value],
        }
        for value in sorted(values)
    ]


def extract_data_facts(path: Path, text: str) -> list[dict[str, Any]]:
    relative = normalized_path(path)
    values: dict[str, int] = {}
    for expression in (SQL_OBJECT_RE, SQL_VIEW_RE):
        for match in expression.finditer(text):
            value = match.group(1).lower()
            values.setdefault(value, line_number(text, match.start()))
    return [
        {
            "fact_type": "data",
            "value": value,
            "source_path": relative,
            "source_line": values[value],
        }
        for value in sorted(values)
    ]


def config_classification(key: str, policy_value: str) -> str:
    if SECRET_KEY_RE.search(key):
        return "secret_or_sensitive_reference"
    if ENDPOINT_KEY_RE.search(key):
        return "trusted_endpoint_or_network_identity"
    return policy_value


def semantic_fact(
    raw: dict[str, Any],
    rule: dict[str, Any],
    module: dict[str, Any] | None,
) -> dict[str, Any]:
    package = module["package"] if module else "repository-migrations"
    workspace_member = module["workspace_member"] if module else None
    owner = module["owner"] if module else "platform-data-governance"
    authority_mode = rule["authority_mode"]
    authority_statement = (
        module["authority"]
        if authority_mode == "inherit_exact_module_catalog_authority" and module
        else authority_mode
    )
    data_classification = rule["data_classification"]
    if raw["fact_type"] == "config":
        data_classification = config_classification(raw["value"], data_classification)

    identity_payload = "\0".join(
        [raw["fact_type"], raw["source_path"], str(raw["source_line"]), raw["value"]]
    ).encode("utf-8")
    if raw["fact_type"] == "route":
        identity_payload += ("\0" + str(raw["source_column"]) + "\0" + raw["expression_sha256"]).encode("utf-8")
    result: dict[str, Any] = {
        "fact_id": sha256_bytes(identity_payload),
        "fact_type": raw["fact_type"],
        "value": raw["value"],
        "source_path": raw["source_path"],
        "source_line": raw["source_line"],
        "workspace_member": workspace_member,
        "package": package,
        "owner": owner,
        "policy_rule_id": rule["id"],
        "bounded_context": rule["bounded_context"],
        "authority_mode": authority_mode,
        "authority_statement": authority_statement,
        "authentication": rule["authentication"],
        "authorization": rule["authorization"],
        "data_classification": data_classification,
        "retirement": rule["retirement"],
        "production_authorization": "not_granted",
    }
    if "methods" in raw:
        for field in ("methods", "observed_methods", "registration", "path_resolution",
                      "method_resolution", "expression_sha256", "source_column"):
            result[field] = raw[field]
    return result


def enforce_consumer_projection_boundary(facts: list[dict[str, Any]]) -> None:
    world_prefixes = (
        "services/consumer-entry-api/src/world_",
        "services/consumer-entry-api/src/real_world_map_shell.rs",
        "services/consumer-entry-api/src/openstreetmap_geodata.rs",
        "services/consumer-entry-api/src/trillionnium_world_adapters.rs",
        "services/consumer-entry-api/src/league_",
    )
    violations = [
        fact
        for fact in facts
        if fact["source_path"].startswith(world_prefixes)
        and fact["authority_mode"] != "projection_only"
    ]
    if violations:
        preview = [
            f"{item['source_path']}:{item['source_line']}:{item['value']}={item['authority_mode']}"
            for item in violations[:20]
        ]
        raise AssertionError(f"Consumer World/League facts escaped projection_only: {preview}")

    data_violations = [
        fact
        for fact in facts
        if fact["fact_type"] == "data"
        and re.match(r"^trillionnium_(?:world|league)_", fact["value"])
        and fact["authority_mode"] != "projection_only"
    ]
    if data_violations:
        raise AssertionError("Trillionnium World/League SQL objects escaped projection_only")


def build_document() -> dict[str, Any]:
    snapshot = InputSnapshot(ROOT)
    parser_bytes = snapshot.read(ROOT / "scripts/rust_route_contract.py")
    snapshot.read(ROOT / "scripts/semantic_source_snapshot.py")
    snapshot.read(ROOT / "scripts/generate-repository-semantics.py")
    policy, rules = compile_policy(snapshot)
    modules = load_modules(snapshot)
    git_inventory = require_complete_workspace(snapshot, modules)
    code_paths, data_paths = tuple(source_files()), tuple(sql_files())
    raw_facts: list[dict[str, Any]] = []
    source_hash_inputs: list[bytes] = []

    for path in code_paths:
        data = snapshot.read(path)
        source_hash_inputs.extend([normalized_path(path).encode(), b"\0", data, b"\0"])
        text = data.decode("utf-8")
        raw_facts.extend(extract_route_facts(path, text))
        raw_facts.extend(extract_config_facts(path, text))

    for path in data_paths:
        data = snapshot.read(path)
        source_hash_inputs.extend([normalized_path(path).encode(), b"\0", data, b"\0"])
        text = data.decode("utf-8")
        raw_facts.extend(extract_data_facts(path, text))

    facts: list[dict[str, Any]] = []
    seen_ids: set[str] = set()
    for raw in sorted(
        raw_facts,
        key=lambda item: (
            item["fact_type"],
            item["source_path"],
            item["source_line"],
            item["value"],
            tuple(item.get("methods", [])),
        ),
    ):
        rule = select_rule(rules, raw)
        module = owning_module(raw["source_path"], modules)
        fact = semantic_fact(raw, rule, module)
        if fact["fact_id"] in seen_ids:
            continue
        seen_ids.add(fact["fact_id"])
        facts.append(fact)

    enforce_consumer_projection_boundary(facts)

    counts = {kind: sum(1 for fact in facts if fact["fact_type"] == kind) for kind in ("route", "config", "data")}
    policy_bytes = snapshot.read(POLICY_PATH)
    module_bytes = snapshot.read(MODULE_CATALOG_PATH)
    snapshot.verify()
    if git_inventory != tracked_paths(ROOT) or code_paths != tuple(source_files()) or data_paths != tuple(sql_files()):
        raise AssertionError("semantic source inventory changed during generation")
    source_tree_hash = sha256_bytes(b"".join(source_hash_inputs))
    document = {
        "schema": "cex.repository-contract-semantics.v1",
        "status": "active_generated_contract",
        "production_authorization": "not_granted",
        "generator": "scripts/generate-repository-semantics.py",
        "policy": {
            "path": normalized_path(POLICY_PATH),
            "sha256": sha256_bytes(policy_bytes),
            "schema": policy.get("schema"),
        },
        "module_catalog": {
            "path": normalized_path(MODULE_CATALOG_PATH),
            "sha256": sha256_bytes(module_bytes),
        },
        "source_tree_sha256": source_tree_hash,
        "counts": {**counts, "total": len(facts), "unclassified": 0,
                   "unresolved_route_methods": sum(f.get("method_resolution") == "unresolved" for f in facts),
                   "dynamic_route_paths": sum(f.get("path_resolution") == "dynamic" for f in facts)},
        "route_extractor": {"path": "scripts/rust_route_contract.py", "sha256": sha256_bytes(parser_bytes),
                            "scope": "explicit_rust_source_declarations", "implicit_head_inferred": False},
        "input_coverage": {"workspace_members": len(modules), "source_files": len(code_paths),
                           "sql_files": len(data_paths), "git_inventory_checked": True,
                           "non_rust_route_sources_not_analyzed": [normalized_path(p) for p in code_paths if p.suffix != ".rs"]},
        "limitations": [
            "Route extraction proves explicit Rust outer-router declarations, not type resolution, cfg/macro expansion, nesting composition or middleware execution.",
            "JavaScript/TypeScript route registration is not analyzed; see input_coverage. Dynamic paths and unknown constructors remain unresolved.",
            "GET does not imply an inferred HEAD entry here; constructor methods are not runtime method coverage.",
            "Configuration extraction proves referenced keys, not secret custody or deployed values.",
            "SQL extraction proves referenced object names, not runtime database ownership or migration execution.",
            "Exact-SHA hosted tests and independent production approval remain separate authorities."
        ],
        "facts": facts,
    }
    return document


def rendered_document() -> str:
    return json.dumps(build_document(), indent=2, sort_keys=False, ensure_ascii=False) + "\n"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true", help="write the generated contract")
    mode.add_argument("--check", action="store_true", help="fail when the generated contract is stale")
    mode.add_argument("--stdout", action="store_true", help="print the generated contract")
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    return parser.parse_args()


def checked_output_path(path: Path) -> Path:
    path = path if path.is_absolute() else ROOT / path
    try:
        relative = path.relative_to(ROOT)
    except ValueError:
        raise AssertionError("semantic output must stay inside the repository") from None
    if ".." in relative.parts or path.suffix != ".json":
        raise AssertionError("invalid semantic output path")
    for parent in [path, *path.parents]:
        if parent == ROOT:
            break
        if parent.is_symlink():
            raise AssertionError("semantic output must not traverse symbolic links")
    return path


def write_document(output: Path, rendered: str) -> None:
    output = checked_output_path(output)
    output.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=".semantic-contract-", suffix=".tmp", dir=output.parent)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as stream:
            stream.write(rendered)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, output)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def main() -> int:
    args = parse_args()
    output = checked_output_path(args.output)
    rendered = rendered_document()
    if args.stdout:
        sys.stdout.write(rendered)
        return 0
    if args.write:
        write_document(output, rendered)
        document = json.loads(rendered)
        print(f"wrote {output.relative_to(ROOT)} with {document['counts']}")
        return 0

    try:
        existing = output.read_text(encoding="utf-8")
    except FileNotFoundError:
        print(f"semantic contract is absent: {output.relative_to(ROOT)}", file=sys.stderr)
        return 1
    if existing != rendered:
        print(
            "semantic contract is stale; run "
            "python3 scripts/generate-repository-semantics.py --write",
            file=sys.stderr,
        )
        return 1
    document = json.loads(rendered)
    print(f"repository semantic contract: OK {document['counts']}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except AssertionError as error:
        print(f"repository semantic contract: FAILED: {error}", file=sys.stderr)
        raise SystemExit(1)
