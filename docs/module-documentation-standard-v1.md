# Workspace module documentation standard v1

Status: active normative documentation contract  
Production authorization: `not_granted`

## Purpose

Every active Cargo workspace member must have one machine-readable catalog entry and one colocated `MODULE.md`. The contract prevents a repository from claiming documentation completeness merely because a fixed set of central documents exists.

## Required module document

The canonical file is `<workspace-member>/MODULE.md`. A module may also keep a README, OpenAPI specification, runbook or ADR, but those do not replace the module contract.

Each `MODULE.md` must contain:

1. status, exact module path, lifecycle and production-authorization marker;
2. `Scope`;
3. `Non-goals`;
4. `Authority and state ownership`;
5. `Interfaces`;
6. `Data and persistence`;
7. `Security and configuration`;
8. `Failure and recovery`;
9. `Observability`;
10. `Verification`;
11. `Compatibility and retirement`.

The document must distinguish repository qualification from production approval and must not contain machine-specific absolute paths.

## Machine-readable catalog

`docs/module-catalog-v1.json` is authoritative for membership and ownership. Its module paths must be exactly equal to `[workspace].members` in the root `Cargo.toml`: no missing member, stale member or duplicate path is allowed.

Each catalog entry records the stable module ID, repository path, kind, lifecycle, bounded-context owner and canonical `MODULE.md`. Detailed interfaces, persistence, security, recovery, observability and compatibility requirements live in the colocated module contract so they remain reviewable next to the implementation.

## Change rules

A workspace addition, removal, move or ownership transfer must update the Cargo workspace, module catalog and module document in the same commit. A new interface, persistent state, security boundary or recovery mode must update the owning module document and any affected protocol, ADR or runbook.

Quarantined source is not an active workspace member. It must use a quarantine marker and cannot be referenced as current authority.

## Verification

Run:

```bash
python3 scripts/check-module-documentation.py
python3 scripts/check-development-docs.py
```

The authoritative `rust-service-gate` runs the contract on every branch push. A missing or malformed module document fails the candidate before compilation. File presence or word count alone is not sufficient: required semantics and exact workspace/catalog equality are validated.

## Production boundary

This contract can establish documentation completeness for one repository tree. It cannot establish branch protection, credential custody, representative-volume recovery, sustained SLO qualification, independent review, legal approval or final human go/no-go. Those remain independently evidenced gates.
