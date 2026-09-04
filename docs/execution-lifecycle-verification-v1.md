# Execution Lifecycle Verification Contract v1

Status: active repository verification contract  
Owner: `execution-runtime`  
Module: `services/execution-service`  
Production authorization: `not_granted`

This contract defines the minimum executable evidence for the Execution module. It consolidates existing unit, integration, static and PostgreSQL checks; it does not replace exact-SHA hosted execution, representative external-provider evidence or independent production approval.

## 1. Authority boundary

Execution owns durable execution-request lifecycle, provider-dispatch intent and evidence correlation, terminal Ledger settlement commands, provider reconciliation observations and operator recovery evidence. It does not own:

- caller or tenant identity;
- Ledger account/effect truth;
- provider truth beyond verified evidence bound to an immutable dispatch identity;
- research, World/Game or Chain-finality authority;
- production authorization.

A successful HTTP response, worker log, provider transport acknowledgement or process-local state is not a terminal business fact unless the owning receipt/evidence contract validates it.

## 2. Verification matrix

| Area | Required behavior | Primary implementation/evidence |
|---|---|---|
| admission authentication | internal caller credential is verified and bound to the declared service principal before state creation | `api.rs`, shared service-auth helpers, package tests |
| tenant/subject binding | organization/tenant and immutable request identity are preserved across reads, claims and outcomes | `api.rs`, `state.rs`, HTTP/package tests |
| initial lifecycle | accepted requests enter the canonical durable initial state; default/fallback state cannot invent success | `state.rs`, `check-execution-default-state-boundary.py` |
| claim/lease | only a live matching worker owner/fence may mutate claimed work; expired work is recoverable | execution migrations, worker/state tests and PostgreSQL gates |
| attempt budget | retries are bounded and exhaustion creates an explicit terminal/operator state | `state.rs`, provider dispatch tests and migrations |
| provider dispatch | immutable dispatch identity and bytes commit before provider I/O | `provider_dispatch.rs`, `providers.rs`, migration 0078 and static checks |
| provider success | terminal success requires complete provider evidence bound to the dispatch/request identity | `check-provider-success-evidence.py`, package tests, migration 0088 |
| unknown provider outcome | timeout after a possible side effect enters reconciliation; it never creates a second dispatch identity | migration 0082/0084, `check-provider-reconciliation-postgres.sh` |
| exact settlement command | one immutable terminal consume/refund command is derived from the execution outcome | `ledger_settlement.rs`, migrations 0067–0074, settlement static/PostgreSQL gates |
| settlement receipt | completion requires an exact Ledger receipt with operation and intent binding | `ledger_settlement.rs`, `check-execution-settlement-commands.py` |
| settlement response loss | same command identity is looked up/reconciled; no blind second monetary operation | settlement PostgreSQL gate and operator transition tests |
| operator recovery | acknowledgement/requeue/repair is actor-, reason-, expected-state- and evidence-bound | migrations 0072/0082/0084 and PostgreSQL gates |
| external-Agent boundary | Execution dispatches only through the reviewed external-Agent/provider boundary and does not discover or host local models | `tests/external_agent_boundary.rs`, `check-external-agent-runtime-boundary.py` |
| production startup | missing durable storage, credentials, trust policy or explicit modes fails before listener/worker mutation | runtime/package tests and shared runtime guards |

## 3. Canonical command set

The hosted Execution gate must run all of the following on the same candidate SHA:

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

A workflow that omits one command does not satisfy this contract. Running commands on different SHAs, using an uncommitted working tree, or attaching manually written success text does not satisfy this contract.

## 4. Required negative cases

The test and PostgreSQL suite must retain explicit failures for:

- missing, malformed or mismatched internal caller credential;
- caller principal different from `x-cex-service-id` or equivalent declared identity;
- cross-tenant read, claim, finish or reconciliation;
- duplicate request identity with different immutable bytes;
- stale owner, stale fence, expired lease and wrong expected state;
- attempt-budget exhaustion and retry after terminal state;
- provider success without complete evidence, with wrong dispatch ID, wrong request ID or tampered evidence hash;
- provider timeout after possible acceptance followed by a new dispatch identity;
- consume/refund command with wrong amount, currency, account, execution ID, operation ID or intent hash;
- HTTP/transport success without a valid Ledger receipt;
- response loss followed by a second monetary identity;
- operator action without actor, reason, expected state or retained evidence;
- local-model discovery, local provider CLI execution or process-local default success.

## 5. Recovery ordering

For provider and Ledger remote effects, the required order is:

1. commit immutable request/dispatch/settlement intent or a fenced claim;
2. release the database transaction;
3. perform bounded remote I/O;
4. validate the complete response/evidence/receipt;
5. commit the outcome in a separate transaction under the same immutable identity;
6. on possible-side-effect timeout, enter pending/reconciliation rather than retrying with a new identity.

No implementation or test may hold an open SQL transaction across provider or Ledger network I/O.

## 6. Gate wiring

`.github/workflows/execution-lifecycle-gate.yml` is the package-level hosted entry point. `scripts/check-execution-lifecycle-coverage.py` verifies that the workflow continues to reference every command and that source, migration and test surfaces retain the contract vocabulary.

The release-candidate aggregate gate must either invoke this workflow for the same SHA or repeat its exact commands. Package-local success is necessary but not sufficient for repository or production qualification.

## 7. Operational evidence still required

Repository closure cannot self-certify:

- actual provider ownership, availability or evidence quality;
- real credential custody and rotation;
- representative latency, concurrency, failure rate and sustained load;
- PostgreSQL failover and recovery in the intended topology;
- Ledger authority identity and real receipt finality;
- alert routing, on-call response and operator approval;
- final human go/no-go.

These remain external evidence inputs governed by the v12 plan/addendum. Until admitted and bound to the exact candidate, `production_authorization` remains `not_granted`.

## 8. Change protocol

A change to lifecycle states, claim rules, provider dispatch identity, success evidence, settlement derivation, retry classification, operator controls, configuration, migrations or topology must update:

1. this contract;
2. the Execution module contract;
3. the relevant migration/protocol/ADR;
4. positive and hostile tests;
5. `scripts/check-execution-lifecycle-coverage.py` where vocabulary or files change;
6. the hosted workflow and shared candidate trigger.

Compatibility aliases may preserve reads, but they may not weaken authentication, tenant binding, immutable identity, evidence validation, exact settlement or reconciliation semantics.
