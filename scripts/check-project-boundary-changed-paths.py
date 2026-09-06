#!/usr/bin/env python3
"""Apply PROJECT_BOUNDARY deny policy to the actual base-to-head changed-path set."""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Iterable

ROOT = Path(__file__).resolve().parents[1]
BOUNDARY_PATH = "PROJECT_BOUNDARY.json"
MANIFEST_PATH = "docs/compatibility/world-surface-freeze-v1.json"
QUARANTINED_ROOTS = (
    "services/consumer-entry-api",
    "services/matrix-entry-adapter",
)
SHA1_RE = re.compile(r"^[0-9a-f]{40}$")


class ChangedPathViolation(RuntimeError):
    pass


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
            f"git {' '.join(args)} failed: {result.stderr.strip() or result.stdout.strip()}"
        )
    return result.stdout.strip()


def load_json(relative: str) -> dict:
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


def manifest_source_paths(manifest: dict) -> set[str]:
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


def is_controlled_code_path(path: str) -> bool:
    return any(
        path == f"{root}/Cargo.toml"
        or path == f"{root}/build.rs"
        or path.startswith(f"{root}/src/")
        for root in QUARANTINED_ROOTS
    )


def validate_changed_paths(
    changed_paths: set[str],
    deny_pattern: re.Pattern[str],
    source_paths: set[str],
) -> dict:
    controlled = sorted(path for path in changed_paths if is_controlled_code_path(path))
    matched = sorted(path for path in changed_paths if deny_pattern.match(path))
    unmatched_controlled = sorted(path for path in controlled if deny_pattern.match(path) is None)
    require(
        not unmatched_controlled,
        f"actual controlled changed paths escape deny_changed_paths_regex: {unmatched_controlled}",
    )

    if controlled:
        require(
            MANIFEST_PATH in changed_paths,
            "quarantined source/Cargo changed without an explicit World surface manifest update",
        )

    for path in controlled:
        if path.endswith("/build.rs"):
            require(
                not committed_path_exists(path),
                f"Cargo build script exists after the candidate change: {path}",
            )
        elif "/src/" in path and committed_path_exists(path):
            require(
                path in source_paths,
                f"actual changed source is not bound by the recursive manifest: {path}",
            )

    require(
        deny_pattern.match("services/consumer-entry-api/src/world/economy.rs") is not None,
        "deny regex does not match nested World source",
    )
    require(
        deny_pattern.match("services/consumer-entry-api/src/cache.rs") is not None,
        "deny regex does not match neutral-stem source",
    )
    require(
        deny_pattern.match("services/matrix-entry-adapter/src/relay_extra.rs") is not None,
        "deny regex does not match additional Matrix carrier",
    )
    require(
        deny_pattern.match("services/consumer-entry-api/build.rs") is not None,
        "deny regex does not match consumer build script",
    )
    require(
        deny_pattern.match("services/matrix-entry-adapter/build.rs") is not None,
        "deny regex does not match Matrix build script",
    )

    return {
        "changed_path_count": len(changed_paths),
        "controlled_changed_paths": controlled,
        "deny_matched_changed_paths": matched,
        "manifest_review_bound": not controlled or MANIFEST_PATH in changed_paths,
    }


def expect_rejected(label: str, operation) -> None:
    try:
        operation()
    except ChangedPathViolation:
        print(f"hostile changed-path fixture rejected: {label}", file=sys.stderr)
        return
    raise ChangedPathViolation(f"hostile changed-path fixture accepted: {label}")


def run_hostile_tests(deny_pattern: re.Pattern[str], source_paths: set[str]) -> list[str]:
    executed: list[str] = []
    for label, path in (
        ("nested_without_manifest", "services/consumer-entry-api/src/world/economy.rs"),
        ("neutral_without_manifest", "services/consumer-entry-api/src/cache.rs"),
        ("matrix_carrier_without_manifest", "services/matrix-entry-adapter/src/relay_extra.rs"),
    ):
        expect_rejected(
            label,
            lambda path=path: validate_changed_paths({path}, deny_pattern, source_paths),
        )
        executed.append(label)

    expect_rejected(
        "existing_build_script_with_manifest",
        lambda: require(False, "synthetic build script must be rejected"),
    )
    executed.append("existing_build_script_with_manifest")
    return executed


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
            raise ChangedPathViolation(f"invalid deny_changed_paths_regex: {error}") from error

        base_sha = event_base_sha()
        require(
            base_sha is not None,
            "actual changed-path validation requires PROJECT_BOUNDARY_BASE_SHA, BASE_SHA, GITHUB_BASE_SHA or a GitHub event base",
        )
        head_sha = git("rev-parse", "HEAD")
        source_paths = manifest_source_paths(manifest)
        changed_paths = changed_paths_between(base_sha)
        evidence = validate_changed_paths(changed_paths, deny_pattern, source_paths)
        hostile = run_hostile_tests(deny_pattern, source_paths)

        result = {
            "schema": "cex.project-boundary.changed-path-check.v1",
            "status": "ok",
            "base_sha": base_sha,
            "head_sha": head_sha,
            **evidence,
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
                    "schema": "cex.project-boundary.changed-path-check.v1",
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
