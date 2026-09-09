# Matrix result reconciliation security contract v2

Status: active source contract  
Sequence: CEX v12 Sequence 54  
Production authorization: `not_granted`

This document closes the three repository-source defects identified during the
independent review of the Matrix response-loss recovery path. It does not claim
that hosted jobs ran, that reviewers approved the final head, or that production
authorization was granted.

## Authority and non-goals

The reconciliation command may convert one previously recorded, explicitly
unknown adapter outcome into `sent` only after a read-only result lookup and a
least-privilege PostgreSQL transition. It never resubmits the Matrix event, the
consumer task, or a downstream invocation.

The durable Matrix outbox remains authoritative for the delivery identity and
payload commitment. Consumer Entry remains authoritative only for its durable
task projection. An HTTP 200 response, a task identifier, or a matching Matrix
principal alone is insufficient to close a delivery.

## Exact delivery causal binding

Security contract v2 introduces the domain-separated
`request_fingerprint`:

```text
domain          cex.matrix.adapter-result-delivery.v1
components      delivery_id
                payload_sha256
                event_id
                sender
                room_id
encoding        for each component: unsigned 64-bit big-endian UTF-8 byte
                length, followed by the exact UTF-8 bytes
digest          sha256:<64 lowercase hexadecimal characters>
```

The same algorithm is implemented independently in:

- `scripts/reconcile-matrix-adapter-result.py`;
- `services/matrix-entry-adapter/src/result_reconciliation.rs`;
- `services/consumer-entry-api/src/matrix_result_lookup.rs`;
- `0005_adapter_result_causal_binding.sql`.

The command derives the fingerprint from operator-supplied delivery data. The
Adapter accepts and signs all five authority components plus the fingerprint.
Consumer Entry recomputes the fingerprint before reading its replay snapshot and
requires the signed assertion to carry the same value. The Adapter validates the
echoed `delivery_id`, `payload_sha256`, Matrix identities and fingerprint before
returning the result. PostgreSQL then locks the exact outbox row, recomputes the
fingerprint from the persisted delivery, and rejects any mismatch.

Consequently, a result recovered for the same Matrix event cannot be attached to
a different `delivery_id`, payload commitment, room, or principal.

## Honest retry idempotency

The durable result binding and individual lookup observations are separate:

- `matrix_transport_adapter_result_reconciliations` stores the stable
  delivery/result association, including `request_fingerprint`,
  `result_payload`, and `result_sha256`;
- `matrix_transport_adapter_result_observations` stores each bounded
  append-only observation: lookup response digest, observation time, candidate
  SHA, result digest, and request fingerprint.

A retry with the same delivery fingerprint and exact result is a `replay`, even
when the Adapter's `generated_at`, raw response digest, and operator observation
time change. The retry appends a new observation and does not write a second
`dead_letter -> sent` transition. A changed task, result, payload, principal, or
fingerprint remains a collision.

Observation time is finite and must fall between 15 minutes before and 5 minutes
after the database clock. This bounds delayed or predated evidence without
coupling idempotency to mutable observation metadata.

## PostgreSQL transport and process custody

For a non-loopback database, the operator command requires all of the following:

- a non-empty password;
- `PGSSLMODE=verify-full`;
- an absolute, canonical, regular, single-link, trusted-owner,
  non-group/world-writable CA file whose immediate directory is also under
  trusted non-writable custody, supplied
  through `MATRIX_RECONCILIATION_DATABASE_CA_FILE`;
- `PGSSLROOTCERT` bound to that file;
- required channel binding;
- an absolute, canonical, executable, regular, single-link, trusted-owner,
  non-group/world-writable `psql` path under a trusted non-writable immediate
  directory, supplied through `MATRIX_RECONCILIATION_PSQL`;
- a closed child environment that does not inherit `PGSERVICE`,
  `PGSERVICEFILE`, `PGSYSCONFDIR`, `PGHOSTADDR`, caller `PGOPTIONS`, or generic
  database URLs.

Plain PostgreSQL is accepted only for an explicitly enabled literal loopback IP
test connection through `--allow-insecure-database-loopback`; the hostname
`localhost` is deliberately insufficient. That switch cannot make a remote host
insecure.

Passwords remain in the child environment, not command-line arguments or output.
The command still uses `-X`, `--no-password`, `ON_ERROR_STOP=1`, bounded
timeouts, a fixed search path, and one v2 security-definer function invocation.

## Migration and least privilege

`0005_adapter_result_causal_binding.sql` is additive. It preserves the historical
`0001` through `0004` bytes, backfills deterministic fingerprints for any
existing reconciliation rows, creates the immutable observation table, and
installs `cex_matrix_reconcile_adapter_result_v2`.

Runtime execution of v1 is revoked from `cex_matrix_reconciler_runtime`; only v2
is granted. Direct table access remains unavailable to the runtime role. The v2
function is owned by `cex_matrix_api_owner`, validates exact evidence keys and
types, locks the outbox row, recomputes all hashes and fingerprints, and records
the caller as `session_user`.

## Verification

Source and self-test gate:

```text
python3 scripts/check-matrix-result-reconciliation.py
python3 scripts/check-matrix-result-reconciliation-security-v2.py
python3 scripts/reconcile-matrix-adapter-result.py --self-test
```

Rust qualification:

```text
cargo fmt -p matrix-entry-adapter -p consumer-entry-api -- --check
cargo test --locked -p matrix-entry-adapter -p consumer-entry-api --all-targets
cargo clippy --locked -p matrix-entry-adapter -p consumer-entry-api --all-targets -- -D warnings
```

PostgreSQL qualification, against the disposable PostgreSQL 16 profile:

```text
bash scripts/check-matrix-source-observation-postgres.sh
python3 scripts/matrix_operator_postgres_regression.py
```

The operator runner applies migrations `0001` through `0005` twice and executes
the prior regressions plus
`scripts/test-matrix-result-causal-binding-postgres.sql`. The new regression
proves a first reconciliation, a real retry with changed observation metadata,
exactly one delivery transition, two append-only observations, v1 privilege
revocation, v2 privilege presence, and rejection of changed fingerprint, stale
evidence, and changed task/result binding.

Repository qualification still requires non-empty hosted execution on one exact
source SHA and prospective merge object, retained artifacts, live Ruleset
readback and negative probes, two eligible approvals including independent
security approval, and resolved conversations. Local or repository-authored
evidence cannot replace those authorities.

```text
all_repository_source_gaps_closed=false
all_plan_gaps_closed=false
production_ready=false
production_authorization=not_granted
```
