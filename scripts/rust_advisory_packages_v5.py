"""Recursive local-package and release-surface closure for CEX."""
from __future__ import annotations

import json
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from rust_advisory_common_v5 import (
    MINIMUM_CONTENT_MARKERS, MINIMUM_SURFACE_GLOBS, REQUIRED_TRIGGER_MARKERS,
    ROOT, SURFACE_POLICY_PATH, WORKFLOW_PATH, PolicyError, dependency_tables,
    package_name, read_json, referenced_packages, require, run, surface_reasons,
    validate_spec,
)


@dataclass(frozen=True)
class PackageRecord:
    name: str
    manifest: str
    proc_macro: bool
    build_script: str | None


def validate_cargo_config() -> list[str]:
    checked: list[str] = []
    cargo_dir = ROOT / ".cargo"
    if not cargo_dir.exists():
        return checked
    for path in sorted(cargo_dir.rglob("*")):
        if not path.is_file() or path.name not in {"config", "config.toml"}:
            continue
        relative = path.relative_to(ROOT).as_posix()
        document = tomllib.loads(path.read_text(encoding="utf-8"))
        require(not document.get("source"), f"{relative} defines Cargo source replacement")
        require(not document.get("paths"), f"{relative} defines Cargo path overrides")
        checked.append(relative)
    return checked


def validate_local_package_closure() -> tuple[dict[str, PackageRecord], dict[str, Any]]:
    root_document = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    require("patch" not in root_document and "replace" not in root_document, "root source replacement forbidden")
    metadata = json.loads(run("cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"))
    require(Path(metadata["workspace_root"]).resolve() == ROOT.resolve(), "cargo metadata workspace root drift")
    workspace_ids = set(metadata.get("workspace_members", []))
    packages = metadata.get("packages", [])
    require(isinstance(packages, list) and workspace_ids, "cargo metadata package inventory invalid")
    workspace_manifests = {
        Path(item["manifest_path"]).resolve() for item in packages if item.get("id") in workspace_ids
    }
    queue = list(sorted(workspace_manifests))
    visited: set[Path] = set()
    records: dict[str, PackageRecord] = {}
    dependency_specs = path_edges = registry_edges = 0
    build_scripts: list[str] = []
    proc_macros: list[str] = []

    workspace_deps = root_document.get("workspace", {}).get("dependencies", {})
    require(isinstance(workspace_deps, dict), "workspace.dependencies must be a table")
    for dep_name, spec in sorted(workspace_deps.items()):
        local = validate_spec(
            owner="Cargo.toml", section="workspace.dependencies", name=dep_name,
            spec=spec, manifest_dir=ROOT, root_document=root_document,
            resolving_workspace=True,
        )
        dependency_specs += 1
        if local is None:
            registry_edges += 1
        else:
            path_edges += 1
            queue.append(local.resolve())

    while queue:
        manifest = queue.pop()
        if manifest in visited:
            continue
        visited.add(manifest)
        try:
            relative = manifest.relative_to(ROOT.resolve()).as_posix()
        except ValueError as error:
            raise PolicyError(f"local package manifest escapes repository: {manifest}") from error
        document = tomllib.loads(manifest.read_text(encoding="utf-8"))
        require("patch" not in document and "replace" not in document, f"{relative} source replacement forbidden")
        name = package_name(document, relative)
        require(name not in records or records[name].manifest == relative, f"duplicate local package name: {name}")
        package = document.get("package", {})
        build_value = package.get("build")
        build_script = None
        if build_value is not False:
            candidate = manifest.parent / (build_value if isinstance(build_value, str) else "build.rs")
            if candidate.exists():
                require(candidate.is_file(), f"{relative} build script is not a file")
                build_script = candidate.relative_to(ROOT).as_posix()
                build_scripts.append(build_script)
        lib = document.get("lib", {})
        proc_macro = isinstance(lib, dict) and lib.get("proc-macro") is True
        if proc_macro:
            proc_macros.append(name)
        records[name] = PackageRecord(name, relative, proc_macro, build_script)
        for section, table in dependency_tables(document):
            for dep_name, spec in sorted(table.items()):
                local = validate_spec(
                    owner=relative, section=section, name=dep_name, spec=spec,
                    manifest_dir=manifest.parent, root_document=root_document,
                )
                dependency_specs += 1
                if local is None:
                    registry_edges += 1
                else:
                    path_edges += 1
                    queue.append(local.resolve())

    tracked = [Path(item) for item in run("git", "ls-files", "-z").split("\0") if item]
    disconnected: list[str] = []
    for path in tracked:
        if path.name != "Cargo.toml" or path == Path("Cargo.toml"):
            continue
        absolute = (ROOT / path).resolve()
        if absolute in visited:
            continue
        try:
            document = tomllib.loads(absolute.read_text(encoding="utf-8"))
        except (OSError, UnicodeDecodeError, tomllib.TOMLDecodeError):
            continue
        if isinstance(document.get("package"), dict):
            disconnected.append(path.as_posix())
    require(not disconnected, f"disconnected local Cargo packages are forbidden: {disconnected}")

    workspace_names = sorted(
        package_name(tomllib.loads(path.read_text(encoding="utf-8")), path.relative_to(ROOT).as_posix())
        for path in workspace_manifests
    )
    return records, {
        "workspace_packages": workspace_names,
        "reachable_local_packages": sorted(records),
        "workspace_package_count": len(workspace_names),
        "reachable_local_package_count": len(records),
        "dependency_spec_count": dependency_specs,
        "repository_path_dependency_count": path_edges,
        "registry_dependency_count": registry_edges,
        "git_dependency_count": 0,
        "registry_wildcard_count": 0,
        "external_path_dependency_count": 0,
        "disconnected_local_package_count": 0,
        "build_scripts": sorted(set(build_scripts)),
        "proc_macro_packages": sorted(set(proc_macros)),
        "cargo_config_files": validate_cargo_config(),
    }


