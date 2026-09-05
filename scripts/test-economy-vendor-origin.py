#!/usr/bin/env python3
"""Pure record/identity fixtures; no external provenance is invented or contacted."""
from __future__ import annotations
import copy, importlib.util, unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location('economy_origin', Path(__file__).with_name('check-economy-vendor-origin.py'))
assert SPEC and SPEC.loader
C = importlib.util.module_from_spec(SPEC); SPEC.loader.exec_module(C)


class OriginTests(unittest.TestCase):
    def setUp(self):
        self.identities = {'tree':'a'*40,'Cargo.toml':'b'*40,'src/lib.rs':'c'*40}
        self.record = {
            'schema':'cex.trnm-economy-vendor-origin.v1', 'package':'trnm-economy-protocol',
            'package_version':'2.4.0','current_path':'vendor/trnm-economy-protocol',
            'current_git_tree':'a'*40,'current_files':{'Cargo.toml':'b'*40,'src/lib.rs':'c'*40},
            'imported_by_cex_commit':C.EXPECTED_IMPORT,'previous_external_path':C.EXPECTED_PREVIOUS,
            'external_origin':{'status':'unresolved','repository':None,'commit':None,'git_tree':None},
            'resolution_requirement':'bind immutable external repository, commit and tree',
            'production_authorization':'not_granted'}

    def validate(self, resolved=False): return C.validate(self.record, require_resolved=resolved, identities=self.identities)
    def test_honest_unresolved_contract_is_valid(self): self.assertEqual(self.validate(), [])
    def test_release_mode_rejects_unresolved_origin(self): self.assertIn('external_origin_unresolved', self.validate(True))
    def test_current_tree_drift_fails(self):
        self.identities['tree']='d'*40; self.assertIn('current_tree_drift', self.validate())
    def test_current_blob_drift_fails(self):
        self.identities['src/lib.rs']='d'*40; self.assertIn('lib_blob_drift', self.validate())
    def test_import_commit_cannot_be_relabelled(self):
        self.record['imported_by_cex_commit']='d'*40; self.assertIn('import_commit', self.validate())
    def test_relative_path_cannot_be_promoted_to_repository_identity(self):
        self.record['external_origin']['repository']=C.EXPECTED_PREVIOUS
        self.assertIn('partial_unresolved_origin', self.validate())
    def test_resolved_requires_exact_git_identities(self):
        self.record['external_origin']={'status':'resolved','repository':'TrillionniumFoundation/example','commit':'x','git_tree':'y'}
        problems=self.validate(True); self.assertIn('external_commit_invalid', problems); self.assertIn('external_git_tree_invalid', problems)
    def test_resolved_shape_can_pass_only_when_complete(self):
        self.record['external_origin']={'status':'resolved','repository':'TrillionniumFoundation/example','commit':'d'*40,'git_tree':'e'*40}
        self.assertEqual(self.validate(True), [])


if __name__=='__main__': unittest.main(verbosity=2)
