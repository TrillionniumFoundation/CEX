#!/usr/bin/env python3
"""Reject local-provider configuration or Prompt retention in default Execution state."""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STATE_PATH = "services/execution-service/src/state.rs"
TEST_PATH = "services/execution-service/tests/external_agent_boundary.rs"
FEATURE = '#[cfg(feature = "legacy-local-provider-dispatch")]'
NO_FEATURE = '#[cfg(not(feature = "legacy-local-provider-dispatch"))]'
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


def require(text: str, relative: str, *markers: str) -> None:
    for marker in markers:
        if marker not in text:
            PROBLEMS.append(f"{relative} lacks required marker: {marker}")


def is_feature_guarded(lines: list[str], index: int) -> bool:
    checked = 0
    for cursor in range(index - 1, -1, -1):
        value = lines[cursor].strip()
        if not value:
            continue
        checked += 1
        if value == FEATURE:
            return True
        if value.startswith("#[cfg(") or checked >= 4:
            return False
    return False


def require_feature_guarded_call(lines: list[str], marker: str) -> None:
    matches = [index for index, line in enumerate(lines) if marker in line]
    if not matches:
        PROBLEMS.append(f"{STATE_PATH} lacks historical compatibility read: {marker}")
        return
    for index in matches:
        if not is_feature_guarded(lines, index):
            PROBLEMS.append(
                f"{STATE_PATH} local-provider read is not feature-gated: {marker}"
            )


state = read(STATE_PATH)
tests = read(TEST_PATH)
lines = state.splitlines()

require(
    state,
    STATE_PATH,
    "pub struct ProviderInputStore",
    'inner: Arc<RwLock<HashMap<Uuid, ProviderDispatchInput>>>,',
    "normal execution admission cannot retain raw Prompt material",
    "Default builds assign inert values and never read local-provider env vars",
    "Legacy local-provider configuration is compiled into meaningful",
    "pub provider_inputs: ProviderInputStore",
    "provider_inputs: ProviderInputStore::default()",
    "let ollama_base_url = String::new();",
    "let openclaw_cli_bin = String::new();",
    "let openclaw_config_path: Option<String> = None;",
    "let openclaw_state_dir: Option<String> = None;",
    "let openclaw_agent_dir: Option<String> = None;",
    FEATURE,
    NO_FEATURE,
)

if "pub provider_inputs: Arc<RwLock<HashMap<Uuid, ProviderDispatchInput>>>" in state:
    PROBLEMS.append("default AppState still exposes a retaining provider Prompt map")
if state.count("provider_inputs: ProviderInputStore::default()") < 2:
    PROBLEMS.append("all AppState constructors must use the fail-closed ProviderInputStore")
if state.count("let _ = (execution_id, input);") != 1:
    PROBLEMS.append("default ProviderInputStore does not deterministically discard inserts")
if state.count("let _ = execution_id;") < 2:
    PROBLEMS.append("default ProviderInputStore read/remove paths are not fail-closed")

for marker in (
    'env::var("OLLAMA_BASE_URL")',
    'env::var("OPENCLAW_CLI_BIN")',
    'optional_non_empty_env("OPENCLAW_CONFIG_PATH")',
    'optional_non_empty_env("OPENCLAW_STATE_DIR")',
    'optional_non_empty_env("OPENCLAW_AGENT_DIR")',
    '"EXECUTION_PROVIDER_DISPATCH_TIMEOUT_SECONDS"',
):
    require_feature_guarded_call(lines, marker)

for marker in (
    f"{NO_FEATURE}\n        let ollama_base_url = String::new();",
    f"{NO_FEATURE}\n        let openclaw_cli_bin = String::new();",
    f"{NO_FEATURE}\n        let openclaw_config_path: Option<String> = None;",
    f"{NO_FEATURE}\n        let openclaw_state_dir: Option<String> = None;",
    f"{NO_FEATURE}\n        let openclaw_agent_dir: Option<String> = None;",
    f"{NO_FEATURE}\n        let execution_provider_dispatch_timeout_seconds =",
):
    if marker not in state:
        PROBLEMS.append(f"{STATE_PATH} lacks default inert compatibility assignment: {marker}")

require(
    tests,
    TEST_PATH,
    "default_state_discards_retired_provider_prompt_material",
    "state.provider_inputs.write().await.insert",
    ".get(&execution_id)\n        .is_none()",
    "assert!(state.ollama_base_url.is_empty())",
    "assert!(state.openclaw_cli_bin.is_empty())",
    "default_state_source_feature_gates_local_provider_environment_reads",
)

result = {
    "schema": "cex.execution-default-state-boundary-check.v1",
    "status": "failed" if PROBLEMS else "ok",
    "candidate_sequence": 52,
    "runtime_policy": "external_only",
    "default_retains_provider_prompt": False,
    "default_reads_local_provider_environment": False,
    "legacy_configuration_requires_non_default_feature": True,
    "checker_may_grant_production_authorization": False,
    "production_authorization": "not_granted",
    "problems": PROBLEMS,
}
print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
raise SystemExit(1 if PROBLEMS else 0)
