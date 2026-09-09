# matrix-entry-adapter module contract

Status: active module contract  
Workspace member: `services/matrix-entry-adapter`  
Package: `matrix-entry-adapter`  
Kind: `adapter-service`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `matrix-integration`  
Production authorization: `not_granted`

This contract describes the Sequence 54 source boundary. It is not hosted
qualification, an approval, or production authorization. Catalog authority is
`docs/module-catalog-v1.json`; the security extension is
`docs/matrix-result-reconciliation-v2.md`.

## Purpose and non-goals

The adapter authenticates Matrix ingress, validates bounded event envelopes,
normalizes supported commands, forwards requests to Consumer Entry, and exposes
an authenticated read-only lookup used to recover an already durable result
after an unknown adapter response. It does not repeat the business request during
reconciliation.

It does not authorize research, Ledger value, World/Game state, Chain finality,
room membership, or human identity. It does not host Agents, discover local
models, or perform inference.

## Authority and owned state

The resident process owns HTTP normalization, ingress rate limiting,
recent-event suppression, session-assertion construction, Consumer Entry lookup
response validation, and bounded reply construction. Consumer Entry owns task
idempotency and task projection. The poller owns cursor/event admission. The
relay owns durable delivery claims and Matrix sends.

The package owns source definitions for the shared Matrix transport schema and
operator reconciliation migrations. Schema ownership does not give the resident
adapter process permission to edit transport history. The separate NOLOGIN
`cex_matrix_reconciler_runtime` capability can execute only the current reviewed
security-definer reconciliation function; it has no direct table mutation
rights.

## Source layout and entry points

Catalog-bound entry points are listed exactly:

- `services/matrix-entry-adapter/src/main.rs` — fail-closed process startup;
- `services/matrix-entry-adapter/src/lib.rs` — validated public facade;
- `services/matrix-entry-adapter/src/runtime_profile.rs` — shared profile parser
  compatibility facade;
- `services/matrix-entry-adapter/migrations/0001_transport_durability.sql`;
- `services/matrix-entry-adapter/migrations/0002_source_observation_replay.sql`;
- `services/matrix-entry-adapter/migrations/0003_sync_recovery_and_send_receipts.sql`;
- `services/matrix-entry-adapter/migrations/0004_stream_scope_binding.sql`;
- `services/matrix-entry-adapter/migrations/0005_filter_definition_pins.sql`.

Additional security-relevant source boundaries are:

- `services/matrix-entry-adapter/src/implementation.rs` — ingress and forwarding
  implementation;
- `services/matrix-entry-adapter/src/result_reconciliation.rs` — exact-delivery
  read-only result lookup;
- operator migrations `0001` through `0005` under
  `services/matrix-entry-adapter/operator-migrations/`;
- `scripts/reconcile-matrix-adapter-result.py` — one-delivery operator command;
- `scripts/matrix_operator_postgres_regression.py` — exact migration/regression
  runner;
- `scripts/test-matrix-result-causal-binding-postgres.sql` — hostile and honest
  retry regression.

Any new route, target, durable object, migration, or authority must update this
contract, the module catalog where applicable, traceability, and executable
verification in the same change.

## Interfaces and contracts

Primary routes include:

- `POST /v1/matrix/events` — authenticated bounded Matrix event admission;
- `GET /v1/matrix/tasks/:id/projection` — bounded projection read;
- `POST /v1/matrix/results/lookup` — authenticated read-only recovery lookup;
- session-auth governance status, validation, and reload routes;
- `/health` and `/metrics`.

The result lookup accepts canonical `delivery_id`, `payload_sha256`, `event_id`,
`sender`, `room_id`, and the domain-separated `request_fingerprint`. It recomputes
the fingerprint, places it in the signed Consumer Entry assertion, and requires
Consumer Entry to echo every delivery component. Success also requires exact
cached source/scope identity, a bounded non-empty task identity,
`reconciliation.schema=cex.matrix.adapter-result-reconciliation.v2`,
`causal_binding=delivery_payload_fingerprint`, and `read_only=true`.

The route always returns `projected_reply=null`; it does not enqueue a Matrix
message or change outbox state.

## Persistence, concurrency, and recovery

Apply transport migrations `0001` through `0005` in order. Inbox,
source-observation, history, poison payload, send binding, send receipt,
stream-scope, and filter-definition evidence remain immutable.

Apply operator migrations in this order:

1. `0001_adapter_result_reconciliation.sql`;
2. `0002_runtime_roles.sql`;
3. `0003_adapter_result_evidence_binding.sql`;
4. `0004_adapter_result_runtime_reconciliation.sql`;
5. `0005_adapter_result_causal_binding.sql`.

Migration `0005` preserves prior migration bytes, backfills the deterministic
request fingerprint, adds immutable append-only observation rows, installs
`cex_matrix_reconcile_adapter_result_v2`, revokes runtime execution of v1, and
grants only v2. The v2 function locks the exact outbox row and independently
recomputes the fingerprint from its persisted delivery ID, payload hash, source
event, principal, and room.

