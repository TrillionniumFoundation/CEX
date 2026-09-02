#!/usr/bin/env python3
"""Validate the accepted external-Agent-only runtime boundary.

This check is intentionally source based. It prevents an implementation plan,
workspace change, workflow, deployment manifest, or runner probe from silently
reintroducing platform-owned model routing, prompt hosting, inference execution,
or synthetic qualification evidence contrary to ADR-004. It can reject a
repository candidate; it cannot grant production authorization.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

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


def require_path(value: object, label: str) -> None:
    if not isinstance(value, str) or not value:
        PROBLEMS.append(f"{label} must be a non-empty repository path")
        return
    path = Path(value)
    if path.is_absolute() or ".." in path.parts or "\\" in value:
        PROBLEMS.append(f"{label} is not a canonical repository path: {value}")
        return
    if not (ROOT / path).is_file():
        PROBLEMS.append(f"{label} references missing file: {value}")


def forbid(relative: str, text: str, *markers: str) -> None:
    for marker in markers:
        if marker in text:
            PROBLEMS.append(f"{relative} contains forbidden runtime marker: {marker}")


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
            PROBLEMS.append(
                f"{TRACEABILITY_PATH} {field} must equal {expected_value!r}"
            )
    for field in (
        "architecture_decision",
        "closure_contract",
        "checker",
        "shared_trigger",
    ):
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
            paths = control.get(field)
            if not isinstance(paths, list) or not paths:
                PROBLEMS.append(f"{control_id}.{field} must be a non-empty array")
                continue
            for path_index, path in enumerate(paths):
                require_path(path, f"{control_id}.{field}[{path_index}]")
        require_path(control.get("hosted_gate"), f"{control_id}.hosted_gate")
    if seen != expected_ids:
        PROBLEMS.append(
            "architecture control set mismatch: missing="
            + ",".join(sorted(expected_ids - seen))
            + " extra="
            + ",".join(sorted(seen - expected_ids))
        )

    blockers = trace.get("external_blockers")
    required_blockers = {
        "runner_allocation_and_non_empty_exact_sha_execution",
        "protected_main_ruleset_governance",
        "independent_non_pusher_review",
        "downstream_world_game_immutable_revision_binding",
        "real_external_agent_provider_reconciliation",
        "final_human_go_no_go",
    }
    if not isinstance(blockers, list) or not required_blockers.issubset(
        {str(item) for item in blockers}
    ):
        PROBLEMS.append(
            f"{TRACEABILITY_PATH} does not preserve all independent external blockers"
        )
    return len(controls)


def scan_activation_surfaces() -> None:
    roots = [ROOT / ".github" / "workflows", ROOT / "deploy", ROOT / "config"]
    candidates: list[Path] = []
    for root in roots:
        if root.is_dir():
            candidates.extend(path for path in root.rglob("*") if path.is_file())
    candidates.extend(path for path in ROOT.glob(".env*.example") if path.is_file())
    candidates.extend(path for path in ROOT.glob("docker-compose*.yml") if path.is_file())
    candidates.extend(path for path in ROOT.glob("docker-compose*.yaml") if path.is_file())

    for path in sorted(set(candidates)):
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        relative = path.relative_to(ROOT).as_posix()
        for marker in (
            "--features legacy-local-provider-dispatch",
            "--all-features",
            "CEX_ENABLE_LEGACY_LOCAL_PROVIDER_DISPATCH=true",
            "CEX_ENABLE_LEGACY_LOCAL_PROVIDER_DISPATCH: true",
            "CEX_ENABLE_LEGACY_LOCAL_PROVIDER_DISPATCH: 'true'",
            'CEX_ENABLE_LEGACY_LOCAL_PROVIDER_DISPATCH: "true"',
        ):
            if marker in text:
                PROBLEMS.append(
                    f"{relative} activates forbidden legacy local provider dispatch: {marker}"
                )


def validate_runner_probe(relative: str, expected_runner_marker: str) -> None:
    text = require(
        relative,
        "workflow_dispatch:",
        "permissions: {}",
        'test "$GITHUB_EVENT_NAME" = workflow_dispatch',
        'test "$GITHUB_REF" = refs/heads/main',
        expected_runner_marker,
    )
    for forbidden in (
        "pull_request:",
        "push:",
        "actions/checkout@",
        "contents: write",
        "pull-requests: write",
        "id-token: write",
    ):
        if forbidden in text:
            PROBLEMS.append(
                f"{relative} is not a bounded no-checkout manual runner probe: {forbidden}"
            )


decision = require(
    "decisions/adr-004-three-module-external-agent-battle-platform.md",
    "状态：Accepted",
    "Runtime policy: `external_only`",
    "不得把 Agent runtime、模型路由、Prompt 托管或推理执行重新引入项目核心",
    "legacy-local-provider-dispatch",
    "production-like profiles must reject",
)
protocol = require(
    "docs/hepta-agent-protocol-v1.md",
    "Runtime policy: `external_only`",
    "Capabilities are declarations used for discovery and eligibility. They do not authorize Hepta to execute the Agent.",
)
closure = require(
    "docs/architecture/external-agent-runtime-boundary-sequence-52.md",
    "Status: active architecture closure contract",
    "Candidate sequence: `52`",
    "Block M",
    "default workspace build",
    "CAPABILITY_EXTERNAL_AGENT_REGISTRY_JSON",
    "retired `/process` route",
    "runner probes remain `workflow_dispatch`-only",
    "Production authorization: `not_granted`",
)
authority = require(
    "docs/development-doc-authority-v1.json",
    '"candidate_sequence": 52',
    '"architecture_decision": "decisions/adr-004-three-module-external-agent-battle-platform.md"',
    '"architecture_closure": "docs/architecture/external-agent-runtime-boundary-sequence-52.md"',
    '"architecture_traceability": "docs/traceability/sequence-52-architecture-v1.json"',
    '"architecture_boundary_checker": "scripts/check-external-agent-runtime-boundary.py"',
)
require(
    "docs/index.md",
    "Sequence 52",
    "provider-specific Ollama/OpenClaw wording",
    "runner probe definition is not execution evidence",
)
require(
    "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md",
    "Sequence-52 correction",
    "external-agent-runtime-boundary-sequence-52.md",
    "sequence-52-architecture-v1.json",
    "retired `/process` route is absent",
)
manifest = require(
    "services/execution-service/Cargo.toml",
    "[features]",
    "default = []",
    "legacy-local-provider-dispatch = []",
    'name = "execution-provider-dispatch-worker"',
    'required-features = ["legacy-local-provider-dispatch"]',
    'name = "external_agent_boundary"',
)
execution_lib = require(
    "services/execution-service/src/lib.rs",
    '#[cfg(feature = "legacy-local-provider-dispatch")]\npub mod provider_dispatch;',
    "pub mod providers;",
    'post(api::start_execution)',
    'post(api::succeed_execution)',
)
forbid(
    "services/execution-service/src/lib.rs",
    execution_lib,
    '"/v1/executions/:id/process"',
    "post(api::process_execution)",
    "post(provider_dispatch::start_execution)",
    "post(provider_dispatch::process_execution)",
)
provider_boundary = require(
    "services/execution-service/src/providers.rs",
    'pub const RUNTIME_POLICY: &str = "external_only";',
    'pub const LEGACY_LOCAL_DISPATCH_STATUS: &str = "legacy_local_provider_dispatch_disabled";',
    "external_agent_runtime_required",
    "pub async fn dispatch_via_provider",
    "Err(ProviderDispatchError::external_agent_required())",
    "hepta_agent_protocol_v1",
    "must not appear",
)
provider_runtime = provider_boundary.split("#[cfg(test)]", 1)[0]
forbid(
    "services/execution-service/src/providers.rs runtime implementation",
    provider_runtime,
    "OllamaProviderAdapter",
    "OpenClawCliProviderAdapter",
    "std::process::Command",
    "tokio::process",
    "/api/generate",
    'args(["infer", "model", "run"',
    ".response.text()",
    "response.text()",
    "Command::new",
)
worker = require(
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
    '"external-agent"',
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
)
require(
    "services/capability-service/src/main.rs",
    "AppState::from_env().await",
    "capability-service startup rejected",
    "CONFIG_ERROR_EXIT_CODE",
    "CAPABILITY_BIND_ADDR",
)
require(
    "services/execution-service/tests/external_agent_boundary.rs",
    "default_execution_router_does_not_expose_retired_process_endpoint",
    "StatusCode::NOT_FOUND",
    "default_router_source_cannot_regain_the_retired_process_route_silently",
    "provider_compatibility_surface_fails_closed_without_network_or_prompt_echo",
    "assert!(!error.message.contains(prompt))",
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
trigger = require(
    "docs/release-evidence/p0-candidate-trigger.json",
    '"sequence": 52',
    "external-Agent-only runtime boundary",
    "retired process route",
    '"production_authorization": "not_granted"',
)
require(
    "scripts/check-development-docs.py",
    "check-external-agent-runtime-boundary.py",
    "cex.external-agent-runtime-boundary-check.v1",
)
validate_runner_probe(
    ".github/workflows/self-hosted-desktop-availability.yml",
    'test "$RUNNER_NAME" = desktop',
)
validate_runner_probe(
    ".github/workflows/self-hosted-fleet-availability.yml",
    'test "$RUNNER_NAME" = rog',
)
traceability_controls = validate_traceability()

if decision and "平台不拥有、托管、调度或执行参赛 Agent" not in decision:
    PROBLEMS.append("ADR-004 no longer denies platform-owned Agent execution")
if protocol and "Agent private keys never enter Hepta, Nakama, or TRNM" not in protocol:
    PROBLEMS.append("external Agent protocol lost its private-key custody boundary")
if authority and closure and "external-agent-runtime-boundary-sequence-52.md" not in authority:
    PROBLEMS.append("machine-readable authority does not bind the architecture closure contract")
if manifest.count("legacy-local-provider-dispatch") < 2:
    PROBLEMS.append("legacy provider feature is not both declared and bound to the worker target")
if worker and "is_production_like" not in worker:
    PROBLEMS.append("legacy provider worker lacks an explicit production-like rejection")
if trigger and closure and "sequence 52" not in closure.lower():
    PROBLEMS.append("candidate trigger and architecture closure do not share sequence-52 identity")
if provider_runtime:
    dispatch_start = provider_runtime.find("pub async fn dispatch_via_provider")
    dispatch_body = provider_runtime[dispatch_start:] if dispatch_start >= 0 else ""
    if "Err(ProviderDispatchError::external_agent_required())" not in dispatch_body:
        PROBLEMS.append("provider boundary does not fail closed to the external Agent protocol")
    if "PRIVATE-PROMPT" in provider_runtime:
        PROBLEMS.append("test-only prompt marker leaked into provider runtime implementation")

scan_activation_surfaces()

result = {
    "schema": "cex.external-agent-runtime-boundary-check.v1",
    "status": "failed" if PROBLEMS else "ok",
    "candidate_sequence": 52,
    "runtime_policy": "external_only",
    "top_level_domains": ["hepta", "nakama", "trnm"],
    "architecture_traceability_controls": traceability_controls,
    "default_build_compiles_local_inference": False,
    "default_router_exposes_local_process_route": False,
    "legacy_local_dispatch_production_allowed": False,
    "capability_registry_authority": "external_agent_declarations_only",
    "runner_probe_definition_is_execution_evidence": False,
    "checker_may_grant_production_authorization": False,
    "production_authorization": "not_granted",
    "problems": PROBLEMS,
}
print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
raise SystemExit(1 if PROBLEMS else 0)
