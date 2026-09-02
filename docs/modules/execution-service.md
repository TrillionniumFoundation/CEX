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

**Purpose.** Owns the durable execution-request lifecycle, external-Agent work/evidence correlation, terminal Ledger settlement commands, and operator recovery records.

**Non-goals.** It does not host, route, schedule, or execute participating Agents; it does not provide model inference, Prompt hosting, or provider credential custody; it must not hold SQL transactions across remote calls, infer a remote outcome, or own Ledger balances.

## Authority and owned state

Execution Service is authoritative for execution request state, immutable operation identity, claim/lease history, external-Agent dispatch intent references, received evidence correlation, terminal settlement commands, and operator recovery acknowledgements.

Provider-dispatch rows created by migrations `0078` through `0088` are retained as historical compatibility and reconciliation evidence. Their presence does not authorize CEX to run an Agent. Agent identity, capability, and signed scientific output remain governed by `hepta_agent_protocol_v1`; Ledger and Chain retain their own authority.

A projection, cache, compatibility row, HTTP success, or transport acknowledgement never transfers authority from its owning component.

## Source layout and entry points

- `api.rs`: execution admission, lifecycle, claim/lease, terminal evidence, and recovery handlers. Its retired local-process handler is not routed by the default service.
- `dispatch_policy.rs`: bounded dispatch-state policy; it grants no Agent runtime authority.
- `ledger_settlement.rs` and `settlement_worker_*`: durable consume/refund command handling.
- `state.rs`: durable/in-memory state boundary.
- `providers.rs`: pure provider identity/input/output types plus the default fail-closed external-Agent-required boundary; it contains no local inference implementation.
- `provider_dispatch.rs` and `bin/execution-provider-dispatch-worker.rs`: retained historical command/reconciliation compatibility source, excluded from the default build behind `legacy-local-provider-dispatch` and forbidden in production-like profiles.
- `tests/external_agent_boundary.rs`: default integration regression proving no process route, network inference, subprocess execution, or prompt echo.

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
- `services/execution-service/tests/external_agent_boundary.rs`

Any new binary, public source boundary, migration owner, or removed path must update the catalog and this document in the same commit.

## Interfaces and contracts

The default HTTP router exposes execution admission, approval, dispatch-state, start, claim/lease, retry, terminal status, dead-letter/reconciliation reads, and settlement-recovery operations. The retired `/process` route is absent from the default router and returns `404 Not Found`. The default workspace build does not compile or route to local provider adapters.

External Agent work and results use the public Hepta Agent protocol with stable Agent identity, explicit versions, signed claims, immutable content hashes, and bounded evidence. Requests and events define authentication, tenant/subject binding, size bounds, immutable identity, idempotency, errors, and retirement conditions. A successful transport status never proves scientific, provider, value, or finality state.

## Persistence, concurrency, and recovery

Every authoritative remote effect follows durable intent/commit → claim/commit → remote exchange → outcome/commit. Expired leases after a possible side effect enter reconciliation and cannot authorize blind replay or a new operation identity.

Historical provider command/evidence tables remain append-only and covered by PostgreSQL regression so response-loss and upgrade behavior stay explainable. The active external-Agent path correlates signed protocol evidence rather than executing a local model. Settlement commands preserve exact consume/refund mutual exclusion, receipt recovery, dead-letter, and explicit operator requeue semantics.

## Configuration and secrets

The default service uses database, runtime profile, internal service authentication, exact Ledger authority, worker identity, lease/retry limits, and downstream protocol trust anchors. It requires no model-provider inference credential and does not read local model catalogs.

The compatibility binary exists only when Cargo feature `legacy-local-provider-dispatch` is selected. It additionally requires `CEX_ENABLE_LEGACY_LOCAL_PROVIDER_DISPATCH=true`, accepts only isolated test/local/dev profiles, and rejects beta, staging, and production before dispatch. Its provider boundary still fails closed and performs no model call. No authoritative workflow or deployment configuration may activate it.

Production-like startup fails before listening or working when durable storage, credentials, trust anchors, or explicit modes are missing. Example values are not activation evidence.

## Security and trust boundaries

Agent private keys and model-provider inference keys stay outside CEX. Inputs, result references, and receipts are bounded and validated before state changes; unrestricted prompts, outputs, credentials, and response bodies must not enter logs, metrics, ordinary Audit payloads, or database error text.

Legacy command/reconciliation source is not a supported production trust boundary. Re-enabling local inference requires an ADR that supersedes ADR-004, bounded transport, a privacy/cost/threat model, dedicated tests, and independent review. Operator reconciliation evidence must bind immutable URI/digest, exact attempt, and outcome without inventing success.

## Verification

Required commands:

```text
cargo test -p execution-service --all-targets
cargo clippy -p execution-service --all-targets -- -D warnings
python3 scripts/check-external-agent-runtime-boundary.py
python3 scripts/check-execution-settlement-commands.py
bash scripts/check-execution-settlement-commands-postgres.sh
python3 scripts/check-provider-success-evidence.py
bash scripts/check-provider-reconciliation-postgres.sh
```

Required behavioral focus:

- The default build excludes local provider adapters and the default router does not expose the retired `/process` route.
- A request to the retired path returns 404, and source-level regression prevents the route from being restored silently.
- The compatibility provider function always returns `external_agent_runtime_required`, performs no network/process execution, and never echoes prompt/target details.
- Execution state transitions, claim ownership, terminal mutual exclusion, and settlement recovery remain deterministic.
- Historical provider evidence still rejects malformed/mismatched/nonterminal success, unsafe lease replay, and evidence collisions.
- External Agent capability/work/result identity remains protocol-bound and does not become platform inference authority.

The exact candidate SHA must also pass the authoritative hosted workflow and appear in the generated immutable candidate manifest.

## Deployment and operations

Deploy only the default external-Agent-only Execution service and settlement worker. Do not build or deploy `execution-provider-dispatch-worker`; the feature and opt-in flag are local compatibility controls, not rollout switches.

Operators record artifact identity, runtime profile, dependency identities, readiness, rollback boundary, retained evidence, alerts, and owner escalation. Rollback stops active workers while preserving commands and evidence. Repository CI does not replace representative-volume recovery, external Agent deployment proof, sustained load, credential custody, or independent approval.

## Compatibility and change protocol

Legacy provider rows and reconciliation functions remain readable until an evidence-backed retirement migration proves they are no longer needed. They may not be used to reintroduce platform-owned Agent execution. New execution integrations must use versioned external-Agent contracts.

Changes to authority, routes, persistence, configuration, migrations, retry semantics, features, or topology require this contract, the module catalog, ADR-004, the Sequence-52 architecture closure contract, executable tests, hosted gate wiring, and a new shared candidate trigger. No module document may declare repository closure or production authorization.
