#!/usr/bin/env python3
"""Exercise real parser/generator behavior in disposable synthetic Git workspaces."""
from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from semantic_source_snapshot import InputSnapshot, regular_bytes, require_complete_workspace

SCRIPTS = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location('semantic_generator', SCRIPTS / 'generate-repository-semantics.py')
assert SPEC and SPEC.loader
G = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(G)


class GeneratorIntegrationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.member = 'services/demo'
        self.module = {'workspace_member': self.member, 'package': 'demo', 'owner': 'test-owner',
                       'authority': 'fixture-only', 'source_entrypoints': [self.member + '/src/main.rs']}
        self.write('Cargo.toml', '[workspace]\nmembers=["services/demo"]\n')
        self.write('services/demo/Cargo.toml', '[package]\nname="demo"\nversion="0.0.0"\n')
        self.write('services/demo/src/main.rs', 'fn main() { Router::new().route("/health", get(h)); }\n')
        self.write('docs/module-catalog-v1.json', json.dumps({'modules': [self.module]}))
        fields = ['bounded_context', 'authority_mode', 'authentication', 'authorization', 'data_classification', 'retirement']
        rule = {key: 'fixture-only' for key in fields}
        rule.update(id='test-rule', fact_types=['route','config','data'], path_globs=['*'])
        self.policy = {'schema':'fixture.policy', 'required_fields':fields, 'rules':[rule]}
        self.write('docs/repository-semantic-policy-v1.json', json.dumps(self.policy))
        self.write('services/demo/migrations/0001_test.sql', 'create table public.test_facts(id integer);\n')
        for name in ['generate-repository-semantics.py', 'rust_route_contract.py', 'semantic_source_snapshot.py']:
            self.write('scripts/' + name, (SCRIPTS / name).read_text())
        subprocess.run(['git', 'init', '-q', str(self.root)], check=True, capture_output=True)
        self.git('add', '.')
        for name, value in {'ROOT':self.root, 'POLICY_PATH':self.root/'docs/repository-semantic-policy-v1.json',
                            'MODULE_CATALOG_PATH':self.root/'docs/module-catalog-v1.json',
                            'SOURCE_ROOTS':tuple(self.root / x for x in ['apps','services','crates'])}.items():
            p = patch.object(G, name, value)
            p.start()
            self.addCleanup(p.stop)

    def write(self, relative, data):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(data)
        return path

    def git(self, *args):
        return subprocess.run(['git','-C',str(self.root),*args], check=True, capture_output=True).stdout

    def test_complete_fixture_generates_deterministically(self):
        one, two = G.build_document(), G.build_document()
        self.assertEqual(one, two)
        self.assertEqual(one['counts']['route'], 1)
        self.assertEqual(one['counts']['data'], 1)
        self.assertEqual(one['production_authorization'], 'not_granted')
        self.assertEqual(one['input_coverage']['workspace_members'], 1)
        self.assertFalse(one['route_extractor']['implicit_head_inferred'])

    def test_same_line_declarations_keep_distinct_identities(self):
        self.write('services/demo/src/main.rs', '.route("/a", get(one)).route("/a", get(two));')
        facts = [f for f in G.build_document()['facts'] if f['fact_type'] == 'route']
        self.assertEqual(len(facts), 2)
        self.assertNotEqual(facts[0]['fact_id'], facts[1]['fact_id'])
        self.assertNotEqual(facts[0]['source_column'], facts[1]['source_column'])

    def test_generator_does_not_reintroduce_adjacent_method_leak(self):
        self.write('services/demo/src/main.rs', '.route("/a", get(one)).route("/b", post(two));')
        facts = [f for f in G.build_document()['facts'] if f['fact_type'] == 'route']
        self.assertEqual([(f['value'], f['methods']) for f in facts], [('/a',['GET']),('/b',['POST'])])

    def test_dynamic_and_nested_routes_report_unresolved_counts(self):
        self.write('services/demo/src/main.rs', '.route(PATH, make_routes()).nest("/api", child);')
        doc = G.build_document()
        self.assertEqual(doc['counts']['dynamic_route_paths'], 1)
        self.assertEqual(doc['counts']['unresolved_route_methods'], 2)

    def test_non_rust_routing_is_explicitly_not_claimed(self):
        self.write('services/demo/client.ts', "app.route('/x', post(handler));")
        doc = G.build_document()
        self.assertIn('services/demo/client.ts', doc['input_coverage']['non_rust_route_sources_not_analyzed'])
        self.assertEqual(doc['counts']['route'], 1)

    def test_missing_member_source_cannot_emit_partial_success(self):
        (self.root / 'services/demo/src/main.rs').unlink()
        with self.assertRaises(AssertionError): G.build_document()

    def test_missing_tracked_non_entrypoint_is_detected(self):
        path = self.write('services/demo/src/extra.rs', 'fn extra() {}')
        self.git('add', '.')
        path.unlink()
        with self.assertRaises(AssertionError): G.build_document()

    def test_no_git_source_subset_is_rejected(self):
        shutil.rmtree(self.root / '.git')
        with self.assertRaises(AssertionError): G.build_document()

    def test_mismatched_workspace_membership_is_rejected(self):
        self.write('Cargo.toml', '[workspace]\nmembers=["services/demo","services/missing"]\n')
        with self.assertRaises(AssertionError): G.build_document()

    def test_glob_membership_requires_explicit_checker_support(self):
        self.write('Cargo.toml', '[workspace]\nmembers=["services/*"]\n')
        with self.assertRaises(AssertionError): G.build_document()

    def test_package_name_mismatch_is_rejected(self):
        self.write('services/demo/Cargo.toml', '[package]\nname="wrong"\n')
        with self.assertRaises(AssertionError): G.build_document()

    def test_entrypoint_cannot_escape_owning_module(self):
        self.module['source_entrypoints'] = ['../secret']
        self.write('docs/module-catalog-v1.json', json.dumps({'modules':[self.module]}))
        with self.assertRaises(AssertionError): G.build_document()

    def test_policy_mutation_after_parsing_is_detected(self):
        def mutate(_):
            self.write('docs/repository-semantic-policy-v1.json', json.dumps({**self.policy, 'changed':True}))
        with patch.object(G,'enforce_consumer_projection_boundary',side_effect=mutate), self.assertRaises(AssertionError):
            G.build_document()

    def test_parser_file_mutation_during_generation_is_detected(self):
        def mutate(_): self.write('scripts/rust_route_contract.py', '# changed during generation')
        with patch.object(G, 'enforce_consumer_projection_boundary', side_effect=mutate), self.assertRaises(AssertionError):
            G.build_document()

    def test_source_mutation_after_parsing_is_detected(self):
        def mutate(_): self.write('services/demo/src/main.rs','.route("/changed", post(h));')
        with patch.object(G,'enforce_consumer_projection_boundary',side_effect=mutate), self.assertRaises(AssertionError):
            G.build_document()

    def test_new_source_after_enumeration_is_detected(self):
        def mutate(_): self.write('services/demo/src/new.rs','fn new() {}')
        with patch.object(G,'enforce_consumer_projection_boundary',side_effect=mutate), self.assertRaises(AssertionError):
            G.build_document()

    def test_changed_git_inventory_is_detected(self):
        def mutate(_):
            self.write('new-note.txt','new tracked file')
            self.git('add','new-note.txt')
        with patch.object(G,'enforce_consumer_projection_boundary',side_effect=mutate), self.assertRaises(AssertionError):
            G.build_document()

    def test_symbolic_source_file_is_rejected(self):
        path = self.root/'services/demo/src/main.rs'
        other = self.write('services/demo/src/real.rs','fn main() {}')
        path.unlink(); path.symlink_to(other)
        with self.assertRaises(AssertionError): G.build_document()

    def test_symbolic_parent_directory_is_rejected(self):
        directory = self.root/'services/demo/src'
        directory.rename(directory.with_name('real-src'))
        directory.symlink_to(directory.with_name('real-src'), target_is_directory=True)
        with self.assertRaises(AssertionError): G.build_document()

    def test_untracked_symbolic_module_directory_cannot_hide_sources(self):
        directory = self.root / 'services/demo/src/linked'
        directory.symlink_to(self.root / 'docs', target_is_directory=True)
        with self.assertRaises(AssertionError): G.build_document()

    def test_single_snapshot_serves_parse_and_hash(self):
        snapshot = InputSnapshot(self.root)
        path = self.root/'Cargo.toml'
        first = snapshot.read(path)
        path.write_text('changed')
        self.assertEqual(snapshot.read(path), first)
        with self.assertRaises(AssertionError): snapshot.verify()

    def test_fifo_input_is_rejected_without_blocking(self):
        if not hasattr(os,'mkfifo'): self.skipTest('FIFO is unavailable on this platform')
        fifo = self.root/'fifo.rs'; os.mkfifo(fifo)
        with self.assertRaises(AssertionError): regular_bytes(self.root,fifo)

    def test_unclassified_fact_is_rejected(self):
        self.policy['rules'][0]['path_globs']=['no-match']
        self.write('docs/repository-semantic-policy-v1.json',json.dumps(self.policy))
        with self.assertRaises(AssertionError): G.build_document()

    def test_consumer_projection_boundary_still_fails_closed(self):
        with self.assertRaises(AssertionError):
            G.enforce_consumer_projection_boundary([{'fact_type':'route','source_path':'services/consumer-entry-api/src/world_routes.rs',
                'authority_mode':'authoritative','source_line':1,'value':'/world'}])

    def test_outside_and_symlink_output_is_rejected_before_writing(self):
        with self.assertRaises(AssertionError): G.checked_output_path(self.root.parent/'external.json')
        output=self.root/'docs/out.json'; output.symlink_to(self.root/'Cargo.toml')
        with self.assertRaises(AssertionError): G.write_document(output,'{}')
        self.assertIn('[workspace]',(self.root/'Cargo.toml').read_text())

    def test_atomic_output_and_temporary_cleanup(self):
        output=self.root/'docs/out.json'
        G.write_document(output,'{"test":true}\n')
        self.assertEqual(json.loads(output.read_text()), {'test':True})
        self.assertFalse(list(output.parent.glob('.semantic-contract-*.tmp')))

    def test_failed_replace_preserves_previous_output(self):
        output=self.write('docs/out.json','{"old":true}')
        with patch.object(G.os,'replace',side_effect=OSError('fixture')), self.assertRaises(OSError):
            G.write_document(output,'{"new":true}')
        self.assertEqual(json.loads(output.read_text()), {'old':True})
        self.assertFalse(list(output.parent.glob('.semantic-contract-*.tmp')))

    def test_source_change_updates_hash_and_method_identity(self):
        a=G.build_document()
        self.write('services/demo/src/main.rs','.route("/health", post(h));')
        b=G.build_document()
        self.assertNotEqual(a['source_tree_sha256'],b['source_tree_sha256'])
        self.assertNotEqual(a['facts'][1]['fact_id'],b['facts'][1]['fact_id'])


if __name__=='__main__':
    unittest.main(verbosity=2)
