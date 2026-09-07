#!/usr/bin/env python3
"""Fail closed around the bounded RustSec and Cargo policy exceptions used by CEX CI."""

from __future__ import annotations

import datetime as dt
import hashlib
import json
import os
import re
import shlex
import subprocess
import tempfile
import tomllib
from pathlib import Path
from typing import Any, Iterable

ROOT = Path(__file__).resolve().parents[1]
POLICY_PATH = ROOT / "docs/security/rust-advisory-exceptions-v1.json"
LOCK_PATH = ROOT / "Cargo.lock"
DENY_PATH = ROOT / "deny.toml"
WORKFLOW_PATH = ROOT / ".github/workflows/trnm-economy-ci.yml"
CODEOWNERS_PATH = ROOT / ".github/CODEOWNERS"
ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")
DEPENDENCY_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")
EXPECTED_ADVISORIES = {
    "RUSTSEC-2023-0071": ("rsa", "0.9.10"),
    "RUSTSEC-2026-0214": ("gumdrop", "0.8.1"),
    "RUSTSEC-2024-0436": ("paste", "1.0.15"),
}
EXPECTED_OWNER = "ProfHepta"
EXPECTED_APPROVER = "Tomasrgbsf"
EXPECTED_RISK_REPOSITORY = "TrillionniumFoundation/CEX"
EXPECTED_RISK_ISSUE = 35
EXPECTED_CANDIDATE_PR = 34
MAX_VALIDITY_DAYS = 30
EXPECTED_LICENSE_EXCEPTIONS = {
    ("webpki-roots", "=0.26.11", ("CDLA-Permissive-2.0",)),
    ("webpki-roots", "=1.0.9", ("CDLA-Permissive-2.0",)),
}
RELEASE_SOURCE_GLOBS = (
    "deploy/systemd/*.service",
    "ops/systemd/*.service",
    "scripts/install-*.sh",
    "scripts/package-*.sh",
    ".github/workflows/*.yml",
    ".github/workflows/*.yaml",
)


class PolicyError(RuntimeError):
    """The checked-in policy is incomplete, widened, stale, or unreachable."""


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


def run(*args: str, cwd: Path = ROOT, allow_empty_error: bool = False) -> str:
    env = os.environ.copy()
    env["CARGO_TERM_COLOR"] = "never"
    result = subprocess.run(
        list(args),
        cwd=cwd,
        env=env,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        encoding="utf-8",
        errors="strict",
    )
    if result.returncode != 0:
        if allow_empty_error and not result.stdout.strip():
            return ""
        raise PolicyError(
            f"command failed ({result.returncode}): {' '.join(args)}\n"
            f"{result.stderr.strip() or result.stdout.strip()}"
        )
    return ANSI_RE.sub("", result.stdout).strip()


def git_blob(path: Path) -> str:
    relative = path.resolve().relative_to(ROOT.resolve()).as_posix()
    value = run("git", "rev-parse", f"HEAD:{relative}")
    require(re.fullmatch(r"[0-9a-f]{40,64}", value) is not None, f"invalid Git blob for {relative}")
    return value


def tool_version(command: str) -> str:
    return run(command, "--version").split()[1]


def parse_date(value: Any, label: str) -> dt.date:
    require(isinstance(value, str), f"{label} must be an ISO date string")
    try:
        return dt.date.fromisoformat(value)
    except ValueError as error:
        raise PolicyError(f"{label} is not a canonical ISO date: {value!r}") from error


