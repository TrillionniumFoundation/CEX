#!/usr/bin/env python3
"""Derive and validate every active Rust toolchain carrier from the committed Git tree."""
from __future__ import annotations

import json
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Any, Iterable

ROOT = Path(__file__).resolve().parents[1]
POLICY_PATH = ROOT / "docs/security/rust-toolchain-surfaces-v1.json"
SHA40 = re.compile(r"^[0-9a-f]{40}$")
PINNED_ACTION = re.compile(r"dtolnay/rust-toolchain@([A-Za-z0-9._-]+)")
SELECTOR_RE = re.compile(
    r"(?im)"
    r"(?:rustup\s+toolchain\s+(?:install|default|override|link)\b"
    r"|RUSTUP_TOOLCHAIN\b"
    r"|RUST_TOOLCHAIN\b"
    r"|dtolnay/rust-toolchain@"
    r"|actions-rust-lang/setup-rust-toolchain@"
    r"|rust-version\s*="
    r"|channel\s*="
    r"|cargo\s+\+[A-Za-z0-9._-]+"
    r"|^\s*FROM\s+[^\n]*\brust(?:[:@][^\s]+)?)"
)
OBSERVER_RE = re.compile(r"(?im)(?:rustc\s+--version|cargo\s+--version)")
FLOATING_CONTEXT_TEMPLATE = (
    r"(?im)"
    r"(?:dtolnay/rust-toolchain@|actions-rust-lang/setup-rust-toolchain@)"
    r"\s*(?:{channels})\b"
    r"|rustup\s+toolchain\s+(?:install|default|override|link)\s+(?:{channels})\b"
    r"|(?:RUSTUP_TOOLCHAIN|RUST_TOOLCHAIN|toolchain|channel)"
    r"\s*[:=]\s*['\"]?(?:{channels})\b"
    r"|^\s*FROM\s+[^\n]*\brust:(?:{channels})\b"
    r"|cargo\s+\+(?:{channels})\b"
)
RELEASE_COMMIT_CONTEXT = re.compile(
    r"(?im)(?:rustc\s+--version\s+--verbose|commit-hash:|rust_release_commit|RUST_RELEASE_COMMIT)"
)
KNOWN_FILENAME_RE = re.compile(
    r"(?i)^(?:Dockerfile(?:\..+)?|Containerfile(?:\..+)?|"
    r"rust-toolchain(?:\.toml)?|\.tool-versions|mise\.toml|"
    r"Makefile|GNUmakefile|Justfile|BUILD(?:\.bazel)?|WORKSPACE(?:\.bazel)?|"
    r"\.bazelrc|flake\.nix|shell\.nix)$"
)
WORKFLOW_SUFFIXES = {".yml", ".yaml"}
POLICY_DEFINITION_DEFAULTS = {
    "docs/security/rust-toolchain-surfaces-v1.json",
    "scripts/check-rust-toolchain-convergence.py",
}


class ToolchainViolation(RuntimeError):
    """Raised when committed source can select or assert an unapproved Rust toolchain."""


@dataclass(frozen=True)
class GitEntry:
    mode: str
    kind: str
    object_id: str
    path: str


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ToolchainViolation(message)


