# Rust toolchain convergence v1

Status: repository implementation complete; exact-head execution pending  
Production authorization: `not_granted`

## Decision

Every active CEX Rust build, test, lint, release and Hepta container lane must execute the same patched compiler and Cargo release: `1.98.1`. A repository default, workflow action input, Docker builder and generated evidence that disagree are a failed candidate, not multiple supported toolchains.

## Repository authority

- `rust-toolchain.toml` is the checkout-wide version pin.
- `scripts/check-toolchain-convergence.py` rejects stale `1.98.0`, stale container `1.95.0`, floating `stable/latest` and policy/version disagreement on active surfaces.
- `.github/workflows/rust-toolchain-convergence.yml` installs and verifies exact `1.98.1`, records verbose compiler/Cargo identities, validates locked metadata and formatting, and uploads exact-source evidence.
- `services/hepta-research-league/Dockerfile` bootstraps exact `1.98.1` inside the digest-pinned builder, verifies the executed version and exports an executed toolchain manifest.

## Required evidence

A source commit does not close the blocker. Closure requires one unchanged exact head on which:

1. every applicable hosted check has at least one executed step and terminates successfully;
2. `rustc --version --verbose` and `cargo --version --verbose` identify `1.98.1`;
3. the Hepta image build exports matching requested and executed manifests;
4. the binary, SBOM, provenance and checksums are regenerated from that image build;
5. an eligible independent security reviewer approves the unchanged final head.

A queued, skipped, cancelled, zero-step or previous-head result provides no credit.

## Failure policy

Any active mixed compiler identity, unavailable requested toolchain, failed metadata resolution, stale generated artifact, absent executable evidence or self-authored approval leaves the blocker open. No workflow or document may convert this state into production authorization.

## Verification

```bash
python3 scripts/check-toolchain-convergence.py
rustc --version --verbose
cargo --version --verbose
cargo metadata --locked --format-version 1
cargo fmt --all -- --check
```

The machine-readable status is `docs/traceability/v12-toolchain-convergence-v1.json`; operational tracking remains Issue #43 and PR #44.
