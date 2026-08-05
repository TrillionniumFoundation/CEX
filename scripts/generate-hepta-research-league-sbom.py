#!/usr/bin/env python3
"""Generate the deterministic CycloneDX 1.5 Hepta runtime SBOM."""

import argparse
import hashlib
import json
import os
import pathlib
import re
import stat
import urllib.parse


RUNTIME_BINARY_PATH = "/usr/local/bin/hepta-research-league"
PROPERTY_PATHS = (
    ("trnm:cargo-lock:sha256", "cargo_lock"),
    ("trnm:dockerfile:sha256", "dockerfile"),
    ("trnm:rust-toolchain:sha256", "rust_toolchain"),
)


def sha256_path(path: pathlib.Path) -> str:
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"SBOM input must be a regular non-symlink file: {path}")
    return hashlib.sha256(path.read_bytes()).hexdigest()


def normalized_identity(package, workspace_root):
    source = package.get("source")
    if source:
        origin = source
    else:
        manifest = pathlib.Path(package["manifest_path"])
        origin = "workspace:" + manifest.relative_to(workspace_root).as_posix()
    return f'{package["name"]}@{package["version"]}|{origin}'


def bom_ref(package, workspace_root):
    digest = hashlib.sha256(
        normalized_identity(package, workspace_root).encode()
    ).hexdigest()
    return f"urn:cdx:cargo:{digest}"


def component(package, workspace_root, component_type):
    name = package["name"]
    version = package["version"]
    result = {
        "type": component_type,
        "bom-ref": bom_ref(package, workspace_root),
        "name": name,
        "version": version,
        "purl": f"pkg:cargo/{urllib.parse.quote(name)}@{urllib.parse.quote(version)}",
    }
    if package.get("license"):
        result["licenses"] = [{"expression": package["license"]}]
    return result


def runtime_digest(args: argparse.Namespace) -> str:
    if args.runtime_binary is not None:
        path = args.runtime_binary
        mode = path.lstat().st_mode if path.exists() or path.is_symlink() else 0
        if (
            path.is_symlink()
            or not stat.S_ISREG(mode)
            or not os.access(path, os.X_OK)
        ):
            raise ValueError(
                f"runtime binary must be a regular non-symlink executable: {path}"
            )
        return hashlib.sha256(path.read_bytes()).hexdigest()
    if not re.fullmatch(r"[0-9a-f]{64}", args.runtime_sha256):
        raise ValueError("runtime binary SHA-256 is not canonical")
    return args.runtime_sha256


def runtime_file_component(digest):
    return {
        "type": "file",
        "bom-ref": f"urn:cdx:file:sha256:{digest}",
        "name": RUNTIME_BINARY_PATH,
        "hashes": [{"alg": "SHA-256", "content": digest}],
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--metadata", required=True, type=pathlib.Path)
    runtime = parser.add_mutually_exclusive_group(required=True)
    runtime.add_argument("--runtime-binary", type=pathlib.Path)
    runtime.add_argument("--runtime-sha256")
    parser.add_argument("--dockerfile", required=True, type=pathlib.Path)
    parser.add_argument("--cargo-lock", required=True, type=pathlib.Path)
    parser.add_argument("--rust-toolchain", required=True, type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    args = parser.parse_args()

    try:
        digest = runtime_digest(args)
        input_hashes = {
            name: sha256_path(getattr(args, argument))
            for name, argument in PROPERTY_PATHS
        }
        if args.metadata.is_symlink() or not args.metadata.is_file():
            raise ValueError("Cargo metadata must be a regular non-symlink file")
        if args.output.is_symlink():
            raise ValueError("SBOM output must not be a symlink")
    except (OSError, ValueError) as error:
        parser.error(str(error))

    metadata = json.loads(args.metadata.read_text(encoding="utf-8"))
    workspace_root = pathlib.Path(metadata["workspace_root"])
    packages_by_id = {package["id"]: package for package in metadata["packages"]}
    nodes_by_id = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    roots = [
        package["id"]
        for package in metadata["packages"]
        if package["name"] == "hepta-research-league"
        and package["id"] in set(metadata["workspace_members"])
    ]
    if len(roots) != 1:
        parser.error("Cargo metadata must contain exactly one workspace Hepta package")
    root_id = roots[0]
    closure = set()
    pending = [root_id]
    while pending:
        package_id = pending.pop()
        if package_id in closure:
            continue
        if package_id not in packages_by_id or package_id not in nodes_by_id:
            parser.error(f"Cargo metadata dependency node is missing: {package_id}")
        closure.add(package_id)
        pending.extend(dependency["pkg"] for dependency in nodes_by_id[package_id]["deps"])

    root_package = packages_by_id[root_id]
    components = [
        component(packages_by_id[package_id], workspace_root, "library")
        for package_id in sorted(
            closure - {root_id},
            key=lambda item: bom_ref(packages_by_id[item], workspace_root),
        )
    ]
    components.append(runtime_file_component(digest))
    dependencies = []
    for package_id in sorted(
        closure,
        key=lambda item: bom_ref(packages_by_id[item], workspace_root),
    ):
        direct = sorted(
            {
                bom_ref(packages_by_id[dependency["pkg"]], workspace_root)
                for dependency in nodes_by_id[package_id]["deps"]
                if dependency["pkg"] in closure
            }
        )
        dependencies.append(
            {"ref": bom_ref(packages_by_id[package_id], workspace_root), "dependsOn": direct}
        )

    document = {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": {
            "tools": {
                "components": [
                    {
                        "type": "application",
                        "name": "generate-hepta-research-league-sbom.py",
                        "version": "2",
                    }
                ]
            },
            "component": component(root_package, workspace_root, "application"),
            "properties": [
                {"name": name, "value": "sha256:" + input_hashes[name]}
                for name, _ in PROPERTY_PATHS
            ],
        },
        "components": components,
        "dependencies": dependencies,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(document, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
