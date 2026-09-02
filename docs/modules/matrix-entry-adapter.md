# matrix-entry-adapter module contract

Status: active module contract  
Workspace member: `services/matrix-entry-adapter`  
Package: `matrix-entry-adapter`  
Kind: `adapter-service`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `matrix-integration`  
Production authorization: `not_granted`

This contract is indexed by `docs/module-catalog-v1.json`. It defines the module boundary for one exact repository tree; it is not release evidence.

## Purpose and non-goals

**Purpose.** Adapts Matrix-shaped ingress and callbacks to the bounded consumer-entry contract while preserving stable event identity and transport replay semantics.

**Non-goals.** It does not make authorization, entitlement, billing, research, provider, or finality decisions and must not become a second user/account authority.

## Authority and owned state

Matrix transport validation, event normalization, cursor/idempotency handling, and forwarding to the product entry boundary; it owns no CEX domain truth.

Owned state: Only adapter-local cursor, deduplication, replay, and delivery observations explicitly implemented by this package. Matrix and CEX services retain their own authoritative state.

A projection, cache, compatibility row, HTTP success, or transport acknowledgement never transfers authority from its owning component.

## Source layout and entry points

- `lib.rs`: adapter state, routes, Matrix parsing, delivery, and tests.
- `main.rs`: startup and listener.

Catalog-bound entry points:

- `services/matrix-entry-adapter/src/main.rs`
- `services/matrix-entry-adapter/src/lib.rs`

Any new binary, public source boundary, migration owner, or removed path must update the catalog and this document in the same commit.

## Interfaces and contracts

Axum routes and Matrix/downstream HTTP clients in `lib.rs`, with the process boundary in `main.rs`. Every forwarded request must carry stable event/idempotency identity.

Requests and events must define authentication, tenant/subject binding, size bounds, immutable identity, idempotency scope, version negotiation, error semantics, and retirement conditions. Authoritative transitions require complete contract/receipt validation, not transport success alone.

## Persistence, concurrency, and recovery

If cursor/dedup state is not durable, the service remains supporting Alpha and must rely on downstream idempotency. A production promotion requires explicit durable cursor/replay storage and recovery tests.

Remote effects, where present, must occur after durable intent or claim commit and before a separate outcome transaction. Possible-side-effect timeouts enter pending or reconciliation state; they never authorize blind retry or a second operation identity.

## Configuration and secrets

Matrix homeserver/ingress settings, downstream consumer-entry URL, shared secrets/tokens, timeout/body limits, and runtime profile.

Production-like startup must fail before listening or working when required durable storage, credentials, trust anchors, or explicit modes are absent. Example values are not activation evidence.

## Security and trust boundaries

Verify transport authentication/signatures where available, bound bodies, reject untrusted claimed user/room authority, disable redirect surprises, and avoid logging event bodies or access tokens.

Inputs must be bounded and validated before authority changes. Logs, metrics, traces, and errors exclude sensitive material, unrestricted payloads, and high-cardinality identity fields unless a reviewed contract explicitly permits them.

## Verification

Required commands:

```text
cargo test -p matrix-entry-adapter
cargo clippy -p matrix-entry-adapter --all-targets -- -D warnings
```

Required behavioral focus:

- Authentication, body bounds, event normalization, duplicate event replay, cursor restart, downstream timeout, and no invented success.
- Refactor pressure: the large `lib.rs` should be split without changing the public contract; new domains require separate modules and ownership.

The exact candidate SHA must also pass the authoritative hosted workflow and appear in the generated immutable candidate manifest.

## Deployment and operations

Run as a separate adapter with least-privilege Matrix and downstream credentials. Monitor cursor lag, duplicate hits, delivery failures, and authentication rejects.

Operators record artifact identity, runtime profile, dependency identities, readiness, rollback boundary, retained evidence, alerts, and owner escalation. Repository CI does not replace representative-volume recovery, sustained load, credential custody, or independent approval.

## Compatibility and change protocol

Matrix event shape is translated into a versioned internal request. Unsupported event versions or ambiguous edits/redactions fail closed rather than creating a new domain action.

Changes to authority, public types/routes, persistence, configuration, migrations, retry semantics, or topology require this contract, the module catalog, relevant ADR/protocol/traceability, executable tests, hosted gate wiring, and a new shared candidate trigger.

No module document may declare repository closure or production authorization.
