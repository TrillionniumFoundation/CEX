#!/usr/bin/env python3
"""Generate the deterministic CycloneDX 1.5 Paper Raid application SBOM."""

import argparse
import hashlib
import json
import pathlib
import urllib.parse


RUNTIME_BINARY_PATH = "/usr/local/bin/hepta-research-league"


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


def runtime_file_component(runtime_binary):
    digest = hashlib.sha256(runtime_binary.read_bytes()).hexdigest()
    return {
        "type": "file",
        "bom-ref": f"urn:cdx:file:sha256:{digest}",
        "name": RUNTIME_BINARY_PATH,
        "hashes": [{"alg": "SHA-256", "content": digest}],
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--metadata", required=True)
    parser.add_argument("--runtime-binary", required=True, type=pathlib.Path)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    if not args.runtime_binary.is_file():
        parser.error(f"runtime binary is not a regular file: {args.runtime_binary}")

    metadata = json.loads(pathlib.Path(args.metadata).read_text(encoding="utf-8"))
    workspace_root = pathlib.Path(metadata["workspace_root"])
    packages_by_id = {package["id"]: package for package in metadata["packages"]}
    nodes_by_id = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    root_id = next(
        package["id"]
        for package in metadata["packages"]
        if package["name"] == "hepta-research-league"
    )
    closure = set()
    pending = [root_id]
    while pending:
        package_id = pending.pop()
        if package_id in closure:
            continue
        closure.add(package_id)
        pending.extend(dependency["pkg"] for dependency in nodes_by_id[package_id]["deps"])

    root_package = packages_by_id[root_id]
    components = [
        component(packages_by_id[package_id], workspace_root, "library")
        for package_id in sorted(
            closure - {root_id},
            key=lambda item: normalized_identity(packages_by_id[item], workspace_root),
        )
    ]
    components.append(runtime_file_component(args.runtime_binary))
    dependencies = []
    for package_id in sorted(
        closure,
        key=lambda item: normalized_identity(packages_by_id[item], workspace_root),
    ):
        direct = sorted(
            {
                bom_ref(packages_by_id[dependency["pkg"]], workspace_root)
                for dependency in nodes_by_id[package_id]["deps"]
                if dependency["pkg"] in closure
            }
        )
        dependencies.append(
            {
                "ref": bom_ref(packages_by_id[package_id], workspace_root),
                "dependsOn": direct,
            }
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
                    }
                ]
            },
            "component": component(root_package, workspace_root, "application"),
        },
        "components": components,
        "dependencies": dependencies,
    }
    output = pathlib.Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(
        json.dumps(document, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
