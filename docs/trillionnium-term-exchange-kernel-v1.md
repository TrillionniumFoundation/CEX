# Trillionnium Term Exchange Kernel v1

**Decision**: the stable economic layer is the **Term Exchange Kernel**, not CEX itself. CEX is the first settlement backend; DEX/chain settlement must plug into the same protocol later.

Primary runtime manifest:

```text
GET /v1/trillionnium/term-exchange/kernel/manifest
contract_version = trillionnium_term_exchange_kernel_v1
protocol_version = term_exchange_protocol_v1
active_backend   = cex-settlement-backend
```

Legacy compatibility endpoint:

```text
GET /v1/trillionnium/runtime/cex/manifest
legacy_contract_version = trillionnium_cex_runtime_plugin_v1
status = upgraded_to_term_exchange_kernel_manifest
```

---

## 1. Code boundary

The protocol type layer now lives in:

```text
crates/term-exchange-protocol
```

It defines the stable vocabulary shared by Trillionnium World, CEX, and future DEX/chain adapters:

- `TermDefinition`
- `EconomicIntent`
- `EconomicReceipt`
- `ReceiptStatus`
- `ReceiptProgressionClass`
- `SettlementBackendManifest`
- `IdempotencyKey`
- `ActorRef`
- `AssetRef`

This crate is intentionally pure Rust + serde/serde_json. It must not depend on Bevy, browser code, CEX service internals, SQL repositories, or chain clients.

---

## 2. Runtime model

```text
Domain plugin / World term
  emits EconomicIntent

Term Exchange Kernel
  validates term + idempotency + progression policy
  routes settlement to active backend

Settlement backend
  CEX now, DEX/chain later
  returns EconomicReceipt

Domain plugin / World term
  advances state only when receipt allows progression or terminal skip
```

The kernel owns the protocol and state-transition vocabulary. It does not own full gameplay.

---

## 3. Backend model

### Active backend now

```text
backend_id   = cex-settlement-backend
backend_kind = cex
contract     = term_exchange_backend_v1
```

CEX backend currently owns:

- wallet read model
- identity/account scope resolution
- ledger intent validation
- reserve
- escrow-like hold
- seller settlement
- buyer consume
- refund
- seller chargeback
- reward/review release
- work-order economy
- audit receipts
- recovery/dead-letter handling

### Planned backend

```text
backend_id   = dex-settlement-backend
backend_kind = dex
contract     = term_exchange_backend_v1
status       = planned_backend_same_protocol
```

DEX backend should later own:

- on-chain reserve/lock
- smart-contract settlement
- on-chain refund
- proof verification
- chain-finality mapping

World and product clients should not change when switching backend; only the backend adapter and receipt verifier change.

---

## 4. Receipt classes

`ReceiptStatus` maps to `ReceiptProgressionClass` in the protocol crate.

### Progression allowed

- `reserved`
- `settled`
- `consumed`
- `refunded`
- `seller_chargeback_consumed`
- `approved_release`

### Terminal skip

- `skipped_zero_price`
- `skipped_zero_seller_net`

### Recoverable hold

- `failed_network`
- `failed_identity`
- `failed_ledger`
- `seller_chargeback_reserve_failed`
- `seller_chargeback_failed`
- `rejected_refund_failed`
- `cancelled_refund_failed`

### Hard fail

- `failed_bad_response`
- `missing_account`
- `missing_ledger_token`

Domain state must not mark economic work as complete on recoverable hold or hard fail.

---

## 5. Why this is better than CEX-containing-World or World-containing-CEX

Hard containment creates migration pain:

- If World contains CEX, future DEX replacement leaks into gameplay code.
- If CEX contains World, CEX becomes too game-specific and stops being a reusable exchange substrate.

The kernel split gives the right shape:

```text
World owns semantics.
Term Exchange Kernel owns economic protocol.
CEX/DEX own settlement implementation.
```

So a new Trillionnium World concept becomes a new term family, not a new CEX hard-coded feature.

---

## 6. Current migration status

Landed first:

- Added `crates/term-exchange-protocol`.
- Upgraded manifest to `trillionnium_term_exchange_kernel_v1`.
- CEX is now declared as `cex-settlement-backend`, the first backend.
- Legacy CEX runtime manifest endpoint remains as compatibility alias.

Landed next slice:

1. Added `TermExchangeBackend` trait in the service layer.
2. Moved first direct world/league economic calls behind the CEX backend adapter.
3. The adapter now converts backend outcomes to typed `EconomicReceipt` values at the boundary, then projects to legacy status fields for endpoint compatibility.

Next:

1. Store typed receipt references/statuses in World/League state instead of relying only on legacy string fields.
2. Keep current CEX endpoints/E2E green during migration.
3. Later add DEX adapter under the same `term_exchange_backend_v1` contract.
---

## 7. Backend adapter slice

The service layer now has a first backend adapter contract:

```text
adapter_contract_version = trillionnium_term_exchange_backend_adapter_v1
trait                    = TermExchangeBackend
request                  = TermExchangeLedgerActionRequest
receipt                  = EconomicReceipt
legacy projection         = LeagueLedgerSettlement
```

Migrated call paths in this slice:

- `league_reward_settlement`
- `world_commerce_purchase_reserve_settle_consume_refund_chargeback`
- `world_contract_completion_settlement`

These paths now build typed `EconomicIntent` data and receive typed `EconomicReceipt` data inside the backend adapter, then project back to legacy status fields so current endpoints, tests, and E2E contracts remain stable.

Policy: world and league economic routes should call the backend adapter, not ledger HTTP directly. Remaining migration work is to store receipt references/statuses directly in World/League state instead of only legacy string fields.
---

## 8. Typed receipt state slice

The adapter-created `EconomicReceipt` is now persisted into runtime state through a compact state projection:

```text
receipt_state_type   = TermExchangeReceiptState
league_receipt_index = LeagueState.term_exchange_receipts
world_receipt_index  = WorldState.world_term_exchange_receipts
```

Stored fields:

- `protocol_version`
- `receipt_id`
- `intent_id`
- `term_id`
- `backend_id`
- `backend_kind`
- `status`
- `progression_class`
- `settlement_reference`
- `ledger_entry_id`
- `reason`
- `finalized_at_epoch`

Legacy status fields remain for endpoint compatibility, but World/League state now also carries typed receipt status and progression class. This is the bridge toward making domain progression read from `EconomicReceipt` instead of stringly ledger fields.

Next cutover: add normalized SQL receipt tables and migrate remaining legacy-only settlement paths.
