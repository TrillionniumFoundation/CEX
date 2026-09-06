#!/usr/bin/env python3
"""Validate the CEX/World ownership boundary as an exact recursive source closure."""

from __future__ import annotations

import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path, PurePosixPath
from typing import Any, Callable

ROOT = Path(__file__).resolve().parents[1]
MANIFEST_PATH = ROOT / "docs/compatibility/world-surface-freeze-v1.json"
BOUNDARY_PATH = ROOT / "PROJECT_BOUNDARY.json"
EXPECTED_QUARANTINED_CRATES = {
    "services/consumer-entry-api",
    "services/matrix-entry-adapter",
}
EXPECTED_HOSTILE_FIXTURES = {
    "nested_world_module",
    "neutral_stem_module",
    "rust_path_attribute",
    "rust_include_macro",
    "out_dir_generated_rust",
    "cargo_build_script",
    "additional_matrix_carrier",
    "sibling_repository_path_dependency",
}
SHA1_RE = re.compile(r"^[0-9a-f]{40}$")
MOD_DECL_RE = re.compile(
    r"(?m)^\s*(?:(?:#\[[^\n]*\])\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;"
)
FORBIDDEN_SOURCE_PATTERNS = {
    "rust_path_attribute": re.compile(r"#\s*\[\s*path\s*="),
    "rust_include_macro": re.compile(r"(?<![A-Za-z0-9_])include!\s*\("),
    "out_dir_generated_rust": re.compile(r"\bOUT_DIR\b"),
}
PROBLEMS: list[str] = []


class BoundaryViolation(RuntimeError):
    """Raised when a hostile fixture or committed source violates the boundary."""


