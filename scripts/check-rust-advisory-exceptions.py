#!/usr/bin/env python3
"""Fail closed around the bounded RustSec and Cargo policy exceptions used by CEX CI."""

from __future__ import annotations

import datetime as dt
import json
import os
import re
import subprocess
import tomllib
from pathlib import Path
from typing import Any, Iterable

ROOT = Path(__file__).resolve().parents[1]
POLICY_PATH = ROOT / "docs/security/rust-advisory-exceptions-v1.json"
LOCK_PATH = ROOT / "Cargo.lock"
DENY_PATH = ROOT / "deny.toml"
WORKFLOW_PATH = ROOT / ".github/workflows/trnm-economy-ci.yml"
ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")
DEPENDENCY_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")


class PolicyError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise PolicyError(message)


def read_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise PolicyError(f"cannot read {path.relative_to(ROOT)}: {error}") from error
    require(isinstance(value, dict), f"{path.relative_to(ROOT)} must contain an object")
    return value


def run(*args: str) -> str:
    env = os.environ.copy()
    env["CARGO_TERM_COLOR"] = "never"
    result = subprocess.run(
        list(args),
        cwd=ROOT,
        env=env,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        encoding="utf-8",
        errors="strict",
    )
    if result.returncode != 0:
        raise PolicyError(
            f"command failed ({result.returncode}): {' '.join(args)}\n{result.stderr.strip()}"
        )
    return ANSI_RE.sub("", result.stdout).strip()


def tool_version(command: str) -> str:
    return run(command, "--version").split()[1]


def ignored_advisories(deny: dict[str, Any]) -> dict[str, str]:
    raw = deny.get("advisories", {}).get("ignore", [])
    require(isinstance(raw, list), "deny.toml advisories.ignore must be a list")
    result: dict[str, str] = {}
    for item in raw:
        if isinstance(item, str):
            result[item] = ""
        elif isinstance(item, dict):
            advisory_id = item.get("id")
            reason = item.get("reason")
            require(isinstance(advisory_id, str), "deny.toml ignored advisory lacks id")
            require(
                isinstance(reason, str) and reason.strip(),
                f"deny.toml ignore {advisory_id} lacks reason",
            )
            result[advisory_id] = reason.strip()
        else:
            raise PolicyError("deny.toml advisories.ignore contains an unsupported entry")
    return result


def dependency_tables(document: dict[str, Any]) -> Iterable[tuple[str, dict[str, Any]]]:
    for table_name in DEPENDENCY_TABLES:
        table = document.get(table_name)
        if isinstance(table, dict):
            yield table_name, table
    targets = document.get("target")
    if isinstance(targets, dict):
        for target_name, target in targets.items():
            if not isinstance(target, dict):
                continue
            for table_name in DEPENDENCY_TABLES:
                table = target.get(table_name)
                if isinstance(table, dict):
                    yield f"target.{target_name}.{table_name}", table


def version_is_wildcard(value: str) -> bool:
    return "*" in value


def validate_dependency_spec(
    *,
    owner: str,
    section: str,
    dependency_name: str,
    spec: Any,
    manifest_dir: Path,
    workspace_specs: dict[str, Any],
    resolving_workspace: bool = False,
) -> str:
    label = f"{owner} [{section}] {dependency_name}"
    if isinstance(spec, str):
        require(spec.strip(), f"{label} has an empty version requirement")
        require(
            not version_is_wildcard(spec),
            f"{label} uses a registry wildcard version: {spec!r}",
        )
        return "registry"

    require(isinstance(spec, dict), f"{label} has an unsupported dependency shape")
    if spec.get("workspace") is True:
        require(not resolving_workspace, f"{label} recursively inherits a workspace dependency")
        require(
            dependency_name in workspace_specs,
            f"{label} refers to an undefined workspace dependency",
        )
        return validate_dependency_spec(
            owner="workspace.dependencies",
            section="workspace.dependencies",
            dependency_name=dependency_name,
            spec=workspace_specs[dependency_name],
            manifest_dir=ROOT,
            workspace_specs=workspace_specs,
            resolving_workspace=True,
        )

    path_value = spec.get("path")
    git_value = spec.get("git")
    require(
        not (isinstance(path_value, str) and isinstance(git_value, str)),
        f"{label} cannot be both path and git sourced",
    )
    if isinstance(git_value, str):
        raise PolicyError(f"{label} uses a git dependency; deny.toml permits no git sources")

    version_value = spec.get("version")
    if version_value is not None:
        require(isinstance(version_value, str), f"{label} version must be text")
        require(version_value.strip(), f"{label} has an empty version requirement")
        require(
            not version_is_wildcard(version_value),
            f"{label} uses a wildcard version: {version_value!r}",
        )

    if isinstance(path_value, str):
        require(path_value.strip(), f"{label} has an empty path")
        resolved = (manifest_dir / path_value).resolve()
        try:
            resolved.relative_to(ROOT.resolve())
        except ValueError as error:
            raise PolicyError(f"{label} path escapes the repository: {path_value}") from error
        require(
            (resolved / "Cargo.toml").is_file(),
            f"{label} path does not resolve to a Cargo package: {path_value}",
        )
        return "repository_path"

    require(
        isinstance(version_value, str),
        f"{label} is a registry dependency without an explicit non-wildcard version",
    )
    return "registry"


