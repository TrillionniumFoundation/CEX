# Build blocker repairs — rounds 6 and 7 integration

Base commit: `cc0c7b27085ad3e7960fae34241075555eebd176`  
Base complete Cargo.lock blob: `75e9ae18ece0a3e58296b75de5b33b623ff3a7c8`  
State: integrated source candidate; not a candidate freeze or a successful build  
Production authorization: `not_granted`

## Two Matrix lock entries

The base lock's matrix-bot-poller and matrix-bot-relay entries omit the manifests'
`anyhow`, `sha2` and `sqlx` direct dependencies. Relay also retains `chrono`, which
its manifest no longer declares. The patch adds `anyhow`, `sha2 0.10.9`, `sqlx`
to these two arrays and removes relay's stale `chrono` edge. It does not change
any registry version, checksum, source, package set or other workspace entry.

The complete 86,749-byte original lock was acquired and matched to its Git blob.
Applying the two dependency-array changes yields blob
`2e1b7b25d89591a5615bee066015fce78483c403`. All other bytes remain unchanged.
The direct-edge preflight passed against the complete repaired lock and the two
actual package manifests; Cargo resolution itself has not executed.
`check-matrix-lock-coherence.py` compares direct dependency names, including dev
and build declarations; it does NOT resolve semver, inherited aliases, transitive
versions, enabled features or platform selection. It is a preflight for these two
current packages, not a replacement for Cargo. The patch is not deemed qualified
until complete-workspace `cargo metadata --locked`, locked build, tests and Clippy
actually pass. Do not regenerate the lock inside qualification CI.

The `scripts/fixtures/matrix-lock/` before and after files are explicitly partial
TOML test fragments. They are never installed or used as the workspace Cargo.lock.
Only the two contextual diff hunks modify the complete lock in an exact-base
checkout. The distribution applicator verifies the complete original lock blob
before applying those hunks.

## Corrected compiler release

Rust 1.98.1 was released on 2026-09-03 to fix a Rust 1.98.0 vtable miscompilation.
The root rust-toolchain.toml and affected Rust/Matrix/TRNM install steps select
1.98.1, with rustfmt and Clippy. Previous build artifacts are not reclassified.

The observed official-image manifest still listed rust:1.98.0-bookworm, not a
verified 1.98.1 tag. Keep that published image only as bootstrap: explicitly
install/select 1.98.1 and uninstall 1.98.0 before any candidate code is compiled.
The final image's RUST_VERSION is corrected too. A failed installation fails the
image build. Neither an image pull nor compilation was executed locally here.
This patch does not assert that every other repository workflow has been audited
for independent compiler overrides; those remain part of full-tree qualification.

The isolated `versions` command now rejects a missing/wrong rustc and uses
fail-fast shell execution. A successful later psql command must not conceal the
compiler failure. Tests execute that actual shell command using explicit fake
version executables; they do not represent an installed Rust compiler.

## Verification

```text
python3 scripts/test-build-unblock.py
python3 scripts/check-matrix-lock-coherence.py
cargo metadata --locked --no-deps --format-version 1
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets --no-fail-fast
cargo clippy --locked --workspace --all-targets -- -D warnings
```

Retain all prior database/homeserver and exact-head authority gates. The added
preflight never changes source or converts a missing lock or client into success.
Local source-test results include source tests, synthetic Git/ELF fixtures and fake
PostgreSQL orchestration. Full Cargo resolution, actual Rust compilation, database
behavior, complete semantic inventory, adapter result reconciliation, large-gap
recovery, privileges, operational qualification and independent approvals remain
open. No full source snapshot, binary, hosted pass or production approval was made.

## Sources checked during this repair

- Rust release team: https://blog.rust-lang.org/2026/09/03/Rust-1.98.1/
- Cargo fetch contract: https://doc.rust-lang.org/cargo/commands/cargo-fetch.html
- Official Rust image manifest: https://github.com/docker-library/official-images/blob/master/library/rust
- CEX lock and manifests: base commit specified above, using the connected repository.

## Authoritative workflow coverage

The existing `rust-service-gate` / `hepta-postgres-integration` job now also runs
complete Matrix package formatting, all-target tests, strict Clippy and the full
current-schema database regression. Matrix uses a separate disposable
`matrix_review_ci` database created in the CI-only PostgreSQL service, never the
Hepta database. Every new command is fail-fast. The generated SQL report has an
exact-SHA/attempt artifact name; it remains supplementary to the existing final
candidate manifest. The original complete Hepta suite and upload remain required.

The existing `p0-execution-settlement-gate` retains its full package tests,
workspace compile and all original SQL lifecycles. It additionally runs complete
lifecycle source checks, all-target Clippy with denied warnings, and historical
provider reconciliation regression. These historical tests do not reactivate a
local provider runtime. Both extended jobs select Rust 1.98.1. No new workflow
identity, source-writing step, waiver, self-approval or release schema is added.

The accompanying workflow tests inspect these exact steps and execute their
Matrix shell sequence with explicit fake tools to verify failure propagation.
They are not Rust, PostgreSQL or hosted execution evidence. Complete source
inventory, runtime gaps, independent review and production gates remain open.
