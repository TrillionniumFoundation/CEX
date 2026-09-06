#!/usr/bin/env python3
"""Validate fail-closed capability-service production startup."""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROBLEMS: list[str] = []


def read(relative: str) -> str:
    path = ROOT / relative
    if not path.is_file():
        PROBLEMS.append(f"missing required file: {relative}")
        return ""
    return path.read_text(encoding="utf-8")


runtime = read("services/capability-service/src/runtime.rs")
main = read("services/capability-service/src/main.rs")
module = read("services/capability-service/MODULE.md")
posture = read("docs/capability-production-posture-v1.md")

for marker in (
    "RuntimeProfile",
    "is_production_like",
    "CAPABILITY_STATIC_REGISTRY_JSON",
    "validate_strict_registry",
    "registry_required",
    "duplicate_capability_id",
    "development_capability_forbidden",
    "development_discovery_forbidden",
    "no_enabled_capability",
    "CAPABILITY_BIND_ADDR",
    "AppState::new_for_tests",
):
    if marker not in runtime:
        PROBLEMS.append(f"capability runtime lacks fail-closed marker: {marker}")

for marker in (
    "RuntimeConfig::from_env",
    "CONFIG_ERROR_EXIT_CODE",
    "runtime.bind_addr()",
    "runtime.build_state().await",
):
    if marker not in main:
        PROBLEMS.append(f"capability main lacks guarded startup marker: {marker}")

for forbidden in (
    'TcpListener::bind("127.0.0.1:7005")',
    "let state = AppState::from_env().await",
):
    if forbidden in main:
        PROBLEMS.append(f"capability main bypasses runtime guard: {forbidden}")

for marker in (
    "Production authorization: `not_granted`",
    "validated registry",
    "development-only",
):
    if marker not in module and marker not in posture:
        PROBLEMS.append(f"capability documentation lacks marker: {marker}")

result = {
    "schema": "cex.capability-production-posture-check.v1",
    "status": "failed" if PROBLEMS else "ok",
    "production_registry": "explicit_validated_static",
    "local_discovery_production_authority": False,
    "production_authorization": "not_granted",
    "problems": PROBLEMS,
}
print(json.dumps(result, indent=2, sort_keys=True))
sys.exit(1 if PROBLEMS else 0)