def run_git(*args: str, check: bool = True) -> str:
    result = subprocess.run(
        ["git", "-C", str(ROOT), *args],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if check and result.returncode != 0:
        raise BoundaryViolation(
            f"git {' '.join(args)} failed: {result.stderr.strip() or result.stdout.strip()}"
        )
    return result.stdout.strip()


def committed_text(relative: str) -> str:
    return run_git("show", f"HEAD:{relative}")


def committed_object(relative: str) -> str:
    return run_git("rev-parse", f"HEAD:{relative}")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise BoundaryViolation(message)


def require_sha(value: Any, label: str) -> str:
    require(
        isinstance(value, str) and SHA1_RE.fullmatch(value) is not None,
        f"{label} must be an exact Git SHA-1",
    )
    return value


def load_json(path: Path, label: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise BoundaryViolation(f"invalid {label}: {error}") from error
    require(isinstance(value, dict), f"{label} must be a JSON object")
    return value


def list_tree_names(relative: str) -> set[str]:
    value = run_git("ls-tree", "--name-only", f"HEAD:{relative}")
    return {line for line in value.splitlines() if line}


def list_recursive_files(relative: str) -> dict[str, tuple[str, str]]:
    output = run_git("ls-tree", "-r", "HEAD", "--", relative)
    entries: dict[str, tuple[str, str]] = {}
    for line in output.splitlines():
        metadata, path = line.split("\t", 1)
        mode, kind, object_id = metadata.split()
        require(kind == "blob", f"non-blob tracked below quarantined source root: {path}")
        require(
            mode in {"100644", "100755"},
            f"non-regular source entry below quarantine: {path} mode={mode}",
        )
        entries[path] = (mode, object_id)
    return entries


def require_inventory_equal(actual: set[str], expected: set[str], label: str) -> None:
    missing = sorted(expected - actual)
    extra = sorted(actual - expected)
    require(
        not missing and not extra,
        f"{label} inventory mismatch: missing={missing} extra={extra}",
    )


def scan_source_text(relative: str, text: str) -> None:
    for policy, pattern in FORBIDDEN_SOURCE_PATTERNS.items():
        if pattern.search(text):
            raise BoundaryViolation(
                f"forbidden indirect source mechanism {policy}: {relative}"
            )


def module_child_candidates(
    source_root: PurePosixPath,
    current: PurePosixPath,
    name: str,
) -> tuple[PurePosixPath, PurePosixPath]:
    relative = current.relative_to(source_root)
    if relative in {PurePosixPath("lib.rs"), PurePosixPath("main.rs")}:
        base = current.parent
    elif (
        len(relative.parts) == 2
        and relative.parts[0] == "bin"
        and relative.suffix == ".rs"
    ):
        base = current.with_suffix("")
    elif current.name == "mod.rs":
        base = current.parent
    else:
        base = current.with_suffix("")
    return base / f"{name}.rs", base / name / "mod.rs"


def source_entrypoints(source_root: str, files: set[str]) -> set[str]:
    root = PurePosixPath(source_root)
    entries: set[str] = set()
    for relative in files:
        path = PurePosixPath(relative)
        local = path.relative_to(root)
        if local in {PurePosixPath("lib.rs"), PurePosixPath("main.rs")}:
            entries.add(relative)
        elif (
            len(local.parts) == 2
            and local.parts[0] == "bin"
            and local.suffix == ".rs"
        ):
            entries.add(relative)
        elif (
            len(local.parts) == 3
            and local.parts[0] == "bin"
            and local.name == "main.rs"
        ):
            entries.add(relative)
    return entries


def validate_module_graph(source_root: str, source_files: set[str]) -> None:
    rust_files = {path for path in source_files if path.endswith(".rs")}
    entries = source_entrypoints(source_root, rust_files)
    require(entries, f"quarantined source root has no Rust crate entrypoint: {source_root}")
    root = PurePosixPath(source_root)
    reachable: set[str] = set()
    pending = list(sorted(entries))
    while pending:
        relative = pending.pop()
        if relative in reachable:
            continue
        reachable.add(relative)
        text = committed_text(relative)
        scan_source_text(relative, text)
        current = PurePosixPath(relative)
        for module_name in MOD_DECL_RE.findall(text):
            candidates = module_child_candidates(root, current, module_name)
            matches = [
                candidate.as_posix()
                for candidate in candidates
                if candidate.as_posix() in rust_files
            ]
            require(
                len(matches) == 1,
                "Rust module declaration must resolve to exactly one "
                f"manifest-bound file: source={relative} module={module_name} "
                f"candidates={[str(item) for item in candidates]} matches={matches}",
            )
            if matches[0] not in reachable:
                pending.append(matches[0])
    require_inventory_equal(
        reachable,
        rust_files,
        f"Rust module graph for {source_root}",
    )


def validate_no_build_script(
    crate_path: str,
    root_entries: set[str],
    cargo_document: dict[str, Any],
) -> None:
    require(
        "build.rs" not in root_entries,
        f"Cargo build script is forbidden in quarantined crate: {crate_path}/build.rs",
    )
    build_value = cargo_document.get("package", {}).get("build")
    require(
        build_value in {None, False},
        f"Cargo package build entry is forbidden in quarantined crate: {crate_path}",
    )


def resolve_path_dependency(manifest_dir: Path, path_value: str) -> Path:
    resolved = (manifest_dir / path_value).resolve()
    try:
        resolved.relative_to(ROOT.resolve())
    except ValueError as error:
        raise BoundaryViolation(
            "Cargo path dependency escapes CEX repository: "
            f"{manifest_dir.relative_to(ROOT)}/{path_value}"
        ) from error
    return resolved


def walk_paths(value: Any, manifest_dir: Path) -> None:
    if isinstance(value, dict):
        path_value = value.get("path")
        if isinstance(path_value, str):
            resolve_path_dependency(manifest_dir, path_value)
        for nested in value.values():
            walk_paths(nested, manifest_dir)
    elif isinstance(value, list):
        for nested in value:
            walk_paths(nested, manifest_dir)


def validate_quarantined_crate(
    entry: dict[str, Any],
    deny_pattern: re.Pattern[str],
) -> tuple[int, set[str]]:
    crate_path = entry.get("path")
    source_root = entry.get("source_root")
    require(isinstance(crate_path, str), "quarantined crate path missing")
    require(isinstance(source_root, str), f"source_root missing for {crate_path}")
    require(
        source_root == f"{crate_path}/src",
        f"source_root must be the crate src tree: {crate_path}",
    )

    expected_crate_tree = require_sha(entry.get("git_tree"), f"crate tree {crate_path}")
    expected_source_tree = require_sha(
        entry.get("source_git_tree"),
        f"source tree {source_root}",
    )
    require(
        committed_object(crate_path) == expected_crate_tree,
        f"quarantined crate tree drift: {crate_path}",
    )
    require(
        committed_object(source_root) == expected_source_tree,
        f"quarantined source tree drift: {source_root}",
    )

    expected_root_entries_raw = entry.get("root_entries")
    require(
        isinstance(expected_root_entries_raw, list),
        f"root_entries must be a list: {crate_path}",
    )
    expected_root_entries = set(expected_root_entries_raw)
    require(
        len(expected_root_entries) == len(expected_root_entries_raw),
        f"duplicate root entry: {crate_path}",
    )
    actual_root_entries = list_tree_names(crate_path)
    require_inventory_equal(
        actual_root_entries,
        expected_root_entries,
        f"crate root {crate_path}",
    )

    cargo_path = f"{crate_path}/Cargo.toml"
    module_doc_path = f"{crate_path}/MODULE.md"
    expected_cargo_blob = require_sha(
        entry.get("cargo_manifest_blob"),
        f"Cargo manifest blob {crate_path}",
    )
    expected_module_blob = require_sha(
        entry.get("module_document_blob"),
        f"module document blob {crate_path}",
    )
    require(
        committed_object(cargo_path) == expected_cargo_blob,
        f"Cargo manifest blob drift: {cargo_path}",
    )
    require(
        committed_object(module_doc_path) == expected_module_blob,
        f"module document blob drift: {module_doc_path}",
    )

    source_entries = entry.get("source_files")
    require(
        isinstance(source_entries, list) and source_entries,
        f"source_files must be nonempty: {crate_path}",
    )
    require(
        entry.get("source_file_count") == len(source_entries),
        f"source_file_count drift: {crate_path}",
    )
    expected_files: dict[str, str] = {}
    for index, source in enumerate(source_entries):
        require(
            isinstance(source, dict),
            f"source_files[{index}] must be an object: {crate_path}",
        )
        path = source.get("path")
        require(
            isinstance(path, str) and path.startswith(f"{source_root}/"),
            f"source file escapes root: {path}",
        )
        require(
            path.endswith(".rs"),
            f"only Rust source is permitted in quarantined src roots: {path}",
        )
        require(path not in expected_files, f"duplicate manifest-bound source file: {path}")
        expected_files[path] = require_sha(
            source.get("git_blob"),
            f"source blob {path}",
        )

    actual_files = list_recursive_files(source_root)
    require_inventory_equal(
        set(actual_files),
        set(expected_files),
        f"recursive tracked source root {source_root}",
    )
    for path, expected_blob in expected_files.items():
        actual_blob = actual_files[path][1]
        require(
            actual_blob == expected_blob,
            f"manifest-bound source blob drift: {path} "
            f"expected={expected_blob} actual={actual_blob}",
        )
        require(
            deny_pattern.match(path) is not None,
            f"deny_changed_paths_regex does not cover quarantined source: {path}",
        )

    require(
        deny_pattern.match(cargo_path) is not None,
        f"deny_changed_paths_regex does not cover quarantined Cargo manifest: {cargo_path}",
    )
    require(
        deny_pattern.match(f"{crate_path}/build.rs") is not None,
        f"deny_changed_paths_regex does not cover build script path: {crate_path}/build.rs",
    )

    cargo_document = tomllib.loads(committed_text(cargo_path))
    validate_no_build_script(crate_path, actual_root_entries, cargo_document)
    validate_module_graph(source_root, set(expected_files))
    return len(expected_files), set(expected_files)


def expect_rejected(label: str, operation: Callable[[], None]) -> None:
    try:
        operation()
    except BoundaryViolation:
        print(f"hostile boundary fixture rejected: {label}", file=sys.stderr)
        return
    raise BoundaryViolation(f"hostile boundary fixture was accepted: {label}")


def run_hostile_fixtures(deny_pattern: re.Pattern[str]) -> set[str]:
    executed: set[str] = set()

    base_consumer = {"services/consumer-entry-api/src/lib.rs"}
    expect_rejected(
        "nested_world_module",
        lambda: require_inventory_equal(
            base_consumer | {"services/consumer-entry-api/src/world/economy.rs"},
            base_consumer,
            "nested fixture",
        ),
    )
    executed.add("nested_world_module")

    expect_rejected(
        "neutral_stem_module",
        lambda: require_inventory_equal(
            base_consumer | {"services/consumer-entry-api/src/cache.rs"},
            base_consumer,
            "neutral fixture",
        ),
    )
    executed.add("neutral_stem_module")

    expect_rejected(
        "rust_path_attribute",
        lambda: scan_source_text(
            "fixture.rs",
            '#[path = "world/economy.rs"]\nmod economy;\n',
        ),
    )
    executed.add("rust_path_attribute")

    expect_rejected(
        "rust_include_macro",
        lambda: scan_source_text(
            "fixture.rs",
            'include!("world/economy.rs");\n',
        ),
    )
    executed.add("rust_include_macro")

    expect_rejected(
        "out_dir_generated_rust",
        lambda: scan_source_text(
            "fixture.rs",
            'include!(concat!(env!("OUT_DIR"), "/world.rs"));\n',
        ),
    )
    executed.add("out_dir_generated_rust")

    expect_rejected(
        "cargo_build_script",
        lambda: validate_no_build_script(
            "services/consumer-entry-api",
            {"Cargo.toml", "MODULE.md", "src", "build.rs"},
            {"package": {}},
        ),
    )
    executed.add("cargo_build_script")

    base_matrix = {
        "services/matrix-entry-adapter/src/lib.rs",
        "services/matrix-entry-adapter/src/main.rs",
    }
    expect_rejected(
        "additional_matrix_carrier",
        lambda: require_inventory_equal(
            base_matrix | {"services/matrix-entry-adapter/src/relay_extra.rs"},
            base_matrix,
            "Matrix fixture",
        ),
    )
    executed.add("additional_matrix_carrier")

    expect_rejected(
        "sibling_repository_path_dependency",
        lambda: resolve_path_dependency(
            ROOT / "services/consumer-entry-api",
            "../../../Trillionnium-World/trillionnium/crates/world-authority",
        ),
    )
    executed.add("sibling_repository_path_dependency")

    for path in (
        "services/consumer-entry-api/src/world/economy.rs",
        "services/consumer-entry-api/src/cache.rs",
        "services/matrix-entry-adapter/src/relay_extra.rs",
        "services/consumer-entry-api/Cargo.toml",
        "services/consumer-entry-api/build.rs",
        "services/matrix-entry-adapter/Cargo.toml",
        "services/matrix-entry-adapter/build.rs",
    ):
        require(
            deny_pattern.match(path) is not None,
            f"deny regex hostile coverage gap: {path}",
        )
    return executed


def main() -> int:
    try:
        manifest = load_json(MANIFEST_PATH, "World surface manifest")
        boundary = load_json(BOUNDARY_PATH, "PROJECT_BOUNDARY.json")

        require(
            manifest.get("schema") == "cex.world-surface-freeze.v1",
            "World surface manifest schema is invalid",
        )
        require(
            manifest.get("status") == "active",
            "World surface manifest must be active",
        )
        require(
            manifest.get("production_authorization") == "not_granted",
            "World surface manifest must deny production authorization",
        )
        require(
            manifest.get("owner_repository")
            == "TrillionniumFoundation/Trillionnium-World",
            "World authority owner repository is invalid",
        )
        require(
            manifest.get("cex_role") == "compatibility_edge_only",
            "CEX World role must remain compatibility_edge_only",
        )

        policy = manifest.get("indirect_source_policy")
        require(isinstance(policy, dict), "indirect_source_policy must be an object")
        for key in (
            "rust_path_attribute",
            "rust_include_macro",
            "out_dir_generated_rust",
            "cargo_build_script",
            "untracked_or_unmanifested_source",
        ):
            require(
                policy.get(key) == "forbidden",
                f"indirect source policy must forbid {key}",
            )

        deny_regex = boundary.get("deny_changed_paths_regex")
        require(
            isinstance(deny_regex, str),
            "deny_changed_paths_regex must be a string",
        )
        try:
            deny_pattern = re.compile(deny_regex)
        except re.error as error:
            raise BoundaryViolation(
                f"deny_changed_paths_regex is invalid: {error}"
            ) from error

        quarantined = manifest.get("quarantined_crates")
        require(
            isinstance(quarantined, list),
            "quarantined_crates must be a list",
        )
        crate_paths = {
            entry.get("path")
            for entry in quarantined
            if isinstance(entry, dict)
        }
        require(
            crate_paths == EXPECTED_QUARANTINED_CRATES,
            f"quarantined crate set drift: {crate_paths}",
        )
        require(
            len(quarantined) == len(EXPECTED_QUARANTINED_CRATES),
            "duplicate quarantined crate entry",
        )

        complete_source_files: set[str] = set()
        total_source_files = 0
        for entry in quarantined:
            count, paths = validate_quarantined_crate(entry, deny_pattern)
            require(
                not (complete_source_files & paths),
                "source file appears in multiple quarantined roots",
            )
            complete_source_files.update(paths)
            total_source_files += count

        frozen_entries = manifest.get("frozen_files")
        require(
            isinstance(frozen_entries, list) and frozen_entries,
            "World frozen_files must be a nonempty list",
        )
        frozen_paths: set[str] = set()
        world_paths: set[str] = set()
        for index, entry in enumerate(frozen_entries):
            require(
                isinstance(entry, dict),
                f"frozen_files[{index}] must be an object",
            )
            relative = entry.get("path")
            expected_blob = require_sha(
                entry.get("git_blob"),
                f"frozen blob at index {index}",
            )
            kind = entry.get("kind")
            require(
                isinstance(relative, str),
                f"frozen_files[{index}] lacks path",
            )
            require(
                relative not in frozen_paths,
                f"duplicate frozen World path: {relative}",
            )
            frozen_paths.add(relative)
            require(
                relative in complete_source_files,
                f"frozen file escapes recursive quarantine: {relative}",
            )
            require(
                committed_object(relative) == expected_blob,
                f"frozen World file blob drift: {relative}",
            )
            if kind == "world_source":
                world_paths.add(relative)
        require(world_paths, "no explicit World source files are classified")
        require(
            "services/matrix-entry-adapter/src/lib.rs" in frozen_paths,
            "Matrix World command carrier is not frozen",
        )

        for marker in (
            "hepta-control-plane",
            "TrillionniumFoundation/CEX",
            "external_path_dependencies",
            "compatibility_inventory",
        ):
            require(
                marker in json.dumps(boundary, sort_keys=True),
                f"PROJECT_BOUNDARY.json lacks marker: {marker}",
            )

        workspace = tomllib.loads(committed_text("Cargo.toml"))
        members = workspace.get("workspace", {}).get("members", [])
        for member in members:
            cargo_relative = f"{member}/Cargo.toml"
            require(
                (ROOT / cargo_relative).is_file(),
                f"workspace member lacks Cargo.toml: {member}",
            )
            walk_paths(
                tomllib.loads(committed_text(cargo_relative)),
                ROOT / member,
            )

        for relative in (
            "services/consumer-entry-api/MODULE.md",
            "services/matrix-entry-adapter/MODULE.md",
            "docs/compatibility/world-surface-freeze-v1.md",
        ):
            text = committed_text(relative)
            for marker in ("World", "compatibility", "not_granted"):
                require(
                    marker in text,
                    f"{relative} lacks boundary marker: {marker}",
                )

        declared_hostile = manifest.get("hostile_fixtures")
        require(
            isinstance(declared_hostile, list),
            "hostile_fixtures must be a list",
        )
        require(
            set(declared_hostile) == EXPECTED_HOSTILE_FIXTURES,
            "hostile fixture declaration drift",
        )
        require(
            len(declared_hostile) == len(EXPECTED_HOSTILE_FIXTURES),
            "duplicate hostile fixture declaration",
        )
        executed_hostile = run_hostile_fixtures(deny_pattern)
        require(
            executed_hostile == EXPECTED_HOSTILE_FIXTURES,
            "hostile fixture execution drift",
        )

        result = {
            "schema": "cex.project-boundary-check.v2",
            "status": "ok",
            "quarantined_crates": sorted(crate_paths),
            "recursive_source_files": total_source_files,
            "explicit_world_source_files": len(world_paths),
            "frozen_files": len(frozen_paths),
            "hostile_fixtures_rejected": sorted(executed_hostile),
            "module_graph_policy": (
                "all_manifest_bound_rust_sources_reachable_from_standard_entrypoints"
            ),
            "indirect_source_policy": policy,
            "deny_changed_paths_regex_semantically_exercised": True,
            "owner_repository": manifest.get("owner_repository"),
            "transfer_status": manifest.get("transfer_status"),
            "production_authorization": "not_granted",
            "problems": [],
        }
        print(json.dumps(result, indent=2, sort_keys=True))
        return 0
    except BoundaryViolation as error:
        PROBLEMS.append(str(error))
        print(
            json.dumps(
                {
                    "schema": "cex.project-boundary-check.v2",
                    "status": "failed",
                    "production_authorization": "not_granted",
                    "problems": PROBLEMS,
                },
                indent=2,
                sort_keys=True,
            )
        )
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
