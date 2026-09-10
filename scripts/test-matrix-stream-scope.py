#!/usr/bin/env python3
"""Mutation negatives for source guards, not Rust/SQL behavioral execution."""
from pathlib import Path
import importlib.util
import unittest

SPEC = importlib.util.spec_from_file_location('stream_scope_source', Path(__file__).with_name('check-matrix-stream-scope.py'))
assert SPEC and SPEC.loader
C = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(C)

class StreamScopeSourceTests(unittest.TestCase):
    def setUp(self): self.files = C.sources()
    def reject(self, path, old, new):
        self.assertIn(old, self.files[path])
        self.files[path] = self.files[path].replace(old, new, 1)
        with self.assertRaises(AssertionError): C.validate(self.files)
    def test_current_source(self): C.validate(self.files)
    def test_account_verification_precedes_polling(self):
        self.reject(C.POLLER, 'verify_homeserver_account(&http, &config).await?', 'skip_account_check()')
    def test_runtime_filter_cannot_silently_drop_zero_or_invalid_values(self):
        self.reject(C.POLLER, 'stream_scope::configured_filter(env::var("MATRIX_SYNC_FILTER"))', 'ignore_runtime_filter()')
    def test_cursor_scope_is_bound_before_http(self):
        self.reject(C.POLLER, 'bind_stream_scope(&mut scope_tx, config, &lease).await?', 'skip_scope_binding()')
    def test_scope_is_checked_in_admission_transaction(self):
        self.reject(C.POLLER, 'bind_stream_scope(&mut tx, config, lease).await?', 'skip_admission_scope()')
    def test_account_reply_must_match_configured_user(self):
        self.reject(C.SCOPE, 'body.get("user_id").and_then(Value::as_str) != Some(expected)', 'false')
    def test_exact_filter_bytes_are_not_silently_normalized(self):
        self.reject(C.SCOPE, '"kind": "inline", "value": raw', '"kind": "inline", "value": value')
    def test_legacy_scope_cannot_be_auto_approved(self):
        self.reject(C.SQL, 'matrix_stream_scope_legacy_review_required', 'skip_review')
    def test_owner_boundary_cannot_be_removed(self):
        self.reject(C.SQL, 'current_user <> scope_owner', 'false')
    def test_legacy_cursor_cannot_be_guessed(self):
        self.reject(C.SQL, 'cursor_row.opaque_cursor is distinct from p_expected_cursor', 'false')
    def test_migration_cannot_escalate_runtime_privileges(self):
        self.reject(C.SQL, 'language plpgsql', 'language plpgsql security definer')
    def test_sql_suite_must_be_in_current_chain(self):
        self.reject(C.DRIVER, '0004_stream_scope_binding.sql', 'skip-scope-migration.sql')
    def test_reply_cannot_carry_undeclared_control_fields(self):
        self.reject(
            C.REPLY,
            '.all(|key| matches!(key.as_str(), "msgtype" | "body"))',
            '.any(|_| true)',
        )

if __name__ == '__main__': unittest.main(verbosity=2)