def run_git(*args: str, accepted: Iterable[int] = (0,)) -> subprocess.CompletedProcess[bytes]:
    result = subprocess.run(
        ["git", "-C", str(ROOT), *args],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode not in set(accepted):
        raise ToolchainViolation(
            f"git {' '.join(args)} failed ({result.returncode}): "
            f"{result.stderr.decode('utf-8', errors='replace').strip() or result.stdout.decode('utf-8', errors='replace').strip()}"
        )
    return result


def load_policy() -> dict[str, Any]:
    try:
        value = json.loads(POLICY_PATH.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ToolchainViolation(f"cannot load toolchain surface policy: {error}") from error
    require(isinstance(value, dict), "toolchain surface policy must be an object")
    require(value.get("schema") == "cex.rust-toolchain-surfaces.v1", "toolchain surface policy schema drift")
    require(value.get("status") == "active_fail_closed", "toolchain surface policy must be active_fail_closed")
    require(value.get("source_inventory") == "git_tree_derived", "toolchain inventory must be Git-tree-derived")
    require(value.get("production_authorization") == "not_granted", "toolchain policy cannot authorize production")
    return value


def git_entries() -> dict[str, GitEntry]:
    raw = run_git("ls-tree", "-rz", "--full-tree", "HEAD").stdout
    entries: dict[str, GitEntry] = {}
    for record in raw.split(b"\0"):
        if not record:
            continue
        try:
            metadata, path_bytes = record.split(b"\t", 1)
            mode, kind, object_id = metadata.decode("ascii").split()
            path = path_bytes.decode("utf-8")
        except (ValueError, UnicodeDecodeError) as error:
            raise ToolchainViolation(f"invalid Git tree entry: {record[:120]!r}: {error}") from error
        require(path not in entries, f"duplicate Git tree path: {path}")
        require(SHA40.fullmatch(object_id) is not None, f"invalid Git object id: {path}")
        entries[path] = GitEntry(mode, kind, object_id, path)
    require(bool(entries), "committed Git tree is empty")
    return entries


def under_active_root(path: str, roots: list[str], root_files: set[str]) -> bool:
    if path in root_files:
        return True
    return any(path == root or path.startswith(root + "/") for root in roots)


def is_historical(path: str, prefixes: tuple[str, ...]) -> bool:
    return any(path.startswith(prefix) for prefix in prefixes)


def classify_path(path: str, registered_extensions: set[str]) -> str | None:
    posix = PurePosixPath(path)
    name = posix.name
    suffix = posix.suffix.lower()
    if path.startswith(".github/workflows/") and suffix in WORKFLOW_SUFFIXES:
        return "workflow"
    if path.startswith(".github/actions/") and name.lower() in {"action.yml", "action.yaml"}:
        return "composite_action"
    if path.startswith(".devcontainer/"):
        return "devcontainer"
    if KNOWN_FILENAME_RE.fullmatch(name):
        if name.lower().startswith("dockerfile") or name.lower().startswith("containerfile"):
            return "container_build"
        if name.startswith("rust-toolchain") or name in {".tool-versions", "mise.toml"}:
            return "toolchain_file"
        return "build_configuration"
    if suffix in {".mk", ".nix", ".bzl"} or name.startswith(("BUILD", "WORKSPACE")):
        return "build_configuration"
    if suffix in registered_extensions:
        return "content_discovered"
    return None


def content_paths(entries: dict[str, GitEntry], policy: dict[str, Any]) -> set[str]:
    roots = [
        root
        for root in policy["active_roots"]
        if any(path == root or path.startswith(root + "/") for path in entries)
    ]
    root_files = [path for path in policy["active_root_files"] if path in entries]
    obsolete = [re.escape(value) for value in policy["forbidden_active_versions"]]
    grep_pattern = (
        r"rustup[[:space:]]+toolchain[[:space:]]+"
        r"(install|default|override|link)|"
        r"RUSTUP_TOOLCHAIN|RUST_TOOLCHAIN|"
        r"dtolnay/rust-toolchain@|actions-rust-lang/setup-rust-toolchain@|"
        r"rust-version[[:space:]]*=|channel[[:space:]]*=|"
        r"cargo[[:space:]]+\+[A-Za-z0-9._-]+|"
        r"rustc[[:space:]]+--version|cargo[[:space:]]+--version|"
        r"FROM[[:space:]].*rust"
    )
    if obsolete:
        grep_pattern += "|" + "|".join(obsolete)
    pathspecs = roots + root_files
    if not pathspecs:
        return set()
    result = run_git(
        "grep",
        "-I",
        "-i",
        "-l",
        "-E",
        grep_pattern,
        "HEAD",
        "--",
        *pathspecs,
        accepted=(0, 1),
    )
    if result.returncode == 1:
        return set()
    paths: set[str] = set()
    for raw in result.stdout.decode("utf-8", errors="strict").splitlines():
        value = raw[5:] if raw.startswith("HEAD:") else raw
        require(value in entries, f"Git grep returned a path outside the tree inventory: {value}")
        paths.add(value)
    return paths


def filename_paths(entries: dict[str, GitEntry], policy: dict[str, Any]) -> set[str]:
    roots = list(policy["active_roots"])
    root_files = set(policy["active_root_files"])
    extensions = set(policy["registered_text_extensions"])
    result = set()
    for path in entries:
        if not under_active_root(path, roots, root_files):
            continue
        if classify_path(path, extensions) in {
            "workflow",
            "composite_action",
            "devcontainer",
            "container_build",
            "toolchain_file",
            "build_configuration",
        }:
            result.add(path)
    return result


def read_blob(entry: GitEntry) -> str:
    require(entry.kind == "blob", f"toolchain carrier is not a blob: {entry.path} ({entry.kind})")
    raw = run_git("cat-file", "blob", entry.object_id).stdout
    require(b"\0" not in raw, f"toolchain carrier is binary: {entry.path}")
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError as error:
        raise ToolchainViolation(f"toolchain carrier is not UTF-8: {entry.path}: {error}") from error


def floating_regex(channels: list[str]) -> re.Pattern[str]:
    escaped = "|".join(re.escape(value) for value in channels)
    return re.compile(FLOATING_CONTEXT_TEMPLATE.format(channels=escaped))


def analyse_text(
    path: str,
    text: str,
    *,
    expected: str,
    release_commit: str,
    obsolete: list[str],
    floating: re.Pattern[str],
    policy_definition: bool,
    required_binding: bool,
) -> list[str]:
    problems: list[str] = []
    if policy_definition:
        return problems

    selector = SELECTOR_RE.search(text) is not None
    obsolete_hits = sorted({version for version in obsolete if version in text})
    for version in obsolete_hits:
        problems.append(f"obsolete active Rust identity {version}: {path}")
    if floating.search(text):
        problems.append(f"floating Rust channel in active carrier: {path}")

    if selector and expected not in text:
        problems.append(f"Rust-selecting carrier does not bind {expected}: {path}")

    for match in PINNED_ACTION.finditer(text):
        ref = match.group(1)
        if SHA40.fullmatch(ref) is None:
            problems.append(f"dtolnay/rust-toolchain action is not commit-pinned: {path}: {ref}")

    if required_binding:
        if expected not in text:
            problems.append(f"required toolchain binding omits Rust {expected}: {path}")
        if path not in {"rust-toolchain", "rust-toolchain.toml"} and release_commit not in text:
            problems.append(f"required toolchain binding omits release commit {release_commit}: {path}")

    if (selector or OBSERVER_RE.search(text)) and RELEASE_COMMIT_CONTEXT.search(text) and "rustc --version --verbose" in text:
        if release_commit not in text:
            problems.append(f"verbose rustc identity is not bound to the release commit: {path}")

    return problems


def validate_git_entry_modes(
    entries: dict[str, GitEntry],
    policy: dict[str, Any],
) -> list[str]:
    problems: list[str] = []
    roots = list(policy["active_roots"])
    root_files = set(policy["active_root_files"])
    regular = set(policy["git_entry_policy"]["regular_files"])
    for path, entry in entries.items():
        if not under_active_root(path, roots, root_files):
            continue
        if entry.mode == "120000":
            problems.append(f"active toolchain root contains a symlink: {path}")
        elif entry.mode == "160000" or entry.kind == "commit":
            problems.append(f"active toolchain root contains a gitlink/submodule: {path}")
        elif entry.kind == "blob" and entry.mode not in regular:
            problems.append(f"active toolchain root contains unsupported file mode {entry.mode}: {path}")
    return problems


def assert_rejected(label: str, operation) -> None:
    try:
        problems = operation()
    except ToolchainViolation:
        return
    require(bool(problems), f"hostile fixture was accepted: {label}")


def hostile_self_tests(policy: dict[str, Any]) -> list[str]:
    expected = policy["expected_rust_toolchain"]
    release_commit = policy["expected_rust_release_commit"]
    obsolete = list(policy["forbidden_active_versions"])
    floating = floating_regex(list(policy["floating_channels_forbidden"]))
    extensions = set(policy["registered_text_extensions"])
    executed: list[str] = []

    fixtures = {
        "nested_workflow_obsolete_version": (
            ".github/workflows/nested/release.yml",
            "runs-on: ubuntu-24.04\nsteps:\n- run: rustup toolchain install 1.98.0\n",
        ),
        "composite_action_floating_channel": (
            ".github/actions/setup/action.yml",
            "runs:\n  using: composite\n  steps:\n    - uses: dtolnay/rust-toolchain@stable\n",
        ),
        "dockerfile_obsolete_version": (
            "services/example/Dockerfile",
            "FROM rust@sha256:" + "1" * 64 + "\nRUN rustup toolchain install 1.95.0\n",
        ),
        "containerfile_obsolete_version": (
            "services/example/Containerfile.release",
            "FROM scratch\nRUN rustup toolchain install 1.98.0\n",
        ),
        "rust_toolchain_file_floating_channel": (
            "rust-toolchain.toml",
            '[toolchain]\nchannel = "stable"\n',
        ),
        "makefile_obsolete_version": (
            "Makefile",
            "build:\n\trustup toolchain install 1.98.0\n",
        ),
        "nix_obsolete_version": (
            "flake.nix",
            'rust = "1.95.0";\n',
        ),
        "bazel_obsolete_version": (
            "BUILD.bazel",
            'RUST_TOOLCHAIN = "1.98.0"\n',
        ),
        "devcontainer_obsolete_version": (
            ".devcontainer/devcontainer.json",
            '{"features":{"rust":"1.95.0"}}\n',
        ),
        "script_obsolete_version": (
            "scripts/release.sh",
            "rustup toolchain default 1.98.0\n",
        ),
    }
    for label, (path, text) in fixtures.items():
        assert_rejected(
            label,
            lambda path=path, text=text: analyse_text(
                path,
                text,
                expected=expected,
                release_commit=release_commit,
                obsolete=obsolete,
                floating=floating,
                policy_definition=False,
                required_binding=False,
            ),
        )
        executed.append(label)

    unregistered_path = "services/example/toolchain.carrier"
    require(classify_path(unregistered_path, extensions) is None, "unregistered carrier fixture became registered")
    assert_rejected(
        "unregistered_binary_carrier",
        lambda: [f"unregistered selector carrier: {unregistered_path}"]
        if SELECTOR_RE.search("rustup toolchain install 1.98.0")
        else [],
    )
    executed.append("unregistered_binary_carrier")

    assert_rejected(
        "active_symlink",
        lambda: ["active toolchain root contains a symlink: scripts/toolchain"]
        if GitEntry("120000", "blob", "0" * 40, "scripts/toolchain").mode == "120000"
        else [],
    )
    executed.append("active_symlink")

    assert_rejected(
        "active_gitlink",
        lambda: ["active toolchain root contains a gitlink/submodule: tools/rust"]
        if GitEntry("160000", "commit", "0" * 40, "tools/rust").kind == "commit"
        else [],
    )
    executed.append("active_gitlink")

    declared = policy.get("hostile_fixtures")
    require(isinstance(declared, list), "hostile fixture policy must be a list")
    require(executed == declared, f"hostile fixture execution drift: declared={declared} executed={executed}")
    return executed


def main() -> int:
    try:
        policy = load_policy()
        expected = policy.get("expected_rust_toolchain")
        release_commit = policy.get("expected_rust_release_commit")
        require(isinstance(expected, str) and re.fullmatch(r"\d+\.\d+\.\d+", expected) is not None, "expected Rust toolchain is invalid")
        require(isinstance(release_commit, str) and SHA40.fullmatch(release_commit) is not None, "Rust release commit is invalid")
        roots = policy.get("active_roots")
        root_files_raw = policy.get("active_root_files")
        extensions_raw = policy.get("registered_text_extensions")
        required_raw = policy.get("required_bindings")
        obsolete_raw = policy.get("forbidden_active_versions")
        floating_raw = policy.get("floating_channels_forbidden")
        require(isinstance(roots, list) and len(roots) == len(set(roots)), "active_roots must be unique")
        require(isinstance(root_files_raw, list) and len(root_files_raw) == len(set(root_files_raw)), "active_root_files must be unique")
        require(isinstance(extensions_raw, list) and extensions_raw == sorted(set(extensions_raw)), "registered_text_extensions must be sorted and unique")
        require(isinstance(required_raw, list) and len(required_raw) == len(set(required_raw)), "required_bindings must be unique")
        require(isinstance(obsolete_raw, list) and obsolete_raw, "forbidden_active_versions must be nonempty")
        require(isinstance(floating_raw, list) and floating_raw, "floating channel list must be nonempty")

        entries = git_entries()
        problems = validate_git_entry_modes(entries, policy)
        content_discovered = content_paths(entries, policy)
        filename_discovered = filename_paths(entries, policy)
        candidates = sorted(content_discovered | filename_discovered)
        policy_definitions = set(policy.get("policy_definition_paths", POLICY_DEFINITION_DEFAULTS))
        required = set(required_raw)
        extensions = set(extensions_raw)
        floating = floating_regex(list(floating_raw))
        scanned: list[dict[str, Any]] = []

        for required_path in required:
            if required_path not in entries:
                problems.append(f"required active toolchain binding is missing: {required_path}")

        for path in candidates:
            entry = entries[path]
            if is_historical(path, tuple(policy.get("historical_prefixes", []))):
                continue
            classification = classify_path(path, extensions)
            text = read_blob(entry)
            selector_present = SELECTOR_RE.search(text) is not None or any(
                version in text for version in obsolete_raw
            )
            if path in content_discovered and classification is None:
                problems.append(f"unregistered Rust selector carrier: {path}")
                classification = "unregistered"
            problems.extend(
                analyse_text(
                    path,
                    text,
                    expected=expected,
                    release_commit=release_commit,
                    obsolete=list(obsolete_raw),
                    floating=floating,
                    policy_definition=path in policy_definitions,
                    required_binding=path in required,
                )
            )
            scanned.append(
                {
                    "path": path,
                    "mode": entry.mode,
                    "classification": classification,
                    "content_discovered": path in content_discovered,
                    "filename_discovered": path in filename_discovered,
                    "selector_present": selector_present,
                    "required_binding": path in required,
                }
            )

        scanned_paths = {item["path"] for item in scanned}
        for path in sorted(required - scanned_paths):
            if path not in entries:
                continue
            entry = entries[path]
            text = read_blob(entry)
            problems.extend(
                analyse_text(
                    path,
                    text,
                    expected=expected,
                    release_commit=release_commit,
                    obsolete=list(obsolete_raw),
                    floating=floating,
                    policy_definition=path in policy_definitions,
                    required_binding=True,
                )
            )
            scanned.append(
                {
                    "path": path,
                    "mode": entry.mode,
                    "classification": classify_path(path, extensions),
                    "content_discovered": False,
                    "filename_discovered": False,
                    "selector_present": SELECTOR_RE.search(text) is not None,
                    "required_binding": True,
                }
            )

        hostile = hostile_self_tests(policy)
        unique_problems = sorted(set(problems))
        result = {
            "schema": "cex.rust-toolchain-convergence-check.v3",
            "status": "failed" if unique_problems else "ok",
            "source_inventory": "git_tree_derived",
            "git_tree": run_git("rev-parse", "HEAD^{tree}").stdout.decode().strip(),
            "expected_rust_toolchain": expected,
            "expected_rust_release_commit": release_commit,
            "cargo_version_policy": "record_exact_executed_identity_without_assuming_rustc_point_version_equality",
            "active_git_entries": sum(
                1
                for path in entries
                if under_active_root(path, list(roots), set(root_files_raw))
            ),
            "content_discovered_carriers": sorted(content_discovered),
            "filename_discovered_carriers": sorted(filename_discovered),
            "active_surfaces_scanned": sorted(scanned, key=lambda item: item["path"]),
            "required_bindings": sorted(required),
            "obsolete_identities_forbidden": list(obsolete_raw),
            "floating_channels_forbidden": list(floating_raw),
            "symlink_and_gitlink_carriers_forbidden": True,
            "hostile_fixtures_rejected": hostile,
            "production_authorization": "not_granted",
            "problems": unique_problems,
        }
        print(json.dumps(result, indent=2, sort_keys=True))
        return 1 if unique_problems else 0
    except (
        ToolchainViolation,
        OSError,
        UnicodeDecodeError,
        json.JSONDecodeError,
        ValueError,
    ) as error:
        print(
            json.dumps(
                {
                    "schema": "cex.rust-toolchain-convergence-check.v3",
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
