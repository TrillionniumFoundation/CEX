# CEX component and qualification status

Status: active canonical component-status document  
Candidate sequence: `54`  
Workspace: `23` Cargo members (`18` first-party + `5` vendored TRNM packages)  
Migration head: `0088_enforce_provider_terminal_evidence_binding.sql`  
Matrix transport head: `0005_filter_definition_pins.sql`  
Matrix operator head: `0004_adapter_result_runtime_reconciliation.sql`  
Repository qualification: `PENDING_EXACT_SHA_HOSTED_EVIDENCE`  
Production authorization: `not_granted`

This document reports source and maturity posture only. It is not exact-SHA
hosted evidence, branch-protection evidence, independent review, external
production evidence or a human release decision.

## Architecture status

Accepted ADR-004 defines Hepta, Nakama and TRNM as the top-level domains.
Participating Agents and model/provider runtimes remain external. The immutable
Sequence 52 architecture baseline removes the retired local process route and
local inference authority; Sequence 54 non-regressively integrates that boundary
with current money, Matrix, toolchain, supply-chain and governance controls.

Execution owns admitted work lifecycle, external-Agent evidence correlation and
exact terminal Ledger settlement. It does not host participating Agents. The
legacy provider worker remains feature-gated, absent from the default build and
rejected in production-like profiles. Capability Service consumes only a strict
`CAPABILITY_EXTERNAL_AGENT_REGISTRY_JSON` snapshot and performs no local model
or CLI discovery.

`.env.production.example` follows the same authority: it contains an external
Agent declaration and no active OpenClaw/Ollama/provider-dispatch configuration.

## Workspace component status

| Component | Maturity | Authority posture | Current repository status | Promotion blockers |
|---|---|---|---|---|
| `shared-types` | active library | versioned value, receipt and event types | documented/catalog-bound | exact-SHA tests and compatibility evidence |
| `shared-errors` | active library | typed error vocabulary | documented/catalog-bound | stable mapping regression |
| `shared-tracing` | active library | tracing initialization and field rules | documented/catalog-bound | collector, retention and redaction evidence |
| `shared-config` | active library | profile, service-auth and client configuration | documented/catalog-bound | cross-service startup regression |
| `hepta-paper-raid-contracts` | contract-qualified | canonical signed frames and identifiers | repository contract present | hosted vectors and downstream binding |
| `identity-service` | repository candidate | tenant, actor and API-key lifecycle | source/migration/docs present | PostgreSQL/startup, custody and operations review |
| `ledger-service` | repository candidate | exact minor-unit effects and receipts | source/migration/docs present | hosted qualification, recovery/load and financial review |
| `trnm-economy-service` | repository candidate | authenticated economy intent and receipt projection | source/config/docs present | Chain binding, real credentials and independent review |
| `gateway-service` | repository candidate | invocation ingress and exact reserve commands | source/migration/docs present | hosted reserve/reconciliation and rollout proof |
| `execution-service` | repository candidate | lifecycle, external-Agent evidence and settlement | default local execution absent | compile/test, external Agent integration and settlement recovery |
| `audit-service` | repository candidate | authenticated append, hash chain and outbox | source/migration/docs present | recovery, custody/retention and independent review |
| `capability-service` | supporting alpha | strict read-only external Agent declarations | external-only registry implemented | hosted tests and external issuer governance |
| `consumer-entry-api` | supporting alpha | sessions, identity mapping and bounded projections | broad source documented | bounded-context decomposition, route/schema inventory and authority rehearsal |
| `hepta-research-league` | functional alpha | research facts, consent, evidence and review | PostgreSQL/lint contracts present | hosted recovery, representative load and independent review |
| `paper-raid-bff` | supporting alpha | browser sessions, CSRF and invite-alpha edge | boundary/runbook/tests present | real IdP/deployment, accessibility/load and custody |
| `matrix-entry-adapter` | supporting alpha | Matrix validation, normalization and read-only recovery | lookup plus operator migrations `0001`–`0004` implemented | PostgreSQL 16, real response-loss and homeserver qualification |
| `matrix-bot-relay` | supporting alpha | durable relay and scoped Matrix sends | unknown outcomes hold; one-delivery reconciler implemented | hosted PostgreSQL and end-to-end response-loss rehearsal |
| `matrix-bot-poller` | supporting alpha | polling, cursor and stream-scope transport | durable fencing/filter contracts present | real homeserver and long-gap recovery evidence |
| `trnm-economy-protocol` | supporting alpha vendor | economy policy and entitlement wire shapes | catalog/documented | independent provenance and consumer conformance |
| `trnm-finality-types` | supporting alpha vendor | receipt/quorum/proof types | pinned/documented | trust-anchor lifecycle and hosted vectors |
| `trnm-finality-verifier` | supporting alpha vendor | node-independent receipt verification | downstream test-only patch ledgered | pinned upstream rebase and hosted compile/tests |
| `trnm-protocol` | supporting alpha vendor | canonical transaction encoding | pinned/documented | cross-language conformance and migration policy |
| `trnm-research-protocol` | supporting alpha vendor | signed research commands/state transitions | pinned/documented | external authority integration and cross-language tests |

## Repository-actionable closure status

Sequence 54 source contains:

- exact 23-member workspace/catalog/module-document correspondence;
- external-Agent-only Capability and Execution boundaries;
- exact Ledger, Gateway, Execution, Audit and TRNM identities and recovery paths;
- durable Matrix transport, cursor, filter, poison, send-binding and receipt state;
- read-only principal-bound Matrix result lookup;
- operator migration 0004 fixing first-success result-payload persistence;
- a one-delivery reconciliation command that never replays the business request;
- bounded vendor provenance and the test-only finality-verifier patch ledger;
- Rust 1.98.1, advisory policy and exact candidate evidence machinery.

Repository source presence is not repository qualification. The current candidate
remains pending until required workflows execute non-empty and succeed on one
unchanged source and prospective merge tuple, artifacts are retained and the
candidate manifest validates them.

## Current independent blockers

The following facts cannot be created by another repository commit:

1. allocated runners and non-empty exact-SHA/prospective-merge execution;
2. effective protected-main/ruleset enforcement and negative probes;
3. two fresh eligible approvals, including independent security approval;
4. accepted immutable World, Game/Nakama, Chain and CEX component tuple;
5. V12-X1 representative-volume disaster recovery;
6. V12-X2 real deployment, cutover and rollback;
7. V12-X3 real external-Agent/provider and Matrix response-loss reconciliation;
8. V12-X4 credential custody, rotation and revocation;
9. V12-X5 sustained production-like soak and SLO evidence;
10. V12-X6 independent security, operations and financial-control review;
11. V12-X7 applicable legal, commercial and provider approval;
12. V12-X8 final accountable human go/no-go.

A workflow with `runner_id=0`, no runner name and no steps is a control-plane or
entitlement failure, not source qualification. A requested reviewer is not an
approval. A desired ruleset JSON is not live enforcement.

## Change protocol

Any source, workflow, migration, security, test, module contract, external
component contract, traceability or normative-document change creates a new
candidate tree. Update the sole shared trigger once after the complete change,
run every authoritative context on the unchanged final tuple, retain the generated
manifest and obtain fresh independent review.

`production_authorization` remains `not_granted` unless the external evidence
bundle and final human decision explicitly grant activation. Repository automation
may reject or qualify a candidate; it may not manufacture production authority.
