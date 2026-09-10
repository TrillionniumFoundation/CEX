#!/usr/bin/env python3
"""Source-level projection boundary regressions; no runtime permissions claimed."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC=importlib.util.spec_from_file_location('projection_checker',Path(__file__).with_name('check-consumer-projection-boundary.py'))
assert SPEC and SPEC.loader
C=importlib.util.module_from_spec(SPEC); SPEC.loader.exec_module(C)


class ProjectionRouteTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name)
        self.path=self.root/'services/consumer-entry-api/src/world_routes.rs'
        self.path.parent.mkdir(parents=True)
        p=patch.object(C,'ROOT',self.root);p.start();self.addCleanup(p.stop)

    def check(self,source):
        self.path.write_text(source)
        return C.check_file(self.path)

    def test_world_projection_routes_remain_allowed(self):
        self.assertEqual(self.check('.route("/v1/world/:id", get(h))'),[])

    def test_admin_route_is_rejected(self):
        self.assertTrue(self.check('.route("/v1/admin", get(h))'))

    def test_escaped_admin_cannot_bypass_the_route_guard(self):
        self.assertTrue(self.check(r'.route("/v1/\u{61}dmin", get(h))'))

    def test_hex_escape_cannot_bypass_the_route_guard(self):
        self.assertTrue(self.check(r'.route("/v1/\x61dmin", get(h))'))

    def test_raw_string_authority_path_is_rejected(self):
        self.assertTrue(self.check('.route(r###"/v1/admin"###, get(h))'))

    def test_service_and_nest_service_are_checked(self):
        for name in ['route_service','nest_service','nest']:
            self.assertTrue(self.check(f'.{name}("/v1/admin", handler)'))

    def test_raw_identifier_registration_is_checked(self):
        self.assertTrue(self.check('.r#route("/v1/admin", get(h))'))

    def test_dynamic_path_requires_resolution(self):
        self.assertTrue(self.check('.route(PATH, get(h))'))

    def test_route_comment_does_not_register_an_endpoint(self):
        self.assertEqual(self.check('// .route("/v1/admin", post(h))\n.route("/v1/world",get(h));'),[])

    def test_text_with_route_shape_does_not_register_endpoint(self):
        self.assertEqual(self.check('let s=r##".route("/v1/admin", post(h))"##;'),[])

    def test_financial_mutation_guard_is_preserved(self):
        self.assertTrue(self.check('sqlx::query("update ledger_accounts set x=1");'))

    def test_allowed_projection_mutation_remains_allowed(self):
        self.assertEqual(
            self.check('sqlx::query("update world_projection_rows set x=1");'),
            [],
        )

    def test_on_conflict_update_set_is_not_a_table_named_set(self):
        self.assertEqual(
            self.check(
                'sqlx::query("insert into league_players (id) values (1) '
                'on conflict (id) do update set id=excluded.id");'
            ),
            [],
        )

    def test_rust_comments_and_sql_comments_cannot_manufacture_mutations(self):
        self.assertEqual(
            self.check(
                '// UPDATE ledger_accounts SET amount=0\n'
                'sqlx::query(r#"-- UPDATE ledger_accounts SET amount=0\nselect 1"#);'
            ),
            [],
        )

    def test_update_prose_is_not_a_sql_table_reference(self):
        self.assertEqual(
            self.check('let note="UPDATE that would trip the append-only guard";'),
            [],
        )

    def test_slash_separated_role_prose_is_not_an_endpoint(self):
        self.assertEqual(
            self.check('let note="Assign scout/build/audit/close roles";'),
            [],
        )

    def test_internal_authority_endpoint_literals_are_rejected(self):
        for source in (
            'let endpoint="/v1/ledger/entries";',
            'let endpoint=r#"https://internal.example/provider/status"#;',
            'let endpoint="{base}/v2/chain-finality/latest";',
        ):
            with self.subTest(source=source):
                self.assertTrue(self.check(source))

    def test_authoritative_json_tokens_and_embedded_json_are_rejected(self):
        for source in (
            'let value=json!({"authoritative": true});',
            'let value=json!({"production_authorization": "granted"});',
            'let value=r#"{\"ledger_settled\":true}"#;',
        ):
            with self.subTest(source=source):
                self.assertTrue(self.check(source))

    def test_authoritative_shapes_in_comments_are_ignored(self):
        self.assertEqual(
            self.check('// json!({"authoritative": true});\nlet value=false;'),
            [],
        )

    def test_invalid_rust_does_not_silently_skip(self):
        with self.assertRaises(AssertionError): self.check('.route("PRIVATE_SECRET')

    def test_symlink_source_is_not_read(self):
        target=self.root/'outside.txt'; target.write_text('private')
        self.path.symlink_to(target)
        with self.assertRaises(AssertionError): C.check_file(self.path)


if __name__=='__main__':
    unittest.main(verbosity=2)
