# matrix-bot-relay module contract

Status: active module contract  
Workspace member: `apps/matrix-bot-relay`  
Package: `matrix-bot-relay`  
Kind: `application`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `matrix-integration`  
Production authorization: `not_granted`

This contract defines the Sequence 54 source boundary. It is not real homeserver,
PostgreSQL, exact-SHA hosted or production evidence.

## Purpose and non-goals

The relay persists incoming Matrix events, claims durable deliveries, calls the
entry adapter and sends bounded replies to the original Matrix room. It does not
grant user, research, financial, World/Game or Chain authority and does not host
or execute participating Agents.

Transport acknowledgement is not a downstream business result. When the adapter
may have completed but its response is lost, the relay holds the delivery. A
separate principal-bound lookup and least-privilege operator transition can close
that exact held delivery without replaying the business request.

## Authority and owned state

The relay owns delivery claims, bounded attempts, immutable delivery history,
outgoing send-scope bindings and validated Matrix send receipts. The same delivery
UUID remains the Matrix transaction ID. External services retain business and
messaging authority.

Dead-letter means no automatic retry is authorized. It may represent an unknown
remote effect rather than definite non-execution. Only the reconciliation
function may transform an eligible adapter unknown-outcome hold to `sent`, and it
must atomically store the exact durable result and lookup evidence.

## Source layout and entry points

Catalog-bound entry points:

- `apps/matrix-bot-relay/src/main.rs` — ingress, claim, dispatch, completion,
  send binding and receipt handling;
- `apps/matrix-bot-relay/src/response_contract.rs` — bounded response and reply
  validation;
- `apps/matrix-bot-relay/src/runtime_profile.rs` — shared profile re-export.

Shared transport migrations `0001` through `0005` and operator migrations `0001`
through `0006` are owned under `services/matrix-entry-adapter/`. The operator
command is `scripts/reconcile-matrix-adapter-result.py`, the canonical facade
that loads only `scripts/reconcile-matrix-adapter-result-v3.py`.

Operator migration head: `0006_adapter_result_embedded_delivery_binding.sql`
Runtime reconciliation function: `cex_matrix_reconcile_adapter_result_v3`
Recovery contract: `docs/matrix-result-reconciliation-v3.md`

These declarations describe the current operation, not historical compatibility.
The v1/v2 contracts and owner-only SQL core cannot authorize runtime repair.

## Interfaces and contracts

`POST /v1/inbound/matrix-event` authenticates before persistence and returns `202`
only after source and adapter-delivery intent are durable. Adapter calls use exact
delivery, payload-hash and idempotency headers. Unknown destinations fail closed.

Matrix send uses Client-Server v3 with immutable delivery UUID as transaction ID.
Only HTTP 200 plus a bounded valid event ID and no Matrix error is accepted.
Partial, malformed, oversized or receipt-free responses are not success. Reply
objects may contain only `msgtype` and `body`, use `m.text` or `m.notice`, and
remain bound to the original room.

The response-loss recovery contract is documented in
`docs/matrix-result-reconciliation-v3.md`. The relay does not automatically call
it from its main claim loop; this avoids creating an implicit authority that
could convert an ambiguous effect to success without operator intent.

## Persistence, concurrency, and recovery

Claims commit before network I/O. Reply enqueue and adapter completion share one
transaction. Before a Matrix send, the relay binds delivery, payload hash, room,
homeserver and credential fingerprint. A changed credential or endpoint holds the
operation rather than creating a new deduplication scope.

Validated Matrix send receipt and `sent` completion commit atomically. Database
triggers reject send completion without matching immutable receipt. Identical
receipt replay is safe; a different event ID collides.

Adapter timeout, network loss, interrupted body, invalid JSON, unknown HTTP status,
oversized response, duplicate outcome and internal post-I/O failure are recorded
as explicit unknown-outcome dead letters. They are not blindly reclaimed. The
operator reconciliation path queries Consumer Entry through the adapter, verifies
the exact principal scope and stores the recovered result before closing the
adapter leg. The v3 result must contain the exact persisted eight-field delivery
binding and a non-empty `task_id == raw.invocation_id`; event identity alone is
only a lookup locator. Honest retries retain stable result identity while each
new observation is append-only. Migration 0006 denies runtime v1/v2 execution.

An adapter delivery repaired to `sent` means that its existing business result
was recovered. It does not prove a Matrix reply was sent or received. The
read-only lookup returns no projected reply; the reconciler neither recreates
the business request nor enqueues or sends a Matrix event. Any later reply
requires its own reviewed transport intent, original-room binding and receipt.

Rollback stops claims, fences workers and retains identities, bindings, receipts,
unknown holds and reconciliation records. Older workers that automatically retry
unknown adapter effects must not be restarted against the current schema.

## Configuration and secrets

Required relay settings include `MATRIX_TRANSPORT_DATABASE_URL` or `DATABASE_URL`,
`MATRIX_RELAY_WORKER_ID`, `MATRIX_RELAY_INGRESS_TOKEN`,
`MATRIX_ENTRY_ADAPTER_TOKEN` or `MATRIX_ENTRY_INGRESS_TOKEN`, and
`MATRIX_ACCESS_TOKEN`.