def validate_policy_time(policy: dict[str, Any], today: dt.date) -> dict[str, Any]:
    created_on = parse_date(policy.get("created_on"), "created_on")
    expires_on = parse_date(policy.get("expires_on"), "expires_on")
    require(created_on <= today, "created_on cannot be in the future")
    require(expires_on >= today, f"RustSec exception policy expired on {expires_on}")
    lifetime = (expires_on - created_on).days
    require(lifetime >= 0, "expires_on precedes created_on")
    require(
        lifetime <= MAX_VALIDITY_DAYS,
        f"policy lifetime {lifetime} days exceeds {MAX_VALIDITY_DAYS}",
    )
    require(
        policy.get("maximum_validity_days") == MAX_VALIDITY_DAYS,
        "maximum_validity_days drift",
    )
    renewal = policy.get("renewal")
    require(isinstance(renewal, dict), "renewal must be an object")
    require(renewal.get("sequence") == 1, "renewal sequence drift")
    require(
        renewal.get("previous_policy_blob") == "90670ef0a19bcd6a33cd61c8373ef24a73502ffc",
        "previous policy blob drift",
    )
    require(
        re.fullmatch(r"[0-9a-f]{40}", str(renewal.get("previous_policy_blob"))) is not None,
        "previous_policy_blob must be an exact SHA-1 blob",
    )
    require(
        run("git", "cat-file", "-t", str(renewal["previous_policy_blob"])) == "blob",
        "previous policy blob is not present in repository history",
    )
    renewed_on = parse_date(renewal.get("renewed_on"), "renewal.renewed_on")
    require(renewed_on == created_on, "renewal date must equal created_on")
    require(
        renewal.get("approval_requirement") == "fresh_exact_head_github_review",
        "renewal approval requirement drift",
    )
    require(
        isinstance(renewal.get("reason"), str) and renewal["reason"].strip(),
        "renewal requires a reason",
    )
    return {
        "created_on": created_on.isoformat(),
        "expires_on": expires_on.isoformat(),
        "lifetime_days": lifetime,
        "renewal_sequence": renewal["sequence"],
        "previous_policy_blob": renewal["previous_policy_blob"],
    }


def validate_policy_authority(policy: dict[str, Any]) -> dict[str, Any]:
    require(policy.get("accountable_owner") == EXPECTED_OWNER, "accountable owner drift")
    approval = policy.get("independent_security_approval")
    require(isinstance(approval, dict), "independent_security_approval must be an object")
    require(approval.get("required_github_login") == EXPECTED_APPROVER, "security approver drift")
    require(
        approval.get("evidence_type") == "github_pull_request_review",
        "approval evidence type drift",
    )
    require(approval.get("repository") == EXPECTED_RISK_REPOSITORY, "approval repository drift")
    require(approval.get("pull_request") == EXPECTED_CANDIDATE_PR, "approval PR drift")
    require(
        approval.get("head_binding") == "exact_final_head",
        "approval must bind the exact final head",
    )
    require(
        approval.get("status") == "required_external_not_embedded",
        "repository policy must not self-claim external approval",
    )

    risk = policy.get("risk_register")
    require(isinstance(risk, dict), "risk_register must be an object")
    require(risk.get("repository") == EXPECTED_RISK_REPOSITORY, "risk repository drift")
    require(risk.get("issue") == EXPECTED_RISK_ISSUE, "risk issue drift")
    require(risk.get("state_required") == "open", "risk issue must remain open")
    require(
        risk.get("closure_condition") == "all_exceptions_removed_or_superseded_by_fresh_approval",
        "risk closure condition drift",
    )

    codeowners = CODEOWNERS_PATH.read_text(encoding="utf-8")
    for path in (
        "/docs/security/rust-advisory-exceptions-v1.json",
        "/scripts/check-rust-advisory-exceptions.py",
        "/deny.toml",
        "/.github/workflows/trnm-economy-ci.yml",
    ):
        pattern = rf"(?m)^{re.escape(path)}\s+@{re.escape(EXPECTED_APPROVER)}\s*$"
        require(
            re.search(pattern, codeowners) is not None,
            f"CODEOWNERS must route {path} exclusively to @{EXPECTED_APPROVER}",
        )
    return {
        "accountable_owner": EXPECTED_OWNER,
        "required_independent_security_approver": EXPECTED_APPROVER,
        "risk_register": f"{EXPECTED_RISK_REPOSITORY}#{EXPECTED_RISK_ISSUE}",
        "approval_status": "required_external_not_embedded",
    }


