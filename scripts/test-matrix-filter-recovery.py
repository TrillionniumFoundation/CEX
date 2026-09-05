#!/usr/bin/env python3
"""Source mutation and lexical-boundary tests only, not Rust/Matrix runtime tests."""
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location('filter_guard', Path(__file__).with_name('check-matrix-filter-recovery.py'))
assert SPEC and SPEC.loader
C = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(C)


class FilterRecoverySourceTests(unittest.TestCase):
    def setUp(self):
        self.sources = C.sources()

    def reject(self, key, old, new):
        self.assertIn(old, self.sources[key])
        self.sources[key] = self.sources[key].replace(old, new, 1)
        with self.assertRaises(AssertionError):
            C.validate(self.sources)

    def test_current_sources(self):
        C.validate(self.sources)

    def test_startup_validates_inline_filter(self):
        self.reject('scope', 'validate_inline(raw)?', 'ignore_invalid_filter()')

    def test_server_id_does_not_default_to_no_filter(self):
        self.reject('policy', 'return Err("matrix_gap_filter_id_requires_resolution");', 'return Ok(None);')

    def test_top_level_room_exclusion_cannot_be_removed(self):
        self.reject('policy', 'room_allowed(room_id, &room.rooms, &room.not_rooms)', 'true')

    def test_timeline_room_exclusion_cannot_be_removed(self):
        self.reject('policy', 'room_allowed(room_id, &timeline.rooms, &timeline.not_rooms)', 'true')

    def test_filter_is_serialized_not_discarded(self):
        self.reject('policy', 'serde_json::to_string(&timeline)', 'serde_json::to_string(&RoomEventFilter::default())')

    def test_backfill_calls_projection_before_network(self):
        self.reject('poller', 'stream_scope::backfill_filter(effective, room_id)', 'ignore_filter()')

    def test_backfill_url_receives_filter_argument(self):
        self.reject('poller', 'message_filter.as_deref(),', 'None,')

    def test_query_filter_cannot_be_removed(self):
        self.reject('recovery', 'url.query_pairs_mut().append_pair("filter", filter)', 'url.query_pairs_mut().append_pair("ignored", filter)')

    def test_projection_fields_cannot_be_silently_accepted(self):
        self.reject('policy', 'struct SyncFilter {', 'struct SyncFilter {\n    event_fields: Option<Vec<String>>,')

    def test_marker_in_test_function_does_not_satisfy_runtime_check(self):
        self.sources['recovery'] += '\nfn fixture_only() { url.query_pairs_mut().append_pair("filter", filter); }\n'
        self.reject('recovery', 'url.query_pairs_mut().append_pair("filter", filter)', 'ignore_filter()')

    def test_function_extraction_ignores_comment_and_string_impostors(self):
        text = '/* fn wanted() { bad(); } */\nfn other(){ let s=r##"fn wanted() { bad(); }"##; }\nfn wanted(){ good(); }'
        self.assertEqual(C.function_body(text, 'wanted').strip(), 'good();')


if __name__ == '__main__':
    unittest.main(verbosity=2)
