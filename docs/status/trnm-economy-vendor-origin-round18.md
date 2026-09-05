# TRNM economy vendor origin trace — round 18

Status: current CEX import identity bound; immutable external origin unresolved
Owner: trnm-integration
Production authorization: `not_granted`

The package `vendor/trnm-economy-protocol` is not covered by the four-crate Chain
vendor manifest. Its earliest CEX path history is commit
`ce681c60d7e09a25f468815a6e7ae6081e8e1a12` (2026-07-12), which changed the
workspace/consumer dependency from the relative external path
`../Trillionnium/trillionnium/crates/trnm-economy-protocol` to the local vendor
copy. That commit describes the crate as the same TRNM-owned protocol, but does
not bind an external repository URL, immutable commit, or source tree.

The current package is version 2.4.0 and is bound in
`vendor/trnm-economy-vendor-origin-v1.json` to tree
`4bf9e5de88cbe54d90802d18cbdfa8a21f19ded3`, Cargo.toml blob
`4c4751d3a8a23d3b9c65c4bf11141aab77977ac4`, and src/lib.rs blob
`6d6def11a1cb06540aebe9400701af8d0c91c912`.

`python3 scripts/check-economy-vendor-origin.py --contract-only` verifies that
this unresolved record is honest and still points at the exact current CEX tree.
The default command deliberately fails with `external_origin_unresolved` until a
reviewed immutable external repository, commit and tree are supplied. Eight pure
fixture tests verify drift and false-resolution rejection. These checks do not
contact a missing upstream repository and do not infer one from a path name.

Therefore the historical-import **traceability** gap is closed, but the immutable
external source-origin gap remains an explicit release blocker. It may be closed
only by locating/reviewing the actual source repository and binding its immutable
identity, or by an explicit governance decision that treats the CEX import commit
as the authoritative source genesis with corresponding compatibility review. No
such decision is inferred here.
