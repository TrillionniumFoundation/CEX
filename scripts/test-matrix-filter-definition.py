#!/usr/bin/env python3
"""Mutation negatives for current source wiring; no compiler/database is simulated as real."""
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location('pin_guard', Path(__file__).with_name('check-matrix-filter-definition.py'))
assert SPEC and SPEC.loader
C = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(C)

class PinSourceTests(unittest.TestCase):
    def setUp(self): self.s = C.sources()
    def reject(self, key, before, after):
        self.assertIn(before, self.s[key])
        self.s[key] = self.s[key].replace(before, after, 1)
        with self.assertRaises(AssertionError): C.validate(self.s)
    def test_current_source(self): C.validate(self.s)
    def test_resolve_precedes_polling(self):
        self.reject(C.POLLER, 'config.resolved_filter = filter_definition::resolve(', 'skip_resolution(')
    def test_identity_before_definition(self):
        self.reject(C.POLLER, 'verify_homeserver_account(&http, &config).await?', 'skip_account()')
    def test_pin_is_explicit_configuration(self):
        self.reject(C.POLLER, 'env::var("MATRIX_SYNC_FILTER_DEFINITION_SHA256")', 'None')
    def test_authentication_not_lost(self):
        self.reject(C.RESOLVER, '.bearer_auth(token)', '.no_auth()')
    def test_entire_fetch_has_deadline(self):
        self.reject(C.RESOLVER, 'timeout(Duration::from_secs(10)', 'timeout(Duration::from_secs(999)')
    def test_http_200_required(self):
        self.reject(C.RESOLVER, 'response.status() != StatusCode::OK', 'false')
    def test_body_is_bounded(self):
        self.reject(C.RESOLVER, 'crate::read_bounded_body(response, DEFINITION_MAX_BYTES)', 'response.bytes()')
    def test_digest_checked(self):
        self.reject(C.RESOLVER, 'if digest != expected', 'if false')
    def test_matching_hash_still_requires_shape(self):
        self.reject(C.RESOLVER, 'stream_scope::validate_inline(raw)?', 'ignore_shape()')
    def test_ids_never_fall_back_to_unfiltered(self):
        self.reject(C.RESOLVER, 'Err("matrix_filter_definition_unresolved")', 'Ok(None)')
    def test_sync_uses_effective_definition(self):
        # Change only build_sync_url, not another call with the same name.
        body=C.function(self.s[C.POLLER], 'build_sync_url')
        self.s[C.POLLER]=self.s[C.POLLER].replace(body, body.replace('filter_definition::effective_filter(', 'skip_pin('), 1)
        with self.assertRaises(AssertionError):C.validate(self.s)
    def test_gap_uses_effective_definition(self):
        self.reject(C.POLLER, 'let effective = filter_definition::effective_filter(', 'let effective = skip_pin(')
    def test_definition_binding_precedes_cursor_request(self):
        self.reject(C.POLLER, 'bind_stream_scope(&mut scope_tx, config, &lease).await?', 'skip_binding()')
    def test_binding_checks_exact_bytes(self):
        self.reject(C.SQL, 'bound.definition is distinct from p_definition', 'false')
    def test_database_recomputes_digest(self):
        self.reject(C.SQL, "encode(sha256(convert_to(definition, 'UTF8')), 'hex')", "'unverified'")
    def test_exact_legacy_position_required(self):
        self.reject(C.SQL, 'cursor_row.opaque_cursor is distinct from p_expected_cursor', 'false')
    def test_insert_trigger_checks_owner(self):
        self.reject(C.SQL, 'current_user <> table_owner', 'false')
    def test_approval_function_checks_owner_independently(self):
        start=self.s[C.SQL].index('create or replace function public.cex_matrix_approve_legacy_filter_definition_v1')
        self.s[C.SQL]=self.s[C.SQL][:start]+self.s[C.SQL][start:].replace('current_user <> table_owner','false',1)
        with self.assertRaises(AssertionError): C.validate(self.s)
    def test_null_selector_is_not_absence(self):
        self.reject(C.SCOPE, '#[serde(default, deserialize_with = "present_non_null")]','#[serde(default)]')
    def test_migration_is_in_full_runner(self):
        self.reject(C.DRIVER, '0005_filter_definition_pins.sql', 'omit-new-migration.sql')
    def test_table_is_in_exact_reset_inventory(self):
        self.reject(C.DRIVER, '"matrix_transport_filter_definitions"', '"other"')
    def test_comment_cannot_replace_executable_hash_check(self):
        self.reject(C.RESOLVER, 'if digest != expected', '// if digest != expected\n    if false')
    def test_string_cannot_replace_executable_auth(self):
        self.reject(C.RESOLVER, '.bearer_auth(token)', '.header("fixture", ".bearer_auth(token)")')
    def test_codegen_maintains_old_runtime_gates(self):
        command_prefix = 'cargo ' + 'test --locked -p ' + 'matrix-'
        for path in C.WORKFLOWS:
            self.assertIn(command_prefix,self.s[path])
            self.assertNotIn('continue-on-error: true',self.s[path])
            self.assertNotIn('contents: write',self.s[path])

if __name__ == '__main__': unittest.main(verbosity=2)