def ignored_advisories(deny: dict[str, Any]) -> dict[str, str]:
    raw = deny.get("advisories", {}).get("ignore", [])
    require(isinstance(raw, list), "deny.toml advisories.ignore must be a list")
    result: dict[str, str] = {}
    for item in raw:
        require(isinstance(item, dict), "every deny.toml advisory ignore must have a reason")
        advisory_id = item.get("id")
        reason = item.get("reason")
        require(isinstance(advisory_id, str), "deny.toml ignored advisory lacks id")
        require(
            isinstance(reason, str) and reason.strip(),
            f"deny.toml ignore {advisory_id} lacks reason",
        )
        require(advisory_id not in result, f"duplicate deny.toml advisory ignore: {advisory_id}")
        result[advisory_id] = reason.strip()
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


def normalize_dependency_spec(
    *,
    owner: str,
    section: str,
    dependency_name: str,
    spec: Any,
    manifest_dir: Path,
    workspace_specs: dict[str, Any],
    resolving_workspace: bool = False,
) -> dict[str, Any]:
    label = f"{owner} [{section}] {dependency_name}"
    if isinstance(spec, str):
        require(spec.strip(), f"{label} has an empty version requirement")
        require(
            not version_is_wildcard(spec),
            f"{label} uses a registry wildcard version: {spec!r}",
        )
        return {"source_kind": "registry", "version": spec}

    require(isinstance(spec, dict), f"{label} has an unsupported dependency shape")
    if spec.get("workspace") is True:
        require(not resolving_workspace, f"{label} recursively inherits a workspace dependency")
        require(
            dependency_name in workspace_specs,
            f"{label} refers to an undefined workspace dependency",
        )
        return normalize_dependency_spec(
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
        raise PolicyError(f"{label} uses a git dependency; no git sources are permitted")

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
        return {
            "source_kind": "repository_path",
            "resolved_path": resolved,
            "version": version_value,
        }

    require(
        isinstance(version_value, str),
        f"{label} is a registry dependency without an explicit non-wildcard version",
    )
    return {"source_kind": "registry", "version": version_value}


def validate_no_manifest_source_replacement(document: dict[str, Any], label: str) -> None:
    require("patch" not in document, f"{label} contains a [patch] source replacement")
    require("replace" not in document, f"{label} contains a [replace] source replacement")


def validate_cargo_config() -> list[str]:
    checked: list[str] = []
    for path in sorted((ROOT / ".cargo").glob("**/*")) if (ROOT / ".cargo").exists() else []:
        if not path.is_file():
            continue
        relative = path.relative_to(ROOT).as_posix()
        checked.append(relative)
        if path.suffix in {".toml", ""}:
            document = tomllib.loads(path.read_text(encoding="utf-8"))
            require(not document.get("source"), f"{relative} defines Cargo source replacement")
            require(not document.get("paths"), f"{relative} defines Cargo path overrides")
    return checked


def validate_build_script(manifest_dir: Path, document: dict[str, Any], owner: str) -> str | None:
    package = document.get("package", {})
    require(isinstance(package, dict), f"{owner} package must be a table")
    build_value = package.get("build")
    if build_value is False:
        return None
    if isinstance(build_value, str):
        build_path = (manifest_dir / build_value).resolve()
        require(build_path.is_file(), f"{owner} declares missing build script: {build_value}")
        return build_path.relative_to(ROOT).as_posix()
    default = manifest_dir / "build.rs"
    if default.is_file():
        return default.relative_to(ROOT).as_posix()
    return None


def manifest_package_name(path: Path, document: dict[str, Any]) -> str:
    package = document.get("package")
    require(isinstance(package, dict), f"{path} has no [package] table")
    name = package.get("name")
    require(isinstance(name, str) and name.strip(), f"{path} lacks package.name")
    return name


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

    root_manifest = ROOT / "Cargo.toml"
    root_document = tomllib.loads(root_manifest.read_text(encoding="utf-8"))
    validate_no_manifest_source_replacement(root_document, "Cargo.toml")
    workspace_specs = root_document.get("workspace", {}).get("dependencies", {})
    require(isinstance(workspace_specs, dict), "workspace.dependencies must be a table")

    checked: list[dict[str, Any]] = []
    manifest_queue: list[Path] = []
    for dependency_name, spec in sorted(workspace_specs.items()):
        normalized = normalize_dependency_spec(
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
                "source_kind": normalized["source_kind"],
            }
        )
        if normalized["source_kind"] == "repository_path":
            manifest_queue.append(normalized["resolved_path"] / "Cargo.toml")
    for package in packages:
        if package.get("id") in workspace_members:
            manifest_queue.append(Path(package["manifest_path"]).resolve())

    seen: set[Path] = set()
    build_scripts: list[str] = []
    package_names: dict[str, str] = {}
    while manifest_queue:
        manifest_path = manifest_queue.pop()
        manifest_path = manifest_path.resolve()
        if manifest_path in seen:
            continue
        seen.add(manifest_path)
        try:
            relative = manifest_path.relative_to(ROOT.resolve()).as_posix()
        except ValueError as error:
            raise PolicyError(f"reachable manifest escapes repository: {manifest_path}") from error
        require(manifest_path.is_file(), f"reachable manifest is missing: {relative}")
        document = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        validate_no_manifest_source_replacement(document, relative)
        name = manifest_package_name(manifest_path, document)
        prior = package_names.get(name)
        require(prior in {None, relative}, f"duplicate local package name {name}: {prior}, {relative}")
        package_names[name] = relative
        build_script = validate_build_script(manifest_path.parent, document, relative)
        if build_script:
            build_scripts.append(build_script)

        for section, table in dependency_tables(document):
            for dependency_name, spec in sorted(table.items()):
                normalized = normalize_dependency_spec(
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
                        "source_kind": normalized["source_kind"],
                    }
                )
                if normalized["source_kind"] == "repository_path":
                    manifest_queue.append(normalized["resolved_path"] / "Cargo.toml")

    workspace_manifest_paths = {
        Path(package["manifest_path"]).resolve()
        for package in packages
        if package.get("id") in workspace_members
    }
    nonworkspace = sorted(
        path.relative_to(ROOT).as_posix() for path in seen if path not in workspace_manifest_paths
    )
    return {
        "reachable_manifest_count": len(seen),
        "workspace_manifest_count": len(workspace_manifest_paths),
        "nonworkspace_reachable_manifests": nonworkspace,
        "dependency_spec_count": len(checked),
        "repository_path_dependency_count": sum(
            item["source_kind"] == "repository_path" for item in checked
        ),
        "registry_dependency_count": sum(item["source_kind"] == "registry" for item in checked),
        "git_dependency_count": 0,
        "registry_wildcard_count": 0,
        "external_path_dependency_count": 0,
        "reachable_build_scripts": sorted(set(build_scripts)),
        "cargo_config_files": validate_cargo_config(),
        "manifest_blobs": {
            path.relative_to(ROOT).as_posix(): git_blob(path)
            for path in sorted(seen)
        },
    }


