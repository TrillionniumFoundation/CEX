# matrix-bot-poller module contract

Status: active module contract  
Workspace member: `apps/matrix-bot-poller`  
Package: `matrix-bot-poller`  
Kind: `application`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `matrix-integration`  
Production authorization: `not_granted`

This contract is indexed by `docs/module-catalog-v1.json`. It defines the module boundary for one exact repository tree; it is not release evidence. The durable runtime wiring described below remains a repository candidate until the exact candidate SHA passes hosted Rust and PostgreSQL recovery gates.

## Purpose and non-goals

**Purpose.** Polls an authenticated Matrix `/sync` stream, validates bounded supported events, durably registers source identity and downstream delivery intent, and advances the opaque Matrix cursor only through PostgreSQL compare-and-set under an unexpired fenced lease.

**Non-goals.** It is not an event store for Matrix, a Matrix identity authority, a scheduler for arbitrary CEX work, a user/room governance service, a response sender, or a substitute for homeserver ordering, edit and redaction semantics.

## Authority and owned state

The poller owns only the transport-local observation that a source event was admitted to the Matrix transport inbox and that its relay delivery intent was durably enqueued. Matrix remains authoritative for source events and sync tokens; downstream services remain authoritative for accepted actions.

Durable state is limited to one cursor row per declared partition, immutable source-event identity and hash, and the first relay delivery intent. It owns no consumer, research, Agent, financial, World/Game or finality state. A process-local value, successful HTTP response or advanced in-memory token is never durable acceptance evidence.

## Source layout and entry points

- `src/main.rs`: fail-closed configuration, bounded `/sync` client, event validation, stable hashing and delivery identity, PostgreSQL lease acquisition, transactional inbox/outbox registration, poison isolation, and cursor CAS advancement.
- `Cargo.toml`: declares direct `sqlx`, `sha2` and `anyhow` dependencies required by the durable runtime.
- `services/matrix-entry-adapter/migrations/0001_transport_durability.sql`: shared Matrix transport schema and stored-procedure authority.

Catalog-bound entry points:

- `apps/matrix-bot-poller/src/main.rs`

Any new worker, partitioning strategy, storage owner or public command grammar must update the module catalog and this contract in the same commit.

## Interfaces and contracts

The executable consumes Matrix Client-Server API v3 `/sync`. It sends the configured bearer token, an explicit long-poll timeout, optional opaque `since` cursor and optional Matrix filter. Redirects are disabled and the response is rejected when either declared or observed bytes exceed `MATRIX_SYNC_MAX_BYTES`.

Supported input is currently a complete `m.room.message` event with `msgtype=m.text`, stable `event_id`, `room_id`, `sender` and non-blank body. Unsupported event kinds are ignored without acquiring CEX authority. A malformed event in the supported class is recorded as poison and prevents cursor advancement for the batch.

Each accepted message becomes `cex.matrix-inbound-event.v1`. Its source hash and relay payload hash are deterministic SHA-256 values. The relay delivery UUID is deterministically derived from source event ID, destination and payload hash. A replay with identical identity and bytes is accepted; reused identity with different bytes is a collision.

## Persistence, concurrency, and recovery

`cex_matrix_acquire_cursor_lease_v1` creates or acquires a partition lease and monotonically increases `lease_fence`. Another owner receives no row while the lease is live. `cex_matrix_accept_source_event_v1` and `cex_matrix_enqueue_delivery_v1` run inside one SQL transaction for every event in the returned sync batch. `cex_matrix_advance_cursor_v1` is the final statement and succeeds only for the expected partition, owner, fence, cursor revision and unexpired lease.

If any event registration, delivery enqueue or cursor CAS fails, the transaction rolls back and the cursor is not advanced. Restart, timeout or process loss therefore replays the same Matrix cursor and the same event/delivery identities. The immutable inbox and unique `(source_event_id, destination)` outbox constraint are the duplicate barrier.