def validate_manifest_dependency_policy() -> dict[str, Any]:
    metadata = json.loads(
        run("cargo", "metadata", "--format-version", "1", "--no-deps", "--locked")
    )
    require(isinstance(metadata, dict), "cargo metadata did not return an object")
    require(
        Path(metadata["workspace_root"]).resolve() == ROOT.resolve(),
        "cargo metadata workspace root drift",
    )
    workspace_members = set(metadata.get("workspace_members", []))
    packages = metadata.get("packages", [])
    require(isinstance(packages, list), "cargo metadata package list is invalid")

    root_document = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    workspace_specs = root_document.get("workspace", {}).get("dependencies", {})
    require(isinstance(workspace_specs, dict), "workspace.dependencies must be a table")

    checked: list[dict[str, str]] = []
    for dependency_name, spec in sorted(workspace_specs.items()):
        source_kind = validate_dependency_spec(
            owner="Cargo.toml",
            section="workspace.dependencies",
            dependency_name=dependency_name,
            spec=spec,
            manifest_dir=ROOT,
            workspace_specs=workspace_specs,
            resolving_workspace=True,
        )
        checked.append(
            {
                "manifest": "Cargo.toml",
                "section": "workspace.dependencies",
                "dependency": dependency_name,
                "source_kind": source_kind,
            }
        )

    manifest_count = 0
    for package in sorted(
        (package for package in packages if package.get("id") in workspace_members),
        key=lambda package: package.get("manifest_path", ""),
    ):
        manifest_path = Path(package["manifest_path"]).resolve()
        try:
            relative = manifest_path.relative_to(ROOT.resolve()).as_posix()
        except ValueError as error:
            raise PolicyError(f"workspace manifest escapes repository: {manifest_path}") from error
        document = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        manifest_count += 1
        for section, table in dependency_tables(document):
            for dependency_name, spec in sorted(table.items()):
                source_kind = validate_dependency_spec(
                    owner=relative,
                    section=section,
                    dependency_name=dependency_name,
                    spec=spec,
                    manifest_dir=manifest_path.parent,
                    workspace_specs=workspace_specs,
                )
                checked.append(
                    {
                        "manifest": relative,
                        "section": section,
                        "dependency": dependency_name,
                        "source_kind": source_kind,
                    }
                )

    return {
        "workspace_manifest_count": manifest_count,
        "dependency_spec_count": len(checked),
        "repository_path_dependency_count": sum(
            item["source_kind"] == "repository_path" for item in checked
        ),
        "registry_dependency_count": sum(item["source_kind"] == "registry" for item in checked),
        "git_dependency_count": 0,
        "registry_wildcard_count": 0,
        "external_path_dependency_count": 0,
    }


