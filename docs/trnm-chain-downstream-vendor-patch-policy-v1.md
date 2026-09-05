# TRNM Chain downstream vendor patch policy v1

Status: active source-provenance policy
Owner: trnm-integration
Production authorization: `not_granted`

## Purpose

`vendor/trnm-chain-vendor-manifest.json` remains the immutable record of the
reviewed upstream Chain commit, source trees and source SHA-256 values. This
policy defines the only permitted exception to byte-for-byte vendoring: a
narrow downstream patch whose original and current identities are both retained
in `vendor/trnm-chain-downstream-patches-v1.json`.

This policy does not rewrite upstream identity, assert that downstream bytes are
upstream bytes, or authorize production. It exists so a necessary local patch is
visible and machine-verifiable instead of being hidden by replacing expected
hashes.

## Allowed patch class

A downstream patch is permitted by this v1 policy only when all of the following
hold:

1. the affected manifest file already exists in the pinned upstream record;
2. the ledger records the exact upstream crate tree, upstream file Git blob and
   upstream SHA-256 already present in the source manifest;
3. the ledger records the exact current vendored crate tree and current Git blob;
4. the introducing CEX commit is immutable and identified exactly;
5. `scope=test_only`, `runtime_behavior_changed=false`, and the production code,
   fixtures and package identity remain unchanged;
6. a rebase disposition says when the overlay must be removed or replaced;
7. every unpatched vendor file still matches the manifest SHA-256 and the complete
   file set contains no unlisted source;
8. `scripts/check-vendor-provenance.py` passes on the exact checkout.

Runtime-affecting, build-script, dependency, protocol, fixture or package metadata
source changes are not approvable by this policy. They require a reviewed upstream
refresh or a new explicit policy version with the corresponding compatibility and
qualification work.

## Current overlay

The sole v1 entry is the finality-verifier `src/cometbft.rs` test-only change
introduced by CEX commit `b0105fa6788784df677e4cb124654420dfd63a3d`.
The pinned upstream blob remains
`51b4ae91185f50b4dd454eb365ea67e42deceddb`; the vendored blob is
`1164e0de867432a7ef9603d00178c109635ec503`. The upstream crate tree remains
`3db7fe2b40faa15d9597ea2824b1079ca93c754c`; the exact patched crate tree is
`37427509e5913b1c71e35082d5609c69a27a978d`.

The textual delta is the already diagnosed replacement, inside the test module,
of manual slice assignment with `slice::fill`. The source manifest's upstream
SHA-256 stays unchanged. This policy therefore reconciles the source provenance
as `pinned upstream + explicit downstream overlay`; it does not claim
byte-for-byte equality with the upstream tree.

## Verification and change protocol

Run:

```text
python3 scripts/test-vendor-provenance.py
python3 scripts/check-vendor-provenance.py
python3 scripts/check-development-docs.py
cargo test --locked -p trnm-finality-verifier --all-targets
cargo clippy --locked -p trnm-finality-verifier --all-targets -- -D warnings
```

The first two checks prove repository identity/policy structure only. Cargo tests
and Clippy remain separate runtime/toolchain evidence. Upstream governance and
production approval remain independent.

Any changed vendor byte must either match the pinned manifest or have a new exact
ledger record permitted by the active policy. Deleting the ledger to silence a
mismatch, changing the upstream expected digest to current bytes, broadening a
patch from test-only to runtime code, or marking a failed checker as skipped is a
hard provenance failure.
