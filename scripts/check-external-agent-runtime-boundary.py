#!/usr/bin/env python3
"""Validate ADR-004 external-Agent-only runtime under Sequence 54 integration.

Sequence 52 remains the immutable architecture-baseline identity. Sequence 54 is
the repository candidate that non-regressively integrates that architecture
with later functional and security controls. This checker may reject a source
tree; it cannot grant production authorization.
"""
from __future__ import annotations

import json
import re
import sys
import tomllib
from pathlib import Path
from typing import Any, Iterable

ROOT = Path(__file__).resolve().parents[1]
TRACEABILITY_PATH = "docs/traceability/sequence-52-architecture-v1.json"
PROBLEMS: list[str] = []


def read(relative: str) -> str:
    path = ROOT / relative
    if not path.is_file():
        PROBLEMS.append(f"missing required file: {relative}")
        return ""
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as error:
        PROBLEMS.append(f"cannot read required UTF-8 file {relative}: {error}")
        return ""


def load_json(relative: str) -> dict[str, Any]:
    raw = read(relative)
    if not raw:
        return {}
    try:
        value = json.loads(raw)
    except json.JSONDecodeError as error:
        PROBLEMS.append(f"invalid JSON in {relative}: {error}")
        return {}
    if not isinstance(value, dict):
        PROBLEMS.append(f"{relative} root must be a JSON object")
        return {}
    return value


def require(relative: str, *markers: str) -> str:
    text = read(relative)
    for marker in markers:
        if marker not in text:
            PROBLEMS.append(f"{relative} lacks required marker: {marker}")
    return text


def forbid(relative: str, text: str, markers: Iterable[str]) -> None:
    for marker in markers:
        if marker in text:
            PROBLEMS.append(f"{relative} contains forbidden runtime marker: {marker}")


def require_path(value: object, label: str) -> None:
    if not isinstance(value, str) or not value:
        PROBLEMS.append(f"{label} must be a non-empty repository path")
        return
    path = Path(value)
    if path.is_absolute() or ".." in path.parts or "\\" in value:
        PROBLEMS.append(f"{label} is not a canonical repository path: {value}")
    elif not (ROOT / path).is_file():
        PROBLEMS.append(f"{label} references missing file: {value}")


def validate_traceability() -> int:
    trace = load_json(TRACEABILITY_PATH)
    expected = {
        "schema": "cex.sequence-52-architecture-traceability.v1",
        "status": "active",
        "candidate_sequence": 52,
        "runtime_policy": "external_only",
        "architecture_decision": "decisions/adr-004-three-module-external-agent-battle-platform.md",
        "closure_contract": "docs/architecture/external-agent-runtime-boundary-sequence-52.md",
        "checker": "scripts/check-external-agent-runtime-boundary.py",
        "shared_trigger": "docs/release-evidence/p0-candidate-trigger.json",
        "production_authorization": "not_granted",
    }
    for field, expected_value in expected.items():
        if trace.get(field) != expected_value:
            PROBLEMS.append(f"{TRACEABILITY_PATH} {field} must equal {expected_value!r}")
    for field in ("architecture_decision", "closure_contract", "checker", "shared_trigger"):
        require_path(trace.get(field), f"architecture traceability.{field}")

    controls = trace.get("controls")
    if not isinstance(controls, list):
        PROBLEMS.append(f"{TRACEABILITY_PATH} controls must be an array")
        controls = []
    expected_ids = {f"M{index}" for index in range(1, 11)}
    seen: set[str] = set()
    for index, control in enumerate(controls):
        label = f"architecture controls[{index}]"
        if not isinstance(control, dict):
            PROBLEMS.append(f"{label} must be an object")
            continue
        control_id = control.get("id")
        if not isinstance(control_id, str) or not control_id:
            PROBLEMS.append(f"{label}.id is invalid")
            continue
        if control_id in seen:
            PROBLEMS.append(f"duplicate architecture control: {control_id}")
        seen.add(control_id)
        requirement = control.get("requirement")
        if not isinstance(requirement, str) or len(requirement.strip()) < 60:
            PROBLEMS.append(f"{control_id} requirement is incomplete")
        for field in ("source", "implementation", "verification"):
            values = control.get(field)
            if not isinstance(values, list) or not values:
                PROBLEMS.append(f"{control_id}.{field} must be a non-empty array")
                continue
            for path_index, path in enumerate(values):
                require_path(path, f"{control_id}.{field}[{path_index}]")
        require_path(control.get("hosted_gate"), f"{control_id}.hosted_gate")
    if seen != expected_ids:
        PROBLEMS.append(
            "architecture control set mismatch: missing="
            + ",".join(sorted(expected_ids - seen))
            + " extra="
            + ",".join(sorted(seen - expected_ids))
        )

    required_blockers = {
        "runner_allocation_and_non_empty_exact_sha_execution",
        "protected_main_ruleset_governance",
        "independent_non_pusher_review",
        "downstream_world_game_immutable_revision_binding",
        "real_external_agent_provider_reconciliation",
        "final_human_go_no_go",
    }
    blockers = trace.get("external_blockers")
    if not isinstance(blockers, list) or not required_blockers.issubset(set(map(str, blockers))):
        PROBLEMS.append(f"{TRACEABILITY_PATH} does not preserve all independent blockers")
    return len(controls)