def main() -> int:
    try:
        policy = read_json(POLICY_PATH)
        require(
            policy.get("schema") == "cex.rust-advisory-exceptions.v1",
            "policy schema mismatch",
        )
        require(
            policy.get("status") == "active_bounded_exceptions",
            "policy must be active",
        )
        require(
            policy.get("production_authorization") == "not_granted",
            "policy cannot grant production authorization",
        )

        today = dt.datetime.now(dt.timezone.utc).date()
        expires_on = dt.date.fromisoformat(str(policy.get("expires_on")))
        require(
            today <= expires_on,
            f"RustSec exception policy expired on {expires_on.isoformat()}",
        )

        exceptions = policy.get("exceptions")
        require(
            isinstance(exceptions, list) and exceptions,
            "exceptions must be a non-empty list",
        )
        expected_ids = {
            "RUSTSEC-2023-0071",
            "RUSTSEC-2026-0214",
            "RUSTSEC-2024-0436",
        }
        actual_ids = {
            entry.get("advisory_id")
            for entry in exceptions
            if isinstance(entry, dict)
        }
        require(actual_ids == expected_ids, f"exception ID drift: {sorted(actual_ids)}")
        require(len(exceptions) == len(expected_ids), "duplicate advisory exception")

        lock = tomllib.loads(LOCK_PATH.read_text(encoding="utf-8"))
        lock_packages = lock.get("package", [])
        require(isinstance(lock_packages, list), "Cargo.lock package inventory is invalid")
        locked = {
            (package.get("name"), package.get("version"))
            for package in lock_packages
            if isinstance(package, dict)
        }

        deny = tomllib.loads(DENY_PATH.read_text(encoding="utf-8"))
        deny_ignores = ignored_advisories(deny)
        require(
            set(deny_ignores) == expected_ids,
            f"deny.toml advisory ignore drift: {sorted(deny_ignores)}",
        )
        require(
            deny.get("advisories", {}).get("yanked") == "deny",
            "yanked dependencies must remain denied",
        )
        require(
            deny.get("advisories", {}).get("unused-ignored-advisory") == "allow",
            "unused ignored advisories must be handled by this reachability checker",
        )
        require(
            deny.get("bans", {}).get("wildcards") == "deny",
            "deny.toml must continue to deny wildcard dependency requirements",
        )
        require(
            deny.get("sources", {}).get("unknown-git") == "deny",
            "unknown git sources must remain denied",
        )
        require(
            not deny.get("sources", {}).get("allow-git"),
            "no git source allowlist is permitted by this policy",
        )

        license_exceptions = deny.get("licenses", {}).get("exceptions", [])
        normalized_license_exceptions = {
            (
                entry.get("name") or entry.get("crate"),
                entry.get("version"),
                tuple(entry.get("allow", [])),
            )
            for entry in license_exceptions
            if isinstance(entry, dict)
        }
        require(
            normalized_license_exceptions
            == {
                ("webpki-roots", "=0.26.11", ("CDLA-Permissive-2.0",)),
                ("webpki-roots", "=1.0.9", ("CDLA-Permissive-2.0",)),
            },
            f"license exception drift: {sorted(normalized_license_exceptions)}",
        )
        require(
            "CDLA-Permissive-2.0" not in deny.get("licenses", {}).get("allow", []),
            "CDLA-Permissive-2.0 must remain crate/version scoped",
        )

        workflow = WORKFLOW_PATH.read_text(encoding="utf-8")
        workflow_ignores = set(
            re.findall(r"--ignore\s+(RUSTSEC-\d{4}-\d{4})", workflow)
        )
        require(
            workflow_ignores == expected_ids,
            f"cargo-audit ignore drift: {sorted(workflow_ignores)}",
        )
        require(
            "python3 scripts/check-rust-advisory-exceptions.py" in workflow,
            "workflow does not execute the exception gate",
        )
        require(
            "cargo-audit@0.22.2,cargo-deny@0.20.2" in workflow,
            "supply-chain tool versions are not pinned",
        )
        require(
            "cargo deny check advisories licenses sources" in workflow,
            "workflow does not run advisory/license/source checks",
        )
        require(
            "cargo deny check --allow wildcard bans" in workflow,
            "workflow does not isolate cargo-deny path/workspace wildcard diagnostics",
        )

        require(
            tool_version("cargo-audit") == policy.get("cargo_audit_version"),
            "cargo-audit version drift",
        )
        require(
            tool_version("cargo-deny") == policy.get("cargo_deny_version"),
            "cargo-deny version drift",
        )
        require(
            run("rustc", "--version").split()[1] == policy.get("rust_toolchain"),
            "Rust toolchain drift",
        )

        manifest_policy = validate_manifest_dependency_policy()

        release_packages = policy.get("release_packages")
        require(
            isinstance(release_packages, list) and release_packages,
            "release_packages must be non-empty",
        )
        release_trees = {
            package: run(
                "cargo",
                "tree",
                "--locked",
                "--target",
                "all",
                "-p",
                package,
                "-e",
                "normal",
            )
            for package in release_packages
        }

        evidence: list[dict[str, Any]] = []
        for entry in exceptions:
            require(isinstance(entry, dict), "exception entry must be an object")
            advisory_id = str(entry["advisory_id"])
            package = str(entry["package"])
            version = str(entry["version"])
            spec = f"{package}@{version}"
            require(
                (package, version) in locked,
                f"{spec} no longer exists in Cargo.lock; remove {advisory_id}",
            )
            require(
                str(entry.get("reason", "")).strip(),
                f"{advisory_id} lacks a reason",
            )
            require(
                str(entry.get("removal_condition", "")).strip(),
                f"{advisory_id} lacks a removal condition",
            )

            all_graph = run(
                "cargo", "tree", "--locked", "--target", "all", "-i", spec, "-e", "all"
            )
            normal_graph = run(
                "cargo",
                "tree",
                "--locked",
                "--target",
                "all",
                "-i",
                spec,
                "-e",
                "normal",
            )
            if entry.get("all_target_graph_must_be_empty"):
                require(
                    not all_graph,
                    f"{advisory_id}/{spec} became reachable in the all-target graph",
                )
            if entry.get("normal_all_target_graph_must_be_empty"):
                require(
                    not normal_graph,
                    f"{advisory_id}/{spec} became reachable in the normal all-target graph",
                )
            for marker in entry.get("required_all_target_markers", []):
                require(
                    marker in all_graph,
                    f"{advisory_id} all-target path lost marker: {marker}",
                )
            for marker in entry.get("required_normal_all_target_markers", []):
                require(
                    marker in normal_graph,
                    f"{advisory_id} normal path lost marker: {marker}",
                )

            release_reachability = {
                release_package: re.search(
                    rf"(?m)^.*\b{re.escape(package)} v{re.escape(version)}(?:\s|$)",
                    tree,
                )
                is not None
                for release_package, tree in release_trees.items()
            }
            if entry.get("forbid_release_package_reachability"):
                require(
                    not any(release_reachability.values()),
                    f"{advisory_id}/{spec} reached a bounded release package: {release_reachability}",
                )

            evidence.append(
                {
                    "advisory_id": advisory_id,
                    "package_spec": spec,
                    "classification": entry.get("classification"),
                    "all_target_graph_empty": not all_graph,
                    "normal_all_target_graph_empty": not normal_graph,
                    "release_package_reachability": release_reachability,
                    "deny_reason": deny_ignores[advisory_id],
                }
            )

        result = {
            "schema": "cex.rust-advisory-exception-check.v2",
            "status": "bounded_exceptions_and_manifest_policy_valid",
            "checked_on": today.isoformat(),
            "expires_on": expires_on.isoformat(),
            "rust_toolchain": policy["rust_toolchain"],
            "cargo_audit_version": policy["cargo_audit_version"],
            "cargo_deny_version": policy["cargo_deny_version"],
            "manifest_dependency_policy": manifest_policy,
            "license_exceptions": [
                {
                    "package": "webpki-roots",
                    "version": "0.26.11",
                    "license": "CDLA-Permissive-2.0",
                },
                {
                    "package": "webpki-roots",
                    "version": "1.0.9",
                    "license": "CDLA-Permissive-2.0",
                },
            ],
            "advisory_exceptions": evidence,
            "production_authorization": "not_granted",
        }
        print(json.dumps(result, indent=2, sort_keys=True))
        return 0
    except (
        PolicyError,
        OSError,
        UnicodeDecodeError,
        ValueError,
        json.JSONDecodeError,
        tomllib.TOMLDecodeError,
    ) as error:
        print(
            json.dumps(
                {
                    "schema": "cex.rust-advisory-exception-check.v2",
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
