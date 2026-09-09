# matrix-entry-adapter module contract

Status: active module contract  
Workspace member: `services/matrix-entry-adapter`  
Package: `matrix-entry-adapter`  
Kind: `adapter-service`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `matrix-integration`  
Production authorization: `not_granted`

This contract defines the Sequence 54 Matrix adapter source boundary. Catalog
authority is `docs/module-catalog-v1.json`; the active recovery extensions are
`docs/matrix-result-reconciliation-v2.md` and
`docs/matrix-result-reconciliation-v3.md`. Source documentation is not hosted
qualification, independent approval, or production authorization.

## Purpose and non-goals

The adapter authenticates Matrix ingress, validates bounded event envelopes,
normalizes supported commands, projects bounded replies, forwards admitted work
to Consumer Entry, and exposes an authenticated read-only lookup for recovery of
an already durable result after an explicitly unknown relay outcome. It never
replays the original task during reconciliation and never grants domain authority.

The adapter does not own research truth, Ledger value, World/Game state, Nakama
authorization, Chain finality, provider outcomes, room membership, or human
identity. It does not host Agents, discover local models, or perform inference.

## Authority and owned state

The resident process owns HTTP normalization, ingress rate limiting, recent-event
suppression, signed Consumer Entry assertions, relay-header delivery validation,
read-only recovery response validation, and reply construction. Consumer Entry
owns task idempotency and durable task projection; the poller owns source cursor
admission; the relay owns durable delivery claims and Matrix send outcomes.

The package owns source definitions for Matrix transport migrations and operator
reconciliation migrations. Schema ownership does not authorize the resident
adapter process to mutate transport tables. The NOLOGIN
`cex_matrix_reconciler_runtime` capability may execute only the reviewed v3
security-definer entrypoint and has no direct table mutation privilege.

## Source layout and entry points

Catalog-bound entry points remain exact:

- `services/matrix-entry-adapter/src/main.rs` — fail-closed process startup;
- `services/matrix-entry-adapter/src/lib.rs` — validated public facade;
- `services/matrix-entry-adapter/src/runtime_profile.rs` — shared profile parser
  compatibility facade;
- `services/matrix-entry-adapter/migrations/0001_transport_durability.sql`;
- `services/matrix-entry-adapter/migrations/0002_source_observation_replay.sql`;
- `services/matrix-entry-adapter/migrations/0003_sync_recovery_and_send_receipts.sql`;
- `services/matrix-entry-adapter/migrations/0004_stream_scope_binding.sql`;
- `services/matrix-entry-adapter/migrations/0005_filter_definition_pins.sql`.

The public facade creates a private-field
`ValidatedMatrixAdapterEnvironment` only through
`validate_process_environment`, and `AppState::from_validated_env` consumes that
proof before `implementation.rs` can construct runtime state. This preserves the
reviewed pre-runtime profile-validation boundary while delivery and response
binding middleware remain private to the facade.

Additional security-relevant implementation boundaries are:

- `services/matrix-entry-adapter/src/implementation.rs` — event and forwarding
  implementation;
- `services/matrix-entry-adapter/src/delivery_binding.rs` — relay delivery and
  canonical payload commitment middleware;
- `services/matrix-entry-adapter/src/result_reconciliation.rs` — authenticated
  read-only result lookup;
- `services/matrix-entry-adapter/src/reconciliation_response_binding.rs` —
  bounded success-response middleware requiring the complete reconciliation
envelope, exact persisted delivery binding, and non-empty
  `task_id == raw.invocation_id` before 2xx leaves the adapter;
- operator migrations `0001` through `0006` below
  `services/matrix-entry-adapter/operator-migrations/`;
- `scripts/reconcile-matrix-adapter-result-v3.py` — canonical operator command;
- `scripts/matrix_operator_postgres_regression.py` — staged migration and
  PostgreSQL regression runner;
- `scripts/test-matrix-result-embedded-binding-postgres.sql` — honest retry,
  privilege, and hostile delivery-binding regression;
- `scripts/test-matrix-result-task-invocation-binding-postgres.sql` — missing and
  changed invocation regression for the v3 SQL boundary.

Any new route, target, durable object, migration, or authority must update this
contract, traceability, catalog metadata where applicable, and executable
verification in the same change.

## Interfaces and contracts

Primary routes include `POST /v1/matrix/events`,
`GET /v1/matrix/tasks/:id/projection`, and the authenticated read-only
`POST /v1/matrix/results/lookup`, plus session-auth governance, health, and
metrics surfaces. The event route accepts the relay payload only after its
canonical JSON bytes match `x-cex-payload-sha256` and its delivery ID matches both
idempotency headers in beta and production.

The delivery middleware rejects caller-provided `cex_delivery_binding`, then
injects the exact schema, relay source marker, delivery ID, payload commitment,
event, room, Matrix principal, and length-prefixed domain-separated request
fingerprint. The lookup independently recomputes the same fingerprint, signs
every authority component for Consumer Entry, accepts only an already durable
bound task result, and always returns `projected_reply=null` without enqueueing a
Matrix message.

A second facade middleware buffers only bounded reconciliation requests and
successful responses. It preserves inner authentication failures unchanged, but
replaces any malformed success with a fail-closed hold unless the response has
the exact read-only reconciliation envelope, `production_authorization` remains
`not_granted`, the task equals the raw invocation, Matrix identity scope matches,
and the embedded eight-field binding independently recomputes to the request.

## Persistence, concurrency, and recovery

Apply transport migrations `0001` through `0005` in order. Inbox,
source-observation, history, poison payload, send binding, send receipt,
stream-scope, and filter-definition evidence remain immutable under existing
lease and collision rules.

Apply operator migrations exactly in this order:

1. `0001_adapter_result_reconciliation.sql`;
2. `0002_runtime_roles.sql`;
3. `0003_adapter_result_evidence_binding.sql`;
4. `0004_adapter_result_runtime_reconciliation.sql`;
5. `0005_adapter_result_causal_binding.sql`;
6. `0006_adapter_result_embedded_delivery_binding.sql`.

Migration 0005 separates stable result identity from append-only lookup
observations and installs the row-locking v2 core. Migration 0006 installs v3,
revokes runtime access to v2, requires non-empty `task_id == raw.invocation_id`,
requires the persisted Consumer Entry result to contain the exact eight-field
relay binding, recomputes the fingerprint, and then delegates to the owner-only
v2 core. A valid first repair writes one `dead_letter -> sent` transition; an
exact retry appends an observation and returns `replay`; changed identity, task,
invocation, result, or binding fails closed.

## Configuration and secrets

Adapter startup validates profile, ingress token, Consumer Entry endpoint and
credential, issuer/key registry, approved registry revision, bot identity,
rate-limit and recent-event state, and bounded limits. Production-like startup
fails before listening when mandatory authority is absent or conflicting.

The operator uses `MATRIX_ENTRY_ADAPTER_BASE_URL`,
`MATRIX_ENTRY_INGRESS_TOKEN`, `MATRIX_RECONCILIATION_DATABASE_URL`,
`MATRIX_RECONCILIATION_DATABASE_CA_FILE`, and the absolute reviewed executable
in `MATRIX_RECONCILIATION_PSQL`. Remote PostgreSQL requires `verify-full`, a
trusted non-writable CA, and channel binding. The child receives a closed
environment without inherited service files, host overrides, generic database
URLs, caller PG options, PATH, or loader injection variables.

Tokens, database URLs, raw replay bytes, session secrets, and private authority
material must not appear in command arguments, output JSON, logs, metrics,
issues, traces, or repository fixtures.

## Security and trust boundaries

A parseable Matrix sender or HTTP success is not identity or result authority.
Ingress credentials, relay delivery headers, payload hashing, reserved-field
rejection, signed assertions, issuer/key selection, exact persisted result
binding, task-to-invocation equality, bounded response parsing, and the
PostgreSQL row lock are independent checks. Event ID is only a candidate locator
and cannot authorize reconciliation.

The operator refuses redirects, duplicate JSON keys, oversized responses,
noncanonical delivery IDs, changed commitments, cross-room or cross-principal
results, changed or missing task invocation, unbound cached results, remote
plaintext databases, untrusted CA or executable paths, and inherited hostile PG
configuration. It performs one read-only lookup and one v3 function call and
never sends a Matrix event or recreates the business request.

Runtime roles receive no DDL, TRUNCATE, ownership, trigger-disable, blanket DML,
or direct transport-table privileges. Repository source checks do not prove real
credential custody, homeserver behavior, hosted execution, independent review,
or production topology.

## Verification

Catalog compatibility commands remain exact:

```text
cargo test -p matrix-entry-adapter
cargo clippy -p matrix-entry-adapter --all-targets -- -D warnings
bash scripts/check-matrix-source-observation-postgres.sh
```

Sequence 54 qualification uses the complete set:

```text
python3 scripts/check-matrix-result-reconciliation.py
python3 scripts/check-matrix-result-reconciliation-security-v2.py
python3 scripts/check-matrix-result-reconciliation-security-v3.py
python3 scripts/check-matrix-result-reconciliation-traceability-v3.py
python3 scripts/reconcile-matrix-adapter-result-v3.py --self-test
python3 scripts/test-matrix-operator-postgres-runner-v4.py
cargo fmt -p matrix-entry-adapter -- --check
cargo test --locked -p matrix-entry-adapter --all-targets
cargo clippy --locked -p matrix-entry-adapter --all-targets -- -D warnings
python3 scripts/matrix_operator_postgres_regression.py
```

The operator runner executes historical migrations and regressions before the v3
runtime revocation, then applies migration 0006 twice and executes missing,
changed, extended, replay, privilege, transition-count, observation-count, and
task/invocation checks against disposable PostgreSQL 16. Adapter package tests
also prove that malformed success envelopes and missing, changed, or extended
bindings cannot leave the facade as 2xx. A source checker or zero-step hosted job
cannot substitute for a non-empty executed qualification result.

## Deployment and operations

Deploy privately with a dedicated runtime identity and keep schema-owner
credentials outside resident processes. Readiness separately reports listener,
issuer-registry validity, Consumer Entry reachability, transport schema, and
operator migration head 0006. Record image, source/tree, profile, credential IDs,
delivery fingerprint, result digest, and observation digest without secrets.

Monitor ingress failures, payload-hash conflicts, reserved-binding rejection,
unknown-outcome age, dead letters, lookup failures, reconciliation
success-response holds, task/invocation mismatches, replay, collision,
observation age, poison holds, cursor age, receipt conflicts, and queue age.
Rollback first stops ingress, polling, relay claims, and reconciliation; preserves
every immutable delivery/result/observation identity; and deploys only a
schema-compatible prior binary or reviewed forward repair. Deleting or replaying
migrations is not rollback.

## Compatibility and change protocol

The validated facade remains the only construction path. Re-exposing private
constructors, restoring local provider execution, weakening profile conflict
handling, accepting identity-only or unbound results, accepting a task without
its exact raw invocation, or restoring runtime access to v1/v2 is a breaking
security change. Historical functions remain only for owner-controlled
compatibility and regression ordering.

Changes to fingerprint encoding, relay binding fields, result/evidence schemas,
reconcilable error classes, grants, assertion semantics, cursor/filter scope, or
reply protocol require a versioned migration, cross-implementation fixtures,
hostile tests, traceability, exact-head execution, and fresh independent review.
No document or source checker grants production authorization.
