# matrix-bot-poller module contract

Status: active module contract  
Workspace member: `apps/matrix-bot-poller`  
Package: `matrix-bot-poller`  
Kind: `application`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `matrix-integration`  
Production authorization: `not_granted`

This is an unqualified source implementation. Tests and exact-tree hosted evidence,
not this contract, determine acceptance. See `docs/matrix-recovery-and-receipt-contract-v2.md`.

## Purpose and non-goals

The poller admits supported joined-room Matrix events into a durable inbox/outbox
and advances an opaque cursor only under a fenced PostgreSQL lease. It now fills
limited incremental timelines before admission. It is not a homeserver, Agent,
user authority, financial writer, scientific evaluator, response sender, or proof
that inaccessible/deleted history can be reconstructed.

## Authority and owned state

Owned facts are cursor revision, fenced lease, source observations and relay
intents. Matrix owns source events. Consumer Entry and Hepta own accepted business
actions. A sync token, presentation field, signature-free sender claim, in-memory
result or HTTP success does not transfer identity or business authority.
The transport migration also preserves quarantine snapshots and cursor history.

## Source layout and entry points

Catalog-bound entry points:

- `apps/matrix-bot-poller/src/main.rs`: sync, bounded gap fetch, separate poison
  persistence, atomic inbox/outbox/cursor admission, startup and recovery.
- `apps/matrix-bot-poller/src/sync_recovery.rs`: opaque-token pagination state,
  cycle/shape checks and safely encoded fixed-authority messages URLs.
- `apps/matrix-bot-poller/src/runtime_profile.rs`: strict pure profile parser.

The zero-dependency parser is temporarily replicated byte-for-byte in the adapter
and relay; the source gate rejects divergence. It adds no Cargo dependency.
Shared-crate extraction remains an explicit future refactor, not an inferred result.

## Interfaces and contracts

Use authenticated Matrix Client-Server v3 `/sync`. For an incremental joined-room
`limited` timeline, fetch `/rooms/{roomId}/messages` backwards from `prev_batch`
to the previously committed sync token. Validate every page's `start`, token
bounds, event limit, room binding and cursor progress. Empty chunks with `end`
continue. An absent `end` exhausts the server-visible range; it does not prove
that removed or permission-hidden history exists. Reverse recovered pages into
chronological order before combining with the returned timeline.

Supported actions are nonblank bounded `m.room.message` / `m.text`. Known other
message types are ignored, edits (`m.replace`) do not create another invocation,
and malformed supported messages become poison. Normal message normalization
and delivery-ID derivation are unchanged, preserving existing healthy replays.

## Persistence, concurrency, and recovery

Apply transport migrations 0001, 0002, 0003 and 0004 in order. Renewals require the same
unexpired owner, fence and revision; an expired lease cannot be resurrected.
Each renewal statement commits before HTTP. Recovery is bounded by 100 pages,
100 events per page, 10,000 combined events, 32 MiB gap bytes and 120 seconds.
Limits, malformed pages, cycles or transport failures leave the cursor unchanged.
An interrupted process re-fetches from the unchanged committed cursor; partial
pages are not separately checkpointed or allowed to create remote actions.

Poison observations and canonical snapshots commit in a separate transaction
before the business-admission transaction. Unacknowledged poison anywhere in the
partition blocks advancement, including when it disappears from a later sync.
An explicit operator acknowledgement authorizes quarantine, not reprocessing.
Canonical payload snapshots remain recoverable in restricted custody. The normal
fenced transaction then admits healthy events and advances the cursor atomically.
Every cursor revision has an immutable history entry. SQL role isolation and real
multi-instance/crash recovery still require executed tests and deployment review.

## Configuration and secrets

Required settings are `MATRIX_ACCESS_TOKEN`, `MATRIX_BOT_USER_ID`,
`MATRIX_TRANSPORT_DATABASE_URL` (or `DATABASE_URL`), `MATRIX_POLL_PARTITION_ID` and
`MATRIX_POLL_WORKER_ID`. Profile sources are `MATRIX_POLL_RUNTIME_PROFILE`,
`CEX_RUNTIME_PROFILE`, `APP_ENV`; unknown, empty, non-Unicode and conflicting
explicit values fail. Beta, staging and production all use production-like checks.

