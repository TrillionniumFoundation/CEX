"""Recursive local-package and release-surface closure for CEX."""
from __future__ import annotations

import json
import re
import tomllib
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
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


EXTERNAL_EXECUTION_KEYS = {"unbound_explicit_checkout", "workflows"}
EXTERNAL_WORKFLOW_KEYS = {"checkouts"}
EXTERNAL_CHECKOUT_KEYS = {"path", "repository", "ref", "allowed_packages"}
PACKAGE_NAME = re.compile(r"^[A-Za-z0-9_.-]+$")
PINNED_CHECKOUT = re.compile(r"^actions/checkout@[0-9a-f]{40}$")


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


def _yaml_scalar(value: str) -> str:
    value = value.strip()
    if len(value) >= 2 and value[0] == value[-1] and value[0] in {"'", '"'}:
        value = value[1:-1]
    return value


def _field(block: str, indent: int, key: str) -> str | None:
    matches = re.findall(
        rf"(?m)^{{spaces}}{re.escape(key)}:\s*([^\r\n]+?)\s*$".format(spaces=" " * indent),
        block,
    )
    require(len(matches) <= 1, f"duplicate workflow field {key}")
    return _yaml_scalar(matches[0]) if matches else None


def _workflow_step_blocks(text: str) -> tuple[list[str], str]:
    lines = text.splitlines(keepends=True)
    spans: list[tuple[int, int]] = []
    index = 0
    while index < len(lines):
        line = lines[index]
        if re.fullmatch(r"    steps:\s*(?:\r?\n)?", line):
            section_start = index + 1
            section_end = section_start
            while section_end < len(lines):
                candidate = lines[section_end]
                if candidate.strip() and len(candidate) - len(candidate.lstrip(" ")) <= 4:
                    break
                section_end += 1
            starts = [
                cursor for cursor in range(section_start, section_end)
                if re.match(r"^      -\s+", lines[cursor])
            ]
            offsets = [0]
            for item in lines:
                offsets.append(offsets[-1] + len(item))
            for position, start in enumerate(starts):
                end = starts[position + 1] if position + 1 < len(starts) else section_end
                spans.append((offsets[start], offsets[end]))
            index = section_end
            continue
        index += 1
    require(bool(spans), "workflow contains no parseable steps")
    blocks = [text[start:end] for start, end in spans]
    residual = list(text)
    for start, end in spans:
        residual[start:end] = "\n" * (end - start)
    return blocks, "".join(residual)


def _safe_checkout_path(value: str, label: str) -> str:
    require(value and value == value.strip(), f"{label} must be a canonical literal path")
    require("\\" not in value and not value.startswith("/") and "${{" not in value, f"{label} is not literal")
    path = PurePosixPath(value)
    require(
        path.as_posix() == value and all(part not in {"", ".", ".."} for part in path.parts),
        f"{label} is not canonical",
    )
    return value


def _external_execution_policy(policy: dict[str, Any]) -> dict[str, dict[str, dict[str, Any]]]:
    external = policy.get("external_repository_execution")
    require(isinstance(external, dict) and set(external) == EXTERNAL_EXECUTION_KEYS, "external execution policy key drift")
    require(external.get("unbound_explicit_checkout") == "forbidden", "unbound explicit checkout policy weakened")
    workflows = external.get("workflows")
    require(isinstance(workflows, dict) and workflows, "external workflow policy is empty")
    normalized: dict[str, dict[str, dict[str, Any]]] = {}
    for workflow_path, raw_workflow in sorted(workflows.items()):
        require(
            isinstance(workflow_path, str)
            and workflow_path.startswith(".github/workflows/")
            and workflow_path.endswith((".yml", ".yaml")),
            f"invalid external workflow path: {workflow_path!r}",
        )
        require(isinstance(raw_workflow, dict) and set(raw_workflow) == EXTERNAL_WORKFLOW_KEYS, f"{workflow_path} policy key drift")
        checkouts = raw_workflow.get("checkouts")
        require(isinstance(checkouts, list) and checkouts, f"{workflow_path} checkouts are empty")
        by_path: dict[str, dict[str, Any]] = {}
        for raw_checkout in checkouts:
            require(isinstance(raw_checkout, dict) and set(raw_checkout) == EXTERNAL_CHECKOUT_KEYS, f"{workflow_path} checkout key drift")
            checkout_path = _safe_checkout_path(str(raw_checkout.get("path", "")), f"{workflow_path} checkout path")
            require("/" not in checkout_path, f"{workflow_path} checkout root must be one path segment")
            repository = raw_checkout.get("repository")
            ref = raw_checkout.get("ref")
            packages = raw_checkout.get("allowed_packages")
            require(isinstance(repository, str) and repository.startswith("${{ steps.") and repository.endswith(" }}"), f"{workflow_path}/{checkout_path} repository binding is not an exact step output")
            require(isinstance(ref, str) and ref.startswith("${{ steps.") and ref.endswith(" }}"), f"{workflow_path}/{checkout_path} ref binding is not an exact step output")
            require(
                isinstance(packages, list)
                and packages == sorted(set(packages))
                and packages
                and all(isinstance(name, str) and PACKAGE_NAME.fullmatch(name) for name in packages),
                f"{workflow_path}/{checkout_path} allowed package set is invalid",
            )
            require(checkout_path not in by_path, f"{workflow_path} duplicate checkout path: {checkout_path}")
            by_path[checkout_path] = {
                "repository": repository,
                "ref": ref,
                "allowed_packages": set(packages),
            }
        normalized[workflow_path] = by_path
    return normalized


