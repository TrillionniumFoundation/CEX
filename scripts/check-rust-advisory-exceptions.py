#!/usr/bin/env python3
"""Fail closed around the bounded RustSec exceptions used by CEX CI."""

from __future__ import annotations

import datetime as dt
import json
import os
import re
import subprocess
import tomllib
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
POLICY_PATH = ROOT / "docs/security/rust-advisory-exceptions-v1.json"
LOCK_PATH = ROOT / "Cargo.lock"
DENY_PATH = ROOT / "deny.toml"
WORKFLOW_PATH = ROOT / ".github/workflows/trnm-economy-ci.yml"
ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")


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
            "unused ignored advisories must be handled by the reachability checker",
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
            "schema": "cex.rust-advisory-exception-check.v1",
            "status": "bounded_exceptions_valid",
            "checked_on": today.isoformat(),
            "expires_on": expires_on.isoformat(),
            "rust_toolchain": policy["rust_toolchain"],
            "cargo_audit_version": policy["cargo_audit_version"],
            "cargo_deny_version": policy["cargo_deny_version"],
            "exceptions": evidence,
            "production_authorization": "not_granted",
        }
        print(json.dumps(result, indent=2, sort_keys=True))
        return 0
    except (
        PolicyError,
        OSError,
        UnicodeDecodeError,
        ValueError,
        tomllib.TOMLDecodeError,
    ) as error:
        print(
            json.dumps(
                {
                    "schema": "cex.rust-advisory-exception-check.v1",
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
