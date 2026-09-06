# CEX → Trillionnium World authority cutover v1

Status: cross-repository source candidate  
production_authorization: `not_granted`

## Decision

`TrillionniumFoundation/Trillionnium-World` owns World topology, movement, tactics, commerce, company/shop/work-order, progression and their source-versioned projections. CEX owns authenticated ingress, identity/session binding, request normalization and exact economic settlement integration. CEX must not remain a second World state writer.

World PR #60 provides a bounded seven-crate server authority workspace and mandatory PostgreSQL cutover hardening at exact commit `ff2155a6ada81cae0a4c83dda73083469ec9a828`. CEX PR #34 adds a remote-only adapter plus a production write fence. Neither change self-grants production authority.

## CEX runtime modes

### Development compatibility

Local embedded World handlers may remain available for migration inspection and deterministic parity tests. They are not production authority and must not receive production evidence credit.

### Production-like profiles

`consumer-entry-api` refuses startup unless all of the following are true:

```text
CEX_WORLD_AUTHORITY_MODE=remote
TRILLIONNIUM_WORLD_BASE_URL=<non-loopback non-placeholder HTTP(S) URL>
TRILLIONNIUM_WORLD_API_CONTRACT=trillionnium_world_api_v1
```

Even after valid startup, the legacy `/world/**` mutation paths are fenced. They return `503 remote_world_authority_required` rather than mutating `LeagueState.world`. `/world/**` reads and `/map-rum` telemetry remain available while consumers move to source-versioned projections.

The dedicated `world-authority-adapter` binary owns remote forwarding. It:

- accepts only `/v1/world/**` plus local `/health`;
- maps requests to the World-owned `/world/**` contract;
- forwards an allowlist of identity, trace and idempotency headers;
- injects the exact World API and cutover contracts;
- uses a separate service bearer token;
- disables redirects;
- caps request bodies at 2 MiB;
- enforces bounded connect/response timeouts;
- returns `503` or `502` on upstream failure or contract mismatch;
- never falls back to CEX-local World state.

Production-like adapter profiles additionally require a strong non-placeholder service token and reject loopback World URLs.

## Durable PostgreSQL authority boundary

The base schema is not a supported standalone migration. The only supported installer applies both:

```text
deploy/postgres/trnm-world-authority-cutover-v1.sql
deploy/postgres/trnm-world-authority-cutover-v1-hardening.sql
```

and then verifies both exact migration markers and the semantic definition of the single-active-writer index. The mandatory hardening adds:

- a database unique constraint allowing at most one active, write-enabled epoch;
- global transaction-level activation serialization;
- canonical JSON parity with PostgreSQL `jsonb::text`;
- request, response and state hash constraints;
- a deterministic event ID derived from epoch, world, idempotency key and request hash;
- a per-idempotency-key lock and post-lock receipt re-read;
- exact replay versus conflicting replay separation;
- fail-closed migration-version conflict handling;
- terminal rollback that disables the writer while retaining append-only evidence.

The PostgreSQL 16 state-machine suite exercises concurrent activation of different epochs, concurrent exact replay, noncanonical snapshot and command JSON, migration marker conflicts, mutation after rollback, database outage, and cold `pg_dump`/`pg_restore`.

A separate installation-protocol suite additionally proves:

- applying the complete supported bundle twice is idempotent;
- a base-only partial schema is explicitly unqualified and is recovered by the supported installer;
- a pre-existing same-name index with a wrong expression or predicate is rejected rather than being mistaken for the single-writer control.

These are source-level database qualifications, not evidence that any production dataset has been migrated.

## Cross-repository source gates

`.github/workflows/p0-world-authority-cutover-gate.yml` checks out World commit `ff2155a6ada81cae0a4c83dda73083469ec9a828` by exact SHA. `.github/workflows/p0-world-authority-postgres-v2-gate.yml` independently runs the durable state-machine and installation-protocol hostile suites against the same exact World candidate. Checkout, Rust toolchain and artifact upload actions are pinned to immutable commits, and Rust is pinned to `1.98.1`.

The World protected source definitions also include the `trnm-game-ci`, `trnm-world-p0-boundaries`, `trnm-world-status-evidence`, and `trnm-world-postgres-cutover` contexts. GitHub has not created native World workflow runs for the current exact head, so checked-in workflow definitions are not treated as successful status checks.

The CEX gates verify:

1. the World seven-crate dependency closure;
2. all seven restored tree objects and 15 restored file blobs against historical commit `d44d8930c917b55da7b23eb19e9645feb8f4ee59`;
3. omission, substitution, extra-file and omitted-tree hostile provenance fixtures;
4. World format, tests, strict Clippy, committed lockfile immutability and structured restart/reload evidence;
5. CEX static authority boundaries and negative startup guards;
6. CEX adapter and consumer-entry binary tests plus strict Clippy;
7. real local HTTP forwarding from CEX to the World server;
8. a World command mutation through the adapter;
9. persistence after World process restart;
10. fail-closed `503` behavior while the World process is unavailable;
11. file-snapshot rollback in the development evidence harness;
12. stable repeated read projections;
13. the PostgreSQL hostile state-machine, installation, cold backup/restore, and outage matrices;
14. exact CEX and World commit/tree identities plus SHA-256 artifact manifests.

These qualifications do not substitute for native World protected-context execution, live backfill reconciliation, deployment-specific secrets/IAM, or a production rollback drill.

## Required migration protocol

1. Freeze the CEX writer at an explicit source watermark.
2. Export every authoritative World record with stable IDs and canonical content hashes.
3. Import into a fenced World-owned durable repository under a migration epoch.
4. Reconcile record counts, IDs and hashes exactly.
5. Run shadow reads through the CEX adapter without enabling the new writer.
6. Disable the CEX writer before enabling the World writer.
7. Exercise successful command, timeout, duplicate/replay, partial outage and rollback cases.
8. Remove or permanently quarantine embedded CEX World sources.
9. Record proof that no actor/entity has two authoritative writers at any instant.

Any mismatch keeps `production_authorization=not_granted`.

## Evidence truth

The machine-readable current matrix is `docs/traceability/world-authority-cutover-v1.json`. CI evidence binds the CEX side to the pull-request head SHA or push SHA and the World side to the exact SHA above. Source-level database and HTTP tests are now present; runtime data reconciliation, native World contexts, deployment-specific no-dual-writer evidence, embedded-source retirement and explicit go-live authorization remain blocking until actually executed.