def _classify_workflow_references(
    relative: str,
    text: str,
    checkout_policy: dict[str, dict[str, Any]],
) -> dict[str, Any]:
    blocks, residual = _workflow_step_blocks(text)
    checkout_blocks: dict[str, list[str]] = {}
    for block in blocks:
        uses = _field(block, 8, "uses")
        explicit_path = _field(block, 10, "path")
        if uses and uses.startswith("actions/checkout@"):
            require(PINNED_CHECKOUT.fullmatch(uses) is not None, f"{relative} uses an unpinned checkout action")
            if explicit_path is not None:
                path = _safe_checkout_path(explicit_path, f"{relative} checkout path")
                require("/" not in path, f"{relative} checkout path must be a root segment")
                checkout_blocks.setdefault(path, []).append(block)

    require(set(checkout_blocks) == set(checkout_policy), f"{relative} explicit checkout set drift")
    for checkout_path, expected in checkout_policy.items():
        candidates = checkout_blocks.get(checkout_path, [])
        require(len(candidates) == 1, f"{relative}/{checkout_path} must have exactly one checkout")
        block = candidates[0]
        require(_field(block, 10, "repository") == expected["repository"], f"{relative}/{checkout_path} repository binding drift")
        require(_field(block, 10, "ref") == expected["ref"], f"{relative}/{checkout_path} ref binding drift")
        require(_field(block, 10, "persist-credentials") == "false", f"{relative}/{checkout_path} must disable credential persistence")

    local_packages, local_manifests, local_binaries = referenced_packages(residual)
    external_packages: dict[str, set[str]] = {path: set() for path in checkout_policy}
    for block in blocks:
        packages, manifests, binaries = referenced_packages(block)
        working_directory = _field(block, 8, "working-directory")
        checkout_root: str | None = None
        if working_directory is not None:
            working_directory = _safe_checkout_path(working_directory, f"{relative} working-directory")
            first = PurePosixPath(working_directory).parts[0]
            if first in checkout_policy:
                checkout_root = first
        if checkout_root is None:
            local_packages.update(packages)
            local_manifests.update(manifests)
            local_binaries.update(binaries)
            continue
        expected = checkout_policy[checkout_root]
        unexpected = packages - expected["allowed_packages"]
        require(not unexpected, f"{relative}/{checkout_root} references undeclared external packages: {sorted(unexpected)}")
        require(not manifests, f"{relative}/{checkout_root} external manifest references are forbidden: {sorted(manifests)}")
        require(not binaries, f"{relative}/{checkout_root} external binary references are forbidden: {sorted(binaries)}")
        external_packages[checkout_root].update(packages)

    for checkout_root, expected in checkout_policy.items():
        require(
            external_packages[checkout_root] == expected["allowed_packages"],
            f"{relative}/{checkout_root} external package set drift: "
            f"expected={sorted(expected['allowed_packages'])} actual={sorted(external_packages[checkout_root])}",
        )
    return {
        "local_packages": local_packages,
        "local_manifests": local_manifests,
        "local_binaries": local_binaries,
        "external_packages": {
            path: sorted(packages) for path, packages in sorted(external_packages.items())
        },
    }


