#!/usr/bin/env python3
"""Pure mutation regressions for the Matrix validated-construction boundary."""
from __future__ import annotations

import copy
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "matrix_adapter_api_boundary",
    ROOT / "scripts/check-matrix-adapter-api-boundary.py",
)
assert SPEC and SPEC.loader
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class MatrixAdapterApiBoundaryTests(unittest.TestCase):
    def setUp(self) -> None:
        self.sources = CHECK.read_sources(ROOT)

    def reject_text(self, path: str, old: str, new: str) -> None:
        changed = copy.deepcopy(self.sources)
        self.assertIn(old, changed[path])
        changed[path] = changed[path].replace(old, new, 1)
        with self.assertRaises(AssertionError):
            CHECK.validate(changed)

    def test_current_boundary(self) -> None:
        CHECK.validate(self.sources)

    def test_implementation_byte_drift_is_rejected(self) -> None:
        changed = copy.deepcopy(self.sources)
        changed[CHECK.IMPLEMENTATION] += b"\n// drift\n"
        with self.assertRaises(AssertionError):
            CHECK.validate(changed)

    def test_implementation_module_cannot_be_public(self) -> None:
        self.reject_text(CHECK.FACADE, "mod implementation;", "pub mod implementation;")

    def test_legacy_constructor_cannot_return_to_facade(self) -> None:
        changed = copy.deepcopy(self.sources)
        changed[CHECK.FACADE] += "\npub async fn from_env() {}\n"
        with self.assertRaises(AssertionError):
            CHECK.validate(changed)

    def test_token_field_must_remain_private(self) -> None:
        self.reject_text(CHECK.FACADE, "    _private: (),", "    pub _private: (),")

    def test_token_cannot_be_default_constructed(self) -> None:
        changed = copy.deepcopy(self.sources)
        changed[CHECK.FACADE] += (
            "\nimpl Default for ValidatedMatrixAdapterEnvironment {\n"
            "    fn default() -> Self { Self { _private: () } }\n}\n"
        )
        with self.assertRaises(AssertionError):
            CHECK.validate(changed)

    def test_invalid_profile_cannot_skip_shared_parser(self) -> None:
        self.reject_text(
            CHECK.FACADE,
            "let profile = resolve_profiles(&values)?;",
            "let profile = AdapterProfile::Local;",
        )

    def test_non_unicode_profile_must_fail_closed(self) -> None:
        self.reject_text(
            CHECK.FACADE,
            'return Err("non_unicode_matrix_runtime_profile");',
            "values.push(None);",
        )

    def test_runtime_cannot_start_before_token(self) -> None:
        self.reject_text(
            CHECK.MAIN,
            "let validated_environment = match prepare_runtime_profile()",
            "let validated_environment = match validate_after_runtime()",
        )

    def test_main_cannot_call_legacy_constructor(self) -> None:
        self.reject_text(
            CHECK.MAIN,
            "AppState::from_validated_env(validated_environment)",
            "AppState::from_env()",
        )

    def test_main_must_consume_token(self) -> None:
        self.reject_text(
            CHECK.MAIN,
            "runtime.block_on(run(validated_environment))",
            "runtime.block_on(run())",
        )

    def test_compatibility_wrapper_cannot_grow_policy(self) -> None:
        changed = copy.deepcopy(self.sources)
        changed[CHECK.PROFILE_WRAPPER] += "\nfn local_fallback() {}\n"
        with self.assertRaises(AssertionError):
            CHECK.validate(changed)

    def test_workflow_cannot_drop_boundary_checker(self) -> None:
        self.reject_text(
            CHECK.WORKFLOW,
            "python3 scripts/check-matrix-adapter-api-boundary.py",
            "echo skipped-matrix-adapter-api-boundary",
        )

    def test_aggregate_cannot_drop_boundary_checker(self) -> None:
        self.reject_text(
            CHECK.AGGREGATE,
            'ROOT / "scripts/check-matrix-adapter-api-boundary.py"',
            'ROOT / "scripts/missing.py"',
        )

    def test_documentation_cannot_claim_production(self) -> None:
        self.reject_text(CHECK.BOUNDARY_DOC, "not_granted", "granted")


if __name__ == "__main__":
    unittest.main(verbosity=2)
