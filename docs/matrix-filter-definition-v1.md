# Matrix ID filter definition pinning v1

Status: unqualified source implementation; real Rust/PostgreSQL/homeserver validation pending
Owner: matrix-integration
Parent: active v12 implementation addendum, Blocks H and K
Production authorization: `not_granted`

## Problem and current behavior

Round 9 held limited recovery for any server-owned filter ID but still sent the
ID to ordinary sync without inspecting its definition. That left both a recovery
availability gap and an unvalidated event-field projection path. The updated
poller resolves and validates the definition before its polling loop, binds the
exact bytes durably to the original ID-backed stream, then uses that immutable
inline snapshot for both sync and backfill. No unresolved ID reaches `/sync`.

This revision adds `apps/matrix-bot-poller/src/filter_definition.rs` and migration
`services/matrix-entry-adapter/migrations/0005_filter_definition_pins.sql`. It does
not add Cargo dependencies, change source/delivery identity, alter Ledger
migration head 0088, or authorize production. Installing source is not executed
qualification. Previous real-world missing events cannot be reconstructed by a pin.

## Explicit configuration and acquisition

An ID-valued `MATRIX_SYNC_FILTER`, including `0`, now requires
`MATRIX_SYNC_FILTER_DEFINITION_SHA256`. Its value is exactly `sha256:` followed
by 64 lowercase hexadecimal characters. Empty, whitespace-padded, non-Unicode,
missing ID commitments and commitments with an inline/absent filter are rejected.
This is a deliberate compatibility change: ID-backed ordinary sync no longer
starts without a reviewed definition commitment.

The operator obtains the expected hash from independently inspected definition
bytes in approved custody. Do not copy an unexpected runtime response's hash into
configuration just to clear a hold. The commitment is a content check, not a secret,
signature or proof of independent approval; it may reveal private selector metadata.

After the existing `/account/whoami` validates the token owner, the process reads
`GET /_matrix/client/v3/user/{userId}/filter/{filterId}` on that same configured
homeserver. The reverse-proxy prefix is retained, user and filter occupy encoded
path segments, and dot-only filter IDs are rejected. Redirects remain disabled;
the credential is in the Authorization header, not URL, logs or stored pin.
The entire request/body read has a ten-second deadline and 4,096-byte limit.
Only HTTP 200 followed by the expected SHA-256, valid UTF-8 and supported filter
schema yields a resolved value. Failures prevent polling, not just gap recovery.

The digest covers the exact response body bytes, including permitted trailing
whitespace; it is not a reserialized JSON hash. Formatting or key-order changes
therefore require review rather than claiming semantic equivalence. The body
must begin with `{`, since the inline sync discriminator depends on that byte.
Transient failure, revoked token, missing ID, changed definition or malformed
body has no automatic fallback to an unfiltered stream, cached database snapshot,
new partition or freshly invented commitment. A supervised restart retries lookup.

## One validated execution snapshot

The process retains the validated definition in a private `FilterDefinition`
value, without Debug formatting. Every `/sync` query uses that exact inline
snapshot, not the remotely managed ID. Limited recovery derives `room.timeline`
from the same snapshot through the existing backfill projection; both room
selectors and all supported timeline predicates remain enforced.

Changes to the remote ID after startup do not change this process's sync query.
On restart, acquisition and digest verification repeat. Changing the configured
commitment cannot silently repin an existing partition because the database also
compares the immutable definition. The stored stream scope still records the
original endpoint, account and ID; it is neither rewritten into an inline scope
nor replaced when a definition is acquired.

The strict supported Filter schema rejects event-field projection, non-client
format, include-leave streams, unknown command-affecting extensions and duplicate
known fields. Recognized optional fields may be absent but may not be JSON null.
The latter now applies to configured inline filters too: null selectors are not
silently interpreted as no restrictions. Non-timeline sections remain outside
the command selection mechanism; full arbitrary filter-extension support is not
claimed. Hash agreement does not bypass these schema checks.

## Durable pin and concurrent workers

`matrix_transport_filter_definitions` records one exact definition and digest
per partition, the original filter ID, cursor revision/fence at binding, origin,
and optional reviewed-legacy approval metadata. The table independently checks
UTF-8 byte length, object shape and PostgreSQL's recomputed SHA-256 of the text.
The poller performs the deeper supported-schema validation. UPDATE and DELETE
are rejected; a changed ID/content/digest is a collision, not a new accepted pin.

