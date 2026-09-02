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

**Purpose.** Adapts authenticated Matrix-shaped ingress and callbacks to the bounded consumer-entry contract while preserving stable event identity, redaction semantics and transport replay behavior.

**Non-goals.** It does not make authorization, entitlement, billing, research, provider, World/Game or finality decisions and must not become a second user/account authority, Matrix homeserver or durable CEX domain store.

## Authority and owned state

The adapter is authoritative only for Matrix transport validation, event normalization, adapter-local cursor/deduplication identity and delivery observations explicitly implemented by this package. Matrix and downstream CEX services retain their own truth.

Owned state, when durability is enabled, is limited to opaque Matrix cursor positions, source event IDs, normalized request fingerprints, claim/fencing metadata, delivery acknowledgements and dead-letter evidence. Caller-supplied Matrix user/room fields never grant CEX authority.

A projection, cache, compatibility row, HTTP success, or transport acknowledgement never transfers authority from its owning component.

## Source layout and entry points

- `lib.rs`: adapter state, routes, Matrix parsing, cursor/deduplication, downstream delivery and tests.
- `main.rs`: startup, configuration validation and listener boundary.

Catalog-bound entry points:

- `services/matrix-entry-adapter/src/main.rs`
- `services/matrix-entry-adapter/src/lib.rs`

Any new binary, public source boundary, migration owner, or removed path must update the catalog and this document in the same commit. The large `lib.rs` remains a refactor target: parsing, cursor store, delivery client, auth and metrics should move behind internal modules without changing authority.

## Interfaces and contracts

Axum routes and Matrix/downstream HTTP clients reside in `lib.rs`, with the process boundary in `main.rs`. Every forwarded request carries stable source event identity, normalized payload fingerprint and scoped idempotency identity.

Requests define inbound authentication/signature policy, Matrix homeserver identity, tenant/subject mapping, event/body/media limits, supported event versions, edit/redaction behavior, downstream timeout, retry classification and retirement conditions. Unsupported or ambiguous events fail closed rather than becoming privileged actions.

## Persistence, concurrency, and recovery

A non-durable cursor/deduplication implementation is supporting Alpha only and relies on downstream idempotency. Production promotion requires an explicit durable cursor/replay repository, transactional delivery intent, expiring claims, fencing tokens, poison-event isolation and restart/multi-instance tests.

Cursor advancement occurs only after durable downstream acceptance. A timeout after possible downstream acceptance enters unknown/reconciliation state; it never advances the cursor blindly, invents success or creates a second operation identity. Multiple replicas may share a cursor partition only with lease ownership and stale-writer fencing.

## Configuration and secrets

Configuration includes Matrix homeserver and ingress identities, inbound verification material, downstream consumer-entry URL/principal, cursor partition, storage mode, worker identity, body/media bounds, batch/lease/poll/timeout limits and runtime profile.

Production-like startup fails before listening when authentication, durable cursor storage, required schema, downstream trust, explicit modes or non-placeholder credentials are absent. Access tokens, sync tokens and signing material come from approved secret custody and never from committed examples.

## Security and trust boundaries

Verify inbound transport identity and signatures where supported; bind claimed sender/room/device fields to the verified source; bound decompressed bodies/media; reject unsafe redirects and cross-tenant mappings; redact access/sync tokens and message bodies from logs.

Edits and redactions preserve the original source event relationship and cannot silently create a new privileged action. Metrics use bounded route/outcome labels, not raw event, room, sender or tenant identifiers. Downstream responses are untrusted until the owning contract and receipt validate.

## Verification

Required commands:

```text
cargo test -p matrix-entry-adapter
cargo clippy -p matrix-entry-adapter --all-targets -- -D warnings
```

Required behavioral focus:

- Authentication, body bounds, event normalization, duplicate replay and cursor restart.
- Downstream timeout/response loss, poison-event dead letter, redaction/edit handling and no invented success.
- Multiple-instance claim/fencing behavior when durable cursor mode is introduced.
- Boundary tests proving Matrix transport fields cannot grant identity, research, value, World/Game or finality authority.

The exact candidate SHA must also pass the authoritative hosted workflow and appear in the generated immutable candidate manifest.

## Deployment and operations

Run as a separate least-privilege adapter. Readiness must be false unless configuration is valid, the cursor store is in the declared mode and required downstream trust is available; liveness reports only process health. Monitor cursor lag, oldest unacknowledged event, duplicate hits, claim expiry, dead-letter age, delivery failures and authentication rejects.

Rollback stops claims before switching binaries, preserves cursor/deduplication/dead-letter evidence and restarts only with a schema/protocol-compatible revision. Operators record artifact identity, runtime profile, Matrix and downstream identities, readiness dimensions, rollback boundary and owner escalation. Repository CI does not replace real homeserver, recovery, load or credential-custody evidence.

## Compatibility and change protocol

Matrix event shape is translated into a versioned internal request. Command-filter, edit/redaction, identity mapping or cursor changes require compatibility fixtures and exact replay tests. A durable-store introduction uses expand/backfill/verify/cutover/contract steps without converting process-local observations into historical authority.

Changes to authority, public routes/types, persistence, configuration, retry semantics or topology require this contract, the module catalog, Matrix architecture/threat model, executable tests, hosted gate wiring and a new shared candidate trigger. No module document may declare repository closure or production authorization.
