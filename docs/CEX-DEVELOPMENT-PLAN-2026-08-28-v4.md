# CEX Development Plan v4 — Canonical Migration Baseline and Existing-Row Audit Backfill

- Status: Active
- Date: 2026-08-28
- Integration branch: `feature/hepta-production-baseline-p0`
- Fact baseline before this revision: `3faa798a0abb7ad8fb525f27ab6a45a608b62819`
- Canonical migration head before P0-N0: `0063_add_identity_transactional_audit_outbox.sql`
- Release posture: Draft integration candidate; not production-ready

## 1. Why v4 supersedes v3

A repository fact check found three duplicate migration numbers:

- two `0060` migrations;
- two `0061` migrations;
- two `0062` migrations.

The migration governance gate rejects duplicate numbers, and the release manifest pointed at
an obsolete `0062` Identity filename. Therefore the branch could not be treated as a valid
migration chain even though the individual source slices existed.

v4 restores a single append-only chain:

1. `0060_close_audit_outbox_delivery_lifecycle.sql`
2. `0061_close_audit_outbox_delivery_transitions.sql`
3. `0062_add_execution_transactional_audit_outbox.sql`
4. `0063_add_identity_transactional_audit_outbox.sql`

The superseded duplicate files are removed rather than silently retained. The static gate now
fails when any obsolete path returns.

## 2. Current repository truth

Implemented candidates:

- explicit production-like runtime guard;
- internal workload identity;
- authenticated Audit v2 append and hash chain;
- durable Audit outbox claim, verified ACK, retry and dead-letter lifecycle;
- dedicated authenticated dispatcher;
- Execution same-transaction Audit intent for new/changed rows;
- Identity API-key same-transaction Audit intent for new/security-mutated rows;
- Money v2 expand/dual-write columns and consistency triggers;
- durable Saga shadow schema.

Pending machine evidence:

- GitHub-hosted or self-hosted required checks;
- fresh and supported-version upgrade migrations;
- dispatcher outage/ACK-crash fault tests;
- workload-token rotation/revocation;
- full workspace clippy/test/build;
- backup/restore and signed release evidence.

PR #1 remains Draft and unmerged until these checks actually execute.

## 3. P0-N0 — Existing-row Audit baseline

This is the next locked implementation slice.

### Goal

Existing `executions` and `api_keys` rows created before migrations 0062/0063 have
`audit_revision = 0` and no durable baseline intent. Production promotion is blocked until
they are processed or covered by a reviewed waiver.

### Required design

- do not bulk-enqueue source history inside a schema migration;
- use bounded batches;
- serialize one worker per source;
- make each source-row revision update and outbox intent one PostgreSQL transaction;
- retain deterministic event identity across restart;
- select only revision-0 rows;
- persist progress and last processed source id;
- pause when nonterminal Audit outbox backlog exceeds a configured ceiling;
- expose remaining counts and blocked/completed status;
- never copy provider output, raw API keys or key hashes into Audit payloads.

### Durable baseline event types

- `execution.persisted.baseline`
- `identity.api_key.persisted.baseline`

### Acceptance

- exact restart/replay does not duplicate an intent;
- a failed batch advances neither revision nor progress;
- source revision and intent commit or roll back together;
- `last_used_at` telemetry remains excluded;
- residual revision-0 count reaches zero or has an explicit reviewed waiver;
- outbox backlog remains within the rollout limit;
- fresh/upgrade probes cover partial progress, restart and completion.

## 4. P0-N1 — Ledger operation identity

Begins only after N0 is an implemented candidate.

Every financial effect must gain immutable:

- `trace_id`;
- `operation_id`;
- operation kind;
- idempotency scope and key;
- source service/principal;
- schema version.

Legacy rows receive explicitly labelled entry-scoped identities; migration must not pretend
that arbitrary historical text is a trustworthy cross-service trace.

Acceptance:

- one operation identity maps to one immutable effect;
- exact replay returns the original effect/receipt;
- collision with different immutable content fails closed;
- entries are queryable by trace and operation;
- new production writes cannot omit operation provenance.

## 5. P0-N2 — Genesis-as-entry

Nonzero account creation must create an immutable genesis entry in the same transaction.
Account summaries are projections, not independent value authorities.

Acceptance:

- no implicit nonzero balance without a genesis entry;
- zero-balance account creation remains valid;
- existing accounts receive reconciliation evidence rather than fabricated history;
- summary rebuild from entries is demonstrable;
- direct summary mutation is denied outside controlled repair.

## 6. P0-N3 — Money v2 read cutover

1. inventory every value-bearing `f64` and numeric conversion;
2. backfill exact minor units;
3. dual-read and compare;
4. publish mismatch metrics;
5. switch authoritative reads to minor units;
6. remove binary floating point from value-bearing contracts and repositories;
7. contract legacy columns only after rollback evidence.

## 7. P0-N4 — Projection reconciliation

Deliver:

- deterministic rebuild;
- drift report;
- dry-run repair plan;
- scoped operator approval;
- immutable before/after repair evidence;
- recurring drift metrics.

## 8. P0-N5 — Network calls outside SQL transactions

Provider and ledger HTTP calls must follow:

```text
short transaction persists command
worker claims and commits claim
network side effect
receipt persists
short transaction advances state
```

Unknown remote outcomes become `reconcile_required`; they are not blindly retried.

## 9. P0-N6 — Saga qualification

Before shadow promotion, execute crash/replay/concurrency/model tests for:

- claim crash;
- provider success then receipt failure;
- duplicate worker;
- lease expiry;
- consume/refund unknown result;
- database failover;
- recovery and reconciliation.

The supported guarantee is at-least-once delivery with idempotent effects and explicit
reconciliation, not an unsupported exactly-once claim.

## 10. Required gates

Fast:

- migration numbering and canonical-path check;
- release-manifest head parity;
- static route/config/source wiring;
- `cargo fmt`;
- clippy with warnings denied;
- unit/property tests.

Persistence:

- fresh migrations;
- supported upgrade matrix;
- source/outbox rollback atomicity;
- baseline partial/restart/completion;
- concurrent claim/replay;
- backup/restore and chain verification.

Integration/fault:

- authenticated dispatcher;
- token rotation;
- Audit outage/recovery;
- remote append success/local ACK crash;
- wrong worker/expired lease;
- source mutation rollback;
- provider/ledger Saga crash matrix.

## 11. Merge and release rules

`Source present` means implementation candidate only.

A release candidate must bind exact commit/tree, Cargo.lock hash, canonical full migration
filename, executed workflow/job IDs, test/fault artifacts, image digest, SBOM/provenance,
deployment profile, approvals and revocation metadata.

## 12. Next locked slice

`P0-N0 Existing-row Audit baseline` is the only next authoritative implementation slice.
