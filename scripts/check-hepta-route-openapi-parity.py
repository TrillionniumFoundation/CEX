#!/usr/bin/env python3
import pathlib
import re
import sys

import yaml


ROOT = pathlib.Path(__file__).resolve().parents[1]
SOURCE = ROOT / "services/hepta-research-league/src"


def normalize(path: str) -> str:
    return re.sub(r":([A-Za-z_][A-Za-z0-9_]*)", r"{\1}", path)


implemented = set()
for source in SOURCE.rglob("*.rs"):
    text = source.read_text(encoding="utf-8")
    implemented.update(
        normalize(path)
        for path in re.findall(r'\.route\(\s*"([^"]+)"', text, flags=re.DOTALL)
    )

contracts = (
    (
        ROOT / "docs/openapi/hepta-research-league-v1.yaml",
        "/v1/hepta/",
        {"/health", "/ready", "/metrics"},
    ),
    (
        ROOT / "docs/openapi/hepta-paper-raid-v2.yaml",
        "/v2/hepta/",
        {"/ready"},
    ),
)

failures = []
for contract, prefix, shared in contracts:
    document = yaml.safe_load(contract.read_text(encoding="utf-8"))
    documented = set(document.get("paths", {}))
    expected = {path for path in implemented if path.startswith(prefix)} | shared
    missing = sorted(expected - documented)
    stale = sorted(documented - expected)
    operation_ids = []
    for path_item in document.get("paths", {}).values():
        for method, operation in path_item.items():
            if method.lower() in {"get", "post", "put", "patch", "delete"}:
                operation_ids.append(operation.get("operationId"))
    duplicate_ids = sorted(
        operation_id
        for operation_id in set(operation_ids)
        if operation_id is None or operation_ids.count(operation_id) > 1
    )
    if missing or stale or duplicate_ids:
        failures.append(
            f"{contract.relative_to(ROOT)}: missing={missing} stale={stale} "
            f"duplicate_or_missing_operation_ids={duplicate_ids}"
        )

if failures:
    print("\n".join(failures), file=sys.stderr)
    raise SystemExit(1)

print("Hepta route/OpenAPI path and operationId parity: PASS")
