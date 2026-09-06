# CEX → Trillionnium World authority cutover v1

Status: cross-repository source candidate  
Production authorization: `not_granted`

## Decision

`TrillionniumFoundation/Trillionnium-World` owns World topology, movement, tactics, commerce, company/shop/work-order, progression and their source-versioned projections. CEX owns authenticated ingress, identity/session binding, request normalization and exact economic settlement integration. CEX must not remain a second World state writer.

World PR #60 restores a bounded seven-crate server authority workspace at exact commit `554761417edbb37a2f20deed23917a9b05abdfe2`. CEX PR #34 adds a remote-only adapter plus a production write fence. Neither change self-grants production authority.

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

## Cross-repository source gate

`.github/workflows/p0-world-authority-cutover-gate.yml` checks out World commit `554761417edbb37a2f20deed23917a9b05abdfe2` by exact SHA instead of a mutable branch. The gate verifies:

1. the World seven-crate dependency closure and historical provenance;
2. World format, tests, Clippy and restart/reload smoke;
3. CEX static authority boundaries and negative startup guards;
4. CEX adapter and consumer-entry binary tests plus Clippy;
5. real local HTTP forwarding from CEX to the World server;
6. a World command mutation through the adapter;
7. persistence after World process restart;
8. fail-closed `503` behavior while the World process is unavailable;
9. file-snapshot rollback in the development evidence harness;
10. stable repeated read projections.

These are source and development-runtime qualifications. They do not substitute for a durable production repository, live migration reconciliation or a production rollback drill.

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

The machine-readable current matrix is `docs/traceability/world-authority-cutover-v1.json`. CI evidence binds the CEX side to `GITHUB_SHA`; the World source is pinned to the exact SHA above. Runtime migration data, durable adapter evidence, mutation-idempotency evidence and the production cutover fence remain blocking until actually executed.
