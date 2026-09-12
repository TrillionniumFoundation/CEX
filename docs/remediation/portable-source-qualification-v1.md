# Detached-checkout fixtures and platform-specific source qualification

Status: source repair requiring exact-head hosted verification and independent review.
Production authorization: `not_granted`.

## Detached HEAD is not an empty release identity

An exact SHA checkout legitimately has no current branch. The strict wiring suite
previously copied that empty value into its synthetic fake-run manifest, causing
its own positive fixture to fail before real qualification. Its 69,644-byte test
body is now retained unchanged as `scripts/strict_release_wiring_cases.py`, bound
to original Git blob `6864a7845bd858e18b4cb39011e1d6e9dbef6ece` and exact size.
The existing entry point runs every original case and validator. Only the copied,
in-memory fixture metadata is adapted: a detached fixture uses the explicit label
`self-test/detached-<actual source SHA>`. Named branches and commit/tree/lock/migration
identities remain unchanged. No Git ref, hosted run, release manifest or approval
is created. Real collectors and manifest validators are untouched. Three added
fixture tests cover preservation, nonmutation, source specificity and malformed
identity rejection. Both named and detached checkouts must execute the full suite.

## Complete documentation gate regression denominator

The gate tests must provide results for the two existing remediation children and
the current Matrix schema versions. The repaired fixture checks the exact call
count and separately injects failure into each of the fourteen children. An
omitted child or a failure still prevents success; no checker is skipped. These
are synthetic controller tests, not successful execution of the child services.

## Windows source semantics versus Linux runtime custody

The Windows Rust lane remains mandatory. Python on Windows does not expose POSIX
uid custody or Git executable bits through ordinary filesystem metadata. The
source checks must not claim either was verified there.

`scripts/matrix_source_platform.py` obtains the executable mode from one stage-zero
Git index entry on Windows and verifies that its Git blob ID matches the actual
bounded, no-symlink file bytes. Missing, conflicted, duplicated, changed or nonregular
entries fail. POSIX keeps the existing native mode checks. The direct historical
loader denial probe still runs as an actual child process; Windows supplies only
its validated SystemRoot in addition to the closed existing environment, not
arbitrary inherited PG/service/Python credentials or settings.

On Windows the source gate executes the actual pure v2/v3 parser and SQL-builder
functions with explicit synthetic envelopes. It verifies binding mutations,
duplicate JSON, stable observations, function selection, input immutability and
closed-environment construction. Results distinguish portable contract testing
from POSIX runtime self-testing. None of these tests starts psql or establishes
real remote TLS, credential custody or a production filesystem ACL.

Linux retains the complete original runtime self-tests without modification. Its
real CLI/PostgreSQL migration, permission, replay, response-loss and concurrent
row-lock regressions remain mandatory. This source-portability change neither
ports the production operator CLI to Windows nor relaxes its POSIX custody checks.
All four runtime command/loader implementation files and SQL migrations are
unchanged. A passing Windows source gate cannot replace Linux runtime evidence.

## Separate target-documentation repair remains required

Actual Cargo metadata revealed ten existing targets missing from the catalog:
one Gateway ignored runtime probe and nine Hepta signer/example/integration-test
entry points. Their four-file repair also requires regenerating the complete
semantic inventory's catalog digest. The prepared patch is not part of this
platform-source commit; the original Cargo authority gate must continue to reject
the omission until the complete catalog/docs/generated-inventory change lands.
No target, test, index check or semantic digest is removed to hide this blocker.
An ignored probe remains ignored and needs a separately approved runtime rehearsal;
recording its path cannot claim that it executed or reactivate retired authority.

The wider 23-module independent handoff, full service rehearsals, protected-main
administration, independent approvals, external custody/load/DR and V12-X1–X8
remain unclosed. Current exact-head and actual-merge results, not this document,
determine acceptance.
