#!/usr/bin/env python3
"""Shared profile source/linkage regressions; not Rust execution or Cargo resolution."""
from __future__ import annotations
import copy
import importlib.util
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('recovery_contract_shared', ROOT/'scripts/check-matrix-recovery-contract.py')
assert SPEC and SPEC.loader
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)
SHARED = 'crates/shared-config/src/runtime_guard/matrix_profile.rs'
MODULE = 'crates/shared-config/src/runtime_guard.rs'
PACKAGES = {
    'matrix-entry-adapter': 'services/matrix-entry-adapter',
    'matrix-bot-poller': 'apps/matrix-bot-poller',
    'matrix-bot-relay': 'apps/matrix-bot-relay',
}


def validate_dependencies(lock: dict, manifests: dict) -> None:
    """Check these three existing direct edges only; never claim resolution."""
    for name, relative in PACKAGES.items():
        dependency = manifests[name]['dependencies'].get('shared-config')
        if dependency != {'path': '../../crates/shared-config'}:
            raise AssertionError('shared-config must be the existing local crate: ' + name)
        rows = [p for p in lock['package'] if p['name'] == name and 'source' not in p]
        if len(rows) != 1 or rows[0]['dependencies'].count('shared-config') != 1:
            raise AssertionError('missing or duplicate shared-config lock edge: ' + name)
    shared = [p for p in lock['package'] if p['name'] == 'shared-config' and 'source' not in p]
    if len(shared) != 1:
        raise AssertionError('existing shared-config node required')


class SharedProfileTests(unittest.TestCase):
    def setUp(self):
        self.sources = CHECK.read_sources(ROOT)

    def reject(self, path, old, new):
        self.assertIn(old, self.sources[path])
        self.sources[path] = self.sources[path].replace(old, new, 1)
        with self.assertRaises(AssertionError):
            CHECK.validate(self.sources)

    def test_current_shared_linkage(self):
        CHECK.validate(self.sources)

    def test_local_parser_cannot_replace_reexport(self):
        for relative in PACKAGES.values():
            path = relative + '/src/runtime_profile.rs'
            changed = dict(self.sources)
            changed[path] += '\nfn parse() {}\n'
            with self.assertRaises(AssertionError):
                CHECK.validate(changed)

    def test_public_module_export_is_required(self):
        self.reject(MODULE, 'pub mod matrix_profile;', 'mod matrix_profile;')

    def test_central_staging_alias_is_not_optional(self):
        self.reject(SHARED, '"stage" | "staging"', '"stage"')

    def test_central_conflicts_are_not_precedence(self):
        self.reject(SHARED, 'previous != parsed', 'false')

    def test_invalid_profile_does_not_fall_back_to_local(self):
        self.reject(SHARED, 'Err("invalid_matrix_runtime_profile")', 'Ok(Self::Local)')

    def test_pure_parser_cannot_mutate_environment(self):
        self.sources[SHARED] += '\nfn mutate() { std::env::set_var("APP_ENV", "dev"); }\n'
        with self.assertRaises(AssertionError):
            CHECK.validate(self.sources)

    def test_original_semantic_cases_are_owned_once(self):
        tests = self.sources[SHARED]
        for name in ('staging_is_never_local_development', 'explicit_invalid_values_fail_closed',
                     'conflicts_do_not_follow_a_weaker_override', 'equivalent_aliases_are_accepted',
                     'absent_sources_are_explicitly_local', 'beta_retains_its_nonlocal_policy'):
            self.assertEqual(tests.count('fn ' + name + '('), 1)
            for relative in PACKAGES.values():
                self.assertNotIn('fn ' + name, self.sources[relative+'/src/runtime_profile.rs'])

    def test_three_local_manifest_and_lock_edges(self):
        lock = tomllib.loads((ROOT/'Cargo.lock').read_text())
        manifests = {name: tomllib.loads((ROOT/relative/'Cargo.toml').read_text())
                     for name, relative in PACKAGES.items()}
        validate_dependencies(lock, manifests)
        for name in PACKAGES:
            changed = copy.deepcopy(lock)
            next(p for p in changed['package'] if p['name'] == name)['dependencies'].remove('shared-config')
            with self.assertRaises(AssertionError):
                validate_dependencies(changed, manifests)
            changed_manifests = copy.deepcopy(manifests)
            changed_manifests[name]['dependencies']['shared-config'] = {'version': '0.1'}
            with self.assertRaises(AssertionError):
                validate_dependencies(lock, changed_manifests)

    def test_shared_tests_and_lints_execute_in_existing_jobs(self):
        for filename, job_name in [('matrix-review-repair-regression.yml', 'adapter-rust'),
                                   ('rust-service-gate.yml', 'hepta-postgres-integration')]:
            text = (ROOT/'.github/workflows'/filename).read_text()
            job = text.split('  '+job_name+':\n',1)[1]
            import re
            job = re.split(r'\n  [a-zA-Z][a-zA-Z0-9_-]*:\n', job, maxsplit=1)[0]
            for command in ('cargo fmt -p shared-config -- --check',
                            'cargo test --locked -p shared-config --all-targets',
                            'cargo clippy --locked -p shared-config --all-targets -- -D warnings'):
                self.assertIn(command, job)
            self.assertNotIn('continue-on-error: true', text)
        matrix = (ROOT/'.github/workflows/matrix-review-repair-regression.yml').read_text()
        self.assertIn('      - crates/shared-config/**', matrix)


if __name__ == '__main__':
    unittest.main(verbosity=2)
