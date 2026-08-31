#!/usr/bin/env python3
"""Fail closed on Gateway exact runtime-profile and principal authority drift."""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
API_PATH = Path("services/gateway-service/src/bin/gateway-exact-reserve-api.rs")
WORKER_PATH = Path("services/gateway-service/src/bin/gateway-exact-reserve-worker.rs")
PROBLEMS: list[str] = []


def read(path: Path) -> str:
    absolute = ROOT / path
    if not absolute.is_file():
        PROBLEMS.append(f"missing required file: {path}")
        return ""
    try:
        return absolute.read_text(encoding="utf-8")
    except UnicodeDecodeError as error:
        PROBLEMS.append(f"required UTF-8 file cannot be decoded: {path}: {error}")
        return ""


def require(path: Path, content: str, *markers: str) -> None:
    for marker in markers:
        if marker not in content:
            PROBLEMS.append(f"{path} lacks required authority marker: {marker}")


api = read(API_PATH)
worker = read(WORKER_PATH)

require(
    API_PATH,
    api,
    "use shared_config::runtime_guard;",
    "runtime_guard::resolve_runtime_profile()",
    'const EXACT_INGRESS_PRINCIPAL: &str = "gateway-exact-ingress";',
    "AuthenticationError::ServiceIdMismatch",
    'headers.get("x-cex-service-id")',
    "constant_time_eq(principal.as_bytes(), EXACT_INGRESS_PRINCIPAL.as_bytes())",
    "Ok(EXACT_INGRESS_PRINCIPAL.to_string())",
    "exact_ingress_principal_cannot_be_relabelled_by_a_header",
)
require(
    WORKER_PATH,
    worker,
    "runtime_guard",
    "runtime_guard::resolve_runtime_profile()",
    "WorkerError::Config(error.to_string())",
)

for path, content in ((API_PATH, api), (WORKER_PATH, worker)):
    for forbidden in (
        "enum RuntimeProfile",
        "fn resolve_profile()",
        '"production" | "prod" => Ok(Self::Production)',
    ):
        if forbidden in content:
            PROBLEMS.append(
                f"{path} retains a private runtime-profile authority: {forbidden}"
            )

header_offset = api.find('headers.get("x-cex-service-id")')
comparison_offset = api.find(
    "constant_time_eq(principal.as_bytes(), EXACT_INGRESS_PRINCIPAL.as_bytes())"
)
return_offset = api.find("Ok(EXACT_INGRESS_PRINCIPAL.to_string())")
if min(header_offset, comparison_offset, return_offset) < 0 or not (
    header_offset < comparison_offset < return_offset
):
    PROBLEMS.append(
        "Gateway exact ingress must validate any service-id header against the credential-bound principal before returning the immutable Audit actor"
    )

if '.unwrap_or("gateway-exact-ingress")' in api:
    PROBLEMS.append(
        "Gateway exact ingress still treats caller-controlled service identity as an optional label"
    )

result = {
    "status": "failed" if PROBLEMS else "ok",
    "shared_runtime_profile_authority": not PROBLEMS,
    "credential_bound_principal": not PROBLEMS,
    "exact_ingress_principal": "gateway-exact-ingress",
    "problems": PROBLEMS,
}
print(json.dumps(result, ensure_ascii=False, indent=2))
raise SystemExit(1 if PROBLEMS else 0)
