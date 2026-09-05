# Shared Matrix runtime profile implementation v1

Status: source implemented; locked Rust and black-box startup acceptance pending
Owner: platform-foundations / matrix-integration
Parent: v12 implementation addendum, Blocks H and K
Production authorization: `not_granted`

## Implementation and API

The adapter, poller and relay previously compiled independent copies of the same
strict parser. They now depend on the existing local shared-config package. Each
existing `src/runtime_profile.rs` is only a re-export of
`shared_config::runtime_guard::matrix_profile::resolve_profiles`. It cannot
contain a separate parser or environment mutation. The canonical source is
`crates/shared-config/src/runtime_guard/matrix_profile.rs`, exposed by the native
`pub mod matrix_profile;` declaration in `runtime_guard.rs`.

The existing reviewed `runtime_guard_impl.rs` is unchanged. Matrix's four-way
profile policy is a separate namespace, not a replacement for the existing
service RuntimeProfile enum. No new crate, workspace member, external dependency,
feature, permission, listener or database migration is introduced.

The public pure API is:

```rust
AdapterProfile::parse(raw: &str) -> Result<AdapterProfile, &'static str>
AdapterProfile::legacy_value(self) -> &'static str
resolve_profiles(values: &[Option<String>]) -> Result<AdapterProfile, &'static str>
```

It does not read or mutate process configuration. Successful parsing proves only
a selected policy, not credential strength, schema readiness or production approval.
The implementation portion was moved verbatim from the previous copies; the
existing six unit-test cases were also retained once in the shared crate.

## Behavior preserved

| Selection | Aliases | Legacy value |
|---|---|---|
| Local | test, local, local_dev, dev, development | local_dev |
| Beta | beta | beta |
| Staging | stage, staging | production |
| Production | prod, production, trnm-economy, trnm_economy | production |

ASCII case and surrounding whitespace normalize. Explicit empty, unknown or
malformed values fail as `invalid_matrix_runtime_profile`. All explicit sources
must select the same enum variant, or the result is
`conflicting_matrix_runtime_profiles`. Staging and Production do not become
equivalent merely because both legacy strings equal production. Only absent
sources permit the local default. Beta retains a nonlocal policy.

The binaries still collect their process-specific profile, CEX_RUNTIME_PROFILE
and APP_ENV and reject non-Unicode values before calling the resolver. No
configuration precedence or call-site ordering changed. Adapter main still
normalizes its legacy environment value before Tokio is constructed; the shared
parser itself has no environment side effect.

## What this does not close

The adapter library's legacy AppState::from_env path has not been rewritten to
require a typed selected profile. A caller embedding that library can still
bypass the binary's pre-runtime preparation. Enforcing typed configuration at
that public boundary remains open, as do real black-box startup tests. Calling
the pure API is not proof an embedding caller supplied every explicit source.

The poller and relay's existing startup/runtime construction is unchanged. This
refactor does not claim to validate every environment variable, modify secret
custody, reconcile ambiguous adapter effects, or fix Matrix recovery behavior.

## Dependency and test ownership

The three Matrix manifest changes add the existing local shared-config path.
The committed lock changes only those three direct dependency arrays. All
package versions, registry checksums, package count and other dependency arrays
remain unchanged. The graph must still be resolved by real Cargo --locked;
manual array comparison is not dependency/feature resolution.

Dependency unit tests do not execute just because a Matrix package is tested.
The existing adapter-rust and authoritative hepta-postgres-integration jobs now
run the full shared-config suite, formatting and strict all-target lint as a
separate step, retaining every prior Matrix/Hepta command:

```text
cargo fmt -p shared-config -- --check
cargo test --locked -p shared-config --all-targets
cargo clippy --locked -p shared-config --all-targets -- -D warnings
```

The Matrix PR path filter includes shared-config changes. The original six Rust
semantic tests now have one owner; four new Rust cases cover alias pairs, invalid
source position, stage/prod distinction and unchanged inputs. They are committed
but have not executed in the current authoring environment.

`python3 scripts/test-matrix-profile-consolidation.py` checks source linkage,
selected mutation negatives, the three local lock edges and workflow placement.
The existing recovery source check now resolves the canonical module and rejects
parser copies. These are source/filesystem/TOML checks, not execution of Rust.
No Python reimplementation is treated as a test of the Rust decision logic.

## Upgrade, rollback and qualification

No deployment configuration or data conversion is required by this source
refactor. Package the shared library with the dependent binaries; do not copy
loose profile sources into a deployed installation. Rollback uses a previously
qualified compatible build without changing profile sources, ignoring conflicts,
or dropping any durable transport state. Behavior equivalence still requires
real compilation, full unit tests, strict lint and black-box startup acceptance
on the final exact candidate. Independent release gates remain unchanged.
