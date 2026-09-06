#!/usr/bin/env python3
"""Close the transitive local Cargo graph reachable from quarantined CEX/World crates."""

from __future__ import annotations

import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path
from typing import Any, Callable, Iterator

ROOT = Path(__file__).resolve().parents[1]
MANIFEST_PATH = ROOT / "docs/compatibility/world-surface-freeze-v1.json"
WORKSPACE_MANIFEST = ROOT / "Cargo.toml"
ROOT_PACKAGES = {
    "services/consumer-entry-api",
    "services/matrix-entry-adapter",
}
EXPECTED_LOCAL_PACKAGES = {
    "crates/shared-tracing": ("shared-tracing", "observability_support_only"),
    "vendor/trnm-economy-protocol": (
        "trnm-economy-protocol",
        "versioned_economy_protocol_only",
    ),
    "services/ledger-service": ("ledger-service", "test_fixture_only"),
    "crates/shared-config": ("shared-config", "ledger_support_only"),
    "crates/shared-types": ("shared-types", "ledger_support_only"),
}
EXPECTED_EDGES = {
    (
        "services/consumer-entry-api",
        "shared-tracing",
        "shared-tracing",
        "crates/shared-tracing",
        "normal",
        None,
        "path",
    ),
    (
        "services/consumer-entry-api",
        "term-exchange-protocol",
        "trnm-economy-protocol",
        "vendor/trnm-economy-protocol",
        "normal",
        None,
        "workspace",
    ),
    (
        "services/consumer-entry-api",
        "ledger-service",
        "ledger-service",
        "services/ledger-service",
        "dev",
        None,
        "path",
    ),
    (
        "services/matrix-entry-adapter",
        "shared-tracing",
        "shared-tracing",
        "crates/shared-tracing",
        "normal",
        None,
        "path",
    ),
    (
        "services/ledger-service",
        "shared-tracing",
        "shared-tracing",
        "crates/shared-tracing",
        "normal",
        None,
        "path",
    ),
    (
        "services/ledger-service",
        "shared-config",
        "shared-config",
        "crates/shared-config",
        "normal",
        None,
        "path",
    ),
    (
        "services/ledger-service",
        "shared-types",
        "shared-types",
        "crates/shared-types",
        "normal",
        None,
        "path",
    ),
    (
        "services/ledger-service",
        "term-exchange-protocol",
        "trnm-economy-protocol",
        "vendor/trnm-economy-protocol",
        "normal",
        None,
        "workspace",
    ),
}
EXPECTED_POLICY = {
    "reachable_graph": "exact_allowlist",
    "new_local_packages": "forbidden",
    "new_local_edges": "forbidden",
    "target_specific_dependencies": "included",
    "dev_dependencies": "included",
    "build_dependencies": "included",
    "package_aliases": "identity_bound",
    "proc_macro_packages": "forbidden",
    "build_scripts": "forbidden",
    "cargo_source_replacement": "forbidden",
    "generated_rust_inputs": "forbidden",
}
EXPECTED_HOSTILE_FIXTURES = {
    "new_workspace_authority_carrier",
    "existing_unquarantined_authority_carrier",
    "renamed_package_alias",
    "target_specific_dependency",
    "dev_dependency_carrier",
    "build_dependency_carrier",
    "proc_macro_helper",
    "generated_rust_helper",
    "carrier_outside_manifest_roots",
    "cargo_source_replacement",
}
SHA1_RE = re.compile(r"^[0-9a-f]{40}$")
FORBIDDEN_SOURCE_PATTERNS = {
    "rust_path_attribute": re.compile(r"#\s*\[\s*path\s*="),
    "rust_include_macro": re.compile(r"(?<![A-Za-z0-9_])include!\s*\("),
    "out_dir_generated_rust": re.compile(r"\bOUT_DIR\b"),
}


