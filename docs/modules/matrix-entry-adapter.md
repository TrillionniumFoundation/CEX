# matrix-entry-adapter module contract

Status: active module contract  
Workspace member: `services/matrix-entry-adapter`  
Package: `matrix-entry-adapter`  
Kind: `adapter-service`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `matrix-integration`  
Production authorization: `not_granted`

This contract describes source and required verification, not hosted qualification.
The module catalog is `docs/module-catalog-v1.json`. The deployment remains Alpha
until end-to-end recovery and independent operational evidence are accepted.

## Purpose and non-goals

The adapter validates authenticated Matrix ingress, normalizes supported events,
and forwards bounded consumer requests. It does not authorize research, value,
World/Game state or Chain finality. Matrix sender/room fields are source claims,
not CEX identity. The adapter does not host Agents or run model inference.

## Authority and owned state

The adapter owns transport normalization and local delivery observations only.
Its current library retains local recent-event/rate-limit caches and optional
file persistence. Those caches do not establish durable multi-instance exactly-once
acceptance. Consumer Entry remains responsible for business idempotency and identity.
The shared PostgreSQL transport schema lives here, but the poller owns cursor
leases/admission and the relay owns outbox dispatch; schema location does not
transfer these process responsibilities to the adapter.

## Source layout and entry points

Catalog-bound files:

- `services/matrix-entry-adapter/src/main.rs`: synchronous profile validation,
  followed by Tokio construction, state validation and listener startup.
- `services/matrix-entry-adapter/src/lib.rs`: existing routes, normalization,
  credential selection, local caches, forwarding and regression tests.
- `services/matrix-entry-adapter/src/runtime_profile.rs`: compatibility import
  for shared-config profile parsing, conflict rejection and legacy mapping.
- `services/matrix-entry-adapter/migrations/0001_transport_durability.sql`:
  existing inbox/outbox/cursor, fenced claims and poison observations.
- `services/matrix-entry-adapter/migrations/0002_source_observation_replay.sql`:
  additive source-observation history and cursor-independent exact replay.

New process targets and persistence boundaries require same-change catalog and
contract updates. Splitting the large library into bounded internal modules must
preserve route, principal, replay and privacy behavior.

- `services/matrix-entry-adapter/migrations/0003_sync_recovery_and_send_receipts.sql`:
  fenced renewal, recoverable poison snapshots, guarded cursor history, immutable
  Matrix credential-scope bindings and send receipts; expired adapter claims are held.

## Interfaces and contracts

The existing `/v1/matrix/events` boundary and consumer-forwarding behavior remain
unchanged by the startup repair. The ingress credential authenticates transport;
it does not authorize caller-supplied subject or tenant fields. Downstream calls
must carry stable event and scoped idempotency identity. A transport success is
not proof of a research, account, settlement or finality transition.

The replay SQL interface remains
`cex_matrix_accept_source_event_v1(text,text,text,text) -> text`.
It returns `accepted` for first admission and `replay` for the same event, content
hash and stream partition. A different content hash or partition is a collision.
A later opaque cursor records another observation, not another source identity.
The first inbox row is not rewritten. NULL cursor repeats are deduplicated too.

## Persistence, concurrency, and recovery

Apply migration 0001 followed by additive 0002, 0003, 0004 and 0005 under the schema owner. Migration
0002 backfills first observations without modifying the immutable inbox, preserves
the four-argument function signature and appends new observations transactionally.
It does not change the numbered Ledger migration head 0088. Reapplying 0002 is
idempotent; deploying only 0001 after 0002 reinstalls old replay semantics and is
not a valid upgrade or rollback procedure.

As with the existing outbox, the admission procedure establishes source linkage;
there is no cascading delete. Source observations reject UPDATE and DELETE.
Runtime identities must not receive TRUNCATE, DDL or unrestricted repair authority.
The existing invoker function needs its reviewed table/sequence permissions; merely
granting EXECUTE is insufficient. Do not solve permission errors by using an owner
role, granting superuser, or introducing an unreviewed SECURITY DEFINER function.

Local adapter cache persistence is not a substitute for the shared transactional
transport. Full adapter response-loss replay and multi-instance business-effect
tests remain mandatory before production promotion.