def validate_runner_probe(relative: str, runner_marker: str) -> None:
    text = require(
        relative,
        "workflow_dispatch:",
        "permissions: {}",
        'test "$GITHUB_EVENT_NAME" = workflow_dispatch',
        'test "$GITHUB_REF" = refs/heads/main',
        runner_marker,
    )
    forbid(
        relative,
        text,
        (
            "pull_request:",
            "push:",
            "actions/checkout@",
            "contents: write",
            "pull-requests: write",
            "id-token: write",
        ),
    )


def scan_activation_surfaces() -> None:
    roots = (ROOT / ".github/workflows", ROOT / "deploy", ROOT / "config")
    candidates: set[Path] = set()
    for root in roots:
        if root.is_dir():
            candidates.update(path for path in root.rglob("*") if path.is_file())
    candidates.update(path for path in ROOT.glob(".env*.example") if path.is_file())
    candidates.update(path for path in ROOT.glob("docker-compose*.y*ml") if path.is_file())
    forbidden = (
        "--features legacy-local-provider-dispatch",
        "CEX_ENABLE_LEGACY_LOCAL_PROVIDER_DISPATCH=true",
        "CEX_ENABLE_LEGACY_LOCAL_PROVIDER_DISPATCH: true",
        "CEX_ENABLE_LEGACY_LOCAL_PROVIDER_DISPATCH: 'true'",
        'CEX_ENABLE_LEGACY_LOCAL_PROVIDER_DISPATCH: "true"',
    )
    for path in sorted(candidates):
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        for marker in forbidden:
            if marker in text:
                PROBLEMS.append(
                    f"{path.relative_to(ROOT).as_posix()} activates legacy local provider dispatch: {marker}"
                )


def validate_cargo_feature_boundary() -> None:
    try:
        manifest = tomllib.loads(read("services/execution-service/Cargo.toml"))
    except tomllib.TOMLDecodeError as error:
        PROBLEMS.append(f"execution Cargo.toml is invalid: {error}")
        return
    features = manifest.get("features", {})
    if features.get("default") != []:
        PROBLEMS.append("execution-service default feature set must remain empty")
    if features.get("legacy-local-provider-dispatch") != []:
        PROBLEMS.append("legacy-local-provider-dispatch feature declaration drift")
    bins = manifest.get("bin", [])
    worker = next(
        (item for item in bins if isinstance(item, dict) and item.get("name") == "execution-provider-dispatch-worker"),
        None,
    )
    if not isinstance(worker, dict) or worker.get("required-features") != ["legacy-local-provider-dispatch"]:
        PROBLEMS.append("legacy worker is not isolated behind its non-default feature")


