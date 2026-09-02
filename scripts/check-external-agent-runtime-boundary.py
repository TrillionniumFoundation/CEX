#!/usr/bin/env python3
"""Validate the accepted external-Agent-only runtime boundary.

This check is intentionally source based. It prevents an implementation plan,
workspace change, workflow, or deployment manifest from silently reintroducing
platform-owned model routing, prompt hosting, or inference execution contrary
to ADR-004. It can reject a repository candidate; it cannot grant production
authorization.
"""

from __future__ import annotations

import json
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
    except (OSError, UnicodeDecodeError) as error:
        PROBLEMS.append(f"cannot read required UTF-8 file {relative}: {error}")
        return ""


def require(relative: str, *markers: str) -> str:
    text = read(relative)
    for marker in markers:
        if marker not in text:
            PROBLEMS.append(f"{relative} lacks required marker: {marker}")
    return text


def forbid(relative: str, text: str, *markers: str) -> None:
    for marker in markers:
        if marker in text:
            PROBLEMS.append(f"{relative} contains forbidden runtime marker: {marker}")


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
    "docs/architecture/external-agent-runtime-boundary-sequence-51.md",
    "Status: active architecture closure contract",
    "Block M",
    "default workspace build",
    "CAPABILITY_EXTERNAL_AGENT_REGISTRY_JSON",
    "production_authorization: `not_granted`",
)
authority = require(
    "docs/development-doc-authority-v1.json",
    '"architecture_decision": "decisions/adr-004-three-module-external-agent-battle-platform.md"',
    '"architecture_closure": "docs/architecture/external-agent-runtime-boundary-sequence-51.md"',
    '"architecture_boundary_checker": "scripts/check-external-agent-runtime-boundary.py"',
)
manifest = require(
    "services/execution-service/Cargo.toml",
    "[features]",
    "default = []",
    "legacy-local-provider-dispatch = []",
    'name = "execution-provider-dispatch-worker"',
    'required-features = ["legacy-local-provider-dispatch"]',
)
execution_lib = require(
    "services/execution-service/src/lib.rs",
    '#[cfg(feature = "legacy-local-provider-dispatch")]\npub mod provider_dispatch;',
    "pub mod providers;",
    'post(api::start_execution)',
    'post(api::process_execution)',
)
forbid(
    "services/execution-service/src/lib.rs",
    execution_lib,
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
    "production-like profiles",
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
    "docs/modules/execution-service.md",
    "default workspace build does not compile or route to local provider adapters",
    "legacy-local-provider-dispatch",
    "external Agent",
)
require(
    "docs/modules/capability-service.md",
    "external Agent capability declarations",
    "CAPABILITY_EXTERNAL_AGENT_REGISTRY_JSON",
    "local model discovery is forbidden",
)
trigger = require(
    "docs/release-evidence/p0-candidate-trigger.json",
    '"sequence": 51',
    "external-Agent-only runtime boundary",
    '"production_authorization": "not_granted"',
)
wrapper = require(
    "scripts/check-development-docs.py",
    "check-external-agent-runtime-boundary.py",
    "cex.external-agent-runtime-boundary-check.v1",
)

if decision and "平台不拥有、托管、调度或执行参赛 Agent" not in decision:
    PROBLEMS.append("ADR-004 no longer denies platform-owned Agent execution")
if protocol and "Agent private keys never enter Hepta, Nakama, or TRNM" not in protocol:
    PROBLEMS.append("external Agent protocol lost its private-key custody boundary")
if authority and closure and "external-agent-runtime-boundary-sequence-51.md" not in authority:
    PROBLEMS.append("machine-readable authority does not bind the architecture closure contract")
if manifest.count("legacy-local-provider-dispatch") < 2:
    PROBLEMS.append("legacy provider feature is not both declared and bound to the worker target")
if worker and "is_production_like" not in worker:
    PROBLEMS.append("legacy provider worker lacks an explicit production-like rejection")
if trigger and closure and "sequence-51" not in closure.lower():
    PROBLEMS.append("candidate trigger and architecture closure do not share sequence-51 identity")
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
    "runtime_policy": "external_only",
    "top_level_domains": ["hepta", "nakama", "trnm"],
    "default_build_compiles_local_inference": False,
    "legacy_local_dispatch_production_allowed": False,
    "capability_registry_authority": "external_agent_declarations_only",
    "checker_may_grant_production_authorization": False,
    "production_authorization": "not_granted",
    "problems": PROBLEMS,
}
print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
raise SystemExit(1 if PROBLEMS else 0)
