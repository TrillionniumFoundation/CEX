# Rust 1.99.0 build-toolchain contract

Status: candidate, pending exact-head hosted execution

Production authorization: `not_granted`

The active compiler is Rust `1.99.0`, release commit
`b940084d7eb6a299eb4bfeb8e34901bc051e7ac4`. Root rustup selection, CI,
container compilation, compiler-identity assertions, active documentation and
admission checkers must agree. Cargo's version is recorded from execution rather
than assumed to match rustc's point version.

## Upstream identity

- Release: https://blog.rust-lang.org/2026/10/01/Rust-1.99.0/
- Compiler: https://github.com/rust-lang/rust/commit/b940084d7eb6a299eb4bfeb8e34901bc051e7ac4
- Official manifest: https://static.rust-lang.org/dist/channel-rust-1.99.0.toml
- Manifest SHA-256: `ce6dddc886364f8d786514771212cebe9b731ba82d6b859951c6b0ccc516b6a2`
- CI installer: https://github.com/dtolnay/rust-toolchain/blob/7e38f4b43b4db5c8dd498af069a4f6196df1d067/action.yml

The installer accepts the explicit `toolchain: 1.99.0` input. A version-specific
Action commit can ignore that input and must not silently select an older compiler.
The official Docker image catalog publishes `rust:1.99.0-bookworm` for the isolated
Matrix test builders. The three production-image build recipes retain their
immutable bootstrap image digest; they explicitly install and select 1.99.0 and
verify its full release commit before compiling candidate code. The bootstrap
image's original compiler is not the candidate compiler.

## Preservation and evidence

Historical audit records, the Sequence 54 security donor's compiler identity,
external evidence locks, vendored protocol sources and intentional MSRV contracts
remain unchanged. RustSec exception scopes, dependency versions, expiry dates,
independent approval requirements, production boundaries and runtime behavior are
not relaxed by this upgrade. Existing runtime SBOMs and image evidence must be
regenerated and independently verified against newly built artifacts before
release; changing this contract does not qualify old artifacts.

Source checks and synthetic probe tests are necessary but do not establish Rust
1.99.0 compilation, formatting, Clippy, PostgreSQL or container success. Run the
existing exact-head/prospective-merge hosted gates, including Linux and Windows
service gates, Sequence 54 integration, Rust toolchain convergence, and applicable
container checks. A strict lint or rustfmt difference must be reviewed on the new
compiler. Qualification jobs must not repair source, refresh Cargo.lock, publish replacement
commits or substitute earlier-SHA evidence. Dedicated non-qualifying rustfmt
proposal jobs may produce review artifacts in disposable checkouts; their output
is not qualification evidence and must be reviewed and committed before retesting.

The external World/CEX workflow preserves its immutable component pins and
requires an unchanged committed Cargo.lock in each component. Its currently pinned
CEX owner lacks that lock, so qualification deliberately fails closed until a
separately reviewed component-lock update supplies a suitable immutable baseline.
It must not generate a lock and present it as original source. Compiler identity
is verified inside both nested build directories and recorded from execution.

Repository qualification and production authorization remain separate. This
upgrade does not authorize deployment, live trading, account mutation, permission
changes or a merge.
