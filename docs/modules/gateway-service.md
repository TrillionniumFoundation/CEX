# gateway-service module contract

Status: active module contract  
Workspace member: `services/gateway-service`  
Package: `gateway-service`  
Kind: `service`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `gateway-runtime`  
Production authorization: `not_granted`

This contract is indexed by `docs/module-catalog-v1.json`. It defines the module boundary for one exact repository tree; it is not release evidence.

## Purpose and non-goals

**Purpose.** Provides the internal invocation ingress and the exact-money reserve adapter that freezes one immutable reserve command before Ledger network I/O.

**Non-goals.** It must not contain provider execution, Ledger balance mutation, long-running SQL transactions across HTTP, or public consumer/admin semantics.

## Authority and owned state

Invocation ingress normalization and durable exact-reserve command preparation; it does not own final Ledger outcomes.

Owned state: Invocation skeletons/contracts and Gateway reserve commands/transition evidence. Ledger owns account/effect truth; Execution owns terminal work state.

A projection, cache, compatibility row, HTTP success, or transport acknowledgement never transfers authority from its owning component.

## Source layout and entry points

- `application/`: invocation orchestration.
- `domain/`: invocation model and invariants.
- `interfaces/`: HTTP/saga boundary.
- `infrastructure/`: downstream clients and persistent state.
- `bin/gateway-exact-reserve-*`: durable reserve admission and worker.

Catalog-bound entry points:

- `services/gateway-service/src/main.rs`
- `services/gateway-service/src/application/invocation_service.rs`
- `services/gateway-service/src/domain/invocation.rs`
- `services/gateway-service/src/interfaces/http.rs`
- `services/gateway-service/src/interfaces/saga.rs`
- `services/gateway-service/src/infrastructure/clients.rs`
- `services/gateway-service/src/infrastructure/ledger_v2_client.rs`
- `services/gateway-service/src/bin/gateway-exact-reserve-api.rs`
- `services/gateway-service/src/bin/gateway-exact-reserve-worker.rs`
- `services/gateway-service/tests/http_flow.rs`
- `services/gateway-service/tests/runtime_blackbox.rs`

Any new binary, public source boundary, migration owner, or removed path must update the catalog and this document in the same commit.

## Interfaces and contracts

Application/domain/interface layers expose invocation and saga routes. Separate exact-reserve API/worker binaries enforce registration, claim, remote call, and outcome transactions.

Requests and events must define authentication, tenant/subject binding, size bounds, immutable identity, idempotency scope, version negotiation, error semantics, and retirement conditions. Authoritative transitions require complete contract/receipt validation, not transport success alone.

## Persistence, concurrency, and recovery

Source registration and command insertion are atomic. Claims commit before Ledger I/O. Success requires a verified explicit receipt; ambiguity enters `reconcile_required`.

Remote effects, where present, must occur after durable intent or claim commit and before a separate outcome transaction. Possible-side-effect timeouts enter pending or reconciliation state; they never authorize blind retry or a second operation identity.

## Configuration and secrets

Bind address, downstream URLs, runtime profile, internal credentials, exact-reserve mode, worker ID, batch/lease/poll/timeout limits, and active rollout switch.

Production-like startup must fail before listening or working when required durable storage, credentials, trust anchors, or explicit modes are absent. Example values are not activation evidence.

## Security and trust boundaries

Authenticate internal callers, bind `x-cex-service-id` to the credential principal, reject dual exact/legacy monetary intent, disable redirects, and bound request/response bodies.

Inputs must be bounded and validated before authority changes. Logs, metrics, traces, and errors exclude sensitive material, unrestricted payloads, and high-cardinality identity fields unless a reviewed contract explicitly permits them.

## Verification

Required commands:

```text
cargo test -p gateway-service
cargo clippy -p gateway-service --all-targets -- -D warnings
python3 scripts/check-gateway-exact-reserve.py
bash scripts/check-gateway-exact-reserve-postgres.sh
```

Required behavioral focus:

- Tenant/auth/body/idempotency and exact/legacy exclusion.
- Claim ownership, lease expiry, retry exhaustion, explicit replay receipt, reconciliation, acknowledgement, and requeue.
- Runtime profile and credential-principal binding before listener startup.

The exact candidate SHA must also pass the authoritative hosted workflow and appear in the generated immutable candidate manifest.

## Deployment and operations

Deploy expand migrations and binaries with active creation disabled, qualify shadow commands, then canary explicitly. Rollback stops workers and preserves immutable command/receipt evidence.

Operators record artifact identity, runtime profile, dependency identities, readiness, rollback boundary, retained evidence, alerts, and owner escalation. Repository CI does not replace representative-volume recovery, sustained load, credential custody, or independent approval.

## Compatibility and change protocol

Legacy non-monetary invocation reads may remain. No legacy amount may be rounded or promoted into exact authority; the exact endpoint becomes sole monetary ingress only after reviewed cutover.

Changes to authority, public types/routes, persistence, configuration, migrations, retry semantics, or topology require this contract, the module catalog, relevant ADR/protocol/traceability, executable tests, hosted gate wiring, and a new shared candidate trigger.

No module document may declare repository closure or production authorization.
