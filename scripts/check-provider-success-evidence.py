#!/usr/bin/env python3
"""Fail-closed static gate for durable provider terminal-success evidence."""

from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROBLEMS: list[str] = []


def read(relative: str) -> str:
    path = ROOT / relative
    if not path.is_file():
        PROBLEMS.append(f"missing required file: {relative}")
        return ""
    try:
        return path.read_text(encoding="utf-8")
    except UnicodeDecodeError as error:
        PROBLEMS.append(f"required UTF-8 file cannot be decoded: {relative}: {error}")
        return ""


def require(relative: str, *markers: str) -> str:
    content = read(relative)
    for marker in markers:
        if marker not in content:
            PROBLEMS.append(f"{relative} lacks required marker: {marker}")
    return content


provider_dispatch = require(
    "services/execution-service/src/provider_dispatch.rs",
    "validate_provider_success_evidence",
    "ProviderSuccessEvidenceError",
    "provider_success_target_mismatch",
    "provider_success_payload_identity_mismatch",
    "provider_success_not_terminal",
    "provider_success_done_missing",
    "provider_success_done_invalid",
    "provider_success_model_missing",
    "provider_success_output_missing",
    "provider_success_requires_explicit_terminal_done_true",
    "provider_success_binds_identity_model_and_output",
)
providers = require(
    "services/execution-service/src/providers.rs",
    "struct OllamaGenerateResponse",
    "done: Option<bool>",
    '"done": parsed.done',
)
workflow = require(
    ".github/workflows/p0-provider-reconciliation-gate.yml",
    "python3 scripts/check-provider-success-evidence.py",
    "bash scripts/check-provider-reconciliation-postgres.sh",
)

if provider_dispatch:
    validate_call = provider_dispatch.find(
        "validate_provider_success_evidence(&command.provider_target, &output)"
    )
    success_persist = provider_dispatch.find(
        'worker_id,\n                "succeeded",\n                Some(output.result_payload)'
    )
    if min(validate_call, success_persist) < 0 or validate_call >= success_persist:
        PROBLEMS.append(
            "durable provider success must be validated before the succeeded outcome is persisted"
        )

    if re.search(
        r'Ok\(output\)\s*=>\s*\{\s*finish_command\([^)]*"succeeded"',
        provider_dispatch,
        re.DOTALL,
    ):
        PROBLEMS.append(
            "provider dispatch contains an unvalidated direct success-persistence branch"
        )

    required_terminal_shape = (
        'match payload.get("done")' in provider_dispatch
        and "Some(Value::Bool(true))" in provider_dispatch
        and '"reconcile_required"' in provider_dispatch
    )
    if not required_terminal_shape:
        PROBLEMS.append(
            "provider success evidence must require an explicit terminal JSON boolean and reconcile otherwise"
        )

if providers and "done: Option<bool>" not in providers:
    PROBLEMS.append("Ollama adapter response shape drifted; update the terminal-evidence contract")

result = {
    "schema": "cex.provider-success-evidence-static.v1",
    "status": "failed" if PROBLEMS else "ok",
    "terminal_success_inference_allowed": False,
    "required_terminal_field": "done=true",
    "invalid_success_outcome": "reconcile_required",
    "workflow_bound": "scripts/check-provider-success-evidence.py" in workflow,
    "problems": PROBLEMS,
}
print(json.dumps(result, ensure_ascii=False, indent=2))
raise SystemExit(1 if PROBLEMS else 0)
