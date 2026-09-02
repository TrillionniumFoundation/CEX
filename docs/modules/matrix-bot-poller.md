# matrix-bot-poller module contract

Status: active module contract  
Workspace member: `apps/matrix-bot-poller`  
Package: `matrix-bot-poller`  
Kind: `application`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `matrix-integration`  
Production authorization: `not_granted`

This contract is indexed by `docs/module-catalog-v1.json`. It defines the module boundary for one exact repository tree; it is not release evidence.

## Purpose and non-goals

**Purpose.** Polls Matrix events, preserves event identity, and forwards supported messages through the adapter/consumer-entry path.

**Non-goals.** It is not an event store, user authority, scheduler for CEX jobs, or substitute for Matrix sync semantics.

## Authority and owned state

Polling/cursor transport only; it owns no consumer, research, financial, or finality state.

Owned state: Only the polling cursor and delivery observations if explicitly persisted. Without durable cursor storage it remains supporting Alpha and relies on downstream idempotency.

A projection, cache, compatibility row, HTTP success, or transport acknowledgement never transfers authority from its owning component.

## Source layout and entry points

- `src/main.rs`: polling loop, Matrix/downstream clients, and current test surface.

Catalog-bound entry points:

- `apps/matrix-bot-poller/src/main.rs`

Any new binary, public source boundary, migration owner, or removed path must update the catalog and this document in the same commit.

## Interfaces and contracts

The executable in `src/main.rs` uses Matrix and downstream HTTP APIs. It must preserve the opaque Matrix sync token and event IDs.

Requests and events must define authentication, tenant/subject binding, size bounds, immutable identity, idempotency scope, version negotiation, error semantics, and retirement conditions. Authoritative transitions require complete contract/receipt validation, not transport success alone.

## Persistence, concurrency, and recovery

Cursor advancement must occur only after durable downstream acceptance. On restart, replay is safe through stable event/idempotency identity; skipped ranges are forbidden.

Remote effects, where present, must occur after durable intent or claim commit and before a separate outcome transaction. Possible-side-effect timeouts enter pending or reconciliation state; they never authorize blind retry or a second operation identity.

## Configuration and secrets

Matrix homeserver/base URL, access token, user/device or room scope, poll interval/timeout, downstream URL/token, body limits, and runtime profile.

Production-like startup must fail before listening or working when required durable storage, credentials, trust anchors, or explicit modes are absent. Example values are not activation evidence.

## Security and trust boundaries

Scope the Matrix token, protect sync tokens, reject untrusted event authority, bound event/media bodies, and redact tokens/message content from logs.

Inputs must be bounded and validated before authority changes. Logs, metrics, traces, and errors exclude sensitive material, unrestricted payloads, and high-cardinality identity fields unless a reviewed contract explicitly permits them.

## Verification

Required commands:

```text
cargo test -p matrix-bot-poller
cargo clippy -p matrix-bot-poller --all-targets -- -D warnings
```

Required behavioral focus:

- Cursor persistence/advance ordering, duplicate replay, restart, Matrix rate limit, downstream timeout, and poison-event handling.
- Multiple instances must not advance one cursor concurrently without lease/fencing evidence.

The exact candidate SHA must also pass the authoritative hosted workflow and appear in the generated immutable candidate manifest.

## Deployment and operations

Run as a singleton per cursor partition unless the cursor store supports fencing. Monitor lag, repeated event failures, rate-limit responses, and downstream availability.

Operators record artifact identity, runtime profile, dependency identities, readiness, rollback boundary, retained evidence, alerts, and owner escalation. Repository CI does not replace representative-volume recovery, sustained load, credential custody, or independent approval.

## Compatibility and change protocol

Changes to command filtering or event translation require explicit versioning and replay tests; redactions/edits must not silently create new privileged actions.

Changes to authority, public types/routes, persistence, configuration, migrations, retry semantics, or topology require this contract, the module catalog, relevant ADR/protocol/traceability, executable tests, hosted gate wiring, and a new shared candidate trigger.

No module document may declare repository closure or production authorization.
