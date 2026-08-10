# Hepta Paper Raid operations v1

## Scope

This runbook defines the alpha-candidate operational contract for Hepta and the
Paper Raid BFF. It does not make Nakama or TRNM facts on their behalf. A green
source gate is not a substitute for a four-component black-box run, a 24-hour
soak, or verified Chain finality evidence.

## Runtime signals

Scrape Hepta `GET /metrics` and require `GET /ready` to remain HTTP 200. The
Paper Raid gauges are intentionally low-cardinality:

- `hepta_paper_raid_papers_total`
- `hepta_paper_raid_active_authorization_epochs`
- `hepta_paper_raid_pending_control_commands`
- `hepta_paper_raid_oldest_pending_control_seconds`
- `hepta_paper_raid_max_pending_control_attempts`
- `hepta_paper_raid_pending_outbox_events`
- `hepta_paper_raid_oldest_pending_outbox_seconds`
- `hepta_paper_raid_max_pending_outbox_attempts`
- `hepta_paper_raid_idempotency_records`
- `hepta_paper_raid_storage_backend_info{backend="postgres"}`

Every HTTP completion emits one structured `hepta_http` event with a generated
request ID, method, matched route template, status, and latency. Request bodies,
credentials, raw research content, and concrete user/session identifiers are
never tracing fields.

## Alert baseline

- Critical: `/ready` is non-200 for 2 minutes, or the active storage backend is
  not PostgreSQL in a non-development environment.
- High: `hepta_paper_raid_oldest_pending_control_seconds > 600`, pending control
  depth grows across three consecutive scrapes, or maximum pending attempts
  exceeds 3.
- High: `hepta_paper_raid_oldest_pending_outbox_seconds > 600`, pending outbox
  depth grows across three consecutive scrapes, or maximum pending attempts
  exceeds 3.
- High: any `hepta_http` 5xx ratio exceeds 1% over 10 minutes. Page immediately
  if the affected route is a consent, finalize, completion, or appeal write.
- Warning: p95 exceeds 750 ms or p99 exceeds 1.5 s for 15 minutes, excluding
  health, readiness, and metrics routes.
- Warning: idempotency-record growth diverges from paper/project growth for 24
  hours; inspect abuse, retries, and retention before deleting anything.

## Retention and archive

- Paper, consent, review, reproduction, appeal, completion receipt, inbox, and
  finality evidence are audit records. Retain them for the product/legal policy
  period; alpha must not delete them automatically.
- Idempotency records must outlive the associated aggregate and every client
  retry window. Archive only completed records after a verified backup and a
  restore drill prove exact replay remains possible.
- Nakama control commands remain online through session completion plus the
  appeal window. Applied commands may then move to immutable archive storage;
  pending commands are never archived or deleted.
- Delivered outbox rows may move to immutable archive after consumer inbox
  reconciliation. Undelivered, leased, or failed rows are never bulk-cleared.
- Raw papers, private datasets, prompts, and model outputs must not enter TRNM,
  metrics, traces, or general-purpose logs.

## Recovery acceptance

Before reopening writes after PostgreSQL restore, compare project counts,
active authorization epochs, pending controls, pending outbox rows, inbox keys,
idempotency records, Nakama completion roots, and TRNM receipt projections.
Then replay expired leases through the normal worker path. See
`docs/hepta-failure-recovery-runbook.md` for the fail-closed invariants.

The BFF restart/revocation gate is:

```bash
bash services/paper-raid-bff/scripts/check-postgres.sh
```

The Hepta database crash/recovery gate is part of:

```bash
bash scripts/check-hepta-research-league-release.sh
```

## Scale and soak evidence

The source suite must cover three- and five-author flows, concurrent finalize,
multi-instance serialization, idempotent replay, control recovery, and outbox
lease recovery. The alpha-candidate environment must additionally run a real
Hepta+BFF+Nakama+CAS black-box flow under concurrent users for 24 hours.

Record, at minimum, commit/image digests, component configuration hashes,
start/end timestamps, request totals, error ratio, p50/p95/p99, peak database
connections, table/index sizes, pending-control high-water mark, pending-outbox
high-water mark, restart timestamps, and recovery duration. A shortened local
run may debug the harness but cannot satisfy the 24-hour evidence requirement.

## Finality boundary

Paper finality begins as `pending_finality`. The canonical Chain path now
supports typed Paper-bound ingress and locally verified Receipt V2 evidence;
only that exact, trust-anchor-pinned path may advance a Paper to
`verified_finality`. A BFF label, mock receipt, local fallback or status rewrite
must never advance it. Verified finality also does not imply ranking, score,
reward or economic eligibility: those four gates remain independently false
until their dedicated release policies are satisfied. Integration remains
`runnable=false` while the current candidate lock or its terminal soak evidence
is blocked, regardless of an individual Paper's finality.
