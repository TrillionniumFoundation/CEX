#!/usr/bin/env python3
"""Filesystem/metadata fixtures only. These tests do not invoke real Cargo."""
from __future__ import annotations
import contextlib
import copy
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('cargo_workspace_authority', Path(__file__).with_name('check-cargo-workspace-authority.py'))
assert SPEC and SPEC.loader
C = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(C)


class WorkspaceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.member = 'vendor/contract'
        self.entry = {'workspace_member':self.member,'package':'real-package-name',
                      'documentation':'docs/modules/contract.md',
                      'source_entrypoints':[self.member+'/src/lib.rs']}
        self.catalog = {'schema':'cex.module-catalog.v1','production_authorization':'not_granted','modules':[self.entry]}
        self.write('Cargo.toml','[workspace]\nmembers=["vendor/contract"]\n')
        self.write('Cargo.lock','# immutable fixture\nversion=4\n')
        self.write(self.member+'/Cargo.toml','[package]\nname="real-package-name"\nversion="0.1.0"\n')
        self.write(self.member+'/src/lib.rs','pub fn fixture() {}\n')
        self.write('docs/modules/contract.md','Fixture, not real package documentation.\n')
        self.sync_catalog()
        self.metadata = {'version':1,'workspace_root':str(self.root),'workspace_members':['opaque-id'],
                         'packages':[{'id':'opaque-id','name':'real-package-name','source':None,
                          'manifest_path':str(self.root/self.member/'Cargo.toml'),
                          'targets':[{'name':'real_package_name','kind':['lib'],
                                      'src_path':str(self.root/self.member/'src/lib.rs')}]}]}

    def write(self,name,text):
        p=self.root/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(text)

    def sync_catalog(self):self.write(C.CATALOG,json.dumps(self.catalog))
    def validate(self):return C.validate_metadata(self.root,self.metadata,{self.member:self.entry})
    def invalid(self,code):
        with self.assertRaisesRegex(C.ContractError,code): self.validate()

    def test_invalid_or_boolean_metadata_version_fails(self):
        for version in [True, False, None, 0, 2, '1']:
            self.metadata['version'] = version
            self.invalid('metadata_schema_mismatch')

    def test_valid_member_and_library(self):
        self.assertEqual(self.validate()['member_count'],1)
        self.assertEqual(self.validate()['target_count'],1)

    def test_implicit_member_not_hidden_by_explicit_catalog(self):
        # This is the old explicit-list blind spot, modelled as Cargo output.
        self.write('vendor/implicit/Cargo.toml','[package]\nname="implicit"\nversion="0.1.0"\n')
        p=copy.deepcopy(self.metadata['packages'][0]);p.update(id='implicit-id',name='implicit',
            manifest_path=str(self.root/'vendor/implicit/Cargo.toml'))
        self.metadata['packages'].append(p);self.metadata['workspace_members'].append('implicit-id')
        self.invalid('cargo_member_not_catalogued')

    def test_unselected_catalog_member_fails(self):
        with self.assertRaisesRegex(C.ContractError,'catalog_member_not_in_cargo'):
            C.validate_metadata(self.root,self.metadata,{self.member:self.entry,'missing':self.entry})

    def test_package_alias_is_not_package_identity(self):
        self.metadata['packages'][0]['name']='dependency-alias'
        self.invalid('cargo_package_name_mismatch')

    def test_same_manifest_cannot_appear_twice(self):
        p=copy.deepcopy(self.metadata['packages'][0]);p['id']='second'
        self.metadata['packages'].append(p);self.metadata['workspace_members'].append('second')
        self.invalid('metadata_duplicate_member_path')

    def test_duplicate_member_and_package_ids_fail(self):
        original=copy.deepcopy(self.metadata)
        self.metadata['workspace_members']*=2;self.invalid('metadata_duplicate_member_id')
        self.metadata=original;self.metadata['packages']*=2;self.invalid('metadata_duplicate_package_id')

    def test_missing_package_record_fails(self):
        self.metadata['packages']=[];self.invalid('metadata_member_package_missing')

    def test_wrong_workspace_root_fails(self):
        self.metadata['workspace_root']=str(self.root/'another');self.invalid('metadata_workspace_root_mismatch')

    def test_manifest_outside_checkout_fails(self):
        self.metadata['packages'][0]['manifest_path']=str(self.root.parent/'outside/Cargo.toml')
        self.invalid('repository_path_escape')

    def test_nonlocal_member_fails(self):
        self.metadata['packages'][0]['source']='registry+fixture';self.invalid('workspace_member_not_local')

    def test_automatic_binary_must_be_documented(self):
        self.write(self.member+'/src/bin/utility.rs','fn main() {}')
        self.metadata['packages'][0]['targets'].append({'name':'utility','kind':['bin'],
            'src_path':str(self.root/self.member/'src/bin/utility.rs')})
        self.invalid('cargo_target_not_documented')
        self.entry['source_entrypoints'].append(self.member+'/src/bin/utility.rs')
        self.assertEqual(self.validate()['target_count'],2)

    def test_test_example_bench_and_build_script_are_not_ignored(self):
        for kind,source in [('test','tests/replay.rs'),('example','examples/client.rs'),
                            ('bench','benches/bytes.rs'),('custom-build','build.rs')]:
            with self.subTest(kind=kind):
                self.write(self.member+'/'+source,'fn main() {}')
                self.metadata['packages'][0]['targets']=[{'name':'fixture','kind':[kind],
                    'src_path':str(self.root/self.member/source)}]
                self.invalid('cargo_target_not_documented')

    def test_missing_or_cross_module_source_fails(self):
        self.metadata['packages'][0]['targets'][0]['src_path']=str(self.root/'absent.rs')
        self.invalid('cargo_target_source_missing')

    def test_empty_unknown_and_duplicate_targets_fail(self):
        orig=copy.deepcopy(self.metadata)
        self.metadata['packages'][0]['targets']=[];self.invalid('cargo_targets_missing')
        self.metadata=copy.deepcopy(orig);self.metadata['packages'][0]['targets'][0]['kind']=['new-unreviewed-kind']
        self.invalid('unsupported_cargo_target_kind')
        self.metadata=orig;self.metadata['packages'][0]['targets']*=2;self.invalid('duplicate_cargo_target')

    def test_symlink_source_is_rejected(self):
        p=self.root/self.member/'src/lib.rs';p.unlink();p.symlink_to(self.root/'Cargo.toml')
        self.invalid('linked_repository_path')

    def test_snapshot_requires_complete_manifests_and_documents(self):
        C.snapshot(self.root)
        (self.root/self.member/'Cargo.toml').unlink()
        with self.assertRaisesRegex(C.ContractError,'source_unavailable'):C.snapshot(self.root)

    def test_explicit_list_drift_cannot_be_ignored(self):
        self.write('Cargo.toml','[workspace]\nmembers=["vendor/another"]\n')
        with self.assertRaisesRegex(C.ContractError,'explicit_member_catalog_mismatch'):C.snapshot(self.root)

    def test_duplicate_catalog_entries_fail(self):
        self.catalog['modules']*=2;self.sync_catalog()
        with self.assertRaisesRegex(C.ContractError,'duplicate_module_identity'):C.snapshot(self.root)

    def test_duplicate_json_fields_fail(self):
        with self.assertRaisesRegex(C.ContractError,'duplicate_json_field'):
            C.unique_json(b'{"version":1,"version":1}')

    def test_cargo_command_is_locked_and_no_deps(self):
        answer=subprocess.CompletedProcess([],0,json.dumps(self.metadata).encode(),b'')
        with patch.object(C.shutil,'which',return_value='/fixture/cargo'), \
             patch.object(C.subprocess,'run',return_value=answer) as invoke:
            result=C.execute(self.root,dict(os.environ))
        argv=invoke.call_args.args[0]
        self.assertEqual(argv[1:6],['metadata','--locked','--no-deps','--format-version','1'])
        self.assertEqual(result['member_count'],1)
        self.assertIn('Cargo.lock',result['input_sha256'])

    def test_failed_or_timed_out_cargo_never_produces_success(self):
        with patch.object(C.shutil,'which',return_value='/fixture/cargo'):
            with patch.object(C.subprocess,'run',return_value=subprocess.CompletedProcess([],1,b'',b'PRIVATE_SECRET')):
                with self.assertRaisesRegex(C.ContractError,'cargo_metadata_nonzero_exit'):C.execute(self.root,{})
            with patch.object(C.subprocess,'run',side_effect=subprocess.TimeoutExpired('fixture',120)):
                with self.assertRaisesRegex(C.ContractError,'could_not_complete'):C.execute(self.root,{})

    def test_no_cargo_is_not_skip_or_success(self):
        with patch.object(C.shutil,'which',return_value=None):
            with self.assertRaisesRegex(C.ContractError,'cargo_required_not_executed'):C.execute(self.root,{})

    def test_source_drift_during_metadata_fails(self):
        def change(*args,**kwargs):
            self.write('Cargo.lock','mutated fixture lock')
            return subprocess.CompletedProcess([],0,json.dumps(self.metadata).encode(),b'')
        with patch.object(C.shutil,'which',return_value='/fixture/cargo'),patch.object(C.subprocess,'run',side_effect=change):
            with self.assertRaisesRegex(C.ContractError,'sources_changed'):C.execute(self.root,{})

    def test_cli_cannot_accept_saved_metadata_instead_of_cargo(self):
        with patch.object(sys,'argv',['checker','--metadata','fake.json']), \
             patch.object(C,'execute',side_effect=AssertionError('should not run')), \
             contextlib.redirect_stdout(io.StringIO()) as output:
            self.assertEqual(C.main(),1)
        record=json.loads(output.getvalue())
        self.assertEqual(record['error'],'unexpected_arguments')
        self.assertFalse(record['cargo_metadata_succeeded'])
        self.assertEqual(record['production_authorization'],'not_granted')


    def mutate_during_metadata(self, change):
        def invoke(*args, **kwargs):
            change()
            return subprocess.CompletedProcess([], 0, json.dumps(self.metadata).encode(), b'')
        with patch.object(C.shutil, 'which', return_value='/fixture/cargo'), \
             patch.object(C.subprocess, 'run', side_effect=invoke):
            return C.execute(self.root, {})

    def test_new_automatic_binary_after_metadata_is_rejected(self):
        with self.assertRaisesRegex(C.ContractError, 'target_layout_changed'):
            self.mutate_during_metadata(lambda: self.write(self.member+'/src/bin/late.rs', 'fn main() {}'))

    def test_new_default_build_script_after_metadata_is_rejected(self):
        with self.assertRaisesRegex(C.ContractError, 'target_layout_changed'):
            self.mutate_during_metadata(lambda: self.write(self.member+'/build.rs', 'fn main() {}'))

    def test_removed_candidate_after_metadata_is_rejected(self):
        self.write(self.member+'/examples/client.rs', 'fn main() {}')
        with self.assertRaisesRegex(C.ContractError, 'target_layout_changed'):
            self.mutate_during_metadata(lambda: (self.root/self.member/'examples/client.rs').unlink())

    def test_nested_target_main_added_after_metadata_is_rejected(self):
        (self.root/self.member/'tests/recovery').mkdir(parents=True)
        with self.assertRaisesRegex(C.ContractError, 'target_layout_changed'):
            self.mutate_during_metadata(lambda: self.write(self.member+'/tests/recovery/main.rs', 'fn main() {}'))

    def test_target_discovery_rejects_linked_directories_before_cargo(self):
        (self.root/self.member/'examples').symlink_to(self.root/'docs', target_is_directory=True)
        with patch.object(C.shutil, 'which', return_value='/fixture/cargo'), \
             patch.object(C.subprocess, 'run', side_effect=AssertionError('cargo started')):
            with self.assertRaisesRegex(C.ContractError, 'linked_repository_path'):
                C.execute(self.root, {})

    def test_target_discovery_has_a_bounded_entry_budget(self):
        with patch.object(C, 'MAX_TARGET_LAYOUT_ENTRIES', 1):
            with self.assertRaisesRegex(C.ContractError, 'target_layout_budget'):
                C.target_layout(self.root, {self.member:self.entry})

    def test_stable_layout_emits_separate_layout_hash(self):
        result = self.mutate_during_metadata(lambda: None)
        self.assertRegex(result['target_layout_sha256'], r'^[0-9a-f]{64}$')
        self.assertNotIn('@target-layout', result['input_sha256'])

    def test_directory_creation_does_not_masquerade_as_absent_layout(self):
        with self.assertRaisesRegex(C.ContractError, 'target_layout_changed'):
            self.mutate_during_metadata(lambda: (self.root/self.member/'benches').mkdir())


if __name__=='__main__':
    unittest.main(verbosity=2)