A first valid unknown-outcome repair stores one stable delivery/result binding,
closes `dead_letter -> sent`, writes one history transition, and appends one
observation. A later lookup with the same fingerprint and exact result returns
`replay` and may append a different bounded observation without writing a second
transition. Changed delivery, payload, principal, room, task, result, or
fingerprint is a collision. Evidence older than 15 minutes or more than 5 minutes
in the future is rejected.

## Configuration and secrets

Adapter runtime validates profile, ingress token, Consumer Entry endpoint/token,
session issuer/key/secret, approved issuer-registry revision, bot identity,
rate-limit path, replay state, and bounded limits. Production-like startup fails
closed on missing or conflicting authority.

The operator command uses:

- `MATRIX_ENTRY_ADAPTER_BASE_URL`;
- `MATRIX_ENTRY_INGRESS_TOKEN`;
- `MATRIX_RECONCILIATION_DATABASE_URL`;
- `MATRIX_RECONCILIATION_DATABASE_CA_FILE` for non-loopback PostgreSQL;
- `MATRIX_RECONCILIATION_PSQL`, an absolute canonical reviewed executable.

Remote PostgreSQL requires `verify-full`, the approved CA, and channel binding.
Plain PostgreSQL is accepted only for an explicitly enabled literal loopback IP
test. The child process receives a closed environment and does not inherit
service files, host overrides, caller PG options, generic database URLs, PATH, or
loader injection variables.

Tokens, database URLs, raw result bytes, replay-store bytes, and session secrets
must not appear in command arguments, result JSON, logs, traces, metrics, issues,
or repository fixtures.

## Security and trust boundaries

A parseable Matrix sender is not authenticated identity. Ingress credentials,
signed assertions, issuer/key selection, tenant mapping, exact delivery
fingerprint, bounded bodies, redirect refusal, response schema, cached source
identity, and the PostgreSQL transition are independent checks.

The operator command accepts no redirect, duplicate JSON key, oversized response,
noncanonical delivery UUID, changed payload commitment, cross-room/user result,
changed task, unbound response, remote plaintext database, unpinned psql, or
untrusted writable CA/binary path. It performs exactly one read-only lookup and
one v2 function call; it never sends a Matrix event or replays the original task.

Runtime roles receive no DDL, TRUNCATE, trigger-disable, ownership, or blanket
DML. Repository checks still do not prove credential custody, a real homeserver,
runner execution, independent review, or production topology.

## Verification

Catalog compatibility commands are retained exactly:

```text
cargo test -p matrix-entry-adapter
cargo clippy -p matrix-entry-adapter --all-targets -- -D warnings
bash scripts/check-matrix-source-observation-postgres.sh
```

Sequence 54 qualification uses the stricter complete set:

```text
python3 scripts/check-matrix-result-reconciliation.py
python3 scripts/check-matrix-result-reconciliation-security-v2.py
python3 scripts/reconcile-matrix-adapter-result.py --self-test
cargo fmt -p matrix-entry-adapter -- --check
cargo test --locked -p matrix-entry-adapter --all-targets
cargo clippy --locked -p matrix-entry-adapter --all-targets -- -D warnings
python3 scripts/matrix_operator_postgres_regression.py
```

The operator runner applies all five operator migrations twice and executes the
baseline, evidence-hardening, payload-persistence, real-retry, privilege, stale
evidence, and causal-collision regressions against disposable PostgreSQL 16.
Source checks or zero-step hosted jobs cannot substitute for non-empty execution.

## Deployment and operations

Deploy privately with a dedicated runtime identity. Keep schema-owner credentials
outside resident processes. Use a separate login inheriting only
`cex_matrix_reconciler_runtime` for the command. Record exact image, source/tree,
transport and operator migration heads, normalized profile, credential IDs,
Consumer Entry endpoint, request fingerprint, result digest, and observation
digest without retaining secrets.

Monitor ingress failures, unknown-outcome age, dead-letter count, lookup failure,
reconciliation success/replay/collision, observation age, poison holds, cursor
age, send receipt conflicts, and queue age. Readiness distinguishes listener
health, issuer-registry validity, Consumer Entry reachability, transport schema,
and operator migration head.

Rollback first stops ingress, polling, relay claims, and reconciliation; it then
preserves every immutable delivery/result/observation identity and deploys only a
schema-compatible prior binary or a reviewed forward repair. Replaying or deleting
migrations is not rollback.

## Compatibility and change protocol

The validated facade remains the only construction path. Re-exposing private
constructors, restoring local model execution, weakening profile conflict
handling, accepting an identity-only result, or restoring runtime access to v1
is a breaking security change.

The v1 database function remains only for historical compatibility; runtime
execution is revoked. Migration `0005` is additive and preserves existing
immutable rows. Any change to the fingerprint encoding, result/evidence schema,
reconcilable error classes, grants, assertion, cursor/filter scope, or reply
protocol requires a versioned migration, cross-implementation fixtures, hostile
tests, traceability, and fresh exact-tree evidence. No document or source checker
grants production authorization.
