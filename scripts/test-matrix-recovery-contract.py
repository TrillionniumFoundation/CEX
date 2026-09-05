#!/usr/bin/env python3
"""Mutation-negative source-contract tests. These do not execute Rust or SQL."""
from __future__ import annotations
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('recovery_contract', ROOT / 'scripts/check-matrix-recovery-contract.py')
assert SPEC and SPEC.loader
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class RecoveryContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sources = CHECK.read_sources(ROOT)

    def mutation_rejected(self, path, old, new):
        changed = dict(self.sources)
        self.assertIn(old, changed[path])
        changed[path] = changed[path].replace(old, new, 1)
        with self.assertRaises(AssertionError):
            CHECK.validate(changed)

    def test_current_source_contract(self):
        CHECK.validate(self.sources)

    def test_missing_gap_recovery(self):
        self.mutation_rejected(CHECK.POLLER, 'recover_limited_timelines(pool, http, config, &lease, &mut body)', 'unverified_sync(body)')

    def test_missing_poison_snapshot(self):
        self.mutation_rejected(CHECK.POLLER, 'persist_poison_observations(pool, config, &lease, &admissions)', 'discard_poison(&admissions)')

    def test_poison_gate_removed(self):
        self.mutation_rejected(CHECK.POLLER, 'acknowledged_at is null', 'false')

    def test_pagination_cycle_detection_removed(self):
        self.mutation_rejected(CHECK.PAGER, 'self.seen.insert', 'untracked_cursor')

    def test_wrong_page_start_accepted(self):
        self.mutation_rejected(CHECK.PAGER, 'page.start != self.cursor', 'false')

    def test_missing_page_bound(self):
        self.mutation_rejected(CHECK.PAGER, 'page.chunk.len() > limit', 'false')

    def test_gap_network_inside_transaction(self):
        self.mutation_rejected(CHECK.POLLER, 'let deadline = Instant::now()', 'let tx = pool.begin().await?; let deadline = Instant::now()')

    def test_blind_adapter_retry_rejected(self):
        self.mutation_rejected(CHECK.RELAY, 'RemoteOutcome::Permanent("adapter_response_unknown_timeout")', 'RemoteOutcome::Retryable("adapter_response_unknown_timeout")')

    def test_unbound_send_rejected(self):
        self.mutation_rejected(CHECK.RELAY, 'select public.cex_matrix_bind_send_attempt_v1', 'select public.bypass_send_binding')

    def test_receipt_not_persisted(self):
        self.mutation_rejected(CHECK.RELAY, 'select public.cex_matrix_record_send_receipt_v1', 'select public.bypass_receipt')

    def test_cross_room_projection_rejected(self):
        self.mutation_rejected(CHECK.RELAY, 'let room_id = event.room_id.as_str()', 'let room_id = upstream["room_id"].as_str().unwrap()')

    def test_event_receipt_check_removed(self):
        self.mutation_rejected(CHECK.RESPONSE, "id.starts_with('$')", 'true')

    def test_error_envelope_not_success(self):
        self.mutation_rejected(CHECK.RESPONSE, 'receipt.get("errcode").is_none()', 'true')

    def test_expired_adapter_reclaim_rejected(self):
        self.mutation_rejected(CHECK.MIGRATION, "o.destination <> 'matrix-relay-adapter-v1'", 'true')

    def test_previous_claim_owner_retained(self):
        self.mutation_rejected(CHECK.MIGRATION, 'stale.previous_owner', 'o.lease_owner')

    def test_database_sent_receipt_guard_required(self):
        self.mutation_rejected(CHECK.MIGRATION, "raise exception 'matrix_send_receipt_required'", "raise exception 'bypass_receipt_guard'")

    def test_transaction_required(self):
        self.mutation_rejected(CHECK.MIGRATION, 'begin;', '-- no transaction')

    def test_no_definer_escalation(self):
        self.mutation_rejected(CHECK.MIGRATION, 'language plpgsql', 'language plpgsql security definer')

    def test_parser_drift_rejected(self):
        self.mutation_rejected('apps/matrix-bot-relay/src/runtime_profile.rs', '"stage" | "staging"', '"stage"')

    def test_skipping_ci_rejected(self):
        self.mutation_rejected('.github/workflows/matrix-review-repair-regression.yml', 'contents: read', 'contents: read\n  continue-on-error: true')

    def test_catalog_missing_source_documentation(self):
        self.mutation_rejected('docs/modules/matrix-bot-poller.md', '`apps/matrix-bot-poller/src/sync_recovery.rs`', '`undocumented.rs`')

    def test_existing_database_regression_retained(self):
        self.mutation_rejected('scripts/matrix_postgres_regression.py', 'check-matrix-transport-postgres.sh', 'skip-old-tests.sh')

    def test_new_database_regression_required(self):
        self.mutation_rejected('scripts/matrix_postgres_regression.py', 'test-matrix-sync-recovery-postgres.sql', 'skip-new-tests.sql')

    def test_adapter_response_identity_validation_is_not_optional(self):
        self.mutation_rejected(CHECK.RELAY, 'response_contract::validate_adapter_response', 'response_contract::unverified_response')

    def test_duplicate_cache_hit_cannot_claim_completion(self):
        self.mutation_rejected(CHECK.RESPONSE, 'adapter_duplicate_outcome_unknown', 'accept_cached_event')

    def test_status_response_binds_its_requested_task(self):
        self.mutation_rejected(CHECK.RESPONSE, 'adapter_response_event_mismatch', 'ignore_task_identity')

    def test_null_no_reply_is_preserved(self):
        self.mutation_rejected(CHECK.RESPONSE, 'None | Some(Value::Null) => Ok(None)', 'None => Ok(None)')

    def test_reverse_proxy_prefix_is_preserved(self):
        self.mutation_rejected(CHECK.RELAY, "url.path().trim_end_matches('/')", '""')


if __name__ == '__main__':
    unittest.main(verbosity=2)