`cex_matrix_bind_filter_definition_v1(text,text,bigint,bigint,text,text,text)`
locks the cursor row, requires the same live owner/fence/revision and a matching
ID-backed stream scope. Exact same content returns `replay`. First automatic
binding is permitted only for a virgin partition: revision zero, NULL cursor,
live lease and no inbox/history/poison records. The INSERT trigger independently
checks this boundary so a direct INSERT grant cannot approve legacy history.

The scope and pin operations share one committed transaction before any
cursor-bearing HTTP. Both repeat inside the admission transaction. No SQL
transaction is held during definition lookup or Matrix requests. Competing
workers serialize on the same cursor row; expired or stolen leases do not bind.
These are source-defined rules; actual PostgreSQL concurrency remains to be run.

## Existing ID streams: reviewed upgrade, not a reset

Existing ID scopes and cursor positions are retained. A non-virgin stream without
a definition pin raises `matrix_filter_definition_legacy_review_required` before
the updated poller sends a cursor. A stream lacking even its original scope still
requires the migration-0004 review first. Definition content is not inferred from
today's environment or today's remote filter.

With workers stopped/fenced and the lease expired, the pin-table owner can call
`cex_matrix_approve_legacy_filter_definition_v1(text,bigint,text,text,text,text,text)`.
Arguments are partition, exact expected revision, exact expected cursor, original
filter ID, reviewed definition bytes, their SHA-256, and the genuine review-record
SHA-256. A SQL NULL cursor is different from the string `null`. The function checks
the caller is the owner; the INSERT trigger separately checks actor, lease and
scope. An EXECUTE grant alone does not make a runtime role an approver.

Approval inserts provenance but does not advance a cursor, rewrite the scope,
repair a message, or attest that the referenced review genuinely occurred.
An existing pin cannot be reapproved or replaced; a changed definition needs a
separately reviewed new-stream/backfill or forward-migration plan. No owner
credential, real cursor, approval record or deployment is provided by this change.

## Migration, permissions, rollback and privacy

Apply the complete Matrix chain `0001 -> 0002 -> 0003 -> 0004 -> 0005` before the
updated poller starts. Every additive migration is reapplied in the disposable
regression chain. All old SQL assertion files remain unchanged. The runner's exact
reset inventory now has twelve tables and includes this new table by name.
No live database reset is authorized. Evidence report format remains v4.

New routines are SECURITY INVOKER with fixed search_path and public access revoked.
The runtime needs reviewed invoker permissions for cursor locking, scope/pin
reads and virgin pin insertion; it must not own the pin table or have DDL,
TRUNCATE, unrestricted repair or trigger-disable powers. The database schema
alone does not make old binaries or malicious broadly privileged callers safe.
A complete least-privilege deployment manifest remains a separate open item.

Definitions contain room and sender selectors; store them in restricted database
custody, not public artifacts or ordinary logs. Normal polling logs use closed
machine codes and never raw SQL diagnostics. Retention, erasure, key custody and
monitoring are still deployment requirements, not consequences of content hashing.

Rollback stops workers and preserves scopes, pins, cursors and evidence. Use a
schema-compatible binary; do not revert to sending unresolved IDs or remove pins
to make polling resume. A recovery hold does not establish non-execution of any
prior downstream business operation.

## Verification and evidence boundaries

```text
python3 scripts/test-matrix-filter-definition.py
python3 scripts/check-matrix-filter-definition.py
cargo test --locked -p matrix-bot-poller --all-targets
cargo clippy --locked -p matrix-bot-poller --all-targets -- -D warnings
bash scripts/check-matrix-source-observation-postgres.sh
```

New Rust definitions cover explicit pin configuration, exact bytes/digest, unsafe
shapes, ID URL construction, effective sync/backfill selection and actual loopback
HTTP acquisition with hostile responses. They have not executed in the authoring
environment. The additional SQL suite defines binding/replay, owner/fence/revision,
independent database digest, immutable bytes, legacy/live-lease/expected-cursor
holds, restart after approval and non-owner rejection, ending with rollback.
That suite likewise remains unexecuted here.

Python mutation tests validate executable token sequences and source wiring only,
including comment/string impostors. They neither run a Rust compiler nor model
PostgreSQL as actual SQL evidence. The existing complete Rust/SQL workflows are
retained and the new source checks are added to their established jobs. Full
repository checks, exact-head hosted execution, actual homeserver failure tests,
independent review and V12-X1-X8 remain mandatory.

Primary references:
- Matrix Client-Server v1.19, Filtering API and Syncing:
  https://spec.matrix.org/v1.19/client-server-api/
- PostgreSQL 16 binary string hashing:
  https://www.postgresql.org/docs/16/functions-binarystring.html