def _external_workflow_self_tests() -> dict[str, bool]:
    checkout_sha = "a" * 40
    policy = {
        "world": {
            "repository": "${{ steps.lock.outputs.world_repository }}",
            "ref": "${{ steps.lock.outputs.world_commit }}",
            "allowed_packages": {"trnm-game-server"},
        }
    }
    prefix = f"""jobs:\n  qualify:\n    steps:\n      - name: Checkout external\n        uses: actions/checkout@{checkout_sha}\n        with:\n          repository: ${{{{ steps.lock.outputs.world_repository }}}}\n          ref: ${{{{ steps.lock.outputs.world_commit }}}}\n          path: world\n          persist-credentials: false\n"""
    good = prefix + """      - name: External test\n        working-directory: world/trillionnium\n        run: cargo test -p trnm-game-server --locked\n"""
    observed = _classify_workflow_references(".github/workflows/fixture.yml", good, policy)
    require(observed["local_packages"] == set(), "external fixture leaked into local package references")
    require(observed["external_packages"] == {"world": ["trnm-game-server"]}, "external fixture was not classified")

    missing_context = prefix + """      - name: Local test\n        run: cargo test -p trnm-game-server --locked\n"""
    observed = _classify_workflow_references(".github/workflows/fixture.yml", missing_context, {
        "world": {**policy["world"], "allowed_packages": set()},
    })
    require("trnm-game-server" in observed["local_packages"], "unbound package was misclassified external")

    comment_spoof = prefix + """      - name: Comment spoof\n        run: |\n          # working-directory: world/trillionnium\n          cargo test -p trnm-game-server --locked\n"""
    observed = _classify_workflow_references(".github/workflows/fixture.yml", comment_spoof, {
        "world": {**policy["world"], "allowed_packages": set()},
    })
    require("trnm-game-server" in observed["local_packages"], "comment spoofed external working-directory")

    hostile: dict[str, str] = {
        "missing_checkout_rejected": """jobs:\n  qualify:\n    steps:\n      - name: External test\n        working-directory: world/trillionnium\n        run: cargo test -p trnm-game-server --locked\n""",
        "unexpected_package_rejected": prefix + """      - name: External test\n        working-directory: world/trillionnium\n        run: cargo test -p undeclared-world-package --locked\n""",
        "parent_traversal_rejected": prefix + """      - name: External test\n        working-directory: world/../cex\n        run: cargo test -p trnm-game-server --locked\n""",
    }
    results = {
        "exact_external_checkout_accepted": True,
        "unbound_package_remains_local": True,
        "comment_spoof_rejected": True,
    }
    for label, workflow in hostile.items():
        try:
            _classify_workflow_references(".github/workflows/fixture.yml", workflow, policy)
        except PolicyError:
            results[label] = True
        else:
            raise PolicyError(f"external workflow hostile fixture escaped: {label}")
    return results


def validate_release_surfaces(
    package_records: dict[str, PackageRecord], package_summary: dict[str, Any]
) -> dict[str, Any]:
    policy = read_json(SURFACE_POLICY_PATH)
    require(policy.get("schema") == "cex.rust-release-surfaces.v1", "release surface schema drift")
    require(policy.get("coverage_strategy") == "all_workspace_packages_are_release_candidates", "release coverage narrowed")
    require(policy.get("unreachable_local_package_manifests") == "forbidden", "disconnected packages must be forbidden")
    require(policy.get("unknown_explicit_package_references") == "forbidden", "unknown package references must be forbidden")
    require(policy.get("production_authorization") == "not_granted", "surface policy cannot authorize production")
    external_policy = _external_execution_policy(policy)
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
    external_packages: dict[str, dict[str, list[str]]] = {}
    seen_external_workflows: set[str] = set()
    for relative in (item for item in run("git", "ls-files", "-z").split("\0") if item):
        path = ROOT / relative
        text = read_text_if_small(path, limit)
        reasons = surface_reasons(relative, text, policy)
        if not reasons:
            continue
        if relative in external_policy:
            classified = _classify_workflow_references(relative, text, external_policy[relative])
            packages = classified["local_packages"]
            manifests = classified["local_manifests"]
            binaries = classified["local_binaries"]
            external_packages[relative] = classified["external_packages"]
            seen_external_workflows.add(relative)
        else:
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
        surface = {
            "path": relative, "reasons": reasons,
            "explicit_packages": sorted(packages),
            "explicit_manifests": sorted(manifests),
            "explicit_binaries": sorted(binaries),
        }
        if relative in external_packages:
            surface["external_checkout_packages"] = external_packages[relative]
        surfaces.append(surface)
    require(seen_external_workflows == set(external_policy), "configured external workflow was not discovered as a release surface")
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
        "external_repository_execution": external_packages,
        "external_workflow_self_tests": _external_workflow_self_tests(),
        "surfaces": surfaces,
    }