def binary_target_map(metadata: dict[str, Any]) -> dict[str, str]:
    mapping: dict[str, str] = {}
    for package in metadata.get("packages", []):
        for target in package.get("targets", []):
            if "bin" not in target.get("kind", []):
                continue
            binary = target.get("name")
            require(isinstance(binary, str), "Cargo target has no binary name")
            prior = mapping.get(binary)
            require(prior in {None, package["name"]}, f"ambiguous binary target {binary}")
            mapping[binary] = package["name"]
    return mapping


def parse_cargo_release_packages(text: str) -> set[str]:
    packages: set[str] = set()
    for match in re.finditer(r"cargo\s+(?:\+\S+\s+)?build\s+([^\n]+)", text):
        arguments = match.group(1)
        if "--release" not in arguments:
            continue
        tokens = shlex.split(arguments.replace("\\", " "))
        for index, token in enumerate(tokens):
            if token in {"-p", "--package"} and index + 1 < len(tokens):
                packages.add(tokens[index + 1])
    return packages


def derive_release_packages() -> dict[str, Any]:
    metadata = json.loads(
        run("cargo", "metadata", "--format-version", "1", "--no-deps", "--locked")
    )
    targets = binary_target_map(metadata)
    package_names = {package["name"] for package in metadata.get("packages", [])}
    sources: dict[str, set[str]] = {}
    scanned_files: list[str] = []
    unknown_cex_binaries: list[str] = []

    paths: set[Path] = set()
    for pattern in RELEASE_SOURCE_GLOBS:
        paths.update(path for path in ROOT.glob(pattern) if path.is_file())

    for path in sorted(paths):
        relative = path.relative_to(ROOT).as_posix()
        text = path.read_text(encoding="utf-8")
        scanned_files.append(relative)

        for line in text.splitlines():
            stripped = line.strip()
            if not stripped.startswith("ExecStart="):
                continue
            command = stripped.split("=", 1)[1].lstrip("-").strip()
            if not command:
                continue
            try:
                executable = shlex.split(command)[0]
            except (ValueError, IndexError) as error:
                raise PolicyError(f"cannot parse ExecStart in {relative}: {line}") from error
            binary = Path(executable).name
            if binary in targets:
                sources.setdefault(targets[binary], set()).add(relative)
            elif executable.startswith(("/opt/cex/bin/", str(ROOT / "target/release"))):
                unknown_cex_binaries.append(f"{relative}:{binary}")

        for binary in re.findall(r"target/release/([A-Za-z0-9_.-]+)", text):
            if binary in targets:
                sources.setdefault(targets[binary], set()).add(relative)
            else:
                unknown_cex_binaries.append(f"{relative}:{binary}")

        for package in parse_cargo_release_packages(text):
            require(package in package_names, f"{relative} releases unknown Cargo package {package}")
            sources.setdefault(package, set()).add(relative)

    require(not unknown_cex_binaries, f"unmapped release binaries: {unknown_cex_binaries}")
    require(sources, "no canonical release package source was discovered")
    return {
        "packages": sorted(sources),
        "sources": {package: sorted(files) for package, files in sorted(sources.items())},
        "scanned_files": scanned_files,
        "source_file_blobs": {
            relative: git_blob(ROOT / relative)
            for relative in scanned_files
            if sources and any(relative in files for files in sources.values())
        },
    }


