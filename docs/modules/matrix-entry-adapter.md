# matrix-entry-adapter module contract

Status: active module contract  
Workspace member: `services/matrix-entry-adapter`  
Package: `matrix-entry-adapter`  
Kind: `adapter-service`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `matrix-integration`  
Production authorization: `not_granted`

This contract describes the current Sequence 54 repository source boundary. It
is not hosted qualification, production approval or evidence that a queued job
executed. Catalog authority is `docs/module-catalog-v1.json`.

## Purpose and non-goals

The adapter authenticates Matrix ingress, validates bounded event envelopes,
normalizes supported commands, forwards requests to Consumer Entry and exposes a
read-only result lookup for response-loss recovery. It does not authorize
research, account value, World or Game state, Chain finality, room membership or
human identity. It does not host Agents, discover local models or perform model
inference.

The response-loss lookup never repeats the original business request. It asks
Consumer Entry for an already durable result bound to the exact event, principal
and room. Delivery-state repair is performed only by the separate least-privilege
PostgreSQL reconciliation authority.

## Authority and owned state

The process owns HTTP normalization, ingress rate limiting, recent-event
suppression, session-auth issuer selection, local reply construction and the
validation of Consumer Entry lookup responses. Consumer Entry owns business
idempotency and principal authorization. The poller owns cursor leases and event
admission. The relay owns durable outbox claims and Matrix sends.

The package owns the shared Matrix transport schema source and the operator
reconciliation migrations, but schema ownership does not grant resident adapter
processes permission to edit transport history. `cex_matrix_reconciler_runtime`
is a separate NOLOGIN capability role. Reconciliation evidence is immutable and
cannot be updated or deleted through normal runtime roles.

## Source layout and entry points

Catalog-bound entry points are:

- `services/matrix-entry-adapter/src/main.rs` — validates process profile before
  Tokio and starts the listener;
- `services/matrix-entry-adapter/src/lib.rs` — public validated facade and router;
- `services/matrix-entry-adapter/src/runtime_profile.rs` — compatibility re-export
  of the shared profile parser;
- transport migrations `0001` through `0005` under
  `services/matrix-entry-adapter/migrations/`.

Additional authoritative source used by this module includes:

- `src/implementation.rs` — private route/state implementation;
- `src/result_reconciliation.rs` — authenticated read-only lookup adapter;
- operator migrations `0001` through `0004` under
  `services/matrix-entry-adapter/operator-migrations/`;
- `scripts/reconcile-matrix-adapter-result.py` — one-delivery operator command;
- `scripts/matrix_operator_postgres_regression.py` — exact operator-chain runner.

Any new route, target, durable object or authority must update this contract,
module catalog where applicable, traceability and executable verification in the
same change.

## Interfaces and contracts

`validate_process_environment` reads `MATRIX_ENTRY_RUNTIME_PROFILE`,
`CEX_RUNTIME_PROFILE` and `APP_ENV`, rejects conflicting or malformed values and
returns a non-inventible `ValidatedMatrixAdapterEnvironment`. The token is
consumed by `AppState::from_validated_env`; callers cannot construct the private
implementation state directly.

Primary routes include:

- `POST /v1/matrix/events` — authenticated bounded Matrix event admission;
- `GET /v1/matrix/tasks/:id/projection` — bounded projection read;
- `POST /v1/matrix/results/lookup` — authenticated read-only result recovery;
- session-auth governance status, validate and reload routes;
- `/health` and `/metrics`.

`POST /v1/matrix/results/lookup` accepts exact `event_id`, `room_id` and `sender`.
It signs a request-fingerprint-bound assertion and calls only Consumer Entry
`POST /v1/matrix/messages/result`. Success requires exact outer identities, exact
cached source identity, exact identity scope, non-empty task identity and
`reconciliation.read_only=true`. The route always returns
`projected_reply=null`; it does not enqueue a Matrix message or change outbox
state.

## Persistence, concurrency, and recovery

Apply transport migrations `0001` through `0005` in order. Inbox,
source-observation, history, poison payload, send binding, send receipt,
stream-scope and filter-definition evidence is immutable. Cursor advance requires
current owner, fence, revision and unexpired lease. Poison observations hold a
partition until explicit acknowledgement; acknowledgement is not successful
reprocessing.

Apply operator migrations in this order:

1. `0001_adapter_result_reconciliation.sql`;
2. `0002_runtime_roles.sql`;
3. `0003_adapter_result_evidence_binding.sql`;
4. `0004_adapter_result_runtime_reconciliation.sql`.

Migration 0004 corrects the prior first-success failure by inserting the validated
`result_payload` into its NOT NULL evidence column. Exact replay compares the
payload, hash, evidence and delivery terminal state and writes no second history
row. Changed content under the same delivery identity collides.

