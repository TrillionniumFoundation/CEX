#!/usr/bin/env python3
"""Fail closed if Matrix adapter callers can bypass strict profile validation.

This checker proves only repository source/API wiring. Rust compilation, startup,
PostgreSQL and homeserver qualification remain separate exact-head gates.
"""
from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
FACADE = "services/matrix-entry-adapter/src/lib.rs"
IMPLEMENTATION = "services/matrix-entry-adapter/src/implementation.rs"
MAIN = "services/matrix-entry-adapter/src/main.rs"
PROFILE_WRAPPER = "services/matrix-entry-adapter/src/runtime_profile.rs"
MANIFEST = "services/matrix-entry-adapter/Cargo.toml"
WORKFLOW = ".github/workflows/matrix-review-repair-regression.yml"
AGGREGATE = "scripts/check-development-docs.py"
MODULE_DOC = "docs/modules/matrix-entry-adapter.md"
BOUNDARY_DOC = "docs/matrix-adapter-validated-construction-v1.md"
EXPECTED_IMPLEMENTATION_BLOB = "2897b042eb60671bf6631857ea45840807360037"
EXPECTED_PROFILE_WRAPPER = (
    "// Compatibility import only; parsing and tests are owned by shared-config.\n"
    "pub use shared_config::runtime_guard::matrix_profile::resolve_profiles;\n"
)


def require(text: str, *markers: str) -> None:
    for marker in markers:
        if marker not in text:
            raise AssertionError(f"missing Matrix adapter API boundary marker: {marker}")


def forbid(text: str, *markers: str) -> None:
    for marker in markers:
        if marker in text:
            raise AssertionError(f"forbidden Matrix adapter API boundary marker: {marker}")


def ordered(text: str, *markers: str) -> None:
    position = 0
    for marker in markers:
        found = text.find(marker, position)
        if found < 0:
            raise AssertionError(f"missing ordered Matrix adapter operation: {marker}")
        position = found + len(marker)


def git_blob_sha(content: bytes) -> str:
    prefix = f"blob {len(content)}\0".encode("ascii")
    return hashlib.sha1(prefix + content, usedforsecurity=False).hexdigest()


def read_sources(root: Path) -> dict[str, Any]:
    text_paths = [
        FACADE,
        MAIN,
        PROFILE_WRAPPER,
        MANIFEST,
        WORKFLOW,
        AGGREGATE,
        MODULE_DOC,
        BOUNDARY_DOC,
    ]
    sources: dict[str, Any] = {
        path: (root / path).read_text(encoding="utf-8") for path in text_paths
    }
    sources[IMPLEMENTATION] = (root / IMPLEMENTATION).read_bytes()
    return sources


