# CEX Development Plan — 2026-08-28 v12

## Status and authority

- Status: active all-blocker closure candidate; **not production-ready**.
- Supersedes: `CEX-DEVELOPMENT-PLAN-2026-08-28-v11.md` for the exact-money, durable-settlement, provider-dispatch and release-evidence workstream.
- Candidate migration head: `0088_enforce_provider_terminal_evidence_binding.sql`.
- Default rollout posture: shadow or fail-closed; no production cutover is authorized by this document.
- Acceptance rule: source presence, a static marker, a template manifest or an unexecuted script is not evidence. A gap closes only when the exact commit/tree has the required hosted execution evidence and the evidence is bound to the release candidate.

## 1. Objective

Close every repository-actionable gap between the legacy compatibility control plane and an exact,
auditable, recoverable Ledger authority without converting, rounding or inferring historical money.
The target state is:

1. exact minor units are authoritative for every new value-bearing operation;
2. Invocation, Gateway, Execution, Ledger, Audit and provider evidence share stable operation identity;
3. all remote side effects are driven by durable commands whose claim transaction commits before network I/O;
4. ambiguous outcomes fail to `reconcile_required`, never to an invented success or failure;
5. restart, replay, lease expiry, retry exhaustion and operator recovery are executable and independently observable;
6. compatibility writes are retired or explicitly break-glass governed;
7. release qualification is based on immutable evidence, not prose or mutable CI status alone.

## 2. Candidate scope

### P0-N5 durable Execution settlement

Migrations `0067`–`0072` provide immutable settlement commands, transition history, bounded claims,
verified Ledger receipts, exact replay, retry/dead-letter semantics and operator recovery.
Execution terminal transitions enqueue consume/refund settlement durably rather than performing a
remote Ledger call inside the business transaction.

### P0-N6 exact Gateway reserve

**P0-N6 delivered by this candidate** means the repository contains the exact reserve registration
contract, durable reserve command, fail-closed worker, receipt validation, operator recovery and
hosted executable gate. It does not mean production cutover has happened.

The Gateway exact path accepts canonical string minor units, rejects dual exact/legacy monetary
intent and binds one immutable Invocation Ledger contract to one reserve operation. Claims and
outcomes run in separate transactions around the Ledger v2 network call.

The caller migration is blocked until the exact reserve contract is authoritative (machine-readable
marker: `caller migration blocked until exact reserve contract`). The canonical
`POST /v1/invocations` path therefore rejects legacy `reserve_amount: f64` before authentication,
persistence or upstream calls; only the explicit non-production break-glass compatibility switch
may invoke the retired v1 reserve/refund methods.

### Genesis-as-entry and exact account authority

**Genesis-as-entry** is the only supported non-zero account opening model. The account is opened
through `cex_open_account_v2`; the opening value is represented by the first append-only Ledger
entry and bound to an immutable opening contract. Direct non-zero account insertion fails closed.

Migrations `0075`–`0077` add exact opening, signed inventory, projection parity, deterministic repair
evidence and append-only controls. HTTP regression coverage uses `/v2/accounts` and
`/v2/ledger/effects`; retired v1 monetary routes are asserted to return `410 Gone`.

### Provider-dispatch authority and unknown outcomes

Migrations `0078`, `0081`, `0082`, `0084` and `0088` separate provider dispatch from Execution state
mutation, require exact authority, classify lease expiry after dispatch as an unknown outcome,
require immutable reconciliation artifacts and permit requeue only after fresh
confirmed-not-executed evidence plus acknowledgement. Migration `0084` keeps exact evidence replay
side-effect-free after confirmed execution has already made the command terminal. Migration `0088`
makes provider-command identity and terminal authority database invariants while preserving two
non-interchangeable evidence paths: a live `claimed -> succeeded` transition must bind the upstream
Ollama model, explicit `done=true`, non-empty output and canonical payload hash to the immutable
target; a `reconcile_required/dead_letter -> succeeded` transition must bind the exact result and
hash to the same-attempt append-only `confirmed_executed` artifact. Neither path may impersonate
the other.