| Key | Current default / constraint |
|---|---|
| `MATRIX_POLL_HOMESERVER` | loopback HTTP only for local use; production-like requires HTTPS |
| `MATRIX_POLL_CURSOR_LEASE_SECONDS` | 60; 5–3600; must exceed sync timeout by more than 10 seconds |
| `MATRIX_POLL_SYNC_TIMEOUT_MS` | 30000; minimum 1000 |
| `MATRIX_POLL_SYNC_MAX_BYTES` | 2097152; maximum 4194304 |
| `MATRIX_POLL_INTERVAL_MS` | 3000; effective minimum 100 |
| `MATRIX_POLL_DELIVERY_MAX_ATTEMPTS` | 8; 1–100 |
| `MATRIX_SYNC_FILTER` | optional Matrix filter; no local inference authority |
| `MATRIX_POLL_BOOTSTRAP_MODE` | `require_cursor` by default; explicit `start_now` establishes the first boundary without executing historical messages |

A missing cursor cannot silently consume old commands. Under explicit `start_now`,
commit the first next-batch cursor with no deliveries; later syncs follow normal
recovery. This defines coverage from that recorded start, not from room creation.
Endpoint credentials, query strings and fragments are rejected. Tokens, cursors,
source messages and database diagnostics are not emitted to ordinary telemetry.

## Security and trust boundaries

Transport trust comes from the configured homeserver and token, not message
fields. Disable redirects, bound response bodies, validate identifiers and isolate
poison bytes. Do not use raw database/HTTP exception bodies in normal logging.
Unknown outcomes never invent business completion. Historical poison hashes using
old mutable metadata may require an operator-reviewed migration decision; they
must not be rewritten or marked acknowledged by this upgrade.

## Verification

```text
cargo test --locked -p matrix-bot-poller --all-targets
cargo clippy --locked -p matrix-bot-poller --all-targets -- -D warnings
python3 scripts/check-matrix-recovery-contract.py
python3 scripts/check-matrix-lock-coherence.py
python3 scripts/test-matrix-recovery-contract.py
```

Run all three Matrix packages and `bash scripts/check-matrix-source-observation-postgres.sh`
on a disposable PostgreSQL 16 database with explicit reset consent. Required cases
include empty continuation, cycles, malformed pages, room mismatch, budgets,
restart replay, stale renewal, poison blocking/quarantine, and cursor history.
Python checks are source-contract tests; they do not execute Rust, SQL or Matrix.

## Deployment and operations

Readiness requires all four transport migrations and valid security settings.
Configure an explicit first-start policy and stable account/filter partition.
Monitor cursor age, recovery holds, poison inventory and pending deliveries.
Recovery limit exhaustion is an operator hold, not a skip; tune capacity only
through a reviewed source/configuration change and requalification.

Rollback stops admission and lease acquisition before switching compatible
binaries. Preserve all cursor, source, poison and outbox history. Never drop the
new evidence tables or reinstate old auto-advance behavior as a recovery shortcut.
A real homeserver and representative load qualification remain required.

## Compatibility and change protocol

Cursors remain opaque. Healthy source hashing and UUID derivation are unchanged.
New bootstrap semantics are explicit and only affect a stream with no cursor.
Changes to filtering, pagination, hash rules, partitioning, quarantine, persistence
or limits update the module catalog, this contract, tests and exact-tree evidence.
No source document or local result grants repository or production authorization.

## Compiler and committed lock repair

`rust-toolchain.toml` selects Rust 1.98.1. The Matrix direct-dependency lock
preflight checks the two workspace package entries against their manifests; it
does not resolve the complete dependency graph or prove compilation. Preserve
`--locked` and execute the full package gates after applying the reviewed lock
patch. See `docs/build-unblock-round6.md` for the preimage and remaining evidence.

## Immutable stream scope extension

`apps/matrix-bot-poller/src/stream_scope.rs` validates the token-owner response
and describes the endpoint, account and exact filter input. Before any cursor
request and again inside admission, the poller requires a matching immutable
binding from migration 0004. Existing non-virgin partitions require a stopped,
owner-reviewed upgrade; configuration changes cannot auto-rebind an old cursor.
`python3 scripts/check-matrix-stream-scope.py` is a source check, not runtime proof.
See `docs/matrix-stream-scope-v1.md` for the scope shape, owner API, privileges,
compatibility holds and remaining membership/large-gap work. No production grant.
