#!/usr/bin/env python3
"""Fail-closed static gate for provider evidence and external-only execution.

Migration 0088 and its reconciliation tests remain authoritative for historical
provider-command evidence. Sequence 51 additionally requires that the runtime
cannot create new local Ollama/OpenClaw effects: the provider compatibility
surface must return one stable external-Agent-required error without reading or
copying prompt/provider response bytes.
"""

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
    "provider_success_model_mismatch",
    "provider_success_output_missing",
    "provider_success_requires_explicit_terminal_done_true",
    "provider_success_binds_identity_model_and_output",
)
terminal_migration = require(
    "migrations/0088_enforce_provider_terminal_evidence_binding.sql",
    "cex_provider_target_ref_v1",
    "cex_provider_result_sha256_v1",
    "cex_validate_provider_live_terminal_result_v1",
    "cex_validate_provider_reconciled_terminal_result_v1",
    "cex_validate_provider_dispatch_binding_v1",
    "cex_guard_provider_dispatch_command_contract_v2",
    "cex_guard_provider_terminal_evidence_v1",
    "live provider result model does not match immutable target",
    "same-attempt confirmed-executed evidence",
)
providers = require(
    "services/execution-service/src/providers.rs",
    'pub const RUNTIME_POLICY: &str = "external_only";',
    'pub const LEGACY_LOCAL_DISPATCH_STATUS: &str = "legacy_local_provider_dispatch_disabled";',
    "external_agent_runtime_required",
    "pub async fn dispatch_via_provider",
    "Err(ProviderDispatchError::external_agent_required())",
    "hepta_agent_protocol_v1",
)
execution_manifest = require(
    "services/execution-service/Cargo.toml",
    "default = []",
    "legacy-local-provider-dispatch = []",
    'required-features = ["legacy-local-provider-dispatch"]',
    'name = "external_agent_boundary"',
)
external_boundary_test = require(
    "services/execution-service/tests/external_agent_boundary.rs",
    "provider_compatibility_surface_fails_closed_without_network_or_prompt_echo",
    "external_agent_runtime_required",
    "TOP-SECRET-RESEARCH-PROMPT",
    "assert!(!error.message.contains(prompt))",
)
postgres_test = require(
    "scripts/check-provider-reconciliation-postgres.sh",
    "worker success model mismatch bypass was not rejected",
    "direct live success hash mismatch bypass was not rejected",
    "reconciliation success without same-attempt evidence was not rejected",
    "terminal provider result mutation was not rejected",
)
hosted_checker = require(
    "scripts/check-hosted-gate-execution-impl.py",
    "Provider terminal-success evidence contract",
)
exact_attempt_verifier = require(
    "scripts/verify-hosted-run-execution.py",
    "Provider terminal-success evidence contract",
)
workflow = require(
    ".github/workflows/p0-provider-reconciliation-gate.yml",
    "migrations/0088_enforce_provider_terminal_evidence_binding.sql",
    "python3 scripts/check-provider-success-evidence.py",
    "bash scripts/check-provider-reconciliation-postgres.sh",
)
architecture_checker = require(
    "scripts/check-external-agent-runtime-boundary.py",
    "services/execution-service/src/providers.rs runtime implementation",
    "external_agent_runtime_required",
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
            "historical provider success must be validated before a succeeded outcome is persisted"
        )

    if re.search(
        r'Ok\(output\)\s*=>\s*\{\s*finish_command\([^)]*"succeeded"',
        provider_dispatch,
        re.DOTALL,
    ):
        PROBLEMS.append(
            "provider reconciliation code contains an unvalidated direct success-persistence branch"
        )

    required_terminal_shape = (
        'match payload.get("done")' in provider_dispatch
        and "Some(Value::Bool(true))" in provider_dispatch
        and '"reconcile_required"' in provider_dispatch
    )
    if not required_terminal_shape:
        PROBLEMS.append(
            "historical provider evidence must require an explicit terminal JSON boolean and reconcile otherwise"
        )

    required_model_binding = (
        "let Some(reported_model)" in provider_dispatch
        and 'payload.get("model")' in provider_dispatch
        and "reported_model != expected_provider_ref" in provider_dispatch
    )
    if not required_model_binding:
        PROBLEMS.append(
            "historical remote provider model identity must equal the immutable provider reference"
        )

provider_runtime = providers.split("#[cfg(test)]", 1)[0]
for forbidden in (
    "OllamaProviderAdapter",
    "OpenClawCliProviderAdapter",
    "/api/generate",
    "Command::new",
    "response.text()",
    'args(["infer", "model", "run"',
):
    if forbidden in provider_runtime:
        PROBLEMS.append(
            f"external-only provider boundary retains executable local inference marker: {forbidden}"
        )
if "_input: &ProviderDispatchInput" not in provider_runtime:
    PROBLEMS.append("provider boundary must make prompt non-consumption explicit")
if "Err(ProviderDispatchError::external_agent_required())" not in provider_runtime:
    PROBLEMS.append("provider boundary must fail closed to the external Agent protocol")
if "--features legacy-local-provider-dispatch" in workflow or "--all-features" in workflow:
    PROBLEMS.append("authoritative provider workflow activates retired local provider execution")

result = {
    "schema": "cex.provider-success-evidence-static.v1",
    "status": "failed" if PROBLEMS else "ok",
    "runtime_policy": "external_only",
    "automatic_local_provider_dispatch_allowed": False,
    "historical_terminal_success_inference_allowed": False,
    "terminal_success_inference_allowed": False,
    "required_terminal_field": "done=true for retained historical live evidence",
    "required_model_binding": "historical remote model == immutable provider_ref",
    "database_guard_bound": (
        "cex_validate_provider_live_terminal_result_v1" in terminal_migration
        and "cex_validate_provider_reconciled_terminal_result_v1" in terminal_migration
    ),
    "invalid_success_outcome": "reconcile_required",
    "default_external_agent_test_bound": bool(external_boundary_test),
    "legacy_worker_feature_isolated": bool(execution_manifest),
    "workflow_bound": "scripts/check-provider-success-evidence.py" in workflow,
    "postgres_negative_paths_bound": bool(postgres_test),
    "architecture_checker_bound": bool(architecture_checker),
    "hosted_step_attested": bool(hosted_checker) and bool(exact_attempt_verifier),
    "checker_may_grant_production_authorization": False,
    "production_authorization": "not_granted",
    "problems": PROBLEMS,
}
print(json.dumps(result, ensure_ascii=False, indent=2))
raise SystemExit(1 if PROBLEMS else 0)
