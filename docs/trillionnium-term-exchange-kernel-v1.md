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

Next:

1. Add `TermExchangeBackend` trait in the service layer.
2. Move direct world/league economic calls behind the backend adapter.
3. Convert stringly ledger outcomes to typed `EconomicReceipt` values at mutation boundaries.
4. Keep current CEX endpoints/E2E green during migration.
5. Later add DEX adapter under the same `term_exchange_backend_v1` contract.