def reverse_tree(package: str, version: str, edges: str) -> str:
    spec = f"{package}@{version}"
    return run(
        "cargo",
        "tree",
        "--locked",
        "--target",
        "all",
        "-i",
        spec,
        "-e",
        edges,
        allow_empty_error=True,
    )


def package_reachability(
    release_packages: list[str], package: str, version: str
) -> tuple[dict[str, bool], dict[str, str]]:
    needle = re.compile(rf"(?m)^.*\b{re.escape(package)} v{re.escape(version)}(?:\s|$)")
    reachability: dict[str, bool] = {}
    trees: dict[str, str] = {}
    for release_package in release_packages:
        tree = run(
            "cargo",
            "tree",
            "--locked",
            "--target",
            "all",
            "-p",
            release_package,
            "-e",
            "normal,build",
        )
        trees[release_package] = tree
        reachability[release_package] = needle.search(tree) is not None
    return reachability, trees


def expect_rejected(label: str, operation) -> None:
    try:
        operation()
    except PolicyError:
        return
    raise PolicyError(f"hostile fixture was accepted: {label}")


def require_expected_set(actual: set[str], expected: set[str], label: str) -> None:
    require(actual == expected, f"{label} drift: expected={sorted(expected)} actual={sorted(actual)}")


def run_hostile_fixtures(policy: dict[str, Any], deny: dict[str, Any]) -> list[str]:
    executed: list[str] = []

    far_future = dict(policy)
    far_future["expires_on"] = "2027-09-07"
    expect_rejected(
        "far_future_expiry",
        lambda: validate_policy_time(far_future, dt.date(2026, 9, 7)),
    )
    executed.append("far_future_expiry")

    added = [dict(entry) for entry in policy["exceptions"]]
    added.append(
        {
            "advisory_id": "RUSTSEC-2099-0001",
            "package": "hostile",
            "version": "1.0.0",
        }
    )
    expect_rejected(
        "added_advisory",
        lambda: require_expected_set(
            {entry.get("advisory_id") for entry in added},
            set(EXPECTED_ADVISORIES),
            "exception IDs",
        ),
    )
    executed.append("added_advisory")

    expanded_deny = json.loads(json.dumps(deny))
    expanded_deny["licenses"]["exceptions"].append(
        {"name": "hostile", "version": "=1.0.0", "allow": ["GPL-3.0-only"]}
    )
    expect_rejected(
        "license_exception_expansion",
        lambda: validate_license_policy(expanded_deny),
    )
    executed.append("license_exception_expansion")

    expect_rejected(
        "omitted_release_package",
        lambda: require_expected_set(
            {"ledger-service"},
            {"ledger-service", "consumer-entry-api"},
            "derived release packages",
        ),
    )
    executed.append("omitted_release_package")

    with tempfile.TemporaryDirectory(prefix="cex-advisory-fixture-") as temporary:
        root = Path(temporary)
        for name, version in EXPECTED_ADVISORIES.values():
            package = root / name
            (package / "src").mkdir(parents=True)
            proc_macro = name == "paste"
            (package / "Cargo.toml").write_text(
                "[package]\n"
                f'name = "{name}"\nversion = "{version}"\nedition = "2021"\n'
                + ("\n[lib]\nproc-macro = true\n" if proc_macro else ""),
                encoding="utf-8",
            )
            (package / "src/lib.rs").write_text(
                "extern crate proc_macro;\n" if proc_macro else "pub fn marker() {}\n",
                encoding="utf-8",
            )
        helper = root / "helper"
        (helper / "src").mkdir(parents=True)
        (helper / "Cargo.toml").write_text(
            "[package]\nname=\"helper\"\nversion=\"0.1.0\"\nedition=\"2021\"\n"
            "[dependencies]\nrsa={path=\"../rsa\"}\n",
            encoding="utf-8",
        )
        (helper / "src/lib.rs").write_text("pub fn helper() {}\n", encoding="utf-8")
        consumer = root / "consumer"
        (consumer / "src").mkdir(parents=True)
        (consumer / "Cargo.toml").write_text(
            "[package]\nname=\"hostile-consumer\"\nversion=\"0.1.0\"\nedition=\"2021\"\n"
            "[build-dependencies]\nhelper={path=\"../helper\"}\n"
            "[target.'cfg(unix)'.build-dependencies]\ngumdrop={path=\"../gumdrop\"}\n"
            "[dependencies]\npaste={path=\"../paste\"}\n",
            encoding="utf-8",
        )
        (consumer / "src/lib.rs").write_text("pub fn consumer() {}\n", encoding="utf-8")
        (consumer / "build.rs").write_text("fn main() {}\n", encoding="utf-8")
        (root / "Cargo.toml").write_text(
            "[workspace]\nresolver=\"2\"\nmembers=[\"consumer\",\"helper\",\"rsa\",\"gumdrop\",\"paste\"]\n",
            encoding="utf-8",
        )
        lock = run("cargo", "generate-lockfile", cwd=root)
        del lock
        hostile_tree = run(
            "cargo", "tree", "--target", "all", "-p", "hostile-consumer", "-e", "normal,build", cwd=root
        )
        for package in ("rsa v0.9.10", "gumdrop v0.8.1", "paste v1.0.15"):
            expect_rejected(
                f"release_build_reachability_{package.split()[0]}",
                lambda package=package: require(
                    package not in hostile_tree,
                    f"hostile release build closure reached {package}",
                ),
            )
            executed.append(f"release_build_reachability_{package.split()[0]}")

        carrier = root / "carrier"
        (carrier / "src").mkdir(parents=True)
        (carrier / "src/lib.rs").write_text("pub fn carrier() {}\n", encoding="utf-8")
        (carrier / "Cargo.toml").write_text(
            "[package]\nname=\"nonworkspace-carrier\"\nversion=\"0.1.0\"\nedition=\"2021\"\n"
            "[dependencies]\nremote={git=\"https://example.invalid/remote.git\"}\n",
            encoding="utf-8",
        )
        carrier_document = tomllib.loads((carrier / "Cargo.toml").read_text(encoding="utf-8"))
        expect_rejected(
            "nonworkspace_local_carrier",
            lambda: normalize_dependency_spec(
                owner="carrier/Cargo.toml",
                section="dependencies",
                dependency_name="remote",
                spec=carrier_document["dependencies"]["remote"],
                manifest_dir=carrier,
                workspace_specs={},
            ),
        )
        executed.append("nonworkspace_local_carrier")

    return executed