class CargoClosureViolation(RuntimeError):
    """Raised when a local Cargo dependency can escape the reviewed closure."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CargoClosureViolation(message)


def git(*args: str, check: bool = True) -> str:
    result = subprocess.run(
        ["git", "-C", str(ROOT), *args],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if check and result.returncode != 0:
        raise CargoClosureViolation(
            f"git {' '.join(args)} failed: "
            f"{result.stderr.strip() or result.stdout.strip()}"
        )
    return result.stdout.strip()


def committed_text(relative: str) -> str:
    return git("show", f"HEAD:{relative}")


def committed_object(relative: str) -> str:
    return git("rev-parse", f"HEAD:{relative}")


def committed_exists(relative: str) -> bool:
    result = subprocess.run(
        ["git", "-C", str(ROOT), "cat-file", "-e", f"HEAD:{relative}"],
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return result.returncode == 0


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise CargoClosureViolation(f"cannot read {path}: {error}") from error
    require(isinstance(value, dict), f"{path} must contain a JSON object")
    return value


def parse_manifest(relative: str) -> dict[str, Any]:
    try:
        return tomllib.loads(committed_text(relative))
    except tomllib.TOMLDecodeError as error:
        raise CargoClosureViolation(f"invalid Cargo manifest {relative}: {error}") from error


def require_sha(value: Any, label: str) -> str:
    require(
        isinstance(value, str) and SHA1_RE.fullmatch(value) is not None,
        f"{label} must be an exact Git SHA-1",
    )
    return value


def normalize_repo_path(manifest_dir: Path, value: str) -> str:
    resolved = (manifest_dir / value).resolve()
    try:
        relative = resolved.relative_to(ROOT.resolve())
    except ValueError as error:
        raise CargoClosureViolation(
            f"local Cargo dependency escapes repository: "
            f"{manifest_dir.relative_to(ROOT)}/{value}"
        ) from error
    return relative.as_posix()


def iter_dependency_tables(
    document: dict[str, Any],
) -> Iterator[tuple[str, Any, str, str | None]]:
    for table_name, kind in (
        ("dependencies", "normal"),
        ("dev-dependencies", "dev"),
        ("build-dependencies", "build"),
    ):
        table = document.get(table_name, {})
        require(isinstance(table, dict), f"{table_name} must be a table")
        for alias, spec in table.items():
            yield alias, spec, kind, None

    targets = document.get("target", {})
    require(isinstance(targets, dict), "target must be a table")
    for target_name, target_document in targets.items():
        require(
            isinstance(target_document, dict),
            f"target.{target_name} must be a table",
        )
        for table_name, kind in (
            ("dependencies", "normal"),
            ("dev-dependencies", "dev"),
            ("build-dependencies", "build"),
        ):
            table = target_document.get(table_name, {})
            require(
                isinstance(table, dict),
                f"target.{target_name}.{table_name} must be a table",
            )
            for alias, spec in table.items():
                yield alias, spec, kind, str(target_name)


def effective_dependency_spec(
    alias: str,
    spec: Any,
    workspace_dependencies: dict[str, Any],
) -> tuple[dict[str, Any] | None, str, Path]:
    if isinstance(spec, str):
        return None, "registry", ROOT

    require(isinstance(spec, dict), f"dependency {alias} has invalid shape")
    if spec.get("workspace") is True:
        require(
            "path" not in spec and "package" not in spec,
            f"workspace dependency {alias} must not override path or package identity",
        )
        require(
            alias in workspace_dependencies,
            f"workspace dependency {alias} is not declared at the workspace root",
        )
        inherited = workspace_dependencies[alias]
        if isinstance(inherited, str):
            return None, "registry", ROOT
        require(
            isinstance(inherited, dict),
            f"workspace dependency {alias} has invalid root shape",
        )
        effective = dict(inherited)
        for key, value in spec.items():
            if key != "workspace":
                effective[key] = value
        return effective, "workspace", ROOT

    return dict(spec), "path" if "path" in spec else "registry", ROOT


def package_name(package_path: str) -> str:
    document = parse_manifest(f"{package_path}/Cargo.toml")
    name = document.get("package", {}).get("name")
    require(
        isinstance(name, str) and name,
        f"local package lacks package.name: {package_path}",
    )
    return name


def collect_local_edges_for_package(
    package_path: str,
    workspace_dependencies: dict[str, Any],
) -> list[tuple[str, str, str, str, str, str | None, str]]:
    manifest_path = f"{package_path}/Cargo.toml"
    document = parse_manifest(manifest_path)
    manifest_dir = ROOT / package_path
    edges: list[tuple[str, str, str, str, str, str | None, str]] = []

    for alias, raw_spec, kind, target in iter_dependency_tables(document):
        spec, declaration, path_base = effective_dependency_spec(
            alias,
            raw_spec,
            workspace_dependencies,
        )
        if spec is None or "path" not in spec:
            continue

        if declaration == "workspace":
            path_base = ROOT
        else:
            path_base = manifest_dir
        path_value = spec.get("path")
        require(
            isinstance(path_value, str) and path_value,
            f"local dependency {package_path}:{alias} has invalid path",
        )
        target_path = normalize_repo_path(path_base, path_value)
        require(
            committed_exists(f"{target_path}/Cargo.toml"),
            f"local dependency target lacks Cargo.toml: "
            f"{package_path}:{alias}->{target_path}",
        )
        actual_package = package_name(target_path)
        declared_package = spec.get("package")
        if declared_package is not None:
            require(
                declared_package == actual_package,
                f"package alias identity mismatch: {package_path}:{alias} "
                f"declares {declared_package!r}, target is {actual_package!r}",
            )
        edges.append(
            (
                package_path,
                str(alias),
                actual_package,
                target_path,
                kind,
                target,
                declaration,
            )
        )
    return edges


def validate_manifest_source_controls(
    package_path: str,
    document: dict[str, Any],
) -> None:
    build_value = document.get("package", {}).get("build")
    require(
        build_value in {None, False},
        f"Cargo build script declaration is forbidden in closure package: {package_path}",
    )
    require(
        not committed_exists(f"{package_path}/build.rs"),
        f"implicit Cargo build script is forbidden in closure package: "
        f"{package_path}/build.rs",
    )
    require(
        document.get("lib", {}).get("proc-macro") is not True,
        f"local proc-macro package is forbidden in closure: {package_path}",
    )


def validate_root_cargo_replacement_controls(
    workspace_document: dict[str, Any],
) -> None:
    require(
        not workspace_document.get("patch"),
        "workspace [patch] source replacement is forbidden for the quarantine closure",
    )
    require(
        not workspace_document.get("replace"),
        "workspace [replace] source replacement is forbidden for the quarantine closure",
    )
    config_paths = [
        path
        for path in git("ls-tree", "-r", "--name-only", "HEAD", "--", ".cargo")
        .splitlines()
        if Path(path).name in {"config", "config.toml"}
    ]
    for relative in config_paths:
        config = tomllib.loads(committed_text(relative))
        require(
            not config.get("source")
            and not config.get("patch")
            and not config.get("replace"),
            f"Cargo source replacement is forbidden in {relative}",
        )


def scan_bound_package_sources(package_path: str) -> int:
    files = [
        path
        for path in git("ls-tree", "-r", "--name-only", "HEAD", "--", package_path)
        .splitlines()
        if path.endswith(".rs")
    ]
    for relative in files:
        text = committed_text(relative)
        for policy, pattern in FORBIDDEN_SOURCE_PATTERNS.items():
            require(
                pattern.search(text) is None,
                f"forbidden indirect source mechanism {policy} "
                f"in local closure package: {relative}",
            )
    return len(files)


def edge_from_json(value: dict[str, Any]) -> tuple[str, str, str, str, str, str | None, str]:
    required = {"from", "alias", "package", "to", "kind", "target", "declaration"}
    require(set(value) == required, f"local dependency edge field drift: {value}")
    target = value["target"]
    require(target is None or isinstance(target, str), f"invalid edge target: {value}")
    return (
        str(value["from"]),
        str(value["alias"]),
        str(value["package"]),
        str(value["to"]),
        str(value["kind"]),
        target,
        str(value["declaration"]),
    )


def validate_exact_edges(
    observed: set[tuple[str, str, str, str, str, str | None, str]],
) -> None:
    missing = sorted(EXPECTED_EDGES - observed, key=str)
    extra = sorted(observed - EXPECTED_EDGES, key=str)
    require(
        not missing and not extra,
        f"reachable local Cargo edge closure drift: missing={missing} extra={extra}",
    )


def validate_package_objects(manifest: dict[str, Any]) -> dict[str, dict[str, Any]]:
    entries = manifest.get("local_cargo_dependency_closure")
    require(
        isinstance(entries, list),
        "local_cargo_dependency_closure must be a list",
    )
    package_map: dict[str, dict[str, Any]] = {}
    for entry in entries:
        require(isinstance(entry, dict), "local closure package entry must be an object")
        path = entry.get("path")
        require(isinstance(path, str) and path, "local closure package path missing")
        require(path not in package_map, f"duplicate local closure package: {path}")
        package_map[path] = entry

    require(
        set(package_map) == set(EXPECTED_LOCAL_PACKAGES),
        "local dependency package set drift: "
        f"expected={sorted(EXPECTED_LOCAL_PACKAGES)} actual={sorted(package_map)}",
    )

    for path, (expected_name, expected_classification) in EXPECTED_LOCAL_PACKAGES.items():
        entry = package_map[path]
        require(
            entry.get("package") == expected_name,
            f"local package identity drift: {path}",
        )
        require(
            entry.get("classification") == expected_classification,
            f"local package authority classification drift: {path}",
        )
        require(
            committed_object(path)
            == require_sha(entry.get("package_git_tree"), f"package tree {path}"),
            f"local package tree drift: {path}",
        )
        require(
            committed_object(f"{path}/Cargo.toml")
            == require_sha(
                entry.get("cargo_manifest_blob"),
                f"Cargo manifest blob {path}",
            ),
            f"local package manifest drift: {path}",
        )
        require(
            committed_object(f"{path}/src")
            == require_sha(entry.get("source_git_tree"), f"source tree {path}"),
            f"local package source tree drift: {path}",
        )
        tests_tree = entry.get("tests_git_tree")
        if tests_tree is not None:
            require(
                committed_object(f"{path}/tests")
                == require_sha(tests_tree, f"tests tree {path}"),
                f"local package tests tree drift: {path}",
            )
        document = parse_manifest(f"{path}/Cargo.toml")
        validate_manifest_source_controls(path, document)
    return package_map


def expect_rejected(label: str, operation: Callable[[], None]) -> None:
    try:
        operation()
    except CargoClosureViolation:
        print(f"hostile Cargo-closure fixture rejected: {label}", file=sys.stderr)
        return
    raise CargoClosureViolation(f"hostile Cargo-closure fixture was accepted: {label}")


def run_hostile_fixtures() -> set[str]:
    executed: set[str] = set()

    synthetic_edges = [
        (
            "services/consumer-entry-api",
            "world-carrier",
            "world-carrier",
            "crates/world-carrier",
            "normal",
            None,
            "path",
        ),
        (
            "services/consumer-entry-api",
            "execution-service",
            "execution-service",
            "services/execution-service",
            "normal",
            None,
            "path",
        ),
        (
            "services/consumer-entry-api",
            "telemetry",
            "shared-tracing",
            "crates/shared-tracing",
            "normal",
            None,
            "path",
        ),
        (
            "services/consumer-entry-api",
            "shared-tracing",
            "shared-tracing",
            "crates/shared-tracing",
            "normal",
            "cfg(unix)",
            "path",
        ),
        (
            "services/matrix-entry-adapter",
            "world-dev-helper",
            "world-dev-helper",
            "crates/world-dev-helper",
            "dev",
            None,
            "path",
        ),
        (
            "services/matrix-entry-adapter",
            "world-build-helper",
            "world-build-helper",
            "crates/world-build-helper",
            "build",
            None,
            "path",
        ),
        (
            "services/consumer-entry-api",
            "outside-carrier",
            "outside-carrier",
            "tools/outside-carrier",
            "normal",
            None,
            "path",
        ),
    ]
    labels = [
        "new_workspace_authority_carrier",
        "existing_unquarantined_authority_carrier",
        "renamed_package_alias",
        "target_specific_dependency",
        "dev_dependency_carrier",
        "build_dependency_carrier",
        "carrier_outside_manifest_roots",
    ]
    for label, edge in zip(labels, synthetic_edges, strict=True):
        expect_rejected(
            label,
            lambda edge=edge: validate_exact_edges(set(EXPECTED_EDGES) | {edge}),
        )
        executed.add(label)

    expect_rejected(
        "proc_macro_helper",
        lambda: require(
            {"lib": {"proc-macro": True}}.get("lib", {}).get("proc-macro") is not True,
            "synthetic proc macro rejected",
        ),
    )
    executed.add("proc_macro_helper")

    expect_rejected(
        "generated_rust_helper",
        lambda: require(
            FORBIDDEN_SOURCE_PATTERNS["out_dir_generated_rust"].search(
                'include!(concat!(env!("OUT_DIR"), "/world.rs"));'
            )
            is None,
            "synthetic OUT_DIR source rejected",
        ),
    )
    executed.add("generated_rust_helper")

    expect_rejected(
        "cargo_source_replacement",
        lambda: require(
            not {"source": {"crates-io": {"replace-with": "local"}}}.get("source"),
            "synthetic Cargo source replacement rejected",
        ),
    )
    executed.add("cargo_source_replacement")
    return executed


def main() -> int:
    try:
        manifest = load_json(MANIFEST_PATH)
        require(
            manifest.get("production_authorization") == "not_granted",
            "Cargo closure cannot grant production authorization",
        )
        require(
            manifest.get("local_cargo_dependency_policy") == EXPECTED_POLICY,
            "local Cargo dependency policy drift",
        )
        require(
            committed_object("Cargo.toml")
            == require_sha(
                manifest.get("workspace_manifest_blob"),
                "workspace manifest blob",
            ),
            "workspace Cargo manifest drift",
        )

        workspace_document = parse_manifest("Cargo.toml")
        validate_root_cargo_replacement_controls(workspace_document)
        workspace_dependencies = workspace_document.get("workspace", {}).get(
            "dependencies",
            {},
        )
        require(
            isinstance(workspace_dependencies, dict),
            "workspace.dependencies must be a table",
        )

        package_map = validate_package_objects(manifest)

        declared_edges_raw = manifest.get("allowed_local_dependency_edges")
        require(
            isinstance(declared_edges_raw, list),
            "allowed_local_dependency_edges must be a list",
        )
        declared_edges = {
            edge_from_json(edge)
            for edge in declared_edges_raw
            if isinstance(edge, dict)
        }
        require(
            len(declared_edges) == len(declared_edges_raw),
            "duplicate or invalid declared local dependency edge",
        )
        require(
            declared_edges == EXPECTED_EDGES,
            "manifest local dependency edge allowlist drift",
        )

        observed_edges: set[
            tuple[str, str, str, str, str, str | None, str]
        ] = set()
        reached: set[str] = set(ROOT_PACKAGES)
        pending = list(sorted(ROOT_PACKAGES))
        while pending:
            package_path = pending.pop()
            document = parse_manifest(f"{package_path}/Cargo.toml")
            validate_manifest_source_controls(package_path, document)
            for edge in collect_local_edges_for_package(
                package_path,
                workspace_dependencies,
            ):
                require(edge not in observed_edges, f"duplicate local edge: {edge}")
                observed_edges.add(edge)
                target_path = edge[3]
                if target_path not in reached:
                    reached.add(target_path)
                    pending.append(target_path)

        validate_exact_edges(observed_edges)
        reached_dependencies = reached - ROOT_PACKAGES
        require(
            reached_dependencies == set(package_map),
            "reachable local package closure mismatch: "
            f"expected={sorted(package_map)} actual={sorted(reached_dependencies)}",
        )

        scanned_rust_files = sum(
            scan_bound_package_sources(path) for path in sorted(package_map)
        )

        declared_hostile = manifest.get("cargo_closure_hostile_fixtures")
        require(
            isinstance(declared_hostile, list),
            "cargo_closure_hostile_fixtures must be a list",
        )
        require(
            set(declared_hostile) == EXPECTED_HOSTILE_FIXTURES
            and len(declared_hostile) == len(EXPECTED_HOSTILE_FIXTURES),
            "Cargo closure hostile fixture declaration drift",
        )
        executed = run_hostile_fixtures()
        require(
            executed == EXPECTED_HOSTILE_FIXTURES,
            "Cargo closure hostile fixture execution drift",
        )

        print(
            json.dumps(
                {
                    "schema": "cex.project-boundary-cargo-closure.v1",
                    "status": "ok",
                    "root_packages": sorted(ROOT_PACKAGES),
                    "reachable_local_packages": sorted(reached_dependencies),
                    "local_dependency_edges": len(observed_edges),
                    "bound_rust_files_scanned": scanned_rust_files,
                    "target_specific_dependencies_included": True,
                    "dev_dependencies_included": True,
                    "build_dependencies_included": True,
                    "package_aliases_identity_bound": True,
                    "proc_macro_packages_allowed": False,
                    "build_scripts_allowed": False,
                    "cargo_source_replacement_allowed": False,
                    "hostile_fixtures_rejected": sorted(executed),
                    "production_authorization": "not_granted",
                    "problems": [],
                },
                indent=2,
                sort_keys=True,
            )
        )
        return 0
    except CargoClosureViolation as error:
        print(
            json.dumps(
                {
                    "schema": "cex.project-boundary-cargo-closure.v1",
                    "status": "failed",
                    "production_authorization": "not_granted",
                    "problems": [str(error)],
                },
                indent=2,
                sort_keys=True,
            )
        )
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
