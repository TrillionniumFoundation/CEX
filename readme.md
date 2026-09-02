# CEX

This README is navigation only. It is not release evidence and does not override the canonical development-document authority.

CEX is the Rust workspace containing exact Ledger, invocation/execution, Audit, Hepta Research League, Paper Raid edge, TRNM economy, and Matrix integration components used by the Trillionnium platform.

## Start here

- Canonical development authority: [`docs/index.md`](docs/index.md)
- Complete workspace module index: [`docs/modules/index.md`](docs/modules/index.md)
- Machine-readable module catalog: [`docs/module-catalog-v1.json`](docs/module-catalog-v1.json)
- Active development plan: [`docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md`](docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md)
- Active implementation addendum: [`docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md`](docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md)
- Component maturity: [`docs/status/component-status-v1.md`](docs/status/component-status-v1.md)
- Clean deployment acceptance: [`docs/clean-deployment-acceptance-v1.md`](docs/clean-deployment-acceptance-v1.md)
- External production evidence intake contract: [`docs/external-production-evidence-contract-v1.md`](docs/external-production-evidence-contract-v1.md)
- Shape-only external evidence template: [`docs/templates/cex-external-production-evidence-bundle-v1.json`](docs/templates/cex-external-production-evidence-bundle-v1.json)

## Development entry points

```bash
python3 scripts/check-module-documentation.py
python3 scripts/check-external-production-evidence-contract.py --contract-only
python3 scripts/check-development-docs.py
python3 scripts/check-repository-integrity.py
cargo fmt --all --check
cargo test --workspace --all-targets
```

Every Cargo workspace member must have exactly one entry in the module catalog and one dedicated technical contract. Adding, removing, or renaming a member without updating both is a failing documentation gate.

The external evidence template is deliberately empty. Real V12-X1 through V12-X8 evidence must remain in approved external custody and bind one exact repository-qualified candidate. Structural validation cannot establish issuer independence or grant authorization.

## Authority boundary

The repository can qualify one exact commit/tree only through real, non-empty hosted workflow execution and a generated candidate manifest. Source presence, this README, templates, local-only output, zero-step workflow failures, administrator assertions, or a result from another SHA are not qualification evidence.

**Production authorization is not granted.** Representative-volume recovery, real cutover/rollback, real provider reconciliation, credential custody, sustained production-like soak, independent security/operations/financial review, legal/provider approvals, and the final human go/no-go remain external evidence gates.
