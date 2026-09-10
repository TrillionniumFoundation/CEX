# execution-service module contract

Status: active module contract  
Workspace member: `services/execution-service`  
Package: `execution-service`  
Kind: `service`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `execution-runtime`  
Production authorization: `not_granted`

The accepted ADR-004 and Sequence-52 architecture contract control this module.
The parent v12 plan's provider-specific requirements apply only to historical
command/evidence interpretation. This document cannot reactivate that runtime.

## Purpose and non-goals

Execution owns admitted work lifecycle, external-Agent work/evidence correlation,
terminal exact Ledger settlement commands and operator recovery. Participating
Agents run outside CEX. Execution does not host or execute them, discover models,
store their inference credentials, decide scientific truth, mutate Ledger balances,
or establish World/Game state or Chain consensus finality.

The default workspace build does not compile or route to local provider adapters.
The retired `/process` route remains absent from every default router.

## Authority and owned state

Owned durable facts are request identity and snapshots, expected versions,
lifecycle transitions, settlement command identity/hash, validated receipt
projections and operator evidence. Historical provider attempts, immutable request
bytes and reconciliation artifacts remain interpretable for migration and audit;
retaining them does not make the provider worker a current production component.
Identity, Hepta research, Ledger and Chain retain their respective authority.

## Source layout and entry points

Catalog-bound entry points:

- `services/execution-service/src/main.rs`: API startup and runtime validation.
- `services/execution-service/src/lib.rs`: public package boundary and modules.
- `services/execution-service/src/api.rs`: authenticated lifecycle HTTP routes.
- `services/execution-service/src/provider_dispatch.rs`: retained historical
  command, claim and reconciliation implementation; not Agent-hosting authority.
- `services/execution-service/src/providers.rs`: compatibility identity/types and
  the fail-closed `external_agent_runtime_required` result; no inference call.
- `services/execution-service/src/dispatch_policy.rs`: historical policy semantics.
- `services/execution-service/src/ledger_settlement.rs`: exact terminal settlement.
- `services/execution-service/src/settlement_worker_runtime.rs`: settlement loop.
- `services/execution-service/src/state.rs`: lifecycle state and persistence.
- `services/execution-service/src/bin/execution-provider-dispatch-worker.rs`:
  explicitly feature-gated legacy compatibility target, excluded from defaults.
- `services/execution-service/src/bin/execution-settlement-worker.rs`:
  current exact consume/refund settlement worker.
- `services/execution-service/tests/external_agent_boundary.rs`:
  external-only and retired-route regression.

Migration-0088 provider terminal-evidence guards remain preserved. Any source or
process boundary change updates the catalog and dedicated behavioral verification.

## Interfaces and contracts

Lifecycle requests require authenticated internal principals, tenant binding,
stable request identity, bounded bodies, and expected version or claim ownership
where applicable. The retired `/v1/executions/:id/process` path remains absent
from the default router. Caller-supplied subject fields never replace principals.

Current participating-Agent work/results enter through the signed Hepta Agent
protocol and its owning control-plane interfaces, not by calling the legacy
provider worker. `dispatch_via_provider` is a compatibility error surface, not a
usable external-Agent execution client. Historical result reconciliation requires
same-attempt, immutable, content-bound evidence and cannot fabricate execution.

Terminal consume/refund intent uses one stable exact Ledger operation identity.
Completion requires a full matching Ledger receipt, not HTTP success alone.
Operator changes require authenticated actor, bounded reason, expected state and
append-only evidence; they cannot create a second operation to evade ambiguity.

## Persistence, concurrency, and recovery

Production-like lifecycle and settlement authority requires PostgreSQL. Request
registration, command creation and source audit/outbox enqueue are transactional.
Claims commit before remote Ledger I/O, and outcomes use a separate transaction.
An expired or wrong owner cannot acknowledge a live claim. A possible remote
side effect yields pending/reconciliation, never guessed success or blind retry.

Historical provider rows retain live-versus-reconciled evidence distinctions.
They remain subject to terminal immutability and migration-0088 negative tests.
Response-loss recovery queries/replays the same immutable identity and bytes.
Dead-letter repair retains attempt, actor, reason and evidence history.

## Configuration and secrets

Current configuration includes runtime profile, bind address, durable database
role, internal caller credentials, body/concurrency bounds, Ledger endpoint and
trust, settlement worker batch/lease/poll/timeout values, and Audit credentials.
Required values and schema are validated before listening or claiming work.

Model inference credentials and participating-Agent private keys are not current
Execution configuration. Historical provider-related names may remain for data
compatibility but must not be activated in production examples or deployments.
`legacy-local-provider-dispatch` is absent from authoritative builds; even an
explicit compatibility build rejects production-like profiles and requires its
separate local-only opt-in. Its provider function still performs no model call.

Ingress, Ledger, Audit and operator identities are separate authorities with
approved custody. Missing durable state, placeholders, conflicting mode selection
or untrusted endpoints must not silently become an in-memory authority.

## Security and trust boundaries

Authenticate before authority mutation; bind service identity to credentials,
not JSON; reject cross-tenant transitions; bound bodies and responses; disable
unsafe redirects; and validate complete receipt identity, amount, currency and
intent hash. Untrusted external output is not scientific or monetary authority.

Do not log credentials, database URLs, unrestricted prompts/results or private
research. Historical compatibility errors must not echo provider target, prompt
or response body. Break-glass repair is independently authenticated and audited.

## Verification

Required commands:

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

Retain every lifecycle and negative test: caller/principal mismatch, cross-tenant
access, duplicate collision, stale claim, attempt exhaustion, wrong or missing
provider evidence, response loss, invalid Ledger receipt and unauthorized repair.
Default-feature tests additionally prove no local provider execution or prompt
disclosure and `404` for the retired process route. Historical SQL regression is
not evidence that a real external Agent or model provider was contacted.

`docs/execution-lifecycle-verification-v1.md` and the hosted lifecycle lane define
complete coverage. Non-empty exact-SHA execution and aggregate candidate evidence
remain mandatory; a static source check or definition-only workflow is not a pass.

## Deployment and operations

Deploy the current API and settlement worker with separate least-privilege
identities after approved expand migrations. Do not launch the legacy provider
worker as part of the normal stack, enable its feature, or provision model keys
to CEX. External Agents are independently deployed under the accepted protocol.

Readiness distinguishes database/schema, caller authentication, Ledger trust and
settlement-worker posture. Monitor queue age, claim expiry, unknown outcomes,
settlement age, invalid receipts, dead letters and operator actions. Do not infer
external Agent availability from a retained provider row or local health endpoint.

Rollback stops admission/claims, fences workers, preserves immutable lifecycle,
settlement and historical evidence, and uses a schema-compatible artifact. Unknown
Ledger outcomes must be reconciled before destructive recovery or policy changes.

## Compatibility and change protocol

Read compatibility does not grant current write or runtime authority. Preserve
historical provider evidence until a separately reviewed data-retirement plan
exists. New work uses the external-Agent boundary and exact settlement contracts.
A local model runtime would require a new accepted superseding ADR, not an option
added to a deployment guide. Update this contract, catalog, lifecycle coverage,
protocol/traceability, tests and exact-tree evidence together. No module document
may declare repository closure or production authorization.