def validate_license_policy(deny: dict[str, Any]) -> set[tuple[str, str, tuple[str, ...]]]:
    licenses = deny.get("licenses", {})
    require(isinstance(licenses, dict), "deny.toml licenses must be a table")
    raw = licenses.get("exceptions", [])
    require(isinstance(raw, list), "licenses.exceptions must be a list")
    normalized = {
        (
            entry.get("name") or entry.get("crate"),
            entry.get("version"),
            tuple(entry.get("allow", [])),
        )
        for entry in raw
        if isinstance(entry, dict)
    }
    require(
        normalized == EXPECTED_LICENSE_EXCEPTIONS,
        f"license exception drift: {sorted(normalized)}",
    )
    require(
        "CDLA-Permissive-2.0" not in licenses.get("allow", []),
        "CDLA-Permissive-2.0 must remain crate/version scoped",
    )
    return normalized


def main() -> int:
    try:
        policy = read_json(POLICY_PATH)
        require(policy.get("schema") == "cex.rust-advisory-exceptions.v2", "policy schema mismatch")
        require(
            policy.get("status") == "active_bounded_exceptions",
            "policy must be active",
        )
        require(
            policy.get("production_authorization") == "not_granted",
            "policy cannot grant production authorization",
        )

        today = dt.datetime.now(dt.timezone.utc).date()
        time_evidence = validate_policy_time(policy, today)
        authority_evidence = validate_policy_authority(policy)

        exceptions = policy.get("exceptions")
        require(isinstance(exceptions, list) and exceptions, "exceptions must be non-empty")
        actual_ids = {
            entry.get("advisory_id") for entry in exceptions if isinstance(entry, dict)
        }
        require_expected_set(actual_ids, set(EXPECTED_ADVISORIES), "exception IDs")
        require(len(exceptions) == len(EXPECTED_ADVISORIES), "duplicate advisory exception")
        for entry in exceptions:
            advisory_id = entry.get("advisory_id")
            require(isinstance(advisory_id, str), "advisory entry lacks id")
            require(
                (entry.get("package"), entry.get("version")) == EXPECTED_ADVISORIES[advisory_id],
                f"{advisory_id} package/version drift",
            )
            require(str(entry.get("reason", "")).strip(), f"{advisory_id} lacks reason")
            require(
                str(entry.get("removal_condition", "")).strip(),
                f"{advisory_id} lacks removal condition",
            )

        lock = tomllib.loads(LOCK_PATH.read_text(encoding="utf-8"))
        lock_packages = lock.get("package", [])
        require(isinstance(lock_packages, list), "Cargo.lock package inventory is invalid")
        locked = {
            (package.get("name"), package.get("version"))
            for package in lock_packages
            if isinstance(package, dict)
        }
        for package in EXPECTED_ADVISORIES.values():
            require(package in locked, f"{package[0]}@{package[1]} left Cargo.lock; remove exception")

        deny = tomllib.loads(DENY_PATH.read_text(encoding="utf-8"))
        deny_ignores = ignored_advisories(deny)
        require_expected_set(set(deny_ignores), set(EXPECTED_ADVISORIES), "deny advisory IDs")
        require(
            deny.get("advisories", {}).get("yanked") == "deny",
            "yanked dependencies must remain denied",
        )
        require(
            deny.get("advisories", {}).get("unused-ignored-advisory") == "allow",
            "unused advisory exceptions must be handled by this checker",
        )
        require(
            deny.get("bans", {}).get("wildcards") == "deny",
            "deny.toml must continue to deny wildcard dependency requirements",
        )
        require(
            deny.get("sources", {}).get("unknown-git") == "deny",
            "unknown git sources must remain denied",
        )
        require(not deny.get("sources", {}).get("allow-git"), "no git source allowlist is permitted")
        license_exceptions = validate_license_policy(deny)

        workflow = WORKFLOW_PATH.read_text(encoding="utf-8")
        workflow_ignores = set(re.findall(r"--ignore\s+(RUSTSEC-\d{4}-\d{4})", workflow))
        require_expected_set(workflow_ignores, set(EXPECTED_ADVISORIES), "cargo-audit ignore IDs")
        for marker in (
            "fetch-depth: 0",
            "python3 scripts/check-rust-advisory-exceptions.py",
            "cargo-audit@0.22.2,cargo-deny@0.20.2",
            "cargo deny check advisories licenses sources",
            "cargo deny check --allow wildcard bans",
            "docs/security/rust-advisory-exceptions-v1.json",
            ".github/CODEOWNERS",
            "**/Cargo.toml",
            ".cargo/**",
            "deploy/systemd/**",
            "ops/systemd/**",
        ):
            require(marker in workflow, f"workflow lacks required marker: {marker}")

        require(tool_version("cargo-audit") == policy.get("cargo_audit_version"), "cargo-audit drift")
        require(tool_version("cargo-deny") == policy.get("cargo_deny_version"), "cargo-deny drift")
        require(run("rustc", "--version").split()[1] == policy.get("rust_toolchain"), "Rust drift")

        manifest_evidence = validate_manifest_dependency_policy()
        release = derive_release_packages()
        expected_release = policy.get("derived_release_package_expectation")
        require(isinstance(expected_release, list), "derived release expectation must be a list")
        require_expected_set(set(release["packages"]), set(expected_release), "derived release packages")
        require(len(expected_release) == len(set(expected_release)), "duplicate release package expectation")

        advisory_evidence: list[dict[str, Any]] = []
        for entry in exceptions:
            advisory_id = str(entry["advisory_id"])
            package = str(entry["package"])
            version = str(entry["version"])
            spec = f"{package}@{version}"
            all_graph = reverse_tree(package, version, "all")
            execution_graph = reverse_tree(package, version, "normal,build")
            if entry.get("all_target_graph_must_be_empty"):
                require(not all_graph, f"{advisory_id}/{spec} became all-target reachable")
            if entry.get("execution_graph_must_be_empty"):
                require(not execution_graph, f"{advisory_id}/{spec} became normal/build reachable")
            for marker in entry.get("required_all_target_markers", []):
                require(marker in all_graph, f"{advisory_id} all-target path lost marker: {marker}")
            for marker in entry.get("required_execution_markers", []):
                require(marker in execution_graph, f"{advisory_id} execution path lost marker: {marker}")

            reachability, release_trees = package_reachability(
                release["packages"], package, version
            )
            actual_reachable = {name for name, reachable in reachability.items() if reachable}
            expected_reachable = set(entry.get("expected_release_package_reachability", []))
            require(
                actual_reachable == expected_reachable,
                f"{advisory_id}/{spec} release reachability drift: "
                f"expected={sorted(expected_reachable)} actual={sorted(actual_reachable)}",
            )
            for release_package, markers in entry.get(
                "required_release_path_markers", {}
            ).items():
                require(
                    release_package in release_trees,
                    f"{advisory_id} marker package is not a release package: {release_package}",
                )
                for marker in markers:
                    require(
                        marker in release_trees[release_package],
                        f"{advisory_id}/{release_package} lost marker: {marker}",
                    )

            advisory_evidence.append(
                {
                    "advisory_id": advisory_id,
                    "package_spec": spec,
                    "classification": entry.get("classification"),
                    "all_target_graph_empty": not all_graph,
                    "normal_build_execution_graph_empty": not execution_graph,
                    "release_package_reachability": reachability,
                    "deny_reason": deny_ignores[advisory_id],
                }
            )

        hostile = run_hostile_fixtures(policy, deny)

        result = {
            "schema": "cex.rust-advisory-exception-check.v3",
            "status": "bounded_exceptions_and_complete_execution_closure_valid",
            "checked_on": today.isoformat(),
            "policy_blob": git_blob(POLICY_PATH),
            "checker_blob": git_blob(Path(__file__)),
            "time_policy": time_evidence,
            "authority": authority_evidence,
            "rust_toolchain": policy["rust_toolchain"],
            "cargo_audit_version": policy["cargo_audit_version"],
            "cargo_deny_version": policy["cargo_deny_version"],
            "manifest_dependency_policy": manifest_evidence,
            "derived_release_packages": release,
            "license_exceptions": [
                {"package": name, "version": version, "licenses": list(licenses)}
                for name, version, licenses in sorted(license_exceptions)
            ],
            "advisory_exceptions": advisory_evidence,
            "hostile_fixtures_rejected": hostile,
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
                    "schema": "cex.rust-advisory-exception-check.v3",
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
