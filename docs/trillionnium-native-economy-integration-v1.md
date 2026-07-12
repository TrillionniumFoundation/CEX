# Trillionnium Native Economy Integration v1

Status: current CEX integration truth as of 2026-07-13.

CEX is the first settlement backend for the current native TRNM game. It does
not own RPG/RTS gameplay, and its historical World Web/Matrix shell is not the
native client.

## Protocol ownership

The stable protocol is owned by the Trillionnium repository in
`trnm-economy-protocol` (`term_exchange_protocol_v2`, package `2.4.0`). This
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
- `POST /v1/trillionnium/economy/projection/rebuild` (internal maintenance)
- `POST :7002/v1/trnm/identity/session/verify` (player-session ownership proof
  for the dedicated TRNM game server)
- `POST :7002/v1/trnm/economy/entitlements` (scoped game-authority issuance)

The old World adapter readiness route remains a compatibility alias. It is not
the current contract.

## Settlement behavior

The intent endpoint accepts only protocol-v2 `trnm_game` intents with explicit
actor/account binding and idempotency scope/key. It maps reward, reserve,
escrow hold, consume/commit, refund and chargeback operations to the PostgreSQL
ledger backend and returns a typed receipt. Ledger mutation and the unique
intent/receipt row commit in one SQL transaction. The seller is credited and
reserved only when consume commits held escrow. The payout remains unspendable
through a 24-hour reversible window. A held refund returns buyer funds; a committed
chargeback consumes the seller payout hold and credits the buyer. The receipt is
also recorded in the CEX Term Exchange read projection. Transport and ledger
failures return non-progressing responses; a bad protocol payload is rejected.
TRNM namespaces connected campaign and intent identifiers by the bound account,
while CEX still enforces global intent and `(scope,key)` uniqueness.

Positive `ReleaseReward` requires a CEX-verifiable
`ServerSignedValueEntitlementV1`. The signature binds actor, account, source
battle, source ID, intent, amount, UTC budget day and expiry. PostgreSQL
consumes each entitlement once and enforces 100 credits per event and 300 per
account/day in the ledger transaction. `CompleteContract` must carry zero
value. A client-authored amount without the trusted entitlement is rejected.
Entitlement issuance no longer accepts a general ledger admin header. It
requires `x-trnm-game-authority`, backed by the dedicated
`TRNM_GAME_AUTHORITY_TOKEN`. The same scoped server credential may submit the
resulting signed intent and reconcile its campaign wallet, but is not shipped
to a native client. General admin tokens cannot mint value through this route.

Online Authority v2 uses the stricter `ServerSignedValueEntitlementV2` path.
The dedicated game server signs match/rules/build/result/participant/nonce-bound
payloads with Ed25519 and submits the signed intent directly; it no longer asks
CEX to issue the online entitlement. CEX loads only a public issuer registry,
requires an active exact key/issuer pair and rejects tampered signatures,
unknown keys, revoked keys or changed authoritative metadata before ledger
mutation. The private seed is a mode-600 game-server runtime file and is not
present in CEX, a native client or either repository. The v1 HMAC contract and
issuance endpoint remain for the existing trusted native/offline integration.
Production KMS/HSM custody and automatic rotation are still pending.

The wallet endpoint reads the configured ledger, persists the actor/account
reconciliation cursor and returns the protocol `WalletSnapshot`. Player routes
require `trnm_player_session_v1`, signed by CEX and persisted as a token hash
with player/account ownership, device, recovery generation, expiry and
revocation. Recovery, suspension and closure revoke live sessions. The
consumer shared entry token is retained only for internal service maintenance
and is not a distributable native-client credential.

Online Product v1 adds a closed-alpha self-service account surface. A scoped
administrator creates a database-backed, time-bounded registration invite with
a bounded use count; registration consumes one use in the same transaction that
creates the ledger account and TRNM identity. New credentials use randomly
salted Argon2id. Legacy high-entropy recovery hashes remain accepted for
migration, and rotation rewrites the credential while incrementing generation
and revoking every old session. Five failed logins in a 15-minute window create
a durable five-minute lock, including unknown player IDs without exposing an
account-existence distinction.

Suspension revokes all sessions. A suspended credential owner can create one
pending appeal; only `ledger:manage` may approve or reject it. Approval
reactivates the identity and appends the existing immutable identity audit.
This is local software evidence, not verified email/phone recovery, MFA or a
staffed support SLA.

TRNM Online Authority v2 calls the session verification endpoint before it
creates, joins, starts, snapshots or commands a network campaign. Verification
checks the signed token, persisted token hash, active identity, recovery
generation, revocation, expiry and exact player/account ownership. The game
server then owns the match result and calls the signed v2 intent path;
the player client cannot supply a terminal result or mint amount.

Migrations `0027_add_trnm_native_economy_persistence.sql` and
`0028_add_trnm_seller_hold_and_identity_recovery.sql` and
`0029_add_trnm_value_entitlements_and_player_sessions.sql` and
`0030_add_trnm_online_product_identity.sql` supply unique intent,
idempotency, receipt, cursor and escrow constraints. Formal release services
are installed as `cex-trnm-ledger.service` and
`cex-trnm-consumer.service`; `LEDGER_FAIL_FAST=true` makes PostgreSQL absence a
startup blocker, not a trigger for memory fallback. PostgreSQL itself uses the
existing durable Docker volume with an `unless-stopped` restart policy. After
building the two release binaries, `scripts/install-trnm-economy-systemd.sh`
reproducibly installs and enables the services.
The installer also enables `cex-trnm-economy-maintenance.timer`; its five-minute
job releases matured seller holds, alerts on overdue holds and reconstructs the
consumer receipt projection from PostgreSQL.

Admin-protected native identity registration and rotating recovery credentials
persist with generation and immutable audit records. This is a software
recovery boundary, not evidence of real-user support or public account issuance.

## Evidence and boundary

`cargo test -p consumer-entry-api --lib` retains in-process boundary tests.
`scripts/check-trnm-native-economy-cross-process.sh` then uses the two formal
services and PostgreSQL to prove independent account creation, reward
exactly-once, byte-identical receipt replay before and after service restart,
held escrow refund, committed escrow, seller chargeback/buyer refund,
wallet/cursor recovery and unique database rows. Legacy League/World receipt
tests remain green for read compatibility.
`scripts/check-trnm-economy-disaster-recovery.sh` restores a logical backup to
an isolated database and races one intent through two ledger instances to prove
database-enforced exactly-once. The TRNM-side native-client gate drives the Bevy
input system through purchase, service restart, UI projection and cancellation.
`scripts/check-trnm-value-entitlement-and-session.sh` proves entitlement,
budget, ownership, device recovery, revocation and suspension policy.
`scripts/check-trnm-postgres-pitr-failover.sh` takes a physical base backup,
restores archived WAL to a named restore point and promotes the restored
instance writable. `scripts/check-trnm-postgres-streaming-standby-chaos.sh`
also proves same-host streaming replay, primary-stop detection and promotion.
The remaining multi-host HA boundary is recorded in
`trnm-economy-disaster-recovery-v1.md`.

This proves persistent local production-profile integration. It does not claim
high availability, public CEX exposure, legal/commercial readiness or public
player trading. Native public listings are explicitly release-gated until
custody/matching review, anti-abuse controls, dispute/support operations,
human usability and legal release approval exist.
