# Contributing to CEX

Read `docs/index.md`, the active v12 plan and implementation addendum, accepted
ADR-004, the relevant `docs/modules/` contract, and `SECURITY.md` before changing
source. The current remediation branch is an unqualified working change, not a
release. Participating Agents remain external; CEX does not host model inference.

## Source and authority

Preserve exact-money identities, immutable receipts, tenant boundaries and the
separation of Research, Nakama and Chain/settlement authority. Update a module's
catalog entry, interfaces, state/recovery contract, configuration and tests in the
same change. Do not re-enable a retired Execution `/process` route or legacy
provider worker in a production build. Generated files are reviewed source inputs,
not substitutes for genuine runtime verification.

## CI is read-only with respect to candidate source

Commit the reviewed dependency lock and generated semantic inventory before
qualification. CI may create test databases and output artifacts in isolated
locations; it must not refresh source files, regenerate Cargo.lock, commit code,
or push a replacement candidate. The two old Sequence-53 self-writing workflows
have been removed from this working branch. Their removal does not regenerate the
missing lock/inventory, freeze Sequence 54, or prove that all other plan work is done.

Use `--locked` for Rust dependency operations. Missing or stale inputs are failures,
not invitations for CI to repair its own source. Every source, documentation, test,
workflow or generated-file change creates a new tree and needs fresh evidence.

## Development checks

```text
python3 scripts/test-trnm-build-evidence.py
python3 scripts/test-rust-route-contract.py
python3 scripts/test-semantic-source-integration.py
python3 scripts/test-consumer-route-contract.py
python3 scripts/check-module-documentation.py
python3 scripts/check-development-docs.py
python3 scripts/generate-repository-semantics.py --check
python3 scripts/check-external-agent-runtime-boundary.py
cargo fmt --all -- --check
cargo metadata --locked --no-deps --format-version 1
cargo test --locked --workspace --all-targets --no-fail-fast
cargo clippy --locked --workspace --all-targets -- -D warnings
```

Execute PostgreSQL-backed suites with their required disposable databases. A fake
client, mocked network, skipped test, source-only pass, zero-step job, or a successful
run on an older SHA is not current integration evidence. Do not direct destructive
test/reset scripts at shared or production databases.

## Review and release

Describe actual changes, exact commit/tree, executed commands, failures, migration
and rollback implications in the PR. CODEOWNERS only routes reviews; it cannot
enforce protected-main rules, establish reviewer independence or approve a release.
The final pusher cannot manufacture independent approval. Use the sole shared
candidate trigger only after source integration is ready for a real freeze.

Repository qualification requires the existing authoritative workflows and their
aggregate on one unchanged candidate. Real deployment, recovery, secret custody,
sustained load, external review and the final human decision remain independent
requirements. Keep `production_authorization=not_granted` unless the separately
required evidence and decision actually exist. No repository-owned build packet
can grant production authorization.
