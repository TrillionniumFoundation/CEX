"""Fail-closed all-feature advisory reachability closure for CEX release candidates."""
from __future__ import annotations

import json
import re
import tempfile
from pathlib import Path
from typing import Any

from rust_advisory_common_v5 import EXPECTED_ADVISORIES, LOCK_PATH, POLICY_PATH, ROOT, PolicyError, read_json, require, run

FEATURE_POLICY_PATH = ROOT / "docs/security/rust-feature-closure-v1.json"
TREE_EDGES = "normal,build"


def package_pattern(name: str, version: str) -> re.Pattern[str]:
    return re.compile(rf"(?m)(?:^|[\s│├└─]){re.escape(name)} v{re.escape(version)}(?:\s|$|\s\()")


def all_feature_tree(package: str, *, cwd: Path = ROOT) -> str:
    return run(
        "cargo", "tree", "--locked", "--target", "all", "--all-features",
        "-p", package, "-e", TREE_EDGES, cwd=cwd,
    )


def all_feature_inverse(spec: str, *, cwd: Path = ROOT) -> str:
    return run(
        "cargo", "tree", "--locked", "--target", "all", "--all-features",
        "-i", spec, "-e", TREE_EDGES, cwd=cwd,
    )


def workspace_packages() -> list[str]:
    metadata = json.loads(run("cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"))
    members = set(metadata.get("workspace_members", []))
    packages = sorted(
        item["name"] for item in metadata.get("packages", [])
        if item.get("id") in members
    )
    require(bool(packages), "workspace package inventory is empty")
    return packages


def validate_policy() -> tuple[dict[str, Any], dict[str, Any]]:
    feature = read_json(FEATURE_POLICY_PATH)
    require(feature.get("schema") == "cex.rust-feature-closure.v1", "feature closure schema drift")
    require(feature.get("status") == "active_fail_closed", "feature closure must remain active")
    require(feature.get("resolution_strategy") == "all_workspace_packages_all_features_superset", "feature resolution strategy narrowed")
    require(feature.get("target_scope") == "all", "feature target scope narrowed")
    require(feature.get("execution_edges") == "normal_and_build", "feature execution edges narrowed")
    require(feature.get("command_failure") == "fatal_including_empty_output", "feature command failure policy weakened")
    require(feature.get("approval_requirement") == "fresh_exact_head_github_review", "feature approval requirement weakened")
    require(feature.get("production_authorization") == "not_granted", "feature policy cannot authorize production")
    expected = feature.get("expected_workspace_all_features_execution_reachability")
    require(isinstance(expected, dict) and set(expected) == set(EXPECTED_ADVISORIES), "feature advisory set drift")
    for advisory_id, packages in expected.items():
        require(isinstance(packages, list) and packages == sorted(set(packages)), f"invalid feature reachability list: {advisory_id}")
    baseline = read_json(POLICY_PATH)
    return feature, baseline


def validate_all_feature_reachability() -> dict[str, Any]:
    feature, baseline = validate_policy()
    exceptions = baseline.get("exceptions")
    require(isinstance(exceptions, list), "baseline advisory exceptions missing")
    entries = {item.get("advisory_id"): item for item in exceptions if isinstance(item, dict)}
    require(set(entries) == set(EXPECTED_ADVISORIES), "baseline advisory exception set drift")
    packages = workspace_packages()
    evidence: list[dict[str, Any]] = []
    for advisory_id, (name, version) in sorted(EXPECTED_ADVISORIES.items()):
        entry = entries[advisory_id]
        spec = f"{name}@{version}"
        inverse = all_feature_inverse(spec)
        if entry.get("execution_graph_must_be_empty"):
            require(not inverse, f"{advisory_id} became all-features normal+build reachable")
        pattern = package_pattern(name, version)
        reached: list[str] = []
        paths: dict[str, str] = {}
        for package in packages:
            tree = all_feature_tree(package)
            if pattern.search(tree):
                reached.append(package)
                paths[package] = tree
        expected = feature["expected_workspace_all_features_execution_reachability"][advisory_id]
        require(reached == expected, f"{advisory_id} all-feature workspace reachability drift: expected={expected} actual={reached}")
        for package in reached:
            markers = entry.get("required_workspace_path_markers", {}).get(package, [])
            for marker in markers:
                require(marker in paths[package], f"{advisory_id}/{package} all-feature path marker missing: {marker}")
        evidence.append({
            "advisory_id": advisory_id,
            "package_spec": spec,
            "workspace_all_features_execution_reachability": reached,
            "inverse_graph_empty": not inverse,
        })
    return {
        "schema": "cex.rust-feature-closure-check.v1",
        "status": "all_workspace_all_features_all_targets_normal_build_reachability_valid",
        "workspace_package_count": len(packages),
        "advisories": evidence,
        "production_authorization": "not_granted",
    }