### Audit and release evidence

Migrations `0059`–`0065`, `0080` and `0083` provide authenticated Audit v2, durable outbox delivery,
source baseline backfill, operation provenance, append-only checkpoints and release evidence.
`0083` makes the bounded baseline cursor executable on PostgreSQL by replacing unsupported UUID
aggregate usage while preserving an additive `0082 -> 0083` upgrade path.

## 3. Non-negotiable invariants

1. No `f32`/`f64`, decimal rounding, truncation or major-unit compatibility conversion is permitted in a new authoritative money write path.
2. Every exact effect carries account, organization/tenancy, trace, operation, currency unit, currency scale, signed minor-unit amount, reference and scoped idempotency identity.
3. Identical immutable input replays the original result; different content under the same identity collides.
4. No SQL transaction remains open while calling Ledger, a provider or another remote service.
5. A claim lease that may have crossed a remote side-effect boundary never implies automatic retry.
6. Append-only evidence cannot be updated or deleted through normal service roles.
7. New writes may not use compatibility-only Ledger columns or retired v1 monetary endpoints.
8. A release candidate cannot be qualified from the template manifest; all hashes, URIs, run IDs, artifacts and approvals in the generated candidate manifest must be real and verified.
9. Repository qualification is not production authorization. Automation may approve only the repository-candidate scope and may never impersonate security, operations, financial, legal or human go/no-go approval.

## 4. Repository-actionable closure blocks

### Block A — migration correctness and upgrade safety

Required evidence:

- numbered migration sequence is contiguous and the release template points at the actual head;
- all P0 migrations are transactional and functions pin `search_path`;
- fresh PostgreSQL 16 apply succeeds through the current head;
- representative existing-row upgrade paths execute, including Audit baseline, Ledger operation identity and Invocation exact contract lifecycle;
- views preserve existing column names/order unless an explicit compatible migration is used;
- function replacement preserves signatures and input parameter names where PostgreSQL requires it;
- migration scripts exercise current exact APIs rather than bypassing closed guards.

Closure gate: `.github/workflows/p0-migration-gate.yml` must complete successfully on the exact
candidate commit.

### Block B — cross-platform Rust and API regression

Required evidence:

- `cargo fmt --all --check`;
- Linux and Windows service-local tests;
- all-target workspace compile;
- exact Ledger account/effect lifecycle, replay and insufficient-funds recovery;
- retired v1 value routes fail closed without in-memory mutation;
- no placeholder repository mode silently accepts value writes.

Closure gate: `.github/workflows/rust-service-gate.yml` must complete successfully on the exact
candidate commit.

### Block C — Gateway exact reserve

Required evidence:

- static architecture and legacy-money exclusion;
- canonical Invocation legacy-reserve fail-closed guard and exact-ingress migration evidence;
- Gateway package tests and clippy with warnings denied;
- fresh migration lifecycle;
- Invocation exact contract lifecycle;
- Gateway command shadow exclusion, claim ownership, lease expiry, retry exhaustion, receipt
  validation, acknowledgement and exact requeue lifecycle.

Closure gate: `.github/workflows/p0-gateway-exact-reserve-gate.yml` must complete successfully on
the exact candidate commit.

### Block D — Execution terminal settlement

Required evidence:

- static architecture and no network-in-transaction path;
- Execution package tests and all-target compile;
- fresh migrations plus Invocation lifecycle and terminal mutual exclusion;
- settlement command claim, consume/refund success, exact replay, retry, dead-letter,
  acknowledgement, requeue and crash/lease recovery.

Closure gate: `.github/workflows/p0-execution-settlement-gate.yml` must complete successfully on
the exact candidate commit.

### Block E — provider reconciliation

Required evidence:

- provider dispatch authority is bound to the exact Invocation/Execution contract;
- transport ambiguity and expired active claims become `reconcile_required`;
- reconciliation artifacts have immutable URI and SHA-256 evidence;
- confirmed execution closes the command exactly once and exact replay remains terminal-safe;
- confirmed-not-executed evidence is required before requeue;
- indeterminate evidence cannot authorize retry;
- transition and reconciliation evidence are append-only;
- live provider-reported model identity equals the immutable provider reference;
- live success requires explicit terminal/output/hash evidence from a claimed command;
- reconciled success requires a same-attempt append-only `confirmed_executed` artifact whose
  exact result payload and hash become the terminal command evidence;
- a live envelope cannot authorize reconciliation and operator evidence cannot replace live
  provider identity on the automatic worker path;
- immutable provider-command fields and terminal evidence cannot be rewritten;
- exact hosted job evidence proves the terminal-success static contract step executed.

Closure gate: `.github/workflows/p0-provider-reconciliation-gate.yml` must execute the hosted
PostgreSQL lifecycle on the exact candidate commit. Static SQL markers alone are not sufficient.

### Block F — Audit baseline and delivery

Required evidence:

- authenticated Audit v2 append and exact replay;
- outbox enqueue collision, bounded claim, retry and verified acknowledgement;
- existing Execution and API-key rows backfill in bounded batches;
- baseline restart is a no-op after completion and never duplicates intents;
- backlog pressure blocks the baseline rather than overrunning delivery capacity;
- UUID source cursors execute on PostgreSQL without lossy conversion;
- source revision advancement and Audit intent creation are atomic.

### Block G — release evidence hygiene

Before repository qualification:

- a clean checkout is byte-clean under `.gitattributes`; LF-governed tracked blobs are normalized so checkout clean filters do not mutate candidate bytes;
- temporary self-patch workflows, patch scripts and CI trigger files are absent;
- the five authoritative workflows listen to `docs/release-evidence/p0-candidate-trigger.json`;
- third-party Actions in those workflows and the candidate workflow are commit-pinned;
- a bounded committed exact-money soak executes before a custom-format dump/restore drill;
- the restored database matches source row counts, exact balances, operation identity and content hashes;
- the generated non-template candidate manifest binds the exact commit/tree, Cargo.lock digest,
  migration head and chain digest, five authoritative workflow run IDs, evidence payload digest,
  SPDX SBOM and in-toto/SLSA-style provenance;
- the manifest records automation approval only for repository qualification and explicitly denies
  production authorization.

Closure gate: `.github/workflows/p0-release-candidate-gate.yml` reruns the complete database
lifecycle matrix, waits for the five exact-SHA authoritative gates, uploads the immutable evidence
payload and then validates the generated candidate manifest.

Branch/ruleset enforcement is a repository-administration control. The candidate evidence must
report its actual state; absence of enforcement may not be hidden by source code or CI prose.

## 5. External gates that repository edits cannot self-certify

These remain blockers until their independent evidence exists. They must not be relabelled as
closed merely because code or hosted CI is green.

1. production-like backup and restore rehearsal against representative data volume and the real storage topology;
2. deployment/cutover and rollback rehearsal with real service identities, network policy and secret custody;
3. real provider reconciliation artifacts for success, definite non-execution and indeterminate outcome;
4. credential issuance, rotation, revocation and break-glass custody review;
5. sustained production-like soak/endurance run with queue age, retry, reconciliation, parity and Audit delivery SLOs;
6. independent security, operations and financial-control review;
7. legal, commercial or provider approvals where the production integration requires them;
8. final human go/no-go decision bound to the immutable release candidate.

The bounded CI exact-state dump/restore and exact Ledger soak close repository regression gaps; they
do not replace representative-volume disaster recovery or sustained production qualification.

## 6. Evidence ledger protocol

