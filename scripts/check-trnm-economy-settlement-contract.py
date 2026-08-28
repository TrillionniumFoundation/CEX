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

REQUIRED_FILES = (
    ROOT / "migrations/0010_trnm_economy_settlement_v1.sql",
    ROOT / "services/trnm-economy-service/src/api.rs",
    ROOT / "services/trnm-economy-service/src/config.rs",
    ROOT / "services/trnm-economy-service/src/contract.rs",
    ROOT / "services/trnm-economy-service/src/repository.rs",
    ROOT / "services/trnm-economy-service/tests/settlement_contract.rs",
    ROOT / "docs/trnm-economy-settlement-receipt-lookup-v1.md",
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

migration = REQUIRED_FILES[0].read_text(encoding="utf-8")
api = REQUIRED_FILES[1].read_text(encoding="utf-8")
config = REQUIRED_FILES[2].read_text(encoding="utf-8")
repository = REQUIRED_FILES[4].read_text(encoding="utf-8")
tests = REQUIRED_FILES[5].read_text(encoding="utf-8")

for token in (
    "trnm_economy_settlement_receipts_v1",
    "trnm_economy_reward_budget_v1",
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

require("pg_advisory_xact_lock" in repository, "intent serialization lock missing")
require("for update" in repository.lower(), "account/budget row lock missing")
require("reqwest" not in "\n".join(
    path.read_text(encoding="utf-8")
    for path in (REQUIRED_FILES[1], REQUIRED_FILES[2], REQUIRED_FILES[4])
), "owner settlement service must not call another external settlement authority")
require("constant_time_equal" in config, "credential digest comparison missing")

for token in (
    "response",
    "concurrent",
    "wrong-audience",
    "invalid_value_entitlement_signature",
    "daily_reward_limit_exceeded",
):
    require(token in tests, f"mandatory PostgreSQL fixture missing: {token}")

require(re.fullmatch(r"[0-9a-f]{40}", status["base_commit"]) is not None, "invalid base commit")
print("TRNM CEX settlement source/status contract: PASS")
