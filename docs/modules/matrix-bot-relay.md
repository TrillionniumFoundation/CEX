# matrix-bot-relay module contract

Status: active module contract  
Workspace member: `apps/matrix-bot-relay`  
Package: `matrix-bot-relay`  
Kind: `application`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `matrix-integration`  
Production authorization: `not_granted`

This contract is indexed by `docs/module-catalog-v1.json`. It defines the module boundary for one exact repository tree; it is not release evidence. The durable runtime remains a repository candidate until its exact SHA passes hosted Rust, PostgreSQL recovery and live Matrix evidence gates.

## Purpose and non-goals

**Purpose.** Owns delivery of the shared Matrix transport outbox. It relays admitted Matrix events to the Matrix entry adapter, atomically creates an optional homeserver reply delivery, and sends replies with a stable Matrix transaction identity so response loss can be retried without creating a second message.

**Non-goals.** It does not authorize Matrix users, grant CEX identity or entitlement, parse privileged CEX administration, own downstream task/research facts, debit value, decide Chain finality, or act as a Matrix homeserver.

## Authority and owned state

The relay is authoritative only for outbox claim ownership, delivery attempt history and transport completion under the shared PostgreSQL contract. Adapter, Consumer Entry, Hepta, Ledger, TRNM and Matrix retain their own domain authority.

Owned state is limited to durable delivery intents, monotonically fenced claims, attempt counts, retry/dead-letter state, immutable transition history and poison observations. Direct HTTP ingress is a compatibility admission path that first persists the source event and outbox delivery; it is never a process-local queue.

## Source layout and entry points

- `src/main.rs`: fail-closed configuration, authenticated compatibility ingress, health/metrics, PostgreSQL outbox worker, adapter client, homeserver client, bounded response handling, retry classification and deterministic delivery identity.
- `Cargo.toml`: declares direct `sqlx`, `sha2` and `anyhow` dependencies required by the durable runtime.
- `services/matrix-entry-adapter/migrations/0001_transport_durability.sql`: shared inbox/outbox, claim/fence, transition history, lookup and poison authority.

Catalog-bound entry points:

- `apps/matrix-bot-relay/src/main.rs`

Any new destination, worker, response format, queue owner or public ingress must update the module catalog, durability design and this document in the same commit.

## Interfaces and contracts

The compatibility route `POST /v1/inbound/matrix-event` requires `x-relay-token`, validates a bounded Matrix event, verifies optional `x-cex-payload-sha256`, accepts an optional stable `x-cex-delivery-id`, and durably performs source-event registration plus adapter-delivery enqueue before returning `202 Accepted`.

The worker currently recognizes exactly two versioned destinations:

- `matrix-relay-adapter-v1`: calls the Matrix entry adapter at `/v1/matrix/events` with `x-entry-token`, `x-cex-delivery-id`, `x-cex-payload-sha256` and `idempotency-key` bound to the immutable delivery;
- `matrix-homeserver-v1`: sends `m.room.message` through Matrix Client-Server API v3 using the delivery UUID as the Matrix transaction ID and as the idempotency key.

Unknown destinations fail permanently. Successful adapter responses may include `projected_reply`; reply enqueue and completion of the adapter delivery occur in the same database transaction. A downstream HTTP success is not accepted as CEX domain authority, only as transport evidence for the owning delivery contract.

## Persistence, concurrency, and recovery

`cex_matrix_claim_delivery_v1` uses `FOR UPDATE SKIP LOCKED`, increments `lease_fence`, persists claim history and returns immutable payload identity. The runtime claims one delivery per worker iteration; multiple replicas may cooperate because stale owners cannot finish a claim with a different owner or fence.

`cex_matrix_finish_delivery_v1` accepts only an unexpired matching claim. Success becomes `sent`; retryable failure becomes `pending` until `max_attempts`; permanent failure or exhausted attempts becomes `dead_letter`. Every transition is appended to immutable history.

Adapter transport failure, timeout or lost response retries the same delivery ID and payload hash. Matrix send failure or response loss retries the same Matrix transaction ID, so the homeserver must treat the retry as the same send operation. If the relay dies after a possible remote effect and before local completion, lease expiry exposes the same immutable claim for recovery; it never creates a new operation identity.

