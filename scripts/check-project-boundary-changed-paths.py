#!/usr/bin/env python3
"""Apply the project-boundary policy to the actual base-to-head changed-path set."""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Any, Callable

ROOT = Path(__file__).resolve().parents[1]
BOUNDARY_PATH = "PROJECT_BOUNDARY.json"
MANIFEST_PATH = "docs/compatibility/world-surface-freeze-v1.json"
CARGO_CLOSURE_CHECKER = "scripts/check-project-boundary-cargo-closure.py"
SHA1_RE = re.compile(r"^[0-9a-f]{40}$")
CARGO_CARRIER_PATH = re.compile(
    r"^(?:crates|services|apps|vendor|tools)/[^/]+/"
    r"(?:Cargo\.toml|build\.rs|src(?:/|$)|tests(?:/|$)|examples(?:/|$)|benches(?:/|$))"
)


class ChangedPathViolation(RuntimeError):
    """Raised when a changed path escapes the reviewed project boundary."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ChangedPathViolation(message)


def git(*args: str, check: bool = True) -> str:
    result = subprocess.run(
        ["git", "-C", str(ROOT), *args],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if check and result.returncode != 0:
        raise ChangedPathViolation(
            f"git {' '.join(args)} failed: "
            f"{result.stderr.strip() or result.stdout.strip()}"
        )
    return result.stdout.strip()


def load_json(relative: str) -> dict[str, Any]:
    try:
        value = json.loads((ROOT / relative).read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ChangedPathViolation(f"cannot read {relative}: {error}") from error
    require(isinstance(value, dict), f"{relative} must contain a JSON object")
    return value


def event_base_sha() -> str | None:
    for name in ("PROJECT_BOUNDARY_BASE_SHA", "BASE_SHA", "GITHUB_BASE_SHA"):
        value = os.environ.get(name, "").strip()
        if value:
            return value
    event_path = os.environ.get("GITHUB_EVENT_PATH", "").strip()
    if event_path and Path(event_path).is_file():
        try:
            event = json.loads(Path(event_path).read_text(encoding="utf-8"))
        except (OSError, UnicodeDecodeError, json.JSONDecodeError):
            event = {}
        pull_request = event.get("pull_request")
        if isinstance(pull_request, dict):
            value = pull_request.get("base", {}).get("sha")
            if isinstance(value, str) and value:
                return value
        value = event.get("before")
        if isinstance(value, str) and value and set(value) != {"0"}:
            return value
    return None


def committed_path_exists(path: str) -> bool:
    result = subprocess.run(
        ["git", "-C", str(ROOT), "cat-file", "-e", f"HEAD:{path}"],
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return result.returncode == 0


def manifest_root_source_paths(manifest: dict[str, Any]) -> set[str]:
    paths: set[str] = set()
    crates = manifest.get("quarantined_crates")
    require(isinstance(crates, list), "quarantined_crates must be a list")
    for crate in crates:
        require(isinstance(crate, dict), "quarantined crate entry must be an object")
        for source in crate.get("source_files", []):
            require(isinstance(source, dict), "source file entry must be an object")
            path = source.get("path")
            require(isinstance(path, str), "source file entry lacks path")
            require(path not in paths, f"duplicate manifest source path: {path}")
            paths.add(path)
    return paths


def manifest_controlled_packages(manifest: dict[str, Any]) -> set[str]:
    packages: set[str] = set()
    for field in ("quarantined_crates", "local_cargo_dependency_closure"):
        entries = manifest.get(field)
        require(isinstance(entries, list), f"{field} must be a list")
        for entry in entries:
            require(isinstance(entry, dict), f"{field} entry must be an object")
            path = entry.get("path")
            require(isinstance(path, str) and path, f"{field} package path missing")
            require(path not in packages, f"duplicate controlled package: {path}")
            packages.add(path)
    return packages


def package_for_path(path: str, packages: set[str]) -> str | None:
    matches = [
        package
        for package in packages
        if path == package or path.startswith(f"{package}/")
    ]
    if not matches:
        return None
    return max(matches, key=len)


def run_cargo_closure_checker() -> dict[str, Any]:
    result = subprocess.run(
        [sys.executable, str(ROOT / CARGO_CLOSURE_CHECKER)],
        cwd=ROOT,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    try:
        evidence = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise ChangedPathViolation(
            "Cargo closure checker did not emit one JSON object: "
            f"{result.stdout[:500]!r}; stderr={result.stderr[:500]!r}"
        ) from error
    require(
        result.returncode == 0 and evidence.get("status") == "ok",
        "transitive Cargo closure failed before changed-path acceptance: "
        f"{evidence.get('problems') or result.stderr.strip()}",
    )
    return evidence


def changed_paths_between(base_sha: str, head_sha: str = "HEAD") -> set[str]:
    require(SHA1_RE.fullmatch(base_sha) is not None, f"base SHA is not exact: {base_sha}")
    git("cat-file", "-e", f"{base_sha}^{{commit}}")
    output = git(
        "diff",
        "--name-only",
        "--diff-filter=ACMRD",
        base_sha,
        head_sha,
        "--",
    )
    return {line for line in output.splitlines() if line}


def validate_changed_paths(
    changed_paths: set[str],
    deny_pattern: re.Pattern[str],
    root_source_paths: set[str],
    controlled_packages: set[str],
    cargo_closure_green: bool,
) -> dict[str, Any]:
    controlled = sorted(
        path
        for path in changed_paths
        if path == "Cargo.toml"
        or path in {".cargo/config", ".cargo/config.toml"}
        or package_for_path(path, controlled_packages) is not None
    )
    potential_carriers = sorted(
        path for path in changed_paths if CARGO_CARRIER_PATH.match(path)
    )
    matched = sorted(path for path in changed_paths if deny_pattern.match(path))
    unmatched_controlled = sorted(
        path for path in controlled if deny_pattern.match(path) is None
    )
    require(
        not unmatched_controlled,
        "actual controlled changed paths escape deny_changed_paths_regex: "
        f"{unmatched_controlled}",
    )
    require(
        cargo_closure_green,
        "changed-path acceptance requires the exact transitive Cargo closure checker",
    )

    if controlled:
        require(
            MANIFEST_PATH in changed_paths,
            "controlled source/Cargo/package tree changed without an explicit "
            "World surface manifest update",
        )

    for path in controlled:
        package = package_for_path(path, controlled_packages)
        if path.endswith("/build.rs") or path in {
            "services/consumer-entry-api/build.rs",
            "services/matrix-entry-adapter/build.rs",
        }:
            require(
                not committed_path_exists(path),
                f"Cargo build script exists after the candidate change: {path}",
            )
        if package in {
            "services/consumer-entry-api",
            "services/matrix-entry-adapter",
        } and "/src/" in path and committed_path_exists(path):
            require(
                path in root_source_paths,
                f"actual quarantined source is not bound by the recursive manifest: {path}",
            )

    coverage_samples = (
        "Cargo.toml",
        "services/consumer-entry-api/src/world/economy.rs",
        "services/consumer-entry-api/src/cache.rs",
        "services/matrix-entry-adapter/src/relay_extra.rs",
        "services/ledger-service/src/lib.rs",
        "crates/shared-tracing/src/lib.rs",
        "crates/shared-config/src/lib.rs",
        "crates/shared-types/src/lib.rs",
        "vendor/trnm-economy-protocol/src/lib.rs",
        "services/consumer-entry-api/build.rs",
        "services/matrix-entry-adapter/build.rs",
    )
    for sample in coverage_samples:
        require(
            deny_pattern.match(sample) is not None,
            f"deny regex does not cover controlled or hostile path: {sample}",
        )

    return {
        "changed_path_count": len(changed_paths),
        "controlled_changed_paths": controlled,
        "potential_local_cargo_carrier_changed_paths": potential_carriers,
        "deny_matched_changed_paths": matched,
        "manifest_review_bound": not controlled or MANIFEST_PATH in changed_paths,
        "cargo_closure_verified": cargo_closure_green,
    }


def expect_rejected(label: str, operation: Callable[[], None]) -> None:
    try:
        operation()
    except ChangedPathViolation:
        print(f"hostile changed-path fixture rejected: {label}", file=sys.stderr)
        return
    raise ChangedPathViolation(f"hostile changed-path fixture accepted: {label}")


def run_hostile_tests(
    deny_pattern: re.Pattern[str],
    root_source_paths: set[str],
    controlled_packages: set[str],
) -> list[str]:
    executed: list[str] = []
    fixtures = (
        (
            "nested_without_manifest",
            {"services/consumer-entry-api/src/world/economy.rs"},
            True,
        ),
        (
            "neutral_without_manifest",
            {"services/consumer-entry-api/src/cache.rs"},
            True,
        ),
        (
            "matrix_carrier_without_manifest",
            {"services/matrix-entry-adapter/src/relay_extra.rs"},
            True,
        ),
        (
            "dependency_package_without_manifest",
            {"services/ledger-service/src/lib.rs"},
            True,
        ),
        (
            "workspace_manifest_without_boundary_manifest",
            {"Cargo.toml"},
            True,
        ),
        (
            "closure_checker_failure",
            {MANIFEST_PATH, "Cargo.toml"},
            False,
        ),
    )
    for label, paths, cargo_green in fixtures:
        expect_rejected(
            label,
            lambda paths=paths, cargo_green=cargo_green: validate_changed_paths(
                paths,
                deny_pattern,
                root_source_paths,
                controlled_packages,
                cargo_green,
            ),
        )
        executed.append(label)
    return executed


def main() -> int:
    try:
        boundary = load_json(BOUNDARY_PATH)
        manifest = load_json(MANIFEST_PATH)
        require(
            manifest.get("production_authorization") == "not_granted",
            "changed-path gate cannot operate on a production-authorized manifest",
        )
        regex = boundary.get("deny_changed_paths_regex")
        require(isinstance(regex, str), "deny_changed_paths_regex must be a string")
        try:
            deny_pattern = re.compile(regex)
        except re.error as error:
            raise ChangedPathViolation(
                f"invalid deny_changed_paths_regex: {error}"
            ) from error

        base_sha = event_base_sha()
        require(
            base_sha is not None,
            "actual changed-path validation requires PROJECT_BOUNDARY_BASE_SHA, "
            "BASE_SHA, GITHUB_BASE_SHA or a GitHub event base",
        )
        head_sha = git("rev-parse", "HEAD")
        root_source_paths = manifest_root_source_paths(manifest)
        controlled_packages = manifest_controlled_packages(manifest)
        cargo_evidence = run_cargo_closure_checker()
        changed_paths = changed_paths_between(base_sha)
        evidence = validate_changed_paths(
            changed_paths,
            deny_pattern,
            root_source_paths,
            controlled_packages,
            cargo_evidence.get("status") == "ok",
        )
        hostile = run_hostile_tests(
            deny_pattern,
            root_source_paths,
            controlled_packages,
        )

        result = {
            "schema": "cex.project-boundary.changed-path-check.v2",
            "status": "ok",
            "base_sha": base_sha,
            "head_sha": head_sha,
            "controlled_packages": sorted(controlled_packages),
            **evidence,
            "cargo_closure_summary": {
                "reachable_local_packages": cargo_evidence.get(
                    "reachable_local_packages",
                    [],
                ),
                "local_dependency_edges": cargo_evidence.get(
                    "local_dependency_edges",
                    0,
                ),
            },
            "hostile_fixtures_rejected": hostile,
            "deny_changed_paths_regex": regex,
            "production_authorization": "not_granted",
            "problems": [],
        }
        print(json.dumps(result, indent=2, sort_keys=True))
        return 0
    except ChangedPathViolation as error:
        print(
            json.dumps(
                {
                    "schema": "cex.project-boundary.changed-path-check.v2",
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
