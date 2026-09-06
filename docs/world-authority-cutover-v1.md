# CEX → Trillionnium World authority cutover v1

Status: cross-repository source candidate  
production_authorization: `not_granted`

## Decision

`TrillionniumFoundation/Trillionnium-World` owns World topology, movement, tactics, commerce, company/shop/work-order, progression and their source-versioned projections. CEX owns authenticated ingress, identity/session binding, request normalization and exact economic settlement integration. CEX must not remain a second World state writer.

World PR #60 restores a bounded seven-crate server authority workspace at exact commit `8b6304e5e78ac497c8ab4f9cafe10f6be6f09df9`. CEX PR #34 adds a remote-only adapter plus a production write fence. Neither change self-grants production authority.

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

`.github/workflows/p0-world-authority-cutover-gate.yml` checks out World commit `8b6304e5e78ac497c8ab4f9cafe10f6be6f09df9` by exact SHA instead of a mutable branch. Checkout, Rust toolchain and artifact upload actions are pinned to immutable commit SHAs, and Rust is pinned to `1.98.1`.

The World protected `trnm-game-ci` candidate is also immutable at this source head: checkout, Rust setup, supply-chain tool installer and artifact upload are commit-pinned; Rust is fixed at `1.98.1`; `cargo-audit` is fixed at `0.22.2`; `cargo-deny` is fixed at `0.20.2`. It defines explicit source-head and prospective-merge qualification.

Its evidence path is fail-closed. Every exact-source, package, prospective-merge and supply-chain gate has an explicit conclusion. A qualified receipt and qualified artifact name are produced only when all applicable required conclusions equal `success`; failed, cancelled or incomplete runs produce distinct unqualified states and artifact names. The artifact manifest binds the status, required gates, gate conclusions, exact source identity and file digests.

GitHub has not created native World workflow runs for this exact head, so these source definitions are not treated as successful status checks.

The CEX exact-SHA gate verifies:

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
13. exact CEX and World commit/tree identities plus a SHA-256 artifact manifest.

These are source and development-runtime qualifications. They do not substitute for native World protected-context execution, a durable production repository, live migration reconciliation or a production rollback drill.

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

The machine-readable current matrix is `docs/traceability/world-authority-cutover-v1.json`. CI evidence binds the CEX side to the pull-request head SHA or push SHA and binds the World source to the exact SHA above. Runtime migration data, durable adapter evidence, mutation-idempotency evidence, native World protected contexts and the production cutover fence remain blocking until actually executed.
