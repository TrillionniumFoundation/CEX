# Hepta Research League container toolchain authority

Status: active build contract  
Production authorization: `not_granted`

## Authority

The immutable `docker.io/library/rust@sha256:4c2fd73ef19c5ef9d54bee03b06b2839a392604fbfcd578ed948b71b37c1d7fb` image is a bootstrap environment only. Its preinstalled compiler is not the CEX release compiler authority.

The authoritative compiler is exact Rust `1.98.1`, declared by the repository root `rust-toolchain.toml` and `docker/rust-toolchain.manifest`. The Docker build downloads the official `channel-rust-1.98.1.toml` plus its SHA-256 file from `https://static.rust-lang.org`, verifies the manifest checksum, installs the exact toolchain through rustup, and rejects any different `rustc --version` result before dependency resolution or compilation.

## Evidence

The build exports and retains:

- `rustc --version --verbose`;
- `cargo --version --verbose`;
- `rustup show active-toolchain`;
- the verified channel-manifest SHA-256 record;
- the exact Dockerfile, toolchain-intent, source commit and source-tree object identities;
- an artifact-level SHA-256 manifest.

The runtime image includes the executed compiler/Cargo identity records under `/usr/share/doc/hepta-research-league/`. These files describe the build that actually occurred; an intended version string is not sufficient evidence.

## Verification

Run the source-side contract:

```bash
python3 scripts/check-rust-toolchain-convergence.py
```

The authoritative hosted execution is `.github/workflows/p0-rust-toolchain-convergence.yml`. It must complete on the unchanged candidate SHA with non-empty job steps and retained artifacts. A workflow definition, a queued job, a zero-step failure, or a successful run on an earlier SHA receives no qualification credit.

## Change control

Changes to the root toolchain file, this Dockerfile, the Docker toolchain intent, convergence checker, workflow, or traceability record require independent review from the security CODEOWNER. Historical build evidence retains the compiler identity that produced it and is never rewritten to resemble a later build.

Container toolchain convergence establishes build reproducibility for one source object. It does not establish production deployment, credential custody, representative-volume recovery, final legal/financial approval, or human go/no-go.