## Configuration and secrets

The executable checks every explicitly present value of
`MATRIX_ENTRY_RUNTIME_PROFILE`, `CEX_RUNTIME_PROFILE`, and `APP_ENV` before tracing,
Tokio, asynchronous state construction or listener startup. Accepted aliases are:

| Group | Accepted values | Legacy adapter policy |
|---|---|---|
| Local | `test`, `local`, `local_dev`, `dev`, `development` | `local_dev` |
| Beta | `beta` | `beta` |
| Staging | `stage`, `staging` | `production` |
| Production | `prod`, `production`, `trnm-economy`, `trnm_economy` | `production` |

Whitespace/case normalize. Unknown, explicitly empty, non-Unicode and conflicting
sources fail with exit 78. All sources absent means local development. Staging and
production remain distinct selections, so their simultaneous explicit use is a
conflict even though both enforce the legacy production policy. A local override
cannot lower an explicitly configured global production policy.

Only the adapter-specific environment value is normalized before threads exist.
The library's legacy parser is not a public strict configuration API: embedded
callers must supply validated configuration. The deployable binaries now share
one parser, but enforcing typed configuration in embedded library callers remains
a separate, uncompleted refactor. Existing ingress/session secrets, approved registry
revisions, downstream endpoints and local-cache settings retain their existing
validation requirements. Secrets and opaque cursors must not enter diagnostics.

## Security and trust boundaries

Unknown runtime configuration cannot select local authority in the deployable
binary. This repair does not establish key custody, secret strength or permission
separation for a real deployment. Validate verified ingress identity, tenant
mapping, bounded bodies, redirects, edit/redaction policy and downstream receipts.
Do not expose raw messages, tokens, database URLs or high-cardinality identities in
logs or metrics. An operator poison acknowledgement does not prove reprocessing.

## Verification

Required commands:

```text
cargo test -p matrix-entry-adapter
cargo clippy -p matrix-entry-adapter --all-targets -- -D warnings
bash scripts/check-matrix-source-observation-postgres.sh
```

The database command requires a disposable PostgreSQL 16 database supplied by
`MATRIX_TEST_DATABASE_URL` and explicit `MATRIX_TEST_ALLOW_SCHEMA_RESET=1`. It runs
the complete 0001/0002/0003/0004/0005 chain twice before the original transport assertions, then
checks different-cursor exact replay, NULL cursor deduplication, content/partition
collisions, first-observation preservation and immutable observation history.
Missing tooling, credentials or reset consent fails; tests are not silently skipped.

Rust tests cover strict profile parsing and conflict behavior. Hosted black-box
startup checks must additionally verify rejection before a listener is available.
Source inspections and local Python tests are not Rust/PostgreSQL execution proof.

## Deployment and operations

Keep the adapter private and run with a least-privilege identity. Readiness must
truthfully distinguish its local cache mode from durable poller/relay state.
Record exact image, schema chain, validated profile, credential identities, cache
mode and consumer endpoint. Monitor failed ingress, delivery lag, duplicates and
poison observations. Real homeserver and restart tests remain required.

Rollback stops new claims/admission before changing binaries. Preserve inbox,
observations, outbox and poison history. Do not drop migration-0002 data or rerun
0001 alone; use a reviewed forward migration for a required semantic reversal.
Restoring an older image cannot authorize loss or recreation of business effects.

## Compatibility and change protocol

Cursor changes no longer create content collisions, but partition and content
identity remain strict. Event normalization or partition changes need explicit
versioning, replay fixtures and retirement rules, not rewriting old inbox rows.
Update this contract, catalog, Matrix design, tests and exact candidate evidence
for source changes. No module document grants repository or production approval.

## Recovery and receipt extension

The detailed extension is `docs/matrix-recovery-and-receipt-contract-v2.md`.
A poison acknowledgement explicitly authorizes quarantine of those exact bytes;
it does not claim successful processing, create a command or advance a cursor.
The ordinary fenced batch operation advances only after all relevant observations
are acknowledged. Evidence payloads remain in restricted database custody.
Migration 0003 also prevents new Matrix `sent` transitions without a validated,
matching receipt and prevents unsafe automatic reclamation of expired adapter
claims. Current library caches still do not prove durable business-effect replay.

