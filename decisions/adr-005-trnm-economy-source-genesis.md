# ADR-005: TRNM economy vendored source genesis

Status: accepted for repository source provenance
Date: 2026-09-05
Production authorization: `not_granted`

## Context

`trnm-economy-protocol` entered CEX at commit
`ce681c60d7e09a25f468815a6e7ae6081e8e1a12` from the historical relative path
`../Trillionnium/trillionnium/crates/trnm-economy-protocol`. That commit did not
record an immutable external repository URL, commit or source tree. Git history
therefore cannot truthfully reconstruct the pre-import repository identity.

The imported bytes themselves are immutable and available in CEX. The package
then changed only in three later path commits (`836bddf...`, `ba399db...`,
`81b49fc...`); the final tree from `81b49fc...` equals the current tree.

## Decision

For **CEX repository provenance only**, the exact package bytes at
`ce681c60d7e09a25f468815a6e7ae6081e8e1a12` are the source genesis of the CEX
vendored lineage. The repository does not claim that this genesis is the first
historical implementation or that its unrecorded external predecessor has been
identified.

The genesis tree is `93dcbc7d7dc9620b9558895f0639a63e67cc0919`.
Its Cargo.toml and src/lib.rs blobs are respectively
`a5128176ed31661c7044a54bb8e8f6257287cb4b` and
`92e602b3601fb3935b63ae99cbad1351ac228dc6`.

The subsequent CEX lineage is recorded exactly in
`vendor/trnm-economy-vendor-origin-v1.json` (schema v2). The last lineage tree is
`4bf9e5de88cbe54d90802d18cbdfa8a21f19ded3`, identical to the current vendored
package tree.

## Consequences

This closes the repository's missing immutable-origin record without fabricating
an external commit. Historical external prehistory remains explicitly unknown and
non-authoritative. Future external refreshes must record repository/commit/tree
before import; future CEX-native changes must extend the explicit lineage and pass
compatibility/locked qualification.

This decision does not prove protocol correctness, signature security, Cargo
compilation, external TRNM approval, or production readiness. Those remain
separate gates.
