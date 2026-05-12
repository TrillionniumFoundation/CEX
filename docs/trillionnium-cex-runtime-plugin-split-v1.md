# Trillionnium World / CEX Runtime Split v1

**Decision**: CEX and Trillionnium World should be split. CEX should be designed as a runtime-inserted economic kernel for Trillionnium World, not as gameplay code embedded inside World.

**Current implementation anchor**: CEX `consumer-entry-api` exposes the first runtime plugin manifest at:

```text
GET /v1/trillionnium/runtime/cex/manifest
contract_version = trillionnium_cex_runtime_plugin_v1
plugin_id        = cex-econ-kernel
host_contract    = trillionnium_world_runtime_plugin_host_v1
```

---

## 1. Why split

Trillionnium World and CEX have different failure domains.

- World failure should affect gameplay, map projection, NPC/task/combat state, or UI hydration.
- CEX failure affects money-like state, reserves, settlements, refunds, rewards, audit, and commercial trust.

Because the second class is higher risk, it should be a separate runtime boundary with stricter idempotency, receipt, audit, and fail-closed rules.

The design target is:

```text
Trillionnium World
  owns gameplay and world truth
  emits economic intent

CEX Runtime / Econ Kernel
  owns economic validity
  validates identity/account/ledger/action
  returns economic receipt

Trillionnium World
  advances world state only after receipt
```

---

## 2. Ownership boundary

### Trillionnium World owns

- `WorldState` gameplay truth.
- Map topology, node graph, region/tile/POI projection.
- Player position and movement rules.
- NPC / Agent presence and interaction semantics.
- Skill practice, lightweight combat, route objectives.
- Task meaning and world progression.
- Rust-owned `/world` UI fragments.
- Client intent protocol and focus/input bridge.

### CEX owns

- Identity/account scope resolution.
- Wallet read model.
- Ledger intent validation.
- Reserve / escrow-like holds.
- Seller settlement.
- Buyer consume.
- Refund.
- Seller chargeback.
- Reward and review release.
- Work-order economic lifecycle.
- Idempotency keys.
- Audit receipts.
- Recovery and dead-letter policy.

### Shared contract

- `economic_intent`
- `economic_receipt`
- `world_progression_gate`
- `route_recovery_hint`
- `operator_evidence_package`

---

## 3. Runtime insertion model

CEX is a runtime plugin behind a stable host contract.

```text
World command handler
  -> build economic intent
  -> call CEX runtime adapter
  -> receive CEX receipt
  -> mutate WorldState only if receipt allows progression
```

CEX runtime can later be one of:

1. **Local CEX runtime** inside current CEX `consumer-entry-api`.
2. **Remote CEX service** with the same contract.
3. **Chain-backed CEX settlement adapter** once Trillionnium chain integration is ready.
4. **Test/fake runtime** only for local-dev and never for production readiness.

The gameplay client must not care which runtime is active.

---

## 4. Receipt policy

World can advance only on allowed receipts.

### Progression receipts

- `reserved`
- `settled`
- `consumed`
- `refunded`
- `seller_chargeback_consumed`
- `approved_release`

### Recoverable holds

- `failed_network`
- `failed_identity`
- `failed_ledger`
- `seller_chargeback_reserve_failed`
- `seller_chargeback_failed`
- `rejected_refund_failed`
- `cancelled_refund_failed`

Recoverable holds may update route hints and recovery UI, but must not mark the economic lifecycle as complete.

### No progression

- `failed_bad_response`
- `failed_ledger`
- `failed_network`
- `failed_identity`
- `missing_account`
- `missing_ledger_token`

---

## 5. Current CEX endpoint mapping

| Capability | Current CEX endpoint |
| --- | --- |
| Runtime manifest | `GET /v1/trillionnium/runtime/cex/manifest` |
| Wallet read model | `GET /v1/matrix/users/:matrix_user_id/wallet` |
| League reward settlement | `POST /v1/league/matches/:match_id/submit` |
| League review release | `POST /v1/league/reviews/:reward_id/approve` |
| League reward read model | `GET /v1/league/players/:matrix_user_id/rewards` |
| World contract completion | `POST /v1/world/contracts/:contract_id/complete` |
| World listing purchase | `POST /v1/world/listings/:listing_id/buy` |
| Work delivery | `POST /v1/world/work-orders/:work_order_id/deliver` |
| Work acceptance | `POST /v1/world/work-orders/:work_order_id/accept` |
| Work rejection/refund/chargeback | `POST /v1/world/work-orders/:work_order_id/reject` |
| Work reopen | `POST /v1/world/work-orders/:work_order_id/reopen` |
| Work cancel/refund/chargeback | `POST /v1/world/work-orders/:work_order_id/cancel` |
| Commerce read model | `GET /v1/world/commerce` |

These endpoints are the current implementation surface, not the final plugin ABI. The next split step is to move calls behind a `CexRuntime` adapter/trait while preserving these endpoints and gates.

---

## 6. Migration sequence

### M0 — Contract manifest, landed first

- Add `trillionnium_cex_runtime_plugin_v1` manifest.
- Add tests proving CEX is declared as `runtime_inserted_econ_system_kernel`.
- Document split boundary in CEX and Trillionnium repos.

### M1 — Adapter seam

- Add `CexRuntime` trait / adapter object.
- Move direct ledger HTTP calls behind the adapter.
- Keep existing endpoint behavior unchanged.
- Test with current CEX adapter and local fake only in local-dev tests.

### M2 — Economic intent/receipt types

- Define typed intents: `ReservePurchase`, `SettleSeller`, `ConsumeBuyer`, `RefundBuyer`, `ChargebackSeller`, `ReleaseReward`, `CompleteContract`.
- Define typed receipts with progression class: `ProgressionAllowed`, `RecoverableHold`, `TerminalSkip`, `HardFail`.
- Replace stringly-typed gate checks at World mutation boundaries.

### M3 — Storage/read-model boundary

- Move CEX-owned economic read models into CEX tables/repository surfaces.
- World stores receipt references and world-facing status, not full ledger internals.
- Preserve SQL snapshot gates during migration.

### M4 — External runtime mode

- Allow Trillionnium World to point to remote CEX runtime by config.
- Add health/compatibility check: host requires manifest contract version and capability list.
- Add rollback to local runtime.

### M5 — Chain-backed settlement adapter

- Add Trillionnium chain settlement adapter under the same runtime contract.
- Keep World gameplay and UI unchanged.
- Use chain receipts as CEX economic receipts.

---

## 7. Go / No-Go gates

Go:

- Manifest endpoint returns `trillionnium_cex_runtime_plugin_v1`.
- Existing CEX tests and runtime E2E stay green.
- World state advances only after CEX receipt.
- Browser/Matrix cannot forge economic completion.
- Failed ledger paths route to recovery, not completion.

No-Go:

- Any migration bypasses reserve/settle/consume/refund/chargeback gates.
- Any World UI button marks economy complete without receipt.
- Any fake/local-dev runtime participates in production readiness.
- Any direct Ledger fallback reintroduces memory-only success in production/local-production.

---

## 8. Product framing

The clean product story is:

```text
Trillionnium World is the world players see.
CEX is the economic runtime the world plugs into.
Trillionnium chain is the long-term settlement and proof layer.
```

This gives World product freedom without weakening the economy, and gives CEX a reusable role beyond one game shell.
