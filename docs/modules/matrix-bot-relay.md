# matrix-bot-relay module contract

Status: active module contract  
Workspace member: `apps/matrix-bot-relay`  
Package: `matrix-bot-relay`  
Kind: `application`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `matrix-integration`  
Production authorization: `not_granted`

This contract is indexed by `docs/module-catalog-v1.json`. It defines the module boundary for one exact repository tree; it is not release evidence.

## Purpose and non-goals

**Purpose.** Receives or relays Matrix bot events to the product entry layer and sends bounded status responses back to Matrix.

**Non-goals.** It does not authorize users, parse raw CEX admin operations, own task state, debit value, or act as an authoritative Matrix homeserver.

## Authority and owned state

Webhook/relay transport only; downstream services decide identity, task, research, value, and finality authority.

Owned state: No CEX domain state. Any process-local delivery cache or observation is expendable and must not be used as the sole source of replay truth.

A projection, cache, compatibility row, HTTP success, or transport acknowledgement never transfers authority from its owning component.

## Source layout and entry points

- `src/main.rs`: complete relay executable and current test surface.

Catalog-bound entry points:

- `apps/matrix-bot-relay/src/main.rs`

Any new binary, public source boundary, migration owner, or removed path must update the catalog and this document in the same commit.

## Interfaces and contracts

The executable in `src/main.rs` exposes relay HTTP behavior and Matrix/downstream clients. Stable Matrix event IDs and downstream idempotency keys must be preserved.

Requests and events must define authentication, tenant/subject binding, size bounds, immutable identity, idempotency scope, version negotiation, error semantics, and retirement conditions. Authoritative transitions require complete contract/receipt validation, not transport success alone.

## Persistence, concurrency, and recovery

Downstream idempotency is mandatory. Production promotion requires durable delivery/outbox semantics if a response can be lost after a remote side effect.

Remote effects, where present, must occur after durable intent or claim commit and before a separate outcome transaction. Possible-side-effect timeouts enter pending or reconciliation state; they never authorize blind retry or a second operation identity.

## Configuration and secrets

Bind address, Matrix API/base URL, bot credential, downstream adapter/consumer-entry URL, internal token, timeouts, and body limits.

Production-like startup must fail before listening or working when required durable storage, credentials, trust anchors, or explicit modes are absent. Example values are not activation evidence.

## Security and trust boundaries

Authenticate inbound callbacks and downstream calls, scope the bot token, bound content, reject redirects where unsafe, and never log access tokens or unrestricted message bodies.

Inputs must be bounded and validated before authority changes. Logs, metrics, traces, and errors exclude sensitive material, unrestricted payloads, and high-cardinality identity fields unless a reviewed contract explicitly permits them.

## Verification

Required commands:

```text
cargo test -p matrix-bot-relay
cargo clippy -p matrix-bot-relay --all-targets -- -D warnings
```

Required behavioral focus:

- Inbound authentication, duplicate event replay, downstream failure, Matrix send failure, and bounded retry.
- No CEX authority is granted from caller-supplied Matrix user/room fields.

The exact candidate SHA must also pass the authoritative hosted workflow and appear in the generated immutable candidate manifest.

## Deployment and operations

Deploy separately from the authoritative CEX services. Readiness should distinguish Matrix reachability from downstream readiness; repeated failures need bounded retry/dead-letter handling.

Operators record artifact identity, runtime profile, dependency identities, readiness, rollback boundary, retained evidence, alerts, and owner escalation. Repository CI does not replace representative-volume recovery, sustained load, credential custody, or independent approval.

## Compatibility and change protocol

Message commands are a presentation protocol. Version or gate them before changing parsing semantics, and never map an unknown command to a privileged default action.

Changes to authority, public types/routes, persistence, configuration, migrations, retry semantics, or topology require this contract, the module catalog, relevant ADR/protocol/traceability, executable tests, hosted gate wiring, and a new shared candidate trigger.

No module document may declare repository closure or production authorization.