def validate_runtime_sources() -> None:
    execution_lib = require(
        "services/execution-service/src/lib.rs",
        '#[cfg(feature = "legacy-local-provider-dispatch")]\npub mod provider_dispatch;',
        "pub mod providers;",
        "post(api::start_execution)",
        "post(api::succeed_execution)",
    )
    forbid(
        "services/execution-service/src/lib.rs",
        execution_lib,
        (
            '"/v1/executions/:id/process"',
            "post(api::process_execution)",
            "post(provider_dispatch::start_execution)",
            "post(provider_dispatch::process_execution)",
        ),
    )

    providers = require(
        "services/execution-service/src/providers.rs",
        'pub const RUNTIME_POLICY: &str = "external_only";',
        "external_agent_runtime_required",
        "pub async fn dispatch_via_provider",
        "Err(ProviderDispatchError::external_agent_required())",
        "hepta_agent_protocol_v1",
    )
    runtime = providers.split("#[cfg(test)]", 1)[0]
    forbid(
        "services/execution-service/src/providers.rs runtime implementation",
        runtime,
        (
            "OllamaProviderAdapter",
            "OpenClawCliProviderAdapter",
            "std::process::Command",
            "tokio::process",
            "/api/generate",
            "Command::new",
            ".response.text()",
            "response.text()",
        ),
    )

    require(
        "services/execution-service/src/bin/execution-provider-dispatch-worker.rs",
        "startup.profile.is_production_like()",
        "CEX_ENABLE_LEGACY_LOCAL_PROVIDER_DISPATCH",
        "hepta_agent_protocol_v1",
        "runtime_policy=legacy_local_only",
    )

    capability = require(
        "services/capability-service/src/lib.rs",
        'const REGISTRY_ENV: &str = "CAPABILITY_EXTERNAL_AGENT_REGISTRY_JSON";',
        '"external_agent_capability"',
        '"runtime_policy": "external_only"',
        "MAX_REGISTRY_BYTES",
        "MAX_REGISTRY_RECORDS",
        "unsupported field",
        "is_production_like",
        "implicit dev is disabled",
    )
    capability_runtime = capability.split("#[cfg(test)]", 1)[0]
    forbid(
        "services/capability-service/src/lib.rs runtime implementation",
        capability_runtime,
        (
            "OpenClaw",
            "OPENCLAW",
            "Ollama",
            "ollama",
            "tokio::process",
            "std::process::Command",
            "~/.openclaw",
            "CAPABILITY_STATIC_REGISTRY_JSON",
            "default_capabilities",
            "cap.demo.",
        ),
    )


def validate_documents_and_tests() -> None:
    require(
        "decisions/adr-004-three-module-external-agent-battle-platform.md",
        "状态：Accepted",
        "Runtime policy: `external_only`",
        "不得把 Agent runtime、模型路由、Prompt 托管或推理执行重新引入项目核心",
        "legacy-local-provider-dispatch",
        "production-like profiles must reject",
    )
    require(
        "docs/hepta-agent-protocol-v1.md",
        "Runtime policy: `external_only`",
        "Capabilities are declarations used for discovery and eligibility. They do not authorize Hepta to execute the Agent.",
        "Agent private keys never enter Hepta, Nakama, or TRNM",
    )
    require(
        "docs/architecture/external-agent-runtime-boundary-sequence-52.md",
        "Status: active architecture closure contract",
        "Candidate sequence: `52`",
        "Block M",
        "CAPABILITY_EXTERNAL_AGENT_REGISTRY_JSON",
        "retired `/process` route",
        "runner probes remain `workflow_dispatch`-only",
        "Production authorization: `not_granted`",
    )
    require(
        "docs/index.md",
        "Sequence 52",
        "Sequence 54",
        "provider-specific Ollama/OpenClaw wording",
        "runner probe definition is not execution evidence",
    )
    require(
        "docs/modules/execution-service.md",
        "default workspace build does not compile or route to local provider adapters",
        "legacy-local-provider-dispatch",
        "external Agent",
        "retired `/process` route",
    )
    require(
        "docs/modules/capability-service.md",
        "external Agent capability declarations",
        "CAPABILITY_EXTERNAL_AGENT_REGISTRY_JSON",
        "local model discovery is forbidden",
    )
    require(
        "services/execution-service/tests/external_agent_boundary.rs",
        "default_execution_router_does_not_expose_retired_process_endpoint",
        "StatusCode::NOT_FOUND",
        "provider_compatibility_surface_fails_closed_without_network_or_prompt_echo",
        "assert!(!error.message.contains(prompt))",
    )


