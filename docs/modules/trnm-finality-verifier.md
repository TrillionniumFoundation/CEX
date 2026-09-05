# trnm-finality-verifier module contract

Status: active module contract
Workspace member: `vendor/trnm-finality-verifier`
Package: `trnm-finality-verifier`
Kind: `library`
Logical module: `trnm`
Deployable: no resident CEX service
Owner role: `trnm-integration`
Production authorization: `not_granted`

This contract records the inspected source boundary and required integration checks. It is not a successful build or production qualification.

## Purpose and non-goals

The package combines a node-independent verification library with the Unix `trnm-research-receipt-v2` evidence/signing utility. It does not host Chain consensus or participating Agents. A verified proof is evidence relative to explicitly supplied trust, not a grant of credential custody, production authorization or a CEX Ledger effect.

## Authority and owned state

The library returns verification results; the CLI reads and writes evidence files and can perform explicitly selected signing operations. Neither owns a consensus database or a platform-wide accepted-checkpoint policy. The operator/calling service owns trusted validator sets, execution-header anchors, private keys and durable acceptance records. Describing this package as non-service-deployable does not hide or omit its executable Cargo target.

## Source layout and entry points

Catalog-bound sources:

- `vendor/trnm-finality-verifier/src/lib.rs`
- `vendor/trnm-finality-verifier/src/cometbft.rs`
- `vendor/trnm-finality-verifier/src/bin/trnm-research-receipt-v2.rs`

The root Cargo manifest names this vendored package explicitly. Internal source modules are not additional services. The source paths are build/contract inputs, not independent release evidence.

## Interfaces and contracts

`verify_finality_receipt(receipt, validator_set) -> anyhow::Result<()>` checks the supported legacy receipt schema; chain/header, height, roots and validator-set identities; computed block hash; quorum binding and signature verification; transaction leaf/index and Merkle root; paired object reference/proof binding when present; and the computed receipt hash. Any failed condition rejects the receipt. The supplied validator set still requires an external trust decision.

The cometbft module exports the separate AppHash receipt-v2 assembly and trust-anchor verification path. The CLI includes `public-key`, fixture/sign-and-wrap modes, Paper Raid versioned signing/wrapping modes and `assemble-and-verify`. The latter accepts an evidence directory, receipt output and trusted execution-header hash, optionally producing a trust-anchor output. Signing is not verification and must be invoked only with separately authorized keys and inputs.

The current CLI imports Unix filesystem APIs. Its declared input budgets include 256 MiB for RPC JSON and 32 MiB for binary evidence; the configured trusting period and drift constants are 24 hours and 10 seconds. These are source-defined limits requiring operational qualification, not measured throughput or deployment guarantees.

## Persistence, concurrency, and recovery

Library calls do not commit acceptance to a database. The evidence utility operates on files, with regular-file/single-link metadata checks visible at its input boundary. Preserve exact source evidence, trust-anchor commitment, command identity and output receipt when recovering from an interrupted qualification run. File existence alone is not a successful completed verification. Multiple invocations need isolated output locations and externally managed key access; no distributed lock is provided by this module contract.

## Configuration and secrets

CLI arguments distinguish signing input/private-key paths from verification evidence and trusted-header inputs. Do not put real keys, private evidence or unreviewed trust anchors in repository fixtures. The source includes deterministic fixture operations; they cannot be used to manufacture independent evidence. The library itself has no hosted access-token configuration, and native Windows cannot compile its Unix CLI without a reviewed portability change.

## Security and trust boundaries

A receipt cannot bootstrap trust in its own validators/header. Keep legacy receipt verification separate from the CometBFT/AppHash trust-anchor path and bind the verified outcome to the expected command and chain before use.

Source provenance uses two identities rather than pretending the current tree is byte-identical to upstream. `vendor/trnm-chain-vendor-manifest.json` remains the pinned upstream origin record. `vendor/trnm-chain-downstream-patches-v1.json` records the exact current verifier tree/blob for the single test-only Rust 1.98 lint patch, together with the original tree/blob/digest and introducing CEX commit. `docs/trnm-chain-downstream-vendor-patch-policy-v1.md` permits only test-only, runtime-neutral overlays under this v1 mechanism. `scripts/check-vendor-provenance.py` fails on unlisted bytes, extra files, a different patched blob/tree or a broadened runtime patch. The round-17 reconciliation records this policy transition. None of these records replaces Cargo/golden-vector or independent trust qualification.

## Verification

Required Linux qualification commands:

```text
python3 scripts/test-vendor-provenance.py
python3 scripts/check-vendor-provenance.py
cargo test --locked -p trnm-finality-verifier --all-targets
cargo clippy --locked -p trnm-finality-verifier --all-targets -- -D warnings
```

Run against the committed Cargo.lock and complete checkout. Missing toolchain or source is failure, not a skip. In addition, `python3 scripts/check-cargo-workspace-authority.py` must bind actual Cargo membership and discovered targets to this catalog entry. The provenance checker proves the repository source/overlay identity only; it does not execute Rust. The full document, semantic inventory and consumer integration gates remain required. The Rust commands have not been accepted without real exact-head execution evidence.

## Deployment and operations

Not independently deployable as a resident CEX service. Build and qualify the library plus utility on Linux with the complete target suite. The existing Windows gate checks this package's library separately while excluding its Unix-only targets from portable workspace checks; that exception is retained, not extended to Linux. Tool readiness requires the configured trust anchor and evidence/key custody. Rollback preserves accepted receipts and uses a schema-compatible library/CLI pair, never re-signing old effects with new identities.

## Compatibility and change protocol

Current package identity remains 0.1.0. The source manifest remains the immutable upstream origin; the current test-only overlay has a separate immutable ledger identity and mandatory upstream-rebase disposition. Any new vendor divergence that is not exactly allowed by the active patch policy fails closed. A runtime-affecting source change, platform extension, new receipt version or changed filesystem policy needs reviewed source reconciliation and relevant hostile/golden fixtures. Root workspace enumeration is not a protocol upgrade. Full compilation, strict lint, actual CLI execution and external trust/custody review remain separate acceptance conditions.