The static plan defines required evidence but does not embed candidate run IDs. Writing run IDs back
into this file would create a new commit and invalidate the very exact-SHA evidence being cited.
Instead, `scripts/p0-release-evidence-strict.py` (backed by the immutable
`scripts/p0-release-evidence-core.py` collector) produces the evidence ledger and candidate manifest
inside the `p0-release-candidate-gate` run. The manifest must contain the final run IDs, exact
attempt/job execution attestations, conclusions and hashes and must pass
`scripts/check-release-baseline-manifest.py` without template mode.
The strict contract also binds each hosted URI to the collector's final run/context snapshot,
binds every local evidence digest to the payload index, and re-hashes the payload directory at
manifest time. A swapped run ID, stale attestation digest, post-upload file mutation, missing
required field or stale candidate-trigger freeze therefore fails closed.

Hosted selection is a single bounded collect pass owned by
`scripts/check-hosted-gate-execution.py`: it performs the run-list/detail/job validation and emits
the authoritative frozen run/attempt snapshot. The strict collector injects that snapshot into the
core in-process, so the core cannot perform a second mutable API selection. The exact-attempt
verifier and hosted-job attestation consume the same frozen IDs, while
`scripts/verify-hosted-snapshot-freshness.py` revalidates the set after job proof, before manifest
generation, and after manifest publication; no newer success, failure, cancellation, skip, or
active rerun can be silently substituted.

The aggregate governance record must contain all nine required contexts, including repository
integrity and the full Hepta PostgreSQL recovery lane. Exact-state backup/restore uses complete
host PostgreSQL tooling when available and otherwise a credential-safe Docker client fallback with
an explicitly proven local port mapping; database URLs are never exposed in process arguments.

The aggregate workflow may also create transient diagnostics under
`run/p0-release-support` (for example, a TRNM receipt-lookup response). Those files are
deliberately outside the closed upload namespace and are not qualification evidence. The
authoritative record for lifecycle checks is `database-lifecycle.json`, together with the
exact-SHA hosted job/step attestation; a support diagnostic must never be substituted for either.

| Evidence | Required state | Binding location |
|---|---|---|
| Fresh migration and existing-row upgrade paths | hosted success | exact-SHA migration gate plus evidence payload |
| Ledger operation identity/replay | hosted success | exact-SHA migration gate plus evidence payload |
| Invocation lifecycle/exclusivity | hosted success | exact-SHA migration gate plus evidence payload |
| Linux and Windows service-local gates | hosted success | exact-SHA Rust gate metadata |
| Gateway exact reserve | hosted success | exact-SHA Gateway gate metadata |
| Execution settlement | hosted success | exact-SHA Execution gate metadata |
| Provider unknown-outcome lifecycle | hosted success | exact-SHA provider gate metadata |
| Bounded exact Ledger soak | pass | evidence payload `exact-ledger-soak.json` |
| Exact-state backup/restore | pass | evidence payload `backup-restore.json` |
| SBOM and provenance | non-placeholder SHA-256 | evidence payload and candidate manifest |
| Representative-volume restore | independent external artifact | external production gate |
| Sustained soak/SLO qualification | independent external artifact | external production gate |
| Independent approvals | signed/recorded approval | external production gate |

## 7. Definition of repository closure

Repository-actionable gaps are closed only when all of the following are true on one final commit:

1. all five authoritative hosted workflows are green;
2. `p0-release-candidate-gate` is green on the same commit/tree;
3. no temporary patch/trigger artifact remains other than the permanent shared candidate trigger;
4. migration and manifest heads agree;
5. fresh and existing-row upgrade paths both execute;
6. exact replay, collision, crash/lease and operator-recovery branches execute;
7. compatibility writes and v1 value routes remain fail-closed;
8. the generated exact-tree evidence ledger names the final run IDs and conclusions;
9. the generated candidate manifest contains no placeholder and its evidence payload digest,
   SBOM and provenance all validate;
10. actual branch/ruleset enforcement state is reported without fabrication.

The shared `docs/release-evidence/p0-candidate-trigger.json` is the sole candidate-freeze authority;
its sequence and explicit `production_authorization=not_granted` are checked as a consistency
control, not an authorization to deploy. A secondary qualification-freeze marker is forbidden.

Even after repository closure, the release remains **not production-ready** until every external
gate in Section 5 is independently satisfied and the final go/no-go approval is recorded.
