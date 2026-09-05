# Matrix SQL runner v4: single-session execution and safe entrypoints

Status: unqualified source implementation; PostgreSQL execution pending
Owner: matrix-integration
Parent: active v12 implementation addendum, Blocks H and K
Production authorization: `not_granted`

## Corrected execution paths

Both public commands delegate to `scripts/matrix_postgres_regression.py`:

```text
bash scripts/check-matrix-transport-postgres.sh
bash scripts/check-matrix-source-observation-postgres.sh
```

The old transport command previously evaluated shell generated from a connection
URL, applied only migration 0001, and reset rows without the newer consent and
identity controls. That executable path is removed, not merely omitted from a
new workflow. Both wrappers now have identical contents and cannot select a
reduced migration chain. Unknown arguments and missing required inputs fail.

All original behavioral assertions are preserved in the new standalone file
`scripts/test-matrix-transport-baseline.sql`. Its 10,870 bytes have SHA-256
`4a773ac20af7115ee5f4fbfdafa484badbea3a34be04b6debae5e69ddd214dd3`, identical
to the SQL heredoc extracted from original wrapper Git blob
`5c951733ee84d9496a1196e2769cd8edfab7ff23`. No original SQL assertion was rewritten,
removed, replaced by a stub, or run against an intermediate downgraded schema.
The extraction proof is a byte comparison, not a PostgreSQL test result.

## Connection and destructive-test prerequisites

Reset requires `MATRIX_TEST_ALLOW_SCHEMA_RESET=1` and an explicit
`MATRIX_TEST_DATABASE_URL` with a dedicated `matrix_*_ci` or `cex_matrix_test_*`
database name. Database names are bounded to 63 ASCII characters. The host must
be a single DNS name, IPv4 or IPv6 address; comma-separated failover targets,
encoded socket/host ambiguity, controls and malformed connection options fail
before client execution. These guards are not permission to use a live database
that happens to have a test-looking name. Use an isolated disposable service.

Credentials enter only a restricted child environment, never argv or report
content. Ambient PGSERVICE/PGOPTIONS/password-file/loader/shell variables cannot
select another database or install a process hook. A trusted Python interpreter,
shell, psql executable and private checkout remain prerequisites. The runner is
not a sandbox for a compromised client or an adversarial database administrator.

A single psql process/connection performs these stages in order: verify actual
database and PostgreSQL major 16, acquire the nonblocking advisory lock for this
test runner, reject unrelated objects, reset allowlisted transport rows, apply
0001 through 0005 twice, run the original transport SQL, then all four additive
regression suites. Source SQL follows the explicit no-backslash/no-NUL profile
below; reconnect and error-waiver psql metacommands are rejected regardless of
line position. The first guard and later mutations cannot choose different
connections through a host list. A connection loss cannot be auto-retried into
a new successful test session by this runner.

The advisory lock serializes cooperating v4 runners for the database and survives
test transaction rollbacks until disconnect. It does not fence old binaries,
manual SQL sessions or uncooperative processes. A dedicated test service with no
application traffic is still required; the runner cannot establish that isolation
from a URI and a table scan. Database ownership and runtime role correctness must
be separately tested.

## Exact object ownership and reset scope

The reset allowlist contains twelve exact `public.matrix_transport_*` table
names from the current Matrix chain. A matching prefix alone is insufficient.
Unrelated ordinary/partitioned tables, views, materialized views or foreign tables
in application schemas reject the attempt. Reset checks again inside its
transaction and truncates only existing allowlisted ordinary tables. It does not
drop databases, use CASCADE, or infer ownership of newly named tables.

New schema objects require a reviewed migration and corresponding allowlist
change. Existing objects' definitions and privilege grants are not authenticated
by their names; only use the disposable CI database, not a database controlled by
an adversary. No deployment credential or persistent database was changed while
this implementation was authored.

## Input, process and output integrity

Inputs must be regular UTF-8 files inside the checkout, at most 2 MiB each, with
no symlink traversal or hardlinks. Read-time size/inode/time changes are rejected.
The acquired source bytes form the SQL script and source digests. The migration
inventory, SQL inputs, runner and entrypoint bytes are rechecked after execution.
A changed source cannot produce a successful final observation report.

The psql process has a 900-second wall limit; statement, lock and idle-transaction
limits remain 30, 5 and 30 seconds respectively. Stdout/stderr go to private
spool files, not an unbounded in-memory capture. Combined output is monitored
against a 1 MiB budget and oversized output fails even after a fast child exit.
Polling may briefly overshoot the threshold; this is not a hard filesystem quota.
The current supervisor requires POSIX waitid/WNOWAIT. It stops the owned process
group on every exit path, including normal client completion, before reaping the
direct child; private spools are removed. Native Windows execution fails before
starting psql rather than claiming equivalent process-tree cleanup. Raw server/client
diagnostics are not echoed because they may contain credentials or data. The last confirmed stage is still
available in normal partial transcripts. A timeout without a returned transcript
means progress is unconfirmed, not that no SQL ran.

An explicit `--evidence` path is validated before any client invocation. It must
be a JSON file under the checkout's `run/` directory, with no symlink traversal
or hardlink destination. External, source-overwriting and parent-escape paths
are rejected. Output is written through a mode-0600 fsynced temporary file and
atomic replacement. Failed attempts replace old success records with failure
when publication is possible. Failed replacement preserves the old bytes but
returns a failure exit; old file presence is never current success evidence.

