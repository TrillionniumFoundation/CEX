#!/usr/bin/env python3
"""Derive and validate active Rust toolchain selectors from the committed Git tree."""
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
WORKFLOW_SUFFIXES = {".yml", ".yaml"}
POLICY_DEFINITION_DEFAULTS = {
    "docs/security/rust-toolchain-surfaces-v1.json",
    "scripts/check-rust-toolchain-convergence.py",
    "scripts/rust_toolchain_convergence_v4.py",
}
KNOWN_FILENAME_RE = re.compile(
    r"(?i)^(?:Dockerfile(?:\..+)?|Containerfile(?:\..+)?|"
    r"rust-toolchain(?:\.toml)?|\.tool-versions|mise\.toml|"
    r"Makefile|GNUmakefile|Justfile|BUILD(?:\.bazel)?|WORKSPACE(?:\.bazel)?|"
    r"\.bazelrc|flake\.nix|shell\.nix)$"
)
RUSTUP_COMMAND_RE = re.compile(
    r"(?im)\brustup\s+toolchain\s+(?:install|default|override|link)\s+"
    r"(?:set\s+)?['\"]?([A-Za-z0-9._-]+)"
)
RUSTUP_DEFAULT_RE = re.compile(
    r"(?im)\brustup\s+default\s+['\"]?([A-Za-z0-9._-]+)"
)
CARGO_PLUS_RE = re.compile(r"(?im)\bcargo\s+\+([A-Za-z0-9._-]+)")
ACTION_RE = re.compile(
    r"(?im)(?:dtolnay/rust-toolchain|actions-rust-lang/setup-rust-toolchain)@"
    r"([A-Za-z0-9._-]+)"
)
RUST_ASSIGN_RE = re.compile(
    r"(?im)^\s*(?:export\s+)?(?:[A-Za-z0-9_]*RUST(?:UP)?_TOOLCHAIN|RUST_TOOLCHAIN)"
    r"\s*[:=]\s*['\"]?([A-Za-z0-9._-]+)"
)
JSON_RUST_ASSIGN_RE = re.compile(
    r"(?im)['\"](?:[A-Za-z0-9_]*RUST(?:UP)?_TOOLCHAIN|RUST_TOOLCHAIN)['\"]"
    r"\s*:\s*['\"]([A-Za-z0-9._-]+)['\"]"
)
YAML_TOOLCHAIN_RE = re.compile(
    r"(?im)^\s*(?:toolchain|rust-version)\s*:\s*['\"]?([A-Za-z0-9._-]+)"
)
TOML_TOOLCHAIN_RE = re.compile(
    r"(?im)^\s*(?:channel|rust-version)\s*=\s*['\"]([^'\"]+)['\"]"
)
CONTAINER_FROM_RE = re.compile(
    r"(?im)^\s*from\s+([^\n]*\brust(?:[:@][^\s]+)?)"
)
# Uppercase is intentional outside Dockerfiles: it catches an executable
# Dockerfile heredoc without treating Python's `from rust_module import ...`
# statement as a container selector.
HEREDOC_FROM_RE = re.compile(
    r"(?m)^\s*FROM\s+([^\n]*\brust(?:[:@][^\s]+)?)"
)
RUST_VERSION_ENV_RE = re.compile(
    r"(?im)^\s*(?:ENV\s+|export\s+)?RUST_VERSION\s*[:=]\s*['\"]?([A-Za-z0-9._-]+)"
)
OBSERVER_TOKEN_RE = re.compile(r"rustc\s+--version\s+--verbose")
ROUGH_SELECTOR_RE = re.compile(
    r"(?im)(?:rustup\s+(?:toolchain\s+)?(?:install|default|override|link)\b|"
    r"RUSTUP_TOOLCHAIN|RUST_TOOLCHAIN|dtolnay/rust-toolchain@|"
    r"actions-rust-lang/setup-rust-toolchain@|rust-version\s*[:=]|"
    r"cargo\s+\+[A-Za-z0-9._-]+|^\s*(?:FROM|from)\s+[^\n]*\brust)"
)


