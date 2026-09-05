#!/usr/bin/env python3
"""Pure fixture tests for the vendor provenance checker; no upstream or Cargo execution."""
from __future__ import annotations
import hashlib, importlib.util, tempfile, unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    'vendor_provenance', Path(__file__).with_name('check-vendor-provenance.py')
)
assert SPEC and SPEC.loader
V = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(V)


class VendorProvenanceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        source = self.root / 'vendor/pkg/src'
        source.mkdir(parents=True)
        (source / 'lib.rs').write_bytes(b'origin')
        (source / 'test.rs').write_bytes(b'patched')
        self.manifest = {
            'schema': 'hepta.vendor.trnm_chain_crates.v1',
            'crates': {'pkg': {
                'source_git_tree': 'a' * 40,
                'files': {
                    'src/lib.rs': hashlib.sha256(b'origin').hexdigest(),
                    'src/test.rs': hashlib.sha256(b'upstream').hexdigest(),
                },
            }},
        }
        self.ledger = {
            'schema': 'cex.trnm-chain-downstream-patches.v1',
            'source_manifest': V.MANIFEST.as_posix(),
            'production_authorization': 'not_granted',
            'patches': [{
                'package': 'pkg', 'path': 'src/test.rs',
                'source_git_tree': 'a' * 40, 'source_git_blob': 'b' * 40,
                'source_sha256': hashlib.sha256(b'upstream').hexdigest(),
                'vendored_git_tree': 'c' * 40,
                'vendored_git_blob': V.git_blob_sha(b'patched'),
                'introduced_by_cex_commit': 'd' * 40,
                'scope': 'test_only', 'runtime_behavior_changed': False,
                'upstream_identity_unchanged': True,
                'required_rebase_disposition': 'remove when upstream contains it',
            }],
        }

    def validate(self, tree='c' * 40):
        return V.validate(self.root, self.manifest, self.ledger, lambda *_: tree)

    def test_valid_exact_patch(self):
        self.assertEqual(self.validate(), [])

    def test_unlisted_source_mutation_fails(self):
        (self.root / 'vendor/pkg/src/lib.rs').write_bytes(b'x')
        self.assertIn('vendor_sha256_mismatch:pkg/src/lib.rs', self.validate())

    def test_patched_blob_mutation_fails(self):
        (self.root / 'vendor/pkg/src/test.rs').write_bytes(b'x')
        self.assertIn('patched_blob_mismatch:pkg/src/test.rs', self.validate())

    def test_extra_file_fails(self):
        (self.root / 'vendor/pkg/src/extra.rs').write_bytes(b'x')
        self.assertIn('vendor_file_set_mismatch:pkg', self.validate())

    def test_unknown_patch_fails(self):
        self.ledger['patches'][0]['path'] = 'src/missing.rs'
        self.assertTrue(any('unknown_manifest_file' in item for item in self.validate()))

    def test_runtime_patch_is_not_approved_by_this_policy(self):
        self.ledger['patches'][0]['runtime_behavior_changed'] = True
        self.assertTrue(any('runtime_patch_not_allowed' in item for item in self.validate()))

    def test_non_test_patch_is_not_approved_by_this_policy(self):
        self.ledger['patches'][0]['scope'] = 'runtime'
        self.assertTrue(any('non_test_patch_not_allowed' in item for item in self.validate()))

    def test_original_digest_remains_bound(self):
        self.ledger['patches'][0]['source_sha256'] = '0' * 64
        self.assertTrue(any('source_digest_mismatch' in item for item in self.validate()))

    def test_current_tree_remains_bound(self):
        self.assertTrue(any('patched_tree_mismatch' in item for item in self.validate('e' * 40)))


if __name__ == '__main__':
    unittest.main(verbosity=2)
