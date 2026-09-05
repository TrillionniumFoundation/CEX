#!/usr/bin/env python3
"""Behavioral fixtures for the Rust declaration extractor (not router runtime)."""
from __future__ import annotations

from pathlib import Path
import random
import unittest
from unittest.mock import patch

import rust_route_contract as R


class RouteExtractionTests(unittest.TestCase):
    def one(self, text):
        rows = R.extract_routes(text)
        self.assertEqual(len(rows), 1)
        return rows[0]

    def test_adjacent_methods_do_not_bleed(self):
        rows = R.extract_routes('Router::new().route("/a", get(a)).route("/b", post(b));')
        self.assertEqual([(row.value, row.methods) for row in rows], [('/a', ('GET',)), ('/b', ('POST',))])

    def test_long_route_has_no_arbitrary_lookahead_limit(self):
        row = self.one('.route("/a", get(' + 'handler_' + 'a' * 2000 + ').post(post_handler))')
        self.assertEqual(row.methods, ('GET', 'POST'))

    def test_nested_handler_calls_do_not_create_methods(self):
        row = self.one('.route("/a", get(|| async { let v = post(x); cache.get(key); v }))')
        self.assertEqual(row.methods, ('GET',))

    def test_comments_do_not_add_fake_routes_or_methods(self):
        source = '// .route("/false", post(x))\n.route("/a", get(x) /* .delete(y) */)'
        self.assertEqual(self.one(source).methods, ('GET',))

    def test_nested_block_comments_are_ignored(self):
        self.assertEqual(self.one('/* outer /* .route("/x", post(h)) */ */.route("/a", get(h))').value, '/a')

    def test_strings_cannot_inject_routes(self):
        text = r'''let x = ".route(\"/false\", post(x))"; let y = r###".route("/fake", delete(h))"###; .route("/real", get(h));'''
        self.assertEqual(self.one(text).value, '/real')

    def test_raw_string_hash_levels_and_quotes(self):
        for count in range(6):
            marks = '#' * count
            path = '/a"b' if count else '/ab'
            row = self.one(f'.route(r{marks}"{path}"{marks}, get(h))')
            self.assertEqual(row.value, path)

    def test_byte_strings_are_not_str_paths(self):
        row = self.one('.route(b"/bytes", get(h))')
        self.assertIsNone(row.value)
        self.assertEqual(row.path_resolution, 'dynamic')

    def test_raw_bytes_and_c_strings_cannot_inject(self):
        text = 'let b=br###".route("/false", post(x))"###; let c=c"quoted"; .route("/a", get(h));'
        self.assertEqual(self.one(text).value, '/a')

    def test_chars_and_lifetimes_do_not_break_delimiters(self):
        text = "fn f<'a>(x: &'a str) { let c = '('; let d = b')'; let e = '\\''; Router::new().route(\"/a\", get(h)); }"
        self.assertEqual(self.one(text).methods, ('GET',))

    def test_all_explicit_http_methods_and_service_constructors(self):
        for method in sorted(R.METHODS):
            for suffix in ['', '_service']:
                row = self.one(f'.route("/a", {method.lower()}{suffix}(h))')
                self.assertEqual(row.methods, (method,))

    def test_qualified_axum_constructors(self):
        row = self.one('.route("/a", axum::routing::post(h).get(h2))')
        self.assertEqual(row.methods, ('GET', 'POST'))

    def test_unknown_qualified_constructor_is_unresolved(self):
        row = self.one('.route("/a", custom::get(h))')
        self.assertEqual(row.methods, ('UNRESOLVED',))

    def test_parenthesized_path_and_router(self):
        row = self.one('.route((("/a")), ((get(h).post(h2))),)')
        self.assertEqual((row.value, row.methods), ('/a', ('GET', 'POST')))

    def test_methodrouter_builder(self):
        row = self.one('.route("/a", axum::routing::MethodRouter::new().get(h).patch(h2))')
        self.assertEqual(row.methods, ('GET', 'PATCH'))

    def test_on_method_filter_union(self):
        row = self.one('.route("/a", on(MethodFilter::POST | MethodFilter::DELETE, h))')
        self.assertEqual(row.methods, ('DELETE', 'POST'))

    def test_chained_on_filter(self):
        row = self.one('.route("/a", get(h).on(MethodFilter::PATCH, h2))')
        self.assertEqual(row.methods, ('GET', 'PATCH'))

    def test_generic_handler_constructor_retains_unresolved_boundary(self):
        row = self.one('.route("/a", get::<Vec<A>, Result<B,C>>(h))')
        self.assertEqual(row.methods, ('UNRESOLVED',))

    def test_dynamic_method_filter_is_unresolved(self):
        row = self.one('.route("/a", on(filters, h))')
        self.assertEqual(row.methods, ('UNRESOLVED',))

    def test_layer_arguments_do_not_add_methods(self):
        row = self.one('.route("/a", get(h).layer(trace(post(x))).with_state(state.get(key)))')
        self.assertEqual(row.methods, ('GET',))

    def test_any_is_explicit_not_an_enumerated_guess(self):
        self.assertEqual(self.one('.route("/a", any(h))').methods, ('ANY',))

    def test_unresolved_helpers_never_claim_partial_coverage(self):
        row = self.one('.route("/a", build_router().post(h))')
        self.assertEqual(row.methods, ('UNRESOLVED',))
        self.assertEqual(row.observed_methods, ('POST',))

    def test_merge_and_fallback_are_not_guessed(self):
        for tail in ['merge(other)', 'fallback(other)', 'boxed()']:
            self.assertEqual(self.one('.route("/a", get(h).' + tail + ')').methods, ('UNRESOLVED',))

    def test_nest_does_not_claim_child_methods(self):
        rows = R.extract_routes('.nest("/api", Router::new().route("/a", post(h)))')
        self.assertEqual(rows[0].methods, ('UNRESOLVED',))
        self.assertEqual(rows[1].methods, ('POST',))
        self.assertEqual(rows[0].registration, 'nest')

    def test_route_service_does_not_borrow_methods_from_service(self):
        self.assertEqual(self.one('.route_service("/a", get_service(h))').methods, ('UNRESOLVED',))

    def test_dynamic_path_is_not_silently_omitted(self):
        for expr in ['PATH', 'concat!("/v1", "/a")', 'format!("/{}", variable)']:
            row = self.one('.route(' + expr + ', get(h))')
            self.assertIsNone(row.value)
            self.assertEqual(row.methods, ('GET',))

    def test_same_path_different_declarations_keep_provenance(self):
        rows = R.extract_routes('.route("/a", get(one));\n.route("/a", get(two));')
        self.assertEqual(len(rows), 2)
        self.assertNotEqual(rows[0].offset, rows[1].offset)
        self.assertNotEqual(rows[0].expression_sha256, rows[1].expression_sha256)

    def test_expression_boundaries_are_exact(self):
        source = 'prefix.route("/a", get(h)).route("/b", post(h))'
        first = R.extract_routes(source)[0]
        self.assertEqual(source[first.offset:first.end], '.route("/a", get(h))')

    def test_string_escape_decoding(self):
        self.assertEqual(self.one(r'.route("/\u{4e2d}/\x41/\\/\"", get(h))').value, '/中/A/\\/"')

    def test_multiline_continuation(self):
        self.assertEqual(self.one('.route("/a\\\n      /b", get(h))').value, '/a/b')

    def test_unterminated_inputs_fail_without_echoing_secrets(self):
        for source in ['.route("PRIVATE_SECRET', '/* PRIVATE_SECRET', 'r##"PRIVATE_SECRET', '.route("/a", get(h)']:
            with self.assertRaises(R.RouteSyntaxError) as failure:
                R.extract_routes(source)
            self.assertNotIn('PRIVATE_SECRET', str(failure.exception))

    def test_invalid_escapes_and_unicode_are_rejected(self):
        for path in [r'/\xFF', r'/\u{D800}', r'/\u{110000}', r'/\q']:
            with self.assertRaises(R.RouteSyntaxError):
                R.extract_routes('.route("' + path + '", get(h))')

    def test_malformed_registration_is_not_silently_accepted(self):
        for expr in ['.route("/a")', '.route("/a",,get(h))', '.route("/a",get(h),other)', '.route::<State>("/a",get(h))']:
            with self.assertRaises(R.RouteSyntaxError):
                R.extract_routes(expr)

    def test_byte_and_nesting_budgets(self):
        with patch.object(R, 'MAX_BYTES', 8), self.assertRaises(R.RouteSyntaxError):
            R.extract_routes('a' * 9)
        with patch.object(R, 'MAX_NESTING', 4), self.assertRaises(R.RouteSyntaxError):
            R.extract_routes('(' * 5 + ')' * 5)
        with patch.object(R, 'MAX_TOKENS', 4), self.assertRaises(R.RouteSyntaxError):
            R.extract_routes('a b c d e')

    def test_seeded_spacing_and_comment_perturbations(self):
        rng = random.Random(5404)
        tokens = ['.', 'route', '(', '"/one"', ',', 'get', '(', 'h', ')', ')', '.', 'route', '(', '"/two"', ',', 'post', '(', 'h', ')', ')']
        trivia = [' ', '\n', '/* ) post(x) */', '// .route("/fake", delete(x))\n']
        for _ in range(60):
            source = ''.join(token + rng.choice(trivia) for token in tokens)
            self.assertEqual([(row.value, row.methods) for row in R.extract_routes(source)], [('/one', ('GET',)), ('/two', ('POST',))])

    def test_real_relay_route_declarations(self):
        root = Path(__file__).resolve().parents[1]
        source = (root / 'apps/matrix-bot-relay/src/main.rs').read_text()
        declarations = R.extract_routes(source)
        routes = [(row.value, row.methods) for row in declarations]
        self.assertIn(('/health', ('GET',)), routes)
        self.assertIn(('/v1/inbound/matrix-event', ('POST',)), routes)


if __name__ == '__main__':
    unittest.main(verbosity=2)
