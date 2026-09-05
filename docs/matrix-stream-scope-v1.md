# Matrix immutable stream scope and legacy-cursor upgrade

Status: unqualified source implementation; runtime validation required
Owner: matrix-integration
Parent: v12 implementation addendum, Blocks H and K; Matrix recovery contract v2
Production authorization: `not_granted`

## Boundary and problem

A durable cursor previously belonged only to `MATRIX_POLL_PARTITION_ID`. The
process could load a different homeserver, bot identity or sync filter while
reusing that partition's old opaque cursor. This implementation adds an immutable
scope assertion before the updated poller transmits a cursor or admits a batch.
It does not reconstruct inaccessible history or infer an approved scope change.

`apps/matrix-bot-poller/src/stream_scope.rs` describes the configured endpoint,
account and filter; `0004_stream_scope_binding.sql` records and enforces the
binding. No new dependency or change to the Ledger migration head 0088 is made.

## Account and scope validation

Before the polling loop, the process performs one authenticated Client-Server v3
`GET /account/whoami` call. The URL keeps the configured reverse-proxy prefix.
Redirects remain disabled. The complete response read has a 10-second deadline
and a 4,096-byte limit. HTTP 200, matching `user_id`, no error envelope and a
non-guest account are required. Missing `is_guest` means non-guest; a present
non-boolean or null value is rejected. The access token is not written to the
scope record. A failed lookup prevents polling, rather than assuming that the
configured bot ID is the token owner. Rate-limit/temporary failures require a
normal supervised restart; no successful identity is invented.

The scope JSON has exactly `schema`, `homeserver`, `bot_user_id` and `filter`.
The schema is `cex.matrix.stream-scope.v1`. Endpoint parsing uses the same URL
library as HTTP: host/default-port normalization is retained, trailing slashes
are removed as in URL construction, and the proxy path remains significant.
Embedded credentials, query strings, fragments and non-HTTP(S) schemes are
rejected. The account must have non-empty local/server components and no control
or whitespace characters.

Filter forms are distinct: no filter, a server-owned opaque filter ID, or an
inline JSON object. Inline filter bytes are retained exactly as transmitted by
the parsed configuration, even for whitespace/key-order-only edits. This
conservative rule avoids claiming two server inputs are equivalent based on a
second parser's duplicate-key or extension interpretation. IDs remain opaque.
Input bounds are 2,048 endpoint bytes, 512 account bytes, 4,096 inline filter
bytes or 1,024 filter-ID bytes. Full membership transitions and changes behind
an externally managed filter ID are not solved by a local binding.

Scope metadata may contain private room IDs or endpoint details. It belongs in
restricted database custody, not logs, metrics, repository commits or public
artifacts. The scope does not read credential configuration or retain messages.
Do not place credentials in filter or endpoint path configuration.

## Normal polling

After acquiring a cursor lease, the updated poller calls
`cex_matrix_bind_stream_scope_v1(text,text,bigint,bigint,jsonb)` in a separate
transaction. The function locks the cursor row and checks the unexpired owner,
fence and revision. That transaction commits before `/sync` or backfill HTTP.
A matching binding replays; a changed scope raises `matrix_stream_scope_mismatch`.
Unexpected database replies and failures are unverified holds, not success.

An absent binding can be created automatically only for a virgin partition:
revision zero, no opaque cursor, active lease, and no inbox, cursor-history or
poison records. The INSERT trigger also applies this rule to direct inserts.
The ordinary admission transaction checks the scope again before source/outbox
writes and cursor advance. None of these operations changes the bound scope.

Only closed-set scope error codes are added to normal logs. Unrecognized SQL
messages remain a generic hold; raw SQL text, cursors and configuration are not
logged. Database revocation/availability failures remain failures.

## Existing partitions: mandatory reviewed upgrade

Migration 0004 does not infer old account/filter settings and does not backfill
scope rows. Existing non-virgin partitions therefore stop at
`matrix_stream_scope_legacy_review_required` when the updated poller starts.
This is a deliberate upgrade hold, not an online transparent migration.

The migration owner may use
`cex_matrix_approve_legacy_stream_scope_v1(text,bigint,text,jsonb,text)` only after
stopping/fencing workers and allowing any active lease to expire. The arguments
are partition, expected revision, exact expected opaque cursor (SQL NULL where
appropriate), reviewed scope JSON and the SHA-256 reference of the review record.
The function and INSERT trigger both require the scope table owner. An EXECUTE
grant alone does not make a runtime role an approver. Every API is SECURITY
INVOKER; no new privilege escalation or public grant is introduced.

