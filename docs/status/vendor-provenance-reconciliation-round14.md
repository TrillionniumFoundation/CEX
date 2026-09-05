# Finality verifier provenance reconciliation — round 14

Status: diagnosed; byte-for-byte vendor compliance still open
Owner: trnm-integration
Production authorization: `not_granted`

## Exact discrepancy

At CEX commit `f1b09501221ce9885c63447b23b684ff6de8b9ba`, the complete verifier
subtree is `37427509e5913b1c71e35082d5609c69a27a978d`. The unchanged
`vendor/trnm-chain-vendor-manifest.json` records the origin subtree
`3db7fe2b40faa15d9597ea2824b1079ca93c754c`. Both Git tree API inventories were
retrieved recursively with `truncated=false`. They have the same ten blob paths
and file modes. Only `src/cometbft.rs` differs:

| Fact | Recorded tree | Current tree |
|---|---|---|
| Git blob | `51b4ae91185f50b4dd454eb365ea67e42deceddb` | `1164e0de867432a7ef9603d00178c109635ec503` |
| Bytes | 150,979 | 150,965 |

The other nine files, including Cargo.toml, the library entry, CLI and fixtures,
have identical Git blob identities. This is a comparison of Git objects, not an
execution of the verifier or an independent audit of the upstream repository.

## Introducing change

The path-specific CEX history identifies commit
`b0105fa6788784df677e4cb124654420dfd63a3d`, parent
`5d5d189eb135ac7d9f34927f43682c7ad5ae61fc`, dated 2026-09-01 09:28:33 UTC.
Its title is `fix: close seq42 manifest order and Rust 1.98 lint gaps`.
The commit's file patch changes one block under `mod tests` near line 2352:

```rust
// Before
for signature in &mut signed.commit.signatures {
    *signature = block::CommitSig::BlockIdFlagAbsent;
}
// After
signed
    .commit
    .signatures
    .fill(block::CommitSig::BlockIdFlagAbsent);
```

The commit API identifies the resulting blob as the current blob above.
The deleted one-shot workflow in the same commit contains that exact replacement
and a Clippy command. These records explain the textual divergence. A workflow
command or commit message is not evidence that the command completed. No old
workflow is restored, and its source-writing pattern is not reintroduced.

## Why diagnosis is not closure

The original vendor record requires byte-for-byte copying from a pinned Chain
commit and allows only inherited package-author metadata to differ. It does not
provide an exception for changes confined to tests. Therefore the present vendor
copy still fails that stated identity policy even though the delta is localized.
Neither the expected subtree nor the expected file hash is replaced here.

A compliant resolution requires a reviewed source refresh from an appropriate
upstream commit with the required lock/golden-vector work, or an explicitly
accepted downstream-patch policy that preserves original and patched identities.
This diagnostic does not adopt either policy or fabricate its review. Full
compilation, Clippy and verifier/golden tests remain required. The separate
`trnm-economy-protocol` origin is not covered by the four-crate Chain record.

## Reproduction sources

Use the authenticated repository connection for these immutable resources:

```text
https://api.github.com/repos/TrillionniumFoundation/CEX/git/trees/37427509e5913b1c71e35082d5609c69a27a978d?recursive=1
https://api.github.com/repos/TrillionniumFoundation/CEX/git/trees/3db7fe2b40faa15d9597ea2824b1079ca93c754c?recursive=1
https://api.github.com/repos/TrillionniumFoundation/CEX/commits/b0105fa6788784df677e4cb124654420dfd63a3d?per_page=1&page=6
https://api.github.com/repos/TrillionniumFoundation/CEX/contents/vendor/trnm-chain-vendor-manifest.json?ref=f1b09501221ce9885c63447b23b684ff6de8b9ba
```

The paged commit response must still identify the exact filename and blob above;
page position alone is not identity. The current-round patch changes no vendor
source or origin-manifest bytes and confers no production approval.