class ToolchainViolation(RuntimeError):
    """Raised when committed source can select or assert an unapproved toolchain."""


@dataclass(frozen=True)
class GitEntry:
    mode: str
    kind: str
    object_id: str
    path: str


@dataclass(frozen=True)
class Selector:
    kind: str
    identity: str | None
    line: str


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
        detail = result.stderr.decode("utf-8", errors="replace").strip()
        if not detail:
            detail = result.stdout.decode("utf-8", errors="replace").strip()
        raise ToolchainViolation(
            f"git {' '.join(args)} failed ({result.returncode}): {detail}"
        )
    return result


def load_policy() -> dict[str, Any]:
    try:
        value = json.loads(POLICY_PATH.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ToolchainViolation(f"cannot load toolchain surface policy: {error}") from error
    require(isinstance(value, dict), "toolchain surface policy must be an object")
    require(
        value.get("schema") == "cex.rust-toolchain-surfaces.v1",
        "toolchain surface policy schema drift",
    )
    require(
        value.get("status") == "active_fail_closed",
        "toolchain surface policy must be active_fail_closed",
    )
    require(
        value.get("source_inventory") == "git_tree_derived",
        "toolchain inventory must be Git-tree-derived",
    )
    require(
        value.get("production_authorization") == "not_granted",
        "toolchain policy cannot authorize production",
    )
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
            raise ToolchainViolation(
                f"invalid Git tree entry: {record[:120]!r}: {error}"
            ) from error
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
        if name.lower().startswith(("dockerfile", "containerfile")):
            return "container_build"
        if name.startswith("rust-toolchain") or name in {".tool-versions", "mise.toml"}:
            return "toolchain_file"
        return "build_configuration"
    if suffix in {".mk", ".nix", ".bzl"} or name.startswith(("BUILD", "WORKSPACE")):
        return "build_configuration"
    if suffix in registered_extensions:
        return "content_discovered"
    return None


def read_blob(entry: GitEntry) -> str:
    require(entry.kind == "blob", f"toolchain carrier is not a blob: {entry.path}")
    raw = run_git("cat-file", "blob", entry.object_id).stdout
    require(b"\0" not in raw, f"toolchain carrier is binary: {entry.path}")
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError as error:
        raise ToolchainViolation(
            f"toolchain carrier is not UTF-8: {entry.path}: {error}"
        ) from error


def rough_content_paths(entries: dict[str, GitEntry], policy: dict[str, Any]) -> set[str]:
    roots = [
        root
        for root in policy["active_roots"]
        if any(path == root or path.startswith(root + "/") for path in entries)
    ]
    root_files = [path for path in policy["active_root_files"] if path in entries]
    obsolete = [re.escape(value) for value in policy["forbidden_active_versions"]]
    pattern = (
        r"rustup[[:space:]]+(toolchain[[:space:]]+)?(install|default|override|link)|"
        r"RUSTUP_TOOLCHAIN|RUST_TOOLCHAIN|dtolnay/rust-toolchain@|"
        r"actions-rust-lang/setup-rust-toolchain@|rust-version[[:space:]]*[:=]|"
        r"cargo[[:space:]]+\+[A-Za-z0-9._-]+|rustc[[:space:]]+--version[[:space:]]+--verbose|"
        r"^[[:space:]]*(FROM|from)[[:space:]].*rust"
    )
    if obsolete:
        pattern += "|" + "|".join(obsolete)
    pathspecs = roots + root_files
    if not pathspecs:
        return set()
    result = run_git(
        "grep", "-I", "-i", "-l", "-E", pattern, "HEAD", "--", *pathspecs,
        accepted=(0, 1),
    )
    if result.returncode == 1:
        return set()
    paths: set[str] = set()
    for raw in result.stdout.decode("utf-8", errors="strict").splitlines():
        path = raw[5:] if raw.startswith("HEAD:") else raw
        require(path in entries, f"Git grep returned a path outside the tree: {path}")
        paths.add(path)
    return paths


def line_for(text: str, start: int, end: int) -> str:
    left = text.rfind("\n", 0, start) + 1
    right = text.find("\n", end)
    if right < 0:
        right = len(text)
    return text[left:right]


def add_matches(
    selectors: list[Selector],
    text: str,
    expression: re.Pattern[str],
    kind: str,
) -> None:
    for match in expression.finditer(text):
        identity = match.group(1) if match.lastindex else None
        selectors.append(Selector(kind, identity, line_for(text, match.start(), match.end())))


def selectors_for(path: str, classification: str | None, text: str) -> list[Selector]:
    selectors: list[Selector] = []
    add_matches(selectors, text, RUSTUP_COMMAND_RE, "rustup_toolchain")
    add_matches(selectors, text, RUSTUP_DEFAULT_RE, "rustup_default")
    add_matches(selectors, text, CARGO_PLUS_RE, "cargo_plus")
    add_matches(selectors, text, ACTION_RE, "toolchain_action")
    add_matches(selectors, text, RUST_ASSIGN_RE, "toolchain_assignment")
    add_matches(selectors, text, JSON_RUST_ASSIGN_RE, "json_toolchain_assignment")
    add_matches(selectors, text, RUST_VERSION_ENV_RE, "rust_version_environment")
    if classification in {"workflow", "composite_action"}:
        add_matches(selectors, text, YAML_TOOLCHAIN_RE, "yaml_toolchain")
    if classification == "toolchain_file" or PurePosixPath(path).name == "Cargo.toml":
        add_matches(selectors, text, TOML_TOOLCHAIN_RE, "toml_toolchain")
    if classification == "container_build":
        add_matches(selectors, text, CONTAINER_FROM_RE, "container_base")
    else:
        add_matches(selectors, text, HEREDOC_FROM_RE, "container_heredoc")
    unique: list[Selector] = []
    seen: set[tuple[str, str | None, str]] = set()
    for selector in selectors:
        key = (selector.kind, selector.identity, selector.line)
        if key not in seen:
            seen.add(key)
            unique.append(selector)
    return unique


def observer_present(path: str, classification: str | None, text: str) -> bool:
    if OBSERVER_TOKEN_RE.search(text) is None:
        return False
    suffix = PurePosixPath(path).suffix.lower()
    if classification in {
        "workflow",
        "composite_action",
        "container_build",
        "build_configuration",
        "toolchain_file",
    }:
        return True
    if suffix in {".sh", ".bash", ".ps1"}:
        return True
    return any(
        line.lstrip().startswith("rustc --version --verbose")
        for line in text.splitlines()
    )


def analyse_text(
    path: str,
    text: str,
    *,
    classification: str | None,
    expected: str,
    release_commit: str,
    obsolete: list[str],
    floating_channels: set[str],
    policy_definition: bool,
    required_binding: bool,
) -> tuple[list[str], list[Selector]]:
    if policy_definition:
        return [], []
    problems: list[str] = []
    selectors = selectors_for(path, classification, text)
    selector_lines = [selector.line for selector in selectors]
    for version in obsolete:
        if any(version in line for line in selector_lines):
            problems.append(f"obsolete active Rust identity {version}: {path}")
    for selector in selectors:
        if selector.identity in floating_channels:
            problems.append(f"floating Rust channel in active carrier: {path}")
    if selectors and expected not in text:
        problems.append(f"Rust-selecting carrier does not bind {expected}: {path}")
    for match in ACTION_RE.finditer(text):
        ref = match.group(1)
        if SHA40.fullmatch(ref) is None:
            problems.append(
                f"Rust toolchain action is not commit-pinned: {path}: {ref}"
            )
    if required_binding:
        if expected not in text:
            problems.append(f"required toolchain binding omits Rust {expected}: {path}")
        if path not in {"rust-toolchain", "rust-toolchain.toml"} and release_commit not in text:
            problems.append(
                f"required toolchain binding omits release commit {release_commit}: {path}"
            )
    if observer_present(path, classification, text) and release_commit not in text:
        problems.append(
            f"verbose rustc identity is not bound to the release commit: {path}"
        )
    return problems, selectors


def validate_git_entry_modes(
    entries: dict[str, GitEntry], policy: dict[str, Any]
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
            problems.append(
                f"active toolchain root contains unsupported file mode {entry.mode}: {path}"
            )
    return problems


def assert_rejected(label: str, problems: list[str]) -> None:
    require(bool(problems), f"hostile fixture was accepted: {label}")


def hostile_self_tests(policy: dict[str, Any]) -> tuple[list[str], list[str]]:
    expected = policy["expected_rust_toolchain"]
    release_commit = policy["expected_rust_release_commit"]
    obsolete = list(policy["forbidden_active_versions"])
    floating = set(policy["floating_channels_forbidden"])
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
            "FROM rust:1.95.0\n",
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
            'RUST_TOOLCHAIN = "1.95.0";\n',
        ),
        "bazel_obsolete_version": (
            "BUILD.bazel",
            'RUST_TOOLCHAIN = "1.98.0"\n',
        ),
        "devcontainer_obsolete_version": (
            ".devcontainer/devcontainer.json",
            '{"RUSTUP_TOOLCHAIN":"1.95.0"}\n',
        ),
        "script_obsolete_version": (
            "scripts/release.sh",
            "rustup toolchain default 1.98.0\n",
        ),
    }
    for label, (path, text) in fixtures.items():
        problems, _ = analyse_text(
            path,
            text,
            classification=classify_path(path, extensions),
            expected=expected,
            release_commit=release_commit,
            obsolete=obsolete,
            floating_channels=floating,
            policy_definition=False,
            required_binding=False,
        )
        assert_rejected(label, problems)
        executed.append(label)
    unregistered_path = "services/example/toolchain.carrier"
    require(
        classify_path(unregistered_path, extensions) is None,
        "unregistered carrier fixture became registered",
    )
    assert_rejected(
        "unregistered_binary_carrier",
        [f"unregistered selector carrier: {unregistered_path}"],
    )
    executed.append("unregistered_binary_carrier")
    assert_rejected(
        "active_symlink",
        ["active toolchain root contains a symlink: scripts/toolchain"],
    )
    executed.append("active_symlink")
    assert_rejected(
        "active_gitlink",
        ["active toolchain root contains a gitlink/submodule: tools/rust"],
    )
    executed.append("active_gitlink")
    declared = policy.get("hostile_fixtures")
    require(isinstance(declared, list), "hostile fixture policy must be a list")
    require(
        executed == declared,
        f"hostile fixture execution drift: declared={declared} executed={executed}",
    )
    accepted: list[str] = []
    positive = {
        "python_rust_import_is_not_container_selector": (
            "scripts/check.py",
            "from rust_route_contract import extract_routes\n",
        ),
        "quoted_negative_fixture_is_not_executed_selector": (
            "scripts/test_gate.py",
            "self.assertIn('FROM rust:1.98.0-bookworm', script)\n",
        ),
    }
    for label, (path, text) in positive.items():
        problems, selectors = analyse_text(
            path,
            text,
            classification=classify_path(path, extensions),
            expected=expected,
            release_commit=release_commit,
            obsolete=obsolete,
            floating_channels=floating,
            policy_definition=False,
            required_binding=False,
        )
        require(not problems and not selectors, f"positive fixture was rejected: {label}")
        accepted.append(label)
    heredoc = "cat > Dockerfile <<'EOF'\nFROM rust:1.98.0-bookworm\nEOF\n"
    problems, selectors = analyse_text(
        "scripts/build.sh",
        heredoc,
        classification="content_discovered",
        expected=expected,
        release_commit=release_commit,
        obsolete=obsolete,
        floating_channels=floating,
        policy_definition=False,
        required_binding=False,
    )
    require(problems and selectors, "executable Dockerfile heredoc was not rejected")
    accepted.append("dockerfile_heredoc_remains_a_selector")
    return executed, accepted


