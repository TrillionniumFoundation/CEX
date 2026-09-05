# Matrix bounded recovery and send-receipt contract v2

Status: implemented source candidate; not repository qualification  
Owner: `matrix-integration`  
Production authorization: `not_granted`

This contract extends the v12 implementation plan without changing ADR-004,
external-Agent authority, exact-money invariants or numbered migration head 0088.
A new source revision is not a frozen release candidate or evidence of production.

## Supported stream and authority

The current poller consumes joined-room text messages from Matrix Client-Server
v3 sync. Matrix owns source transport; Consumer Entry owns admitted business
commands. These repairs do not implement encrypted-room decryption, federation,
Matrix identity governance or complete redaction/edit business semantics. Edits
are not new executable commands. Known non-text message classes are ignored.

The source contract is the Matrix Client-Server specification, sections Syncing,
Getting events for a room, and Transaction identifiers. In particular, an empty
messages page with an end token is not the end of pagination. A valid Matrix send
response identifies an event; an HTTP success alone is insufficient for this
implementation to mark delivery sent. Runtime tests must use the supported
homeserver versions and verify their token and idempotency behavior.

## Initial boundary and limited timeline

An existing partition starts from its committed opaque cursor. A new partition
fails closed by default (`MATRIX_POLL_BOOTSTRAP_MODE=require_cursor`). Operators
may explicitly choose `start_now`: the first sync establishes a cursor boundary
with zero historical command admissions. This deliberate baseline is recorded in
cursor history; it is not a promise to replay all prior messages.

For each limited joined-room timeline, obtain its `prev_batch`, request messages
backwards toward the last committed sync token, and merge the recovered events
chronologically before the returned sync timeline. Required page start equals the
requested cursor. End tokens are opaque. A repeated token is an error. An absent
end means the server's visible range is exhausted; it cannot prove availability
of deleted, filtered or inaccessible historical events.

One recovery iteration is bounded to 100 pages, 100 events per page, 10,000 total
sync/recovery events, 32 MiB recovered bytes and 120 seconds. Per-response size is
also bounded. Empty pages with a new end token continue. Missing boundaries,
wrong-room events, malformed JSON, timeout, cycle and exhausted budgets leave the
committed sync cursor unchanged. There is no fallback that silently skips a gap.

Lease renewal checks unexpired owner, fence and expected revision and commits
before each network request. No database transaction crosses the network boundary.
A stolen/expired lease is not resurrected. Pages are deliberately not separately
checkpointed in this revision: process loss re-fetches from the committed cursor.
Very large gaps require an independently designed bounded backfill mechanism;
raising limits or manually skipping cursors is not an automatic remedy.

## Poison observation and quarantine

Malformed supported messages retain source identity/hash, partition, room and a
bounded restricted payload snapshot. Unstable `unsigned` and redundant `room_id`
fields are excluded from the canonical poison snapshot. Snapshots have a separate
privacy/retention boundary and may contain confidential message content: runtime
logs must not contain them, database access must be restricted, and production
requires approved encryption, retention and legal-erasure handling.

Observation and snapshot creation commit together under the cursor lease in a
separate transaction. The following admission transaction rejects any partition
with unacknowledged poison, so failed admission cannot erase the diagnosis. The
SQL cursor trigger also enforces the hold. An operator acknowledges a reviewed
quarantine through the existing identity/hash-bound, one-time interface. Only a
later ordinary batch transaction may advance. Acknowledgement is not a repaired
message, a successful business effect or evidence of reprocessing.

Source/content changes collide rather than overwrite historical evidence. The
current hold is partition-wide, so a poison event may stop unrelated rooms in that
partition. This is a safety/availability tradeoff, not an achieved throughput SLO.

## Remote outcomes and same-operation recovery

