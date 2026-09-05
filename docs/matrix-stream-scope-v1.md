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

The SQL runner applies the entire `0001 -> 0002 -> 0003 -> 0004` chain twice
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
Other inline-filter bytes, including surrounding whitespace, are preserved.
This avoids binding a different effective stream than the operator configured.
