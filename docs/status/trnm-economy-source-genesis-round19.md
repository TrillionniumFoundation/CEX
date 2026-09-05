# TRNM economy source-genesis reconciliation — round 19

Status: CEX repository provenance resolved; historical external prehistory disclosed as unknown
Owner: trnm-integration
Production authorization: `not_granted`

Round 18 established that the historical import did not record an immutable
external repository/commit/tree. Further path-history review found exactly four
CEX commits touching `vendor/trnm-economy-protocol`: the import at `ce681c60...`,
then `836bddf1...`, `ba399db7...`, and `81b49fc2...`. The tree produced by the
last of those commits is exactly the current package tree.

ADR-005 therefore defines the immutable import bytes as the source genesis for
**CEX repository provenance**. It explicitly does not claim those bytes are the
first historical implementation or identify the missing external predecessor.
The v2 origin record binds the genesis plus all three subsequent CEX transitions
and the current terminal tree.

Exact package-tree lineage:

- `ce681c60d7e09a25f468815a6e7ae6081e8e1a12` -> `93dcbc7d7dc9620b9558895f0639a63e67cc0919`
- `836bddf1c1efa6d90aa7893256df2b88103b9d53` -> `91c39dbc4d5ada39d272f32b7cc943d49888bfbf`
- `ba399db7de9f80062414427a5b8207f37d91229a` -> `8101ca1e70a534d11c8161a2e69deec23ccc0ad2`
- `81b49fc27ece5c3a35c9eba6023dc1f65a8b6a1f` -> `4bf9e5de88cbe54d90802d18cbdfa8a21f19ded3`

`python3 scripts/check-economy-vendor-origin.py` verifies every historical tree
and both package blobs directly through Git, then requires HEAD to equal the
terminal lineage identity. The aggregate development-document gate now executes
that checker. Nine pure mutation fixtures reject history/current drift, relabelled
commits/blobs, a missing genesis decision, an external-prehistory overclaim and a
missing future-update policy.

This closes the missing **CEX source-origin/provenance record**. Historical external
prehistory remains intentionally non-authoritative rather than fabricated. Cargo
compilation, protocol compatibility, external TRNM approval and production
qualification are independent and remain subject to their existing gates.
