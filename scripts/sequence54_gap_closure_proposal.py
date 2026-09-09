#!/usr/bin/env python3
"""Produce a deterministic, no-write Sequence 54 blocker-closure proposal."""
from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def replace_exact(relative: str, old: str, new: str, expected: int = 1) -> None:
    path = ROOT / relative
    text = path.read_text(encoding="utf-8")
    actual = text.count(old)
    if actual != expected:
        raise SystemExit(
            f"{relative}: expected {expected} occurrences, found {actual}: {old[:100]!r}"
        )
    path.write_text(text.replace(old, new), encoding="utf-8")


def insert_before_exact(
    relative: str, marker: str, addition: str, expected: int = 1
) -> None:
    path = ROOT / relative
    text = path.read_text(encoding="utf-8")
    actual = text.count(marker)
    if actual != expected:
        raise SystemExit(
            f"{relative}: expected {expected} insertion markers, found {actual}: "
            f"{marker[:100]!r}"
        )
    if addition in text:
        raise SystemExit(f"{relative}: insertion already present")
    path.write_text(text.replace(marker, addition + marker), encoding="utf-8")


def repair_build_unblock() -> None:
    relative = "scripts/test-build-unblock.py"
    replace_exact(
        relative,
        "[('matrix-review-repair-regression.yml', 2), ('rust-service-gate.yml', 3)]",
        "[('matrix-review-repair-regression.yml', 4), ('rust-service-gate.yml', 3)]",
    )
    replace_exact(
        relative,
        "Complete Matrix package and current-schema regression",
        "Complete Matrix package and current-schema/operator regression",
        expected=2,
    )
    insert_before_exact(
        relative,
        """            env = {**os.environ, 'PATH': str(tools) + os.pathsep + os.environ['PATH'],
                   'TRACE': str(trace), 'FAIL': failure}
""",
        r"""            (scripts / 'check-matrix-operator-postgres.sh').write_text(
                '#!/bin/sh\nprintf "%s\\n" operator >> "$TRACE"\n[ "$FAIL" != operator ] || exit 71\n')
""",
    )
    replace_exact(
        relative,
        "self.assertEqual(trace, ['python3', 'fmt', 'test', 'clippy', 'database', 'git'])",
        "self.assertEqual(trace, ['python3', 'fmt', 'test', 'clippy', 'database', 'operator', 'git'])",
    )
    replace_exact(
        relative,
        """        self.assertIn('path: run/matrix-transport-postgres.json', self.rust)
        self.assertIn('if-no-files-found: error', self.rust)
""",
        """        self.assertIn('path: |', self.rust)
        self.assertIn('run/matrix-transport-postgres.json', self.rust)
        self.assertIn('run/matrix-operator-postgres.json', self.rust)
        self.assertIn('if-no-files-found: error', self.rust)
""",
    )


def repair_route_parser() -> None:
    replace_exact(
        "scripts/rust_route_contract.py",
        """        args = _arguments(tokens, pairs, begin + 1, end)
        if len(args) != 2 or any(lo >= hi for lo, hi in args):
""",
        """        args = _arguments(tokens, pairs, begin + 1, end)
        # A zero-argument application method named `route` cannot be an Axum
        # route registration, whose API requires both path and method-router
        # arguments. Keep malformed one/three-argument registrations fatal.
        if not args:
            continue
        if len(args) != 2 or any(lo >= hi for lo, hi in args):
""",
    )
    replace_exact(
        "scripts/test-rust-route-contract.py",
        """    def test_malformed_registration_is_not_silently_accepted(self):
""",
        """    def test_zero_argument_application_route_method_is_not_registration(self):
        self.assertEqual(R.extract_routes('let route = command.route()?;'), [])

    def test_malformed_registration_is_not_silently_accepted(self):
""",
    )


def repair_matrix_postgres_regression() -> None:
    replace_exact(
        "scripts/test-matrix-result-reconciliation-postgres.sql",
        "%matrix_adapter_result_principal_mismatch%",
        "%matrix_adapter_result_identity_mismatch%",
    )


def repair_candidate_trigger() -> None:
    path = ROOT / "docs/release-evidence/p0-candidate-trigger.json"
    value = json.loads(path.read_text(encoding="utf-8"))
    value["qualification_scope"] = (
        "sequence54-non-regressive-23-module-migration-0088-matrix-operator-0006-"
        "v3-four-boundary-functional-security-toolchain-governance-integration"
    )
    marker = (
        " Qualification requires non-empty successful required jobs on the unchanged exact-head "
        "and actual prospective-merge subjects; production authorization remains not_granted."
    )
    if marker.strip() not in value["purpose"]:
        value["purpose"] = value["purpose"].rstrip() + marker
    path.write_text(
        json.dumps(value, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def main() -> int:
    repair_build_unblock()
    repair_route_parser()
    repair_matrix_postgres_regression()
    repair_candidate_trigger()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
