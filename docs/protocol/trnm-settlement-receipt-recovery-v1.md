# TRNM settlement receipt recovery v1

Status: CEX-owned P0 contract  
Contract version: `trnm_cex_settlement_receipt_lookup_v1`  
Authoritative store: PostgreSQL `trnm_economic_intents` + `trnm_economic_receipts`

## Purpose

A World settlement client may deliver an intent successfully but lose the HTTP response. The client must recover the already committed `EconomicReceipt` before deciding whether to submit again. CEX therefore exposes a read-only lookup that binds an intent identifier, the immutable intent bytes, and the durable receipt in the same authority store used by `POST /v1/trnm/economy/intents`.

This contract is not a player-market API and does not grant custody, commercial, or production authorization.

## Endpoint

```http
GET /v1/trnm/economy/receipts/by-intent?intent_id=<intent_id>
x-trnm-game-authority: <game-authority credential>
x-trnm-intent-sha256: <64 lowercase hexadecimal characters>
```

The query must contain exactly one `intent_id`. The identifier must be non-empty, have no leading or trailing whitespace or control characters, and be no longer than 512 UTF-8 bytes. Authentication is evaluated before the lookup result is disclosed.

## Intent hash

`x-trnm-intent-sha256` is the SHA-256 of the `EconomicIntent` object, not the outer submit request wrapper.

Canonical bytes are produced as follows:

1. Decode the submitted object as `EconomicIntent`.
2. Convert it to a JSON value.
3. Recursively order every JSON object by key.
4. Encode compact UTF-8 JSON with no insignificant whitespace.
5. Compute SHA-256 and encode 64 lowercase hexadecimal characters.

CEX computes and stores this hash before applying the economic effect. The hash is also embedded in the persisted receipt evidence. Lookup independently recomputes the hash from the durable `intent_json`, compares it with `trnm_economic_intents.payload_hash`, compares the requested hash, and finally compares the receipt evidence hash.

## Success response

```json
{
  "contract_version": "trnm_cex_settlement_receipt_lookup_v1",
  "intent_id": "world:settlement:example",
  "intent_hash": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
  "receipt": {
    "protocol_version": "term_exchange_protocol_v2",
    "receipt_id": "cex-native-receipt:world:settlement:example",
    "intent_id": "world:settlement:example",
    "term_id": "...",
    "backend_id": "cex-settlement-backend",
    "backend_kind": "cex",
    "status": "reserved",
    "progression_class": "progression_allowed",
    "settlement_reference": "...",
    "ledger_entry_id": "...",
    "reason": null,
    "evidence": {
      "authority": "cex-ledger-postgres",
      "atomic_intent_receipt": true,
      "payload_hash": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    },
    "finalized_at_epoch": 1700000000
  }
}
```

`200 OK` is returned only after all of these bindings pass:

- the intent row exists;
- the stored intent JSON hashes to the stored `payload_hash`;
- the requested hash equals the stored hash;
- the receipt row exists;
- the decoded receipt ID equals the durable receipt row ID;
- the receipt intent ID equals the requested intent ID;
- the receipt evidence hash equals the stored intent hash.

Repeated successful lookups return the same durable receipt identity and economic binding. A later lookup may observe a legitimate persisted transition from a recoverable-hold receipt to its final receipt, but never a receipt synthesized from request data or memory.

## Stable error contract

Every endpoint-owned error is JSON:

```json
{
  "contract_version": "trnm_cex_settlement_receipt_lookup_v1",
  "error": {
    "code": "immutable_intent_hash_conflict",
    "message": "intent_id is durably bound to a different immutable payload hash"
  }
}
```

| HTTP | Code | Meaning |
|---|---|---|
| `400` | `invalid_lookup_query` | The query cannot decode to exactly one `intent_id`. |
| `400` | `invalid_intent_id` | The identifier violates the published shape. |
| `400` | `intent_hash_required` | The hash header is absent or duplicated. |
| `400` | `invalid_intent_hash` | The hash is not exactly 64 lowercase hexadecimal characters. |
| `401` | `unauthorized` | The game-authority credential is absent, duplicated, malformed, or wrong for this CEX audience. |
| `404` | `intent_receipt_not_found` | Neither an intent row nor a receipt row exists for the identifier. |
| `409` | `immutable_intent_hash_conflict` | The identifier already binds different immutable intent bytes. |
| `503` | `receipt_not_finalized` | The intent exists but no durable receipt has committed yet. This is never a `404`. |
| `503` | `receipt_binding_corrupt` | Stored intent/hash/receipt integrity checks disagree. |
| `503` | `receipt_lookup_unavailable` | PostgreSQL or the authoritative operation pool is unavailable. A timeout or `5xx` must never be interpreted as absence. |

Clients may retry `503` with bounded backoff. They must not submit a replacement intent after `409`, and must not convert a timeout, transport error, or `5xx` into `404`.

## Atomicity and replay

`POST /v1/trnm/economy/intents` acquires a PostgreSQL transaction-scoped advisory lock over the idempotency identity. Within that transaction it:

1. computes the canonical payload hash;
2. inserts or validates `trnm_economic_intents`;
3. applies at most one Ledger effect;
4. inserts or updates `trnm_economic_receipts` with the same intent identity and hash evidence;
5. commits before returning success.

A concurrent duplicate with the same ID and bytes returns the same stored receipt. The same ID or idempotency identity with different bytes is an immutable conflict. There is no in-memory recovery cache.

## Retention policy

CEX v1 retains authoritative TRNM intent and receipt rows **indefinitely**, which exceeds the supported World compatibility window. No migration or Ledger maintenance path deletes or truncates `trnm_economic_intents` or `trnm_economic_receipts`. The receipt foreign key remains bound to the intent identity.

Any future pruning policy requires a new versioned owner contract, an explicit minimum World compatibility window, migration and restore evidence, and consumer rollout proving that no supported World build depends on the rows. Silent TTL expiry is forbidden.

## Qualification requirements

The exact candidate commit must prove all of the following:

- a complete submit request can commit after the client closes without reading the response;
- lookup recovers that receipt and exactly one corresponding Ledger mutation exists;
- two concurrent first submissions return the same receipt or an immutable conflict without duplicate mutation;
- mismatched and malformed hashes fail closed;
- missing and wrong-audience credentials fail closed;
- an existing intent without a receipt returns `503`, not `404`;
- a missing identity alone returns `404`;
- database unavailability returns `503`, not `404`;
- process restart preserves byte-identical lookup output;
- candidate evidence records the contract, hash, receipt IDs, Ledger counts, commit SHA, Cargo lock, SBOM, and provenance;
- World pins the exact qualified CEX commit and reports zero unexplained receipt divergence.
