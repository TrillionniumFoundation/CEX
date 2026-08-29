#!/usr/bin/env python3
"""Strictly verify the checked-in Hepta CycloneDX runtime binding."""

import argparse
import hashlib
import json
import pathlib
import re
import urllib.parse


RUNTIME_PATH = "/usr/local/bin/hepta-research-league"
ROOT_NAME = "hepta-research-league"
ROOT_VERSION = "0.1.0"
ROOT_IDENTITY = (
    "hepta-research-league@0.1.0|"
    "workspace:services/hepta-research-league/Cargo.toml"
)
ROOT_REF = "urn:cdx:cargo:" + hashlib.sha256(ROOT_IDENTITY.encode()).hexdigest()


def digest(path: pathlib.Path) -> str:
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"expected a regular non-symlink input: {path}")
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sbom", required=True, type=pathlib.Path)
    parser.add_argument("--runtime-sha256", required=True)
    parser.add_argument("--dockerfile", required=True, type=pathlib.Path)
    parser.add_argument("--cargo-lock", required=True, type=pathlib.Path)
    parser.add_argument("--rust-toolchain", required=True, type=pathlib.Path)
    args = parser.parse_args()

    try:
        if not re.fullmatch(r"[0-9a-f]{64}", args.runtime_sha256):
            raise ValueError("runtime SHA-256 is not canonical")
        if args.sbom.is_symlink() or not args.sbom.is_file():
            raise ValueError("SBOM must be a regular non-symlink file")
        expected_properties = [
            {
                "name": "trnm:cargo-lock:sha256",
                "value": "sha256:" + digest(args.cargo_lock),
            },
            {
                "name": "trnm:dockerfile:sha256",
                "value": "sha256:" + digest(args.dockerfile),
            },
            {
                "name": "trnm:rust-toolchain:sha256",
                "value": "sha256:" + digest(args.rust_toolchain),
            },
        ]
        document = json.loads(args.sbom.read_text(encoding="utf-8"))
    except (OSError, ValueError, json.JSONDecodeError) as error:
        parser.error(str(error))

    if not isinstance(document, dict) or set(document) != {
        "bomFormat",
        "specVersion",
        "version",
        "metadata",
        "components",
        "dependencies",
    }:
        parser.error("SBOM top-level document shape is not canonical")
    if document.get("bomFormat") != "CycloneDX":
        parser.error("SBOM bomFormat is not CycloneDX")
    if document.get("specVersion") != "1.5" or document.get("version") != 1:
        parser.error("SBOM version is not deterministic CycloneDX 1.5")
    if "serialNumber" in document or "timestamp" in document.get("metadata", {}):
        parser.error("SBOM must not contain a serial number or timestamp")
    metadata = document.get("metadata")
    if not isinstance(metadata, dict) or set(metadata) != {
        "tools",
        "component",
        "properties",
    }:
        parser.error("SBOM metadata shape is not canonical")
    if metadata.get("tools") != {
        "components": [
            {
                "type": "application",
                "name": "generate-hepta-research-league-sbom.py",
                "version": "2",
            }
        ]
    }:
        parser.error("SBOM generator identity is not canonical")
    if metadata.get("properties") != expected_properties:
        parser.error("SBOM metadata properties differ from the exact release-input set")
    component = metadata.get("component")
    expected_root = {
        "type": "application",
        "bom-ref": ROOT_REF,
        "name": ROOT_NAME,
        "version": ROOT_VERSION,
        "purl": f"pkg:cargo/{ROOT_NAME}@{ROOT_VERSION}",
        "licenses": [{"expression": "MIT"}],
    }
    if component != expected_root:
        parser.error("SBOM root component is not Hepta Research League")
    components = document.get("components")
    if not isinstance(components, list) or len(components) < 2:
        parser.error("SBOM dependency component closure is missing")
    if not all(isinstance(item, dict) for item in components):
        parser.error("SBOM components must be objects")
    files = [item for item in components if item.get("type") == "file"]
    expected_file = {
        "type": "file",
        "bom-ref": f"urn:cdx:file:sha256:{args.runtime_sha256}",
        "name": RUNTIME_PATH,
        "hashes": [{"alg": "SHA-256", "content": args.runtime_sha256}],
    }
    if files != [expected_file]:
        parser.error("SBOM must bind exactly one canonical Hepta runtime file")
    libraries = [item for item in components if item.get("type") == "library"]
    if len(libraries) + 1 != len(components) or components[-1] != expected_file:
        parser.error("SBOM component types/order are not canonical")
    library_refs = []
    for library in libraries:
        required_keys = {"type", "bom-ref", "name", "version", "purl"}
        if set(library) not in (required_keys, required_keys | {"licenses"}):
            parser.error("SBOM library component shape is not canonical")
        reference = library.get("bom-ref")
        name = library.get("name")
        version = library.get("version")
        if not isinstance(reference, str) or not re.fullmatch(
            r"urn:cdx:cargo:[0-9a-f]{64}", reference
        ):
            parser.error("SBOM library bom-ref is not canonical")
        if not isinstance(name, str) or not name or not isinstance(version, str) or not version:
            parser.error("SBOM library identity is missing")
        expected_purl = (
            f"pkg:cargo/{urllib.parse.quote(name)}@{urllib.parse.quote(version)}"
        )
        if library.get("purl") != expected_purl:
            parser.error("SBOM library purl is not canonical")
        if "licenses" in library:
            licenses = library["licenses"]
            if (
                not isinstance(licenses, list)
                or len(licenses) != 1
                or not isinstance(licenses[0], dict)
                or set(licenses[0]) != {"expression"}
                or not isinstance(licenses[0]["expression"], str)
                or not licenses[0]["expression"]
            ):
                parser.error("SBOM library license expression is not canonical")
        library_refs.append(reference)
    if library_refs != sorted(library_refs) or len(set(library_refs)) != len(library_refs):
        parser.error("SBOM library components are not uniquely sorted")

    dependencies = document.get("dependencies")
    if not isinstance(dependencies, list) or len(dependencies) < 2:
        parser.error("SBOM dependency graph is missing")
    expected_refs = {ROOT_REF, *library_refs}
    dependency_refs = []
    adjacency = {}
    for dependency in dependencies:
        if not isinstance(dependency, dict) or set(dependency) != {"ref", "dependsOn"}:
            parser.error("SBOM dependency entry shape is not canonical")
        reference = dependency["ref"]
        direct = dependency["dependsOn"]
        if (
            not isinstance(reference, str)
            or not isinstance(direct, list)
            or not all(isinstance(item, str) for item in direct)
            or direct != sorted(set(direct))
        ):
            parser.error("SBOM dependency entry is not uniquely sorted")
        if not set(direct).issubset(expected_refs):
            parser.error("SBOM dependency points outside the component closure")
        dependency_refs.append(reference)
        adjacency[reference] = direct
    if (
        dependency_refs != sorted(dependency_refs)
        or len(dependency_refs) != len(set(dependency_refs))
        or set(dependency_refs) != expected_refs
    ):
        parser.error("SBOM dependency graph does not exactly cover its components")

    visited = set()
    visiting = set()

    def visit(reference: str) -> None:
        if reference in visiting:
            parser.error("SBOM dependency graph contains a cycle")
        if reference in visited:
            return
        visiting.add(reference)
        for direct in adjacency[reference]:
            visit(direct)
        visiting.remove(reference)
        visited.add(reference)

    visit(ROOT_REF)
    if visited != expected_refs:
        parser.error("SBOM dependency graph is not rooted at Hepta")


if __name__ == "__main__":
    main()