The transaction rejects an active lease, stale revision, wrong cursor, malformed
scope, missing approval commitment or existing binding. It records the database
owner identity and evidence reference but does not advance/replace any cursor,
rewrite source events, grant business authority or authenticate an external
review by itself. The external review must genuinely exist in approved custody.

Example SQL shape, using variables supplied through an approved private operator
session rather than putting real cursors or metadata in shell history:

```sql
begin;
select public.cex_matrix_approve_legacy_stream_scope_v1(
    :'partition', :'expected_revision'::bigint, :'expected_cursor',
    :'scope_json'::jsonb, :'approval_sha256'
);
commit;
```

Use an actual SQL NULL parameter for a null cursor; the literal string `null` is
not equivalent. Do not generate an approval hash just to bypass the upgrade hold.
A changed established scope has no rebind/delete API in this revision. It needs
an independently reviewed new-partition/backfill or forward-migration plan.
Changing `start_now` does not approve an old partition. A new explicit start-now
partition still declares only a new observation boundary, not replay coverage.

## Persistence and privileges

`matrix_transport_stream_scopes` stores one immutable JSON scope and binding
revision/fence per partition. Origin is `bootstrap` or `reviewed_legacy`; only the
latter carries owner and review commitment. UPDATE and DELETE are rejected.
SQL CHECK constraints reject JSON null schema/kind values explicitly, rather than
letting SQL three-valued logic accept them. Scope shape/size is independently
bounded in the table. The migration is additive and transactional.

The runtime needs reviewed invoker permissions for cursor row locking, reading
scope/history/inbox/poison metadata and inserting virgin scope rows. It must not
be the migration/table owner or have TRUNCATE, DDL, trigger-disable or arbitrary
repair authority. This change does not supply or qualify a complete deployment
role manifest. Existing cursor APIs remain compatible: the stronger scope
protocol is implemented by the updated poller and the new functions. An old
binary with broad database grants is not made safe by installing a table alone.

Rollback must stop workers and preserve the scope/history data. Do not roll back
to a poller that skips scope checks, drop bindings or disable triggers to make
polling resume. Use a schema-compatible build or a reviewed forward repair.

## Reply content control

The relay's bounded text/notice reply validator now accepts only `msgtype` and
`body` fields. Structured edits/replacement content, HTML, mentions, relations,
URLs and unknown extension keys are rejected, not silently stripped or sent
using the relay account. This intentionally narrows the previous object-shape
check. Existing rich replies need an explicit reviewed extension; they now hold.
Plain body text is not a content-moderation or notification-policy guarantee.

## Verification and remaining work

```text
python3 scripts/test-matrix-stream-scope.py
python3 scripts/check-matrix-stream-scope.py
cargo test --locked -p matrix-bot-poller -p matrix-bot-relay --all-targets
cargo clippy --locked -p matrix-bot-poller -p matrix-bot-relay --all-targets -- -D warnings
bash scripts/check-matrix-source-observation-postgres.sh
```

The SQL runner applies the entire `0001 -> 0002 -> 0003 -> 0004 -> 0005` chain twice
before every original assertion and all new suites. The new SQL suite covers
fresh/replayed scope, changed filter, owner/fence checks, immutable metadata,
legacy/live-lease/wrong-cursor holds, approved restart, null shape guards, and
non-owner approval rejection. Its temporary role and rows are rolled back.
A dedicated disposable PostgreSQL 16 instance with reset and fixture-role creation
permission is required. No test/reset operation is authorized on a live database.

Rust tests cover scope description, exact filter preservation, safe account URL,
identity responses, bounds, and reply extension rejection. Python mutations
check source wiring only. They neither execute Rust nor PostgreSQL. All three
remain in the existing supporting and authoritative job paths; no release gate
is removed. Current compiler/database/homeserver execution and full repository
qualification are still required before this source implementation is accepted.

This does not complete large-gap staged recovery, membership/filter-ID lifecycle,
durable adapter result lookup, runtime role deployment, retention/erasure or
first-playable integration. No frozen candidate, independent approval or
production authorization is created by the table, source checks or this document.

Protocol reference: Matrix Client-Server API, current-account information and
sync/filtering: https://spec.matrix.org/v1.19/client-server-api/

## Zero filter ID and environment compatibility

