# CEX component and qualification status

Status: active canonical component-status document  
Candidate sequence: `51`  
Migration head: `0088_enforce_provider_terminal_evidence_binding.sql`  
Repository qualification: `PENDING_EXACT_SHA_HOSTED_EVIDENCE`  
Production authorization: `not_granted`

This document reports source and maturity posture only. It is not exact-SHA hosted evidence, branch-protection evidence, independent review, external production evidence or a human release decision.

## Architecture status

Accepted ADR-004 defines three top-level domains: Hepta, Nakama and TRNM. Participating Agents and model/provider runtimes remain external. Sequence 51 closes the repository contradiction that previously allowed Capability Service to discover local models and Execution Service to execute Ollama/OpenClaw providers.

The default Execution build now routes lifecycle operations through the external-Agent control/evidence boundary. The retained provider-command database, reconciliation logic and migration-0088 terminal-evidence guards remain historical compatibility and recovery facts. The legacy worker is excluded from the default build, rejects every production-like profile and its provider compatibility function performs no local inference.

Capability Service now accepts a bounded, strict `CAPABILITY_EXTERNAL_AGENT_REGISTRY_JSON` snapshot containing external Agent declarations only. Missing registry is non-ready in local development and a startup failure in production-like profiles. Local model scanning, OpenClaw CLI execution, implicit demo capabilities and provider-availability authority are not part of the active component.

Executable architecture status is determined by `scripts/check-external-agent-runtime-boundary.py` and the M1–M8 ledger in `docs/traceability/sequence-51-architecture-v1.json`, never by this prose alone.

## Workspace component status

| Component | Maturity | Authority posture | Current repository status | Promotion blockers |
|---|---|---|---|---|
| `shared-types` | active library | versioned cross-service value/receipt/event types; no durable state | documented and catalog-bound | hosted exact-SHA workspace tests and compatibility evidence |
| `shared-errors` | active library | shared typed error vocabulary only | documented and catalog-bound | stable error-code/mapping regression on exact candidate |
| `shared-tracing` | active library | tracing initialization/field conventions; no business authority | documented and catalog-bound | production collector, retention and redaction evidence |
| `shared-config` | active library | runtime-profile, service-auth and fail-closed configuration semantics | documented and catalog-bound | exact-SHA cross-service startup regression |
| `hepta-paper-raid-contracts` | contract-qualified | canonical signed-byte and cryptographic contract surface | repository contract present | exact-head hosted vectors and downstream compatibility binding |
| `identity-service` | repository candidate | tenant, actor and API-key lifecycle | source/documentation present | exact-head PostgreSQL/startup tests, credential custody and operational review |
| `ledger-service` | repository candidate | exact minor-unit Ledger effects and immutable receipts | migration/test/document contracts present | exact-head hosted qualification, real recovery/load and financial-control review |
| `trnm-economy-service` | repository candidate | signed economic-intent admission and settlement projection | source/config/credential contracts present | Chain/downstream binding, real credentials, recovery and independent approval |
| `gateway-service` | repository candidate | invocation normalization and durable exact-reserve commands | source/migration/test contracts present | exact-head hosted reserve/reconciliation evidence and production rollout proof |
| `execution-service` | repository candidate | execution lifecycle, external-Agent evidence correlation and terminal settlement | default local inference removed; historical evidence retained | exact-head compile/test, external Agent integration evidence and real reconciliation |
| `audit-service` | repository candidate | authenticated append, hash chain and durable outbox evidence | source/migration/test contracts present | exact-head hosted recovery, external custody/retention and independent review |
| `capability-service` | supporting alpha | validated external Agent capability declarations only | strict registry/readiness implementation present | exact-head Rust tests, authenticated distribution if required and external issuer governance |
| `consumer-entry-api` | supporting alpha | sessions, identity mapping and bounded product projections | broad source surface documented | bounded-context decomposition, route/schema inventory and World/Game ownership enforcement |
| `hepta-research-league` | functional alpha | durable research facts, Agent bindings, consent, evidence and review | strict PostgreSQL recovery/lint contracts present | non-empty exact-SHA hosted execution, representative load and independent release review |
| `paper-raid-bff` | supporting alpha | browser sessions, CSRF, invite-alpha access and edge assertions | boundary/runbook/test contracts present | real IdP/deployment, accessibility/load, credential custody and release approval |
| `matrix-entry-adapter` | supporting alpha | Matrix validation, normalization, cursor/dedup and delivery observations | detailed durability/fencing contract present | durable cursor implementation, restart/multi-instance evidence and real homeserver tests |
| `matrix-bot-relay` | supporting alpha | relay transport only | module contract present | durable outbox/dead-letter semantics, readiness and response-loss evidence |
| `matrix-bot-poller` | supporting alpha | polling/cursor transport only | detailed cursor/fencing contract present | durable cursor CAS/fencing, poison-event and real Matrix recovery evidence |

## External components

Nakama, Trillionnium Chain, Matrix homeserver, content-addressed object storage, external providers/Agents, the pinned Trillionnium World fixture and the pinned Trillionnium Game/Nakama runtime are external components. Their repository pins or protocol contracts do not make them Cargo members or transfer their authority to CEX.

World fixture checks establish only deterministic compatibility for an exact source identity. Game/Nakama exact-source checks establish only the recorded source/build/test facts. Public online, public market, authoritative deployment, cutover, rollback and commercial release remain denied without independent external evidence.

## Repository-actionable closure status

Sequence 51 contains repository-side implementations for:

- complete 18-member module documentation and source-fact validation;
- explicit seven-component external authority/integration catalog;
- ADR-004 external-Agent-only runtime enforcement;
- strict Capability registry validation and truthful readiness;
- removal of executable local Ollama/OpenClaw inference from the Execution provider source;
- historical provider terminal-evidence and reconciliation preservation;
- architecture source-to-test-to-gate traceability;
- existing exact Ledger, Gateway, Execution settlement, Audit, TRNM and Hepta durability controls;
- exact-byte external-evidence intake isolation and anti-self-certification.

These changes are a repository candidate only until all authoritative workflows execute real non-empty jobs and succeed on one unchanged exact head and the generated candidate manifest validates that head/tree.

## Independent blockers

The following cannot be closed by another source commit or repository-owned pseudo-evidence:

1. runner allocation and non-empty exact-SHA hosted execution;
2. enforceable protected-main/ruleset governance with all required contexts and no administrator bypass;
3. independent approval by a reviewer other than the final pusher on the unchanged candidate;
4. downstream World/Game immutable revision binding and zero unexplained receipt divergence;
5. V12-X1 representative-volume disaster recovery;
6. V12-X2 real deployment/cutover/rollback;
7. V12-X3 real external Agent/provider reconciliation;
8. V12-X4 credential custody, separation, rotation and revocation;
9. V12-X5 sustained production-like soak;
10. V12-X6 independent security, operations and financial-control review;
11. V12-X7 legal, commercial and provider approval;
12. V12-X8 final human go/no-go bound to the exact qualified candidate.

Open issues #17 and #20 remain the operational tracking surfaces for these independent authorities. Their existence is not closure evidence.

## Change protocol

Any source, workflow, migration, security, test, module contract, external-component contract, authority, traceability or normative-document change creates a new candidate tree and invalidates earlier exact-SHA evidence. Update the shared trigger once, run all authoritative contexts on the unchanged final head, generate a new immutable candidate manifest and retain `production_authorization=not_granted` unless the independent final decision explicitly grants activation.
