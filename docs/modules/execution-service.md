# execution-service module contract

Status: active module contract  
Workspace member: `services/execution-service`  
Package: `execution-service`  
Kind: `service`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `execution-runtime`  
Production authorization: `not_granted`

This contract is indexed by `docs/module-catalog-v1.json`. It defines the module boundary for one exact repository tree; it is not release evidence. `docs/execution-lifecycle-verification-v1.md` is the executable coverage contract for lifecycle, authentication, provider evidence, exact settlement and recovery.

## Purpose and non-goals

**Purpose.** Owns the durable lifecycle of an admitted execution request, coordinates work through the reviewed external-Agent/provider boundary, retains immutable provider-dispatch and success/reconciliation evidence, and derives exactly one terminal Ledger consume/refund command under the execution identity.

**Non-goals.** It does not own caller or tenant identity, host or discover local models, grant Agent ownership, mutate Ledger balances directly, infer provider success from transport status, decide scientific truth, own World/Game state, prove Chain finality, or grant production authorization.

## Authority and owned state

The service is authoritative for execution-request state, immutable dispatch identity and bytes, provider attempt/claim/reconciliation records, verified provider evidence bound to that dispatch, exact terminal settlement-command intent, settlement receipt projection and operator recovery evidence.

Identity Service remains authoritative for caller/tenant identity. External providers remain authoritative for their own work facts, subject to evidence validation. Ledger remains authoritative for account/effect truth and exact receipts. A cache, process state, HTTP success, provider acknowledgement or compatibility row never transfers those authorities.

Owned durable state includes request identity and snapshots, expected versions, queue/claim/lease state, bounded attempt budget, dispatch identity, immutable provider request bytes, possible-side-effect/unknown-outcome state, success evidence, reconciliation observations, settlement command identity/hash/status, receipt binding, dead-letter/operator actions and transactional audit/outbox evidence defined by the migrations.

## Source layout and entry points

- `main.rs`: startup, runtime-profile validation, listener and worker composition.
- `lib.rs`: public package boundary and module exports.
- `api.rs`: authenticated HTTP admission, reads and governed transitions.
- `state.rs`: execution lifecycle persistence and invariants.
- `providers.rs`: reviewed external provider interface and response vocabulary.
- `provider_dispatch.rs`: durable provider-dispatch claim/I/O/outcome ordering.
- `dispatch_policy.rs`: bounded provider selection and dispatch policy.
- `ledger_settlement.rs`: exact consume/refund derivation, receipt validation and response-loss handling.
- `settlement_worker_config.rs`, `settlement_worker_helpers.rs`, `settlement_worker_runtime.rs`: dedicated settlement worker configuration and loop.
- `bin/execution-provider-dispatch-worker.rs`: external-provider dispatch worker.
- `bin/execution-settlement-worker.rs`: terminal Ledger settlement worker.
- `tests/external_agent_boundary.rs`: external-Agent-only architectural regression.
- `migrations/0067`–`0074`, `0078`, `0082`, `0084`, `0088`: settlement, provider dispatch, unknown-outcome reconciliation and terminal evidence guards.

Catalog-bound entry points:

- `services/execution-service/src/main.rs`
- `services/execution-service/src/lib.rs`
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

Any new process target, public route, provider adapter, state owner, migration, retry classification or operator action must update the module catalog and this document in the same commit.

## Interfaces and contracts

HTTP and worker interfaces require authenticated internal principals, tenant/organization binding, stable execution/request identity, expected-version or claim ownership where applicable, bounded bodies and explicit error/retry semantics. Caller-provided principal fields never override the authenticated principal.

Provider dispatch uses one immutable dispatch identity and payload fingerprint. The provider boundary must return a bounded typed outcome. Transport success alone is not provider success; terminal success requires the complete evidence contract bound to execution ID, dispatch ID, provider identity, request/payload hash and expected result semantics.

Terminal settlement is an immutable Ledger v2 command derived from the final execution outcome. A successful execution consumes the exact reserved amount only under the reviewed outcome contract; a non-chargeable terminal outcome refunds it. The command carries stable operation and intent identity. Completion requires a complete Ledger receipt with matching account, amount, currency, effect, operation identity and intent hash.

Operator recovery interfaces require authenticated actor, bounded reason, expected state/version, retained evidence and append-only audit. They may reconcile or requeue the same immutable identity; they may not fabricate provider success, Ledger settlement or a second remote operation.

## Persistence, concurrency, and recovery

Authoritative execution, dispatch and settlement state is PostgreSQL-backed in production-like profiles. Source/request registration, immutable command creation and local outbox/audit enqueue are transactional. Remote provider or Ledger I/O is performed only after the durable intent or fenced claim commits and after the transaction is released.

Claims contain owner, lease/fence or equivalent expected-state identity, expiry and bounded attempt budget. A stale owner, stale fence, expired lease or wrong expected version cannot finish work. Expired claims expose the same immutable identity for recovery.

