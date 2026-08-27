# Runtime Guard Implementation Status — 2026-08-27

## Implemented in this branch

The first P0 slice is intentionally source-shared from:

`crates/shared-config/src/runtime_guard.rs`

It is included by the gateway, identity, ledger, execution, and audit binaries without changing workspace dependency metadata or `Cargo.lock`. This keeps the first baseline patch reviewable while the repository still lacks a green canonical integration branch.

The module provides:

- explicit runtime profile resolution;
- conflict detection between `CEX_RUNTIME_PROFILE` and `APP_ENV`;
- production-like fail-fast checks;
- weak credential and placeholder rejection;
- PostgreSQL connect/query preflight before bind/listen;
- production identity deny-only static sink as a transitional fail-closed measure;
- pure unit tests compiled with the guarded binaries.

## Deliberate limitations

- Capability service is not yet wired because its current package does not depend on PostgreSQL/UUID and the first patch avoids lockfile churn before CI is green.
- The source-shared module is not the final packaging shape.
- Identity still contains the legacy static fallback implementation; the random deny-only sink prevents the public development key from becoming authoritative in production-like profiles, but the fallback code must still be removed.
- Workload identity and authenticated audit writes are the next implementation slice.

## Promotion criteria

Promote the module into a normal workspace crate or exported `shared-config` module after:

1. hosted CI reaches real steps and is green;
2. the lockfile can be regenerated and verified with `--locked`;
3. capability-service startup semantics are defined;
4. runtime profile metrics/readiness contract is finalized.
