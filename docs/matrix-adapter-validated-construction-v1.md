# Matrix adapter validated construction boundary v1

Status: active source/API contract  
Owner: `matrix-integration`  
Production authorization: `not_granted`

## Decision

The deployable `matrix-entry-adapter` package keeps one public facade at
`services/matrix-entry-adapter/src/lib.rs`. The former crate-root implementation
is retained byte-for-byte at
`services/matrix-entry-adapter/src/implementation.rs`, Git blob
`2897b042eb60671bf6631857ea45840807360037`, and is loaded as a private module.
The module is not re-exported. Its legacy `MatrixAdapterConfig::from_env`,
`AppState::new` and `AppState::from_env` methods therefore remain available to
its own regression tests and the facade, but are no longer nameable by an
external crate through the `matrix-entry-adapter` API.

This is a source/API boundary only. It does not claim real Rust compilation,
black-box startup, PostgreSQL, homeserver or production qualification.

## Public construction protocol

A caller must first invoke synchronous `validate_process_environment`. It reads
all three declared profile sources—`MATRIX_ENTRY_RUNTIME_PROFILE`,
`CEX_RUNTIME_PROFILE` and `APP_ENV`—and delegates parsing to the shared-config
`resolve_profiles` implementation. Explicit invalid, empty, non-Unicode or
conflicting values fail closed. A successful result normalizes the legacy
adapter-specific value and returns `ValidatedMatrixAdapterEnvironment`.

`ValidatedMatrixAdapterEnvironment` has a private field, no public constructor,
no `Default`, `Clone` or `Copy`, and is consumed by
`AppState::from_validated_env`. The facade does not expose a public `from_env`,
`new`, raw `MatrixAdapterConfig`, or the private implementation module. The
binary obtains the token before building Tokio, passes it through `run`, then
constructs the state. It captures the validated bind address before consuming
state in `build_router`.

## Preserved implementation and tests

The implementation file is the exact prior `src/lib.rs` blob. The facade crate
root carries the active recursion limit. The private module locally suppresses
only the expected `unused_attributes` warning for the now-non-root copy and
`dead_code` warnings from intentionally hidden legacy public items. The original
unit tests remain attached to that same implementation source and continue to
run under `cargo test --locked -p matrix-entry-adapter --all-targets`.

`scripts/check-matrix-adapter-api-boundary.py` verifies the exact implementation
blob, private module linkage, non-inventible token, strict source set,
pre-runtime ordering, absence of legacy public constructors, workflow wiring
and documentation. `scripts/test-matrix-adapter-api-boundary.py` mutates each
of those boundaries and requires fail-closed rejection. The checker is also
part of `scripts/check-development-docs.py`.

## Verification

Required source and hosted commands:

```text
python3 scripts/test-matrix-adapter-api-boundary.py
python3 scripts/check-matrix-adapter-api-boundary.py
cargo fmt -p matrix-entry-adapter -- --check
cargo test --locked -p matrix-entry-adapter --all-targets
cargo clippy --locked -p matrix-entry-adapter --all-targets -- -D warnings
```

Only the first two are repository source checks. The Cargo commands must run on
the exact committed head with Rust 1.98.1. A black-box startup test must also
show invalid/conflicting profiles fail before a listener appears and accepted
profiles reach the validated constructor. Queued jobs, source inspection and
this decision record are not passes.

## Compatibility and rollback

HTTP routes and the internal state implementation are unchanged by this split.
The intentionally removed compatibility surface is direct external access to
legacy construction/configuration types. Embedded callers must adopt the token
protocol instead of bypassing startup validation. Rollback must not restore the
legacy public constructors without an explicit security review and equivalent
strict typed construction boundary.
