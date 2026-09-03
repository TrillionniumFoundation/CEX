#!/usr/bin/env python3
"""Regression tests for the Matrix transport durability source contract."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from matrix_transport_contract import validate_document, validate_sql  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
MIGRATION = ROOT / "services/matrix-entry-adapter/migrations/0001_transport_durability.sql"
DOCUMENT = ROOT / "docs/matrix-transport-durability-v1.md"


class MatrixTransportContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.sql = MIGRATION.read_text(encoding="utf-8")
        cls.document = DOCUMENT.read_text(encoding="utf-8")

    def assert_problem(self, mutated: str, needle: str) -> None:
        problems = validate_sql(mutated)
        self.assertTrue(
            any(needle in problem for problem in problems),
            f"expected {needle!r} in {problems!r}",
        )

    def test_current_migration_and_document_pass(self) -> None:
        self.assertEqual(validate_sql(self.sql), [])
        self.assertEqual(validate_document(self.document), [])

    def test_missing_cursor_table_fails(self) -> None:
        self.assert_problem(
            self.sql.replace(
                "create table if not exists public.matrix_transport_cursors",
                "create table if not exists public.matrix_transport_cursor_typo",
                1,
            ),
            "matrix_transport_cursors",
        )

    def test_missing_search_path_fails(self) -> None:
        self.assert_problem(
            self.sql.replace("set search_path = pg_catalog, public", "", 1),
            "pinned search_path",
        )

    def test_missing_revoke_fails(self) -> None:
        self.assert_problem(
            self.sql.replace(
                "revoke all on function public.cex_matrix_lookup_delivery_v1(uuid, text) from public;",
                "",
                1,
            ),
            "cex_matrix_lookup_delivery_v1",
        )

    def test_claim_requires_skip_locked(self) -> None:
        self.assert_problem(
            self.sql.replace("for update skip locked", "for update", 1),
            "for update skip locked",
        )

    def test_claim_history_requires_previous_status(self) -> None:
        self.assert_problem(
            self.sql.replace("selected.previous_status", "'pending'", 1),
            "pre-claim status",
        )

    def test_response_loss_lookup_requires_payload_hash(self) -> None:
        self.assert_problem(
            self.sql.replace(
                "and outbox.payload_sha256 = p_payload_sha256",
                "and true",
                1,
            ),
            "payload hash",
        )

    def test_outbox_identity_guard_is_required(self) -> None:
        self.assert_problem(
            self.sql.replace(
                "before update on public.matrix_transport_outbox",
                "after update on public.matrix_transport_outbox",
                1,
            ),
            "identity guard",
        )

    def test_atomic_register_must_advance_cursor(self) -> None:
        self.assert_problem(
            self.sql.replace(
                "cex_matrix_advance_cursor_v1(",
                "cex_matrix_advance_cursor_removed_v1(",
                1,
            ),
            "cex_matrix_advance_cursor_v1(",
        )

    def test_migration_must_be_transactional(self) -> None:
        self.assert_problem(self.sql.replace("begin;", "", 1), "transaction")

    def test_destructive_history_statement_fails(self) -> None:
        self.assert_problem(
            self.sql.replace(
                "commit;",
                "delete from public.matrix_transport_delivery_history;\ncommit;",
                1,
            ),
            "forbidden Matrix durability marker",
        )

    def test_document_cannot_grant_production(self) -> None:
        problems = validate_document(
            self.document.replace(
                "Production authorization: `not_granted`",
                "Production authorization: `granted`",
                1,
            )
        )
        self.assertTrue(any("not_granted" in problem for problem in problems))


if __name__ == "__main__":
    unittest.main(verbosity=2)
