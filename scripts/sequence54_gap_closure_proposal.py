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


def repair_toolchain_workflow() -> None:
    path = ROOT / ".github/workflows/p0-rust-toolchain-convergence.yml"
    text = path.read_text(encoding="utf-8")
    start_marker = "      - name: Bind source, base and prospective merge identities\n"
    end_marker = "      - name: Install exact Rust toolchain\n"
    start = text.find(start_marker)
    end = text.find(end_marker, start + len(start_marker))
    if start < 0 or end < 0:
        raise SystemExit("toolchain identity step boundary is missing")
    replacement = """      - name: Bind source, base and prospective merge identities
        id: identity
        shell: bash
        run: |
          set -euo pipefail
          mkdir -p run/rust-toolchain-convergence
          python3 - <<'PYTHON' >> "$GITHUB_OUTPUT"
          import json
          import os
          import pathlib
          import re
          import subprocess

          SHA40 = re.compile(r'^[0-9a-f]{40}$')
          root = pathlib.Path.cwd()

          def require(condition, message):
              if not condition:
                  raise SystemExit(message)

          def git(cwd, *arguments):
              result = subprocess.run(
                  ['git', '-C', str(cwd), *arguments],
                  capture_output=True,
                  text=True,
                  check=False,
              )
              require(
                  result.returncode == 0,
                  f"git {' '.join(arguments)} failed in {cwd}: "
                  f"{result.stderr.strip() or result.stdout.strip()}",
              )
              return result.stdout.strip()

          source_sha = os.environ['SOURCE_SHA']
          base_sha = os.environ['BASE_SHA']
          merge_sha = os.environ.get('PROSPECTIVE_MERGE_SHA', '')
          event_name = os.environ['GITHUB_EVENT_NAME']
          require(SHA40.fullmatch(source_sha), 'source SHA is not canonical')
          require(SHA40.fullmatch(base_sha), 'base SHA is not canonical')
          require(git(root, 'rev-parse', 'HEAD') == source_sha, 'source checkout identity mismatch')
          source_tree = git(root, 'rev-parse', 'HEAD^{tree}')
          require(SHA40.fullmatch(source_tree), 'source tree is not canonical')

          base_tree = None
          merge_tree = None
          merge_parents = None
          if event_name == 'pull_request':
              require(SHA40.fullmatch(merge_sha), 'prospective merge SHA is not canonical')
              base_dir = root / '.candidate-identity/base'
              merge_dir = root / '.candidate-identity/merge'
              require(git(base_dir, 'rev-parse', 'HEAD') == base_sha, 'base checkout identity mismatch')
              require(git(merge_dir, 'rev-parse', 'HEAD') == merge_sha, 'merge checkout identity mismatch')
              base_tree = git(base_dir, 'rev-parse', 'HEAD^{tree}')
              merge_tree = git(merge_dir, 'rev-parse', 'HEAD^{tree}')
              require(SHA40.fullmatch(base_tree), 'base tree is not canonical')
              require(SHA40.fullmatch(merge_tree), 'merge tree is not canonical')
              merge_parents = git(merge_dir, 'show', '-s', '--format=%P', 'HEAD').split()
              require(
                  merge_parents == [base_sha, source_sha],
                  f'prospective merge parent order mismatch: {merge_parents!r}',
              )
          else:
              merge_sha = ''

          value = {
              'schema': 'cex.rust-toolchain-candidate-identity.v2',
              'repository': os.environ['GITHUB_REPOSITORY'],
              'repository_id': os.environ['SOURCE_REPOSITORY_ID'],
              'event_name': event_name,
              'pull_request_number': int(os.environ['PULL_REQUEST_NUMBER']),
              'source_repository': os.environ['SOURCE_REPOSITORY'],
              'source_repository_id': os.environ['SOURCE_REPOSITORY_ID'],
              'source_sha': source_sha,
              'source_tree': source_tree,
              'base_sha': base_sha,
              'base_tree': base_tree,
              'prospective_merge_sha': merge_sha or None,
              'prospective_merge_tree': merge_tree,
              'prospective_merge_parents': merge_parents,
              'run_id': os.environ['GITHUB_RUN_ID'],
              'run_attempt': os.environ['GITHUB_RUN_ATTEMPT'],
              'production_authorization': 'not_granted',
          }
          pathlib.Path('run/rust-toolchain-convergence/candidate-identity.json').write_text(
              json.dumps(value, indent=2, sort_keys=True) + '\\n',
              encoding='utf-8',
          )
          print(f'source_tree={source_tree}')
          print(f'base_tree={base_tree or ""}')
          print(f'merge_tree={merge_tree or ""}')
          PYTHON

"""
    path.write_text(text[:start] + replacement + text[end:], encoding="utf-8")


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


def remove_transient_apply_surface() -> None:
    paths = (
        ROOT / ".github/workflows/sequence54-gap-closure-proposal.yml",
        ROOT / "scripts/sequence54_gap_closure_proposal.py",
    )
    for path in paths:
        if not path.is_file() or path.is_symlink():
            raise SystemExit(f"transient apply surface is missing or invalid: {path}")
    for path in paths:
        path.unlink()


def main() -> int:
    repair_build_unblock()
    repair_route_parser()
    repair_matrix_postgres_regression()
    repair_toolchain_workflow()
    repair_candidate_trigger()
    remove_transient_apply_surface()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