The database wrapper invokes `scripts/matrix_postgres_regression.py`. Dedicated
test names and explicit reset consent are required; server/database identity and
unrelated-table checks precede every destructive test reset. Both shell entrypoints delegate to the same guarded runner. The original SQL
assertion body lives unchanged in `scripts/test-matrix-transport-baseline.sql`
and runs against the complete current schema in the same database session. `python3 scripts/test-matrix-postgres-runner.py` validates
orchestration using a fake client; it is not database execution evidence.

## Stream-scope migration extension

`services/matrix-entry-adapter/migrations/0004_stream_scope_binding.sql` adds
immutable poller stream metadata, virgin-stream binding and owner-only legacy
scope approval. It changes no adapter business authority or source identities.
Apply the complete current migrations before starting the updated poller; existing streams
will hold until their exact cursor and prior configuration are reviewed. See
`docs/matrix-stream-scope-v1.md`. The SQL runner now uses the complete five-step
chain for both migration replays and all original/new assertions. Runtime
privilege deployment and real PostgreSQL validation remain mandatory.


## Single-session SQL verification contract

Both `bash scripts/check-matrix-transport-postgres.sh` and the catalog-bound
`bash scripts/check-matrix-source-observation-postgres.sh` now use one runner.
Neither can execute a 0001-only upgrade or reset without explicit consent. The
original 10,870-byte SQL body is retained unchanged in the dedicated baseline.

The runner verifies PostgreSQL 16 and the exact test database, acquires a
session-scoped advisory lock, rejects unrelated table/view/foreign-table names,
and uses a fixed twelve-table reset allowlist before both complete migration
replays and all SQL suites. Successful reports require a zero client exit and
all ordered post-SQL markers. No per-step subprocess exit code is fabricated.
The source snapshot is rechecked after the session. See
`docs/matrix-sql-runner-v4.md` for the deliberately versioned v4 evidence shape,
output-path restrictions, process limits and remaining execution qualification.

`python3 scripts/test-matrix-runner-hardening.py` exercises actual subprocesses
and filesystem operations with fake psql, not PostgreSQL. Database permissions,
advisory-lock contention and the SQL suite still require real PostgreSQL 16.

## ID filter definition persistence

`services/matrix-entry-adapter/migrations/0005_filter_definition_pins.sql` adds
immutable exact filter bytes/digest under the original stream ID. Bootstrap is
virgin-stream-only; reviewed legacy insertion requires the table owner, exact
cursor/revision and no live lease. No scope/cursor is rewritten. Updated pollers
resolve ID filters before ordinary sync and use the validated inline snapshot
for both sync and recovery. See `docs/matrix-filter-definition-v1.md`.
The SQL runner includes all five migrations and the new pin suite, preserving
all previous assertion bytes. Runtime permissions and actual SQL execution are
still required, not granted by this supporting contract.


## SQL test-process lifecycle

`python3 scripts/test-matrix-runner-lifecycle.py` exercises actual POSIX parent
and descendant processes and the SQL input policy; it does not execute PostgreSQL.
The shared runner now rejects inline psql controls under a deliberate
no-backslash/no-NUL source profile and stops its owned process group even after
a successful client exit, before reaping its leader. Native Windows execution of
this SQL runner is unsupported and fails before launching a client. The current
Linux SQL jobs and full migration/assertion requirements remain unchanged. See
`docs/matrix-sql-runner-v4.md` for limits and pending actual database qualification.

## Shared profile linkage (round 15)

This package now depends on the existing local `shared-config` crate, and the
profile compatibility file re-exports its resolver without any local policy.
The three explicit environment sources, non-Unicode rejection and startup order
are unchanged. `Cargo.lock` adds only that existing local direct edge. No registry
package/version/checksum or protocol/schema change is part of this refactor.
The original semantic tests are retained once in shared-config and must execute
there; testing only this dependent package does not run dependency unit tests.
See `docs/matrix-profile-sharing-v1.md`. Actual locked resolution, compilation,
formatting, lint and black-box startup verification remain required.
