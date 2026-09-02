# execution-service module contract

Status: active module contract  
Workspace member: `services/execution-service`  
Package: `execution-service`  
Kind: `service`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `execution-runtime`  
Production authorization: `not_granted`

This contract is indexed by `docs/module-catalog-v1.json`. It defines the module boundary for one exact repository tree; it is not release evidence.

## Purpose and non-goals

**Purpose.** Owns asynchronous execution state and turns terminal work outcomes into durable, separately claimed provider and Ledger commands.

**Non-goals.** It must not hold business SQL transactions across network calls, infer provider success, own Ledger balances, or automatically retry after a possible remote side effect.

## Authority and owned state

Execution lifecycle, durable provider-dispatch commands, terminal Ledger settlement commands, and operator recovery evidence.

Owned state: Execution records, provider-dispatch commands and evidence, settlement commands, claim leases, retry budgets, transition history, acknowledgements, and requeue records.

A projection, cache, compatibility row, HTTP success, or transport acknowledgement never transfers authority from its owning component.

## Source layout and entry points

- `api.rs`: execution HTTP lifecycle.
- `provider_dispatch.rs`: durable provider command worker and reconciliation boundary.
- `providers.rs`: provider adapters and response shapes.
- `ledger_settlement.rs`: consume/refund adapter.
- `settlement_worker_*`: worker configuration/runtime.
- `state.rs`: durable/in-memory state boundary.

Catalog-bound entry points:

- `services/execution-service/src/main.rs`
- `services/execution-service/src/api.rs`
- `services/execution-service/src/provider_dispatch.rs`
- `services/execution-service/src/providers.rs`
- `services/execution-service/src/dispatch_policy.rs`
- `services/execution-service/src/ledger_settlement.rs`
- `services/execution-service/src/settlement_worker_runtime.rs`
- `services/execution-service/src/state.rs`
- `services/execution-service/src/bin/execution-provider-dispatch-worker.rs`
- `services/execution-service/src/bin/execution-settlement-worker.rs`
- `services/execution-service/tests/http_flow.rs`

Any new binary, public source boundary, migration owner, or removed path must update the catalog and this document in the same commit.

## Interfaces and contracts

HTTP execution routes plus worker binaries. Provider adapters return typed output, but durable success is written only after immutable target and terminal payload evidence validate.

Requests and events must define authentication, tenant/subject binding, size bounds, immutable identity, idempotency scope, version negotiation, error semantics, and retirement conditions. Authoritative transitions require complete contract/receipt validation, not transport success alone.

## Persistence, concurrency, and recovery

Every remote operation follows enqueue/commit → claim/commit → network I/O → outcome/commit. Expired claims after possible dispatch require reconciliation, not blind replay.

Remote effects, where present, must occur after durable intent or claim commit and before a separate outcome transaction. Possible-side-effect timeouts enter pending or reconciliation state; they never authorize blind retry or a second operation identity.

## Configuration and secrets

Database, runtime profile, provider targets, credentials, exact Ledger mode, worker identity, batch/lease/poll/timeout, maximum attempts, and shadow/active rollout settings.

Production-like startup must fail before listening or working when required durable storage, credentials, trust anchors, or explicit modes are absent. Example values are not activation evidence.

## Security and trust boundaries

Provider target, provider identity, model, explicit terminal flag, and nonempty output evidence must match the immutable command. Secrets and provider bodies are bounded and redacted from logs/audit.

Inputs must be bounded and validated before authority changes. Logs, metrics, traces, and errors exclude sensitive material, unrestricted payloads, and high-cardinality identity fields unless a reviewed contract explicitly permits them.

## Verification

Required commands:

```text
cargo test -p execution-service
cargo clippy -p execution-service --all-targets -- -D warnings
python3 scripts/check-execution-settlement-commands.py
bash scripts/check-execution-settlement-commands-postgres.sh
python3 scripts/check-provider-success-evidence.py
bash scripts/check-provider-reconciliation-postgres.sh
```

Required behavioral focus:

- Execution state transitions and terminal mutual exclusion.
- Provider timeout, malformed/mismatched/nonterminal success, lease expiry, confirmed-not-executed requeue, and exact terminal replay.
- Settlement consume/refund receipt verification, crash recovery, dead letter, acknowledgement, and requeue.

The exact candidate SHA must also pass the authoritative hosted workflow and appear in the generated immutable candidate manifest.

## Deployment and operations

Start in shadow/fail-closed mode. Operate provider and settlement workers separately with least-privilege roles. Stop workers for rollback while preserving commands and evidence.

Operators record artifact identity, runtime profile, dependency identities, readiness, rollback boundary, retained evidence, alerts, and owner escalation. Repository CI does not replace representative-volume recovery, sustained load, credential custody, or independent approval.

## Compatibility and change protocol

Legacy execution reads may remain, but provider/settlement side effects must use durable exact commands. Unsupported providers or malformed success envelopes become reconciliation.

Changes to authority, public types/routes, persistence, configuration, migrations, retry semantics, or topology require this contract, the module catalog, relevant ADR/protocol/traceability, executable tests, hosted gate wiring, and a new shared candidate trigger.

No module document may declare repository closure or production authorization.
