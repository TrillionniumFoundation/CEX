# consumer-entry-api module contract

Status: active module contract  
Workspace member: `services/consumer-entry-api`  
Package: `consumer-entry-api`  
Kind: `service`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `consumer-edge`  
Production authorization: `not_granted`

This contract is indexed by `docs/module-catalog-v1.json`. It defines the module boundary for one exact repository tree; it is not release evidence.

## Purpose and non-goals

**Purpose.** Translates consumer- and Matrix-facing actions into bounded CEX/Hepta calls and projects backend state into player/task views.

**Non-goals.** The edge may not become the unique writer for research facts, Nakama match state, Ledger balances, provider outcomes, or Chain finality.

## Authority and owned state

Consumer session/identity mapping, bounded task and product projections, and edge-local governance; it is not scientific, Ledger, provider, or finality authority.

Owned state: Edge sessions, identity-binding/registry governance, rate-limit buckets, CSRF/idempotency material, and supporting product read models explicitly assigned to this package.

A projection, cache, compatibility row, HTTP success, or transport acknowledgement never transfers authority from its owning component.

## Source layout and entry points

- `consumer_ingress.rs` and `task_routes.rs`: chat/task admission and projection.
- `identity_admin_routes.rs`: governed binding/registry reload.
- `term_exchange_backend.rs`: economy adapter boundary.
- `world_*` and League files: supporting product projections that require explicit ownership.
- `health_metrics.rs`: bounded readiness and metrics.
- `tests.rs`: broad service regression suite.

Catalog-bound entry points:

- `services/consumer-entry-api/src/main.rs`
- `services/consumer-entry-api/src/lib.rs`
- `services/consumer-entry-api/src/consumer_ingress.rs`
- `services/consumer-entry-api/src/identity_admin_routes.rs`
- `services/consumer-entry-api/src/task_routes.rs`
- `services/consumer-entry-api/src/term_exchange_backend.rs`
- `services/consumer-entry-api/src/world_routes.rs`
- `services/consumer-entry-api/src/health_metrics.rs`
- `services/consumer-entry-api/src/tests.rs`

Any new binary, public source boundary, migration owner, or removed path must update the catalog and this document in the same commit.

## Interfaces and contracts

Chat/Matrix task routes, browser shells, identity administration, health/metrics, and bounded World/League projections. Raw internal admin/Ledger/Audit APIs remain behind the edge.

Requests and events must define authentication, tenant/subject binding, size bounds, immutable identity, idempotency scope, version negotiation, error semantics, and retirement conditions. Authoritative transitions require complete contract/receipt validation, not transport success alone.

## Persistence, concurrency, and recovery

PostgreSQL or explicitly configured files provide durable edge state. Identity/registry reload uses approved revisions, actor gates, reference-integrity checks, and append-only audit.

Remote effects, where present, must occur after durable intent or claim commit and before a separate outcome transaction. Possible-side-effect timeouts enter pending or reconciliation state; they never authorize blind retry or a second operation identity.

## Configuration and secrets

Runtime profile, ingress/session secrets, identity binding/registry paths, approval source, actor allowlist, rate-limit store, downstream URLs, and product-specific rollout switches.

Production-like startup must fail before listening or working when required durable storage, credentials, trust anchors, or explicit modes are absent. Example values are not activation evidence.

## Security and trust boundaries

Outside local development, browser mutations require signed sessions and CSRF. Do not expose authority hashes, secrets, raw private identifiers, or unrestricted provider/research content.

Inputs must be bounded and validated before authority changes. Logs, metrics, traces, and errors exclude sensitive material, unrestricted payloads, and high-cardinality identity fields unless a reviewed contract explicitly permits them.

## Verification

Required commands:

```text
cargo test -p consumer-entry-api
cargo clippy -p consumer-entry-api --all-targets -- -D warnings
```

Required behavioral focus:

- Signed ingress/session/CSRF, idempotency, rate limits, and identity governance.
- Downstream failure translation without inventing success, balance, or finality.
- Boundary tests preventing World/League projections from becoming CEX authority.

The exact candidate SHA must also pass the authoritative hosted workflow and appear in the generated immutable candidate manifest.

## Deployment and operations

Deploy as a supporting edge behind an authenticated transport. Separate operator/admin access from consumer routes and monitor readiness dimensions rather than one green process flag.

Operators record artifact identity, runtime profile, dependency identities, readiness, rollback boundary, retained evidence, alerts, and owner escalation. Repository CI does not replace representative-volume recovery, sustained load, credential custody, or independent approval.

## Compatibility and change protocol

Consumer terms are projections of backend contracts. New product surfaces must declare their owner and must not silently expand the edge into a second authoritative monolith.

Changes to authority, public types/routes, persistence, configuration, migrations, retry semantics, or topology require this contract, the module catalog, relevant ADR/protocol/traceability, executable tests, hosted gate wiring, and a new shared candidate trigger.

No module document may declare repository closure or production authorization.
