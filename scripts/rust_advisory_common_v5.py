"""Shared fail-closed primitives for the CEX Rust supply-chain gate."""
from __future__ import annotations

import fnmatch
import json
import os
import re
import subprocess
import tomllib
from pathlib import Path
from typing import Any, Iterable

ROOT = Path(__file__).resolve().parents[1]
POLICY_PATH = ROOT / "docs/security/rust-advisory-exceptions-v1.json"
SURFACE_POLICY_PATH = ROOT / "docs/security/rust-release-surfaces-v1.json"
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
EXPECTED_LICENSE_EXCEPTIONS = {
    ("webpki-roots", "=0.26.11", ("CDLA-Permissive-2.0",)),
    ("webpki-roots", "=1.0.9", ("CDLA-Permissive-2.0",)),
}
MINIMUM_SURFACE_GLOBS = {
    ".github/workflows/*.yml", ".github/workflows/*.yaml",
    "Dockerfile", "Dockerfile.*", "Containerfile", "Containerfile.*",
    "**/Dockerfile", "**/Dockerfile.*", "**/Containerfile", "**/Containerfile.*",
    "**/docker-compose*.yml", "**/docker-compose*.yaml",
    "**/compose*.yml", "**/compose*.yaml",
    "deploy/**", "ops/**", "k8s/**", "kubernetes/**", "helm/**", "charts/**",
    "Makefile", "makefile", "GNUmakefile", "Justfile", "justfile",
    "**/Makefile", "**/makefile", "**/GNUmakefile", "**/Justfile", "**/justfile",
    "**/*.mk", "scripts/**",
}
MINIMUM_CONTENT_MARKERS = {
    "cargo build", "cargo install", "cargo publish", "docker build",
    "docker buildx build", "podman build", "target/release/", "/opt/cex/bin/",
    "ExecStart=", "helm install", "helm upgrade", "helm package", "kubectl apply",
    "gh release",
}
REQUIRED_TRIGGER_MARKERS = {
    "'**/Dockerfile*'", "'**/Containerfile*'",
    "'**/docker-compose*.yml'", "'**/docker-compose*.yaml'",
    "'**/compose*.yml'", "'**/compose*.yaml'", "'.github/workflows/**'",
    "'deploy/**'", "'ops/**'", "'k8s/**'", "'kubernetes/**'", "'helm/**'",
    "'charts/**'", "'**/Makefile'", "'**/makefile'", "'**/GNUmakefile'",
    "'**/Justfile'", "'**/justfile'", "'**/*.mk'", "'scripts/**'",
    "'docs/security/rust-release-surfaces-v1.json'",
    "'docs/security/rust-advisory-exceptions-v1.json'",
}


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


def checked_result(result: Any, label: str) -> str:
    """Only a zero exit is success; empty output never converts an error to absence."""
    if result.returncode != 0:
        detail = (result.stderr or result.stdout or "<no output>").strip()
        raise PolicyError(f"command failed ({result.returncode}): {label}\n{detail}")
    return ANSI_RE.sub("", result.stdout or "").strip()


def run(*args: str, cwd: Path = ROOT) -> str:
    env = os.environ.copy()
    env["CARGO_TERM_COLOR"] = "never"
    return checked_result(
        subprocess.run(
            list(args), cwd=cwd, env=env, check=False,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            encoding="utf-8", errors="strict",
        ),
        " ".join(args),
    )


def dependency_tables(document: dict[str, Any]) -> Iterable[tuple[str, dict[str, Any]]]:
    for name in DEPENDENCY_TABLES:
        value = document.get(name)
        if isinstance(value, dict):
            yield name, value
    targets = document.get("target")
    if isinstance(targets, dict):
        for target_name, target in targets.items():
            if not isinstance(target, dict):
                continue
            for name in DEPENDENCY_TABLES:
                value = target.get(name)
                if isinstance(value, dict):
                    yield f"target.{target_name}.{name}", value


def package_name(document: dict[str, Any], label: str) -> str:
    package = document.get("package")
    require(isinstance(package, dict), f"{label} lacks [package]")
    name = package.get("name")
    require(isinstance(name, str) and name.strip(), f"{label} lacks package.name")
    return name


def workspace_dependency(root_document: dict[str, Any], name: str) -> Any:
    table = root_document.get("workspace", {}).get("dependencies", {})
    require(isinstance(table, dict) and name in table, f"undefined workspace dependency: {name}")
    return table[name]


def validate_spec(
    *, owner: str, section: str, name: str, spec: Any,
    manifest_dir: Path, root_document: dict[str, Any], resolving_workspace: bool = False,
) -> Path | None:
    label = f"{owner} [{section}] {name}"
    if isinstance(spec, str):
        require(spec.strip() and "*" not in spec, f"{label} uses an empty or wildcard registry version")
        return None
    require(isinstance(spec, dict), f"{label} has unsupported dependency syntax")
    if spec.get("workspace") is True:
        require(not resolving_workspace, f"{label} recursively inherits workspace dependency")
        return validate_spec(
            owner="workspace.dependencies", section="workspace.dependencies", name=name,
            spec=workspace_dependency(root_document, name), manifest_dir=ROOT,
            root_document=root_document, resolving_workspace=True,
        )
    require(not isinstance(spec.get("git"), str), f"{label} uses a forbidden git dependency")
    version = spec.get("version")
    if version is not None:
        require(isinstance(version, str) and version.strip() and "*" not in version, f"{label} uses an invalid version")
    path_value = spec.get("path")
    if isinstance(path_value, str):
        require(path_value.strip(), f"{label} has an empty path")
        resolved = (manifest_dir / path_value).resolve()
        try:
            resolved.relative_to(ROOT.resolve())
        except ValueError as error:
            raise PolicyError(f"{label} escapes the repository: {path_value}") from error
        require((resolved / "Cargo.toml").is_file(), f"{label} does not resolve to a package")
        return resolved / "Cargo.toml"
    require(isinstance(version, str), f"{label} registry dependency lacks an explicit version")
    return None


def surface_reasons(relative: str, text: str, policy: dict[str, Any]) -> list[str]:
    reasons = [
        f"path:{pattern}" for pattern in policy["surface_detection"]["path_globs"]
        if fnmatch.fnmatch(relative, pattern)
    ]
    lower = text.lower()
    reasons.extend(
        f"content:{marker}" for marker in policy["surface_detection"]["content_markers"]
        if marker.lower() in lower
    )
    return sorted(set(reasons))


def referenced_packages(text: str) -> tuple[set[str], set[str], set[str]]:
    normalized = re.sub(r"\\\s*\n\s*", " ", text)
    commands = re.findall(
        r"(?im)\bcargo(?:\s+\+[^\s]+)?\s+"
        r"(?:build|run|test|check|clippy|install|publish|tree|metadata|audit|deny)\b[^\n;]*",
        normalized,
    )
    packages: set[str] = set()
    manifests: set[str] = set()
    for command in commands:
        packages.update(
            re.findall(
                r"(?:^|\s)(?:-p|--package)(?:=|\s+)([A-Za-z0-9_.-]+)(?=\s|$)",
                command,
            )
        )
        manifests.update(re.findall(r"--manifest-path(?:=|\s+)([^\s'\"\\]+)", command))
    binaries = set(re.findall(r"(?:target/release/|/opt/cex/bin/)([A-Za-z0-9_.-]+)", text))
    return packages, manifests, binaries