Only adapter outcomes explicitly classified as unknown are reconcilable. A
permanent rejection cannot be rewritten as success. The operator command performs
one lookup, computes the exact lookup-response digest and calls only the security-
definer reconciliation function using credentials supplied through the
environment. It never directly reads or updates Matrix tables.

Rollback first stops ingress, polling, relay claims and operator reconciliation.
It preserves all immutable identities and evidence, then deploys a schema-
compatible binary or reviewed forward repair. Migration replay is not rollback.

## Configuration and secrets

The adapter validates profile, ingress token, Consumer Entry endpoint/token,
session-auth issuer/key/secret, approved issuer-registry revision, bot identity,
rate-limit path, recent-event path and bounded limits. Production-like profiles
must fail closed on missing or conflicting authority.

Result-reconciliation runtime uses:

- `MATRIX_ENTRY_ADAPTER_BASE_URL` — adapter base URL; HTTPS outside explicit
  loopback tests;
- `MATRIX_ENTRY_INGRESS_TOKEN` — adapter lookup credential;
- `MATRIX_RECONCILIATION_DATABASE_URL` — dedicated reconciler login URL;
- optional `MATRIX_RECONCILIATION_PSQL` — reviewed psql executable path.

Tokens, database URLs, raw result payloads, replay-store bytes and session secrets
must not appear in process arguments, result JSON, logs, traces, metrics, issues or
repository fixtures. The reconciler command moves database credentials into libpq
environment variables before spawning psql.

## Security and trust boundaries

A valid Matrix sender field is not authenticated merely because it parses. The
ingress credential, signed session assertion, issuer/key selection, tenant
mapping, replay fingerprint, bounded body, redirect policy, exact source identity
and database transition are independent checks.

The adapter lookup disables redirects and bounds response bytes. Duplicate JSON
keys, cross-room or cross-user responses, changed task identity, invalid source
scope and non-read-only envelopes fail closed. HTTP status alone never proves a
business result.

Runtime roles receive no DDL, TRUNCATE, trigger-disable, ownership or blanket
DML. The reconciliation function verifies the held delivery's immutable payload,
source, room, principal, hash and unknown-outcome reason before atomically storing
evidence and closing the delivery. Repository tests do not prove secret custody,
real homeserver authority or production topology.

## Verification

Required catalog commands:

```text
cargo test -p matrix-entry-adapter
cargo clippy -p matrix-entry-adapter --all-targets -- -D warnings
bash scripts/check-matrix-source-observation-postgres.sh
```

Additional Sequence 54 checks:

```text
python3 scripts/check-matrix-result-reconciliation.py
python3 scripts/reconcile-matrix-adapter-result.py --self-test
bash scripts/check-matrix-operator-postgres.sh
cargo fmt -p matrix-entry-adapter -- --check
cargo test --locked -p matrix-entry-adapter --all-targets
cargo clippy --locked -p matrix-entry-adapter --all-targets -- -D warnings
```

The operator PostgreSQL runner applies all four migrations twice and executes
baseline, hostile evidence and runtime payload-persistence regressions. It must
run against disposable PostgreSQL 16 with non-empty hosted steps. Source checks,
self-tests or zero-step jobs cannot substitute for that execution.

## Deployment and operations

Deploy privately with a dedicated runtime identity. Keep schema-owner credentials
outside resident processes. Use a separate login inheriting only
`cex_matrix_reconciler_runtime` for the reconciliation command. Record exact
image, commit/tree, transport and operator migration heads, normalized profile,
credential identifiers and Consumer Entry endpoint.

Monitor ingress failures, replay lookup failures, unknown-outcome age, dead-letter
count, reconciliation success/collision, poison holds, cursor age, send receipt
conflicts and queue age. Every reconciliation must retain delivery ID, event ID,
principal, room, candidate SHA and lookup-response digest without retaining
secrets in the operational record.

Readiness must distinguish listener health from issuer-registry validity,
Consumer Entry reachability, transport schema availability and operator migration
head. Real restart, credential rotation, least-privilege inspection, retention,
SLO and homeserver drills remain promotion requirements.

## Compatibility and change protocol

The validated facade remains the only construction path. Re-exposing private
configuration/state constructors, restoring local model execution, weakening
profile conflict handling or accepting an unbound result is a breaking security
change.

The reconciliation function retains its v1 signature. Migration 0004 is additive
and replaces only function behavior; existing immutable evidence rows are
preserved. Changes to result schema, evidence keys, reconcilable error classes,
role grants, session assertion, cursor/filter scope or reply protocol require
versioned fixtures, migrations, hostile tests, traceability and fresh exact-tree
evidence. No document or source checker grants production authorization.