def write_package(root: Path, relative: str, cargo: str, source: str = "pub fn marker() {}\n") -> None:
    package = root / relative
    (package / "src").mkdir(parents=True, exist_ok=True)
    (package / "Cargo.toml").write_text(cargo, encoding="utf-8")
    (package / "src/lib.rs").write_text(source, encoding="utf-8")


def hostile_self_tests() -> dict[str, bool]:
    results: dict[str, bool] = {}
    with tempfile.TemporaryDirectory(prefix="cex-feature-hostile-") as temp:
        root = Path(temp)
        (root / "Cargo.toml").write_text(
            "[workspace]\nmembers=['app','ignored','build-carrier','proc-carrier','target-carrier']\nresolver='2'\n",
            encoding="utf-8",
        )
        write_package(root, "ignored", "[package]\nname='fixture-ignored-advisory'\nversion='0.1.0'\nedition='2021'\n")
        write_package(
            root, "build-carrier",
            "[package]\nname='fixture-build-carrier'\nversion='0.1.0'\nedition='2021'\nbuild='build.rs'\n[build-dependencies]\nfixture-ignored-advisory={path='../ignored'}\n",
        )
        (root / "build-carrier/build.rs").write_text("fn main() {}\n", encoding="utf-8")
        write_package(
            root, "proc-carrier",
            "[package]\nname='fixture-proc-carrier'\nversion='0.1.0'\nedition='2021'\n[lib]\nproc-macro=true\n[dependencies]\nfixture-ignored-advisory={path='../ignored'}\n",
            "extern crate proc_macro;\nuse proc_macro::TokenStream;\n#[proc_macro] pub fn fixture(input: TokenStream) -> TokenStream { input }\n",
        )
        write_package(
            root, "target-carrier",
            "[package]\nname='fixture-target-carrier'\nversion='0.1.0'\nedition='2021'\n[target.'cfg(unix)'.dependencies]\nfixture-ignored-advisory={path='../ignored'}\n",
        )
        write_package(
            root, "app",
            "[package]\nname='fixture-app'\nversion='0.1.0'\nedition='2021'\n"
            "[features]\ndefault=[]\noptional-normal=['dep:fixture-ignored-advisory']\noptional-build=['dep:fixture-build-carrier']\noptional-proc=['dep:fixture-proc-carrier']\noptional-target=['dep:fixture-target-carrier']\n"
            "[dependencies]\nfixture-ignored-advisory={path='../ignored',optional=true}\nfixture-build-carrier={path='../build-carrier',optional=true}\nfixture-proc-carrier={path='../proc-carrier',optional=true}\nfixture-target-carrier={path='../target-carrier',optional=true}\n",
        )
        run("cargo", "generate-lockfile", "--offline", cwd=root)
        default_tree = run("cargo", "tree", "--locked", "--target", "all", "-p", "fixture-app", "-e", TREE_EDGES, cwd=root)
        require("fixture-ignored-advisory v0.1.0" not in default_tree, "hostile fixture default graph unexpectedly contains ignored advisory")
        all_tree = run("cargo", "tree", "--locked", "--target", "all", "--all-features", "-p", "fixture-app", "-e", TREE_EDGES, cwd=root)
        require("fixture-ignored-advisory v0.1.0" in all_tree, "all-features failed to expose optional advisory")
        results["optional_dependency_rejected"] = True
        explicit_tree = run(
            "cargo", "tree", "--locked", "--target", "all", "--no-default-features", "--features", "optional-normal",
            "-p", "fixture-app", "-e", TREE_EDGES, cwd=root,
        )
        require("fixture-ignored-advisory v0.1.0" in explicit_tree, "no-default explicit feature failed to expose advisory")
        results["no_default_explicit_feature_rejected"] = True
        for label, marker in (
            ("build_dependency_rejected", "fixture-build-carrier v0.1.0"),
            ("proc_macro_dependency_rejected", "fixture-proc-carrier v0.1.0"),
            ("target_specific_dependency_rejected", "fixture-target-carrier v0.1.0"),
        ):
            require(marker in all_tree and "fixture-ignored-advisory v0.1.0" in all_tree, f"hostile fixture escaped: {label}")
            results[label] = True
    required = set(read_json(FEATURE_POLICY_PATH).get("hostile_fixtures_required", []))
    observed = {
        "optional_dependency",
        "no_default_features_plus_explicit_feature",
        "build_dependency",
        "proc_macro_dependency",
        "target_specific_dependency",
    }
    require(required == observed, f"hostile fixture inventory drift: expected={sorted(required)} observed={sorted(observed)}")
    return results


def validate_feature_closure() -> dict[str, Any]:
    evidence = validate_all_feature_reachability()
    evidence["hostile_self_tests"] = hostile_self_tests()
    return evidence
