# matrix-entry-adapter module contract

Status: active module contract  
Workspace member: `services/matrix-entry-adapter`  
Package: `matrix-entry-adapter`  
Kind: `adapter-service`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `matrix-integration`  
Production authorization: `not_granted`

This contract describes the current repository source boundary and required
verification. It is not hosted qualification, production approval or evidence
that queued jobs executed. The catalog authority is
`docs/module-catalog-v1.json`.

## Purpose and non-goals

The adapter authenticates Matrix ingress, validates bounded event envelopes,
normalizes supported text commands and forwards requests to Consumer Entry. It
returns protocol-shaped Matrix responses and maintains transport observations.
It does not authorize research, account value, World or Game state, Chain
finality, room membership or human identity. Matrix sender and room fields are
untrusted source claims until the configured ingress and downstream identity
boundaries validate them. The adapter does not host Agents or perform model
inference.

## Authority and owned state

The process owns HTTP normalization, ingress rate limiting, recent-event
suppression, session-auth issuer selection and local reply construction. Its
legacy local file/cache state is not authoritative multi-instance exactly-once
storage. Consumer Entry owns business idempotency and principal authorization.
The shared Matrix PostgreSQL schema is located under this package, while the
poller owns cursor leases/admission and the relay owns claimed outbox dispatch.
Schema ownership does not transfer those runtime responsibilities or grant the
adapter permission to repair transport history.

## Source layout and entry points

`services/matrix-entry-adapter/src/main.rs` performs synchronous profile
validation, consumes the resulting token, constructs Tokio, builds validated
state and starts the listener. `services/matrix-entry-adapter/src/lib.rs` is the
only public facade. `services/matrix-entry-adapter/src/implementation.rs` is the
private byte-preserved route/state implementation and original unit-test body.
`services/matrix-entry-adapter/src/runtime_profile.rs` remains an exact
compatibility re-export of the shared parser; it contains no policy.

The persistence chain is
`services/matrix-entry-adapter/migrations/0001_transport_durability.sql`,
`services/matrix-entry-adapter/migrations/0002_source_observation_replay.sql`,
`services/matrix-entry-adapter/migrations/0003_sync_recovery_and_send_receipts.sql`,
`services/matrix-entry-adapter/migrations/0004_stream_scope_binding.sql`, then
`services/matrix-entry-adapter/migrations/0005_filter_definition_pins.sql`.
New targets, routes or durable objects require same-change catalog, contract,
migration and regression updates.

## Interfaces and contracts

The public Rust construction API is deliberately narrow.
`validate_process_environment` reads every supported profile source and returns
`ValidatedMatrixAdapterEnvironment` only after strict shared parsing succeeds.
The token has a private field and is consumed by
`AppState::from_validated_env`. The facade exposes neither raw
`MatrixAdapterConfig` nor legacy `new`/`from_env` state constructors, and it
keeps `implementation.rs` private. `build_router` consumes the validated facade
state. See `docs/matrix-adapter-validated-construction-v1.md`.

At HTTP ingress, `/v1/matrix/events` accepts only the documented bounded Matrix
event shape and configured transport credentials. Downstream calls carry stable
source-event and scoped idempotency identities. HTTP success is not proof that a
research, account, settlement or finality transition occurred. The replay SQL
function `cex_matrix_accept_source_event_v1(text,text,text,text) -> text`
returns `accepted` for first admission and `replay` only for identical event,
content hash and partition; conflicting content or partition fails closed.

## Persistence, concurrency, and recovery

Apply migrations 0001 through 0005 in order under the schema owner. Inbox,
source-observation, delivery-history, poison-payload, send-binding, send-receipt,
stream-scope and filter-definition evidence is immutable under the defined
triggers. Cursor advance requires the current lease owner, fence and exact next
revision. Poison observations hold the partition until exact operator
quarantine acknowledgement; acknowledgement is not successful reprocessing.
Expired ambiguous adapter claims are dead-lettered rather than blindly resent.
Matrix sends bind delivery, payload, room, homeserver and credential identities
before network I/O and require a matching event receipt before `sent`.

Runtime roles must not receive DDL, TRUNCATE, trigger-disable, owner or blanket
table privileges. Migration replay is not rollback. Rollback stops new
admission and claims, preserves all durable identities, and deploys a
schema-compatible binary or reviewed forward repair. Durable adapter
result-lookup and principal-bound response-loss reconciliation remain separately
required; current unknown effects hold safely instead of being declared complete.