Poison observations are durable and collision checked. They require an explicit one-time operator acknowledgement through the database contract; acknowledgement alone does not advance or rewrite a cursor. Multiple pollers may target one partition only through the fenced lease contract.

## Configuration and secrets

Required runtime inputs are `MATRIX_ACCESS_TOKEN` and `MATRIX_TRANSPORT_DATABASE_URL` or `DATABASE_URL`. Production-like profiles additionally require explicit `MATRIX_POLL_WORKER_ID` and `MATRIX_POLL_PARTITION_ID`.

Other bounded settings are `MATRIX_POLL_HOMESERVER`, `MATRIX_BOT_USER_ID`, `MATRIX_SYNC_FILTER`, `MATRIX_SYNC_TIMEOUT_MS`, `MATRIX_SYNC_MAX_BYTES`, `MATRIX_POLL_INTERVAL_MS`, `MATRIX_CURSOR_LEASE_SECONDS`, and `MATRIX_RELAY_DELIVERY_MAX_ATTEMPTS`. The cursor lease must exceed the sync timeout by at least five seconds. Production-like homeserver transport must use HTTPS.

The Matrix access token and database URL come from approved secret custody and must never be logged, committed, placed in payloads or reused as downstream credentials.

## Security and trust boundaries

The poller trusts only the configured homeserver transport endpoint and its bearer-token response. Caller-visible Matrix sender, room and event fields remain untrusted source claims until the homeserver and downstream identity contracts validate them. Bot-authored messages are ignored to prevent reply loops.

HTTP redirects are disabled, bodies are bounded before JSON decoding, supported events fail closed when incomplete, and telemetry excludes message bodies, tokens, sync cursors, raw payloads and high-cardinality personal identifiers. A Matrix event never grants CEX identity, entitlement, research, value or finality authority.

## Verification

Required commands:

```text
cargo fmt --all -- --check
cargo check --locked -p matrix-bot-poller --all-targets
cargo test --locked -p matrix-bot-poller --all-targets
cargo clippy --locked -p matrix-bot-poller --all-targets -- -D warnings
python3 scripts/check-matrix-runtime-wiring.py
python3 scripts/check-matrix-transport-durability.py
python3 scripts/test-matrix-transport-durability.py
bash scripts/check-matrix-transport-postgres.sh
```

Required behavioral focus:

- exact replay versus event/delivery collision;
- lease acquisition, revision CAS, stale fence rejection and expired-owner recovery;
- whole-batch rollback when registration or cursor advancement fails;
- restart after durable enqueue but before local acknowledgement;
- bounded `/sync`, redirect rejection, malformed supported-event poison isolation, and bot-loop suppression;
- absence of file-backed or process-local cursor authority.

The exact candidate SHA must pass the authoritative hosted workflow and appear in the generated immutable candidate manifest. Source formatting and static checks are necessary but not sufficient.

## Deployment and operations

Apply the Matrix transport migration with an approved schema owner before starting the poller. Run with a least-privilege role permitted to execute only the required transport procedures. Assign exactly one stable partition identity per Matrix account/filter stream; scale through disjoint partitions or the fenced lease contract, never through an unfenced shared cursor.

Readiness is false unless configuration, PostgreSQL schema and credential policy validate. Monitor sync lag, cursor revision age, lease expiry, poison count, oldest pending relay delivery and collision rejects. Rollback stops lease acquisition before switching binaries and preserves the exact cursor, inbox, outbox, history and poison records.

## Compatibility and change protocol

Matrix cursors are opaque and are never parsed or synthesized. Changes to event filtering, payload schema, source hashing, destination identity, partitioning, cursor representation or poison policy require a new explicit protocol version, replay fixtures and migration/rollback evidence.

Changes to authority, interfaces, persistence, configuration, retry semantics or deployment topology require this contract, the module catalog, Matrix durability design, executable tests, hosted gate wiring and a new shared candidate trigger. No module document may declare repository closure or production authorization.
