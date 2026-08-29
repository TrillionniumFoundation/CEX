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
documents = {}
for contract, prefix, shared in contracts:
    document = yaml.safe_load(contract.read_text(encoding="utf-8"))
    documents[contract] = document
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

research_contract = contracts[0][0]
research_document = documents[research_contract]
missing_finality_description = (
    research_document["paths"]["/v1/hepta/trnm/finality/{command_id}"]["get"]
    ["responses"]["404"]["description"]
)
expected_missing_finality_description = (
    "No command-finality projection is available; "
    "pending versus unavailable is not inferred"
)
if missing_finality_description != expected_missing_finality_description:
    failures.append(
        f"{research_contract.relative_to(ROOT)}: missing finality projection "
        "must remain unknown rather than being rewritten as pending"
    )

workflows = (SOURCE / "workflows.rs").read_text(encoding="utf-8")
required_missing_finality_message = (
    "No TRNM finality projection is available for command {command_id}; "
    "pending versus unavailable is not inferred"
)
if required_missing_finality_message not in workflows:
    failures.append(
        "services/hepta-research-league/src/workflows.rs: missing finality "
        "projection does not use the canonical unknown-state message"
    )
if "TRNM command {command_id} is still pending finality" in workflows:
    failures.append(
        "services/hepta-research-league/src/workflows.rs: missing projection "
        "still manufactures pending finality"
    )

service_readme = (
    ROOT / "services/hepta-research-league/README.md"
).read_text(encoding="utf-8")
for required in (
    "A queued authoritative command is\n`pending_finality`",
    "an absent projection is `unknown_finality`",
    "conflicting\nauthority is `unavailable_finality`",
    "an upstream or malformed projection is\n`error_finality`",
    "must never collapse to `pending_finality`",
):
    if required not in service_readme:
        failures.append(
            "services/hepta-research-league/README.md: finality availability "
            f"taxonomy is missing {required!r}"
        )

if failures:
    print("\n".join(failures), file=sys.stderr)
    raise SystemExit(1)

print("Hepta route/OpenAPI path and operationId parity: PASS")
