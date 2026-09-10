#!/usr/bin/env python3
"""Apply the exact CEX blocker batch to a temporary 28c9 candidate clone."""

from __future__ import annotations

from pathlib import Path
import sys


def replace_once(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise SystemExit(f"{label} anchor did not match exactly once in {path}")
    path.write_text(text.replace(old, new), encoding="utf-8", newline="\n")


def patch_build_unblock(root: Path) -> None:
    path = root / "scripts/test-build-unblock.py"
    old = '''    def test_container_bootstrap_must_install_and_select_fixed_compiler(self):
        script = (ROOT / 'scripts/run-isolated-matrix-tests.sh').read_text()
        self.assertIn('FROM rust:1.98.0-bookworm', script)
        self.assertIn('rustup toolchain install 1.98.1 --profile minimal --component rustfmt,clippy', script)
        self.assertIn('rustup default 1.98.1', script)
        self.assertIn('rustup toolchain uninstall 1.98.0', script)
        self.assertIn('ENV RUST_VERSION=1.98.1', script)
'''
    new = '''    def test_container_bootstrap_must_install_and_select_fixed_compiler(self):
        script = (ROOT / 'scripts/run-isolated-matrix-tests.sh').read_text()
        self.assertIn('FROM rust:1.98.1-bookworm', script)
        self.assertIn('rustup toolchain install 1.98.1 --profile minimal --component rustfmt,clippy', script)
        self.assertIn('rustup default 1.98.1', script)
        self.assertIn('ENV RUST_VERSION=1.98.1', script)
        self.assertNotIn('1.98.0', script)
'''
    replace_once(path, old, new, "fixed compiler container regression")


def patch_replay_snapshot(root: Path) -> None:
    path = root / "services/consumer-entry-api/src/replay_store_snapshot.rs"
    replace_once(
        path,
        '''    Changed,
    UnsafePath,
    UnsupportedPlatform,
''',
        '''    Changed,
    UnsafePath,
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        windows
    )))]
    UnsupportedPlatform,
''',
        "unsupported-platform enum variant",
    )
    replace_once(
        path,
        '''            Self::Changed => formatter.write_str("replay store changed while it was being read"),
            Self::UnsafePath => formatter.write_str("replay store path is not descriptor-safe"),
            Self::UnsupportedPlatform => formatter
                .write_str("descriptor-safe replay snapshots are unsupported on this platform"),
''',
        '''            Self::Changed => formatter.write_str("replay store changed while it was being read"),
            Self::UnsafePath => formatter.write_str("replay store path is not descriptor-safe"),
            #[cfg(not(any(
                target_os = "linux",
                target_os = "android",
                target_os = "macos",
                target_os = "ios",
                windows
            )))]
            Self::UnsupportedPlatform => formatter
                .write_str("descriptor-safe replay snapshots are unsupported on this platform"),
''',
        "unsupported-platform display arm",
    )


def patch_operator_runner(root: Path) -> None:
    path = root / "scripts/matrix_operator_postgres_regression.py"
    old_constants = '''BASE_MIGRATIONS = (
    "0001_adapter_result_reconciliation.sql",
    "0002_runtime_roles.sql",
    "0003_adapter_result_evidence_binding.sql",
    "0004_adapter_result_runtime_reconciliation.sql",
    "0005_adapter_result_causal_binding.sql",
)
SECURITY_MIGRATIONS = (
    "0006_adapter_result_embedded_delivery_binding.sql",
)
MIGRATIONS = BASE_MIGRATIONS + SECURITY_MIGRATIONS
BASE_REGRESSIONS = (
    "scripts/test-matrix-result-reconciliation-postgres.sql",
    "scripts/test-matrix-result-evidence-hardening-postgres.sql",
    "scripts/test-matrix-result-runtime-reconciliation-postgres.sql",
    "scripts/test-matrix-result-causal-binding-postgres.sql",
)
SECURITY_REGRESSIONS = (
    "scripts/test-matrix-result-embedded-binding-postgres.sql",
    "scripts/test-matrix-result-task-invocation-binding-postgres.sql",
)
REGRESSIONS = BASE_REGRESSIONS + SECURITY_REGRESSIONS
'''
    new_constants = '''V1_MIGRATIONS = (
    "0001_adapter_result_reconciliation.sql",
    "0002_runtime_roles.sql",
    "0003_adapter_result_evidence_binding.sql",
    "0004_adapter_result_runtime_reconciliation.sql",
)
V2_MIGRATIONS = (
    "0005_adapter_result_causal_binding.sql",
)
V3_MIGRATIONS = (
    "0006_adapter_result_embedded_delivery_binding.sql",
)
MIGRATIONS = V1_MIGRATIONS + V2_MIGRATIONS + V3_MIGRATIONS
V1_REGRESSIONS = (
    "scripts/test-matrix-result-reconciliation-postgres.sql",
    "scripts/test-matrix-result-evidence-hardening-postgres.sql",
    "scripts/test-matrix-result-runtime-reconciliation-postgres.sql",
)
V2_REGRESSIONS = (
    "scripts/test-matrix-result-causal-binding-postgres.sql",
)
V3_REGRESSIONS = (
    "scripts/test-matrix-result-embedded-binding-postgres.sql",
    "scripts/test-matrix-result-task-invocation-binding-postgres.sql",
)
REGRESSIONS = V1_REGRESSIONS + V2_REGRESSIONS + V3_REGRESSIONS
'''
    replace_once(path, old_constants, new_constants, "operator migration eras")

    old_sequence = '''    for pass_number in (1, 2):
        for name in BASE_MIGRATIONS:
            migration_stage(pass_number, name)

    # Preserve the historical v1/v2 regressions before v3 deliberately revokes
    # runtime access to v2. This proves that the additive migration does not
    # rewrite or conceal the earlier contract.
    for relative in BASE_REGRESSIONS:
        inputs[relative] = base.read_input(root, relative)
        stages.append((Path(relative).stem, inputs[relative]))

    for pass_number in (1, 2):
        for name in SECURITY_MIGRATIONS:
            migration_stage(pass_number, name)

    for relative in SECURITY_REGRESSIONS:
        inputs[relative] = base.read_input(root, relative)
        stages.append((Path(relative).stem, inputs[relative]))
'''
    new_sequence = '''    # Each compatibility regression executes immediately before the next
    # additive migration deliberately revokes its runtime function. Running all
    # migrations first would test a historical API under a role that no longer
    # has permission to invoke it and would misclassify correct least privilege
    # as a database failure.
    for migrations, regressions in (
        (V1_MIGRATIONS, V1_REGRESSIONS),
        (V2_MIGRATIONS, V2_REGRESSIONS),
        (V3_MIGRATIONS, V3_REGRESSIONS),
    ):
        for pass_number in (1, 2):
            for name in migrations:
                migration_stage(pass_number, name)
        for relative in regressions:
            inputs[relative] = base.read_input(root, relative)
            stages.append((Path(relative).stem, inputs[relative]))
'''
    replace_once(path, old_sequence, new_sequence, "operator migration/regression sequence")


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: patch-cex-blocker-batch-28c9.py ROOT", file=sys.stderr)
        return 64
    root = Path(sys.argv[1]).resolve()
    patch_build_unblock(root)
    patch_replay_snapshot(root)
    patch_operator_runner(root)
    print("CEX_BLOCKER_BATCH_PATCH=PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