| Key | Default / bounds |
|---|---|
| `MATRIX_BOT_RELAY_BIND` / `MATRIX_BOT_RELAY_BIND_ADDR` | `127.0.0.1:8092` |
| `MATRIX_ADAPTER_BASE_URL` | local `http://127.0.0.1:8091` |
| `MATRIX_HOMESERVER_BASE_URL` | local `http://127.0.0.1:8008` |
| `MATRIX_RELAY_CLAIM_LEASE_SECONDS` | 60; 5–3600 |
| `MATRIX_RELAY_HTTP_TIMEOUT_SECONDS` | 20; at least five seconds below lease |
| `MATRIX_RELAY_POLL_INTERVAL_MS` | 500; minimum 50 |
| `MATRIX_RELAY_INGRESS_MAX_BYTES` | maximum 1048576 |
| `MATRIX_RELAY_MAX_RESPONSE_BYTES` | maximum 4194304 |
| `MATRIX_RELAY_DELIVERY_MAX_ATTEMPTS` | 8; 1–100 |

Production-like endpoints require HTTPS and credentials are pairwise distinct.
Secrets must not enter logs, snapshots, receipts, command arguments or evidence.
The credential fingerprint is operational binding data, not a credential source.

## Security and trust boundaries

Ingress is authenticated before persistence; redirects and unbounded bodies are
disabled. Original-room binding prevents cross-room sends. Server status and
payload remain untrusted until the complete receipt validates.

The reconciliation operator must use a separate database login inheriting only
`cex_matrix_reconciler_runtime`. The runtime command reads its adapter token and
database URL from environment variables, validates duplicate-free JSON and exact
outer/source/principal/task identities, and sends result bytes to psql only over
stdin. It cannot claim deliveries or perform direct table DML.

A Matrix event receipt proves a messaging response, not Ledger value, research
quality or Chain finality. A recovered Consumer Entry result proves only that the
exact principal-bound operation result was durably recorded.

## Verification

Required catalog commands:

```text
cargo test --locked -p matrix-bot-relay --all-targets
cargo clippy --locked -p matrix-bot-relay --all-targets -- -D warnings
python3 scripts/check-matrix-recovery-contract.py
python3 scripts/test-matrix-recovery-contract.py
python3 scripts/check-matrix-lock-coherence.py
```

Additional reconciliation checks:

```text
python3 scripts/check-matrix-result-reconciliation.py
python3 scripts/reconcile-matrix-adapter-result.py --self-test
python3 scripts/check-matrix-result-reconciliation-security-v3.py
python3 scripts/check-matrix-result-reconciliation-traceability-v3.py
python3 scripts/reconcile-matrix-adapter-result-v3.py --self-test
python3 scripts/matrix_operator_postgres_regression.py
bash scripts/check-matrix-operator-postgres.sh
```

Database regression must cover wrong role/fence, receipt-required completion,
receipt collision, endpoint binding drift, each adapter unknown-outcome class,
exact recovered payload persistence, side-effect-free replay and changed-payload
collision. After the complete 0001–0006 operator chain, the actual canonical CLI
must execute successfully as the least-privilege reconciler, while direct v1/v2
calls fail. Changed task/invocation, delivery, payload, event, room, principal,
fingerprint or embedded binding must not change the delivery state. A second
honest lookup with fresh timestamps must append an observation without a second
terminal transition. Source checks do not replace PostgreSQL or real
response-loss testing.

## Deployment and operations

Apply transport and operator migrations under the schema owner, then remove that
credential. Resident relay processes use the relay runtime role. Reconciliation
runs use a separate short-lived reconciler identity and an approved operator
surface.

Readiness requires transport functions, receipt/binding functions and the current
operator reconciliation function. Monitor pending age, unknown-outcome age,
reconciliation failures, receipt conflicts and dead letters. Operators must
distinguish effect-unknown holds from confirmed rejection.

Rollback stops ingress and new claims, fences relay workers, restores the previous
schema-compatible artifact, and preserves delivery identities, receipts,
unknown-outcome holds and reconciliation evidence for later recovery.

A response-loss rehearsal must demonstrate: Consumer Entry commits the exact
result, the adapter response is lost, the relay holds, the operator lookup returns
the same result, PostgreSQL stores evidence and closes the adapter delivery once,
and no second business request or cross-room reply occurs.

## Compatibility and change protocol

Destination strings, payload hashes and delivery UUIDs remain unchanged. Existing
terminal rows are preserved; no history is rewritten. Operator migration 0004
is historical compatibility only. Migration 0005 separates stable result
identity from observations. Migration 0006 makes v3 the only runtime repair
entrypoint; historical v1/v2 functions remain owner-controlled, not operator
alternatives. Do not restore their runtime grants to make an old client work.

Changes to error classification, result schema, role grants, retry scope, endpoint
or credential binding, payload bounds or reply fields require this contract,
Matrix reconciliation contract, migrations, positive/hostile tests and fresh
exact-tree evidence. No repository document grants production authorization.