Only an absent `MATRIX_SYNC_FILTER` disables filtering. An explicitly empty,
whitespace-only or non-Unicode environment value is invalid. The opaque ID `0`
is preserved and sent to Matrix; it is no longer a local disable sentinel.
Deployments that previously configured `0` to request an unfiltered stream must
remove that variable and have the resulting unfiltered scope reviewed explicitly.
Inline filters must begin with `{` as their first character. Trailing whitespace
is preserved, but leading whitespace is rejected rather than misclassified.
This avoids binding a different effective stream than the operator configured.


## Round 9: filter-preserving backfill (ID lifecycle superseded below)

The prior backfill path sent `/messages` without a filter even when `/sync` used
one. Consequently, a filtered-out sender/type could reappear during recovery and
be normalised into a command. Stream-scope binding alone did not prevent that.
The same API specification defines a full Filter for `/sync` but a RoomEventFilter
for `/messages`; they cannot be passed interchangeably.

The updated `stream_scope.rs` parses a supported inline Filter using strict Serde
structs and derives only its `room.timeline` RoomEventFilter. The poller passes
that JSON as one URL-encoded `filter` query parameter on every backfill page.
Sender/type include/exclude lists, URL predicates, room selectors, member-loading
options and limits are retained. Top-level room includes/excludes and timeline
room selectors must both allow the particular joined room. An unexpected excluded
room holds the whole batch; it is not silently advanced or broadened.

The scope continues to retain the original, exact inline input bytes. The derived
per-room query is not used to replace or rewrite the stored scope. No migration,
dependency, command identity or cursor algorithm changes in this revision.

Only absent filters and inline filters without a timeline predicate legitimately
produce no `/messages` filter. A server-side filter ID is not a RoomEventFilter:
for a nonempty limited-timeline recovery range it raises
`matrix_gap_filter_id_requires_resolution` before backfill I/O. This includes the
valid opaque ID `0`; it is never reinterpreted as an unfiltered stream. Ordinary
unlimited `/sync` requests with an ID retain their existing behaviour. Pinned ID
definition lookup, its schema validation, and lifecycle evidence remain open.
No claim is made here about event-field projections hidden behind an unresolved ID.

Inline `event_fields` projections are unsupported because they can remove the
normaliser's identity fields while allowing cursor advance. Non-client event
format, include-leave streams and unknown timeline/root/room extensions also
fail before polling. The current poller does not implement leave-room replay or
extension-specific membership semantics. Recognised struct fields reject
malformed types and duplicate selectors. Non-timeline presence/account-data/state
sections are not interpreted as command selection. This is a deliberately bounded
subset, not complete support for every Matrix filter extension.

The Matrix `/sync` discriminator examines the first character. An inline object
with leading whitespace is therefore rejected instead of binding it as JSON and
sending a value the server treats as a filter ID. Existing such configurations
need an explicitly reviewed configuration correction, not an automatic scope
rewrite. Existing unsupported inline filters now fail startup. These changes are
not a transparent no-downtime upgrade, and do not solve previously missing events.

Required checks, in addition to all existing gates:

```text
python3 scripts/test-matrix-filter-recovery.py
python3 scripts/check-matrix-filter-recovery.py
cargo test --locked -p matrix-bot-poller --all-targets
cargo clippy --locked -p matrix-bot-poller --all-targets -- -D warnings
```

Nine new Rust tests define predicate preservation, room intersections, ID holds,
unsupported projections, duplicate/type negatives and single-parameter URL
encoding. They have not executed in the current authoring environment. Twelve
executed Python mutations exercise the source guard and balanced function
extraction; they do not run Rust or a homeserver. The guard is wired into the
existing Matrix source job and authoritative repository-integrity job without
removing any old step. Actual filtered `/sync` plus limited `/messages` integration,
compiler validation and the unchanged final-candidate gates remain required.

## Current ID lifecycle: definition pinning

The previous round-9 ordinary-ID behavior is superseded by
`docs/matrix-filter-definition-v1.md`. ID-backed filters, including `0`, require
an explicit reviewed SHA-256 and bounded authenticated definition lookup before
the polling loop. Both `/sync` and gap recovery use the verified inline snapshot,
not an unresolved remotely managed ID. Migration 0005 preserves the original scope
and adds an immutable companion definition pin checked before requests and inside
admission. Existing unpinned ID streams require a stopped, exact-cursor-reviewed
upgrade. There is no auto-repin or cursor reset. Optional known filter fields
may be absent but may no longer be JSON null. No runtime qualification is implied.