These are private-checkout consistency protections, not immunity to concurrent
ancestor replacement, malicious root processes or compromise of the host.

## Deliberately versioned local report

Reports use `cex.matrix-postgres-regression.v4`, not the previous v3 profile.
They retain source digests, stage names/status, observation time and
`production_authorization=not_granted`, and add `execution_model=single_psql_session`
and one `session_exit_code`. Stage confirmation is `post_sql_marker`, not a
fabricated separate process exit code for every SQL block.

The runner generates a fresh nonce and requires one correctly ordered start/ok
pair for every expected stage and a PostgreSQL-16 version observation in the
identity stage. Missing, repeated, out-of-order or mismatched observations cannot
be repaired from a generic success message. Even a complete transcript cannot
pass with a nonzero client exit. A marker indicates where a trusted psql process
progressed; it does not independently attest the host, issuer, backend or source
origin. Synthetic clients can reproduce markers and remain test fixtures only.

Strict v3 consumers must explicitly adopt v4; old reports are not rewritten or
promoted. The final CEX release manifest, authority workflow identities, shared
candidate trigger and independent approval gates are unchanged. This local report
cannot replace exact-SHA GitHub execution, final candidate aggregation or X1-X8.

## Verification scope

```text
python3 scripts/test-matrix-postgres-runner.py
python3 scripts/test-matrix-runner-hardening.py
python3 scripts/test-matrix-runner-lifecycle.py
```

The suites use real child processes and filesystem operations plus explicitly
fake psql clients and SQL fragments. They test both entrypoints, pre-client
refusal, unchanged original assertions, session construction, transcript failure,
source mutation, time/output limits and atomic publication. They do not execute
PostgreSQL, compile Rust or verify business behavior. The source recovery checker
is updated to require the new path and retains all existing SQL-suite requirements.
All three suites are wired into existing Matrix and repository-integrity jobs.

Actual PostgreSQL 16 qualification must prove guard refusal on wrong database or
version, concurrent runner rejection, no mutation on rejected foreign objects,
all original assertions, complete migration replays and role behavior. Full CEX
checkout, compiler/runtime execution, current semantic inventory, independent
review and external production conditions remain open where evidence is absent.

Primary API references for the implemented connection and locking semantics:

```text
https://www.postgresql.org/docs/16/app-psql.html
https://www.postgresql.org/docs/16/explicit-locking.html
https://www.postgresql.org/docs/16/libpq-connect.html
```


## Round 12: inline control commands and process-group lifecycle

The previous source guard rejected only a backslash at the start of a line.
PostgreSQL psql permits SQL and client metacommands on the same line, so an input
such as `select 1;` followed by a backslash command could pass that guard. The
current fixed regression-input profile rejects every backslash and NUL byte in
all migration, baseline and additive SQL source inputs before client selection.
This deliberately also rejects backslashes in SQL string literals, dollar-quoted
bodies, identifiers and comments. It is not a general-purpose SQL parser. Future
fixtures requiring literal backslashes need a separately reviewed input protocol;
do not weaken this guard back to a line-start regular expression. The runner's
own fixed psql commands remain generated outside the source-input profile.
No migration or existing assertion file is changed by this repair.

The prior supervisor used Popen.poll(), which reaped a completed direct child
before cleanup. A parent exiting successfully could leave an already started
child performing work after the supervisor returned. The repaired supervisor
uses waitid with WNOWAIT to observe completion without reaping the leader, then
signals its owned process group and only afterwards waits for the direct child.
Keeping the leader waitable reserves the process identity during cleanup rather
than signalling a group after its identifier has been released for reuse. Normal
exit, nonzero exit, output overflow, timeout and exceptions take this same cleanup
path. Nonstandard timeout values fail before process creation, and unrelated
inheritable file descriptors are closed in the child.

This requires a POSIX platform exposing waitid/WNOWAIT and a normal trusted Python
child-waiting environment. Existing PostgreSQL execution jobs are Linux jobs;
native Windows has no equivalent implementation here and is rejected before a
client starts. Process-group custody does not contain descendants that deliberately
create a new session, other concurrent reapers or privileged hostile processes.
Only the direct child is reaped by this process; descendant reaping belongs to
the operating system. An operating-system stall can delay cleanup. The wall/output
limits remain supervised limits, not hard real-time or filesystem isolation.

Unrelated-object checks now recognize genuine temporary schemas through
pg_my_temp_schema() and pg_is_other_temp_schema(oid). They no longer exclude
arbitrary schemas matching pg_temp_% or pg_toast% spelling patterns. The exact
12-table allowlist and transaction-local reset recheck remain. These function
calls and their actual database refusal behavior still need PostgreSQL execution;
a source assertion is not a database test.

The new lifecycle suite has 17 tests. Native fixtures start the current Python
interpreter with standard-library-only startup, use a ready-file handshake to
prove the descendant started, then check that no delayed write occurs after
supervision returns. The suite covers success, nonzero exit, timeout, output
overflow, preserved child output/exit, waitable-before-kill ordering and descriptor
isolation. Input-policy tests reject line-inline client commands before selecting
psql. They neither start PostgreSQL nor simulate a database pass. Existing fake
psql orchestration tests remain separately identified; all original runtime and
release gates remain mandatory.

Primary lifecycle and namespace references:

```text
https://docs.python.org/3/library/os.html#os.waitid
https://www.postgresql.org/docs/16/functions-info.html
```
