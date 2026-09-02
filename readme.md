# CEX

This README is navigation only. It is not release evidence and does not override the canonical development-document authority.

CEX is the Rust workspace for the Hepta research control plane, exact off-chain Ledger and settlement boundaries, Audit, Paper Raid edge, and Matrix integrations used by the Trillionnium platform. Accepted ADR-004 fixes the top-level architecture as Hepta, Nakama, and TRNM; participating Agents and model/provider runtimes execute externally.

## Start here

- Canonical development authority: [`docs/index.md`](docs/index.md)
- Accepted external-Agent architecture: [`decisions/adr-004-three-module-external-agent-battle-platform.md`](decisions/adr-004-three-module-external-agent-battle-platform.md)
- Sequence-52 architecture closure: [`docs/architecture/external-agent-runtime-boundary-sequence-52.md`](docs/architecture/external-agent-runtime-boundary-sequence-52.md)
- Sequence-52 machine traceability: [`docs/traceability/sequence-52-architecture-v1.json`](docs/traceability/sequence-52-architecture-v1.json)
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
python3 scripts/check-external-agent-runtime-boundary.py
python3 scripts/check-module-documentation.py
python3 scripts/check-external-production-evidence-contract.py --contract-only
python3 scripts/check-external-production-evidence-contract.py --self-test
python3 scripts/check-development-docs.py
python3 scripts/check-repository-integrity.py
cargo fmt --all --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

Every Cargo workspace member must have exactly one entry in the module catalog and one dedicated technical contract. Adding, removing, or renaming a member, explicit Cargo target, source boundary, or external authority without updating the corresponding machine and human contracts is a failing documentation gate.

## Runtime boundary

The default workspace build does not execute participating Agents or local Ollama/OpenClaw inference. Execution Service owns durable work/evidence and settlement lifecycles; Capability Service publishes validated external Agent declarations. The retired `/v1/executions/:id/process` route is absent from the default router. The retained `legacy-local-provider-dispatch` worker target exists only for historical command/reconciliation compatibility, is excluded from default builds, rejects production-like profiles, and performs no model call.

External Agent declarations enter Capability Service only through `CAPABILITY_EXTERNAL_AGENT_REGISTRY_JSON`. Agent identity, keys, signed work/results, and scientific authority remain governed by the Hepta Agent protocol and external custody.

## Qualification boundary

The external evidence template is deliberately empty. Real V12-X1 through V12-X8 evidence must remain in approved external custody and bind one exact repository-qualified candidate. Structural validation cannot establish issuer independence or grant authorization.

The repository can qualify one exact commit/tree only through real, non-empty hosted workflow execution and a generated candidate manifest. Source presence, this README, templates, manual runner-probe definitions, local-only output, zero-step workflow failures, administrator assertions, or a result from another SHA are not qualification evidence.

**Production authorization is not granted.** Runner allocation, protected-main governance, independent exact-head approval, downstream World/Game revision binding, representative-volume recovery, real cutover/rollback, real external Agent/provider reconciliation, credential custody, sustained production-like soak, independent security/operations/financial review, legal/provider approvals, and final human go/no-go remain independent evidence gates.
