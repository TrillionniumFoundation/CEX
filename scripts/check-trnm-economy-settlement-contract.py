#!/usr/bin/env python3
"""Fail-closed source/status gate for the CEX-owned settlement receipt contract."""

from __future__ import annotations

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
STATUS = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else (
    ROOT / "docs/status/trnm-economy-settlement-v1.json"
)

MIGRATION = ROOT / "migrations/0010_trnm_economy_settlement_v1.sql"
API = ROOT / "services/trnm-economy-service/src/api.rs"
CONFIG = ROOT / "services/trnm-economy-service/src/config.rs"
CONTRACT = ROOT / "services/trnm-economy-service/src/contract.rs"
REPOSITORY = ROOT / "services/trnm-economy-service/src/repository.rs"
OWNER_TEST = ROOT / "services/trnm-economy-service/tests/settlement_contract.rs"
DURABLE_BYTES_TEST = (
    ROOT / "services/trnm-economy-service/tests/durable_bytes_immutability.rs"
)
DOC = ROOT / "docs/trnm-economy-settlement-receipt-lookup-v1.md"

REQUIRED_FILES = (
    MIGRATION,
    API,
    CONFIG,
    CONTRACT,
    REPOSITORY,
    OWNER_TEST,
    DURABLE_BYTES_TEST,
    DOC,
)


def fail(message: str) -> None:
    raise SystemExit(message)


def require(condition: bool, message: str) -> None:
    if not condition:
        fail(message)


status = json.loads(STATUS.read_text(encoding="utf-8"))
require(status["schema"] == "trnm_cex_settlement_runtime_status_v1", "invalid status schema")
require(status["owner_repository"] == "TrillionniumFoundation/CEX", "wrong owner repository")
require(status["issue"] == 6, "CEX owner issue must remain #6")
require(status["status"] == "implemented_pending_exact_commit_ci", "status overclaim")
require(status["verified_commit"] is None, "unverified source cannot name a verified commit")
require(status["release_effect"] == "none", "candidate source cannot claim release effect")
require(status["trusted_settlement"] is False, "candidate source cannot enable trusted settlement")
require(status["public_online"] is False, "candidate source cannot enable public online")
require(status["public_player_market"] is False, "candidate source cannot enable public market")
require(status["evidence"]["remote_workflow_runs"] == [], "source status cannot self-embed future workflow runs")
require(status["evidence"]["artifacts"] == [], "source status cannot self-embed future artifacts")
require(status["evidence"]["reviewers"] == [], "source status cannot invent reviewer evidence")
require("obtain_exact_commit_github_actions_evidence" in status["open_gates"], "CI gate missing")
require("merge_owner_repository_pull_request" in status["open_gates"], "owner merge gate missing")
require("bind_exact_cex_revision_in_trillionnium_integration" in status["open_gates"], "component lock gate missing")

for path in REQUIRED_FILES:
    require(path.is_file(), f"required settlement file missing: {path.relative_to(ROOT)}")

migration = MIGRATION.read_text(encoding="utf-8")
api = API.read_text(encoding="utf-8")
config = CONFIG.read_text(encoding="utf-8")
repository = REPOSITORY.read_text(encoding="utf-8")
tests = "\n".join(
    path.read_text(encoding="utf-8") for path in (OWNER_TEST, DURABLE_BYTES_TEST)
)

for token in (
    "trnm_economy_settlement_receipts_v1",
    "trnm_economy_reward_budget_v1",
    "intent_bytes bytea not null",
    "digest(intent_bytes, 'sha256')",
    "convert_from(intent_bytes, 'utf8')::jsonb = intent_json",
    "before update or delete",
    "before truncate",
    "errcode = '55000'",
    "on delete restrict",
):
    require(token in migration.lower(), f"migration invariant missing: {token}")

for token in (
    "/v1/trnm/economy/intents",
    "/v1/trnm/economy/receipts/by-intent",
    "/v1/trnm/economy/issuer-keys/status",
    "intent_hash_conflict",
    "wrong_game_authority_audience",
):
    require(token in api, f"HTTP contract token missing: {token}")

for token in (
    "pg_advisory_xact_lock",
    "serde_json::to_vec(intent)",
    "sha256::digest(&intent_bytes)",
    "existing.intent_bytes != intent_bytes",
    "stored intent bytes and json projection diverge",
    "stored receipt is not bound to exact durable intent bytes",
):
    require(token in repository.lower(), f"durable repository invariant missing: {token}")

require("for update" in repository.lower(), "account/budget row lock missing")
require(
    "reqwest"
    not in "\n".join(
        path.read_text(encoding="utf-8") for path in (API, CONFIG, REPOSITORY)
    ),
    "owner settlement service must not call another external settlement authority",
)
require("constant_time_equal" in config, "credential digest comparison missing")

for token in (
    "response",
    "concurrent",
    "wrong-audience",
    "invalid_value_entitlement_signature",
    "daily_reward_limit_exceeded",
    "exact_intent_bytes_are_durable_hash_checked_and_append_only",
    "database must reject bytes/hash mismatch",
    "append-only receipt row must reject update",
    "append-only receipt row must reject delete",
    "append-only receipt table must reject truncate",
):
    require(token in tests, f"mandatory PostgreSQL fixture missing: {token}")

for control in (
    "exact_serialized_intent_bytes_are_durable",
    "database_recomputes_intent_sha256",
    "receipt_rows_reject_update_delete_and_truncate",
    "runtime_revalidates_durable_bytes_hash_and_json",
):
    require(control in status["implemented_controls"], f"status control missing: {control}")

require(re.fullmatch(r"[0-9a-f]{40}", status["base_commit"]) is not None, "invalid base commit")
print("TRNM CEX settlement source/status contract: PASS")
