# Exact-source handoff and Matrix fault-window regression

Status: executable repository regression contract; independent acceptance remains open.
Production authorization: `not_granted`.

## Source input for the 23-module handoff

`scripts/export-development-source.py` reads HEAD Git blobs, not runtime files.
It rejects dirty tracked state, missing module contracts, catalog/package mismatch,
unsafe paths, symlinks, gitlinks and over-budget inputs. The artifact includes a
reproducible `source.tar.gz`, a complete file/hash/mode manifest and package-to-source
index. Ignored/untracked credentials, CI logs and Git history are not inputs;
committed example/test files remain ordinary source. The archive is source input,
not test success, Cargo resolution, an independent review or production evidence.

The existing Sequence 54 workflow retains this input separately, before qualification.
Every consumer must verify the artifact digest, source/tree and all archive entries.
An early uploaded source archive cannot qualify a later failed or cancelled job.
Existing metadata, source-integrity, tests, lint and release gates remain mandatory.

## Real database failure windows

`scripts/test-matrix-cli-fault-windows.py` uses the existing explicit disposable
consent, literal loopback and `matrix_review_ci` database restriction. It creates
only a temporary ordinary login inheriting the existing reconciler runtime role.
The real canonical CLI, real libpq and PostgreSQL execute unchanged production
code. HTTP result lookup remains an explicitly synthetic fixture.

The loopback wire proxy accepts only unencrypted test traffic. It never logs or
stores authentication or query payloads. Before-query injection drops the command
before forwarding it; the independently read database must remain unchanged.
After-commit injection withholds every result frame, waits for PostgreSQL's idle
ReadyForQuery, then disconnects. A separate owner connection must witness one
committed terminal transition and one observation while the CLI reports failure.
Retry must preserve the same identity, return replay and append only an observation.

For concurrency, an owner transaction holds the exact outbox row. Both real CLI
sessions must be observed waiting on database locks before the owner releases it.
Identical results yield one reconciliation and one replay. Different result bytes
with the same delivery binding yield one winner and one rejection. Each case
requires one terminal transition and one original delivery, not just HTTP success.
Missing prerequisites, missing window/lock witnesses, child failure or unexpected
state cause failure; no database test is silently skipped. Append-only fixture
records remain; the temporary login is revoked and removed. No production database,
homeserver, external Agent, remote TLS, real credential custody or disaster-recovery
claim is made by these tests.

The existing Matrix workflow runs the new test after its complete migration and
original CLI regression and retains a separate exact-head/attempt JSON artifact.
Six named database assertion groups are required; they count as executed only when
the actual current-head report, job steps and artifact digest have been verified.
The self-tests use synthetic wire peers and do not count as database execution.

## Acceptance boundary

This batch advances P1-03 source reproducibility and the database subsection of
P1-05. It does not close the full work packages, freeze business SLO targets, grant
Administration permissions, manufacture independent approvals, or waive V12-X1–X8.
P2 runtime refactoring is not included. Preserve the existing candidate trigger as
the only freeze authority and requalify each changed exact source/merge subject.
