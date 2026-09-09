#!/usr/bin/env python3
"""Reject local model/provider activation from canonical CEX environment examples."""
from __future__ import annotations

import json
from pathlib import Path
import stat
import sys

ROOT = Path(__file__).resolve().parents[1]
FILES = (".env.example", ".env.production.example")
FORBIDDEN = (
    "OLLAMA_BASE_URL=",
    "OPENCLAW_STATE_DIR=",
    "OPENCLAW_CONFIG_PATH=",
    "OPENCLAW_AGENT_DIR=",
    "CAPABILITY_OPENCLAW_MODELS_JSON_PATH=",
    "CEX_PROVIDER_PROBE_MODEL=",
    "CEX_PROVIDER_PROBE_TIMEOUT_SECONDS=",
    "EXECUTION_PROVIDER_DISPATCH_TIMEOUT_SECONDS=",
    "CEX_ENABLE_QUEUED_WORKER=",
    "CEX_ENABLE_LEGACY_LOCAL_PROVIDER_DISPATCH=true",
    "--features legacy-local-provider-dispatch",
)


def read(relative: str) -> str:
    path = ROOT / relative
    info = path.lstat()
    if path.is_symlink() or not stat.S_ISREG(info.st_mode) or info.st_size > 1_048_576:
        raise ValueError(f"invalid_environment_example:{relative}")
    return path.read_text(encoding="utf-8")


def main() -> int:
    problems: list[str] = []
    try:
        sources = {path: read(path) for path in FILES}
    except (OSError, UnicodeError, ValueError) as error:
        problems.append(str(error).split(":", 1)[0])
        sources = {}

    for path, text in sources.items():
        for marker in FORBIDDEN:
            if marker in text:
                problems.append(f"{path}:forbidden_local_provider_marker:{marker}")
        if "CAPABILITY_EXTERNAL_AGENT_REGISTRY_JSON=" not in text:
            problems.append(f"{path}:external_agent_registry_missing")
        if "external-Agent" not in text and "external Agent" not in text:
            problems.append(f"{path}:external_agent_boundary_not_documented")

    production = sources.get(".env.production.example", "")
    for marker in (
        "MATRIX_ENTRY_ADAPTER_BASE_URL=https://",
        "MATRIX_RECONCILIATION_DATABASE_URL=postgresql://",
        "CEX_ENABLE_LEGACY_LOCAL_PROVIDER_DISPATCH",
    ):
        if marker not in production:
            problems.append(f".env.production.example:missing_fail_closed_marker:{marker}")

    result = {
        "schema": "cex.external-agent-config-boundary-check.v1",
        "status": "failed" if problems else "ok",
        "files": list(FILES),
        "runtime_policy": "external_only",
        "checker_may_grant_production_authorization": False,
        "production_authorization": "not_granted",
        "problems": problems,
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