def validate_sequence54_authority() -> None:
    authority = load_json("docs/development-doc-authority-v1.json")
    expected = {
        "candidate_sequence": 54,
        "architecture_decision": "decisions/adr-004-three-module-external-agent-battle-platform.md",
        "architecture_closure": "docs/architecture/external-agent-runtime-boundary-sequence-52.md",
        "architecture_traceability": "docs/traceability/sequence-52-architecture-v1.json",
        "architecture_boundary_checker": "scripts/check-external-agent-runtime-boundary.py",
        "integration_plan": "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-SEQUENCE54-INTEGRATION.md",
        "integration_traceability": "docs/traceability/v12-sequence54-integration-v1.json",
        "production_authorization": "not_granted",
    }
    for key, value in expected.items():
        if authority.get(key) != value:
            PROBLEMS.append(f"development authority {key} must equal {value!r}")
    for field in (
        "architecture_decision",
        "architecture_closure",
        "architecture_traceability",
        "architecture_boundary_checker",
        "integration_plan",
        "integration_traceability",
    ):
        require_path(authority.get(field), f"development authority.{field}")

    trigger = load_json("docs/release-evidence/p0-candidate-trigger.json")
    if trigger.get("sequence") != 54:
        PROBLEMS.append("shared candidate trigger must be Sequence 54")
    if trigger.get("integration_traceability") != "docs/traceability/v12-sequence54-integration-v1.json":
        PROBLEMS.append("shared candidate trigger lacks Sequence 54 traceability")
    if trigger.get("production_authorization") != "not_granted":
        PROBLEMS.append("shared candidate trigger grants production authorization")
    purpose = str(trigger.get("purpose", ""))
    for marker in ("Sequence 52/53", "external-Agent-only runtime", "retired process route"):
        if marker not in purpose:
            PROBLEMS.append(f"Sequence 54 trigger purpose lacks inherited architecture marker: {marker}")


validate_documents_and_tests()
validate_sequence54_authority()
validate_cargo_feature_boundary()
validate_runtime_sources()
validate_runner_probe(
    ".github/workflows/self-hosted-desktop-availability.yml",
    'test "$RUNNER_NAME" = desktop',
)
validate_runner_probe(
    ".github/workflows/self-hosted-fleet-availability.yml",
    'test "$RUNNER_NAME" = rog',
)
validate_runner_probe(
    ".github/workflows/p0-self-hosted-runner-diagnostics.yml",
    'test "$RUNNER_NAME" = rog',
)
scan_activation_surfaces()
control_count = validate_traceability()

result = {
    "schema": "cex.external-agent-runtime-boundary-check.v1",
    "status": "failed" if PROBLEMS else "ok",
    "architecture_sequence": 52,
    "repository_candidate_sequence": 54,
    "runtime_policy": "external_only",
    "traceability_controls": control_count,
    "default_process_route": "absent",
    "local_provider_runtime": "legacy_non_default_and_production_forbidden",
    "checker_may_grant_production_authorization": False,
    "production_authorization": "not_granted",
    "problems": sorted(set(PROBLEMS)),
}
print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
sys.exit(1 if PROBLEMS else 0)
