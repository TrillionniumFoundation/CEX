---
status: current-candidate
owner: TrillionniumFoundation/CEX
issue: 6
contract: trnm_cex_settlement_receipt_lookup_v1
consumer: TrillionniumFoundation/Trillionnium-World
last_reviewed: 2026-08-28
review_due: 2026-09-11
---

# TRNM CEX Settlement Receipt Lookup v1

## Purpose

This service is the CEX-owned half of `WORLD-P0-001`. It closes the ambiguous
outcome window in which an economic intent and its ledger mutation commit
durably, while the HTTP response is lost.

The only supported recovery strategy is:

```text
lookup exact intent_id + SHA-256
  -> found and hash matches: return the durable receipt
  -> intent truly absent: return 404 and permit one submit
  -> same intent_id under different hash: return 409
  -> lookup unavailable: return 5xx; the caller must not submit
```

This contract does not enable the public player market and does not grant
trusted-settlement, custody, commercial-release or production-deployment
credit by itself.

## Endpoints

### Readiness

```http
GET /v1/trnm/economy/readiness
```

`200` requires PostgreSQL, the settlement receipt table, at least one active
game-authority principal for audience `trnm-cex-settlement-v1`, and at least one
active Ed25519 entitlement issuer key. Otherwise the service returns `503`.

### Issuer registry status

```http
POST /v1/trnm/economy/issuer-keys/status
x-trnm-game-authority: <credential>
Content-Type: application/json

{"key_id":"..."}
```

The response binds the exact key ID, issuer, active/retired status, signature
algorithm and public-key SHA-256. Unknown keys return `404`.

### Submit an economic intent

```http
POST /v1/trnm/economy/intents
x-trnm-game-authority: <credential>
x-trnm-intent-sha256: <64 lowercase hexadecimal characters>
Content-Type: application/json

{"intent": { "...": "EconomicIntent" }}
```

The supplied hash is recomputed from the exact typed `EconomicIntent` JSON
encoding before any value mutation begins.

Supported candidate kinds:

- `release_reward`: validates the active Ed25519
  `trnm_server_signed_value_entitlement_v2`, account, amount, currency,
  per-event cap and UTC daily cap, then commits the wallet credit, ledger entry
  and immutable receipt in one PostgreSQL transaction.
- `complete_contract`: commits a non-value durable receipt.

All other intent kinds fail closed with `422` until separately versioned and
reviewed.

### Lookup a durable receipt

```http
GET /v1/trnm/economy/receipts/by-intent?intent_id=<intent_id>
x-trnm-game-authority: <credential>
x-trnm-intent-sha256: <same exact hash>
```

Response:

```json
{
  "contract_version": "trnm_cex_settlement_receipt_lookup_v1",
  "intent_id": "...",
  "intent_hash": "...",
  "receipt": {
    "protocol_version": "term_exchange_protocol_v2",
    "receipt_id": "trnm-cex-receipt-v1:...",
    "intent_id": "...",
    "term_id": "...",
    "backend_id": "cex-settlement-backend",
    "backend_kind": "cex",
    "status": "approved_release",
    "progression_class": "progression_allowed",
    "settlement_reference": "trnm-cex-settlement-v1:...",
    "ledger_entry_id": "...",
    "reason": null,
    "evidence": {},
    "finalized_at_epoch": 0
  }
}
```

## Authentication and audience

Runtime configuration is supplied through:

```text
TRNM_GAME_AUTHORITY_PRINCIPALS_JSON
TRNM_ENTITLEMENT_ISSUER_KEYS_JSON
```

Game authority credentials are stored as SHA-256 digests. A token that matches
a configured principal but has a different audience returns `403`; missing,
inactive or unknown credentials return `401`.

The service never accepts player sessions or moderator credentials on the
settlement endpoints.

## Durable identity and transaction model

The immutable identities are:

```text
intent_id
intent_hash = SHA-256(exact EconomicIntent JSON bytes)
receipt_id = trnm-cex-receipt-v1:<intent_hash>
```

For each submit, PostgreSQL takes a transaction-scoped advisory lock derived
from `intent_id`.

Within that transaction:

1. an existing row with the same hash returns the stored receipt;
2. an existing row with a different hash returns an immutable conflict;
3. a new reward locks the wallet account and daily-budget row;
4. the account balance and ledger entry are written;
5. the exact intent and receipt are inserted;
6. the transaction commits;
7. only after commit may the HTTP success response be emitted.

No in-memory cache is authoritative. Receipt and budget foreign keys use
`ON DELETE RESTRICT`.

## Reward policy

Candidate online battle rewards require:

- exactly one actor and one CEX wallet account UUID;
- exactly one `cex-wallet-credit` asset;
- `1..=100` credits per event;
- at most `300` credits per account per UTC budget day;
- active account status and `wallet_credits` denomination;
- an unexpired Ed25519 entitlement issued by `trnm-online-game-server`;
- exact actor, account, intent, amount, currency, match, result and participant
  bindings.

Invalid signatures, retired keys, wrong accounts, wrong currencies, expired
entitlements and policy-limit violations produce no ledger mutation.

## Stable error semantics

All errors use `trnm_cex_settlement_error_v1` and include stable `code` and
`retryable` fields.

Important mappings:

| HTTP | Code | Meaning |
|---|---|---|
| 400 | `invalid_intent_hash` | malformed hash |
| 401 | `missing_game_authority` / `invalid_game_authority` | missing or invalid credential |
| 403 | `wrong_game_authority_audience` | valid principal for another audience |
| 404 | `intent_not_found` | neither durable intent nor receipt exists |
| 409 | `intent_hash_conflict` | immutable ID/hash collision |
| 422 | policy or entitlement code | permanent input/policy rejection |
| 503 | `settlement_database_unavailable` | ambiguous infrastructure failure; do not submit again without lookup |

A timeout or `5xx` is never translated into `404`.

## Test and evidence requirements

The mandatory PostgreSQL contract suite covers:

- commit followed by discarded response, then lookup recovery;
- exact duplicate receipt reuse with one ledger mutation;
- concurrent duplicate submission with one mutation;
- immutable hash conflict;
- missing credential, wrong audience and malformed hash;
- invalid Ed25519 signature;
- non-value contract completion;
- per-account daily reward cap;
- lookup hash mismatch.

Promotion still requires exact-commit GitHub Actions, review, merge, immutable
deployment artifact, deployed process-kill/response-loss tests, retention and
restore approval, and a cross-repository component lock.