A timeout after a possible provider or Ledger side effect enters pending/unknown/reconciliation state. Recovery queries or retries using the same dispatch/operation identity and the same bytes. It never creates a second dispatch or monetary operation. Provider reconciliation replay is terminal-safe; complete terminal provider evidence cannot be overwritten by a later ambiguous observation.

Settlement command and receipt history remain queryable after response loss and process restart. Dead-letter and operator state retain the last error classification, attempts, actor/reason and evidence required for review.

## Configuration and secrets

Configuration includes runtime profile, listener, durable database URL/role, authenticated caller registry, body and concurrency limits, provider registry/policy, provider endpoints and credentials, dispatch worker identity/batch/lease/poll/timeout/attempt limits, Ledger endpoint/credential/authority, settlement worker identity/batch/lease/poll/timeout limits, audit endpoint/credential and explicit rollout modes.

Production-like startup must fail before listening, claiming or performing remote I/O when durable storage, required schema, trust anchors, explicit provider/Ledger mode, pairwise credential separation or bounded limits are absent. Development placeholders and implicit local-model discovery are rejected.

Provider, Ledger, audit, operator and ingress credentials are separate authorities and come from approved secret custody. They are not embedded in committed examples, payload snapshots, logs, metrics or errors.

## Security and trust boundaries

Authenticate before tenant lookup or mutation; bind service identity to credential rather than request JSON; reject cross-tenant reads/transitions; bound decompressed request and provider response bytes; disable unsafe redirects; validate provider and Ledger endpoint identity; and redact credentials, database URLs, unrestricted prompts/results, private research material and high-cardinality identifiers from telemetry.

External-Agent/provider output is untrusted until complete evidence validation. Provider identity, dispatch identity, payload hash, result hash, timestamps/expiry and configured policy must match. Local model directories, CLIs or process-local allowlists are not provider authority.

Ledger response validation is fail closed. A transport acknowledgement, status code, compatibility amount or projection cannot become exact account/effect truth. Break-glass behavior requires a separate authenticated, audited and evidence-bound operator contract.

## Verification

Required exact-SHA commands:

```text
cargo fmt --all -- --check
cargo test --locked -p execution-service --all-targets
cargo clippy --locked -p execution-service --all-targets -- -D warnings
python3 scripts/check-execution-lifecycle-coverage.py
python3 scripts/check-execution-default-state-boundary.py
python3 scripts/check-execution-ledger-settlement.py
python3 scripts/check-execution-settlement-commands.py
python3 scripts/check-provider-success-evidence.py
python3 scripts/check-external-agent-runtime-boundary.py
bash scripts/check-execution-settlement-commands-postgres.sh
bash scripts/check-provider-reconciliation-postgres.sh
```

`.github/workflows/execution-lifecycle-gate.yml` must execute the complete command set on one checkout. Required hostile coverage includes missing/mismatched caller identity, cross-tenant access, duplicate identity collision, stale/expired claim, attempt exhaustion, provider success without evidence, tampered/wrong evidence, timeout after possible provider acceptance, wrong exact settlement command, invalid/mismatched receipt, response loss, second-operation rejection and unauthorised operator repair.

The exact candidate SHA must also pass the repository aggregate release gate and appear in the immutable candidate manifest. A local run, static marker, skipped PostgreSQL probe or a workflow created but not allocated to a runner is not qualification evidence.

## Deployment and operations

Apply expand migrations with a schema owner, verify constraints/procedures, remove the owner secret from resident processes, then run API, provider-dispatch worker and settlement worker under separate least-privilege identities. Enable creation/dispatch/settlement through explicit staged rollout switches; never infer activation from deployed code alone.

Readiness reports database/schema, caller-auth registry, provider policy/trust, Ledger trust and worker posture separately. Monitor oldest queued request, claim age/expiry, attempt exhaustion, provider unknown-outcome age, reconciliation backlog, settlement-command age, invalid receipt count, dead-letter count and operator actions.

Rollback stops new admission/claims, fences active workers, preserves immutable request/dispatch/settlement/evidence state, and switches only to a schema/protocol-compatible artifact. Unknown provider or Ledger outcomes must be reconciled before destructive rollback or retry-policy change.

## Compatibility and change protocol

Legacy reads may remain during migration, but new authoritative writes use the canonical durable request, provider-dispatch and exact settlement contracts. Compatibility fields may not infer precision, evidence, success or finality that the source contract did not provide.

Changes to lifecycle states, authority, routes/types, provider evidence, persistence, configuration, migrations, retry semantics, operator controls, settlement derivation or topology require this contract, `docs/execution-lifecycle-verification-v1.md`, the module catalog, relevant ADR/protocol/traceability, executable positive and hostile tests, hosted gate wiring and a new shared candidate trigger.

No module document may declare repository closure or production authorization.