## Configuration and secrets

Before tracing, Tokio or worker creation, the executable validates
`MATRIX_ENTRY_RUNTIME_PROFILE`, `CEX_RUNTIME_PROFILE` and `APP_ENV`. Accepted
local aliases are `test`, `local`, `local_dev`, `dev`, `development`; beta is
`beta`; staging is `stage` or `staging`; production is `prod`, `production`,
`trnm-economy` or `trnm_economy`. Whitespace and case normalize. Explicit empty,
unknown, non-Unicode or conflicting values fail with exit 78. All absent values
select explicit local development. Staging and production remain distinct before
both map to the legacy production policy.

The facade owns this strict read through `validate_process_environment`; the
binary passes the non-inventible `ValidatedMatrixAdapterEnvironment` into
`from_validated_env`. Embedded callers can no longer name the legacy public
configuration/state constructors through this package. Existing ingress tokens,
session-auth secrets, issuer/key identifiers, approved registry revisions,
downstream endpoints, cache paths and limits still require their profile-specific
validation. Secrets, database URLs and opaque cursors must not be emitted in
errors or evidence.

## Security and trust boundaries

A valid runtime profile does not authenticate a Matrix sender or authorize a
business effect. The ingress credential, session issuer/key selection, tenant
mapping, bounded request body, redirect policy, reply identity and downstream
receipt all remain independent checks. Unknown configuration cannot silently
select local authority in either the binary or public facade. The private
implementation module is not a security sandbox; its safety derives from the
facade visibility boundary, source checker and exact compiled package.

Operators must protect Matrix access tokens and issuer secrets, separate schema
ownership from runtime execution, and avoid raw message bodies or high-cardinality
principal identifiers in logs and metrics. Redaction/edit/membership semantics,
poison retention/erasure and credential rotation require explicit operational
policy and real homeserver/database evidence before promotion.

## Verification

Required catalog commands are:

```text
cargo test -p matrix-entry-adapter
cargo clippy -p matrix-entry-adapter --all-targets -- -D warnings
bash scripts/check-matrix-source-observation-postgres.sh
```

The stricter exact-head lanes also run:

```text
python3 scripts/test-matrix-adapter-api-boundary.py
python3 scripts/check-matrix-adapter-api-boundary.py
cargo fmt -p matrix-entry-adapter -- --check
cargo test --locked -p matrix-entry-adapter --all-targets
cargo clippy --locked -p matrix-entry-adapter --all-targets -- -D warnings
```

The SQL wrapper requires a disposable PostgreSQL 16 database, explicit
`MATRIX_TEST_ALLOW_SCHEMA_RESET=1`, one guarded client session and the complete
0001–0005 chain twice before all preserved and additive assertions. Source and
Python mutation checks do not prove Rust compilation, PostgreSQL behavior or
black-box startup. Missing tools, nonzero child status, absent steps or queued
jobs are failures or unexecuted states, never skips or passes.

## Deployment and operations

Deploy the adapter privately with a dedicated least-privilege identity and a
validated nonlocal profile where applicable. Readiness must distinguish listener
health from downstream authentication, issuer-registry validity, database
transport health, queue age and successful receipt reconciliation. Record exact
image, commit/tree, schema chain, normalized profile, credential identifiers,
cache mode and Consumer Entry endpoint. Monitor ingress failures, duplicate
suppression, delivery age, poison holds, dead letters and response-unknown volume.

Rollback first stops ingress/admission and new outbox claims, then preserves
inbox, observations, cursor history, send bindings/receipts and poison evidence.
Never restore an image that requires weaker constructors or an earlier migration
semantic against the current schema. Real restart, credential rotation,
least-privilege role, retention, SLO and homeserver drills remain required before
production authorization can change from `not_granted`.

## Compatibility and change protocol

The facade split preserves the existing route/state implementation blob and its
unit tests but intentionally removes direct external access to legacy config and
state constructors. Embedded callers must migrate to
`validate_process_environment` plus `ValidatedMatrixAdapterEnvironment` and
`from_validated_env`. Re-exposing `implementation.rs`, adding a caller-inventible
token, accepting parser precedence, or restoring `AppState::from_env` at the
facade is a security regression.

Protocol, partition, normalization, cursor, filter, credential-scope or reply
changes require versioned fixtures, migration compatibility, replay/rollback
rules and exact-head evidence. Update this module contract, focused design
records, source gates and catalog together. No document, source checker or local
commit can grant repository qualification or production approval.
