# Finality verifier provenance reconciliation — round 17

Status: source provenance reconciled by explicit downstream overlay; runtime qualification open
Owner: trnm-integration
Production authorization: `not_granted`

## Resolution selected

Round 14 established that the pinned upstream verifier tree
`3db7fe2b40faa15d9597ea2824b1079ca93c754c` and current vendored tree
`37427509e5913b1c71e35082d5609c69a27a978d` have the same ten paths and differ
in exactly one blob, `src/cometbft.rs`. It also traced that delta to CEX commit
`b0105fa6788784df677e4cb124654420dfd63a3d` and showed that the changed block is
inside the verifier test module.

The repository now adopts the second compliant resolution identified by the
round-14 diagnosis: retain the original source manifest unchanged as upstream
identity, and record the exact downstream overlay separately rather than replacing
an expected hash with current bytes.

Active records:

- upstream origin: `vendor/trnm-chain-vendor-manifest.json`;
- overlay policy: `docs/trnm-chain-downstream-vendor-patch-policy-v1.md`;
- exact overlay ledger: `vendor/trnm-chain-downstream-patches-v1.json`;
- executable checker: `scripts/check-vendor-provenance.py`;
- pure mutation regressions: `scripts/test-vendor-provenance.py`.

## Exact preserved identities

| Identity | Value |
|---|---|
| upstream verifier tree | `3db7fe2b40faa15d9597ea2824b1079ca93c754c` |
| current vendored verifier tree | `37427509e5913b1c71e35082d5609c69a27a978d` |
| upstream `src/cometbft.rs` blob | `51b4ae91185f50b4dd454eb365ea67e42deceddb` |
| current `src/cometbft.rs` blob | `1164e0de867432a7ef9603d00178c109635ec503` |
| upstream file SHA-256 | `a59ea2540efd1e7482e6ba148dc15bb9ff7522f5b3d6592ebe2b38512b7fbf9c` |
| introducing CEX commit | `b0105fa6788784df677e4cb124654420dfd63a3d` |

The upstream digest remains in the origin manifest. The overlay ledger stores the
current Git identity separately and does not call it upstream. This closes the
previous unexplained/policy-contradicting source state without claiming the two
trees are byte-identical.

## Fail-closed verification

The checker walks every file declared for the four Chain vendor crates. Unpatched
files must match the origin SHA-256 exactly, the file set must not grow silently,
links/special/executable entries are rejected, and a patched file must match its
exact current Git blob. A patch record must bind the manifest source tree/digest,
current tree, introducing commit, test-only classification, no runtime behavior
change and a required rebase disposition. Runtime-affecting changes cannot be
approved by the v1 checker.

For patched crates, the checker also asks Git for the exact `HEAD:<crate>` tree;
therefore changing another file while leaving the patched file intact still
invalidates the recorded patched tree. The checker is wired into the aggregate
development-document gate, so provenance drift cannot be ignored by running only
the module documentation checker.

Nine pure fixture tests cover the accepted overlay and mutations to unpatched
bytes, patched bytes, file set, patch target, runtime/test classification, original
digest and current tree. They do not execute Cargo or contact the upstream repo.

## What remains open

This reconciliation closes the **repository source-provenance policy mismatch** for
the finality-verifier. It does not prove that the verifier compiles, passes Clippy,
passes golden vectors, or has been independently reviewed on the final candidate.
Those gates still require the actual Rust toolchain and hosted exact-head evidence.

`trnm-economy-protocol` remains outside the four-crate Chain origin manifest and
still needs its own source-origin record. Production authorization remains
`not_granted`.