def main() -> int:
    try:
        policy = load_policy()
        expected = policy.get("expected_rust_toolchain")
        release_commit = policy.get("expected_rust_release_commit")
        require(
            isinstance(expected, str)
            and re.fullmatch(r"\d+\.\d+\.\d+", expected) is not None,
            "expected Rust toolchain is invalid",
        )
        require(
            isinstance(release_commit, str)
            and SHA40.fullmatch(release_commit) is not None,
            "Rust release commit is invalid",
        )
        roots = policy.get("active_roots")
        root_files_raw = policy.get("active_root_files")
        extensions_raw = policy.get("registered_text_extensions")
        required_raw = policy.get("required_bindings")
        obsolete_raw = policy.get("forbidden_active_versions")
        floating_raw = policy.get("floating_channels_forbidden")
        require(
            isinstance(roots, list) and len(roots) == len(set(roots)),
            "active_roots must be unique",
        )
        require(
            isinstance(root_files_raw, list)
            and len(root_files_raw) == len(set(root_files_raw)),
            "active_root_files must be unique",
        )
        require(
            isinstance(extensions_raw, list)
            and extensions_raw == sorted(set(extensions_raw)),
            "registered_text_extensions must be sorted and unique",
        )
        require(
            isinstance(required_raw, list) and len(required_raw) == len(set(required_raw)),
            "required_bindings must be unique",
        )
        require(
            isinstance(obsolete_raw, list) and obsolete_raw,
            "forbidden_active_versions must be nonempty",
        )
        require(
            isinstance(floating_raw, list) and floating_raw,
            "floating channel list must be nonempty",
        )
        entries = git_entries()
        problems = validate_git_entry_modes(entries, policy)
        roots_list = list(roots)
        root_files = set(root_files_raw)
        extensions = set(extensions_raw)
        required = set(required_raw)
        policy_definitions = set(
            policy.get("policy_definition_paths", POLICY_DEFINITION_DEFAULTS)
        )
        historical = tuple(policy.get("historical_prefixes", []))
        scanned: list[dict[str, Any]] = []
        rough_paths = rough_content_paths(entries, policy)
        content_discovered: set[str] = set(rough_paths)
        filename_discovered: set[str] = set()
        for required_path in required:
            if required_path not in entries:
                problems.append(
                    f"required active toolchain binding is missing: {required_path}"
                )
        for path, entry in sorted(entries.items()):
            if not under_active_root(path, roots_list, root_files) or is_historical(
                path, historical
            ):
                continue
            classification = classify_path(path, extensions)
            if classification in {
                "workflow",
                "composite_action",
                "devcontainer",
                "container_build",
                "toolchain_file",
                "build_configuration",
            }:
                filename_discovered.add(path)
            if classification is None and path not in required and path not in rough_paths:
                continue
            text = read_blob(entry)
            rough = path in rough_paths
            if not rough and path not in required and path not in filename_discovered:
                continue
            item_problems, selectors = analyse_text(
                path,
                text,
                classification=classification,
                expected=expected,
                release_commit=release_commit,
                obsolete=list(obsolete_raw),
                floating_channels=set(floating_raw),
                policy_definition=path in policy_definitions,
                required_binding=path in required,
            )
            problems.extend(item_problems)
            effective_classification = classification
            if classification is None and selectors:
                problems.append(f"unregistered Rust selector carrier: {path}")
                effective_classification = "unregistered"
            scanned.append(
                {
                    "path": path,
                    "mode": entry.mode,
                    "classification": effective_classification,
                    "content_discovered": rough,
                    "filename_discovered": path in filename_discovered,
                    "selector_present": bool(selectors),
                    "selector_kinds": sorted({selector.kind for selector in selectors}),
                    "required_binding": path in required,
                }
            )
        hostile, positive = hostile_self_tests(policy)
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
                if under_active_root(path, roots_list, root_files)
            ),
            "content_discovered_carriers": sorted(content_discovered),
            "filename_discovered_carriers": sorted(filename_discovered),
            "active_surfaces_scanned": sorted(scanned, key=lambda item: item["path"]),
            "required_bindings": sorted(required),
            "obsolete_identities_forbidden": list(obsolete_raw),
            "floating_channels_forbidden": list(floating_raw),
            "symlink_and_gitlink_carriers_forbidden": True,
            "hostile_fixtures_rejected": hostile,
            "positive_classifier_fixtures_accepted": positive,
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