def validate(sources: dict[str, Any]) -> None:
    implementation = sources[IMPLEMENTATION]
    if not isinstance(implementation, bytes):
        raise AssertionError("implementation source must be validated as bytes")
    actual_blob = git_blob_sha(implementation)
    if actual_blob != EXPECTED_IMPLEMENTATION_BLOB:
        raise AssertionError(
            "private implementation is not the reviewed former crate-root blob: "
            f"{actual_blob}"
        )

    facade = sources[FACADE]
    require(
        facade,
        '#![recursion_limit = "256"]',
        "#![forbid(unsafe_code)]",
        "#[allow(dead_code, unused_attributes)]",
        '#[path = "implementation.rs"]',
        "mod implementation;",
        "use shared_config::runtime_guard::matrix_profile::resolve_profiles;",
        "pub struct ValidatedMatrixAdapterEnvironment",
        "_private: ()",
        "pub fn validate_process_environment()",
        '"MATRIX_ENTRY_RUNTIME_PROFILE"',
        '"CEX_RUNTIME_PROFILE"',
        '"APP_ENV"',
        "Err(std::env::VarError::NotUnicode(_))",
        'return Err("non_unicode_matrix_runtime_profile");',
        "let profile = resolve_profiles(&values)?;",
        'std::env::set_var("MATRIX_ENTRY_RUNTIME_PROFILE", profile.legacy_value());',
        "pub struct AppState",
        "inner: implementation::AppState",
        "pub async fn from_validated_env(",
        "_validated: ValidatedMatrixAdapterEnvironment",
        "implementation::AppState::from_env()",
        "pub fn bind_addr(&self) -> &str",
        "pub fn build_router(state: AppState) -> Router",
        "implementation::build_router(state.inner)",
    )
    forbid(
        facade,
        "pub mod implementation",
        "pub use implementation",
        "pub use crate::implementation",
        "pub struct MatrixAdapterConfig",
        "pub enum RuntimeProfile",
        "pub _private:",
        "include!(",
    )
    if re.search(r"pub\s+(?:async\s+)?fn\s+(?:from_env|new)\s*\(", facade):
        raise AssertionError("facade exposes an unvalidated state/config constructor")
    if re.search(r"pub\s+(?:\([^)]*\)\s+)?mod\s+implementation", facade):
        raise AssertionError("private implementation module became externally visible")
    if re.search(
        r"(?:derive\s*\([^)]*(?:Clone|Copy|Default)[^)]*\)|impl\s+Default\s+for)"
        r"[^;]{0,200}ValidatedMatrixAdapterEnvironment",
        facade,
        flags=re.DOTALL,
    ):
        raise AssertionError("validated environment token became caller-inventible")

    main = sources[MAIN]
    require(
        main,
        "fn prepare_runtime_profile() -> Result<ValidatedMatrixAdapterEnvironment, &'static str>",
        "validate_process_environment()",
        "let validated_environment = match prepare_runtime_profile()",
        "runtime.block_on(run(validated_environment))",
        "async fn run(validated_environment: ValidatedMatrixAdapterEnvironment)",
        "AppState::from_validated_env(validated_environment)",
        "let bind_addr = state.bind_addr().to_owned();",
        "let app: Router = build_router(state);",
    )
    ordered(
        main,
        "let validated_environment = match prepare_runtime_profile()",
        "tokio::runtime::Builder::new_multi_thread()",
        "runtime.block_on(run(validated_environment))",
    )
    forbid(
        main,
        "AppState::from_env",
        "MatrixAdapterConfig",
        "state.config()",
        "#[tokio::main]",
        "mod implementation",
        "mod runtime_profile",
        "unsafe",
    )

    if sources[PROFILE_WRAPPER] != EXPECTED_PROFILE_WRAPPER:
        raise AssertionError("profile compatibility file is not the exact shared-parser re-export")
    manifest = sources[MANIFEST]
    require(
        manifest,
        'name = "matrix-entry-adapter"',
        'shared-config = { path = "../../crates/shared-config" }',
    )
    forbid(manifest, "matrix-entry-adapter-implementation", "implementation/Cargo.toml")

    workflow = sources[WORKFLOW]
    require(
        workflow,
        "python3 scripts/test-matrix-adapter-api-boundary.py",
        "python3 scripts/check-matrix-adapter-api-boundary.py",
        "cargo test --locked -p matrix-entry-adapter --all-targets",
        "cargo clippy --locked -p matrix-entry-adapter --all-targets -- -D warnings",
    )
    forbid(workflow, "continue-on-error: true")

    aggregate = sources[AGGREGATE]
    require(
        aggregate,
        'ROOT / "scripts/check-matrix-adapter-api-boundary.py"',
        '"cex.matrix-adapter-api-boundary-check.v1"',
    )

    module_doc = sources[MODULE_DOC]
    boundary_doc = sources[BOUNDARY_DOC]
    for text in (module_doc, boundary_doc):
        require(
            text,
            "ValidatedMatrixAdapterEnvironment",
            "validate_process_environment",
            "from_validated_env",
            "implementation.rs",
            "not_granted",
        )
    require(
        boundary_doc,
        EXPECTED_IMPLEMENTATION_BLOB,
        "source/API boundary only",
        "real Rust compilation",
    )


def result(problems: list[str]) -> dict[str, Any]:
    return {
        "schema": "cex.matrix-adapter-api-boundary-check.v1",
        "status": "failed" if problems else "ok",
        "ok": not problems,
        "implementation_blob": EXPECTED_IMPLEMENTATION_BLOB,
        "runtime_execution_proven": False,
        "checker_may_grant_production_authorization": False,
        "production_authorization": "not_granted",
        "problems": problems,
    }


def main() -> int:
    problems: list[str] = []
    try:
        validate(read_sources(ROOT))
    except (AssertionError, OSError, UnicodeError, ValueError, TypeError) as error:
        problems.append(str(error))
    print(json.dumps(result(problems), indent=2, ensure_ascii=False, sort_keys=True))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
