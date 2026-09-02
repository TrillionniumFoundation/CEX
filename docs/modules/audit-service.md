# audit-service module contract

Status: active module contract  
Workspace member: `services/audit-service`  
Package: `audit-service`  
Kind: `service`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `audit-integrity`  
Production authorization: `not_granted`

This contract is indexed by `docs/module-catalog-v1.json`. It defines the module boundary for one exact repository tree; it is not release evidence.

## Purpose and non-goals

**Purpose.** Provides tamper-evident, authenticated audit recording and durable source delivery for high-value state changes.

**Non-goals.** It is not an external notarization service, raw application log sink, secret archive, or substitute for independent security/operations review.

## Authority and owned state

Authenticated Audit v2 append, per-scope hash chains, source baseline, durable outbox delivery, and immutable acknowledgement evidence.

Owned state: Audit v2 events, chain heads, sequences/hashes, source baseline checkpoints, durable outbox claims, delivery receipts, retry/dead-letter and operator evidence.

A projection, cache, compatibility row, HTTP success, or transport acknowledgement never transfers authority from its owning component.

## Source layout and entry points

- `v2.rs` and `api.rs`: authenticated append/read contract.
- `state.rs`: persistence state and backend selection.
- `outbox_dispatcher.rs`: claim/deliver/ack lifecycle.
- `bin/audit-outbox-dispatcher.rs`: dedicated worker process.

Catalog-bound entry points:

- `services/audit-service/src/main.rs`
- `services/audit-service/src/api.rs`
- `services/audit-service/src/v2.rs`
- `services/audit-service/src/state.rs`
- `services/audit-service/src/outbox_dispatcher.rs`
- `services/audit-service/src/bin/audit-outbox-dispatcher.rs`
- `services/audit-service/tests/http_flow.rs`
- `services/audit-service/tests/runtime_blackbox.rs`

Any new binary, public source boundary, migration owner, or removed path must update the catalog and this document in the same commit.

## Interfaces and contracts

Canonical `/v2/audit/events`, trace reads, metrics, and a dedicated outbox dispatcher. Compatibility aliases may not weaken auth, tenancy, replay, or persistence.

Requests and events must define authentication, tenant/subject binding, size bounds, immutable identity, idempotency scope, version negotiation, error semantics, and retirement conditions. Authoritative transitions require complete contract/receipt validation, not transport success alone.

## Persistence, concurrency, and recovery

Append-only PostgreSQL rows and transactional source enqueue. Remote append success followed by local ACK loss recovers through stable event-ID replay.

Remote effects, where present, must occur after durable intent or claim commit and before a separate outcome transaction. Possible-side-effect timeouts enter pending or reconciliation state; they never authorize blind retry or a second operation identity.

## Configuration and secrets

Database role, runtime profile, writer/read tokens, dispatcher identity/endpoint, claim batch/lease/retry bounds, and baseline pressure thresholds.

Production-like startup must fail before listening or working when required durable storage, credentials, trust anchors, or explicit modes are absent. Example values are not activation evidence.

## Security and trust boundaries

Writer identity comes from authentication, not request bodies. Payload schemas must redact credentials, provider prompts/results, private keys, unrestricted personal data, and database URLs.

Inputs must be bounded and validated before authority changes. Logs, metrics, traces, and errors exclude sensitive material, unrestricted payloads, and high-cardinality identity fields unless a reviewed contract explicitly permits them.

## Verification

Required commands:

```text
cargo test -p audit-service
cargo clippy -p audit-service --all-targets -- -D warnings
bash scripts/check-audit-source-baseline-postgres.sh
```

Required behavioral focus:

- Authenticated append, replay/collision, tenant scope, sequence/hash integrity.
- Outbox claim ownership, ACK loss, retry/dead-letter, and baseline restart/no-duplicate behavior.
- Production-like durable-backend and credential failures before listener startup.

The exact candidate SHA must also pass the authoritative hosted workflow and appear in the generated immutable candidate manifest.

## Deployment and operations

Apply migrations with an owner role, run runtime and dispatcher under separate least-privilege identities, monitor backlog/oldest age/hash-chain health, and preserve dead-letter evidence.

Operators record artifact identity, runtime profile, dependency identities, readiness, rollback boundary, retained evidence, alerts, and owner escalation. Repository CI does not replace representative-volume recovery, sustained load, credential custody, or independent approval.

## Compatibility and change protocol

Compatibility routes require telemetry and explicit retirement. Corrections append new events; normal roles never update/delete historical events or chain heads.

Changes to authority, public types/routes, persistence, configuration, migrations, retry semantics, or topology require this contract, the module catalog, relevant ADR/protocol/traceability, executable tests, hosted gate wiring, and a new shared candidate trigger.

No module document may declare repository closure or production authorization.