Malformed stored payloads and source/delivery identity mismatch are recorded as poison and dead-lettered. The runtime contains no `/tmp` JSON queue, `VecDeque` authority or file-backed replay state.

## Configuration and secrets

Required inputs are `MATRIX_TRANSPORT_DATABASE_URL` or `DATABASE_URL`, `MATRIX_ENTRY_ADAPTER_TOKEN`, and `MATRIX_ACCESS_TOKEN`. Production-like profiles additionally require `MATRIX_RELAY_WORKER_ID` and a distinct `MATRIX_RELAY_INGRESS_TOKEN`.

Other bounded settings are `MATRIX_BOT_RELAY_BIND`, `MATRIX_RELAY_POLL_INTERVAL_MS`, `MATRIX_RELAY_LEASE_SECONDS`, `MATRIX_RELAY_BATCH_SIZE`, `MATRIX_RELAY_MAX_ATTEMPTS`, `MATRIX_ENTRY_ADAPTER_URL`, `MATRIX_HOMESERVER_BASE_URL`, and `MATRIX_RELAY_HTTP_TIMEOUT_SECONDS`. The current batch size is fixed to one and the lease must exceed the HTTP timeout by at least five seconds.

In production-like profiles, ingress, adapter and Matrix credentials must be pairwise distinct, external endpoints must use HTTPS, and all credentials must come from approved secret custody.

## Security and trust boundaries

Inbound compatibility requests are authenticated before persistence. Redirects are disabled for adapter and Matrix clients. HTTP request and response bodies are bounded. Stored payload identity is checked against source event identity before dispatch, and raw message bodies, bearer tokens, database URLs, cursors and high-cardinality personal identifiers are excluded from telemetry.

Matrix sender/room fields are source claims, not CEX authorization. Adapter replies are presentation projections and cannot create identity, task, research, value or finality authority. Credentials for ingress, downstream adapter and Matrix homeserver are separated to limit confused-deputy and lateral-movement risk.

## Verification

Required commands:

```text
cargo fmt --all -- --check
cargo check --locked -p matrix-bot-relay --all-targets
cargo test --locked -p matrix-bot-relay --all-targets
cargo clippy --locked -p matrix-bot-relay --all-targets -- -D warnings
python3 scripts/check-matrix-runtime-wiring.py
python3 scripts/check-matrix-transport-durability.py
python3 scripts/test-matrix-transport-durability.py
bash scripts/check-matrix-transport-postgres.sh
```

Required behavioral focus:

- authenticated direct ingress and exact replay/collision;
- concurrent claim fencing, wrong-owner finish rejection and expired-claim recovery;
- adapter timeout, response loss, malformed response and permanent/retryable status classification;
- atomic adapter-complete/reply-enqueue behavior;
- stable Matrix transaction ID across transport timeout and process restart;
- max-attempt dead letter, poison isolation and immutable history;
- absence of local file or memory queue authority.

The exact candidate SHA must pass the authoritative hosted workflow and appear in the generated immutable candidate manifest. A green health endpoint or local static test is not production evidence.

## Deployment and operations

Apply the shared migration before starting the relay. Run under a least-privilege role permitted to execute the admission, enqueue, claim, finish and poison procedures. Readiness is false unless database schema and security configuration validate; liveness reports process health only.

Monitor pending and claimed delivery age, attempts, expired claims, dead-letter count, poison count, adapter outcome rate and Matrix outcome rate. Rollback stops new claims, waits for or fences current claims, preserves all immutable delivery/history evidence, and resumes only with a schema/protocol-compatible binary.

## Compatibility and change protocol

Destination names, payload hashes, delivery IDs and Matrix transaction IDs are protocol identities. A new adapter or homeserver API version requires a new destination, compatibility fixtures and shadow/replay evidence. Unknown commands or destinations must never map to a privileged default.

Changes to authority, routes, payloads, persistence, configuration, retry semantics or topology require this contract, the module catalog, Matrix durability design, executable tests, hosted gate wiring and a new shared candidate trigger. No module document may declare repository closure or production authorization.
