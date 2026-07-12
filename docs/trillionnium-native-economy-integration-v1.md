# Trillionnium Native Economy Integration v1

Status: current CEX integration truth as of 2026-07-12.

CEX is the first settlement backend for the current native TRNM game. It does
not own RPG/RTS gameplay, and its historical World Web/Matrix shell is not the
native client.

## Protocol ownership

The stable protocol is owned by the Trillionnium repository in
`trnm-economy-protocol` (`term_exchange_protocol_v2`). This CEX workspace
depends on that pure crate. `consumer-entry-api` no longer imports the removed
`trnm-world-api`, `trnm-world-domain` or `trnm-world-projection` crates.

Current endpoints:

- `GET /v1/trillionnium/term-exchange/kernel/manifest`
- `GET /v1/trillionnium/economy/adapters/readiness`
- `POST /v1/trillionnium/economy/intents`
- `POST /v1/trillionnium/economy/wallet`

The old World adapter readiness route remains a compatibility alias. It is not
the current contract.

## Settlement behavior

The intent endpoint accepts only protocol-v2 `trnm_game` intents with explicit
actor/account binding and idempotency scope/key. It maps reward, reserve,
settle, consume, refund and chargeback operations to the ledger backend and
returns a typed receipt. The receipt is recorded in the CEX Term Exchange
receipt index. Transport and ledger failures return non-progressing typed
receipts; a bad protocol payload is rejected.

The wallet endpoint reads the configured ledger with the CEX admin token and
returns the protocol `WalletSnapshot`. Ingress authentication remains governed
by the normal consumer-entry configuration.

## Evidence and boundary

`cargo test -p consumer-entry-api --lib` includes real in-process ledger tests
covering reward exactly-once, duplicate replay, reserve/refund,
reserve/chargeback, wallet reconciliation, service-down recovery hold and
invalid-protocol rejection. Legacy League/World receipt tests remain green for
read compatibility.

This proves local build/runtime integration. It does not claim public CEX
deployment, player-account provisioning, legal/commercial readiness or public
player trading.