def read_text_if_small(path: Path, limit: int) -> str:
    try:
        data = path.read_bytes()
    except OSError as error:
        raise PolicyError(f"cannot read tracked file {path.relative_to(ROOT)}: {error}") from error
    if len(data) > limit or b"\0" in data:
        return ""
    try:
        return data.decode("utf-8", errors="strict")
    except UnicodeDecodeError:
        return ""


def validate_release_surfaces(
    package_records: dict[str, PackageRecord], package_summary: dict[str, Any]
) -> dict[str, Any]:
    policy = read_json(SURFACE_POLICY_PATH)
    require(policy.get("schema") == "cex.rust-release-surfaces.v1", "release surface schema drift")
    require(policy.get("coverage_strategy") == "all_workspace_packages_are_release_candidates", "release coverage narrowed")
    require(policy.get("unreachable_local_package_manifests") == "forbidden", "disconnected packages must be forbidden")
    require(policy.get("unknown_explicit_package_references") == "forbidden", "unknown package references must be forbidden")
    require(policy.get("production_authorization") == "not_granted", "surface policy cannot authorize production")
    detection = policy.get("surface_detection")
    require(isinstance(detection, dict), "surface_detection missing")
    globs, markers = detection.get("path_globs"), detection.get("content_markers")
    require(isinstance(globs, list) and MINIMUM_SURFACE_GLOBS.issubset(set(globs)), "surface path detection narrowed")
    require(isinstance(markers, list) and MINIMUM_CONTENT_MARKERS.issubset(set(markers)), "surface content detection narrowed")
    limit = policy.get("maximum_text_scan_bytes")
    require(isinstance(limit, int) and limit >= 4 * 1024 * 1024, "surface text scan limit too small")

    metadata = json.loads(run("cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"))
    workspace_ids = set(metadata.get("workspace_members", []))
    workspace_packages = {
        item["name"]: item for item in metadata.get("packages", []) if item.get("id") in workspace_ids
    }
    require(set(workspace_packages) == set(package_summary["workspace_packages"]), "workspace package set drift")
    binary_to_package: dict[str, str] = {}
    for name, item in workspace_packages.items():
        for target in item.get("targets", []):
            if any(kind in {"bin", "cdylib", "staticlib"} for kind in target.get("kind", [])):
                target_name = target.get("name")
                if isinstance(target_name, str):
                    require(target_name not in binary_to_package or binary_to_package[target_name] == name, f"ambiguous target: {target_name}")
                    binary_to_package[target_name] = name

    allowed_manifests = {record.manifest for record in package_records.values()} | {"Cargo.toml"}
    surfaces: list[dict[str, Any]] = []
    explicit_packages: set[str] = set()
    for relative in (item for item in run("git", "ls-files", "-z").split("\0") if item):
        path = ROOT / relative
        text = read_text_if_small(path, limit)
        reasons = surface_reasons(relative, text, policy)
        if not reasons:
            continue
        packages, manifests, binaries = referenced_packages(text)
        for name in packages:
            require(name in package_records, f"release surface {relative} references unknown package {name}")
        for manifest_ref in manifests:
            candidate = (path.parent / manifest_ref).resolve()
            try:
                manifest_relative = candidate.relative_to(ROOT.resolve()).as_posix()
            except ValueError as error:
                raise PolicyError(f"release surface {relative} uses external manifest {manifest_ref}") from error
            require(candidate.is_file(), f"release surface {relative} references missing manifest {manifest_ref}")
            require(manifest_relative in allowed_manifests, f"release surface {relative} references disconnected manifest {manifest_relative}")
        known_binaries = {value for value in binaries if value in binary_to_package}
        unknown_cex = {
            value for value in binaries if value not in binary_to_package
            and ("cex" in value or value.endswith("-service") or value.startswith("trnm-"))
        }
        require(not unknown_cex, f"release surface {relative} references unknown CEX binaries: {sorted(unknown_cex)}")
        packages.update(binary_to_package[value] for value in known_binaries)
        explicit_packages.update(packages)
        surfaces.append({
            "path": relative, "reasons": reasons,
            "explicit_packages": sorted(packages),
            "explicit_manifests": sorted(manifests),
            "explicit_binaries": sorted(binaries),
        })
    require(surfaces, "no build/deploy/release surfaces discovered")
    workflow = WORKFLOW_PATH.read_text(encoding="utf-8")
    missing = sorted(marker for marker in REQUIRED_TRIGGER_MARKERS if marker not in workflow)
    require(not missing, f"workflow surface trigger coverage missing: {missing}")
    require("cargo build --release --locked --workspace" in workflow, "workflow must build all workspace candidates")
    return {
        "schema": policy["schema"],
        "coverage_strategy": policy["coverage_strategy"],
        "workspace_release_candidates": sorted(workspace_packages),
        "surface_count": len(surfaces),
        "explicitly_referenced_packages": sorted(explicit_packages),
        "surfaces": surfaces,
    }