| Boundary | Observation | Permitted action |
|---|---|---|
| Adapter | Timeout, network loss, interrupted body, invalid success body or transient status | Preserve unknown outcome in dead-letter/hold; do not resend automatically |
| Adapter | Claim expires after possible acceptance | Preserve prior claim owner and unknown outcome; no automatic reclaim |
| Adapter | Valid presentation reply | Reply must target the original room; bounded text/notice body only |
| Matrix | Before first network attempt | Bind delivery, payload hash, room, homeserver and credential digest durably |
| Matrix | Lost/partial response or retryable status | Bounded retry with the same delivery UUID and unchanged bound scope |
| Matrix | Credential or homeserver scope changes | Hold; never turn the same local delivery into a second remote operation |
| Matrix | HTTP 200 and valid event ID | Store immutable receipt and complete the fenced claim in one transaction |
| Matrix | Oversized response, rejected status or exhausted attempts | Hold with its error classification; no invented non-execution proof |

The credential digest is a restricted correlation value, never a bearer credential
or an authorization primitive. It deliberately blocks automatic replay after token
rotation, even when a homeserver might preserve a device scope. Independent
credential rotation/reconciliation procedures must resolve that case.

An adapter's persistent exactly-once business replay is not established by these
changes. Holding ambiguous adapter effects avoids duplicate commands at the cost
of availability. A durable result lookup, principal-bound replay and operator
recovery implementation remain repository work. Neither a dead letter nor an
empty JSON object proves an external business outcome.

## Migration, permissions and rollback

Apply the separate Matrix chain in order: `0001 -> 0002 -> 0003 -> 0004 -> 0005`. Existing source
identities and delivered rows are not rewritten. Receipt requirements govern new
transitions and do not fabricate receipts for historical sent rows. Each additive
migration is replayed in the disposable database suite. Never run 0001 alone on an
upgraded deployment: it reinstalls obsolete function bodies.

The new invoker functions do not gain SECURITY DEFINER or public grants. Runtime
roles need explicit, reviewed table/sequence/function permissions. Owners are for
one-shot migration only. Public privileges are revoked; normal roles must not
receive DDL, TRUNCATE, unrestricted repair or ability to disable triggers. Full
runtime-role privilege tests remain required; an owner-role CI run does not prove
least-privilege deployment correctness.

Rollback stops admission and claims, preserves inbox/observations/cursor history,
poison snapshots, send bindings and receipts, then uses a schema-compatible image.
Forward repair is required for a semantic reversal. Unknown outcomes and changed
credential scopes must be reconciled, not erased or reissued with fresh identities.

## Verification and independent qualification

The Python recovery checker validates source wiring, shared-parser linkage, documentation
and preserved CI requirements. Mutation tests demonstrate rejection of selected
source regressions. They do not compile Rust or execute PostgreSQL.

Cargo tests cover pagination transitions, token cycles/bounds, URL construction,
receipt classification, original-room reply constraints and profile parsing.
PostgreSQL tests cover renewal, poison hold, cursor history, expired-adapter hold,
credential binding, receipt immutability and sent-transition enforcement. Missing
toolchains or database configuration must fail, not silently skip.

The supporting workflow executes exact-head source, adapter, transport and SQL
lanes with read-only permissions. It does not replace the five v12 authoritative
workflows or aggregate immutable manifest. Independent real-homeserver recovery,
credential custody, representative volume, first-playable integration, protected
main, exact-head review and V12-X1-X8 remain separate qualification gates.

## Round 3: exact adapter response and current-schema regression

The relay now validates the adapter's actual response envelope before starting
its completion transaction. `accepted` must be a JSON boolean and `action` a
bounded machine identifier; sender and room must equal the immutable source.
The response event ID must equal the source event ID, except for the existing
`status_lookup` protocol: that handler returns the requested task ID. The relay
accepts that exception only when the source bytes contain the same `/status`
request. A missing/null forwarded source ID stays an explicit unverified hold;
it is not guessed from response content. Error envelopes cannot complete a claim.

