# Trillionnium Native Economy Integration v1

Status: current CEX integration truth as of 2026-07-12.

CEX is the first settlement backend for the current native TRNM game. It does
not own RPG/RTS gameplay, and its historical World Web/Matrix shell is not the
native client.

## Protocol ownership

The stable protocol is owned by the Trillionnium repository in
`trnm-economy-protocol` (`term_exchange_protocol_v2`, package `2.1.0`). This
CEX workspace vendors and pins that exact pure crate version under `vendor/`,
so an independent checkout does not require a sibling repository path. The old
CEX-local v1 crate is removed and represented only by `QUARANTINED.md` plus Git
history. `consumer-entry-api` no longer imports the removed
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
escrow hold, consume/commit, refund and chargeback operations to the PostgreSQL
ledger backend and returns a typed receipt. Ledger mutation and the unique
intent/receipt row commit in one SQL transaction. The seller is credited only
when consume commits held escrow. A held refund returns buyer funds; a committed
chargeback atomically debits the seller and credits the buyer. The receipt is
also recorded in the CEX Term Exchange read projection. Transport and ledger
failures return non-progressing responses; a bad protocol payload is rejected.

The wallet endpoint reads the configured ledger, persists the actor/account
reconciliation cursor and returns the protocol `WalletSnapshot`. Ingress
authentication remains governed by the normal consumer-entry configuration.

Migration `0027_add_trnm_native_economy_persistence.sql` supplies unique intent,
idempotency, receipt, cursor and escrow constraints. Formal release services
are installed as `cex-trnm-ledger.service` and
`cex-trnm-consumer.service`; `LEDGER_FAIL_FAST=true` makes PostgreSQL absence a
startup blocker, not a trigger for memory fallback. PostgreSQL itself uses the
existing durable Docker volume with an `unless-stopped` restart policy. After
building the two release binaries, `scripts/install-trnm-economy-systemd.sh`
reproducibly installs and enables the services.

## Evidence and boundary

`cargo test -p consumer-entry-api --lib` retains in-process boundary tests.
`scripts/check-trnm-native-economy-cross-process.sh` then uses the two formal
services and PostgreSQL to prove independent account creation, reward
exactly-once, byte-identical receipt replay before and after service restart,
held escrow refund, committed escrow, seller chargeback/buyer refund,
wallet/cursor recovery and unique database rows. Legacy League/World receipt
tests remain green for read compatibility.

This proves persistent local production-profile integration. It does not claim
high availability, public CEX exposure, legal/commercial readiness or public
player trading. Native public listings are explicitly release-gated until
custody/matching review, anti-abuse controls, dispute/support operations,
human usability and legal release approval exist.
