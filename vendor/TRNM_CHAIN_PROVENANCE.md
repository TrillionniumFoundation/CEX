# Vendored TRNM protocol crates

The following release-only protocol crates are copied byte-for-byte from a
repository-external fresh clone of the canonical Trillionnium Chain repository,
checked out detached at commit
`4adfbadaa8c35cd3515f20381eb6b80d6885f457` (root tree
`396ae6037b24037aff6983fd30d6baf906fda687`) fetched from branch
`feature/chain-paper-raid-receipt-v2`:

- `trnm-research-protocol` 0.1.0
- `trnm-protocol` 0.1.0
- `trnm-finality-types` 0.1.0
- `trnm-finality-verifier` 0.1.0

They are vendored so the Hepta release image can be built without GitHub
credentials or a sibling Chain worktree. Product code must continue treating
these crates as frozen external protocol dependencies. Updating them requires
an explicit new Chain revision, a byte-for-byte copy from another
repository-external fresh detached checkout, and regeneration of `Cargo.lock`
and all cross-language golden vectors.

`trnm-protocol`, `trnm-finality-types`, and `trnm-finality-verifier` intentionally retain their
upstream `edition.workspace`, `license.workspace`, and `authors.workspace`
declarations so every vendored file remains byte-identical to its recorded Git
tree. In this embedding workspace, edition (2021) and license (MIT) are the
same; Cargo package author metadata resolves to `Qi Team` instead of upstream
`Trillionnium Contributors`. This packaging-only authorship-field difference
does not alter compiled code. The machine-readable manifest freezes and checks
that exception explicitly.
