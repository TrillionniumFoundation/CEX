#!/usr/bin/env python3
"""Fail closed when the CEX-owned TRNM receipt recovery contract drifts."""

from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CONTRACT = "trnm_cex_settlement_receipt_lookup_v1"
ENDPOINT = "/v1/trnm/economy/receipts/by-intent"


def read(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def require(problems: list[str], condition: bool, message: str) -> None:
    if not condition:
        problems.append(message)


def compact_sql(value: str) -> str:
    return re.sub(r"\s+", " ", value.lower())


def main() -> int:
    problems: list[str] = []
    module = read("services/ledger-service/src/receipt_lookup.rs")
    router = read("services/ledger-service/src/lib.rs")
    native = read("services/ledger-service/src/repository/native_economy.rs")
    migration = read("migrations/0027_add_trnm_native_economy_persistence.sql")
    evidence_migration = read("migrations/0086_add_trnm_native_receipt_evidence.sql")
    normalized_event_migration = read(
        "migrations/0087_add_term_exchange_receipt_event_history.sql"
    )
    contract = read("docs/protocol/trnm-settlement-receipt-recovery-v1.md")
    blackbox = read("scripts/check-trnm-settlement-receipt-lookup-http.sh")
    migration_gate = read(".github/workflows/p0-migration-gate.yml")
    candidate_gate = read(".github/workflows/p0-release-candidate-gate.yml")

    require(problems, f'"{ENDPOINT}"' in router, "Ledger router does not own the lookup endpoint")
    require(
        problems,
        "get_trnm_economic_receipt_by_intent" in router,
        "Ledger router is not bound to the receipt lookup handler",
    )
    for marker in (
        CONTRACT,
        "x-trnm-game-authority",
        "x-trnm-intent-sha256",
        "ImmutableConflict",
        "ReceiptNotFinalized",
        "DatabaseUnavailable",
        "StatusCode::NOT_FOUND",
        "StatusCode::CONFLICT",
        "StatusCode::SERVICE_UNAVAILABLE",
    ):
        require(problems, marker in module, f"receipt lookup module is missing {marker!r}")

    module_sql = compact_sql(module)
    require(
        problems,
        "from public.trnm_economic_intents where intent_id = $1" in module_sql,
        "lookup does not read the authoritative intent identity/hash row",
    )
    require(
        problems,
        "from public.trnm_economic_receipt_events_v1" in module_sql,
        "lookup does not read the append-only native receipt event stream",
    )
    require(
        problems,
        "order by e.event_sequence desc, e.event_id desc" in module_sql,
        "lookup does not select the latest immutable native receipt event",
    )
    require(
        problems,
        "from public.trnm_economic_receipts where intent_id = $1" in module_sql,
        "lookup does not retain the 0027 compatibility receipt fallback",
    )
    require(
        problems,
        "sha256_json(&intent_json)" in module,
        "lookup does not independently verify stored intent bytes against payload_hash",
    )
    require(
        problems,
        '.get("payload_hash")' in module,
        "lookup does not verify receipt evidence against the stored payload hash",
    )
    require(
        problems,
        "constant_time_eq" in module,
        "game-authority authentication is not compared in constant time",
    )

    persist_position = native.find("persist_native_receipt(&mut tx")
    commit_position = native.find('db_error("commit TRNM native-economy transaction"')
    require(
        problems,
        persist_position >= 0 and commit_position > persist_position,
        "native submission does not persist the receipt before committing/returning success",
    )
    for marker in (
        "let payload_hash = format!",
        "insert into trnm_economic_intents",
        "insert into public.trnm_economic_receipt_events_v1",
        "on conflict (intent_id) do nothing",
        "recoverable_hold_retry",
        "pg_advisory_xact_lock",
        "stored_hash != payload_hash",
    ):
        require(problems, marker in native, f"native submission is missing {marker!r}")
    require(
        problems,
        re.search(r"insert into (?:public\.)?trnm_economic_receipts\b", compact_sql(native))
        is not None,
        "native submission is missing 'insert into [public.]trnm_economic_receipts'",
    )
    require(
        problems,
        "on conflict (intent_id) do update" not in native.lower(),
        "native receipt persistence still rewrites receipt evidence with an upsert",
    )

    for marker in (
        "cex_apply_ledger_effect_v1",
        "balance_minor",
        "reserved_minor",
        "credits_to_minor",
        "exact_ledger_effects",
        "cex_open_account_v2",
    ):
        require(problems, marker in native, f"TRNM exact-money cutover is missing {marker!r}")
    for forbidden in (
        "append_native_entry(",
        "balance::float8",
        "reserved::float8",
        "seller_hold_amount::float8",
        "amount as f64",
        "update accounts set balance =",
    ):
        require(
  problems,
  forbidden not in native,
  f"TRNM native economy still contains legacy monetary write/read {forbidden!r}",
        )

    migration_sql = compact_sql(migration)
    require(
        problems,
        "intent_id text primary key" in migration_sql,
        "intent identity is not durable and unique",
    )
    require(
        problems,
        "intent_id text not null unique references trnm_economic_intents(intent_id)" in migration_sql,
        "receipt is not uniquely bound to the durable intent row",
    )

    evidence_sql = compact_sql(evidence_migration)
    for marker in (
        "begin;",
        "create table if not exists public.trnm_economic_receipt_events_v1",
        "event_sequence bigint not null",
        "unique (intent_id, event_sequence)",
        "cex_validate_trnm_economic_receipt_event_v1",
        "cex_reject_trnm_economic_receipt_event_mutation_v1",
        "before update or delete on public.trnm_economic_receipt_events_v1",
        "before truncate on public.trnm_economic_receipt_events_v1",
        "enable always trigger",
        "recoverable_hold_retry",
        "terminal receipt event cannot append after final progression",
        "trnm receipt status/progression mapping mismatch",
        "insert into public.trnm_economic_receipt_events_v1",
        "commit;",
    ):
        require(
            problems,
            marker in evidence_sql,
            f"native receipt evidence migration is missing {marker!r}",
        )

    normalized_event_sql = compact_sql(normalized_event_migration)
    for marker in (
        "latest_event.event_id is null",
        "latest_event.progression_class is distinct from 'recoverable_hold'",
        "normalized terminal receipt event cannot append after final progression",
        "normalized receipt status/progression mapping mismatch",
        "progression_class",
    ):
        require(
            problems,
            marker in normalized_event_sql,
            f"normalized receipt event migration is missing {marker!r}",
        )

    destructive = re.compile(
        r"\b(delete\s+from|truncate(?:\s+table)?)\s+(?:public\.)?trnm_economic_(?:intents|receipts)\b",
        re.IGNORECASE,
    )
    retention_sources = [
        ROOT / "migrations",
        ROOT / "services/ledger-service/src/repository",
    ]
    for directory in retention_sources:
        for path in directory.rglob("*"):
            if path.suffix not in {".rs", ".sql"}:
                continue
            if destructive.search(path.read_text(encoding="utf-8")):
                problems.append(
                    f"destructive receipt-retention statement found in {path.relative_to(ROOT)}"
                )

    for marker in (
        CONTRACT,
        ENDPOINT,
        "404",
        "409",
        "503",
        "indefinitely",
        "World compatibility window",
    ):
        require(problems, marker in contract, f"owner contract documentation is missing {marker!r}")

    for marker in (
        "response-loss",
        "concurrent",
        "immutable_intent_hash_conflict",
        "intent_receipt_not_found",
        "receipt_lookup_unavailable",
        "restart",
        "ledger_entry_count",
        "append_only_event_count",
    ):
        require(problems, marker in blackbox, f"black-box qualification is missing {marker!r}")

    require(
        problems,
        "check-trnm-settlement-receipt-lookup.py" in migration_gate,
        "migration gate does not enforce static receipt lookup wiring",
    )
    for marker in (
        "check-trnm-settlement-receipt-lookup.py",
        "check-trnm-settlement-receipt-lookup-http.sh",
        "Build Ledger receipt recovery target",
        "TRNM response-loss receipt recovery",
    ):
        require(problems, marker in candidate_gate, f"candidate gate is missing {marker!r}")

    result = {
        "status": "ok" if not problems else "failed",
        "contract": CONTRACT,
        "endpoint": ENDPOINT,
        "shared_authority_store": True,
        "retention_mode": "indefinite-no-pruning-v1",
        "problems": problems,
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if not problems else 1


if __name__ == "__main__":
    raise SystemExit(main())
