# Vendored TRNM protocol crates

The following release-only protocol crates are copied byte-for-byte from
Cargo's immutable Git checkout for Trillionnium Chain commit
`f2e3da051effabc97c2d0e7c47acd1df3d0dd4aa`:

- `trnm-research-protocol` 0.1.0
- `trnm-protocol` 0.1.0
- `trnm-finality-types` 0.1.0
- `trnm-finality-verifier` 0.1.0

They are vendored so the Hepta release image can be built without GitHub
credentials or a sibling Chain worktree. Product code must continue treating
these crates as frozen external protocol dependencies. Updating them requires
an explicit new Chain revision, a byte-for-byte copy from Cargo's checkout,
and regeneration of `Cargo.lock` and all cross-language golden vectors.

`trnm-protocol`, `trnm-finality-types`, and `trnm-finality-verifier` intentionally retain their
upstream `edition.workspace`, `license.workspace`, and `authors.workspace`
declarations so every vendored file remains byte-identical to its recorded Git
tree. In this embedding workspace, edition (2021) and license (MIT) are the
same; Cargo package author metadata resolves to `Qi Team` instead of upstream
`Trillionnium Contributors`. This packaging-only authorship-field difference
does not alter compiled code. The machine-readable manifest freezes and checks
that exception explicitly.
