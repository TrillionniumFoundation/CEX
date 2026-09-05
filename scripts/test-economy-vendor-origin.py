#!/usr/bin/env python3
"""Pure lineage fixtures; no historical external repository is inferred."""
from __future__ import annotations
import copy, importlib.util, unittest
from pathlib import Path

SPEC=importlib.util.spec_from_file_location('economy_origin',Path(__file__).with_name('check-economy-vendor-origin.py'))
assert SPEC and SPEC.loader
C=importlib.util.module_from_spec(SPEC); SPEC.loader.exec_module(C)


class LineageTests(unittest.TestCase):
    def setUp(self):
        rows=[]; self.identities={}
        for commit,tree,cargo,lib in C.EXPECTED:
            rows.append({'commit':commit,'git_tree':tree,'files':{'Cargo.toml':cargo,'src/lib.rs':lib}})
            self.identities[(commit,'vendor/trnm-economy-protocol')]=tree
            self.identities[(commit,'vendor/trnm-economy-protocol/Cargo.toml')]=cargo
            self.identities[(commit,'vendor/trnm-economy-protocol/src/lib.rs')]=lib
        last=C.EXPECTED[-1]
        for path,value in [('vendor/trnm-economy-protocol',last[1]),('vendor/trnm-economy-protocol/Cargo.toml',last[2]),('vendor/trnm-economy-protocol/src/lib.rs',last[3])]: self.identities[('HEAD',path)]=value
        genesis=rows[0]; genesis.update(authority='CEX_import_commit',decision='decisions/adr-005-trnm-economy-source-genesis.md')
        self.record={'schema':'cex.trnm-economy-vendor-origin.v2','package':'trnm-economy-protocol','package_version':'2.4.0','current_path':'vendor/trnm-economy-protocol','current_git_tree':last[1],'current_files':{'Cargo.toml':last[2],'src/lib.rs':last[3]},'source_genesis':genesis,'cex_lineage':rows[1:],'historical_external_context':{'previous_path':'../Trillionnium/trillionnium/crates/trnm-economy-protocol','immutable_repository_commit_tree_recorded_at_import':False,'status':'unknown_non_authoritative_prehistory'},'future_update_policy':'explicit CEX lineage or reviewed external refresh','production_authorization':'not_granted'}
    def validate(self): return C.validate(self.record,self.identities)
    def test_exact_lineage_passes(self): self.assertEqual(self.validate(),[])
    def test_current_tree_drift_fails(self): self.identities[('HEAD','vendor/trnm-economy-protocol')]='0'*40; self.assertTrue(any('current_tree_drift' in x for x in self.validate()))
    def test_historical_tree_drift_fails(self): self.identities[(C.EXPECTED[1][0],'vendor/trnm-economy-protocol')]='0'*40; self.assertTrue(any('git_history_drift' in x for x in self.validate()))
    def test_lineage_commit_cannot_be_relabelled(self): self.record['cex_lineage'][0]['commit']='0'*40; self.assertTrue(any('lineage_identity' in x for x in self.validate()))
    def test_lineage_file_blob_cannot_be_relabelled(self): self.record['cex_lineage'][1]['files']['src/lib.rs']='0'*40; self.assertTrue(any('lineage_file_identity' in x for x in self.validate()))
    def test_genesis_decision_is_mandatory(self): self.record['source_genesis']['decision']='missing.md'; self.assertIn('genesis_authority',self.validate())
    def test_external_prehistory_cannot_be_claimed_resolved(self): self.record['historical_external_context']['status']='resolved'; self.assertIn('historical_context_overclaim',self.validate())
    def test_current_record_must_equal_terminal_lineage(self): self.record['current_git_tree']='0'*40; self.assertIn('current_record_identity',self.validate())
    def test_future_update_policy_is_required(self): self.record['future_update_policy']=''; self.assertIn('future_update_policy',self.validate())

if __name__=='__main__': unittest.main(verbosity=2)