`accepted=false` is legitimate for help, unsupported-command explanations and
ignored events; it never means a successful business mutation. A duplicate-cache
response is held as `adapter_duplicate_outcome_unknown`, because the adapter
inserts its recent-event marker before the downstream outcome is durable. This
closes a false-success path but is not durable result lookup or reconciliation.
An explicit `projected_reply: null` is compatible with Serde Option serialization
and means no reply. Non-null replies still require the bounded Matrix message
contract. Matrix send URLs retain a configured reverse-proxy path prefix and
reject embedded credentials, queries or fragments rather than changing authority.

The database entry script delegates to `scripts/matrix_postgres_regression.py`.
Both public shell entrypoints use the same runner; the old eval/0001-only
bootstrap implementation is removed. The complete original SQL body is retained
byte-for-byte in `scripts/test-matrix-transport-baseline.sql`. The runner snapshots
the complete migration chain and regression inputs and compares the migration
directory to its fixed manifest. On a dedicated PostgreSQL 16 test database it applies
the full current migration chain twice, then executes ALL original and new SQL
assertions against the final schema. A stale 0001-only function body cannot be
what the baseline regression accidentally tests. No original assertion is removed.

Reset requires `MATRIX_TEST_ALLOW_SCHEMA_RESET=1`, a dedicated name matching
`matrix_*_ci` or `cex_matrix_test_*`, and a database with no unrelated tables.
The runner verifies server major and actual database before reset. Credentials
are passed only through a restricted environment, never argv; inherited service,
password-file and option variables cannot select a different database. Unknown or
duplicate URL options, malformed ports and encodings fail before a client runs.
One database session covers identity checks, an advisory lock, object guards,
reset, all migration replays and regressions. Statement/lock timeouts and a
900-second client wall limit are enforced. Failure cannot become skip/success. The optional JSON report binds input
hashes and stage results; Python tests use a fake client and do not prove SQL.

The trusted ROG lane is restricted to push events on the remediation branch.
It retains the existing hosted jobs and does not replace required release
contexts. Actual test code runs as a non-root user in resource-bounded containers
without host home, SSH agent or Docker socket; build/test network access is
limited to an ephemeral internal PostgreSQL service with no published port.
Runner availability is not a test result, and image/source snapshots are not
qualification evidence by themselves. The lane retains failed command results.

## Round 8: immutable stream scope and plain replies

`docs/matrix-stream-scope-v1.md` adds the migration-0004 upgrade boundary. Updated
pollers verify token ownership at startup, bind immutable endpoint/account/filter
metadata before cursor-bearing HTTP, and check it again during admission. Old
unbound streams require owner-reviewed exact-cursor approval with no live lease.
Binding does not advance history or grant a new scope. The full current SQL chain
now has four migrations; no prior assertion is removed. Replies now admit only
`msgtype` and `body`, with other structured control fields rejected. These are
unqualified source changes, not completed Rust/SQL/homeserver or production gates.


## Round 10: one guarded SQL entrypoint implementation

`docs/matrix-sql-runner-v4.md` supersedes the old multi-connection runner/report
shape. Both existing shell command names remain usable, but both now require
explicit disposable-DB consent and run the complete current chain. Reports must
be written under `run/`, never over source or a linked/external path. Unknown
objects are rejected by exact name rather than trusting a transport-like prefix.
A report confirms only observed stage markers in one psql session; it does not
prove independent hosted execution, application runtime or deployment roles.

## Round 11: execute a pinned ID filter definition

`docs/matrix-filter-definition-v1.md` defines explicit expected definition hashes,
bounded authenticated ID resolution, identical sync/backfill execution snapshots
and the migration-0005 pin/legacy-approval interfaces. This supersedes ordinary
unresolved-ID sync: the updated poller never sends a cursor with an unresolved
filter ID. All previous SQL assertions remain, with the complete five-step chain
reapplied twice before all suites. Rust/SQL/homeserver execution and the separate
adapter result-reconciliation work remain unqualified or open as documented.
